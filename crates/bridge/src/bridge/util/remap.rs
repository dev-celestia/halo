//! Path remapping when coder files move or are deleted.

use super::*;

pub(crate) enum CoderToggleAction {
    Collapse,
    Load,
}

pub(crate) fn coder_remap_path_string(path: &mut String, from: &str, to: &str, prefix: &str) {
    if path.as_str() == from {
        *path = to.to_string();
    } else if path.starts_with(prefix) {
        *path = format!("{to}{}", &path[from.len()..]);
    }
}

pub(crate) fn coder_remap_open_paths(coder: &mut CoderWork, from: &str, to: &str) {
    let prefix = format!("{from}{}", std::path::MAIN_SEPARATOR);
    for file in &mut coder.open_files {
        coder_remap_path_string(&mut file.path, from, to, &prefix);
    }
    for pane in &mut coder.panes {
        for tab in &mut pane.tabs {
            coder_remap_path_string(tab, from, to, &prefix);
        }
        if let Some(active) = pane.active.as_mut() {
            coder_remap_path_string(active, from, to, &prefix);
        }
    }
    for p in &mut coder.expanded {
        coder_remap_path_string(p, from, to, &prefix);
    }
}

pub(crate) fn coder_close_paths_under(coder: &mut CoderWork, path: &str) {
    let prefix = format!("{path}{}", std::path::MAIN_SEPARATOR);
    let under = |p: &str| p == path || p.starts_with(&prefix);
    coder.open_files.retain(|f| !under(&f.path));
    coder.expanded.retain(|p| !under(p));

    let mut empty_panes: Vec<u8> = Vec::new();
    for pane in &mut coder.panes {
        pane.tabs.retain(|p| !under(p));
        if pane.active.as_ref().is_some_and(|a| under(a)) {
            pane.active = pane.tabs.last().cloned();
        }
        if pane.tabs.is_empty() && pane.id != coder.layout.first_leaf() {
            empty_panes.push(pane.id);
        }
    }
    for id in empty_panes {
        let _ = coder.close_pane(id);
    }
    // If the focused pane vanished, fall back to the first leaf.
    if coder.pane(coder.focused_pane).is_none() {
        coder.focused_pane = coder.layout.first_leaf();
    }
}
