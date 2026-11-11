//! The local completion source: words already in the buffer, plus keywords.
//!
//! Runs off the UI thread (`Autofill::completions` hands it a cloned rope), so
//! the budget below bounds latency rather than frame time. A buffer under the
//! budget is scanned whole; a larger one contributes its first
//! [`SCAN_BUDGET`] characters, which is enough vocabulary for a menu that shows
//! sixty rows.

use std::collections::HashMap;

use gpui_component::input::{Rope, RopeExt};
use lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, Range, TextEdit};

use super::keywords;
use super::rank;

/// Characters walked before the scan gives up on the rest of the buffer.
const SCAN_BUDGET: usize = 400_000;

/// Distinct matching words kept before ranking.
///
/// A menu of sixty rows cannot use more, and the cap keeps a pathological
/// buffer (minified JSON, a bundled JS file) from turning the scan into a
/// sort of everything.
const MATCH_LIMIT: usize = 4_000;

/// Shortest identifier worth offering: below this the menu is mostly noise.
const MIN_WORD_LEN: usize = 3;

/// Longest identifier tracked, so a run of punctuation-free text cannot grow
/// the accumulator without bound.
const MAX_WORD_LEN: usize = 64;

/// Longest identifier prefix read back from the caret.
const MAX_PREFIX_CHARS: usize = 64;

/// Keywords rank a little above a document word of equal match strength: they
/// are what the language means, not what this one buffer happens to contain.
const KEYWORD_BONUS: i32 = 40;

/// Completion items for the caret at `offset` in `text`.
///
/// `manual` marks a user-invoked request (⌃Space). An empty prefix only
/// produces keywords, so an automatic trigger after `foo.` opens nothing while
/// the manual chord still lists what the language offers.
pub(super) fn complete(
    text: &Rope,
    offset: usize,
    language: &str,
    manual: bool,
) -> Vec<CompletionItem> {
    if offset > text.len() {
        return Vec::new();
    }
    let (start, prefix) = prefix_before(text, offset);
    if prefix.is_empty() && !manual {
        return Vec::new();
    }

    // (score, frequency, label, kind). Sorted once, then built into items.
    let mut scored: Vec<(i32, u32, String, CompletionItemKind)> = Vec::new();

    for &keyword in keywords::for_language(language) {
        if let Some(score) = rank::score(keyword, prefix.as_str()) {
            scored.push((
                score + KEYWORD_BONUS,
                u32::MAX,
                keyword.to_string(),
                CompletionItemKind::KEYWORD,
            ));
        }
    }

    if !prefix.is_empty() {
        let mut words: HashMap<String, (i32, u32)> = HashMap::new();
        scan_words(text, prefix.as_str(), &mut words);
        for (word, (score, count)) in words {
            scored.push((score, count, word, CompletionItemKind::TEXT));
        }
    }

    // Strongest match first, then the word this buffer uses most, then
    // alphabetical so the order is stable across keystrokes.
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    scored.truncate(super::LOCAL_ONLY_LIMIT);

    let range = replace_range(text, start, offset);
    scored
        .into_iter()
        .map(|(score, _, label, kind)| item(label, kind, score, range.clone()))
        .collect()
}

/// The identifier immediately before `offset`: where it starts, and its text.
///
/// Byte offsets throughout: this rope is byte-indexed, and `chars_at` walks
/// backwards from a byte offset in O(1), so a long identifier costs nothing
/// until the run actually ends.
pub(super) fn prefix_before(text: &Rope, offset: usize) -> (usize, String) {
    // `chars_at` panics past the end; the caret never is, but this is also
    // called from the repair path with whatever offset the engine hands over.
    let offset = offset.min(text.len());
    let len: usize = text
        .chars_at(offset)
        .reversed()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .take(MAX_PREFIX_CHARS)
        .map(char::len_utf8)
        .sum();
    let start = offset - len;
    (start, text.slice(start..offset).to_string())
}

/// The document range a completion replaces: the identifier under the caret.
///
/// The engine's own fallback range is the *latched query* start, which for
/// `foo.ba` is the `f` — accepting `bar` there would delete `foo.`. Every item
/// this module returns carries an explicit edit range instead.
pub(super) fn replace_range(text: &Rope, start: usize, end: usize) -> Range {
    Range {
        start: text.offset_to_position(start),
        end: text.offset_to_position(end),
    }
}

fn item(label: String, kind: CompletionItemKind, score: i32, range: Range) -> CompletionItem {
    let detail = match kind {
        CompletionItemKind::KEYWORD => "keyword",
        _ => "word",
    };
    CompletionItem {
        filter_text: Some(label.clone()),
        sort_text: Some(format!("{:05}", 10_000 - score)),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: label.clone(),
        })),
        label,
        kind: Some(kind),
        detail: Some(detail.to_string()),
        ..Default::default()
    }
}

