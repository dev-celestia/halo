//! MITM certificate-authority trust probes.

use super::*;

pub(crate) fn probe_mitm_ca(backend: &dyn Backend) -> DoctorItem {
    let ca_pem = backend.get_ca_cert();
    let ca_exists = ca_pem.is_ok();

    #[cfg(target_os = "macos")]
    let trusted = ca_pem
        .as_ref()
        .ok()
        .map(|pem| ca_trusted(pem))
        .unwrap_or(false);

    #[cfg(not(target_os = "macos"))]
    let trusted = ca_exists;

    let status = if trusted && ca_exists {
        DoctorStatus::Ready
    } else if ca_exists {
        DoctorStatus::NeedsUpgrade
    } else {
        DoctorStatus::Missing
    };

    let ca_path = dirs::data_local_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Library/Application Support/Halo")))
        .map(|d| d.join("halo-ca.pem").display().to_string());

    DoctorItem {
        id: "proxy:mitm-ca".to_string(),
        category: DoctorCategory::BrowserProxy,
        name: "Halo MITM Root CA".to_string(),
        description: "Local Root Certificate Authority for HTTPS inspection and proxying"
            .to_string(),
        command: "halo-ca".to_string(),
        status,
        version: if !ca_exists {
            None
        } else if trusted {
            Some("Trusted".into())
        } else {
            Some("Present, untrusted".into())
        },
        resolved_path: ca_path,
        hint: if status == DoctorStatus::Ready {
            "Installed and trusted in the login keychain.".to_string()
        } else if ca_exists {
            "The CA exists but the login keychain does not trust it, so HTTPS interception fails in every client. Click Trust CA to install it as a trust anchor."
                .to_string()
        } else {
            "No CA on disk yet. Click Trust CA — it generates the certificate and installs it as a trust anchor in one step."
                .to_string()
        },
        can_auto_install: true,
        action_label: "Trust CA".to_string(),
        doc_url: None,
    }
}

/// Is this PEM a trusted root on this machine?
///
/// `security find-certificate -c <name>` is not enough: the installer writes
/// several legacy brand names, and a present-but-untrusted anchor looks
/// identical to a trusted one. Verify the chain, and fall back to "is any of
/// our known names in the keychain" when `verify-cert` cannot answer.
#[cfg(target_os = "macos")]
pub(crate) fn ca_trusted(pem: &str) -> bool {
    let path = std::env::temp_dir().join(format!("halo-ca-verify-{}.pem", std::process::id()));
    let verified = std::fs::write(&path, pem).is_ok()
        && Command::new("security")
            .arg("verify-cert")
            .arg("-c")
            .arg(&path)
            .args(["-p", "ssl"])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false);
    let _ = std::fs::remove_file(&path);
    verified || keychain_has_our_ca()
}

/// Names the CA installer writes, current brand first. Kept in sync with
/// `crates/intercept/src/cert.rs`.
#[cfg(target_os = "macos")]
pub(crate) fn keychain_has_our_ca() -> bool {
    const NAMES: [&str; 6] = [
        "Halo Proxy CA",
        "Hexbuffer Proxy CA",
        "Hexbuffer Security Tools Root CA",
        "hexbuffer Security Tools Root CA",
        "Apprecon Security Tools Root CA",
        "Apprecon Proxy CA",
    ];
    NAMES.iter().any(|name| {
        Command::new("security")
            .args(["find-certificate", "-c", name])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    })
}
