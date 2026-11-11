//! Filesystem hardening for data that must stay private to the current user.
//!
//! Halo keeps two categories of sensitive material under the shared app
//! directory: the trusted-root **CA private key**, and **captured traffic**
//! (cookies, `Authorization` headers, request bodies) in SQLite and in payload
//! segments. Both must be reachable by the current user only, so every create
//! or open in that tree goes through this module instead of raw `fs::write` /
//! `OpenOptions` — which default to `0666 & ~umask`, i.e. world-readable.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// Directories holding key material or captured traffic.
pub const DIR_MODE: u32 = 0o700;
/// Files holding key material or captured traffic.
pub const FILE_MODE: u32 = 0o600;

/// Best-effort `chmod`. Never fails the caller: an unreadable-but-private file
/// is a lesser problem than refusing to start.
#[cfg(unix)]
pub fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;

    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    let mut perms = meta.permissions();
    if perms.mode() & 0o777 != mode {
        perms.set_mode(mode);
        let _ = fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
pub fn set_mode(_path: &Path, _mode: u32) {
    // Windows inherits the `%APPDATA%` ACL, which is already per-user (plus
    // Administrators). Tightening it further needs an ACL API that std does
    // not expose; see the audit report for the follow-up.
}

/// `create_dir_all` + `0700`. Used for the app dir, the CA dir, payload and
/// session directories.
pub fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    set_mode(path, DIR_MODE);
    Ok(())
}

/// Tighten an existing file to `0600`. Used to heal installs created before
/// this module existed (the CA key shipped world-readable).
pub fn harden_file(path: &Path) {
    set_mode(path, FILE_MODE);
}

/// Tighten an existing file only when it is present.
pub fn harden_file_if_exists(path: &Path) {
    if path.exists() {
        set_mode(path, FILE_MODE);
    }
}

/// Open (creating if needed) a private file for append.
///
/// `O_NOFOLLOW` refuses a symlink planted at the final path component, so a
/// local attacker cannot redirect writes into a file the user owns.
pub fn open_private_append(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.create(true).append(true).read(true);
    apply_private_flags(&mut opts);

    let file = opts.open(path)?;
    harden_file(path);
    Ok(file)
}

/// Create or truncate a private file. Used for log rotation and the
/// "clear log" action.
pub fn truncate_private(path: &Path) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    apply_private_flags(&mut opts);

    opts.open(path)?;
    harden_file(path);
    Ok(())
}

#[cfg(unix)]
fn apply_private_flags(opts: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    opts.mode(FILE_MODE);
    // Do not follow a symlink at the target path.
    opts.custom_flags(libc::O_NOFOLLOW);
}

#[cfg(not(unix))]
fn apply_private_flags(_opts: &mut OpenOptions) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_dir_and_file_are_not_world_readable() {
        let root = std::env::temp_dir().join(format!("halo-fs-secure-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);

        let dir = root.join("nested");
        create_private_dir(&dir).expect("create private dir");
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, DIR_MODE, "dir must be 0700, got {mode:o}");

        let file = dir.join("payload.bin");
        let mut f = open_private_append(&file).expect("open private file");
        use std::io::Write;
        f.write_all(b"x").unwrap();
        drop(f);

        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, FILE_MODE, "file must be 0600, got {mode:o}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn truncate_refuses_symlink() {
        let root = std::env::temp_dir().join(format!("halo-fs-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        create_private_dir(&root).expect("create dir");

        let victim = root.join("victim.txt");
        fs::write(&victim, b"do not truncate me").unwrap();
        let link = root.join("log.txt");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        assert!(
            truncate_private(&link).is_err(),
            "truncate must refuse a symlink (O_NOFOLLOW)"
        );
        assert_eq!(fs::read(&victim).unwrap(), b"do not truncate me");

        let _ = fs::remove_dir_all(&root);
    }
}
