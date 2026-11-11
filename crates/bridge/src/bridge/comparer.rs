//! Comparer — side-by-side text diffing with lines / words / chars
//! granularity. Pure computation like the reference implementation
//! (`pages/comparer`, built on npm `diff`); the port uses the `similar`
//! crate. No backend commands, no persistence.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use similar::{ChangeTag, DiffTag, TextDiff};

use serde::{Deserialize, Serialize};

use super::*;

/// Bumped on every edit / explicit Edit so a pending idle timer is cancelled.
static DIFF_IDLE_SEQ: AtomicU64 = AtomicU64::new(0);
/// Pause after the last keystroke before flipping the two columns to the
/// highlighted diff view.
const DIFF_IDLE_MS: u64 = 400;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffMode {
    #[default]
    Lines,
    Words,
    Chars,
}

impl DiffMode {
    pub const ALL: [DiffMode; 3] = [Self::Lines, Self::Words, Self::Chars];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lines => "Lines",
            Self::Words => "Words",
            Self::Chars => "Chars",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffPartKind {
    Unchanged,
    Added,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffInlinePart {
    pub text: String,
    pub kind: DiffPartKind,
}

/// One aligned row of the two-column diff table. `empty` sides render as a
/// blank gutter cell with no line number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffRow {
    pub left_num: Option<usize>,
    pub right_num: Option<usize>,
    pub left_content: String,
    pub right_content: String,
    pub left_kind: RowKind,
    pub right_kind: RowKind,
    pub inline_left: Vec<DiffInlinePart>,
    pub inline_right: Vec<DiffInlinePart>,
    /// Both sides changed on this aligned row (yellow tint).
    pub paired: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowKind {
    Unchanged,
    Removed,
    Added,
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparerView {
    pub value_a: String,
    pub value_b: String,
    pub diff_mode: DiffMode,
    pub rows: Vec<DiffRow>,
    pub show_inputs: bool,
}

impl Default for ComparerView {
    fn default() -> Self {
        Self {
            value_a: String::new(),
            value_b: String::new(),
            diff_mode: DiffMode::Lines,
            rows: Vec::new(),
            // true = edit the two text columns; false = highlighted diff in
            // those same columns (toggled via View Diff / Edit Texts).
            show_inputs: true,
        }
    }
}

impl ComparerView {
    pub fn has_content(&self) -> bool {
        !self.value_a.is_empty() || !self.value_b.is_empty()
    }

    pub fn has_diff(&self) -> bool {
        self.rows
            .iter()
            .any(|r| r.left_kind == RowKind::Removed || r.right_kind == RowKind::Added)
    }
}

fn inline_parts(
    left: &str,
    right: &str,
    mode: DiffMode,
) -> (Vec<DiffInlinePart>, Vec<DiffInlinePart>) {
    let mut inline_left = Vec::new();
    let mut inline_right = Vec::new();
    if mode == DiffMode::Lines || left.is_empty() || right.is_empty() {
        return (inline_left, inline_right);
    }
    let diff = match mode {
        DiffMode::Words => TextDiff::configure().diff_words(left, right),
        DiffMode::Chars => TextDiff::configure().diff_chars(left, right),
        DiffMode::Lines => unreachable!(),
    };
    for change in diff.iter_all_changes() {
        let text = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => {
                inline_left.push(DiffInlinePart {
                    text: text.clone(),
                    kind: DiffPartKind::Unchanged,
                });
                inline_right.push(DiffInlinePart {
                    text,
                    kind: DiffPartKind::Unchanged,
                });
            }
            ChangeTag::Delete => inline_left.push(DiffInlinePart {
                text,
                kind: DiffPartKind::Removed,
            }),
            ChangeTag::Insert => inline_right.push(DiffInlinePart {
                text,
                kind: DiffPartKind::Added,
            }),
        }
    }
    (inline_left, inline_right)
}

/// Build the aligned row model from both inputs at the current granularity.
pub fn build_rows(value_a: &str, value_b: &str, mode: DiffMode) -> Vec<DiffRow> {
    if value_a.is_empty() && value_b.is_empty() {
        return Vec::new();
    }

    // Ignore a single trailing newline on either side (reference
    // `ignoreNewlineAtEof`) so `"a"` and `"a\n"` compare equal.
    let a = value_a.strip_suffix('\n').unwrap_or(value_a);
    let b = value_b.strip_suffix('\n').unwrap_or(value_b);
    let old_lines: Vec<&str> = a.split('\n').collect();
    let new_lines: Vec<&str> = b.split('\n').collect();

    let diff = TextDiff::configure().diff_lines(a, b);
    let mut rows = Vec::new();

    for op in diff.ops() {
        let old = &old_lines[op.old_range().clone()];
        let new = &new_lines[op.new_range().clone()];
        match op.tag() {
            DiffTag::Equal => {
                for line in new {
                    rows.push(DiffRow {
                        left_num: None,
                        right_num: None,
                        left_content: (*line).to_string(),
                        right_content: (*line).to_string(),
                        left_kind: RowKind::Unchanged,
                        right_kind: RowKind::Unchanged,
                        inline_left: Vec::new(),
                        inline_right: Vec::new(),
                        paired: false,
                    });
                }
            }
            DiffTag::Delete => {
                for line in old {
                    rows.push(DiffRow {
                        left_num: None,
                        right_num: None,
                        left_content: (*line).to_string(),
                        right_content: String::new(),
                        left_kind: RowKind::Removed,
                        right_kind: RowKind::Empty,
                        inline_left: Vec::new(),
                        inline_right: Vec::new(),
                        paired: false,
                    });
                }
            }
            DiffTag::Insert => {
                for line in new {
                    rows.push(DiffRow {
                        left_num: None,
                        right_num: None,
                        left_content: String::new(),
                        right_content: (*line).to_string(),
                        left_kind: RowKind::Empty,
                        right_kind: RowKind::Added,
                        inline_left: Vec::new(),
                        inline_right: Vec::new(),
                        paired: false,
                    });
                }
            }
            DiffTag::Replace => {
                // Removed block + added block: zip the lines side by side,
                // computing inline parts per aligned pair.
                let len = old.len().max(new.len());
                for i in 0..len {
                    let left = old.get(i).copied().unwrap_or("");
                    let right = new.get(i).copied().unwrap_or("");
                    // Lines the diff grouped into a Replace but that are
                    // identical render as plain unchanged rows.
                    if left == right && !left.is_empty() {
                        rows.push(DiffRow {
                            left_num: None,
                            right_num: None,
                            left_content: left.to_string(),
                            right_content: right.to_string(),
                            left_kind: RowKind::Unchanged,
                            right_kind: RowKind::Unchanged,
                            inline_left: Vec::new(),
                            inline_right: Vec::new(),
                            paired: false,
                        });
                        continue;
                    }
                    let (inline_left, inline_right) = if left.is_empty() || right.is_empty() {
                        (Vec::new(), Vec::new())
                    } else {
                        inline_parts(left, right, mode)
                    };
                    rows.push(DiffRow {
                        left_num: None,
                        right_num: None,
                        left_content: left.to_string(),
                        right_content: right.to_string(),
                        left_kind: if left.is_empty() {
                            RowKind::Empty
                        } else {
                            RowKind::Removed
                        },
                        right_kind: if right.is_empty() {
                            RowKind::Empty
                        } else {
                            RowKind::Added
                        },
                        inline_left,
                        inline_right,
                        paired: !left.is_empty() && !right.is_empty(),
                    });
                }
            }
        }
    }

    // Assign line numbers per side (skip empty gutter rows).
    let mut left_num = 0usize;
    let mut right_num = 0usize;
    for row in &mut rows {
        row.left_num = (row.left_kind != RowKind::Empty).then(|| {
            left_num += 1;
            left_num
        });
        row.right_num = (row.right_kind != RowKind::Empty).then(|| {
            right_num += 1;
            right_num
        });
    }

    rows
}

/// Unified diff text for the Copy Diff action: `+ ` / `- ` / two-space prefix
/// per line.
pub fn unified_text(rows: &[DiffRow]) -> String {
    let mut out = String::new();
    for row in rows {
        for (content, kind) in [
            (&row.left_content, row.left_kind),
            (&row.right_content, row.right_kind),
        ] {
            let prefix = match kind {
                RowKind::Added => "+ ",
                RowKind::Removed => "- ",
                _ => "  ",
            };
            if kind != RowKind::Empty && !content.is_empty() {
                for line in content.split('\n') {
                    out.push_str(prefix);
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }
    out
}

impl Bridge {
    pub fn comparer_set_a(value: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.comparer.value_a = value.to_string();
            s.comparer.rows = build_rows(
                &s.comparer.value_a,
                &s.comparer.value_b,
                s.comparer.diff_mode,
            );
        });
        Self::comparer_schedule_idle_diff(cx);
    }

    pub fn comparer_set_b(value: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.comparer.value_b = value.to_string();
            s.comparer.rows = build_rows(
                &s.comparer.value_a,
                &s.comparer.value_b,
                s.comparer.diff_mode,
            );
        });
        Self::comparer_schedule_idle_diff(cx);
    }

    pub fn comparer_set_mode(mode: DiffMode, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.comparer.diff_mode = mode;
            s.comparer.rows = build_rows(&s.comparer.value_a, &s.comparer.value_b, mode);
        });
    }

    pub fn comparer_swap(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            std::mem::swap(&mut s.comparer.value_a, &mut s.comparer.value_b);
            s.comparer.rows = build_rows(
                &s.comparer.value_a,
                &s.comparer.value_b,
                s.comparer.diff_mode,
            );
        });
        Self::comparer_schedule_idle_diff(cx);
    }

    pub fn comparer_toggle_inputs(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.comparer.show_inputs = !s.comparer.show_inputs;
        });
    }

    /// Return to the editable text columns (cancels a pending idle flip).
    pub fn comparer_edit(cx: &mut App) {
        DIFF_IDLE_SEQ.fetch_add(1, Ordering::SeqCst);
        Self::state(cx).update(cx, |s| {
            s.comparer.show_inputs = true;
        });
    }

    pub fn comparer_clear(cx: &mut App) {
        DIFF_IDLE_SEQ.fetch_add(1, Ordering::SeqCst);
        Self::state(cx).update(cx, |s| {
            s.comparer.value_a.clear();
            s.comparer.value_b.clear();
            s.comparer.rows.clear();
            s.comparer.show_inputs = true;
        });
    }

    /// Keep editors visible while typing; after [`DIFF_IDLE_MS`] of quiet with
    /// both sides non-empty, flip the two columns to the highlighted diff.
    fn comparer_schedule_idle_diff(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.comparer.show_inputs = true;
        });
        let seq = DIFF_IDLE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            executor.timer(Duration::from_millis(DIFF_IDLE_MS)).await;
            if DIFF_IDLE_SEQ.load(Ordering::SeqCst) != seq {
                return;
            }
            let _ = cx.update(|cx| {
                Self::state(cx).update(cx, |s| {
                    if !s.comparer.value_a.is_empty() && !s.comparer.value_b.is_empty() {
                        s.comparer.show_inputs = false;
                    }
                });
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inputs_produce_no_rows() {
        assert!(build_rows("", "", DiffMode::Lines).is_empty());
    }

    #[test]
    fn identical_inputs_are_all_unchanged() {
        let rows = build_rows("a\nb", "a\nb", DiffMode::Lines);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.left_kind == RowKind::Unchanged));
        assert!(rows.iter().all(|r| r.left_num == r.right_num));
    }

    #[test]
    fn pure_add_and_remove_stack_on_their_sides() {
        let rows = build_rows("a", "a\nb", DiffMode::Lines);
        let added = rows.last().unwrap();
        assert_eq!(added.right_kind, RowKind::Added);
        assert_eq!(added.left_kind, RowKind::Empty);
        assert_eq!(added.left_num, None);
        assert_eq!(added.right_num, Some(2));
    }

    #[test]
    fn replaced_lines_pair_with_inline_parts() {
        let rows = build_rows("hello world\nx", "hello there\nx", DiffMode::Words);
        let paired: Vec<_> = rows.iter().filter(|r| r.paired).collect();
        assert_eq!(paired.len(), 1);
        let row = paired[0];
        assert_eq!(row.left_kind, RowKind::Removed);
        assert_eq!(row.right_kind, RowKind::Added);
        // Inline: "hello " unchanged, "world" removed, "there" added.
        assert!(row
            .inline_left
            .iter()
            .any(|p| p.kind == DiffPartKind::Removed && p.text == "world"));
        assert!(row
            .inline_right
            .iter()
            .any(|p| p.kind == DiffPartKind::Added && p.text == "there"));
        assert!(row
            .inline_left
            .iter()
            .any(|p| p.kind == DiffPartKind::Unchanged && p.text.starts_with("hello")));
    }

    #[test]
    fn lines_mode_has_no_inline_parts() {
        let rows = build_rows("hello world", "hello there", DiffMode::Lines);
        assert!(rows
            .iter()
            .all(|r| r.inline_left.is_empty() && r.inline_right.is_empty()));
    }

    #[test]
    fn chars_mode_splits_characters() {
        let rows = build_rows("abc", "abd", DiffMode::Chars);
        let row = rows.iter().find(|r| r.paired).expect("paired row");
        assert!(row
            .inline_left
            .iter()
            .any(|p| p.text == "c" && p.kind == DiffPartKind::Removed));
        assert!(row
            .inline_right
            .iter()
            .any(|p| p.text == "d" && p.kind == DiffPartKind::Added));
    }

    #[test]
    fn unified_text_marks_adds_and_removes() {
        let rows = build_rows("a\nb", "a\nc", DiffMode::Lines);
        let text = unified_text(&rows);
        assert!(text.contains("  a\n"));
        assert!(text.contains("- b\n"));
        assert!(text.contains("+ c\n"));
    }

    #[test]
    fn line_numbers_skip_empty_gutters() {
        let rows = build_rows("a", "a\nb\nc", DiffMode::Lines);
        let added_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.right_kind == RowKind::Added)
            .collect();
        assert_eq!(added_rows.len(), 2);
        assert_eq!(added_rows[0].right_num, Some(2));
        assert_eq!(added_rows[1].right_num, Some(3));
    }
}
