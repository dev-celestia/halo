//! Combined asset source: Celestia Phosphor + explorer file-type SVGs + harness
//! brand marks + browser marks.
//!
//! gpui accepts a single [`AssetSource`] at startup, so Halo wraps
//! [`CelestiaAssets`] and serves `explorer-icon/<name>.svg` from
//! `assets/explorer-icon/`, `harness-mark/<name>.svg` from
//! `assets/harness-mark/`, and `browser/<name>.png` from `assets/browser/`.

use std::borrow::Cow;

use celestia_ui::assets::CelestiaAssets;
use gpui::{AssetSource, Result, SharedString};

#[derive(rust_embed::RustEmbed)]
#[folder = "assets/explorer-icon"]
#[include = "*.svg"]
struct ExplorerIconAssets;

/// Vendor marks for the harness picker (Pi, OpenCode, Antigravity) and model
/// rows (Claude/Anthropic, OpenAI/Codex).
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/harness-mark"]
#[include = "*.svg"]
struct HarnessMarkAssets;

/// Browser marks, drawn inline in the HTTP traffic table's `Host` cell.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/browser"]
#[include = "*.png"]
struct BrowserMarkAssets;

/// App-wide asset source registered in `main`.
pub struct HaloAssets;

impl AssetSource for HaloAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path.strip_prefix("explorer-icon/") {
            // Skip duplicate " copy" files; map to the clean name if present.
            if name.contains(" copy") {
                return Ok(None);
            }
            return Ok(ExplorerIconAssets::get(name).map(|file| file.data));
        }
        if let Some(name) = path.strip_prefix("harness-mark/") {
            return Ok(HarnessMarkAssets::get(name).map(|file| file.data));
        }
        if let Some(name) = path.strip_prefix("browser/") {
            return Ok(BrowserMarkAssets::get(name).map(|file| file.data));
        }
        if path == "icons/close.svg" || path == "icons/x.svg" {
            return CelestiaAssets.load("phosphor/regular/x.svg");
        }
        // `PopupMenuItem::checked` draws `IconName::Check` in the item's leading
        // slot. Without this alias the SVG resolves to nothing, so a checked
        // item (e.g. the HTTP toolbar's Method/Status/Options filters) looks
        // identical to an unchecked one.
        if path == "icons/check.svg" {
            return CelestiaAssets.load("phosphor/regular/check.svg");
        }
        if path == "icons/circle-check.svg" {
            return CelestiaAssets.load("phosphor/regular/check-circle.svg");
        }
        if path == "icons/circle-x.svg" {
            return CelestiaAssets.load("phosphor/regular/x-circle.svg");
        }
        if path == "icons/info.svg" {
            return CelestiaAssets.load("phosphor/regular/info.svg");
        }
        if path == "icons/triangle-alert.svg" {
            return CelestiaAssets.load("phosphor/regular/warning.svg");
        }
        CelestiaAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.starts_with("explorer-icon/") {
            let prefix = path.strip_prefix("explorer-icon/").unwrap_or("");
            return Ok(ExplorerIconAssets::iter()
                .filter(|name| !name.contains(" copy") && name.starts_with(prefix))
                .map(|name| SharedString::from(format!("explorer-icon/{name}")))
                .collect());
        }
        if path.starts_with("harness-mark/") {
            let prefix = path.strip_prefix("harness-mark/").unwrap_or("");
            return Ok(HarnessMarkAssets::iter()
                .filter(|name| name.starts_with(prefix))
                .map(|name| SharedString::from(format!("harness-mark/{name}")))
                .collect());
        }
        if path.starts_with("browser/") {
            let prefix = path.strip_prefix("browser/").unwrap_or("");
            return Ok(BrowserMarkAssets::iter()
                .filter(|name| name.starts_with(prefix))
                .map(|name| SharedString::from(format!("browser/{name}")))
                .collect());
        }
        CelestiaAssets.list(path)
    }
}

pub(crate) fn load_bytes(path: &str) -> Option<Cow<'static, [u8]>> {
    HaloAssets.load(path).ok().flatten()
}
