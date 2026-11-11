//! Runtime probes: node, Pi, OpenCode, Antigravity, Cursor, Chrome.

use super::*;

pub(crate) fn probe_node_runtime() -> DoctorItem {
    let mut version = None;
    let mut resolved_path = None;
    let mut status = DoctorStatus::Missing;

    // `find_tool` also checks the usual package prefixes, so a Finder launch
    // with a minimal PATH does not report a healthy Node as missing.
    if let Some(path) = doctor_actions::find_tool("node") {
        resolved_path = Some(path.clone());
        if let Ok(output) = Command::new(&path).arg("--version").output() {
            if output.status.success() {
                let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !v.is_empty() {
                    version = Some(v);
                    status = DoctorStatus::Ready;
                }
            }
        }
    }

    DoctorItem {
        id: "runtime:node".to_string(),
        category: DoctorCategory::Harnesses,
        name: "Node.js & npm".to_string(),
        description: "JavaScript runtime required by Pi, Cursor, and JS language servers"
            .to_string(),
        command: "node".to_string(),
        status,
        version,
        resolved_path,
        hint: if status == DoctorStatus::Ready {
            "Node.js is available to the app.".to_string()
        } else {
            "Node.js >= 22.19 is required. Click Install Node (Homebrew), or install it from https://nodejs.org."
                .to_string()
        },
        can_auto_install: status != DoctorStatus::Ready,
        action_label: "Install Node".to_string(),
        doc_url: Some("https://nodejs.org".to_string()),
    }
}

pub(crate) fn probe_pi(backend: &dyn Backend, pi_custom: Option<&str>) -> DoctorItem {
    use halo_backend::commands::halo_agent::PiProbeStatus;
    let probe = backend
        .halo_agent_probe_pi(pi_custom)
        .unwrap_or_else(|e| PiProbeStatus::Failed {
            message: e.to_string(),
        });

    let (status, version, path, hint, can_auto_install, action_label) = match probe {
        PiProbeStatus::Ready { version, path } => (
            DoctorStatus::Ready,
            Some(version),
            Some(path),
            "Pi 1.4.0 is installed and ready.".to_string(),
            true,
            "Reinstall".to_string(),
        ),
        PiProbeStatus::WrongVersion { found, path } => (
            DoctorStatus::NeedsUpgrade,
            Some(found.clone()),
            Some(path),
            format!("Found {found}; Halo requires @earendil-works/pi-coding-agent 1.4.0."),
            true,
            "Upgrade".to_string(),
        ),
        PiProbeStatus::NodeMissing => (
            DoctorStatus::Missing,
            None,
            None,
            "Node.js >= 22.19 was not found, so Pi cannot start. Click Install Node to install it first."
                .to_string(),
            true,
            "Install Node".to_string(),
        ),
        PiProbeStatus::Missing { .. } => (
            DoctorStatus::Missing,
            None,
            None,
            "Pi coding agent 1.4.0 not found on PATH. Click Install to install via npm."
                .to_string(),
            true,
            "Install".to_string(),
        ),
        PiProbeStatus::Failed { message } => (
            DoctorStatus::Failed,
            None,
            None,
            message,
            true,
            "Retry".to_string(),
        ),
    };

    DoctorItem {
        id: "harness:pi".to_string(),
        category: DoctorCategory::Harnesses,
        name: "Pi Coding Agent".to_string(),
        description: "Official Pi 1.4.0 agent harness (JSON-L RPC subprocess)".to_string(),
        command: "pi".to_string(),
        status,
        version,
        resolved_path: path,
        hint,
        can_auto_install,
        action_label,
        doc_url: Some("https://github.com/earendil-works/pi".to_string()),
    }
}

pub(crate) fn probe_opencode(backend: &dyn Backend, oc_custom: Option<&str>) -> DoctorItem {
    use halo_backend::commands::halo_agent::OpenCodeProbeStatus;
    let probe = backend
        .halo_agent_probe_opencode(oc_custom)
        .unwrap_or_else(|e| OpenCodeProbeStatus::Failed {
            message: e.to_string(),
        });

    let (status, version, path, hint, can_auto_install, action_label) = match probe {
        OpenCodeProbeStatus::Ready { version, path } => (
            DoctorStatus::Ready,
            Some(version),
            Some(path),
            "OpenCode >= 2.2.0 is installed and ready.".to_string(),
            true,
            "Reinstall".to_string(),
        ),
        OpenCodeProbeStatus::WrongVersion {
            found,
            required,
            path,
        } => (
            DoctorStatus::NeedsUpgrade,
            Some(found.clone()),
            Some(path),
            format!("Found {found}; OpenCode >= {required} required."),
            true,
            "Upgrade".to_string(),
        ),
        OpenCodeProbeStatus::Missing => (
            DoctorStatus::Missing,
            None,
            None,
            "OpenCode >= 2.2.0 not found. Click Install to run official v2 curl installer."
                .to_string(),
            true,
            "Install".to_string(),
        ),
        OpenCodeProbeStatus::StateBlocked { message, path } => (
            DoctorStatus::Failed,
            None,
            Some(path),
            message,
            true,
            "Fix Ownership".to_string(),
        ),
        OpenCodeProbeStatus::Failed { message } => (
            DoctorStatus::Failed,
            None,
            None,
            message,
            true,
            "Retry".to_string(),
        ),
    };

    DoctorItem {
        id: "harness:opencode".to_string(),
        category: DoctorCategory::Harnesses,
        name: "OpenCode".to_string(),
        description: "OpenCode 2 coding agent harness (`opencode serve` HTTP/SSE)".to_string(),
        command: "opencode".to_string(),
        status,
        version,
        resolved_path: path,
        hint,
        can_auto_install,
        action_label,
        doc_url: Some("https://opencode.ai".to_string()),
    }
}

