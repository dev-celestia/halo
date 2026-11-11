//! R2 Storage app bridge: credentials, bucket/prefix browsing, transfers.
//!
//! Every backend call blocks on the S3 client, so each one runs on a named
//! worker thread and is polled from a GPUI task (same shape as the Port
//! Scanner). Listings carry a request epoch so a slow response can never
//! overwrite a newer prefix.

use std::path::{Path, PathBuf};

use super::file_explorer::{is_safe_file_name, ExplorerViewMode};
use super::*;

pub use halo_backend::commands::r2::{
    R2CredentialsStatus, R2FileEntry, R2FolderEntry, R2Listing, R2UploadProgress,
};

/// R2 bucket names: 3–63 lowercase alphanumerics with single interior hyphens.
pub fn is_valid_bucket_name(name: &str) -> bool {
    (3..=63).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct R2View {
    pub credentials: Option<R2CredentialsStatus>,
    pub credentials_loaded: bool,
    pub loading_credentials: bool,
    pub buckets: Vec<String>,
    pub bucket: Option<String>,
    /// Canonical prefix: empty or ending in `/`.
    pub prefix: String,
    pub folders: Vec<R2FolderEntry>,
    pub files: Vec<R2FileEntry>,
    pub selected_key: Option<String>,
    pub search: String,
    /// Inline "New Folder" form open.
    pub creating: bool,
    pub view_mode: ExplorerViewMode,
    pub loading: bool,
    pub busy: bool,
    pub upload: Option<R2UploadProgress>,
    pub error: Option<String>,
    /// Bumped on every listing request; stale results are dropped.
    pub request_epoch: u64,
}

impl R2View {
    pub fn has_credentials(&self) -> bool {
        self.credentials
            .as_ref()
            .map(|c| c.has_secret)
            .unwrap_or(false)
    }

    pub fn filtered_folders(&self) -> Vec<R2FolderEntry> {
        let query = self.search.to_lowercase();
        self.folders
            .iter()
            .filter(|folder| query.is_empty() || folder.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    pub fn filtered_files(&self) -> Vec<R2FileEntry> {
        let query = self.search.to_lowercase();
        self.files
            .iter()
            .filter(|file| {
                query.is_empty()
                    || file.name.to_lowercase().contains(&query)
                    || file.key.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    /// Breadcrumb segments of the current bucket + prefix.
    pub fn breadcrumbs(&self) -> Vec<(String, String)> {
        let mut crumbs = Vec::new();
        let mut acc = String::new();
        if let Some(bucket) = &self.bucket {
            crumbs.push((bucket.clone(), acc.clone()));
        }
        for segment in self.prefix.trim_end_matches('/').split('/') {
            if segment.is_empty() {
                continue;
            }
            acc.push_str(segment);
            acc.push('/');
            crumbs.push((segment.to_string(), acc.clone()));
        }
        crumbs
    }

    /// Full object key for a file in the current prefix.
    pub fn key_for(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name)
    }
}

/// Normalize a prefix so it is empty or ends with `/`.
pub fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim_start_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

/// Spawn `work` on a named R2 worker thread.
fn r2_dispatch<T, F>(work: F) -> std::sync::mpsc::Receiver<BridgeResult<T>>
where
    T: Send + 'static,
    F: FnOnce() -> BridgeResult<T> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("halo-r2".into())
        .spawn(move || {
            let _ = tx.send(work());
        });
    rx
}

macro_rules! r2_wait {
    ($rx:expr, $executor:expr) => {{
        loop {
            match $rx.try_recv() {
                Ok(result) => break result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    $executor.timer(Duration::from_millis(50)).await;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    break Err(BridgeError::message("R2 backend worker died"));
                }
            }
        }
    }};
}

fn r2_offload<T, F, A>(work: F, apply: A, cx: &mut App)
where
    T: Send + 'static,
    F: FnOnce() -> BridgeResult<T> + Send + 'static,
    A: FnOnce(&mut R2View, BridgeResult<T>) + 'static,
{
    let handle = Bridge::state(cx);
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        let rx = r2_dispatch(work);
        let result = r2_wait!(rx, &executor);
        let _ = cx.update(move |cx| {
            handle.update(cx, |s| apply(&mut s.r2, result));
        });
    })
    .detach();
}

impl Bridge {
    // ── credentials ────────────────────────────────────────────────────────

    pub fn r2_refresh_credentials(cx: &mut App) {
        let backend = match Self::backend(cx) {
            Ok(backend) => backend,
            Err(err) => {
                Self::state(cx).update(cx, |s| {
                    s.r2.credentials_loaded = true;
                    s.r2.error = Some(err.to_string());
                });
                return;
            }
        };
        Self::state(cx).update(cx, |s| {
            s.r2.loading_credentials = true;
        });

        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let rx = r2_dispatch(move || backend.r2_credentials_status());
            let result = r2_wait!(rx, &executor);
            let fetch_buckets = cx.update(|cx| {
                handle.update(cx, |s| {
                    s.r2.loading_credentials = false;
                    s.r2.credentials_loaded = true;
                    match result {
                        Ok(credentials) => {
                            s.r2.credentials = credentials;
                        }
                        Err(err) => {
                            s.r2.error = Some(err.to_string());
                        }
                    }
                    s.r2.has_credentials() && s.r2.buckets.is_empty()
                })
            });
            if fetch_buckets {
                let _ = cx.update(|cx| {
                    let _ = Bridge::r2_refresh_buckets(cx);
                });
            }
        })
        .detach();
    }

    pub fn r2_save_credentials(
        account_id: String,
        access_key_id: String,
        secret_access_key: Option<String>,
        custom_endpoint_url: Option<String>,
        cx: &mut App,
    ) -> BridgeResult<()> {
        if account_id.trim().is_empty() || access_key_id.trim().is_empty() {
            return Err(BridgeError::message(
                "Account ID and Access Key ID are required",
            ));
        }
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.r2.busy = true;
            s.r2.error = None;
        });

        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let rx = r2_dispatch(move || {
                backend.r2_save_credentials(
                    account_id,
                    access_key_id,
                    secret_access_key,
                    custom_endpoint_url,
                )
            });
            let result = r2_wait!(rx, &executor);
            let saved = cx.update(|cx| {
                handle.update(cx, |s| {
                    s.r2.busy = false;
                    match result {
                        Ok(()) => {
                            s.r2.credentials_loaded = false;
                            s.r2.buckets.clear();
                            s.r2.bucket = None;
                            s.r2.prefix.clear();
                            s.r2.folders.clear();
                            s.r2.files.clear();
                            s.apply_ok_msg("R2 credentials saved".into());
                            true
                        }
                        Err(err) => {
                            s.r2.error = Some(err.to_string());
                            false
                        }
                    }
                })
            });
            if saved {
                let _ = cx.update(|cx| Bridge::r2_refresh_credentials(cx));
            }
        })
        .detach();
        Ok(())
    }

    pub fn r2_clear_credentials(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.r2.busy = true;
            s.r2.error = None;
        });
        r2_offload(
            move || backend.r2_clear_credentials(),
            |view, result| {
                view.busy = false;
                match result {
                    Ok(()) => {
                        view.credentials = None;
                        view.credentials_loaded = true;
                        view.buckets.clear();
                        view.bucket = None;
                        view.prefix.clear();
                        view.folders.clear();
                        view.files.clear();
                        view.selected_key = None;
                        view.upload = None;
                    }
                    Err(err) => view.error = Some(err.to_string()),
                }
            },
            cx,
        );
        Ok(())
    }

    // ── buckets ────────────────────────────────────────────────────────────

    pub fn r2_refresh_buckets(cx: &mut App) -> BridgeResult<()> {
        if !Self::state(cx).read(cx).r2.has_credentials() {
            return Ok(());
        }
        let backend = Self::backend(cx)?;
        let epoch = Self::state(cx).update(cx, |s| {
            s.r2.request_epoch += 1;
            s.r2.loading = true;
            s.r2.error = None;
            s.r2.request_epoch
        });
        r2_offload(
            move || backend.r2_list_buckets(),
            move |view, result| {
                if view.request_epoch != epoch {
                    return;
                }
                view.loading = false;
                match result {
                    Ok(buckets) => {
                        view.buckets = buckets;
                        let missing = view
                            .bucket
                            .as_ref()
                            .map(|current| !view.buckets.contains(current))
                            .unwrap_or(false);
                        if missing {
                            view.bucket = None;
                            view.prefix.clear();
                            view.folders.clear();
                            view.files.clear();
                            view.selected_key = None;
                        }
                    }
                    Err(err) => view.error = Some(err.to_string()),
                }
            },
            cx,
        );
        Ok(())
    }

    pub fn r2_create_bucket(name: &str, cx: &mut App) -> BridgeResult<()> {
        let name = name.trim().to_string();
        if !is_valid_bucket_name(&name) {
            return Err(BridgeError::message(
                "Bucket names need 3–63 lowercase letters, digits, or single hyphens",
            ));
        }
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.r2.busy = true;
            s.r2.error = None;
        });

        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let created = name.clone();
            let rx = r2_dispatch(move || backend.r2_create_bucket(created));
            let result = r2_wait!(rx, &executor);
            let ok = cx.update(|cx| {
                handle.update(cx, |s| {
                    s.r2.busy = false;
                    match result {
                        Ok(()) => {
                            s.apply_ok_msg(format!("Created bucket {name}"));
                            true
                        }
                        Err(err) => {
                            s.r2.error = Some(err.to_string());
                            false
                        }
                    }
                })
            });
            if ok {
                let _ = cx.update(move |cx| {
                    let _ = Bridge::r2_refresh_buckets(cx);
                    let _ = Bridge::r2_open_bucket(&name, cx);
                });
            }
        })
        .detach();
        Ok(())
    }

    pub fn r2_delete_bucket(name: &str, cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        Self::state(cx).update(cx, |s| {
            s.r2.busy = true;
            s.r2.error = None;
        });
        let name = name.to_string();
        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let target = name.clone();
            let rx = r2_dispatch(move || backend.r2_delete_bucket(target));
            let result = r2_wait!(rx, &executor);
            let ok = cx.update(|cx| {
                handle.update(cx, |s| {
                    s.r2.busy = false;
                    match result {
                        Ok(()) => {
                            if s.r2.bucket.as_deref() == Some(name.as_str()) {
                                s.r2.bucket = None;
                                s.r2.prefix.clear();
                                s.r2.folders.clear();
                                s.r2.files.clear();
                                s.r2.selected_key = None;
                            }
                            s.apply_ok_msg(format!("Deleted bucket {name}"));
                            true
                        }
                        Err(err) => {
                            s.r2.error = Some(err.to_string());
                            false
                        }
                    }
                })
            });
            if ok {
                let _ = cx.update(|cx| {
                    let _ = Bridge::r2_refresh_buckets(cx);
                });
            }
        })
        .detach();
        Ok(())
    }

    pub fn r2_open_bucket(bucket: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.r2.bucket = Some(bucket.to_string());
            s.r2.prefix.clear();
            s.r2.folders.clear();
            s.r2.files.clear();
            s.r2.selected_key = None;
            s.r2.search.clear();
        });
        Self::r2_refresh(cx)
    }

    // ── objects ────────────────────────────────────────────────────────────

    pub fn r2_navigate(prefix: &str, cx: &mut App) -> BridgeResult<()> {
        let prefix = normalize_prefix(prefix);
        Self::state(cx).update(cx, |s| {
            s.r2.prefix = prefix;
            s.r2.selected_key = None;
        });
        Self::r2_refresh(cx)
    }

    pub fn r2_navigate_up(cx: &mut App) -> BridgeResult<()> {
        let prefix = Self::state(cx).read(cx).r2.prefix.clone();
        let stripped = prefix.trim_end_matches('/');
        let up = match stripped.rfind('/') {
            Some(index) => stripped[..=index].to_string(),
            None => String::new(),
        };
        Self::r2_navigate(&up, cx)
    }

    pub fn r2_refresh(cx: &mut App) -> BridgeResult<()> {
        let Some(bucket) = Self::state(cx).read(cx).r2.bucket.clone() else {
            return Ok(());
        };
        let backend = Self::backend(cx)?;
        let (prefix, epoch) = Self::state(cx).update(cx, |s| {
            s.r2.request_epoch += 1;
            s.r2.loading = true;
            s.r2.error = None;
            (s.r2.prefix.clone(), s.r2.request_epoch)
        });
        r2_offload(
            move || backend.r2_list_objects(bucket, prefix),
            move |view, result| {
                if view.request_epoch != epoch {
                    return;
                }
                view.loading = false;
                match result {
                    Ok(listing) => {
                        let R2Listing { folders, files } = listing;
                        view.folders = folders;
                        view.files = files;
                    }
                    Err(err) => view.error = Some(err.to_string()),
                }
            },
            cx,
        );
        Ok(())
    }

    pub fn r2_create_folder(name: &str, cx: &mut App) -> BridgeResult<()> {
        let name = name.trim().to_string();
        if !is_safe_file_name(&name) {
            return Err(BridgeError::message(
                "Folder name must be a single path component",
            ));
        }
        let Some(bucket) = Self::state(cx).read(cx).r2.bucket.clone() else {
            return Err(BridgeError::message("Open a bucket first"));
        };
        let prefix = Self::state(cx).read(cx).r2.prefix.clone();
        let key = format!("{prefix}{name}/");
        let backend = Self::backend(cx)?;
        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let rx = r2_dispatch(move || backend.r2_create_folder(bucket, key));
            let result = r2_wait!(rx, &executor);
            let ok = cx.update(|cx| {
                handle.update(cx, |s| match result {
                    Ok(()) => {
                        s.apply_ok_msg(format!("Created folder {name}"));
                        true
                    }
                    Err(err) => {
                        s.r2.error = Some(err.to_string());
                        false
                    }
                })
            });
            if ok {
                let _ = cx.update(|cx| {
                    let _ = Bridge::r2_refresh(cx);
                });
            }
        })
        .detach();
        Ok(())
    }

    pub fn r2_delete_object(key: &str, cx: &mut App) -> BridgeResult<()> {
        let Some(bucket) = Self::state(cx).read(cx).r2.bucket.clone() else {
            return Err(BridgeError::message("Open a bucket first"));
        };
        let backend = Self::backend(cx)?;
        let key = key.to_string();
        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let target = key.clone();
            let rx = r2_dispatch(move || backend.r2_delete_object(bucket, target));
            let result = r2_wait!(rx, &executor);
            let ok = cx.update(|cx| {
                handle.update(cx, |s| match result {
                    Ok(()) => {
                        if s.r2.selected_key.as_deref() == Some(key.as_str()) {
                            s.r2.selected_key = None;
                        }
                        s.apply_ok_msg(format!("Deleted {key}"));
                        true
                    }
                    Err(err) => {
                        s.r2.error = Some(err.to_string());
                        false
                    }
                })
            });
            if ok {
                let _ = cx.update(|cx| {
                    let _ = Bridge::r2_refresh(cx);
                });
            }
        })
        .detach();
        Ok(())
    }

    /// Upload one local file into the current prefix.
    pub fn r2_upload(path: PathBuf, cx: &mut App) -> BridgeResult<()> {
        let Some(bucket) = Self::state(cx).read(cx).r2.bucket.clone() else {
            return Err(BridgeError::message("Open a bucket first"));
        };
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            return Err(BridgeError::message("Pick a file to upload"));
        };
        if !is_safe_file_name(&name) {
            return Err(BridgeError::message("Unsupported file name"));
        }
        let prefix = Self::state(cx).read(cx).r2.prefix.clone();
        let key = format!("{prefix}{name}");
        let source = path.display().to_string();
        let backend = Self::backend(cx)?;

        Self::state(cx).update(cx, |s| {
            s.r2.busy = true;
            s.r2.error = None;
            s.r2.upload = Some(R2UploadProgress {
                file_name: name.clone(),
                progress: 0,
            });
        });

        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let rx = r2_dispatch(move || backend.r2_upload_file(bucket, key, source, None));
            let result = r2_wait!(rx, &executor);
            let ok = cx.update(|cx| {
                handle.update(cx, |s| {
                    s.r2.busy = false;
                    s.r2.upload = None;
                    match result {
                        Ok(()) => {
                            s.apply_ok_msg(format!("Uploaded {name}"));
                            true
                        }
                        Err(err) => {
                            s.r2.error = Some(err.to_string());
                            false
                        }
                    }
                })
            });
            if ok {
                let _ = cx.update(|cx| {
                    let _ = Bridge::r2_refresh(cx);
                });
            }
        })
        .detach();
        Ok(())
    }

    /// Download an object into the local cache and open it.
    pub fn r2_download_and_open(key: &str, cx: &mut App) -> BridgeResult<()> {
        let Some(bucket) = Self::state(cx).read(cx).r2.bucket.clone() else {
            return Err(BridgeError::message("Open a bucket first"));
        };
        let backend = Self::backend(cx)?;
        let key = key.to_string();
        Self::state(cx).update(cx, |s| {
            s.r2.busy = true;
            s.r2.error = None;
        });

        let handle = Self::state(cx);
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let target = key.clone();
            let rx = r2_dispatch(move || backend.r2_download_object(bucket, target));
            let result = r2_wait!(rx, &executor);
            let _ = cx.update(|cx| match result {
                Ok(path) => {
                    cx.open_with_system(Path::new(&path));
                    handle.update(cx, |s| {
                        s.r2.busy = false;
                        s.apply_ok_msg(format!("Downloaded {key}"));
                    });
                }
                Err(err) => {
                    handle.update(cx, |s| {
                        s.r2.busy = false;
                        s.r2.error = Some(err.to_string());
                    });
                }
            });
        })
        .detach();
        Ok(())
    }

    pub fn r2_presign_url(key: &str, expires_seconds: u64, cx: &mut App) -> BridgeResult<String> {
        let Some(bucket) = Self::state(cx).read(cx).r2.bucket.clone() else {
            return Err(BridgeError::message("Open a bucket first"));
        };
        let backend = Self::backend(cx)?;
        backend.r2_presign_url(bucket, key.to_string(), expires_seconds.clamp(1, 604_800))
    }

    // ── view state ─────────────────────────────────────────────────────────

    pub fn r2_select(key: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.r2.selected_key = key);
    }

    pub fn r2_set_search(query: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.r2.search = query.to_string());
    }

    pub fn r2_set_view_mode(mode: ExplorerViewMode, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.r2.view_mode = mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_names_follow_r2_rules() {
        assert!(is_valid_bucket_name("halo-files"));
        assert!(is_valid_bucket_name("abc123"));
        assert!(!is_valid_bucket_name("ab"));
        assert!(!is_valid_bucket_name("Halo"));
        assert!(!is_valid_bucket_name("-halo"));
        assert!(!is_valid_bucket_name("halo-"));
        assert!(!is_valid_bucket_name("halo--files"));
        assert!(!is_valid_bucket_name("halo.files"));
    }

    #[test]
    fn prefixes_are_canonicalized() {
        assert_eq!(normalize_prefix(""), "");
        assert_eq!(normalize_prefix("a/b"), "a/b/");
        assert_eq!(normalize_prefix("a/b/"), "a/b/");
        assert_eq!(normalize_prefix("/a/"), "a/");
    }

    #[test]
    fn breadcrumbs_walk_bucket_and_prefix() {
        let view = R2View {
            bucket: Some("halo".into()),
            prefix: "a/b/".into(),
            ..R2View::default()
        };
        assert_eq!(
            view.breadcrumbs(),
            vec![
                ("halo".to_string(), String::new()),
                ("a".to_string(), "a/".to_string()),
                ("b".to_string(), "a/b/".to_string()),
            ]
        );
    }
}
