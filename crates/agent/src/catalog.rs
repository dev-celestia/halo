//! Workspace file search and skill discovery for the agent composer.
//!
//! Bounded directory walk for `@`, and SKILL.md / flat markdown under the
//! usual agent skill roots for `/`.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE_SEARCH_MAX_RESULTS: usize = 60;
const FILE_SEARCH_MAX_SCAN: usize = 10_000;
const SKILL_MAX_ENTRIES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchMatch {
    pub path: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    /// Stable id a harness attaches (`/api/skill` id on OpenCode; name locally).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub description: String,
    /// `"project" | "user" | "system"`.
    #[serde(default)]
    pub scope: String,
}

fn skip_dir_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | "dist"
            | "build"
            | ".next"
            | ".turbo"
            | ".cache"
            | "vendor"
            | "__pycache__"
    )
}

/// Case-insensitive subsequence match — enough for a composer picker without
fn single_fuzzy_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut h = haystack.chars().flat_map(|c| c.to_lowercase());
    for n in needle.chars().flat_map(|c| c.to_lowercase()) {
        loop {
            match h.next() {
                Some(c) if c == n => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// Case-insensitive subsequence match. When `needle` contains spaces,
/// all whitespace-separated terms must match (multi-term fuzzy match).
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let parts: Vec<&str> = needle.split_whitespace().collect();
    if parts.is_empty() {
        return true;
    }
    parts.iter().all(|part| single_fuzzy_match(haystack, part))
}

/// Rank: basename prefix > path prefix > fuzzy; shorter paths first within a tier.
fn rank_path(path: &str, query: &str) -> u8 {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return 2;
    }
    let base = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let full = path.to_lowercase();
    if base.starts_with(&q) {
        0
    } else if full.contains(&q) {
        1
    } else if fuzzy_match(path, query) {
        2
    } else {
        3
    }
}

/// Search workspace file and directory names under `root` using breadth-first traversal.
pub fn search_files(root: &str, query: &str) -> Result<Vec<FileSearchMatch>, String> {
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err("Workspace root is not a directory".into());
    }
    let query = query.trim();
    let mut matches = Vec::new();
    let mut scanned = 0usize;
    let mut queue = VecDeque::new();
    queue.push_back(root.clone());

    while let Some(dir) = queue.pop_front() {
        if scanned >= FILE_SEARCH_MAX_SCAN || matches.len() >= FILE_SEARCH_MAX_RESULTS * 4 {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            scanned += 1;
            if scanned >= FILE_SEARCH_MAX_SCAN {
                break;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') && name.as_ref() != ".agents" && name.as_ref() != ".cursor" {
                // Still descend into skill roots; skip other dot dirs.
                let is_skill_root = matches!(
                    name.as_ref(),
                    ".agents" | ".claude" | ".cursor" | ".codex" | ".opencode"
                );
                if !is_skill_root {
                    continue;
                }
            }
            let path = entry.path();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir && skip_dir_name(&name) {
                continue;
            }
            if is_dir {
                queue.push_back(path.clone());
            }
            let Ok(rel) = path.strip_prefix(&root) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel.is_empty() {
                continue;
            }
            if rank_path(&rel, query) < 3 {
                matches.push(FileSearchMatch { path: rel, is_dir });
            }
        }
    }

    matches.sort_by(|a, b| {
        rank_path(&a.path, query)
            .cmp(&rank_path(&b.path, query))
            .then_with(|| a.path.len().cmp(&b.path.len()))
            .then_with(|| a.path.cmp(&b.path))
    });
    matches.truncate(FILE_SEARCH_MAX_RESULTS);
    Ok(matches)
}

/// Skill roots under the project and under the user home.
///
/// OpenCode reads `.opencode/skills` and `~/.config/opencode/skills`; the other
/// entries are shared conventions Pi/Claude/Qoder already honour. The harness
/// argument is kept for callers that want harness-specific roots later.
fn skill_roots(_harness: &str) -> &'static [&'static str] {
    &[
        ".agents/skills",
        ".pi/skills",
        ".opencode/skills",
        ".claude/skills",
        ".qoder/skills",
    ]
}

fn home_skill_roots() -> &'static [&'static str] {
    &[
        ".agents/skills",
        ".pi/skills",
        ".config/opencode/skills",
        ".claude/skills",
        ".qoder/skills",
    ]
}

/// Scope for a discovered skill path (used by the scope tests).
#[cfg(test)]
fn scope_for(path: &Path, project: &Path, home: &Path) -> &'static str {
    let canonical = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let path = canonical(path);
    if path.starts_with(canonical(project)) {
        "project"
    } else if path.starts_with(canonical(home)) {
        "user"
    } else {
        "system"
    }
}

