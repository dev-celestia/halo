//! Match scoring for the local completion source.
//!
//! The engine shows items in the order the provider returns them, so ranking is
//! the provider's job. Scores are tiered far enough apart that a weaker kind of
//! match never overtakes a stronger one on length alone:
//!
//! | tier | match | range |
//! |------|-------|-------|
//! | 1000 | `label` starts with `prefix`, exact case | 800–1000 |
//! |  800 | `label` starts with `prefix`, ignoring case | 600–800 |
//! |  600 | `prefix` is a subsequence (`gfi` → `getFileInfo`) | 450–600 |
//! |  400 | `prefix` appears inside `label`, ignoring case | 200–400 |
//!
//! Within a tier the shorter completion wins: typing `ret` should offer
//! `return` before `return_value_from_the_thing`.

/// Relevance of `label` for the identifier `prefix` typed so far.
///
/// `None` means "do not offer it": either it cannot match, or it *is* the text
/// already under the caret, in which case accepting it would be a no-op.
pub(super) fn score(label: &str, prefix: &str) -> Option<i32> {
    if prefix.is_empty() {
        return Some(0);
    }
    if label == prefix {
        return None;
    }
    if label.starts_with(prefix) {
        return Some(1000 - slack(label, prefix));
    }
    if starts_with_ignore_case(label, prefix) {
        return Some(800 - slack(label, prefix));
    }
    if let Some(gaps) = subsequence_gaps(label, prefix) {
        // Every character the match skipped costs a point, so `fil` inside
        // `getFileInfo` (three skipped) ranks just under `gfi` (two skipped)
        // instead of tying with it. Capped so a long label still beats a
        // coincidental substring hit.
        return Some(600 - gaps.min(150));
    }
    if let Some(at) = find_ignore_case(label, prefix) {
        return Some(400 - (at as i32).min(200));
    }
    None
}

/// How much longer the completion is than what was typed, capped.
fn slack(label: &str, prefix: &str) -> i32 {
    (label.len().saturating_sub(prefix.len())).min(200) as i32
}

/// `starts_with`, comparing ASCII case-insensitively without allocating.
fn starts_with_ignore_case(label: &str, prefix: &str) -> bool {
    let mut label_chars = label.chars();
    for want in prefix.chars() {
        match label_chars.next() {
            Some(have) if have == want || have.eq_ignore_ascii_case(&want) => {}
            _ => return false,
        }
    }
    true
}

/// Byte offset of the first case-insensitive occurrence of `prefix` in `label`.
fn find_ignore_case(label: &str, prefix: &str) -> Option<usize> {
    label
        .char_indices()
        .map(|(ix, _)| ix)
        .find(|ix| starts_with_ignore_case(&label[*ix..], prefix))
}

/// Number of characters skipped when matching `prefix` as a subsequence.
///
/// `None` when the characters are not all present in order.
fn subsequence_gaps(label: &str, prefix: &str) -> Option<i32> {
    let mut gaps = 0;
    let mut label_chars = label.chars();
    for want in prefix.chars() {
        loop {
            let have = label_chars.next()?;
            if have == want || have.eq_ignore_ascii_case(&want) {
                break;
            }
            gaps += 1;
        }
    }
    Some(gaps)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn exact_prefix_outranks_everything_else() {
        let hit = score("return", "ret").expect("prefix match");
        let folded = score("Return", "ret").expect("case-folded prefix match");
        let inside = score("my_return", "ret").expect("substring match");
        assert!(hit > folded, "{hit} vs {folded}");
        assert!(folded > inside, "{folded} vs {inside}");
    }

    #[test]
    fn shorter_completions_win_within_a_tier() {
        let short = score("return", "ret").expect("match");
        let long = score("return_value_from", "ret").expect("match");
        assert!(short > long, "{short} vs {long}");
    }

    #[test]
    fn abbreviations_match_camel_case() {
        assert!(score("getFileInfo", "gfi").is_some());
        assert!(score("getFileInfo", "gfi") > score("getFileInfo", "fil"));
    }

    #[test]
    fn the_word_already_typed_is_not_offered() {
        assert_eq!(score("return", "return"), None);
    }

    #[test]
    fn unrelated_words_are_rejected() {
        assert_eq!(score("return", "xyz"), None);
        assert_eq!(score("ab", "abcd"), None);
    }

    #[test]
    fn empty_prefix_matches_everything() {
        assert_eq!(score("anything", ""), Some(0));
    }

    #[test]
    fn multibyte_prefix_does_not_panic() {
        // Byte slicing inside `find_ignore_case` must land on char boundaries.
        assert!(score("变量名", "变量").is_some());
        assert_eq!(score("变量名", "数组"), None);
    }
}
