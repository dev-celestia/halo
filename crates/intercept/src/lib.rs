pub mod cert;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use halo_core::tauri::{AppHandle, State};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub use halo_traffic::state::{
    InterceptMode, InterceptStatus, PausedRequest, ProxyRequest, ProxyResponse, ProxyState,
};

#[derive(Debug, Deserialize, Clone)]
pub struct InterceptForwardRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InterceptForwardResponse {
    pub status: u16,
    pub status_text: Option<String>,
    pub headers: HashMap<String, String>,
    pub body: String,
}

fn sync_content_length(
    headers: &mut HashMap<String, String>,
    body_len: usize,
    insert_if_missing: bool,
) {
    let content_length_key = headers
        .keys()
        .find(|key| key.eq_ignore_ascii_case("content-length"))
        .cloned();

    if let Some(content_length_key) = content_length_key {
        headers.insert(content_length_key, body_len.to_string());
    } else if insert_if_missing && body_len > 0 {
        headers.insert("content-length".to_string(), body_len.to_string());
    }

    if body_len > 0 {
        if let Some(transfer_encoding_key) = headers
            .keys()
            .find(|key| key.eq_ignore_ascii_case("transfer-encoding"))
            .cloned()
        {
            headers.remove(&transfer_encoding_key);
        }
    }
}

pub async fn get_intercept_status(
    proxy_state: State<'_, ProxyState>,
) -> Result<InterceptStatus, String> {
    Ok(proxy_state.get_status())
}

pub async fn set_intercept_enabled(
    proxy_state: State<'_, ProxyState>,
    enabled: bool,
) -> Result<InterceptStatus, String> {
    if enabled {
        proxy_state.set_mode(InterceptMode::Enabled);
    } else {
        proxy_state.set_mode(InterceptMode::Disabled);
    }

    Ok(proxy_state.get_status())
}

pub async fn set_intercept_scope(
    proxy_state: State<'_, ProxyState>,
    tab_id: String,
    capture_patterns: Vec<String>,
) -> Result<(), String> {
    proxy_state.set_intercept_scope(tab_id, capture_patterns);
    Ok(())
}

pub async fn get_paused_requests(
    proxy_state: State<'_, ProxyState>,
) -> Result<Vec<PausedRequest>, String> {
    Ok(proxy_state.get_all_paused())
}

pub fn forward_intercepted_request_state(
    proxy_state: &ProxyState,
    request_id: &str,
    request: Option<InterceptForwardRequest>,
    intercept_response: Option<bool>,
) -> Result<(), String> {
    let id = Uuid::parse_str(request_id).map_err(|e| e.to_string())?;
    let request = request.map(|request| {
        let mut body = request.body.into_bytes();

        if let Some(encoding) = request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
            .map(|(_, v)| v.clone())
        {
            if !encoding.is_empty() {
                match halo_traffic::utils::encode_body(&encoding, &body) {
                    Ok(encoded) => body = encoded,
                    Err(e) => eprintln!("[intercept] re-encode failed ({encoding}): {e}"),
                }
            }
        }

        let mut headers = request.headers;
        sync_content_length(&mut headers, body.len(), true);

        ProxyRequest {
            method: request.method,
            uri: request.url,
            http_version: "HTTP/1.1".to_string(),
            headers,
            body,
            content_decoded: false,
        }
    });
    let forwarded =
        proxy_state.forward_paused_request(&id, request, intercept_response.unwrap_or(false));

    if forwarded {
        Ok(())
    } else {
        Err("Paused request not found.".to_string())
    }
}

