//! Repairs a language server's answer for the engine's inserter.
//!
//! `EditorState::insert_completion` reads a [`CompletionItem`] three ways, and
//! two of them used to insert the wrong text:
//!
//! 1. **`text_edit`** — the server names the range and the text. Correct, and
//!    left alone here.
//! 2. **`insert_text` only** — the engine appends at `range.end..range.end`,
//!    i.e. *after* the identifier instead of over it, so accepting `println`
//!    for a typed `pri` produced `priprintln`. Several servers (pyright, gopls
//!    for some items) send this shape.
//! 3. **`label` only** — the engine replaces its *latched query* range, which
//!    for `foo.ba` starts at the `f`: accepting `bar` deleted `foo.`.
//!
//! [`repair`] gives every item an explicit edit range over the identifier under
//! the caret, so all three shapes land identically. It also strips snippet
//! placeholders, because this client advertises no snippet support but not
//! every server believes it.

use std::collections::HashSet;

use gpui_component::input::Rope;
use lsp_types::{CompletionItem, CompletionTextEdit, InsertTextFormat, TextEdit};

use super::words;

/// Hard ceiling on what reaches the menu, however many items a server returns.
const TOTAL_LIMIT: usize = 400;

/// Rewrite `items` in place so every one of them inserts correctly.
pub(super) fn repair(items: &mut Vec<CompletionItem>, text: &Rope, offset: usize) {
    let offset = offset.min(text.len());
    let (start, _) = words::prefix_before(text, offset);
    let range = words::replace_range(text, start, offset);

    for item in items.iter_mut() {
        let snippet = item.insert_text_format == Some(InsertTextFormat::SNIPPET);

        if let Some(edit) = item.text_edit.as_mut() {
            match edit {
                CompletionTextEdit::Edit(edit) => {
                    if snippet {
                        edit.new_text = strip_snippet(&edit.new_text);
                    }
                }
                CompletionTextEdit::InsertAndReplace(edit) => {
                    if snippet {
                        edit.new_text = strip_snippet(&edit.new_text);
                    }
                }
            }
        }
        if let Some(insert_text) = item.insert_text.as_mut() {
            if snippet {
                *insert_text = strip_snippet(insert_text);
            }
        }

        if item.text_edit.is_none() {
            let new_text = item
                .insert_text
                .take()
                .unwrap_or_else(|| item.label.clone());
            item.text_edit = Some(CompletionTextEdit::Edit(TextEdit { range, new_text }));
        }

        if item.filter_text.is_none() {
            item.filter_text = Some(item.label.clone());
        }
        item.insert_text = None;
    }

    items.retain(|item| !item.label.is_empty());
    items.truncate(TOTAL_LIMIT);
}

/// Append `local` after `server`, skipping labels the server already offered.
///
/// `limit` bounds only the appended tail: the server's own ranking is the one
/// worth reading, so its list is never reordered or trimmed in favour of local
/// words.
pub(super) fn merge(
    mut server: Vec<CompletionItem>,
    local: Vec<CompletionItem>,
    limit: usize,
) -> Vec<CompletionItem> {
    let mut seen: HashSet<String> = server.iter().map(|item| item.label.clone()).collect();
    let mut added = 0;

    for item in local {
        if added >= limit || server.len() >= TOTAL_LIMIT {
            break;
        }
        if !seen.insert(item.label.clone()) {
            continue;
        }
        server.push(item);
        added += 1;
    }
    server
}

/// Replace LSP snippet syntax with the text a user would have typed.
///
/// `${1:name}` keeps `name` (it is the default the user would edit, and the
/// caret cannot be moved into it), `$1` / `${1}` / `$TM_FILENAME` vanish, and
/// `\$` becomes a literal `$`.
fn strip_snippet(text: &str) -> String {
    if !text.contains('$') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '\\' if chars.get(i + 1) == Some(&'$') => {
                out.push('$');
                i += 2;
            }
            '$' => match chars.get(i + 1).copied() {
                Some('{') => match closing_brace(&chars, i + 1) {
                    Some(end) => {
                        let inner: String = chars[i + 2..end].iter().collect();
                        if let Some((_, default)) = inner.split_once(':') {
                            out.push_str(&strip_snippet(default));
                        }
                        i = end + 1;
                    }
                    // Unterminated `${` — keep the `$` rather than eat the tail.
                    None => {
                        out.push('$');
                        i += 1;
                    }
                },
                Some(digit) if digit.is_ascii_digit() => {
                    i += 1;
                    while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
                        i += 1;
                    }
                }
                Some(start) if start.is_alphanumeric() || start == '_' => {
                    i += 1;
                    while chars
                        .get(i)
                        .is_some_and(|c| c.is_alphanumeric() || *c == '_')
                    {
                        i += 1;
                    }
                }
                _ => {
                    out.push('$');
                    i += 1;
                }
            },
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    out
}

/// Index of the `}` matching the `{` at `open`, if any.
fn closing_brace(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (ix, ch) in chars.iter().enumerate().skip(open) {
        match *ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(ix);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{merge, strip_snippet};
    use lsp_types::CompletionItem;

    fn item(label: &str) -> CompletionItem {
        CompletionItem {
            label: label.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn placeholders_are_dropped() {
        assert_eq!(strip_snippet("println!($0)"), "println!()");
        assert_eq!(strip_snippet("fn ${1:name}()"), "fn name()");
        assert_eq!(strip_snippet("$1 and $2"), " and ");
        assert_eq!(strip_snippet("${1}"), "");
    }

    #[test]
    fn nested_defaults_keep_their_text() {
        assert_eq!(
            strip_snippet("let ${1:x} = ${2:${3:value}};"),
            "let x = value;"
        );
    }

    #[test]
    fn variables_are_dropped_but_literal_dollars_survive() {
        assert_eq!(strip_snippet("$TM_FILENAME"), "");
        assert_eq!(strip_snippet(r"cost is \$5"), "cost is $5");
        assert_eq!(strip_snippet("a $ b"), "a $ b");
    }

    #[test]
    fn unterminated_placeholder_does_not_eat_the_tail() {
        assert_eq!(strip_snippet("open ${1:name"), "open ${1:name");
    }

    #[test]
    fn text_without_snippets_is_untouched() {
        assert_eq!(strip_snippet("hello"), "hello");
    }

    #[test]
    fn merge_keeps_the_server_order_and_skips_duplicates() {
        let server = vec![item("alpha"), item("beta")];
        let local = vec![item("beta"), item("gamma"), item("delta")];
        let merged = merge(server, local, 10);
        let labels: Vec<&str> = merged.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["alpha", "beta", "gamma", "delta"]);
    }

    #[test]
    fn merge_bounds_only_the_local_tail() {
        let server = vec![item("alpha")];
        let local = vec![item("one"), item("two"), item("three")];
        let merged = merge(server, local, 2);
        let labels: Vec<&str> = merged.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["alpha", "one", "two"]);
    }
}
