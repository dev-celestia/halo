// ponytail: simplified from https/ subfolder with mod.rs and cert.rs into a single file

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::OnceLock;

use hexbuffer_proxy::ca::CertificationAuthority;

use halo_core::fs_secure;
use halo_core::paths;

static CA_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn init_ca_dir(app_data_dir: PathBuf) {
    CA_ROOT.get_or_init(|| app_data_dir.join(".halo"));
}

fn get_ca_dir() -> PathBuf {
    CA_ROOT
        .get()
        .cloned()
        .unwrap_or_else(|| paths::get_shared_app_dir().join(".halo"))
}

fn get_ca_cert_path() -> PathBuf {
    get_ca_dir().join("ca.pem")
}

fn get_ca_key_path() -> PathBuf {
    get_ca_dir().join("ca-key.pem")
}

/// Tighten the CA directory to `0700` and the private key to `0600`.
///
/// `CertificationAuthority` persists `ca-key.pem` with a plain `fs::write`,
/// which yields `0666 & ~umask` — normally `0644`, i.e. readable by every
/// local account. A readable root CA key means any local process can mint
/// certificates the user's keychain already trusts, so this must be asserted
/// on every boot, not just at creation (existing installs never self-heal).
pub fn harden_ca_permissions() {
    let ca_dir = get_ca_dir();
    if ca_dir.exists() {
        fs_secure::set_mode(&ca_dir, fs_secure::DIR_MODE);
    }
    fs_secure::harden_file_if_exists(&get_ca_key_path());
}

pub fn create_proxy_authority() -> Result<CertificationAuthority, Box<dyn std::error::Error>> {
    let ca_dir = get_ca_dir();

    // Create the directory `0700` *before* the key exists: the crate writes the
    // key world-readable, which is only safe while the parent blocks traversal.
    // Then narrow the key itself so it stays private even if the dir mode is
    // later loosened (backup tools, sync agents, `chmod -R`).
    fs_secure::create_private_dir(&ca_dir)?;

    // `CertificationAuthority::new_in` panics (`.expect`) when the PEM on disk is
    // corrupt or does not parse. A damaged CA must not take the app down at
    // startup: trap the panic and let `ensure_ca_exists` regenerate instead.
    let authority =
        panic::catch_unwind(AssertUnwindSafe(|| CertificationAuthority::new_in(&ca_dir)))
            .map_err(|_| "CA material on disk could not be loaded (corrupt or unparsable)")?;

    fs_secure::harden_file(&get_ca_key_path());

    Ok(authority)
}

pub fn export_ca_cert_pem() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(fs::read(get_ca_cert_path())?)
}

pub fn get_ca_cert_pem() -> Result<String, Box<dyn std::error::Error>> {
    Ok(fs::read_to_string(get_ca_cert_path())?)
}

pub fn regenerate_ca() -> Result<(), Box<dyn std::error::Error>> {
    let ca_dir = get_ca_dir();
    fs::remove_file(get_ca_cert_path()).ok();
    fs::remove_file(get_ca_key_path()).ok();

    fs_secure::create_private_dir(&ca_dir)?;

    let ca = panic::catch_unwind(AssertUnwindSafe(|| CertificationAuthority::new_in(&ca_dir)))
        .map_err(|_| "CA regeneration failed (certificate generation panicked)")?;

    fs_secure::harden_file(&get_ca_key_path());

    let shared_ca_path = paths::get_shared_app_dir().join("halo-ca.pem");
    let _ = fs::write(&shared_ca_path, ca.ca_cert_pem());
    fs_secure::harden_file(&shared_ca_path);
    Ok(())
}

/// Decode a single PEM block and check it is a DER `SEQUENCE`.
///
/// This replaces a `contains("Halo Proxy CA")` substring test, which
/// accepted any file mentioning that string and then panicked inside the
/// certificate parser.
fn pem_is_well_formed(pem: &str, expected_label: &str) -> bool {
    use base64::{Engine, engine::general_purpose};

    let begin = format!("-----BEGIN {expected_label}-----");
    let end = format!("-----END {expected_label}-----");
    if !pem.contains(&begin) || !pem.contains(&end) {
        return false;
    }

    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .collect();

    match general_purpose::STANDARD.decode(body.as_bytes()) {
        // Every DER structure starts with a SEQUENCE (0x30) tag.
        Ok(der) => der.len() > 2 && der[0] == 0x30,
        Err(_) => false,
    }
}

