//! Formatting, whitespace trimming, and shell-command helpers.

pub(crate) fn coder_trim_trailing_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let _ = i;
        if let Some(stripped) = line.strip_suffix('\n') {
            out.push_str(stripped.trim_end());
            out.push('\n');
        } else {
            out.push_str(line.trim_end());
        }
    }
    out
}

/// Format `text` with Prettier when available (`node_modules/.bin/prettier` or PATH).
pub(crate) fn coder_format_with_prettier(root: &str, path: &str, text: &str) -> Option<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let root = std::path::Path::new(root);
    let local = root.join("node_modules").join(".bin").join("prettier");
    let prettier = if local.is_file() {
        local
    } else {
        std::path::PathBuf::from("prettier")
    };

    let mut child = Command::new(&prettier)
        .args(["--stdin-filepath", path])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    {
        let mut stdin = child.stdin.take()?;
        stdin.write_all(text.as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let formatted = String::from_utf8(output.stdout).ok()?;
    if formatted.is_empty() && !text.is_empty() {
        None
    } else {
        Some(formatted)
    }
}

/// Package manager implied by lockfiles under `root`.
pub(crate) fn coder_node_package_manager(root: &str) -> &'static str {
    let root = std::path::Path::new(root);
    if root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if root.join("yarn.lock").is_file() {
        "yarn"
    } else {
        "npm"
    }
}

/// Script names from `package.json` `scripts`, insertion order preserved.
pub(crate) fn coder_list_node_scripts(root: &str) -> Vec<String> {
    let path = std::path::Path::new(root).join("package.json");
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) else {
        return Vec::new();
    };
    scripts.keys().cloned().collect()
}

/// Script to run for a Node project: `dev`, else `start`, else first script in package.json.
pub(crate) fn coder_node_run_script(root: &str) -> Option<String> {
    let scripts = coder_list_node_scripts(root);
    if scripts.iter().any(|s| s == "dev") {
        return Some("dev".into());
    }
    if scripts.iter().any(|s| s == "start") {
        return Some("start".into());
    }
    scripts.into_iter().next()
}

/// Build script for a Node project: only `build` when present.
pub(crate) fn coder_node_build_script(root: &str) -> Option<String> {
    coder_list_node_scripts(root)
        .into_iter()
        .find(|s| s == "build")
}

/// Format a Node package-manager invocation for `script`.
pub(crate) fn coder_node_script_command(root: &str, script: &str) -> String {
    match coder_node_package_manager(root) {
        "pnpm" => format!("pnpm run {script}"),
        "yarn" => format!("yarn {script}"),
        _ => format!("npm run {script}"),
    }
}

fn coder_prefer_node(
    _root: &str,
    kind: crate::work::CoderProjectKind,
    focused_path: Option<&str>,
    node_available: bool,
) -> bool {
    let ext = focused_path
        .and_then(|p| std::path::Path::new(p).extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_js_ts = matches!(ext.as_str(), "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx");
    let is_rs = ext == "rs";

    match kind {
        crate::work::CoderProjectKind::Node => true,
        crate::work::CoderProjectKind::Rust => false,
        crate::work::CoderProjectKind::Both => {
            if is_js_ts {
                true
            } else if is_rs {
                false
            } else {
                // No focused language file — prefer Node when both exist if a
                // relevant script is available, else Rust.
                node_available
            }
        }
    }
}

/// Shell command for the Run button given project kind and focused file path.
pub(crate) fn coder_run_shell_command(
    root: &str,
    kind: crate::work::CoderProjectKind,
    focused_path: Option<&str>,
) -> Option<String> {
    let prefer_node =
        coder_prefer_node(root, kind, focused_path, coder_node_run_script(root).is_some());

    if prefer_node && kind.has_node() {
        let script = coder_node_run_script(root)?;
        Some(coder_node_script_command(root, &script))
    } else if kind.has_rust() {
        Some("cargo run".into())
    } else {
        None
    }
}

/// Shell command for the Build button given project kind and focused file path.
pub(crate) fn coder_build_shell_command(
    root: &str,
    kind: crate::work::CoderProjectKind,
    focused_path: Option<&str>,
) -> Option<String> {
    let prefer_node =
        coder_prefer_node(root, kind, focused_path, coder_node_build_script(root).is_some());

    if prefer_node && kind.has_node() {
        let script = coder_node_build_script(root)?;
        Some(coder_node_script_command(root, &script))
    } else if kind.has_rust() {
        Some("cargo build".into())
    } else {
        None
    }
}

/// Raster / icon / SVG formats opened as an image preview instead of the text editor.
pub(crate) fn coder_is_image_path(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" | "avif" | "tif" | "tiff"
            | "svg"
    )
}
