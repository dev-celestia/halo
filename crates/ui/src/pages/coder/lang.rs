//! Rust and JavaScript/TypeScript marks for the Coder surfaces.
//!
//! Icons reuse the colored explorer SVGs. Toolchain choice here is display
//! only and follows the same focused-file rule as run-command selection.

use std::path::Path;

use gpui::{IntoElement, Pixels};
use halo_bridge::CoderProjectKind;

/// Cargo commands offered as run presets. Clicking one sets the run command.
pub(super) const CARGO_RUN_PRESETS: &[&str] =
    &["cargo run", "cargo test", "cargo check", "cargo clippy"];

/// Cargo command offered as the build preset.
pub(super) const CARGO_BUILD_PRESETS: &[&str] = &["cargo build"];

/// Files the empty editor can open when they exist in the project root.
const QUICK_FILES: &[&str] = &[
    "Cargo.toml",
    "src/main.rs",
    "src/lib.rs",
    "package.json",
    "src/App.tsx",
    "src/App.jsx",
    "src/main.tsx",
    "app/page.tsx",
];

/// Package scripts shown before the rest, in this order.
const SCRIPT_FIRST: &[&str] = &["dev", "start", "build", "test", "lint", "typecheck"];

/// Colored file mark at `size`. The name is a filename, not a full path.
pub(super) fn file_mark(name: &str, size: Pixels) -> impl IntoElement {
    crate::explorer_icons::sized_icon(crate::explorer_icons::file_icon_path(name), size)
}

pub(super) fn path_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn is_js_ts_ext(ext: &str) -> bool {
    matches!(ext, "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx")
}

/// True when Run would follow Node for this project and focused file.
///
/// Matches `coder_prefer_node`: a mixed project follows `.rs` vs JS/TS, and
/// with neither focused it follows whether a package script exists.
pub(super) fn prefer_node(
    kind: CoderProjectKind,
    focused_path: Option<&str>,
    node_available: bool,
) -> bool {
    let ext = focused_path.map(path_extension).unwrap_or_default();
    match kind {
        CoderProjectKind::Node => true,
        CoderProjectKind::Rust => false,
        CoderProjectKind::Both => {
            if is_js_ts_ext(&ext) {
                true
            } else if ext == "rs" {
                false
            } else {
                node_available
            }
        }
    }
}

/// Filename whose explorer icon represents the JS/TS side of the project.
pub(super) fn node_mark_name(focused: Option<&str>) -> &'static str {
    match focused.map(path_extension).as_deref() {
        Some("jsx") => "App.jsx",
        Some("tsx") => "App.tsx",
        Some("ts") => "file.ts",
        Some("js" | "mjs" | "cjs") => "file.js",
        _ => "package.json",
    }
}

/// Short label for the JS/TS toolchain, from the focused file.
pub(super) fn node_toolchain_label(focused: Option<&str>) -> &'static str {
    match focused.map(path_extension).as_deref() {
        Some("jsx" | "tsx") => "React",
        Some("ts") => "TypeScript",
        Some("js" | "mjs" | "cjs") => "JavaScript",
        _ => "Node",
    }
}

/// `dev` / `start` / `build` / `test` / `lint` / `typecheck`, then the rest.
pub(super) fn prioritize_scripts(scripts: &[String]) -> Vec<String> {
    let mut ordered = Vec::with_capacity(scripts.len());
    for name in SCRIPT_FIRST {
        if scripts.iter().any(|script| script == name) {
            ordered.push((*name).to_string());
        }
    }
    for script in scripts {
        if !SCRIPT_FIRST.iter().any(|name| script == name) {
            ordered.push(script.clone());
        }
    }
    ordered
}

/// `(relative label, absolute path)` for quick files that exist under `root`.
pub(super) fn existing_quick_files(root: &str) -> Vec<(String, String)> {
    let root_path = Path::new(root);
    QUICK_FILES
        .iter()
        .filter_map(|rel| {
            let abs = root_path.join(rel);
            abs.is_file()
                .then(|| ((*rel).to_string(), abs.to_string_lossy().into_owned()))
        })
        .collect()
}