pub fn forward_intercepted_response_state(
    proxy_state: &ProxyState,
    request_id: &str,
    response: Option<InterceptForwardResponse>,
) -> Result<(), String> {
    let id = Uuid::parse_str(request_id).map_err(|e| e.to_string())?;
    let response = response.map(|response| {
        let mut body = response.body.into_bytes();

        if let Some(encoding) = response
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-encoding"))
            .map(|(_, v)| v.clone())
        {
            if !encoding.is_empty() {
                match halo_traffic::utils::encode_body(&encoding, &body) {
                    Ok(encoded) => body = encoded,
                    Err(e) => eprintln!("[intercept] response re-encode failed ({encoding}): {e}"),
                }
            }
        }

        let mut headers = response.headers;
        sync_content_length(&mut headers, body.len(), false);

        ProxyResponse {
            status_code: response.status,
            status_text: response.status_text.unwrap_or_default(),
            http_version: "HTTP/1.1".to_string(),
            headers,
            body,
            content_decoded: false,
        }
    });
    let forwarded = proxy_state.forward_paused_response(&id, response);

    if forwarded {
        Ok(())
    } else {
        Err("Paused response not found.".to_string())
    }
}

pub async fn forward_intercepted_request(
    proxy_state: State<'_, ProxyState>,
    request_id: String,
    request: Option<InterceptForwardRequest>,
    intercept_response: Option<bool>,
) -> Result<(), String> {
    forward_intercepted_request_state(&proxy_state, &request_id, request, intercept_response)
}

pub async fn forward_intercepted_response(
    proxy_state: State<'_, ProxyState>,
    request_id: String,
    response: Option<InterceptForwardResponse>,
) -> Result<(), String> {
    forward_intercepted_response_state(&proxy_state, &request_id, response)
}

pub async fn drop_intercepted_request(
    proxy_state: State<'_, ProxyState>,
    request_id: String,
) -> Result<(), String> {
    let id = Uuid::parse_str(&request_id).map_err(|e| e.to_string())?;
    let dropped = proxy_state.drop_paused_request(&id);

    if dropped {
        Ok(())
    } else {
        Err("Paused request not found.".to_string())
    }
}

pub async fn forward_intercepted_tab(
    proxy_state: State<'_, ProxyState>,
    tab_id: String,
) -> Result<usize, String> {
    Ok(proxy_state.forward_paused_by_tab(&tab_id))
}

pub async fn get_intercept_bypass_patterns(
    proxy_state: State<'_, ProxyState>,
) -> Result<Vec<String>, String> {
    Ok(proxy_state.get_bypass_patterns())
}

pub async fn set_intercept_bypass_patterns(
    proxy_state: State<'_, ProxyState>,
    patterns: Vec<String>,
) -> Result<Vec<String>, String> {
    proxy_state.set_bypass_patterns(patterns);
    Ok(proxy_state.get_bypass_patterns())
}

pub async fn add_intercept_bypass_pattern(
    proxy_state: State<'_, ProxyState>,
    pattern: String,
) -> Result<Vec<String>, String> {
    Ok(proxy_state.add_bypass_pattern(pattern))
}

pub async fn remove_intercept_bypass_pattern(
    proxy_state: State<'_, ProxyState>,
    pattern: String,
) -> Result<Vec<String>, String> {
    Ok(proxy_state.remove_bypass_pattern(&pattern))
}

pub fn browser_candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
            PathBuf::from(
                "/Applications/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            ),
            PathBuf::from(
                "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
            ),
        ]
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();

        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local_app_data).join("Chromium/Application/chrome.exe"));
        }

        if let Some(program_files) = std::env::var_os("PROGRAMFILES") {
            let base = PathBuf::from(program_files);
            candidates.push(base.join("Google/Chrome/Application/chrome.exe"));
            candidates.push(base.join("Chromium/Application/chrome.exe"));
        }

        if let Some(program_files_x86) = std::env::var_os("PROGRAMFILES(X86)") {
            candidates.push(
                PathBuf::from(program_files_x86).join("Google/Chrome/Application/chrome.exe"),
            );
        }

        return candidates;
    }

    #[cfg(target_os = "linux")]
    {
        return vec![
            PathBuf::from("chromium"),
            PathBuf::from("chromium-browser"),
            PathBuf::from("google-chrome"),
            PathBuf::from("google-chrome-stable"),
        ];
    }
}

