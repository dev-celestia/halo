//! Coder editor font catalog — loaded from `assets/coder-fonts.json`.
//!
//! Edit that file to add or rename switcher options; settings store only the
//! selected `id`, and the live editor looks up the GPUI `family` here.
//!
//! Bundled open-source faces are registered at startup via `halo::fonts`
//! (`crates/ui/assets/fonts/`). System faces (Menlo, SF Mono, …) still require
//! the OS to provide them.

use std::sync::LazyLock;

use serde::Deserialize;

const FONTS_JSON: &str = include_str!("../assets/coder-fonts.json");

#[derive(Debug, Deserialize)]
struct FontsFile {
    default: String,
    fonts: Vec<CoderFontOption>,
}

/// One chip in the Coder settings font switcher.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct CoderFontOption {
    pub id: String,
    pub label: String,
    /// GPUI family name. `None` (or omitted) keeps the theme mono face.
    #[serde(default)]
    pub family: Option<String>,
}

struct Catalog {
    default_id: String,
    fonts: Vec<CoderFontOption>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let file: FontsFile = serde_json::from_str(FONTS_JSON)
        .unwrap_or_else(|err| panic!("invalid assets/coder-fonts.json: {err}"));
    assert!(
        !file.fonts.is_empty(),
        "assets/coder-fonts.json must list at least one font"
    );
    assert!(
        file.fonts.iter().any(|f| f.id == file.default),
        "assets/coder-fonts.json default {:?} is not in fonts",
        file.default
    );
    Catalog {
        default_id: file.default,
        fonts: file.fonts,
    }
});

/// Default font id from the config file.
pub fn default_font_id() -> &'static str {
    CATALOG.default_id.as_str()
}

/// Every switcher option, in config order.
pub fn font_options() -> &'static [CoderFontOption] {
    &CATALOG.fonts
}

/// Resolve a stored id to its catalog entry (falls back to the default).
pub fn font_option(id: &str) -> &'static CoderFontOption {
    CATALOG
        .fonts
        .iter()
        .find(|f| f.id == id)
        .or_else(|| CATALOG.fonts.iter().find(|f| f.id == CATALOG.default_id))
        .unwrap_or(&CATALOG.fonts[0])
}

/// GPUI family for `id`, or `None` for the theme mono face.
pub fn font_family(id: &str) -> Option<&'static str> {
    font_option(id).family.as_deref()
}

/// Display label for `id`.
pub fn font_label(id: &str) -> &'static str {
    font_option(id).label.as_str()
}
