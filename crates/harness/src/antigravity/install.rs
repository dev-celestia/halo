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

/// Written once extraction and permission setup finish cleanly.
pub const INSTALL_MARKER: &str = ".halo-install-ok";

/// The archive is ~100MB+; reqwest's default 30s timeout fails mid-body on
/// typical connections (`download body: error decoding response body`).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// Hard ceiling on archive size (512 MiB) so a misbehaving upstream cannot fill disk.
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;

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
    ensure_installed_with_progress(|_| {})
}

/// Ensure the pinned ACP server is on disk while reporting human-readable
/// download/extraction progress stages.
pub fn ensure_installed_with_progress<F>(mut on_progress: F) -> Result<PathBuf, String>
where
    F: FnMut(String),
{
    if let (Some(dir), Some(entry)) = (managed_dir(), managed_entry()) {
        let is_regular_file = fs::symlink_metadata(&entry).is_ok_and(|m| m.is_file());
        if is_regular_file && dir.join(INSTALL_MARKER).is_file() {
            return Ok(entry);
        }
    }
    let pin = pinned().ok_or_else(|| {
        "Antigravity ACP has no pinned archive for this platform; set ANTIGRAVITY_ACP_EXECUTABLE"
            .to_string()
    })?;
    let dir = managed_dir().ok_or_else(|| "cannot resolve Halo data directory".to_string())?;
    fs::create_dir_all(&dir).map_err(|e| format!("create install dir: {e}"))?;

    let part_path = dir.join("server.zip.part");
    let zip_path = dir.join("server.zip");
    let marker_path = dir.join(INSTALL_MARKER);
    let _ = fs::remove_file(&part_path);
    let _ = fs::remove_file(&marker_path);

    on_progress("Downloading Antigravity ACP…".into());
    if let Err(err) = download_and_verify(pin.url, &part_path, pin.sha256, &mut on_progress) {
        let _ = fs::remove_file(&part_path);
        return Err(err);
    }
    fs::rename(&part_path, &zip_path).map_err(|e| format!("stage zip: {e}"))?;

    on_progress("Extracting Antigravity ACP archive…".into());
    let extract_res = extract_zip(&zip_path, &dir);
    let _ = fs::remove_file(&zip_path);
    extract_res?;

    let entry = dir.join(probe::entry_name());
    let entry_meta = fs::symlink_metadata(&entry).map_err(|_| {
        format!(
            "archive extracted but {} is missing",
            probe::entry_name()
        )
    })?;
    if !entry_meta.is_file() {
        return Err(format!(
            "archive entry {} is not a regular file",
            probe::entry_name()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = entry_meta.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&entry, perms).map_err(|e| format!("chmod entry: {e}"))?;
        let harness = dir.join("localharness_external");
        if let Ok(harness_meta) = fs::symlink_metadata(&harness) {
            if harness_meta.is_file() {
                let mut perms = harness_meta.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&harness, perms);
            }
        }
    }
    let _ = fs::write(&marker_path, format!("{PINNED_VERSION}\n"));
    on_progress("Ready".into());
    Ok(entry)
}

fn download_and_verify<F>(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    on_progress: &mut F,
) -> Result<(), String>
where
    F: FnMut(String),
{
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("download client: {e}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("download: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("download HTTP {}", response.status()));
    }
    let total_bytes = response.content_length();
    stream_to_file_and_verify_with_limit(
        response,
        dest,
        total_bytes,
        MAX_ARCHIVE_BYTES,
        expected_sha256,
        on_progress,
    )
}

#[cfg(test)]
fn stream_to_file_and_verify<R, F>(
    reader: R,
    dest: &Path,
    total_bytes: Option<u64>,
    expected_sha256: &str,
    on_progress: &mut F,
) -> Result<(), String>
where
    R: Read,
    F: FnMut(String),
{
    stream_to_file_and_verify_with_limit(
        reader,
        dest,
        total_bytes,
        MAX_ARCHIVE_BYTES,
        expected_sha256,
        on_progress,
    )
}

