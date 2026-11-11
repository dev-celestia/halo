//! Effective skill catalog from a short-lived `opencode serve`.
//!
//! The CLI applies OpenCode's own config, compatibility roots and precedence
//! rules, which a filesystem walk cannot reproduce. A discovery server is
//! spawned per refresh, polled until the list settles, then killed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use uuid::Uuid;

use super::probe;
use super::run::{get_json, http_client, spawn_serve, wait_for_listen_url};
use super::types::{OpenCodeProbeResult, OpenCodeSkill};

const SKILL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const SKILL_POLL_INTERVAL: Duration = Duration::from_millis(250);
const BUILTIN_PREFIX: &str = "/builtin/";

pub fn discover_skills(
    custom_executable: Option<&str>,
    cwd: &str,
) -> Result<Vec<OpenCodeSkill>, String> {
    let executable = match probe::probe_environment(custom_executable) {
        OpenCodeProbeResult::Ready { path, .. } => path,
        other => return Err(other.setup_hint()),
    };
    let project = PathBuf::from(cwd);
    if !project.is_dir() {
        return Err("Workspace root is not a directory".into());
    }

    let password = Uuid::new_v4().simple().to_string();
    let mut server = KillOnDrop(spawn_serve(
        &executable,
        &project,
        &password,
        &std::collections::HashMap::new(),
    )?);
    let base = wait_for_listen_url(server.child_mut(), Instant::now() + SKILL_DISCOVERY_TIMEOUT)?;
    let client = http_client(&password)?;

    // Built-ins are listed first; project, user and plugin skills follow as
    // one batch. Settle on an unchanged batch beyond built-ins, or on any
    // list that has held for eight polls.
    let mut last: Option<Vec<Value>> = None;
    let mut unchanged = 0usize;
    let deadline = Instant::now() + SKILL_DISCOVERY_TIMEOUT;
    loop {
        if let Some(entries) = get_json(&client, &base, "/api/skill")
            .ok()
            .and_then(|value| value.get("data").and_then(Value::as_array).cloned())
            .filter(|entries| !entries.is_empty())
        {
            unchanged = if last
                .as_ref()
                .is_some_and(|previous| previous.len() == entries.len())
            {
                unchanged + 1
            } else {
                0
            };
            let beyond_builtins = entries.iter().any(|entry| {
                !entry["path"]
                    .as_str()
                    .unwrap_or("")
                    .starts_with(BUILTIN_PREFIX)
            });
            let settled = (beyond_builtins && unchanged >= 1) || unchanged >= 8;
            let parsed = settled.then(|| parse_skills(&entries, &project));
            last = Some(entries);
            if let Some(skills) = parsed {
                drop(server);
                return Ok(skills);
            }
        }
        if Instant::now() >= deadline {
            let skills = last
                .map(|entries| parse_skills(&entries, &project))
                .unwrap_or_default();
            drop(server);
            return Ok(skills);
        }
        std::thread::sleep(SKILL_POLL_INTERVAL);
    }
}

/// `GET /api/skill` rows → catalog. The id is what a prompt attaches.
pub(super) fn parse_skills(entries: &[Value], project: &Path) -> Vec<OpenCodeSkill> {
    let canonical_project = fs::canonicalize(project).ok();
    let home = dirs::home_dir();
    let canonical_home = home.as_deref().and_then(|home| fs::canonicalize(home).ok());
    let in_dir = |path: &Path, dir: &Path, canonical: Option<&PathBuf>| {
        path.starts_with(dir) || canonical.is_some_and(|root| path.starts_with(root))
    };

    let mut seen = std::collections::HashSet::new();
    let mut skills: Vec<OpenCodeSkill> = entries
        .iter()
        .filter_map(|entry| {
            let id = entry["id"].as_str()?.trim();
            if id.is_empty() || !seen.insert(id.to_ascii_lowercase()) {
                return None;
            }
            let path = entry["path"].as_str().unwrap_or("").to_string();
            let builtin = path.starts_with(BUILTIN_PREFIX);
            let scope = if builtin {
                "system"
            } else {
                let resolved = Path::new(&path);
                if in_dir(resolved, project, canonical_project.as_ref()) {
                    "project"
                } else if home
                    .as_deref()
                    .is_some_and(|home| in_dir(resolved, home, canonical_home.as_ref()))
                {
                    "user"
                } else {
                    "system"
                }
            };
            let name = entry["name"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(id)
                .to_string();
            let description = entry["description"]
                .as_str()
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .unwrap_or("")
                .to_string();
            Some(OpenCodeSkill {
                id: id.to_string(),
                name,
                description,
                path,
                scope: scope.to_string(),
                builtin,
            })
        })
        .collect();
    skills.sort_by_key(|skill| skill.id.to_ascii_lowercase());
    skills
}

/// Kills the discovery server on every exit path, including `?` returns.
struct KillOnDrop(std::process::Child);

impl KillOnDrop {
    fn child_mut(&mut self) -> &mut std::process::Child {
        &mut self.0
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_skills_tags_scope_and_dedupes() {
        let project = Path::new("/work/app");
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/me"));
        let user_skill = home.join(".config/opencode/skills/demo/SKILL.md");
        let rows = vec![
            json!({"id": "explain", "name": "Explain", "description": "Walk a codebase", "path": "/builtin/explain"}),
            json!({"id": "local", "name": "Local", "path": "/work/app/.opencode/skills/local/SKILL.md"}),
            json!({"id": "local", "name": "Duplicate", "path": "/work/app/.opencode/skills/dup/SKILL.md"}),
            json!({"id": "mine", "path": user_skill.to_string_lossy()}),
        ];
        let skills = parse_skills(&rows, project);
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].id, "explain");
        assert_eq!(skills[0].scope, "system");
        assert!(skills[0].builtin);
        assert_eq!(skills[1].id, "local");
        assert_eq!(skills[1].scope, "project");
        assert_eq!(skills[2].id, "mine");
        assert_eq!(skills[2].scope, "user");
        // Missing name falls back to the id.
        assert_eq!(skills[2].name, "mine");
    }
}
