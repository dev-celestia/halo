//! `file://` URI ↔ filesystem path conversion.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_types::Uri;

use super::error::{LspError, LspResult};

/// Convert a filesystem path to an LSP `file://` URI.
pub fn path_to_uri(path: &Path) -> LspResult<Uri> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let url = url::Url::from_file_path(&abs)
        .map_err(|_| LspError::message(format!("bad path: {}", abs.display())))?;
    Uri::from_str(url.as_str())
        .map_err(|e| LspError::message(format!("bad uri {}: {e}", url.as_str())))
}

/// Convert a `file://` URI back to a filesystem path.
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let url = url::Url::parse(uri.as_str()).ok()?;
    url.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_uri_round_trip() {
        let dir = std::env::temp_dir();
        let path = dir.join("halo-lsp-test-file.rs");
        let _ = std::fs::write(&path, b"");
        let uri = path_to_uri(&path).unwrap();
        let back = uri_to_path(&uri).unwrap();
        assert_eq!(back, path.canonicalize().unwrap_or(path));
    }
}