fn intercept_browser_profile_dir(_app: &AppHandle) -> Result<PathBuf, String> {
    Ok(halo_core::paths::get_shared_app_dir().join("intercept-browser-profile"))
}

/// Remove volatile Chromium disk/GPU/code/service-worker cache directories while
/// preserving NSS certificate databases (`pki/nssdb`) and user preferences.
pub fn prune_chrome_profile_caches(profile_dir: &std::path::Path) {
    const VOLATILE_DIRS: &[&str] = &[
        "Default/Cache",
        "Default/Code Cache",
        "Default/GPUCache",
        "Default/DawnCache",
        "Default/Service Worker/CacheStorage",
        "Default/Service Worker/ScriptCache",
        "Default/Storage/ext",
        "ShaderCache",
        "GrShaderCache",
        "GraphiteDawnCache",
        "BrowserMetrics",
        "Crashpad",
    ];
    for rel in VOLATILE_DIRS {
        let p = profile_dir.join(rel);
        if p.exists() {
            let _ = std::fs::remove_dir_all(&p);
        }
    }
}

fn write_intercept_ca(_app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = halo_core::paths::get_shared_app_dir();
    std::fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;

    let ca_path = app_data_dir.join("halo-ca.pem");
    let ca_pem = halo_traffic::ca::export_ca_cert_pem().map_err(|error| format!("{error}"))?;
    std::fs::write(&ca_path, ca_pem).map_err(|e| e.to_string())?;

    Ok(ca_path)
}

#[allow(dead_code)]
fn certutil_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("certutil"),
        PathBuf::from("/opt/homebrew/opt/nss/bin/certutil"),
        PathBuf::from("/opt/homebrew/bin/certutil"),
        PathBuf::from("/usr/local/opt/nss/bin/certutil"),
        PathBuf::from("/usr/local/bin/certutil"),
    ]
}

#[allow(dead_code)]
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

pub async fn open_intercept_browser(app: AppHandle, proxy_port: u16) -> Result<(), String> {
    let profile_dir = intercept_browser_profile_dir(&app)?;
    std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
    prune_chrome_profile_caches(&profile_dir);

    #[cfg(target_os = "macos")]
    let ca_import_result = install_intercept_ca_to_macos_keychain(&app);

    #[cfg(not(target_os = "macos"))]
    let ca_import_result = import_intercept_ca_to_chrome_profile(&app);

    let mut last_error = None;
    let proxy_port = halo_traffic::active_proxy_port().unwrap_or(proxy_port);
    #[allow(unused_mut)]
    let mut args = vec![
        format!("--user-data-dir={}", profile_dir.display()),
        "--disk-cache-size=33554432".to_string(),
        "--media-cache-size=8388608".to_string(),
        "--disable-component-update".to_string(),
        "--new-window".to_string(),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--ignore-certificate-errors".to_string(),
        format!("--proxy-server=127.0.0.1:{proxy_port}"),
        "about:blank".to_string(),
    ];
    #[cfg(target_os = "macos")]
    args.push("--use-mock-keychain".to_string());

    for candidate in browser_candidates() {
        if candidate.components().count() > 1 && !candidate.exists() {
            continue;
        }

        match Command::new(&candidate).args(&args).spawn() {
            Ok(_) => {
                if let Err(error) = ca_import_result {
                    eprintln!("[intercept/browser] Browser opened, but CA import failed: {error}");
                }

                return Ok(());
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        "Google Chrome or Chromium was not found. Install Chrome or Chromium to use Open Browser."
            .to_string()
    }))
}