pub(crate) fn probe_antigravity(backend: &dyn Backend, ag_custom: Option<&str>) -> DoctorItem {
    use halo_backend::commands::halo_agent::AntigravityProbeStatus;
    let probe = backend
        .halo_agent_probe_antigravity(ag_custom)
        .unwrap_or_else(|e| AntigravityProbeStatus::Failed {
            message: e.to_string(),
        });

    let (status, version, path, hint, can_auto_install, action_label) = match probe {
        AntigravityProbeStatus::Ready { version, path } => (
            DoctorStatus::Ready,
            Some(version),
            Some(path),
            "Antigravity ACP server is installed and ready.".to_string(),
            true,
            "Reinstall".to_string(),
        ),
        AntigravityProbeStatus::Missing => (
            DoctorStatus::Missing,
            None,
            None,
            "Antigravity ACP server archive missing. Click Install to download from Google dl."
                .to_string(),
            true,
            "Install".to_string(),
        ),
        AntigravityProbeStatus::Failed { message } => (
            DoctorStatus::Failed,
            None,
            None,
            message,
            true,
            "Retry".to_string(),
        ),
    };

    DoctorItem {
        id: "harness:antigravity".to_string(),
        category: DoctorCategory::Harnesses,
        name: "Antigravity ACP".to_string(),
        description: "Google's official `agy_acp_server` (Agent Client Protocol stdio)".to_string(),
        command: "agy_acp_server".to_string(),
        status,
        version,
        resolved_path: path,
        hint,
        can_auto_install,
        action_label,
        doc_url: None,
    }
}

/// Cursor SDK harness — Node + pinned `@cursor/sdk` shim + user API key.
pub(crate) fn probe_cursor() -> DoctorItem {
    use halo_backend::commands::halo_agent::{
        cursor_auth_status, cursor_resolve_node, cursor_sdk_ready,
    };

    let node = cursor_resolve_node();
    let sdk = cursor_sdk_ready();
    let auth = cursor_auth_status();

    let (status, version, path, hint, can_auto_install, action_label) = if node.is_none() {
        (
            DoctorStatus::Missing,
            None,
            None,
            "Node.js is required for the Cursor harness. Click Install to install Node (if needed) and the pinned @cursor/sdk.".to_string(),
            true,
            "Install".to_string(),
        )
    } else if !sdk {
        (
            DoctorStatus::Missing,
            None,
            node.as_ref().map(|p| p.display().to_string()),
            "Pinned @cursor/sdk shim is not installed. Click Install to download it into Halo's adapters directory.".to_string(),
            true,
            "Install".to_string(),
        )
    } else if !auth.connected {
        (
            DoctorStatus::Failed,
            Some("@cursor/sdk".to_string()),
            node.as_ref().map(|p| p.display().to_string()),
            "Cursor SDK is installed, but no user API key is configured. Set CURSOR_API_KEY or paste a key from https://cursor.com/settings into Halo's cursor-state/auth.json, then Re-check All.".to_string(),
            false,
            "Needs API key".to_string(),
        )
    } else {
        let email = auth
            .email
            .filter(|e| !e.is_empty())
            .map(|e| format!(" ({e})"))
            .unwrap_or_default();
        (
            DoctorStatus::Ready,
            Some("@cursor/sdk".to_string()),
            node.as_ref().map(|p| p.display().to_string()),
            format!("Cursor harness is ready{email}."),
            true,
            "Reinstall".to_string(),
        )
    };

    DoctorItem {
        id: "harness:cursor".to_string(),
        category: DoctorCategory::Harnesses,
        name: "Cursor".to_string(),
        description: "Cursor agent harness via pinned @cursor/sdk Node shim (JSONL stdio)"
            .to_string(),
        command: "cursor-sdk".to_string(),
        status,
        version,
        resolved_path: path,
        hint,
        can_auto_install,
        action_label,
        doc_url: Some("https://cursor.com/docs/sdk/typescript".to_string()),
    }
}

pub(crate) fn probe_chrome() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        for candidate in candidates {
            if Path::new(candidate).exists() {
                return Some(candidate.to_string());
            }
        }
    }

    let binaries = [
        "google-chrome",
        "chromium",
        "google-chrome-stable",
        "chrome",
    ];
    for bin in binaries {
        if let Some(path) = halo_lsp::which(bin) {
            return Some(path);
        }
    }
    None
}
