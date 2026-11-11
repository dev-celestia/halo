//! App-bundle and instance-lock probes.

use super::*;

pub(crate) fn probe_app_bundle() -> DoctorItem {
    let mut resolved_path = None;
    let mut version = None;
    let mut status = DoctorStatus::Ready;
    let mut can_auto_install = false;
    let mut action_label = "Healthy".to_string();
    let mut hint = "Running in development binary mode.".to_string();

    #[cfg(target_os = "macos")]
    {
        let candidate_paths = [
            std::env::current_exe().ok().and_then(|p| {
                p.ancestors()
                    .find(|a| a.extension().map_or(false, |ext| ext == "app"))
                    .map(|p| p.to_path_buf())
            }),
            Some(PathBuf::from("/Applications/Halo.app")),
            Some(PathBuf::from("dist/Halo.app")),
        ];

        let found_app = candidate_paths.into_iter().flatten().find(|p| p.is_dir());
        if let Some(app_path) = found_app {
            resolved_path = Some(app_path.display().to_string());
            let out = Command::new("du")
                .args(["-sk", &app_path.display().to_string()])
                .output();
            let kb = out
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8_lossy(&o.stdout)
                            .split_whitespace()
                            .next()
                            .and_then(|s| s.parse::<u64>().ok())
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            let mb = kb as f64 / 1024.0;
            version = Some(format!("{mb:.1} MB"));

            let lsp_res = app_path.join("Contents/Resources/lsp");
            if lsp_res.is_dir() && mb > 100.0 {
                status = DoctorStatus::NeedsUpgrade;
                hint = format!(
                    "Bundle is {mb:.1} MB with offline language servers. Click Slim Bundle to shrink to ~60 MB."
                );
                can_auto_install = true;
                action_label = "Slim Bundle".to_string();
            } else {
                status = DoctorStatus::Ready;
                hint = format!("Bundle size is {mb:.1} MB (slim mode). Resolves tools from PATH.");
                can_auto_install = true;
                action_label = "Re-sign".to_string();
            }
        }
    }

    DoctorItem {
        id: "system:app-bundle".to_string(),
        category: DoctorCategory::System,
        name: "Application Bundle".to_string(),
        description: "macOS application bundle size, embedded payload, and code signature"
            .to_string(),
        command: "codesign".to_string(),
        status,
        version,
        resolved_path,
        hint,
        can_auto_install,
        action_label,
        doc_url: None,
    }
}

pub(crate) fn probe_instance_locks() -> DoctorItem {
    let mut status = DoctorStatus::Ready;
    let mut stale_count = 0usize;
    let mut foreign_count = 0usize;
    let mut unwritable: Option<PathBuf> = None;
    let mut resolved_path = None;
    let mut hint = "Single-instance socket and runtime cache are healthy.".to_string();

    for root in doctor_actions::brand_roots() {
        if !root.is_dir() {
            continue;
        }
        resolved_path.get_or_insert_with(|| root.display().to_string());
        foreign_count += doctor_actions::foreign_paths(&root).len();
        if unwritable.is_none() && !doctor_actions::dir_writable(&root) {
            unwritable = Some(root.clone());
        }
    }

    for dir in doctor_actions::run_dirs() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "sock")
                    && !doctor_actions::socket_is_live(&path)
                {
                    stale_count += 1;
                }
            }
        }
    }

    if let Some(dir) = &unwritable {
        status = DoctorStatus::Failed;
        hint = format!(
            "{} is owned by another user (root), so the app cannot write its instance lock or caches. \
             Click Reclaim Cache — if it cannot take the tree back it will hand you the exact command.",
            dir.display()
        );
    } else if stale_count > 0 || foreign_count > 0 {
        status = DoctorStatus::NeedsUpgrade;
        let mut parts: Vec<String> = Vec::new();
        if stale_count > 0 {
            parts.push(format!("{stale_count} stale socket(s)"));
        }
        if foreign_count > 0 {
            parts.push(format!("{foreign_count} entr(ies) owned by root"));
        }
        hint = format!(
            "{}. Click Reclaim Cache to tidy the app cache tree.",
            parts.join(" and ")
        );
    }

    let version = if unwritable.is_some() {
        Some("Root-owned".to_string())
    } else if stale_count > 0 {
        Some(format!("{stale_count} stale"))
    } else if foreign_count > 0 {
        Some(format!("{foreign_count} foreign"))
    } else {
        Some("Active".to_string())
    };

    DoctorItem {
        id: "system:instance-locks".to_string(),
        category: DoctorCategory::System,
        name: "App Cache & IPC Sockets".to_string(),
        description: "Instance lock, browser cache and LSP state under ~/Library/Caches"
            .to_string(),
        command: "lsof".to_string(),
        status,
        version,
        resolved_path,
        hint,
        can_auto_install: status != DoctorStatus::Ready,
        action_label: "Reclaim Cache".to_string(),
        doc_url: None,
    }
}
