//! Attachment filtering and bounded, off-thread image payload preparation.

use std::io::Read;
use std::path::Path;

use base64::Engine as _;
use halo_harness::PiImage;

const MAX_IMAGES: usize = 8;
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 10 * 1024 * 1024;

/// Cheap extension filtering for the picker; this is not content validation.
pub fn is_supported_image_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp"))
}

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        Some("image/png")
    } else if bytes.len() >= 4 && bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.len() >= 13 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        Some("image/gif")
    } else if bytes.len() >= 16 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The caller runs this on a worker. Read a regular file through one handle and
/// cap the read itself, not just a racy pre-read metadata length check.
pub(crate) fn prepare_images(paths: &[String]) -> Result<Vec<PiImage>, String> {
    if paths.len() > MAX_IMAGES {
        return Err(format!("Attach at most {MAX_IMAGES} images per message"));
    }
    let mut total = 0usize;
    let mut images = Vec::with_capacity(paths.len());
    for path in paths {
        if !is_supported_image_path(path) {
            return Err("Unsupported image attachment; use PNG, JPEG, GIF or WebP".into());
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            // A replaced path could be a FIFO; don't block before validating
            // the opened handle. Nonblocking mode is inert for regular files.
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options
            .open(path)
            .map_err(|_| "An image attachment could not be opened")?;
        let metadata = file
            .metadata()
            .map_err(|_| "An image attachment could not be inspected")?;
        if !metadata.is_file() {
            return Err("Image attachments must be regular files".into());
        }
        if metadata.len() > MAX_IMAGE_BYTES as u64 {
            return Err("Each image attachment must be at most 5 MiB".into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_IMAGE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "An image attachment could not be read")?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err("An image attachment exceeded the 5 MiB limit while reading".into());
        }
        total += bytes.len();
        if total > MAX_TOTAL_BYTES {
            return Err("Image attachments must total at most 10 MiB per message".into());
        }
        let mime_type =
            image_mime(&bytes).ok_or("The attachment contents are not a supported image")?;
        images.push(PiImage {
            data: base64::engine::general_purpose::STANDARD.encode(&bytes),
            mime_type: mime_type.to_owned(),
        });
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mime_from_contents_not_filename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("misnamed.PNG");
        let bytes = b"GIF89a\x01\0\x01\0\0\0\0";
        std::fs::write(&path, bytes).unwrap();
        let images = prepare_images(&[path.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(images[0].mime_type, "image/gif");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&images[0].data)
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn rejects_disguised_truncated_and_oversized_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        std::fs::write(&path, "not an image").unwrap();
        assert!(prepare_images(&[path.to_string_lossy().into_owned()]).is_err());
        std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
        assert!(prepare_images(&[path.to_string_lossy().into_owned()]).is_err());
        std::fs::File::create(&path)
            .unwrap()
            .set_len((MAX_IMAGE_BYTES + 1) as u64)
            .unwrap();
        assert!(prepare_images(&[path.to_string_lossy().into_owned()]).is_err());
        assert!(prepare_images(&vec!["missing.png".into(); MAX_IMAGES + 1]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn named_pipe_attachment_is_rejected_without_waiting_for_a_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pipe.png");
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: c_path is a valid nul-terminated path in this test's temporary directory.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let error = prepare_images(&[path.to_string_lossy().into_owned()]).unwrap_err();
        assert!(error.contains("regular files"));
    }

    #[test]
    fn launch_context_debug_never_formats_credentials() {
        let context = super::super::types::AgentLaunchContext {
            anthropic_api_key: Some("test-secret-not-for-logs".into()),
            ..Default::default()
        };
        assert!(!format!("{context:?}").contains("test-secret"));
    }
}
