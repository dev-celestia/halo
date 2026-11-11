//! Explorer tree decorations derived from the cached git snapshot.
//!
//! Also hosts the shared author avatar used by the commit graph, inspector,
//! and pull-request surfaces.

use std::collections::HashMap;
use std::path::Path;

use celestia_ui::components::primitive::avatar::Avatar;
use celestia_ui::components::traits::{Sizable as _, Size};
use celestia_ui::theme::ActiveTheme as _;
use gpui::*;
use halo_bridge::GitState;
use md5::{Digest, Md5};

/// Compact Celestia avatar for a git author / PR login.
///
/// Prefers a remote image (GitHub avatar or Gravatar) when `email` / login can
/// be resolved; otherwise falls back to the identity-ring initials.
pub(crate) fn author_avatar(
    name: impl Into<SharedString>,
    email: impl AsRef<str>,
    size: impl Into<Size>,
) -> Avatar {
    let name = name.into();
    let email = email.as_ref().trim();
    let src = avatar_image_url(name.as_ref(), email);

    let avatar = if name.is_empty() {
        Avatar::new().with_size(size)
    } else {
        Avatar::new().name(name).with_size(size)
    };
    match src {
        Some(url) => avatar.src(url),
        None => avatar,
    }
}

/// Resolve a remote avatar URL for a git author or GitHub login.
fn avatar_image_url(name: &str, email: &str) -> Option<String> {
    if let Some(login) = github_login_from_email(email) {
        return Some(github_avatar_url(&login));
    }
    if !email.is_empty() {
        return Some(gravatar_url(email));
    }
    // Pull-request rows only have a GitHub login — no spaces, ASCII handle.
    let login = name.trim();
    if !login.is_empty() && login != "Unknown" && !login.contains(' ') && login.is_ascii() {
        return Some(github_avatar_url(login));
    }
    None
}

fn github_avatar_url(login: &str) -> String {
    // github.com/{user}.png redirects to the CDN avatar and is the documented
    // public shorthand; size is honored via the query string.
    format!("https://github.com/{login}.png?size=80")
}

fn github_login_from_email(email: &str) -> Option<String> {
    let email = email.trim().to_ascii_lowercase();
    let local = email.strip_suffix("@users.noreply.github.com")?;
    if local.is_empty() {
        return None;
    }
    // `1234567+login` or bare `login`.
    let login = local
        .rsplit_once('+')
        .map(|(_, login)| login)
        .unwrap_or(local);
    if login.is_empty() || !login.is_ascii() {
        None
    } else {
        Some(login.to_string())
    }
}

fn gravatar_url(email: &str) -> String {
    let hash = md5_hex(&email.trim().to_ascii_lowercase());
    // `d=identicon` keeps a visible mark when the address has no Gravatar.
    format!("https://www.gravatar.com/avatar/{hash}?s=80&d=identicon")
}

fn md5_hex(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    let mut out = String::with_capacity(32);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod avatar_tests {
    use super::*;

    #[test]
    fn github_noreply_email_maps_to_login() {
        assert_eq!(
            github_login_from_email("1234567+octocat@users.noreply.github.com").as_deref(),
            Some("octocat")
        );
        assert_eq!(
            github_login_from_email("octocat@users.noreply.github.com").as_deref(),
            Some("octocat")
        );
        assert_eq!(github_login_from_email("dev@example.com"), None);
    }

    #[test]
    fn gravatar_hash_matches_known_fixture() {
        // Classic Gravatar test vector for `hello@example.com` is not required;
        // pin the empty-string MD5 and a simple address instead.
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            md5_hex("test@example.com"),
            "55502f40dc8b7c769880b10874abc9d0"
        );
    }

    #[test]
    fn avatar_url_prefers_github_then_gravatar() {
        assert_eq!(
            avatar_image_url("Ada", "9+ada@users.noreply.github.com").as_deref(),
            Some("https://github.com/ada.png?size=80")
        );
        assert!(
            avatar_image_url("Ada", "ada@example.com")
                .unwrap()
                .starts_with("https://www.gravatar.com/avatar/")
        );
        assert_eq!(
            avatar_image_url("octocat", "").as_deref(),
            Some("https://github.com/octocat.png?size=80")
        );
        assert_eq!(avatar_image_url("Ada Lovelace", ""), None);
        assert_eq!(avatar_image_url("Unknown", ""), None);
    }
}

