//! Diagnostics: LSP/lint merging, rope normalization, and status refresh.

use super::*;

pub(crate) fn has_diagnostics_for_bound_path(rt: &CoderLspRuntime) -> bool {
    let mut any_bound = false;
    for bound in rt.bound_path.iter().flatten() {
        any_bound = true;
        if rt.diagnostics.keys().any(|k| paths_match(k, bound)) {
            return true;
        }
    }
    !any_bound
}

pub(crate) fn diagnostic_counts(rt: &CoderLspRuntime) -> (u32, u32) {
    let mut e = 0u32;
    let mut w = 0u32;
    let mut keys: HashSet<String> = rt.diagnostics.keys().cloned().collect();
    keys.extend(rt.lint_diagnostics.keys().cloned());
    for key in keys {
        let lsp = rt
            .diagnostics
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let lint = rt
            .lint_diagnostics
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for d in lsp {
            match d.severity {
                Some(DiagnosticSeverity::ERROR) => e += 1,
                Some(DiagnosticSeverity::WARNING) => w += 1,
                _ => {}
            }
        }
        for ld in lint {
            let covered_by_lsp = lsp.iter().any(|d| {
                d.range.start.line <= ld.range.end.line && d.range.end.line >= ld.range.start.line
            });
            if !covered_by_lsp {
                match ld.severity {
                    Some(DiagnosticSeverity::ERROR) => e += 1,
                    Some(DiagnosticSeverity::WARNING) => w += 1,
                    _ => {}
                }
            }
        }
    }
    (e, w)
}

/// Run the fast built-in tree-sitter syntax linter over `text` for `language`.
pub(crate) fn compute_syntax_lint_diagnostics(
    language: &str,
    text: &Rope,
) -> Vec<lsp_types::Diagnostic> {
    let source = text.to_string();
    let raw: Vec<lsp_types::Diagnostic> =
        gpui_component::highlighter::syntax_error_spans(language, &source)
            .into_iter()
            .map(|(byte_range, message)| lsp_types::Diagnostic {
                range: Range {
                    start: text.offset_to_position(byte_range.start),
                    end: text.offset_to_position(byte_range.end),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String("syntax-error".into())),
                source: Some("linter".into()),
                message,
                ..Default::default()
            })
            .collect();
    normalize_diagnostics_for_rope(text, &raw)
}

/// Merge normalized LSP diagnostics with instant tree-sitter syntax-linter
/// diagnostics, deduplicating linter diagnostics on lines already covered by LSP.
pub(crate) fn merge_lsp_and_lint_diagnostics(
    text: &Rope,
    lsp_diags: &[lsp_types::Diagnostic],
    lint_diags: &[lsp_types::Diagnostic],
) -> Vec<lsp_types::Diagnostic> {
    let mut combined = normalize_diagnostics_for_rope(text, lsp_diags);
    let normalized_lint = normalize_diagnostics_for_rope(text, lint_diags);
    for lint_diag in normalized_lint {
        let covered = combined.iter().any(|d| {
            d.range.start.line <= lint_diag.range.end.line
                && d.range.end.line >= lint_diag.range.start.line
        });
        if !covered {
            combined.push(lint_diag);
        }
    }
    combined.sort_by_key(|d| {
        (
            d.range.start.line,
            d.range.start.character,
            d.range.end.line,
            d.range.end.character,
        )
    });
    combined
}

pub(crate) fn is_lsp_syntax_diagnostic(d: &lsp_types::Diagnostic) -> bool {
    let is_syntax_code = matches!(
        &d.code,
        Some(NumberOrString::String(s)) if s.eq_ignore_ascii_case("syntax-error")
    );
    is_syntax_code || d.message.starts_with("Syntax Error")
}

/// Normalize LSP diagnostics against `text` so zero-width or newline/EOF ranges
/// (such as `rust-analyzer`'s `Syntax Error: expected an item` at the end of a
/// bare identifier or `rustc`'s `<eof>`) expand onto the nearest visible token
/// and sort in ascending order for `DiagnosticSet` / `combine_highlights`.
pub(crate) fn normalize_diagnostics_for_rope(
    text: &Rope,
    diags: &[lsp_types::Diagnostic],
) -> Vec<lsp_types::Diagnostic> {
    let mut out: Vec<lsp_types::Diagnostic> = diags
        .iter()
        .cloned()
        .map(|mut d| {
            let start = text.position_to_offset(&d.range.start);
            let end = text.position_to_offset(&d.range.end);
            let has_visible = start < end
                && text
                    .slice(start..end)
                    .chars()
                    .any(|c| c != '\n' && c != '\r');
            if !has_visible {
                if let Some(expanded) = expand_empty_diagnostic_range(text, &d.range) {
                    d.range = expanded;
                }
            }
            d
        })
        .collect();
    out.sort_by_key(|d| {
        (
            d.range.start.line,
            d.range.start.character,
            d.range.end.line,
            d.range.end.character,
        )
    });
    out
}

