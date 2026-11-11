//! Wordlists — fuzzing/pentest wordlist catalog.
//!
//! Bundled lists ship inside the binary; remote lists come from the GitHub
//! manifest and are downloaded into `Documents/Halo Files/Wordlists`.
//! Manifest hrefs are sanitized before any path join.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::file_explorer::{halo_files_root, safe_join, safe_path_segments};
use super::*;

pub const WORDLISTS_DIR_NAME: &str = "Wordlists";
pub const WORDLISTS_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/dev-celestia/wordlists/main/wordlists.json";
pub const WORDLISTS_RAW_BASE_URL: &str =
    "https://raw.githubusercontent.com/dev-celestia/wordlists/main/";

// ── DTOs ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WordlistStatus {
    Bundled,
    Ready,
    Downloading,
    Error,
    Cloud,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordlistItem {
    /// Display name (e.g. "Dns Subdomains Top50").
    pub name: String,
    /// Manifest href relative to the raw base (e.g. `dns/subdomains-top50.txt`).
    pub href: String,
    pub tags: Vec<String>,
    pub lines: Option<u64>,
    pub file_size: Option<u64>,
    pub status: WordlistStatus,
    /// Set for bundled lists: the embedded content.
    pub bundled_content: Option<String>,
    /// Error detail when `status == Error`.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordlistTag {
    pub tag: String,
    pub count: usize,
    pub installed_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordlistsView {
    pub items: Vec<WordlistItem>,
    pub tags: Vec<WordlistTag>,
    pub selected_tag: String,
    pub selected: Option<String>,
    pub search: String,
    pub preview: Option<String>,
    pub loading: bool,
    pub bundle_current: usize,
    pub bundle_total: usize,
    /// `Documents/Halo Files/Wordlists`.
    pub dir: Option<String>,
}

impl Default for WordlistsView {
    fn default() -> Self {
        Self {
            items: bundled_wordlists(),
            tags: Vec::new(),
            selected_tag: "all".into(),
            selected: None,
            search: String::new(),
            preview: None,
            loading: false,
            bundle_current: 0,
            bundle_total: 0,
            dir: None,
        }
    }
}

impl WordlistsView {
    /// Items filtered by the tag sidebar + search box.
    pub fn catalog(&self) -> Vec<WordlistItem> {
        let query = self.search.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                let tag_ok =
                    self.selected_tag == "all" || item.tags.iter().any(|t| *t == self.selected_tag);
                let search_ok = query.is_empty()
                    || item.name.to_lowercase().contains(&query)
                    || item.href.to_lowercase().contains(&query)
                    || item.tags.iter().any(|t| t.to_lowercase().contains(&query));
                tag_ok && search_ok
            })
            .cloned()
            .collect()
    }

    /// `(installed, total)`.
    pub fn totals(&self) -> (usize, usize) {
        let total = self.items.len();
        let installed = self
            .items
            .iter()
            .filter(|item| matches!(item.status, WordlistStatus::Ready | WordlistStatus::Bundled))
            .count();
        (installed, total)
    }
}

// ── bundled wordlists ──────────────────────────────────────────────────────

struct BundledSource {
    name: &'static str,
    href: &'static str,
    tags: &'static [&'static str],
    content: &'static str,
}

const BUNDLED_SOURCES: &[BundledSource] = &[
    BundledSource {
        name: "Dns Subdomains Top50",
        href: "dns/subdomains-top50.txt",
        tags: &["dns"],
        content: include_str!("wordlists_data/subdomains-top50.txt"),
    },
    BundledSource {
        name: "Api Endpoints Sample",
        href: "api/api-endpoints-sample.txt",
        tags: &["api"],
        content: include_str!("wordlists_data/api-endpoints-sample.txt"),
    },
    BundledSource {
        name: "Logins Fuzz Sample",
        href: "logins/logins-fuzz-sample.txt",
        tags: &["logins"],
        content: include_str!("wordlists_data/logins-fuzz-sample.txt"),
    },
    BundledSource {
        name: "Usernames Top50",
        href: "usernames/usernames-top50.txt",
        tags: &["usernames"],
        content: include_str!("wordlists_data/usernames-top50.txt"),
    },
];