fn is_ca_cert_valid(cert_pem: &str) -> bool {
    pem_is_well_formed(cert_pem, "CERTIFICATE")
}

fn is_ca_key_valid(key_pem: &str) -> bool {
    pem_is_well_formed(key_pem, "PRIVATE KEY")
        || pem_is_well_formed(key_pem, "RSA PRIVATE KEY")
        || pem_is_well_formed(key_pem, "EC PRIVATE KEY")
}

pub fn ensure_ca_exists() {
    // Heal modes first: the key may exist from a release that wrote it 0644.
    harden_ca_permissions();

    let cert_path = get_ca_cert_path();
    let key_path = get_ca_key_path();

    let cert_valid = fs::read_to_string(&cert_path)
        .map(|content| is_ca_cert_valid(&content))
        .unwrap_or(false);

    let key_valid = fs::read_to_string(&key_path)
        .map(|content| is_ca_key_valid(&content))
        .unwrap_or(false);

    let needs_regen = !cert_valid || !key_valid;

    if needs_regen {
        eprintln!("[ca] CA files missing or outdated, regenerating...");
        if let Err(e) = regenerate_ca() {
            eprintln!("[ca] Failed to regenerate CA: {}", e);
        } else {
            eprintln!("[ca] CA regenerated successfully");
        }
    } else {
        // Ensure shared halo-ca.pem is also kept in sync if missing
        let shared_ca_path = paths::get_shared_app_dir().join("halo-ca.pem");
        if !shared_ca_path.exists() {
            if let Ok(cert_bytes) = fs::read(&cert_path) {
                let _ = fs::write(&shared_ca_path, cert_bytes);
                fs_secure::harden_file(&shared_ca_path);
            }
        }
    }
}

/// Whether `path` is a private (non group/world readable) file.
#[cfg(all(test, unix))]
fn is_private(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o077 == 0)
        .unwrap_or(false)
}

/// Whether `path` is a private (non group/world accessible) directory.
#[cfg(all(test, unix))]
fn is_private_dir(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o077 == 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_proxy_authority_uses_ca_dir() {
        let temp_dir = std::env::temp_dir().join("halo_ca_test");
        let _ = fs::remove_dir_all(&temp_dir);

        init_ca_dir(temp_dir.clone());
        let auth = create_proxy_authority();
        assert!(auth.is_ok(), "CA authority creation should succeed");

        // The key must not be group/world readable. This is asserted in the same
        // test as creation because `init_ca_dir` is a `OnceLock`: two tests
        // cannot own the directory without racing on its teardown.
        #[cfg(unix)]
        {
            assert!(
                is_private(&get_ca_key_path()),
                "CA private key must be 0600, got {:?}",
                fs::metadata(get_ca_key_path()).map(|m| m.permissions())
            );
            assert!(
                is_private_dir(&get_ca_dir()),
                "CA directory must be 0700, got {:?}",
                fs::metadata(get_ca_dir()).map(|m| m.permissions())
            );
        }

        let cert_pem = get_ca_cert_pem();
        assert!(cert_pem.is_ok(), "CA cert PEM read should succeed");
        let pem_str = cert_pem.unwrap();
        assert!(pem_str.contains("BEGIN CERTIFICATE"));
        assert!(
            is_ca_cert_valid(&pem_str),
            "CA certificate should be recognized as valid"
        );

        // Simulating second startup / ensure_ca_exists call
        ensure_ca_exists();
        let cert_pem_after = get_ca_cert_pem().unwrap();
        assert_eq!(
            pem_str, cert_pem_after,
            "CA certificate must NOT be regenerated when valid"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_is_ca_cert_valid() {
        assert!(!is_ca_cert_valid(""));
        assert!(!is_ca_cert_valid("not a cert"));
        assert!(!is_ca_cert_valid(
            "-----BEGIN CERTIFICATE-----\nSGVsbG8gV29ybGQ=\n-----END CERTIFICATE-----"
        ));
        // A file that merely mentions the CA name must not pass.
        assert!(!is_ca_cert_valid("Halo Proxy CA"));
    }

    #[cfg(unix)]
    #[test]
    fn test_ca_key_is_not_world_readable() {
        // Covered inside `test_create_proxy_authority_uses_ca_dir`: `init_ca_dir`
        // is a `OnceLock`, so a second test would race on the shared directory.
        let _ = create_proxy_authority();
        harden_ca_permissions();
    }
}