pub(crate) fn expand_empty_diagnostic_range(text: &Rope, range: &Range) -> Option<Range> {
    let line_count = text.lines_len();
    if line_count == 0 {
        return None;
    }
    let start_offset = text.position_to_offset(&range.start);
    let target_line = (range.start.line as usize).min(line_count.saturating_sub(1));
    let line_start = text.line_start_offset(target_line);
    let line_end = text.line_end_offset(target_line);
    let line_str = text.slice(line_start..line_end).to_string();

    if !line_str.trim().is_empty() {
        let rel = start_offset.saturating_sub(line_start).min(line_str.len());
        if let Some(span) = token_byte_span_before(&line_str[..rel]) {
            return Some(Range {
                start: text.offset_to_position(line_start + span.start),
                end: text.offset_to_position(line_start + span.end),
            });
        }
        if let Some(span) = token_byte_span_after(&line_str[rel..]) {
            return Some(Range {
                start: text.offset_to_position(line_start + rel + span.start),
                end: text.offset_to_position(line_start + rel + span.end),
            });
        }
    }

    for prev_line in (0..target_line).rev() {
        let p_start = text.line_start_offset(prev_line);
        let p_end = text.line_end_offset(prev_line);
        let p_str = text.slice(p_start..p_end).to_string();
        if let Some(span) = token_byte_span_before(&p_str) {
            return Some(Range {
                start: text.offset_to_position(p_start + span.start),
                end: text.offset_to_position(p_start + span.end),
            });
        }
    }

    for next_line in (target_line + 1)..line_count {
        let n_start = text.line_start_offset(next_line);
        let n_end = text.line_end_offset(next_line);
        let n_str = text.slice(n_start..n_end).to_string();
        if let Some(span) = token_byte_span_after(&n_str) {
            return Some(Range {
                start: text.offset_to_position(n_start + span.start),
                end: text.offset_to_position(n_start + span.end),
            });
        }
    }

    None
}

pub(crate) fn token_byte_span_before(s: &str) -> Option<OpsRange<usize>> {
    let trimmed = s.trim_end_matches(|c: char| c.is_whitespace());
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed.len();
    let last_char = trimmed.chars().next_back()?;
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let start = if is_ident(last_char) {
        trimmed
            .char_indices()
            .rev()
            .take_while(|&(_, c)| is_ident(c))
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    } else {
        trimmed
            .char_indices()
            .rev()
            .take_while(|&(_, c)| !c.is_whitespace() && !is_ident(c))
            .last()
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    };
    Some(start..end)
}

pub(crate) fn token_byte_span_after(s: &str) -> Option<OpsRange<usize>> {
    let non_ws_offset = s.find(|c: char| !c.is_whitespace())?;
    let rest = &s[non_ws_offset..];
    let first_char = rest.chars().next()?;
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let len: usize = if is_ident(first_char) {
        rest.chars()
            .take_while(|&c| is_ident(c))
            .map(char::len_utf8)
            .sum()
    } else {
        rest.chars()
            .take_while(|&c| !c.is_whitespace() && !is_ident(c))
            .map(char::len_utf8)
            .sum()
    };
    Some(non_ws_offset..(non_ws_offset + len.max(first_char.len_utf8())))
}

pub(crate) fn refresh_lsp_status(cx: &mut App) {
    let (alive, starting, progress, bound_ready, (errors, warnings)) = match runtime_ref(cx) {
        Some(rt) => (
            any_client_alive(rt),
            !rt.starting.is_empty(),
            active_progress_label(rt),
            has_diagnostics_for_bound_path(rt),
            diagnostic_counts(rt),
        ),
        None => {
            Bridge::state(cx).update(cx, |s| {
                s.coder.lsp = CoderLspStatus::Idle;
            });
            return;
        }
    };
    Bridge::state(cx).update(cx, |s| {
        if alive {
            if let Some(detail) = progress.filter(|_| errors == 0 && warnings == 0) {
                s.coder.lsp = CoderLspStatus::Indexing(detail);
            } else if !bound_ready && errors == 0 && warnings == 0 {
                s.coder.lsp = CoderLspStatus::Starting;
            } else {
                s.coder.lsp = CoderLspStatus::Ready { errors, warnings };
            }
            s.last_error = None;
        } else if starting {
            if errors > 0 || warnings > 0 {
                s.coder.lsp = CoderLspStatus::Ready { errors, warnings };
            } else {
                s.coder.lsp = CoderLspStatus::Starting;
            }
        } else if errors > 0 || warnings > 0 {
            s.coder.lsp = CoderLspStatus::Ready { errors, warnings };
        } else if matches!(s.coder.lsp, CoderLspStatus::Unavailable(_)) {
            // Keep soft unavailable if nothing is running.
        } else {
            s.coder.lsp = CoderLspStatus::Idle;
        }
    });
}
