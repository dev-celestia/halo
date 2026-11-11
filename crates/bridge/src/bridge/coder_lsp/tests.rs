//! Coder LSP session tests.

use super::*;

#[test]
fn progress_summary_formats_percentage_and_fraction() {
    let task = LspProgressTask {
        title: "Indexing".into(),
        message: Some("crates/bridge".into()),
        percentage: Some(42),
        seq: 1,
    };
    assert_eq!(task.summary(), "Indexing 42%");

    let roots = LspProgressTask {
        title: "Roots Scanned".into(),
        message: Some("120/754: /some/long/path".into()),
        percentage: None,
        seq: 2,
    };
    assert_eq!(roots.summary(), "Roots Scanned 120/754");

    let fetching = LspProgressTask {
        title: "Fetching".into(),
        message: Some("discovering sysroot".into()),
        percentage: None,
        seq: 3,
    };
    assert_eq!(fetching.summary(), "Fetching…");
}

#[test]
fn status_labels_cover_starting_indexing_and_ready() {
    assert_eq!(CoderLspStatus::Starting.label(), "LSP…");
    assert_eq!(
        CoderLspStatus::Indexing("Fetching…".into()).label(),
        "LSP: Fetching…"
    );
    assert_eq!(
        CoderLspStatus::Ready {
            errors: 0,
            warnings: 0
        }
        .label(),
        "LSP"
    );
    assert_eq!(
        CoderLspStatus::Ready {
            errors: 2,
            warnings: 1
        }
        .label(),
        "LSP E2 W1"
    );
}

#[cfg(unix)]
#[test]
fn paths_match_across_symlinked_root() {
    let dir = std::env::temp_dir().join(format!("halo-lsp-symlink-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let real_sub = dir.join("real");
    std::fs::create_dir_all(&real_sub).unwrap();
    let file = real_sub.join("mod.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();

    let link_sub = dir.join("linked");
    std::os::unix::fs::symlink(&real_sub, &link_sub).unwrap();
    let via_link = link_sub.join("mod.rs");

    assert!(paths_match(
        &file.display().to_string(),
        &via_link.display().to_string()
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn zero_width_rust_analyzer_diagnostic_expands_to_token() {
    use lsp_types::Position;

    let text = Rope::from("pub use foo;\n\nsads\n\n");
    let lsp_diag = lsp_types::Diagnostic {
        range: lsp_types::Range {
            start: Position::new(2, 4),
            end: Position::new(2, 4),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: "Syntax Error: expected an item".into(),
        ..Default::default()
    };
    let normalized = normalize_diagnostics_for_rope(&text, &[lsp_diag]);
    assert_eq!(normalized[0].range.start, Position::new(2, 0));
    assert_eq!(normalized[0].range.end, Position::new(2, 4));

    // EOF zero-width diagnostic on trailing blank line also attaches to `sads`.
    let eof_diag = lsp_types::Diagnostic {
        range: lsp_types::Range {
            start: Position::new(4, 0),
            end: Position::new(4, 0),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: "expected one of `!` or `::`, found `<eof>`".into(),
        ..Default::default()
    };
    let normalized_eof = normalize_diagnostics_for_rope(&text, &[eof_diag]);
    assert_eq!(normalized_eof[0].range.start, Position::new(2, 0));
    assert_eq!(normalized_eof[0].range.end, Position::new(2, 4));
}

#[test]
fn syntax_linter_detects_bare_identifier_and_deduplicates_with_lsp() {
    use lsp_types::Position;

    let text = Rope::from("pub mod a;\n\nsads\n");
    let lint = compute_syntax_lint_diagnostics("rust", &text);
    assert!(
        !lint.is_empty(),
        "expected built-in tree-sitter syntax linter to flag bare `sads`"
    );
    assert_eq!(lint[0].range.start, Position::new(2, 0));
    assert_eq!(lint[0].range.end, Position::new(2, 4));

    // Before LSP arrives, merged diagnostics contain the linter diagnostic.
    let merged_before_lsp = merge_lsp_and_lint_diagnostics(&text, &[], &lint);
    assert_eq!(merged_before_lsp.len(), 1);
    assert_eq!(merged_before_lsp[0].source.as_deref(), Some("linter"));

    // Once rust-analyzer publishes a diagnostic on the same line, the
    // merged list prefers rust-analyzer's diagnostic without duplicating.
    let ra_diag = lsp_types::Diagnostic {
        range: lsp_types::Range {
            start: Position::new(2, 4),
            end: Position::new(2, 4),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("rust-analyzer".into()),
        message: "Syntax Error: expected an item".into(),
        ..Default::default()
    };
    let merged_after_lsp = merge_lsp_and_lint_diagnostics(&text, &[ra_diag], &lint);
    assert_eq!(merged_after_lsp.len(), 1);
    assert_eq!(merged_after_lsp[0].source.as_deref(), Some("rust-analyzer"));
    assert_eq!(merged_after_lsp[0].range.start, Position::new(2, 0));
    assert_eq!(merged_after_lsp[0].range.end, Position::new(2, 4));
}

#[test]
fn syntax_linter_covers_bash_python_and_javascript() {
    let bash = Rope::from("if true; then\n  echo hi\n");
    assert!(
        !compute_syntax_lint_diagnostics("bash", &bash).is_empty(),
        "bash grammar should flag the missing `fi`"
    );

    let python = Rope::from("def broken(:\n    pass\n");
    assert!(
        !compute_syntax_lint_diagnostics("python", &python).is_empty(),
        "python grammar should flag the broken parameter list"
    );

    let javascript = Rope::from("const = ;\n");
    assert!(
        !compute_syntax_lint_diagnostics("javascript", &javascript).is_empty(),
        "javascript grammar should flag the missing binding name"
    );
}
