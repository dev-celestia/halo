//! Cloudflare R2 object storage.
//!
//! The S3-compatible R2 API, driven from the desktop app's R2 Storage page.
//! The Secret Access Key lives in the OS keychain; only the account metadata
//! (`r2-settings.json`, `0600`) ever touches the filesystem.

use std::path::{Path, PathBuf};
use std::time::Duration;

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use halo_core::fs_secure;
use halo_core::paths::get_shared_app_dir;
use halo_core::tauri::Emitter;
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};

use crate::BackendRuntime;

const SETTINGS_FILE: &str = "r2-settings.json";
const CACHE_DIR: &str = "r2-cache";
const KEYRING_SERVICE: &str = "com.halo.r2";
const KEYRING_USER: &str = "default_secret";
const MULTIPART_CHUNK_SIZE: usize = 5 * 1024 * 1024; // S3 minimum part size
const MULTIPART_THRESHOLD: usize = 50 * 1024 * 1024;
const UPLOAD_PART_CONCURRENCY: usize = 3;
const UPLOAD_PROGRESS_EVENT: &str = "r2-upload-progress";
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";
const MAX_PRESIGN_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Account metadata. Never contains the Secret Access Key.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct R2Settings {
    pub account_id: String,
    pub access_key_id: String,
    pub custom_endpoint_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct R2CredentialsStatus {
    pub account_id: String,
    pub access_key_id: String,
    pub custom_endpoint_url: Option<String>,
    pub has_secret: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct R2FolderEntry {
    pub prefix: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct R2FileEntry {
    pub key: String,
    pub name: String,
    pub size: Option<i64>,
    pub last_modified_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct R2Listing {
    pub folders: Vec<R2FolderEntry>,
    pub files: Vec<R2FileEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct R2UploadProgress {
    pub file_name: String,
    pub progress: u32,
}

fn settings_path() -> PathBuf {
    get_shared_app_dir().join(SETTINGS_FILE)
}

fn cache_root() -> PathBuf {
    get_shared_app_dir().join(CACHE_DIR)
}

fn keyring_entry() -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

fn read_secret() -> Result<Option<String>, String> {
    match keyring_entry()?.get_password() {
        Ok(secret) if !secret.is_empty() => Ok(Some(secret)),
        Ok(_) => Ok(None),
        Err(KeyringError::NoEntry) => Ok(None),
        Err(err) => Err(format!("OS keychain error: {err}")),
    }
}

fn write_secret(secret: &str) -> Result<(), String> {
    keyring_entry()?
        .set_password(secret)
        .map_err(|e| format!("Failed to save the secret in the OS keychain: {e}"))
}

/// Strips empty, `.`, and `..` segments so remote-provided keys can never
/// escape the intended directory when they become filesystem paths.
pub fn sanitize_key_segments(input: &str) -> Vec<String> {
    input
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .map(str::to_string)
        .collect()
}

/// Local cache path for `bucket`/`key` under `root`, or `None` when either
/// name sanitizes to nothing.
pub fn cache_path(root: &Path, bucket: &str, key: &str) -> Option<PathBuf> {
    let mut segments = sanitize_key_segments(bucket);
    segments.extend(sanitize_key_segments(key));
    if segments.is_empty() {
        return None;
    }
    let mut path = root.to_path_buf();
    for segment in segments {
        path.push(segment);
    }
    Some(path)
}

pub fn parse_settings(raw: &str) -> Result<R2Settings, String> {
    let settings: R2Settings =
        serde_json::from_str(raw).map_err(|e| format!("Invalid R2 settings: {e}"))?;
    if settings.account_id.trim().is_empty() || settings.access_key_id.trim().is_empty() {
        return Err("R2 settings must include an account id and access key id".into());
    }
    Ok(settings)
}

fn read_settings() -> Result<R2Settings, String> {
    let path = settings_path();
    if !path.exists() {
        return Err("R2 credentials are not configured".into());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    parse_settings(&raw)
}

fn write_settings(settings: &R2Settings) -> Result<(), String> {
    let dir = get_shared_app_dir();
    fs_secure::create_private_dir(&dir).map_err(|e| e.to_string())?;
    let path = settings_path();
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs_secure::truncate_private(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    fs_secure::harden_file(&path);
    Ok(())
}

/// Build an S3 client for R2. The secret never leaves this process.
fn load_client() -> Result<Client, String> {
    let settings = read_settings()?;
    let secret =
        read_secret()?.ok_or_else(|| "R2 secret access key is not configured".to_string())?;

    let endpoint = match settings
        .custom_endpoint_url
        .as_deref()
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
    {
        Some(endpoint) => endpoint.trim_end_matches('/').to_string(),
        None => format!(
            "https://{}.r2.cloudflarestorage.com",
            settings.account_id.trim()
        ),
    };

    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("auto"))
        .endpoint_url(endpoint)
        .force_path_style(true)
        .credentials_provider(Credentials::new(
            settings.access_key_id.trim(),
            secret,
            None,
            None,
            "R2",
        ))
        .build();

    Ok(Client::from_conf(config))
}

fn object_name(key: &str) -> Option<String> {
    // A trailing separator marks a folder placeholder, not a file.
    if key.ends_with(['/', '\\']) {
        return None;
    }
    sanitize_key_segments(key).pop()
}

fn content_type_for_name(name: &str) -> &'static str {
    let ext = name
        .rsplit('.')
        .next()
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "xml" => "application/xml",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => DEFAULT_CONTENT_TYPE,
    }
}

// ── credentials ────────────────────────────────────────────────────────────

pub async fn credentials_status(
    _runtime: &BackendRuntime,
) -> Result<Option<R2CredentialsStatus>, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(None);
    }
    let settings = read_settings()?;
    Ok(Some(R2CredentialsStatus {
        account_id: settings.account_id,
        access_key_id: settings.access_key_id,
        custom_endpoint_url: settings.custom_endpoint_url,
        has_secret: read_secret()?.is_some(),
    }))
}

/// Save account metadata and (when provided) the secret.
///
/// An empty or absent secret keeps the stored keychain entry, so the settings
/// form can update metadata without asking the user to re-enter it.
pub async fn save_credentials(
    _runtime: &BackendRuntime,
    account_id: String,
    access_key_id: String,
    secret_access_key: Option<String>,
    custom_endpoint_url: Option<String>,
) -> Result<(), String> {
    let account_id = account_id.trim();
    let access_key_id = access_key_id.trim();
    if account_id.is_empty() || access_key_id.is_empty() {
        return Err("Account ID and Access Key ID must not be empty".into());
    }

    match secret_access_key.as_deref().map(str::trim) {
        Some(secret) if !secret.is_empty() => write_secret(secret)?,
        _ => {
            if read_secret()?.is_none() {
                return Err("Secret Access Key must not be empty on first setup".into());
            }
        }
    }

    write_settings(&R2Settings {
        account_id: account_id.to_string(),
        access_key_id: access_key_id.to_string(),
        custom_endpoint_url: custom_endpoint_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_string),
    })
}

pub async fn clear_credentials(_runtime: &BackendRuntime) -> Result<(), String> {
    let path = settings_path();
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    match keyring_entry()?.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(err) => Err(format!("Failed to delete the OS keychain entry: {err}")),
    }
}

