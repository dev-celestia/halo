//! Install Google's `antigravity-acp` archive from the ACP registry pin.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::probe;

/// Pinned to ACP registry `antigravity-acp` 1.4.4 (sha256 from agent.json).
pub const PINNED_VERSION: &str = "1.4.4";

/// The archive is ~100MB+; reqwest's default 30s timeout fails mid-body on
/// typical connections (`download body: error decoding response body`).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

struct Pin {
    url: &'static str,
    sha256: &'static str,
}

fn pinned() -> Option<Pin> {
    let (url, sha256) = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        (
            "https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-1.4.4-darwin-arm64.zip",
            "a3cf324046396380fcd8c055a4a4c83ac4ea74a38ea883ccbfce5ca1e9927ae9",
        )
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        (
            "https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-1.4.4-darwin-x86_64.zip",
            "019e6ce30cf906ba94b761d752c7ccd01a324f4701efca31542de04b60be8fd7",
        )
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        (
            "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-1.4.4-linux-x86_64.zip",
            "baf31b7a97602dee359cf2a43f72b6e11c6bf354be874987d80660cd3526bb6a",
        )
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        (
            "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-1.4.4-linux-arm64.zip",
            "b0273227c07cfa1ea5c1d9ff869b64c5b2b39b98862081fcd4c597ec316e0c57",
        )
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        (
            "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-1.4.4-windows-x86_64.zip",
            "4b68e5936a3d7a60b520aa1ee9479c8be58c43e015f026dc41e7ad17ec54d427",
        )
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        (
            "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-1.4.4-windows-arm64.zip",
            "655835b095e160d726ee894a10e56960a9a81b03503edbd348079336163b28de",
        )
    } else {
        return None;
    };
    Some(Pin { url, sha256 })
}

pub fn adapters_root() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("halo").join("adapters").join("antigravity-acp"))
}

pub fn managed_dir() -> Option<PathBuf> {
    adapters_root().map(|r| r.join(PINNED_VERSION))
}

pub fn managed_entry() -> Option<PathBuf> {
    managed_dir().map(|d| d.join(probe::entry_name()))
}

/// Ensure the pinned ACP server is on disk; returns the entry path.
pub fn ensure_installed() -> Result<PathBuf, String> {
    if let Some(entry) = managed_entry().filter(|p| p.is_file()) {
        return Ok(entry);
    }
    let pin = pinned().ok_or_else(|| {
        "Antigravity ACP has no pinned archive for this platform; set ANTIGRAVITY_ACP_EXECUTABLE"
            .to_string()
    })?;
    let dir = managed_dir().ok_or_else(|| "cannot resolve Halo data directory".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("create install dir: {e}"))?;

    let zip_path = dir.join("server.zip");
    download(&pin.url, &zip_path)?;
    verify_sha256(&zip_path, pin.sha256)?;
    extract_zip(&zip_path, &dir)?;
    let _ = fs::remove_file(&zip_path);

    let entry = dir.join(probe::entry_name());
    if !entry.is_file() {
        return Err(format!(
            "archive extracted but {} is missing",
            probe::entry_name()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&entry)
            .map_err(|e| format!("stat entry: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&entry, perms).map_err(|e| format!("chmod entry: {e}"))?;
        let harness = dir.join("localharness_external");
        if harness.is_file() {
            let mut perms = fs::metadata(&harness)
                .map_err(|e| format!("stat harness: {e}"))?
                .permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&harness, perms);
        }
    }
    Ok(entry)
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("download client: {e}"))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|e| format!("download: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("download HTTP {}", response.status()));
    }

    // Stream to disk — avoid buffering the full ~100MB+ archive in memory.
    let mut file = fs::File::create(dest).map_err(|e| format!("write zip: {e}"))?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|e| format!("download body: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("write zip: {e}"))?;
    }
    file.flush().map_err(|e| format!("write zip: {e}"))?;
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| format!("read zip: {e}"))?;
    let digest = Sha256::digest(&bytes);
    let actual = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if actual != expected {
        return Err(format!(
            "Antigravity archive sha256 mismatch (got {actual}, expected {expected})"
        ));
    }
    Ok(())
}

fn extract_zip(zip: &Path, dest: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -Force -Path '{}' -DestinationPath '{}'",
                    zip.display(),
                    dest.display()
                ),
            ])
            .status()
            .map_err(|e| format!("expand archive: {e}"))?;
        if !status.success() {
            return Err(format!("Expand-Archive failed ({status})"));
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("unzip")
            .args(["-o", "-q"])
            .arg(zip)
            .arg("-d")
            .arg(dest)
            .status()
            .map_err(|e| format!("unzip: {e}"))?;
        if !status.success() {
            return Err(format!("unzip failed ({status})"));
        }
        Ok(())
    }
}
