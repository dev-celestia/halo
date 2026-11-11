//! Aggregate dependency probe: node, agent desktop, runtime, CA, bundle, storage.

use super::*;

pub(crate) fn probe_all_dependencies(
    backend: &dyn Backend,
    pi_custom: Option<&str>,
    oc_custom: Option<&str>,
    ag_custom: Option<&str>,
) -> Vec<DoctorItem> {
    let mut items = Vec::new();

    // ── 1. System & Bundle ────────────────────────────────────────────────────
    items.push(probe_app_bundle());
    items.push(probe_instance_locks());
    items.push(probe_orphan_processes());
    items.push(probe_storage_health(backend));
    // OpenCode's own state directory: a root-owned `log/opencode.log` makes
    // every non-root `opencode` call exit 1 at startup.
    items.push(crate::bridge::agent_state::probe());

    // ── 2. AI Harnesses ───────────────────────────────────────────────────────
    // Node runtime
    let node_probe = probe_node_runtime();
    items.push(node_probe);

    // Pi Coding Agent
    let pi_probe = probe_pi(backend, pi_custom);
    items.push(pi_probe);

    // OpenCode
    let oc_probe = probe_opencode(backend, oc_custom);
    items.push(oc_probe);

    // Antigravity ACP
    let ag_probe = probe_antigravity(backend, ag_custom);
    items.push(ag_probe);

    // Cursor SDK harness
    items.push(probe_cursor());

    // ── 2. Language Servers ───────────────────────────────────────────────────
    for entry in halo_lsp::catalog() {
        let (status, path) = match halo_lsp::which(entry.command) {
            Some(path) => (DoctorStatus::Ready, Some(path)),
            None => (DoctorStatus::Missing, None),
        };
        items.push(DoctorItem {
            id: format!("lsp:{}", entry.id),
            category: DoctorCategory::LanguageServers,
            name: entry.display_name.to_string(),
            description: format!("Language server for {}", entry.language_labels),
            command: entry.command.to_string(),
            status,
            version: None,
            resolved_path: path,
            hint: entry.install_hint.to_string(),
            can_auto_install: !entry.install_commands.is_empty(),
            action_label: if status == DoctorStatus::Ready {
                "Reinstall".into()
            } else {
                "Install".into()
            },
            doc_url: None,
        });
    }

    // ShellCheck linter
    let (sc_status, sc_path) = match halo_lsp::which("shellcheck") {
        Some(path) => (DoctorStatus::Ready, Some(path)),
        None => (DoctorStatus::Missing, None),
    };
    items.push(DoctorItem {
        id: "lsp:shellcheck".to_string(),
        category: DoctorCategory::LanguageServers,
        name: "ShellCheck".to_string(),
        description: "Static analysis and linter for shell scripts (used by bash-lsp)".to_string(),
        command: "shellcheck".to_string(),
        status: sc_status,
        version: None,
        resolved_path: sc_path,
        hint: "brew install shellcheck (macOS) or package manager".to_string(),
        can_auto_install: cfg!(target_os = "macos"),
        action_label: if sc_status == DoctorStatus::Ready {
            "Reinstall".into()
        } else {
            "Install".into()
        },
        doc_url: Some("https://www.shellcheck.net".to_string()),
    });

    // ── 3. Browser & Proxy ────────────────────────────────────────────────────
    // agent-browser
    let browser_status = backend.halo_agent_browser_status().unwrap_or_default();
    let ab_status = if browser_status.available {
        DoctorStatus::Ready
    } else {
        DoctorStatus::Missing
    };
    items.push(DoctorItem {
        id: "browser:agent-browser".to_string(),
        category: DoctorCategory::BrowserProxy,
        name: "Agent Browser".to_string(),
        description: "Headless Chrome driving engine for Halo Agent actions".to_string(),
        command: "agent-browser".to_string(),
        status: ab_status,
        version: browser_status.version,
        resolved_path: browser_status.binary,
        hint: "npm install -g agent-browser && agent-browser install".to_string(),
        can_auto_install: true,
        action_label: if ab_status == DoctorStatus::Ready {
            "Reinstall".into()
        } else {
            "Install".into()
        },
        doc_url: Some("https://agent-browser.dev".to_string()),
    });

    // agent-desktop (https://agent-browser.dev/desktop)
    let (agent_desktop_item, agent_desktop_permissions) = probe_agent_desktop();
    items.push(agent_desktop_item);
    if let Some(permissions) = agent_desktop_permissions {
        items.push(permissions);
    }

    // halo-browser CLI
    let halo_browser_path = browser_status.cli.clone();
    let hb_status = if halo_browser_path.is_some() {
        DoctorStatus::Ready
    } else {
        DoctorStatus::Missing
    };
    items.push(DoctorItem {
        id: "browser:halo-browser".to_string(),
        category: DoctorCategory::BrowserProxy,
        name: "Halo Browser CLI".to_string(),
        description: "App-bundled bridge binary between agents and browser sessions".to_string(),
        command: "halo-browser".to_string(),
        status: hb_status,
        version: None,
        resolved_path: halo_browser_path,
        hint: "Bundled beside the Halo executable; a dev checkout can rebuild it in place."
            .to_string(),
        can_auto_install: hb_status != DoctorStatus::Ready,
        action_label: "Rebuild CLI".to_string(),
        doc_url: None,
    });

    // Google Chrome / Chromium
    let chrome_path = probe_chrome();
    let ch_status = if chrome_path.is_some() {
        DoctorStatus::Ready
    } else {
        DoctorStatus::Missing
    };
    items.push(DoctorItem {
        id: "browser:chrome".to_string(),
        category: DoctorCategory::BrowserProxy,
        name: "Google Chrome / Chromium".to_string(),
        description: "Browser binary used for live crawler sessions and managed Open Browser"
            .to_string(),
        command: "google-chrome".to_string(),
        status: ch_status,
        version: None,
        resolved_path: chrome_path,
        hint: "Chrome, Chromium, Brave or Edge. Halo cannot install a browser silently — Get Chrome opens the download page."
            .to_string(),
        can_auto_install: ch_status != DoctorStatus::Ready,
        action_label: "Get Chrome".to_string(),
        doc_url: Some("https://google.com/chrome".to_string()),
    });

    // MITM Root CA
    let ca_item = probe_mitm_ca(backend);
    items.push(ca_item);

    items
}