// ── buckets & objects ──────────────────────────────────────────────────────

pub async fn list_buckets(_runtime: &BackendRuntime) -> Result<Vec<String>, String> {
    let client = load_client()?;
    let response = client
        .list_buckets()
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(response
        .buckets()
        .iter()
        .filter_map(|bucket| bucket.name().map(str::to_string))
        .collect())
}

pub async fn list_objects(
    _runtime: &BackendRuntime,
    bucket: String,
    prefix: String,
) -> Result<R2Listing, String> {
    let client = load_client()?;
    let response = client
        .list_objects_v2()
        .bucket(&bucket)
        .prefix(&prefix)
        .delimiter("/")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let folders = response
        .common_prefixes()
        .iter()
        .filter_map(|entry| {
            let prefix = entry.prefix()?;
            let name = prefix.trim_end_matches('/').rsplit('/').next()?;
            if name.is_empty() {
                return None;
            }
            Some(R2FolderEntry {
                prefix: prefix.to_string(),
                name: name.to_string(),
            })
        })
        .collect();

    let files = response
        .contents()
        .iter()
        .filter_map(|object| {
            let key = object.key()?;
            let name = object_name(key)?;
            Some(R2FileEntry {
                key: key.to_string(),
                name,
                size: object.size(),
                last_modified_ms: object.last_modified().map(|dt| {
                    dt.secs()
                        .saturating_mul(1000)
                        .saturating_add(i64::from(dt.subsec_nanos() / 1_000_000))
                }),
            })
        })
        .filter(|file| file.key != prefix)
        .collect();

    Ok(R2Listing { folders, files })
}

