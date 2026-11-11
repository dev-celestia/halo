//! Split (side-by-side) diff row alignment and intraline change detection.

use similar::{ChangeTag, TextDiff};
use std::ops::Range;

use super::model::{Hunk, LineKind};

#[derive(Clone, Debug)]
pub(crate) struct SplitCell {
    pub(crate) no: Option<u32>,
    pub(crate) kind: LineKind,
    pub(crate) display_text: String,
    pub(crate) inline_spans: Vec<Range<usize>>,
}

pub(crate) struct SplitRow {
    pub(crate) left: Option<SplitCell>,
    pub(crate) right: Option<SplitCell>,
    /// `\ No newline at end of file`, shown across both columns.
    pub(crate) meta: Option<String>,
}

/// Compute word-level intraline diff spans for paired deletion and addition lines.
pub(crate) fn compute_intraline_spans(
    del_text: &str,
    add_text: &str,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    if del_text.is_empty() || add_text.is_empty() || del_text.len() > 1200 || add_text.len() > 1200
    {
        return (Vec::new(), Vec::new());
    }

    let diff = TextDiff::configure().diff_words(del_text, add_text);
    let mut del_spans: Vec<Range<usize>> = Vec::new();
    let mut add_spans: Vec<Range<usize>> = Vec::new();

    let mut del_offset = 0;
    let mut add_offset = 0;
    let mut has_equal_meaningful = false;

    for change in diff.iter_all_changes() {
        let val = change.value();
        let val_len = val.len();
        match change.tag() {
            ChangeTag::Equal => {
                if !val.trim().is_empty() {
                    has_equal_meaningful = true;
                }
                del_offset += val_len;
                add_offset += val_len;
            }
            ChangeTag::Delete => {
                let range = del_offset..del_offset + val_len;
                if let Some(last) = del_spans.last_mut() {
                    if last.end == del_offset {
                        last.end = range.end;
                    } else {
                        del_spans.push(range);
                    }
                } else {
                    del_spans.push(range);
                }
                del_offset += val_len;
            }
            ChangeTag::Insert => {
                let range = add_offset..add_offset + val_len;
                if let Some(last) = add_spans.last_mut() {
                    if last.end == add_offset {
                        last.end = range.end;
                    } else {
                        add_spans.push(range);
                    }
                } else {
                    add_spans.push(range);
                }
                add_offset += val_len;
            }
        }
    }

    if !has_equal_meaningful {
        return (Vec::new(), Vec::new());
    }

    (del_spans, add_spans)
}

/// Pair deletions with additions so the left column is the old file and the
/// right column the new one.
pub(crate) fn split_rows(hunk: &Hunk) -> Vec<SplitRow> {
    let mut rows = Vec::new();
    let mut dels: Vec<(Option<u32>, String)> = Vec::new();
    let mut adds: Vec<(Option<u32>, String)> = Vec::new();

    for line in &hunk.lines {
        match line.kind {
            LineKind::Del => dels.push((line.old_no, line.display_text())),
            LineKind::Add => adds.push((line.new_no, line.display_text())),
            LineKind::Context => {
                flush_pair(&mut rows, &mut dels, &mut adds);
                let text = line.display_text();
                rows.push(SplitRow {
                    left: Some(SplitCell {
                        no: line.old_no,
                        kind: LineKind::Context,
                        display_text: text.clone(),
                        inline_spans: Vec::new(),
                    }),
                    right: Some(SplitCell {
                        no: line.new_no,
                        kind: LineKind::Context,
                        display_text: text,
                        inline_spans: Vec::new(),
                    }),
                    meta: None,
                });
            }
            LineKind::Meta => {
                flush_pair(&mut rows, &mut dels, &mut adds);
                rows.push(SplitRow {
                    left: None,
                    right: None,
                    meta: Some(line.raw.clone()),
                });
            }
        }
    }
    flush_pair(&mut rows, &mut dels, &mut adds);
    rows
}

pub(crate) fn align_pair_entries(
    dels: Vec<(Option<u32>, String)>,
    adds: Vec<(Option<u32>, String)>,
) -> Vec<(Option<(Option<u32>, String)>, Option<(Option<u32>, String)>)> {
    if dels.is_empty() {
        return adds.into_iter().map(|a| (None, Some(a))).collect();
    }
    if adds.is_empty() {
        return dels.into_iter().map(|d| (Some(d), None)).collect();
    }
    if dels.len() == adds.len() {
        return dels
            .into_iter()
            .zip(adds)
            .map(|(d, a)| (Some(d), Some(a)))
            .collect();
    }

    if dels.len() == 1 {
        let d = &dels[0];
        let mut best_idx = 0;
        let mut best_ratio = 0.0f32;
        for (idx, a) in adds.iter().enumerate() {
            let ratio = TextDiff::configure().diff_words(&d.1, &a.1).ratio();
            if ratio > best_ratio {
                best_ratio = ratio;
                best_idx = idx;
            }
        }
        if best_ratio >= 0.35 {
            let d_val = dels.into_iter().next().unwrap();
            let mut out = Vec::new();
            for (idx, a) in adds.into_iter().enumerate() {
                if idx == best_idx {
                    out.push((Some(d_val.clone()), Some(a)));
                } else {
                    out.push((None, Some(a)));
                }
            }
            return out;
        }
    }

    if adds.len() == 1 {
        let a = &adds[0];
        let mut best_idx = 0;
        let mut best_ratio = 0.0f32;
        for (idx, d) in dels.iter().enumerate() {
            let ratio = TextDiff::configure().diff_words(&d.1, &a.1).ratio();
            if ratio > best_ratio {
                best_ratio = ratio;
                best_idx = idx;
            }
        }
        if best_ratio >= 0.35 {
            let a_val = adds.into_iter().next().unwrap();
            let mut out = Vec::new();
            for (idx, d) in dels.into_iter().enumerate() {
                if idx == best_idx {
                    out.push((Some(d), Some(a_val.clone())));
                } else {
                    out.push((Some(d), None));
                }
            }
            return out;
        }
    }

    let count = dels.len().max(adds.len());
    let mut out = Vec::with_capacity(count);
    let mut dels_iter = dels.into_iter();
    let mut adds_iter = adds.into_iter();
    for _ in 0..count {
        out.push((dels_iter.next(), adds_iter.next()));
    }
    out
}

pub(crate) fn flush_pair(
    rows: &mut Vec<SplitRow>,
    dels: &mut Vec<(Option<u32>, String)>,
    adds: &mut Vec<(Option<u32>, String)>,
) {
    if dels.is_empty() && adds.is_empty() {
        return;
    }

    let aligned = align_pair_entries(std::mem::take(dels), std::mem::take(adds));

    for (del_opt, add_opt) in aligned {
        let (inline_del, inline_add) = match (&del_opt, &add_opt) {
            (Some((_, d)), Some((_, a))) => compute_intraline_spans(d, a),
            _ => (Vec::new(), Vec::new()),
        };

        rows.push(SplitRow {
            left: del_opt.map(|(no, text)| SplitCell {
                no,
                kind: LineKind::Del,
                display_text: text,
                inline_spans: inline_del,
            }),
            right: add_opt.map(|(no, text)| SplitCell {
                no,
                kind: LineKind::Add,
                display_text: text,
                inline_spans: inline_add,
            }),
            meta: None,
        });
    }
}
