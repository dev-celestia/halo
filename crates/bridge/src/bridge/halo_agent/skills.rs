//! Per-chat Skills & MCP selection, the skill catalog refresh, and the
//! right-side Context rail toggle.
//!
//! The catalog is fetched off the UI thread (OpenCode needs a short-lived
//! server); selections live on [`HaloAgentSession`] so each chat remembers its
//! own tools. A run resolves ids/names against the catalog and the configured
//! servers — disabling a server globally drops it from every chat.

use super::super::*;
use crate::work::{CoderWork, HaloAgentSession};
use halo_backend::commands::halo_agent::{McpServerConfig, SkillInfo};

impl Bridge {
    /// Toggle the right-side Context rail (Session / Subagents / Skills & MCP /
    /// Artifacts / Sources).
    pub fn halo_agent_toggle_context(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_ensure_session();
            s.coder.agent_context_open = !s.coder.agent_context_open;
            if s.coder.agent_context_open {
                s.coder.agent_picker_open = false;
                s.coder.agent_approval_picker_open = false;
                s.coder.agent_completion = None;
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Fold or expand a Context rail section (`session`, `subagents`, `skills`,
    /// `mcp`, `artifacts`, `sources`).
    pub fn halo_agent_toggle_context_section(section: &str, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_toggle_context_section(section);
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Refresh the skill catalog for the open project and current harness.
    ///
    /// OpenCode spawns a short-lived `opencode serve`, so this must never run
    /// from paint or a keystroke. The epoch guard keeps a superseded refresh
    /// from clobbering a newer one.
    pub fn halo_agent_refresh_skills(cx: &mut App) -> BridgeResult<()> {
        let backend = Self::backend(cx)?;
        let (root, harness, executable, epoch) = Self::state(cx).update(cx, |s| {
            s.coder.agent_skills_loading = true;
            s.coder.agent_skills_error = None;
            s.coder.agent_skills_epoch = s.coder.agent_skills_epoch.wrapping_add(1);
            (
                s.coder.root_path.clone(),
                s.coder.agent_harness.clone(),
                s.coder_settings.opencode_executable_path.clone(),
                s.coder.agent_skills_epoch,
            )
        });
        let Some(root) = root else {
            Self::state(cx).update(cx, |s| {
                s.coder.agent_skills_loading = false;
                s.coder.agent_skills.clear();
            });
            return Ok(());
        };
        Self::run_blocking(
            cx,
            move || {
                backend
                    .halo_agent_discover_skills(&root, &harness, executable.as_deref())
                    .map_err(|error| BridgeError::message(error.to_string()))
            },
            move |result, cx| {
                Self::apply_skill_catalog(epoch, result, cx);
            },
        );
        Ok(())
    }

    fn apply_skill_catalog(epoch: u64, result: BridgeResult<Vec<SkillInfo>>, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            if s.coder.agent_skills_epoch != epoch {
                return;
            }
            s.coder.agent_skills_loading = false;
            match result {
                Ok(skills) => {
                    s.coder.agent_skills = skills;
                    s.coder.agent_skills_error = None;
                }
                Err(error) => {
                    s.coder.agent_skills_error = Some(error.to_string());
                }
            }
            // A skill that disappeared from the catalog cannot stay enabled.
            let catalog = s.coder.agent_skills.clone();
            s.coder
                .agent_enabled_skills
                .retain(|id| catalog.iter().any(|skill| &skill.id == id));
            let enabled = s.coder.agent_enabled_skills.clone();
            if let Some(session) = active_session_mut(&mut s.coder) {
                session.skills = enabled;
            }
            s.commit_active_tab();
        });
    }

    /// Enable / disable one skill for the active chat.
    pub fn halo_agent_toggle_skill(id: &str, cx: &mut App) -> BridgeResult<()> {
        let id = id.to_string();
        Self::state(cx).update(cx, |s| {
            s.coder.agent_ensure_session();
            let enabled = &mut s.coder.agent_enabled_skills;
            match enabled.iter().position(|existing| existing == &id) {
                Some(index) => {
                    enabled.remove(index);
                }
                None => enabled.push(id),
            }
            let snapshot = s.coder.agent_enabled_skills.clone();
            if let Some(session) = active_session_mut(&mut s.coder) {
                session.skills = snapshot;
                session.updated_at = crate::work::agent_now_ms();
            }
            s.commit_active_tab();
        });
        Ok(())
    }

    /// Enable / disable one configured MCP server for the active chat.
    ///
    /// Only OpenCode consumes MCP; the switch is hidden/disabled elsewhere, and
    /// a globally disabled or incomplete server can never be turned on.
    pub fn halo_agent_toggle_mcp(name: &str, cx: &mut App) -> BridgeResult<()> {
        let name = name.to_string();
        Self::state(cx).update(cx, |s| {
            if s.coder.agent_harness != "opencode" {
                return;
            }
            if !s
                .coder_settings
                .mcp_servers
                .iter()
                .any(|server| server.name == name && server.enabled && server.is_configured())
            {
                return;
            }
            s.coder.agent_ensure_session();
            let enabled = &mut s.coder.agent_enabled_mcp;
            match enabled.iter().position(|existing| existing == &name) {
                Some(index) => {
                    enabled.remove(index);
                }
                None => enabled.push(name),
            }
            let snapshot = s.coder.agent_enabled_mcp.clone();
            if let Some(session) = active_session_mut(&mut s.coder) {
                session.mcp_servers = snapshot;
                session.updated_at = crate::work::agent_now_ms();
            }
            s.commit_active_tab();
        });
        Ok(())
    }
}

fn active_session_mut(coder: &mut CoderWork) -> Option<&mut HaloAgentSession> {
    let id = coder.agent_active_session_id.clone();
    coder
        .agent_sessions
        .iter_mut()
        .find(|session| session.id == id)
}

/// Case-insensitive filter with prefix matches first, then alphabetical.
pub(crate) fn rank_skills<'a>(
    skills: &'a [SkillInfo],
    query: &str,
    limit: usize,
) -> Vec<&'a SkillInfo> {
    let query = query.trim().to_lowercase();
    let mut ranked: Vec<(u8, &SkillInfo)> = skills
        .iter()
        .filter_map(|skill| {
            let name = skill.name.to_lowercase();
            let rank = if query.is_empty() {
                1
            } else if name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else if skill.description.to_lowercase().contains(&query) {
                2
            } else {
                return None;
            };
            Some((rank, skill))
        })
        .collect();
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill)
        .collect()
}