/// Walk the buffer and collect every word that could match `prefix`.
///
/// Matching happens during the scan rather than afterwards: a 400 KB file holds
/// tens of thousands of words, and keeping only the ones that can appear in the
/// menu keeps the map small enough that ranking is free.
fn scan_words(text: &Rope, prefix: &str, out: &mut HashMap<String, (i32, u32)>) {
    let mut word = String::new();
    let mut budget = SCAN_BUDGET;

    for ch in text.chars() {
        if budget == 0 || out.len() >= MATCH_LIMIT {
            break;
        }
        budget -= 1;

        if ch.is_alphanumeric() || ch == '_' {
            if word.len() < MAX_WORD_LEN {
                word.push(ch);
            }
            continue;
        }
        record(&mut word, prefix, out);
    }
    record(&mut word, prefix, out);
}

fn record(word: &mut String, prefix: &str, out: &mut HashMap<String, (i32, u32)>) {
    let usable = word.chars().count() >= MIN_WORD_LEN
        && !word.chars().next().is_some_and(|c| c.is_ascii_digit());
    match if usable {
        rank::score(word.as_str(), prefix)
    } else {
        None
    } {
        Some(score) => {
            let entry = out.entry(word.clone()).or_insert((score, 0));
            entry.1 = entry.1.saturating_add(1);
            word.clear();
        }
        None => word.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rope(text: &str) -> Rope {
        Rope::from(text)
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn prefix_is_the_identifier_before_the_caret() {
        let text = rope("let total_count = 0;");
        let (start, prefix) = prefix_before(&text, 15);
        assert_eq!(prefix, "total_count");
        assert_eq!(start, 4);
    }

    #[test]
    fn prefix_stops_at_a_member_access() {
        let text = rope("config.database_url");
        let (start, prefix) = prefix_before(&text, 19);
        assert_eq!(prefix, "database_url");
        assert_eq!(start, 7);
    }

    #[test]
    fn prefix_is_empty_between_tokens() {
        let text = rope("let x = ");
        let (_, prefix) = prefix_before(&text, 8);
        assert!(prefix.is_empty());
    }

    #[test]
    fn prefix_handles_multibyte_identifiers() {
        // `chars_at` takes a byte offset; handing it a char index here would
        // return the wrong run of text (or panic off a boundary).
        let text = rope("let 变量名 = 1;");
        let (start, prefix) = prefix_before(&text, 13);
        assert_eq!(prefix, "变量名");
        assert_eq!(start, 4);
    }

    #[test]
    fn words_come_from_the_buffer_and_are_ranked_by_match() {
        let text = rope("fn connect() {}\nfn connection_pool() {}\nfn disconnect() {}\n");
        // Caret at the end of the first `connect`.
        let items = complete(&text, 10, "rust", false);
        let found = labels(&items);

        assert!(found.contains(&"connection_pool"), "{found:?}");
        assert!(found.contains(&"disconnect"), "{found:?}");
        // The word already under the caret is never offered back.
        assert!(!found.contains(&"connect"), "{found:?}");

        // A prefix match outranks the substring `disconnect` only contains.
        let pool = found.iter().position(|l| *l == "connection_pool").unwrap();
        let disconnect = found.iter().position(|l| *l == "disconnect").unwrap();
        assert!(pool < disconnect, "{found:?}");
    }

    #[test]
    fn keywords_appear_for_the_language() {
        let text = rope("ret");
        let items = complete(&text, 3, "rust", false);
        assert!(labels(&items).contains(&"return"), "{:?}", labels(&items));
    }

    #[test]
    fn an_automatic_trigger_with_no_prefix_offers_nothing() {
        let text = rope("value.");
        assert!(complete(&text, 6, "rust", false).is_empty());
    }

    #[test]
    fn a_manual_trigger_with_no_prefix_offers_keywords() {
        let text = rope("value.");
        let items = complete(&text, 6, "rust", true);
        assert!(!items.is_empty());
        assert!(
            items
                .iter()
                .all(|item| item.kind == Some(CompletionItemKind::KEYWORD)),
            "{:?}",
            labels(&items)
        );
    }

    #[test]
    fn every_item_replaces_only_the_identifier() {
        let text = rope("config.database_url = database_url_for_you;");
        let items = complete(&text, 19, "rust", false);
        assert!(!items.is_empty());
        for item in &items {
            let Some(CompletionTextEdit::Edit(edit)) = &item.text_edit else {
                panic!("{} has no edit range", item.label);
            };
            assert_eq!(edit.range.start.character, 7, "{}", item.label);
            assert_eq!(edit.range.end.character, 19, "{}", item.label);
            assert_eq!(edit.new_text, item.label);
        }
    }

    #[test]
    fn a_result_is_bounded() {
        // Every rust keyword matches an empty prefix; the menu must still be
        // capped rather than listing the whole table.
        let text = rope("value.");
        let items = complete(&text, 6, "rust", true);
        assert_eq!(items.len(), super::super::LOCAL_ONLY_LIMIT);
    }

    #[test]
    fn numbers_are_not_offered() {
        let text = rope("1234 5678 9012");
        let items = complete(&text, 4, "plaintext", false);
        assert!(items.is_empty(), "{:?}", labels(&items));
    }
}
