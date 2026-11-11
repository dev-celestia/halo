//! CA certificate export, regeneration, and platform trust.

use std::path::{Path, PathBuf};
use std::process::Command;

use halo_traffic::ca::{export_ca_cert_pem, regenerate_ca};

/// PEM text of the local Halo root CA.
pub fn get_ca_cert() -> Result<String, String> {
    let pem = export_ca_cert_pem().map_err(|error| format!("{error}"))?;
    String::from_utf8(pem).map_err(|e| e.to_string())
}

/// Write the CA PEM to `~/<app-data>/halo-ca.pem` (used by trust install).
pub fn export_ca_cert_file() -> Result<PathBuf, String> {
    let app_data_dir = halo_core::paths::get_shared_app_dir();
    std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
    export_ca_cert_to(app_data_dir.join("halo-ca.pem"))
}

/// Write the CA PEM to an explicit destination (user-picked save path).
///
/// The parent directory must already exist: this path comes from a save
/// dialog, so it does. Creating it on demand (`create_dir_all`) turned the
/// function into a general-purpose "make these directories and write this
/// file anywhere" helper reachable from the bridge.
pub fn export_ca_cert_to(path: impl AsRef<std::path::Path>) -> Result<PathBuf, String> {
    let path = path.as_ref().to_path_buf();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
    {
        return Err(format!("Directory does not exist: {}", parent.display()));
    }
    let ca_pem = export_ca_cert_pem().map_err(|error| format!("{error}"))?;
    std::fs::write(&path, ca_pem).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Default folder for the CA download save dialog (`~/Downloads`).
pub fn default_ca_download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Wipe and regenerate the local root CA key material.
pub fn regenerate_ca_cert() -> Result<(), String> {
    regenerate_ca().map_err(|e| e.to_string())
}

/// Install / trust the CA for external browsers (macOS Keychain or Chrome NSS).
pub fn trust_ca() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        install_ca_to_macos_keychain()
    }

    #[cfg(not(target_os = "macos"))]
    {
        import_ca_to_chrome_nss()
    }
}

fn write_ca_pem() -> Result<PathBuf, String> {
    export_ca_cert_file()
}

#[cfg(target_os = "macos")]
fn user_login_keychain_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    Ok(PathBuf::from(home).join("Library/Keychains/login.keychain-db"))
}

#[cfg(target_os = "macos")]
fn run_security(args: &[String]) -> Result<(), String> {
    let output = Command::new("security")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        format!("security exited with {}", output.status)
    } else {
        stderr
    })
}

/// Upper-case hex SHA-256 of our CA certificate, as printed by
/// `security find-certificate -Z`.
#[cfg(target_os = "macos")]
pub(crate) fn ca_sha256_hex() -> Option<String> {
    use base64::{Engine, engine::general_purpose};
    use sha2::{Digest, Sha256};

    let pem = export_ca_cert_pem().ok()?;
    let pem = String::from_utf8(pem).ok()?;
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("-----"))
        .collect();
    let der = general_purpose::STANDARD.decode(body.as_bytes()).ok()?;

    Some(
        Sha256::digest(&der)
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect(),
    )
}

/// Whether `cert_name` in `keychain` resolves to **our** CA.
///
/// The cleanup loop below deletes by common name, and that name list includes
/// other products' trust anchors ("Apprecon Proxy CA"). Deleting those would
/// silently untrust a different tool, so only a certificate whose SHA-256
/// matches ours is removed.
#[cfg(target_os = "macos")]
pub(crate) fn keychain_holds_our_ca(cert_name: &str, keychain: &Path, fingerprint: &str) -> bool {
    let output = Command::new("security")
        .args([
            "find-certificate",
            "-Z",
            "-a",
            "-c",
            cert_name,
            &keychain.display().to_string(),
        ])
        .output();

    match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout)
            .to_ascii_uppercase()
            .contains(fingerprint),
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn install_ca_to_macos_keychain() -> Result<String, String> {
    let ca_path = write_ca_pem()?;
    let keychain_path = user_login_keychain_path()?;
    let cert_names = [
        "Halo Proxy CA",
        // Pre-rebrand trust anchors. Removed only when the fingerprint matches
        // ours, so unrelated products with these names keep their trust.
        "Hexbuffer Proxy CA",
        "Hexbuffer Security Tools Root CA",
        "hexbuffer Security Tools Root CA",
        "Apprecon Security Tools Root CA",
        "Apprecon Proxy CA",
    ];

    for cert_name in &cert_names {
        // Only remove certificates that are actually ours: several of these
        // names belong to other products, and their trust anchors must survive.
        let Some(fingerprint) = ca_sha256_hex() else {
            break;
        };
        for _ in 0..8 {
            if !keychain_holds_our_ca(cert_name, &keychain_path, &fingerprint) {
                break;
            }
            let delete_args = vec![
                "delete-certificate".to_string(),
                "-c".to_string(),
                cert_name.to_string(),
                keychain_path.display().to_string(),
            ];
            if run_security(&delete_args).is_err() {
                break;
            }
        }
    }

    let add_args = vec![
        "add-trusted-cert".to_string(),
        "-r".to_string(),
        "trustRoot".to_string(),
        "-p".to_string(),
        "ssl".to_string(),
        "-k".to_string(),
        keychain_path.display().to_string(),
        ca_path.display().to_string(),
    ];

    run_security(&add_args).map(|_| {
        "Halo CA installed in your macOS login keychain and trusted for SSL. Restart browsers that were already open.".to_string()
    })
}

#[cfg(not(target_os = "macos"))]
fn certutil_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("certutil"),
        PathBuf::from("/opt/homebrew/opt/nss/bin/certutil"),
        PathBuf::from("/opt/homebrew/bin/certutil"),
        PathBuf::from("/usr/local/opt/nss/bin/certutil"),
        PathBuf::from("/usr/local/bin/certutil"),
    ]
}

#[cfg(not(target_os = "macos"))]
fn run_certutil(args: &[String]) -> Result<(), String> {
    let mut last_error = None;

    for candidate in certutil_candidates() {
        let output = Command::new(&candidate).args(args).output();

        match output {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                last_error = Some(if stderr.is_empty() {
                    format!("certutil exited with {}", output.status)
                } else {
                    stderr
                });
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        "certutil was not found. Install NSS tools first, for example: brew install nss".to_string()
    }))
}

#[cfg(not(target_os = "macos"))]
fn import_ca_to_chrome_nss() -> Result<String, String> {
    let ca_path = write_ca_pem()?;
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    let nss_db = PathBuf::from(home).join(".pki/nssdb");
    std::fs::create_dir_all(&nss_db).map_err(|e| e.to_string())?;

    let db_arg = format!("sql:{}", nss_db.display());
    let nick = "Halo Proxy CA".to_string();

    // Drop both the current and the pre-rebrand nickname before adding.
    for stale in ["Halo Proxy CA", "Hexbuffer Proxy CA"] {
        let _ = run_certutil(&[
            "-d".into(),
            db_arg.clone(),
            "-D".into(),
            "-n".into(),
            stale.to_string(),
        ]);
    }

    run_certutil(&[
        "-d".into(),
        db_arg,
        "-A".into(),
        "-t".into(),
        "C,,".into(),
        "-n".into(),
        nick,
        "-i".into(),
        ca_path.display().to_string(),
    ])
    .map(|_| {
        "Halo CA imported into the Chrome/NSS trust store. Restart browsers that were already open."
            .to_string()
    })
}