/// Catalog rows for the ids a chat enabled, in catalog order.
pub(super) fn resolve_run_skills(catalog: &[SkillInfo], enabled: &[String]) -> Vec<SkillInfo> {
    catalog
        .iter()
        .filter(|skill| enabled.iter().any(|id| id == &skill.id))
        .cloned()
        .collect()
}

/// Configured servers the chat enabled. Globally disabled or incomplete rows
/// are dropped even when a chat still has their name.
pub(super) fn resolve_run_mcp(
    servers: &[McpServerConfig],
    enabled: &[String],
) -> Vec<McpServerConfig> {
    servers
        .iter()
        .filter(|server| {
            server.enabled
                && server.is_configured()
                && enabled.iter().any(|name| name == &server.name)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_backend::commands::halo_agent::McpTransport;

    fn skill(id: &str, name: &str, description: &str) -> SkillInfo {
        SkillInfo {
            id: id.into(),
            name: name.into(),
            path: format!("/p/{id}/SKILL.md"),
            description: description.into(),
            scope: "project".into(),
        }
    }

    #[test]
    fn rank_skills_prefix_before_description() {
        let catalog = vec![
            skill("a", "deploy", "ship it"),
            skill("b", "testing", "deploy checks"),
            skill("c", "lint", "style"),
        ];
        let ranked = rank_skills(&catalog, "dep", 10);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].id, "a");
        assert_eq!(ranked[1].id, "b");
        assert!(rank_skills(&catalog, "zzz", 10).is_empty());
    }

    #[test]
    fn resolve_run_skills_keeps_catalog_order() {
        let catalog = vec![skill("a", "A", ""), skill("b", "B", "")];
        let rows = resolve_run_skills(&catalog, &["b".into()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "b");
    }

    #[test]
    fn resolve_run_mcp_drops_disabled_and_unselected() {
        let servers = vec![
            McpServerConfig {
                name: "fs".into(),
                command: "npx".into(),
                ..McpServerConfig::default()
            },
            McpServerConfig {
                name: "off".into(),
                command: "npx".into(),
                enabled: false,
                ..McpServerConfig::default()
            },
            McpServerConfig {
                name: "web".into(),
                transport: McpTransport::Remote,
                url: "https://mcp.example".into(),
                ..McpServerConfig::default()
            },
        ];
        let rows = resolve_run_mcp(&servers, &["fs".into(), "off".into()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "fs");
    }
}
