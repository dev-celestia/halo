//! `@` file and `/` skill completions logic and token parsing for agent prompt composer.

use super::super::*;
use crate::HaloAgentCompletionKind;

impl Bridge {
    /// Refresh `@` / `/` completion from draft text and cursor byte offset.
    pub fn halo_agent_completion_sync(text: &str, cursor: usize, cx: &mut App) -> BridgeResult<()> {
        use crate::{HaloAgentCompletion, HaloAgentCompletionItem, HaloAgentCompletionKind};

        Self::halo_agent_set_draft(text, cx);
        let Some(token) = completion::active_token(text, cursor) else {
            Self::state(cx).update(cx, |s| s.coder.agent_completion = None);
            return Ok(());
        };

        let root = Self::state(cx).read(cx).coder.root_path.clone();
        let harness = Self::state(cx).read(cx).coder.agent_harness.clone();
        let backend = Self::backend(cx)?;

        let (kind, items) = match token.kind {
            completion::TokenKind::File => {
                let Some(root) = root.as_deref() else {
                    Self::state(cx).update(cx, |s| s.coder.agent_completion = None);
                    return Ok(());
                };
                let matches = backend.halo_agent_search_files(root, &token.query)?;
                let items = matches
                    .into_iter()
                    .map(|m| HaloAgentCompletionItem {
                        label: m.path.rsplit('/').next().unwrap_or(&m.path).to_string(),
                        detail: m.path.clone(),
                        insert: if m.is_dir {
                            format!("@{}/", m.path.trim_end_matches('/'))
                        } else {
                            format!("@{}", m.path)
                        },
                        is_dir: m.is_dir,
                    })
                    .collect();
                (HaloAgentCompletionKind::File, items)
            }
            completion::TokenKind::Skill => {
                // Prefer the Context rail's catalog; only shell out when it has
                // never been loaded for this project.
                let cached = Self::state(cx).read(cx).coder.agent_skills.clone();
                let skills = if cached.is_empty() {
                    match root.as_deref() {
                        Some(root) => backend.halo_agent_list_skills(root, &harness)?,
                        None => Vec::new(),
                    }
                } else {
                    cached
                };
                let items = super::skills::rank_skills(&skills, &token.query, 40)
                    .into_iter()
                    .map(|s| HaloAgentCompletionItem {
                        label: s.name.clone(),
                        detail: s.description.clone(),
                        insert: format!("/{}", s.name),
                        is_dir: false,
                    })
                    .collect();
                (HaloAgentCompletionKind::Skill, items)
            }
        };

        Self::state(cx).update(cx, |s| {
            s.coder.agent_completion = Some(HaloAgentCompletion {
                kind,
                query: token.query,
                token_start: token.range.start,
                token_end: token.range.end,
                selected: 0,
                items,
            });
        });
        Ok(())
    }

    pub fn halo_agent_completion_dismiss(cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            s.coder.agent_completion = None;
        });
        Ok(())
    }

    pub fn halo_agent_completion_move(delta: i32, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(comp) = s.coder.agent_completion.as_mut() {
                if comp.items.is_empty() {
                    return;
                }
                let len = comp.items.len() as i32;
                let next = (comp.selected as i32 + delta).rem_euclid(len) as usize;
                comp.selected = next;
            }
        });
        Ok(())
    }

    pub fn halo_agent_completion_select(index: usize, cx: &mut App) -> BridgeResult<()> {
        Self::state(cx).update(cx, |s| {
            if let Some(comp) = s.coder.agent_completion.as_mut() {
                if index < comp.items.len() {
                    comp.selected = index;
                }
            }
        });
        Ok(())
    }

    /// Accept the selected completion into `draft`.
    ///
    /// Returns `(new_draft, cursor_byte_offset)` for the UI textarea.
    pub fn halo_agent_completion_accept(
        draft: &str,
        cx: &mut App,
    ) -> BridgeResult<Option<(String, usize)>> {
        let comp = Self::state(cx).read(cx).coder.agent_completion.clone();
        let Some(comp) = comp else {
            return Ok(None);
        };
        let Some(item) = comp.items.get(comp.selected).cloned() else {
            Self::state(cx).update(cx, |s| s.coder.agent_completion = None);
            return Ok(None);
        };
        if comp.token_end > draft.len()
            || comp.token_start > comp.token_end
            || !draft.is_char_boundary(comp.token_start)
            || !draft.is_char_boundary(comp.token_end)
        {
            Self::state(cx).update(cx, |s| s.coder.agent_completion = None);
            return Ok(None);
        }
        let mut insert = item.insert;
        if !insert.ends_with(' ') && !insert.ends_with('/') {
            insert.push(' ');
        }
        // Accepting `/name` opts that skill into the chat, so the run attaches
        // it without a second trip to the Context rail.
        let enable_skill = (comp.kind == HaloAgentCompletionKind::Skill)
            .then(|| {
                Self::state(cx)
                    .read(cx)
                    .coder
                    .agent_skills
                    .iter()
                    .find(|skill| skill.name == item.label)
                    .map(|skill| skill.id.clone())
            })
            .flatten();
        let mut new_draft = String::with_capacity(draft.len() + insert.len());
        new_draft.push_str(&draft[..comp.token_start]);
        new_draft.push_str(&insert);
        let cursor = new_draft.len();
        new_draft.push_str(&draft[comp.token_end..]);
        Self::state(cx).update(cx, |s| {
            s.coder.agent_completion = None;
            if let Some(id) = enable_skill {
                if !s.coder.agent_enabled_skills.contains(&id) {
                    s.coder.agent_enabled_skills.push(id);
                    let snapshot = s.coder.agent_enabled_skills.clone();
                    let session_id = s.coder.agent_active_session_id.clone();
                    if let Some(session) = s
                        .coder
                        .agent_sessions
                        .iter_mut()
                        .find(|session| session.id == session_id)
                    {
                        session.skills = snapshot;
                    }
                }
            }
        });
        Ok(Some((new_draft, cursor)))
    }
}

