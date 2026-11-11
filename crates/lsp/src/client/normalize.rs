//! Normalising the loosely-typed JSON some servers return, plus text-edit application.

use lsp_types::{
    CodeAction, CodeActionOrCommand, Location, LocationLink, Position, Range, TextEdit,
};
use serde_json::Value;

/// Servers vary between `CodeAction[]` and `Command[]`; keep only real code actions.
pub(super) fn normalize_code_actions(value: Value) -> Vec<CodeAction> {
    if value.is_null() {
        return Vec::new();
    }
    let Ok(items) = serde_json::from_value::<Vec<CodeActionOrCommand>>(value) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            CodeActionOrCommand::CodeAction(action) => Some(action),
            CodeActionOrCommand::Command(_) => None,
        })
        .collect()
}

/// `textDocument/definition` may answer with a single link, a link array, a
/// single location or a location array. Reduce all four to `Vec<LocationLink>`.
pub(super) fn normalize_definition(value: Value) -> Vec<LocationLink> {
    if value.is_null() {
        return Vec::new();
    }
    if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(value.clone()) {
        return links;
    }
    if let Ok(link) = serde_json::from_value::<LocationLink>(value.clone()) {
        return vec![link];
    }
    if let Ok(locs) = serde_json::from_value::<Vec<Location>>(value.clone()) {
        return locs.into_iter().map(location_to_link).collect();
    }
    if let Ok(loc) = serde_json::from_value::<Location>(value) {
        return vec![location_to_link(loc)];
    }
    Vec::new()
}

fn location_to_link(loc: Location) -> LocationLink {
    LocationLink {
        origin_selection_range: None,
        target_uri: loc.uri,
        target_range: loc.range,
        target_selection_range: loc.range,
    }
}

/// Apply LSP text edits to a full document string (edits applied in reverse start order).
pub fn apply_text_edits(text: &str, mut edits: Vec<TextEdit>) -> String {
    edits.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
    });
    let mut out = text.to_string();
    for edit in edits {
        let Some((start, end)) = lsp_range_byte_offsets(&out, edit.range) else {
            continue;
        };
        out.replace_range(start..end, &edit.new_text);
    }
    out
}

fn lsp_range_byte_offsets(text: &str, range: Range) -> Option<(usize, usize)> {
    let start = lsp_position_byte_offset(text, range.start)?;
    let end = lsp_position_byte_offset(text, range.end)?;
    if start <= end {
        Some((start, end))
    } else {
        Some((end, start))
    }
}

fn lsp_position_byte_offset(text: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(i);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
            if line > pos.line {
                return Some(i);
            }
        } else {
            col += 1;
        }
    }
    if line == pos.line && col >= pos.character {
        // End of last line / past end → EOF
        return Some(text.len());
    }
    if line < pos.line {
        return Some(text.len());
    }
    Some(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_location_array() {
        let value = json!([{
            "uri": "file:///tmp/a.rs",
            "range": {
                "start": {"line": 1, "character": 2},
                "end": {"line": 1, "character": 5}
            }
        }]);
        let links = normalize_definition(value);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_selection_range.start.line, 1);
    }

    #[test]
    fn apply_text_edits_replaces_range() {
        let text = "hello world\n";
        let edits = vec![TextEdit {
            range: Range {
                start: Position::new(0, 6),
                end: Position::new(0, 11),
            },
            new_text: "Rust".into(),
        }];
        assert_eq!(apply_text_edits(text, edits), "hello Rust\n");
    }
}