#[allow(dead_code)]
fn import_intercept_ca_to_chrome_profile(app: &AppHandle) -> Result<String, String> {
    let profile_dir = intercept_browser_profile_dir(app)?;
    std::fs::create_dir_all(&profile_dir).map_err(|e| e.to_string())?;
    let ca_path = write_intercept_ca(app)?;
    let ca_pem = std::fs::read_to_string(&ca_path).map_err(|e| e.to_string())?;
    let ca_fingerprint = format!("{:x}", Sha256::digest(ca_pem.as_bytes()));
    let import_marker_path = profile_dir.join("halo-ca.sha256");
    if std::fs::read_to_string(&import_marker_path)
        .map(|value| value.trim() == ca_fingerprint)
        .unwrap_or(false)
    {
        return Ok("Halo CA is already trusted in the managed Chrome profile.".to_string());
    }

    let db_dir = format!("sql:{}", profile_dir.display());
    let nicknames = [
        "Halo Proxy CA",
        // Pre-rebrand nicknames.
        "Hexbuffer Proxy CA",
        "Hexbuffer security Tools Root CA",
        "hexbuffer Security Tools Root CA",
        "Apprecon Security Tools Root CA",
    ];

    let init_args = vec![
        "-N".to_string(),
        "-d".to_string(),
        db_dir.clone(),
        "--empty-password".to_string(),
    ];
    let _ = run_certutil(&init_args);

    for nick in &nicknames {
        let delete_args = vec![
            "-D".to_string(),
            "-d".to_string(),
            db_dir.clone(),
            "-n".to_string(),
            nick.to_string(),
        ];
        let _ = run_certutil(&delete_args);
    }

    let add_args = vec![
        "-A".to_string(),
        "-d".to_string(),
        db_dir,
        "-n".to_string(),
        "Halo Proxy CA".to_string(),
        "-t".to_string(),
        "C,,".to_string(),
        "-i".to_string(),
        ca_path.display().to_string(),
    ];

    match run_certutil(&add_args) {
        Ok(()) => {
            std::fs::write(import_marker_path, ca_fingerprint).map_err(|e| e.to_string())?;
            Ok("Halo CA imported into the managed Chrome profile. Close old Intercept browser windows and open it again.".to_string())
        }
        Err(error) => Err(format!(
            "Chrome-profile CA import failed: {error}. Install NSS tools with `brew install nss`, then try again."
        )),
    }
}

#[cfg(target_os = "macos")]
fn user_login_keychain_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| "Could not resolve the current home directory.".to_string())?;
    Ok(PathBuf::from(home).join("Library/Keychains/login.keychain-db"))
}

#[cfg(target_os = "macos")]
fn run_security(args: &[String]) -> Result<(), String> {
    let output = Command::new("security")
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;

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

#[cfg(target_os = "macos")]
fn install_intercept_ca_to_macos_keychain(app: &AppHandle) -> Result<String, String> {
    let ca_path = write_intercept_ca(app)?;
    let keychain_path = user_login_keychain_path()?;
    let cert_names = [
        "Halo Proxy CA",
        // Pre-rebrand trust anchors.
        "Hexbuffer Proxy CA",
        "Hexbuffer Security Tools Root CA",
        "hexbuffer Security Tools Root CA",
        "Apprecon Security Tools Root CA",
        "Apprecon Proxy CA",
    ];

    // Clean up stale or duplicate certificates from the login keychain — but
    // only ones that are actually ours. Several of these common names belong to
    // other products, and their trust anchors must survive.
    let fingerprint = crate::cert::ca_sha256_hex();
    for cert_name in &cert_names {
        let Some(fingerprint) = fingerprint.as_deref() else {
            break;
        };
        for _ in 0..8 {
            if !crate::cert::keychain_holds_our_ca(cert_name, &keychain_path, fingerprint) {
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

pub async fn trust_intercept_ca(app: AppHandle) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        install_intercept_ca_to_macos_keychain(&app)
    }

    #[cfg(not(target_os = "macos"))]
    {
        import_intercept_ca_to_chrome_profile(&app)
    }
}
