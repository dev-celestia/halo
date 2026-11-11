//! Pure transcript bookkeeping. Scrolling itself belongs to Celestia/GPUI.
use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChatKey {
    pub workspace: String,
    pub chat: String,
    /// `Some(child session id)` for a subagent drawer transcript.
    pub subagent: Option<String>,
}

impl ChatKey {
    pub fn child(
        workspace: impl Into<String>,
        chat: impl Into<String>,
        session: impl Into<String>,
    ) -> Self {
        Self {
            workspace: workspace.into(),
            chat: chat.into(),
            subagent: Some(session.into()),
        }
    }

    pub fn part_id(&self, message: &str, part: usize) -> String {
        // Length-prefixed components, not ambiguous concatenation of user IDs.
        let base = format!(
            "agent:{}:{}:{}:{}:{}:{}:{part}",
            self.workspace.len(),
            self.workspace,
            self.chat.len(),
            self.chat,
            message.len(),
            message
        );
        // Root ids stay byte-identical; only children carry a suffix so two
        // transcripts never share an element id.
        match &self.subagent {
            Some(session) => format!("{base}@{}:{session}", session.len()),
            None => base,
        }
    }
}

/// Minimal splice, retaining both unchanged ends (and their scroll anchors).
#[derive(Debug, PartialEq, Eq)]
pub struct RowSplice {
    pub old: Range<usize>,
    pub inserted: usize,
}

pub fn row_splice(old: &[String], new: &[String]) -> Option<RowSplice> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    if prefix == old.len() && prefix == new.len() {
        return None;
    }
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    Some(RowSplice {
        old: prefix..old.len() - suffix,
        inserted: new.len() - prefix - suffix,
    })
}

pub fn is_live_part(active: Option<(&str, usize)>, message: &str, part: usize) -> bool {
    active == Some((message, part))
}

pub const PREVIEW_BYTES: usize = 24 * 1024;
pub const PREVIEW_LINES: usize = 200;

/// Bound the work done by RenderOnce CodeBlock (which splits every line).
/// This is a preview boundary, NOT a Markdown parser. The AST and copy keep source.
pub fn preview_end(source: &str) -> usize {
    let mut end = source.len().min(PREVIEW_BYTES);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    if let Some((offset, _)) = source[..end].match_indices('\n').nth(PREVIEW_LINES - 1) {
        end = offset;
    }
    end
}

/// Cache accounting includes an estimate for parsed nodes/source copies, not just
/// the input buffer. Pinned selection/live entries are the only budget exception.
pub const CACHE_ENTRIES: usize = 128;
pub const CACHE_BYTES: usize = 8 * 1024 * 1024;
pub fn text_cost(source_bytes: usize) -> usize {
    source_bytes.saturating_mul(4).saturating_add(4096)
}
