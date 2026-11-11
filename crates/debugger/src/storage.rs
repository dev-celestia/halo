//! Prefs / storage tree helpers for the Storage tab.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Known prefs / settings filenames under the app data dir.
const PREFS_FILES: &[&str] = &[
    "shell.json",
    "coder.json",
    "ai-settings.json",
    "r2-settings.json",
];

#[derive(Clone, Debug)]
pub struct PrefsNode {
    pub key: String,
    pub path: String,
    pub value_preview: String,
    pub children: Vec<PrefsNode>,
    pub is_leaf: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PrefsTree {
    pub roots: Vec<PrefsNode>,
    pub errors: Vec<String>,
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// Load a read-only tree of known prefs JSON files under `app_data_dir`.
pub fn load_prefs_tree(app_data_dir: &str) -> PrefsTree {
    let root = PathBuf::from(app_data_dir);
    let mut tree = PrefsTree::default();

    for name in PREFS_FILES {
        let path = root.join(name);
        match read_json_file(&path) {
            Ok(value) => {
                let node = value_to_node(name, name, &value);
                tree.roots.push(node);
            }
            Err(err) => {
                if path.exists() {
                    tree.errors.push(format!("{name}: {err}"));
                }
            }
        }
    }

    // Also expose log file path as a leaf marker if present.
    let log_path = root.join("logs").join("halo.log");
    if log_path.exists() {
        let size = fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
        tree.roots.push(PrefsNode {
            key: "logs/halo.log".into(),
            path: "logs/halo.log".into(),
            value_preview: format_bytes(size),
            children: Vec::new(),
            is_leaf: true,
        });
    }

    tree
}

fn read_json_file(path: &Path) -> Result<Value, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn value_to_node(key: &str, path: &str, value: &Value) -> PrefsNode {
    match value {
        Value::Object(map) => {
            let mut children = Vec::with_capacity(map.len());
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            for k in keys {
                let child_path = format!("{path}/{k}");
                children.push(value_to_node(&k, &child_path, &map[&k]));
            }
            PrefsNode {
                key: key.into(),
                path: path.into(),
                value_preview: format!("{{{} keys}}", children.len()),
                children,
                is_leaf: false,
            }
        }
        Value::Array(arr) => {
            let mut children = Vec::with_capacity(arr.len().min(50));
            for (i, item) in arr.iter().take(50).enumerate() {
                let child_path = format!("{path}[{i}]");
                children.push(value_to_node(&i.to_string(), &child_path, item));
            }
            let preview = if arr.len() > 50 {
                format!("[{} items, showing 50]", arr.len())
            } else {
                format!("[{} items]", arr.len())
            };
            PrefsNode {
                key: key.into(),
                path: path.into(),
                value_preview: preview,
                children,
                is_leaf: false,
            }
        }
        other => PrefsNode {
            key: key.into(),
            path: path.into(),
            value_preview: preview_scalar(other),
            children: Vec::new(),
            is_leaf: true,
        },
    }
}

fn preview_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.len() > 120 {
                format!("\"{}…\"", &s[..117])
            } else {
                format!("\"{s}\"")
            }
        }
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

/// Tail the last `max_bytes` of the app log file, if present.
pub fn tail_log_file(app_data_dir: &str, max_bytes: u64) -> Result<String, String> {
    let path = PathBuf::from(app_data_dir).join("logs").join("halo.log");
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    let len = meta.len();
    let start = len.saturating_sub(max_bytes);
    let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
    use std::io::{Read, Seek, SeekFrom};
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}