/// Token parsers for `@` / `/`.
pub(crate) mod completion {
    use std::ops::Range;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TokenKind {
        File,
        Skill,
    }

    #[derive(Clone, Debug)]
    pub struct ActiveToken {
        pub kind: TokenKind,
        pub range: Range<usize>,
        pub query: String,
    }

    pub fn active_token(text: &str, cursor: usize) -> Option<ActiveToken> {
        mention_token(text, cursor)
            .map(|t| ActiveToken {
                kind: TokenKind::File,
                range: t.range,
                query: t.query,
            })
            .or_else(|| {
                slash_token(text, cursor).map(|t| ActiveToken {
                    kind: TokenKind::Skill,
                    range: t.range,
                    query: t.query,
                })
            })
    }

    struct MentionToken {
        range: Range<usize>,
        query: String,
    }

    fn mention_token(text: &str, cursor: usize) -> Option<MentionToken> {
        if cursor > text.len() || !text.is_char_boundary(cursor) {
            return None;
        }
        let token_start = text[..cursor]
            .char_indices()
            .rev()
            .find_map(|(at, ch)| ch.is_whitespace().then_some(at + ch.len_utf8()))
            .unwrap_or(0);
        let relative_at = text[token_start..cursor].rfind('@')?;
        let at = token_start + relative_at;
        let valid_boundary = at == 0
            || text[..at]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '[' | '{' | '>'));
        if text[at + 1..cursor].contains('@') || !valid_boundary {
            return None;
        }
        let closing = text[..at].chars().next_back().and_then(|ch| match ch {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            _ => None,
        });
        let end = text[cursor..]
            .char_indices()
            .find_map(|(offset, ch)| {
                (ch.is_whitespace() || Some(ch) == closing).then_some(cursor + offset)
            })
            .unwrap_or(text.len());
        if closing.is_some_and(|ch| text[at + 1..cursor].contains(ch)) {
            return None;
        }
        Some(MentionToken {
            range: at..end,
            query: text[at + 1..cursor].to_string(),
        })
    }

    fn slash_token(text: &str, cursor: usize) -> Option<MentionToken> {
        if cursor > text.len() || !text.is_char_boundary(cursor) {
            return None;
        }
        let start = text[..cursor]
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '>'))
            .map_or(0, |(i, c)| i + c.len_utf8());
        if !text[start..cursor].starts_with('/') {
            return None;
        }
        let query = &text[start + 1..cursor];
        let name_ok = |ch: char| ch.is_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.');
        if !query.chars().all(name_ok) {
            return None;
        }
        let end = text[start + 1..]
            .char_indices()
            .find_map(|(offset, ch)| (!name_ok(ch)).then_some(start + 1 + offset))
            .unwrap_or(text.len());
        if text.get(end..end + 1) == Some("/") {
            return None;
        }
        Some(MentionToken {
            range: start..end,
            query: query.to_string(),
        })
    }
}