fn stream_to_file_and_verify_with_limit<R, F>(
    mut reader: R,
    dest: &Path,
    total_bytes: Option<u64>,
    max_bytes: u64,
    expected_sha256: &str,
    on_progress: &mut F,
) -> Result<(), String>
where
    R: Read,
    F: FnMut(String),
{
    if let Some(total) = total_bytes {
        if total > max_bytes {
            return Err(format!(
                "Antigravity archive size ({total} bytes) exceeds safety limit ({max_bytes} bytes)"
            ));
        }
    }
    // Stream to disk while computing SHA-256 incrementally — avoids buffering
    // or re-reading the ~100MB+ archive in memory.
    let mut file = fs::File::create(dest).map_err(|e| format!("write zip: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_reported_bytes: u64 = 0;
    const REPORT_STRIDE: u64 = 1024 * 1024; // 1 MiB

    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("download body: {e}"))?;
        if n == 0 {
            break;
        }
        downloaded = downloaded.saturating_add(n as u64);
        if downloaded > max_bytes {
            return Err(format!(
                "Antigravity archive download exceeded safety limit ({max_bytes} bytes)"
            ));
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("write zip: {e}"))?;
        hasher.update(&buf[..n]);
        if downloaded.saturating_sub(last_reported_bytes) >= REPORT_STRIDE {
            last_reported_bytes = downloaded;
            let mb = downloaded as f64 / (1024.0 * 1024.0);
            if let Some(total) = total_bytes.filter(|&t| t > 0) {
                let pct = ((downloaded.saturating_mul(100)) / total).min(100);
                let total_mb = total as f64 / (1024.0 * 1024.0);
                on_progress(format!("Downloading ({pct}% · {mb:.0}/{total_mb:.0} MB)…"));
            } else {
                on_progress(format!("Downloading ({mb:.0} MB)…"));
            }
        }
    }
    file.flush().map_err(|e| format!("write zip: {e}"))?;
    on_progress("Verifying SHA-256 checksum…".into());
    let digest = hasher.finalize();
    let actual = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if actual != expected_sha256 {
        return Err(format!(
            "Antigravity archive sha256 mismatch (got {actual}, expected {expected_sha256})"
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
                "-NonInteractive",
                "-Command",
                "Expand-Archive -Force -LiteralPath $env:HALO_ZIP_SRC -DestinationPath $env:HALO_ZIP_DEST",
            ])
            .env("HALO_ZIP_SRC", zip.as_os_str())
            .env("HALO_ZIP_DEST", dest.as_os_str())
            .stdin(std::process::Stdio::null())
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
            .stdin(std::process::Stdio::null())
            .status()
            .map_err(|e| format!("unzip: {e}"))?;
        if !status.success() {
            return Err(format!("unzip failed ({status})"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_to_file_verifies_sha256_and_writes_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("server.zip.part");
        let payload = b"antigravity-test-archive-payload";
        let expected = Sha256::digest(payload)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut stages = Vec::new();
        stream_to_file_and_verify(
            &payload[..],
            &dest,
            Some(payload.len() as u64),
            &expected,
            &mut |stage| stages.push(stage),
        )
        .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);
        assert!(stages.iter().any(|s| s.contains("Verifying SHA-256")));
    }

    #[test]
    fn stream_to_file_rejects_mismatched_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("server.zip.part");
        let payload = b"corrupted-payload";
        let err = stream_to_file_and_verify(
            &payload[..],
            &dest,
            None,
            "0000000000000000000000000000000000000000000000000000000000000000",
            &mut |_| {},
        )
        .unwrap_err();
        assert!(err.contains("sha256 mismatch"));
    }

    #[test]
    fn stream_to_file_rejects_archive_exceeding_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("server.zip.part");
        let payload = b"0123456789abcdef";
        let err = stream_to_file_and_verify_with_limit(
            &payload[..],
            &dest,
            None,
            8,
            "unused",
            &mut |_| {},
        )
        .unwrap_err();
        assert!(err.contains("safety limit"));
    }
}