fn read_skill_description(path: &Path) -> String {
    let Ok(raw) = fs::read_to_string(path) else {
        return String::new();
    };
    // YAML front matter `description:` when present; else first prose line.
    if let Some(rest) = raw.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                let line = line.trim();
                if let Some(desc) = line
                    .strip_prefix("description:")
                    .or_else(|| line.strip_prefix("Description:"))
                {
                    return desc.trim().trim_matches('"').trim_matches('\'').to_string();
                }
            }
        }
    }
    raw.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("---"))
        .unwrap_or("")
        .chars()
        .take(120)
        .collect()
}

fn skill_name_from_path(path: &Path, skills_root: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if file_name.eq_ignore_ascii_case("SKILL.md") {
        let parent = path.parent()?.file_name()?.to_str()?.to_string();
        return Some(parent);
    }
    if path.extension().is_some_and(|e| e == "md") {
        let rel = path.strip_prefix(skills_root).ok()?;
        return Some(
            rel.with_extension("")
                .to_string_lossy()
                .replace(['/', '\\'], ":"),
        );
    }
    None
}

fn scan_skill_root(
    root: &Path,
    scope: &str,
    found: &mut BTreeMap<String, SkillInfo>,
    budget: &mut usize,
) {
    if *budget == 0 || !root.is_dir() {
        return;
    }
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut seen = HashSet::new();
    while let Some((dir, depth)) = pending.pop() {
        if *budget == 0 || depth > 8 {
            continue;
        }
        let Ok(canonical) = fs::canonicalize(&dir) else {
            continue;
        };
        if !seen.insert(canonical) {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if *budget == 0 {
                break;
            }
            *budget -= 1;
            let path = entry.path();
            if path.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            let Some(name) = skill_name_from_path(&path, root) else {
                continue;
            };
            if name.is_empty() || found.contains_key(&name) {
                continue;
            }
            let description = read_skill_description(&path);
            found.insert(
                name.clone(),
                SkillInfo {
                    id: name.clone(),
                    name,
                    path: path.to_string_lossy().into(),
                    description,
                    scope: scope.to_string(),
                },
            );
        }
    }
}

/// Discover skills for `harness` under the project and the user home.
pub fn list_skills(root: &str, harness: &str) -> Result<Vec<SkillInfo>, String> {
    let project = PathBuf::from(root);
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut found = BTreeMap::new();
    let mut budget = SKILL_MAX_ENTRIES;

    for suffix in home_skill_roots() {
        scan_skill_root(&home.join(suffix), "user", &mut found, &mut budget);
    }
    for suffix in skill_roots(harness) {
        // Nearest worktree ancestors, project overrides home by name.
        let mut dir = Some(project.as_path());
        while let Some(cwd) = dir {
            let scope = if cwd == home { "user" } else { "project" };
            scan_skill_root(&cwd.join(suffix), scope, &mut found, &mut budget);
            if cwd.join(".git").exists() || cwd == home {
                break;
            }
            dir = cwd.parent();
        }
    }

    Ok(found.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match() {
        assert!(fuzzy_match("crates/ui/src/main.rs", "main"));
        assert!(fuzzy_match("crates/ui/src/main.rs", "ui main"));
        assert!(fuzzy_match("crates/ui/src/main.rs", "crtsmain"));
        assert!(!fuzzy_match("crates/ui/src/main.rs", "foobar"));
    }

    #[test]
    fn test_rank_path() {
        assert_eq!(rank_path("src/main.rs", "main"), 0);
        assert_eq!(rank_path("crates/ui/src/main.rs", "ui"), 1);
        assert_eq!(rank_path("crates/ui/src/main.rs", "crtsmain"), 2);
        assert_eq!(rank_path("crates/ui/src/main.rs", "notfound"), 3);
    }

    #[test]
    fn skill_roots_include_opencode_and_qoder() {
        let roots = skill_roots("opencode");
        assert!(roots.contains(&".opencode/skills"));
        assert!(roots.contains(&".qoder/skills"));
        assert!(home_skill_roots().contains(&".config/opencode/skills"));
    }

    #[test]
    fn skill_scope_prefers_project_then_user() {
        let project = Path::new("/work/app");
        let home = Path::new("/home/me");
        assert_eq!(
            scope_for(
                Path::new("/work/app/.agents/skills/demo/SKILL.md"),
                project,
                home
            ),
            "project"
        );
        assert_eq!(
            scope_for(
                Path::new("/home/me/.qoder/skills/demo/SKILL.md"),
                project,
                home
            ),
            "user"
        );
        assert_eq!(
            scope_for(Path::new("/opt/skills/demo/SKILL.md"), project, home),
            "system"
        );
    }
}