pub async fn create_bucket(_runtime: &BackendRuntime, name: String) -> Result<(), String> {
    let client = load_client()?;
    client
        .create_bucket()
        .bucket(&name)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn delete_bucket(_runtime: &BackendRuntime, name: String) -> Result<(), String> {
    let client = load_client()?;
    client
        .delete_bucket()
        .bucket(&name)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Folder placeholders are zero-byte objects whose key ends in `/`.
pub async fn create_folder(
    _runtime: &BackendRuntime,
    bucket: String,
    key: String,
) -> Result<(), String> {
    let client = load_client()?;
    client
        .put_object()
        .bucket(&bucket)
        .key(&key)
        .body(ByteStream::from(Vec::new()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn delete_object(
    _runtime: &BackendRuntime,
    bucket: String,
    key: String,
) -> Result<(), String> {
    let client = load_client()?;
    client
        .delete_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn emit_progress(runtime: &BackendRuntime, file_name: &str, progress: u32) {
    let _ = runtime.app.emit(
        UPLOAD_PROGRESS_EVENT,
        R2UploadProgress {
            file_name: file_name.to_string(),
            progress,
        },
    );
}

pub async fn upload_file(
    runtime: &BackendRuntime,
    bucket: String,
    key: String,
    source_path: String,
    content_type: Option<String>,
) -> Result<(), String> {
    let client = load_client()?;

    // The renderer composes the key (prefix + file name); enforce the same
    // sanitization here so a compromised UI cannot write outside the prefix.
    let safe_key = sanitize_key_segments(&key).join("/");
    if safe_key.is_empty() {
        return Err("Invalid object key".into());
    }
    let file_name = safe_key.rsplit('/').next().unwrap_or("file").to_string();
    let content_type = content_type
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| content_type_for_name(&file_name).to_string());

    let bytes = tokio::fs::read(&source_path)
        .await
        .map_err(|e| format!("Failed to read source file: {e}"))?;

    if bytes.len() <= MULTIPART_THRESHOLD {
        emit_progress(runtime, &file_name, 10);
        client
            .put_object()
            .bucket(&bucket)
            .key(&safe_key)
            .content_type(content_type)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        emit_progress(runtime, &file_name, 100);
        return Ok(());
    }

    let upload = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(&safe_key)
        .content_type(content_type)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let upload_id = upload
        .upload_id()
        .ok_or_else(|| "Multipart upload failed to initialize".to_string())?
        .to_string();

    let total = bytes.len();
    let part_count = total.div_ceil(MULTIPART_CHUNK_SIZE);
    let mut parts: Vec<CompletedPart> = Vec::with_capacity(part_count);

    for batch_start in (0..part_count).step_by(UPLOAD_PART_CONCURRENCY) {
        let batch_end = (batch_start + UPLOAD_PART_CONCURRENCY).min(part_count);
        let mut tasks = tokio::task::JoinSet::new();

        for index in batch_start..batch_end {
            let start = index * MULTIPART_CHUNK_SIZE;
            let end = (start + MULTIPART_CHUNK_SIZE).min(total);
            let body = bytes[start..end].to_vec();
            let number = (index + 1) as i32;
            let client = client.clone();
            let bucket = bucket.clone();
            let key = safe_key.clone();
            let upload_id = upload_id.clone();
            tasks.spawn(async move {
                let output = client
                    .upload_part()
                    .bucket(&bucket)
                    .key(&key)
                    .upload_id(&upload_id)
                    .part_number(number)
                    .body(ByteStream::from(body))
                    .send()
                    .await
                    .map_err(|e| format!("Part {number} failed: {e}"))?;
                let etag = output
                    .e_tag()
                    .ok_or_else(|| format!("Part {number} returned no ETag"))?
                    .to_string();
                Ok::<(i32, String), String>((number, etag))
            });
        }

        while let Some(result) = tasks.join_next().await {
            let (number, etag) = result.map_err(|e| format!("Upload part task failed: {e}"))??;
            parts.push(
                CompletedPart::builder()
                    .part_number(number)
                    .e_tag(etag)
                    .build(),
            );
        }

        emit_progress(runtime, &file_name, (batch_end * 100 / part_count) as u32);
    }

    parts.sort_by_key(|part| part.part_number().unwrap_or(0));
    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(&safe_key)
        .upload_id(&upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build(),
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;

    emit_progress(runtime, &file_name, 100);
    Ok(())
}

/// Stream an object into the local cache and return the cached path.
/// Cached files are returned as-is.
pub async fn download_object(
    _runtime: &BackendRuntime,
    bucket: String,
    key: String,
) -> Result<String, String> {
    let client = load_client()?;
    let local_path =
        cache_path(&cache_root(), &bucket, &key).ok_or_else(|| "Invalid object key".to_string())?;

    if tokio::fs::try_exists(&local_path)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(local_path.to_string_lossy().to_string());
    }

    let output = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let bytes = output
        .body
        .collect()
        .await
        .map_err(|e| e.to_string())?
        .into_bytes();

    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&local_path, bytes)
        .await
        .map_err(|e| e.to_string())?;

    Ok(local_path.to_string_lossy().to_string())
}

pub async fn presign_url(
    _runtime: &BackendRuntime,
    bucket: String,
    key: String,
    expires_seconds: u64,
) -> Result<String, String> {
    let client = load_client()?;
    let expires = Duration::from_secs(expires_seconds.clamp(1, MAX_PRESIGN_SECONDS));
    let config = PresigningConfig::expires_in(expires).map_err(|e| e.to_string())?;
    let presigned = client
        .get_object()
        .bucket(&bucket)
        .key(&key)
        .presigned(config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(presigned.uri().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_both_separators_and_traversal() {
        assert_eq!(
            sanitize_key_segments("a/../../b\\c/./d"),
            vec!["a", "b", "c", "d"]
        );
        assert!(sanitize_key_segments("../..").is_empty());
        assert!(sanitize_key_segments("/").is_empty());
    }

    #[test]
    fn cache_path_never_escapes_its_root() {
        let root = Path::new("/tmp/r2-cache");
        let path = cache_path(root, "bucket", "../../etc/passwd").expect("path");
        assert_eq!(path, Path::new("/tmp/r2-cache/bucket/etc/passwd"));
        assert!(cache_path(root, "..", "../").is_none());
    }

    #[test]
    fn settings_require_account_and_key() {
        assert!(parse_settings("{}").is_err());
        assert!(parse_settings(r#"{"accountId":"","accessKeyId":"key"}"#).is_err());
        let parsed =
            parse_settings(r#"{"accountId":"acct","accessKeyId":"key","customEndpointUrl":null}"#)
                .expect("valid settings");
        assert_eq!(parsed.account_id, "acct");
        assert!(parsed.custom_endpoint_url.is_none());
    }

    #[test]
    fn object_names_strip_prefixes() {
        assert_eq!(
            object_name("nested/dir/file.txt").as_deref(),
            Some("file.txt")
        );
        assert!(object_name("dir/").is_none());
    }

    #[test]
    fn content_type_defaults_for_unknown_extensions() {
        assert_eq!(content_type_for_name("data.json"), "application/json");
        assert_eq!(content_type_for_name("blob"), DEFAULT_CONTENT_TYPE);
    }
}