/// Git decoration for one explorer path (relative to the repo root).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GitDecor {
    pub ignored: bool,
    /// Short porcelain letter when dirty (`M`, `A`, `D`, `?`, …).
    pub status: Option<&'static str>,
}

/// Lookup built once per tree render from the cached git snapshot.
pub(crate) struct GitDecorMap {
    /// Exact relative paths → decoration (files + ignored dirs).
    by_path: HashMap<String, GitDecor>,
    /// Ignored path prefixes (dirs) for descendant matching.
    ignored_prefixes: Vec<String>,
    /// Dirty relative paths (for bubbling status up to parent folders).
    dirty_paths: Vec<String>,
}

impl GitDecorMap {
    pub(crate) fn from_git(git: &GitState) -> Self {
        let mut by_path: HashMap<String, GitDecor> = HashMap::new();
        let mut ignored_prefixes = Vec::new();
        let mut dirty_paths = Vec::new();

        // Later lists win (unstaged / untracked override staged for the badge).
        let upsert_status =
            |map: &mut HashMap<String, GitDecor>, path: &str, status: &'static str| {
                map.entry(path.to_string()).or_default().status = Some(status);
            };

        for f in &git.staged {
            let status = match f.status.as_str() {
                "A" => "A",
                "D" => "D",
                "R" => "R",
                "C" => "C",
                _ => "M",
            };
            upsert_status(&mut by_path, &f.path, status);
            dirty_paths.push(f.path.clone());
        }
        for f in &git.unstaged {
            let status = match f.status.as_str() {
                "D" => "D",
                "R" => "R",
                "C" => "C",
                _ => "M",
            };
            upsert_status(&mut by_path, &f.path, status);
            dirty_paths.push(f.path.clone());
        }
        for f in &git.untracked {
            upsert_status(&mut by_path, &f.path, "?");
            dirty_paths.push(f.path.clone());
        }
        for f in &git.ignored {
            let path = f.path.trim_end_matches('/').to_string();
            by_path.entry(path.clone()).or_default().ignored = true;
            ignored_prefixes.push(path);
        }

        ignored_prefixes.sort_unstable();
        ignored_prefixes.dedup();
        dirty_paths.sort_unstable();
        dirty_paths.dedup();

        Self {
            by_path,
            ignored_prefixes,
            dirty_paths,
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            by_path: HashMap::new(),
            ignored_prefixes: Vec::new(),
            dirty_paths: Vec::new(),
        }
    }

    pub(crate) fn lookup(&self, rel: &str) -> GitDecor {
        let mut decor = self.by_path.get(rel).copied().unwrap_or_default();
        if !decor.ignored {
            decor.ignored = self.ignored_prefixes.iter().any(|prefix| {
                rel == prefix.as_str()
                    || rel
                        .strip_prefix(prefix.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            });
        }
        if decor.status.is_none() {
            // Folder: inherit a dirty marker when any descendant is dirty.
            let prefix = format!("{rel}/");
            if self.dirty_paths.iter().any(|p| p.starts_with(&prefix)) {
                decor.status = Some("M");
            }
        }
        decor
    }
}

pub(crate) fn rel_to_repo(abs: &str, repo_root: &str) -> Option<String> {
    let abs = Path::new(abs);
    let root = Path::new(repo_root);
    abs.strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

pub(crate) fn status_color(status: &str, cx: &App) -> Hsla {
    match status {
        "A" | "?" => cx.theme().success,
        "D" => cx.theme().danger,
        "M" | "R" | "C" => cx.theme().warning,
        _ => cx.theme().muted_foreground,
    }
}

pub(crate) fn status_dot(status: &str, cx: &App) -> impl IntoElement {
    let color = status_color(status, cx);
    div().size(px(6.)).rounded_full().flex_shrink_0().bg(color)
}

/// Build the explorer decoration map for the current bridge snapshot.
pub(crate) fn decor_map(state: &halo_bridge::BridgeState) -> std::sync::Arc<GitDecorMap> {
    let repo_root = state
        .git
        .repo_root
        .clone()
        .or_else(|| state.coder.root_path.clone())
        .unwrap_or_default();
    if state.git.not_a_repo || repo_root.is_empty() {
        std::sync::Arc::new(GitDecorMap::empty())
    } else {
        std::sync::Arc::new(GitDecorMap::from_git(&state.git))
    }
}

pub(crate) fn repo_root(state: &halo_bridge::BridgeState) -> String {
    state
        .git
        .repo_root
        .clone()
        .or_else(|| state.coder.root_path.clone())
        .unwrap_or_default()
}