pub fn bundled_wordlists() -> Vec<WordlistItem> {
    BUNDLED_SOURCES
        .iter()
        .map(|source| {
            let lines = source.content.lines().count() as u64;
            WordlistItem {
                name: source.name.to_string(),
                href: source.href.to_string(),
                tags: source.tags.iter().map(|t| t.to_string()).collect(),
                lines: Some(lines),
                file_size: Some(source.content.len() as u64),
                status: WordlistStatus::Bundled,
                bundled_content: Some(source.content.to_string()),
                error: None,
            }
        })
        .collect()
}

fn format_wordlist_name(href: &str) -> String {
    let file = href.rsplit('/').next().unwrap_or(href);
    let stem = file.trim_end_matches(".txt").replace(['-', '_'], " ");
    let mut out = String::new();
    for word in stem.split_whitespace() {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

/// Validate one manifest entry from the remote JSON array.
fn wordlist_from_manifest(value: &serde_json::Value) -> Option<WordlistItem> {
    let href = value.get("href")?.as_str()?;
    let segments = safe_path_segments(href);
    // Strip a leading `wordlists/` so the manifest cannot steer writes above
    // the wordlists root, then require at least one safe segment.
    let segments = match segments.first() {
        Some(first) if first == "wordlists" => segments[1..].to_vec(),
        _ => segments,
    };
    if segments.is_empty() {
        return None;
    }
    let clean_href = segments.join("/");
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format_wordlist_name(&clean_href));
    let tags = value
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let lines = value.get("lines").and_then(|v| v.as_u64());
    let file_size = value.get("fileSize").and_then(|v| v.as_u64());
    Some(WordlistItem {
        name,
        href: clean_href,
        tags,
        lines,
        file_size,
        status: WordlistStatus::Cloud,
        bundled_content: None,
        error: None,
    })
}

fn wordlists_root() -> Option<PathBuf> {
    halo_files_root().map(|root| root.join(WORDLISTS_DIR_NAME))
}

fn sync_wordlist_statuses(state: &mut WordlistsView) {
    let Some(dir) = state.dir.clone() else {
        return;
    };
    for item in &mut state.items {
        if item.status == WordlistStatus::Bundled || item.status == WordlistStatus::Downloading {
            continue;
        }
        let Some(path) = safe_join(Path::new(&dir), &safe_path_segments(&item.href)) else {
            continue;
        };
        if path.is_file() {
            item.status = WordlistStatus::Ready;
            if item.file_size.is_none() {
                item.file_size = fs::metadata(&path).ok().map(|m| m.len());
            }
            if item.lines.is_none() {
                item.lines = fs::read_to_string(&path)
                    .ok()
                    .map(|text| text.lines().count() as u64);
            }
        } else {
            item.status = WordlistStatus::Cloud;
        }
    }
    rebuild_wordlist_tags(state);
}

fn rebuild_wordlist_tags(state: &mut WordlistsView) {
    let mut tags: Vec<WordlistTag> = Vec::new();
    for item in &state.items {
        let installed = matches!(item.status, WordlistStatus::Ready | WordlistStatus::Bundled);
        for tag in &item.tags {
            if let Some(entry) = tags.iter_mut().find(|t| t.tag == *tag) {
                entry.count += 1;
                entry.installed_count += installed as usize;
            } else {
                tags.push(WordlistTag {
                    tag: tag.clone(),
                    count: 1,
                    installed_count: installed as usize,
                });
            }
        }
    }
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    state.tags = tags;
}

impl Bridge {
    /// Create `Documents/Halo Files/Wordlists` and remember its path.
    pub fn wordlists_ensure_dir(cx: &mut App) -> BridgeResult<()> {
        let Some(dir) = wordlists_root() else {
            return Err(BridgeError::message("Documents directory unavailable"));
        };
        fs::create_dir_all(&dir).map_err(|e| BridgeError::message(e.to_string()))?;
        Self::state(cx).update(cx, |s| {
            s.wordlists.dir = Some(dir.display().to_string());
        });
        Ok(())
    }

    pub fn wordlists_sync(cx: &mut App) -> BridgeResult<()> {
        Self::wordlists_ensure_dir(cx)?;
        Self::state(cx).update(cx, |s| {
            sync_wordlist_statuses(&mut s.wordlists);
        });
        Ok(())
    }

    pub fn wordlists_select(href: Option<String>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.wordlists.selected = href.clone();
            s.wordlists.preview = None;
        });
        if let Some(href) = href {
            let _ = Self::wordlists_preview(&href, cx);
        }
    }

    pub fn wordlists_set_tag(tag: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.wordlists.selected_tag = tag.to_string();
        });
    }

    pub fn wordlists_set_search(query: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.wordlists.search = query.to_string());
    }

    /// First 100 lines: bundled content or the local copy.
    pub fn wordlists_preview(href: &str, cx: &mut App) -> BridgeResult<()> {
        let item = Self::state(cx)
            .read(cx)
            .wordlists
            .items
            .iter()
            .find(|item| item.href == href)
            .cloned();
        let Some(item) = item else {
            return Ok(());
        };
        let preview: Option<String> = match &item.bundled_content {
            Some(content) => Some(content.lines().take(100).collect::<Vec<_>>().join("\n")),
            None => {
                let Some(dir) = Self::state(cx).read(cx).wordlists.dir.clone() else {
                    return Ok(());
                };
                let Some(path) = safe_join(Path::new(&dir), &safe_path_segments(href)) else {
                    return Ok(());
                };
                fs::read_to_string(path)
                    .ok()
                    .map(|text| text.lines().take(100).collect::<Vec<_>>().join("\n"))
            }
        };
        Self::state(cx).update(cx, |s| {
            s.wordlists.preview = preview
                .or_else(|| Some("(No local copy — download the wordlist to preview it.)".into()));
        });
        Ok(())
    }

    /// Fetch the remote manifest from GitHub and merge it with the bundled
    /// catalog. `force` bypasses the previously fetched set.
    pub fn wordlists_fetch_manifest(force: bool, cx: &mut App) -> BridgeResult<()> {
        Self::wordlists_ensure_dir(cx)?;
        let has_remote = Self::state(cx)
            .read(cx)
            .wordlists
            .items
            .iter()
            .any(|item| item.bundled_content.is_none());
        if has_remote && !force {
            return Ok(());
        }

        Self::state(cx).update(cx, |s| {
            s.wordlists.loading = true;
        });

        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let fetch = std::thread::Builder::new()
                .name("halo-wordlists".into())
                .spawn(|| {
                    let response = reqwest::blocking::get(WORDLISTS_MANIFEST_URL)
                        .and_then(|response| response.error_for_status());
                    match response {
                        Ok(response) => response.json::<serde_json::Value>().ok(),
                        Err(_) => None,
                    }
                });

            let result = match fetch {
                Ok(handle) => handle.join().unwrap_or(None),
                Err(_) => None,
            };

            let _ = executor.timer(Duration::from_millis(1)).await;
            let _ = cx.update(|cx| {
                let entries: Vec<WordlistItem> = result
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_default()
                    .iter()
                    .filter_map(wordlist_from_manifest)
                    .collect();
                Self::state(cx).update(cx, |s| {
                    s.wordlists.loading = false;
                    let bundled: Vec<WordlistItem> = s
                        .wordlists
                        .items
                        .iter()
                        .filter(|item| item.bundled_content.is_some())
                        .cloned()
                        .collect();
                    s.wordlists.items = bundled;
                    s.wordlists.items.extend(entries.clone());
                    sync_wordlist_statuses(&mut s.wordlists);
                    if entries.is_empty() {
                        s.last_error = Some("Wordlists manifest unavailable (offline?)".into());
                        s.last_error_info = false;
                    } else {
                        s.apply_ok_msg(format!(
                            "Wordlists catalog loaded ({} remote)",
                            entries.len()
                        ));
                    }
                });
            });
        })
        .detach();
        Ok(())
    }

    /// Download one wordlist (remote items only) into the Wordlists dir.
    pub fn wordlists_download(href: &str, cx: &mut App) -> BridgeResult<()> {
        Self::wordlists_ensure_dir(cx)?;
        let Some(dir) = Self::state(cx).read(cx).wordlists.dir.clone() else {
            return Err(BridgeError::message("Wordlists directory unavailable"));
        };
        let Some(path) = safe_join(Path::new(&dir), &safe_path_segments(href)) else {
            return Err(BridgeError::message("Unsafe wordlist path"));
        };
        Self::state(cx).update(cx, |s| {
            if let Some(item) = s.wordlists.items.iter_mut().find(|item| item.href == href) {
                item.status = WordlistStatus::Downloading;
            }
        });

        let url = format!("{WORDLISTS_RAW_BASE_URL}{href}");
        let href = href.to_string();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let handle = std::thread::Builder::new()
                .name("halo-wordlist-dl".into())
                .spawn(move || {
                    reqwest::blocking::get(&url)
                        .and_then(|response| response.error_for_status())
                        .and_then(|response| response.bytes())
                        .map(|bytes| bytes.to_vec())
                });
            let result: Result<Vec<u8>, String> = match handle {
                Ok(handle) => match handle.join() {
                    Ok(Ok(bytes)) => Ok(bytes),
                    Ok(Err(err)) => Err(err.to_string()),
                    Err(_) => Err("fetch thread died".to_string()),
                },
                Err(_) => Err("failed to spawn download thread".into()),
            };

            let _ = cx.update(|cx| {
                Self::state(cx).update(cx, |s| {
                    if let Some(item) = s.wordlists.items.iter_mut().find(|item| item.href == href)
                    {
                        match result {
                            Ok(bytes) => {
                                if let Some(parent) = path.parent() {
                                    let _ = fs::create_dir_all(parent);
                                }
                                match fs::write(&path, &bytes) {
                                    Ok(()) => {
                                        item.status = WordlistStatus::Ready;
                                        item.file_size = Some(bytes.len() as u64);
                                        item.lines =
                                            Some(String::from_utf8_lossy(&bytes).lines().count()
                                                as u64);
                                        s.apply_ok_msg(format!("Downloaded {href}"));
                                    }
                                    Err(err) => {
                                        item.status = WordlistStatus::Error;
                                        item.error = Some(err.to_string());
                                    }
                                }
                            }
                            Err(err) => {
                                item.status = WordlistStatus::Error;
                                item.error = Some(err.to_string());
                            }
                        }
                    }
                    if s.wordlists.bundle_total > 0
                        && s.wordlists.bundle_current < s.wordlists.bundle_total
                    {
                        s.wordlists.bundle_current += 1;
                    }
                    sync_wordlist_statuses(&mut s.wordlists);
                });
            });
        })
        .detach();
        Ok(())
    }

    /// Download every non-installed wordlist under the selected tag (or all).
    pub fn wordlists_download_bundle(cx: &mut App) -> BridgeResult<()> {
        Self::wordlists_ensure_dir(cx)?;
        let hrefs: Vec<String> = {
            let state = Self::state(cx).read(cx);
            state
                .wordlists
                .items
                .iter()
                .filter(|item| {
                    item.status == WordlistStatus::Cloud
                        && (state.wordlists.selected_tag == "all"
                            || item.tags.iter().any(|t| *t == state.wordlists.selected_tag))
                })
                .map(|item| item.href.clone())
                .collect()
        };
        let total = hrefs.len();
        Self::state(cx).update(cx, |s| {
            s.wordlists.bundle_total = total;
            s.wordlists.bundle_current = 0;
        });
        for href in hrefs {
            Self::wordlists_download(&href, cx)?;
        }
        Ok(())
    }

    /// Local path for a downloaded wordlist, if the item is installed.
    pub fn wordlists_local_path(href: &str) -> Option<String> {
        let dir = {
            // Resolve without `cx` so this can be used while building UI.
            wordlists_root()?.display().to_string()
        };
        safe_join(Path::new(&dir), &safe_path_segments(href))
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string())
    }

    pub fn wordlists_open(href: &str, cx: &mut App) {
        if let Some(path) = Self::wordlists_local_path(href) {
            cx.open_with_system(Path::new(&path));
        }
    }

    pub fn wordlists_reveal(href: &str, cx: &mut App) {
        if let Some(path) = Self::wordlists_local_path(href) {
            cx.reveal_path(Path::new(&path));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_populated() {
        let items = bundled_wordlists();
        assert_eq!(items.len(), 4);
        assert!(items
            .iter()
            .all(|item| item.status == WordlistStatus::Bundled));
        assert!(items[0].lines.unwrap() > 0);
    }

    #[test]
    fn manifest_entries_are_sanitized() {
        let entry = serde_json::json!({
            "name": "Evil",
            "href": "wordlists/../../etc/passwd.txt",
            "tags": ["x"],
            "lines": 10,
            "fileSize": 20
        });
        let item = wordlist_from_manifest(&entry).unwrap();
        // `..` segments stripped; the `wordlists/` prefix removed.
        assert!(!item.href.contains(".."));
        assert!(!item.href.starts_with("wordlists/"));
        assert!(item.href.starts_with("etc/"));

        let empty = serde_json::json!({ "href": ".." });
        assert!(wordlist_from_manifest(&empty).is_none());
    }

    #[test]
    fn manifest_name_falls_back_to_href() {
        let entry = serde_json::json!({ "href": "api/api-endpoints-res.txt" });
        let item = wordlist_from_manifest(&entry).unwrap();
        assert_eq!(item.name, "Api Endpoints Res");
    }
}
