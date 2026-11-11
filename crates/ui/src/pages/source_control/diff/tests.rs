use super::split::split_rows;
use super::*;
use std::ops::Range;
use std::prelude::rust_2024::test;

const SAMPLE: &str = "diff --git a/f.txt b/f.txt
index ac9837c..64275c7 100644
--- a/f.txt
+++ b/f.txt
@@ -1,4 +1,4 @@
 line 1
-line 2
+line 2 CHANGED
 line 3
 line 4
@@ -20,3 +20,4 @@
 line 20
+line 20.5
 line 21
 line 22
";

#[test]
fn parses_header_and_both_hunks() {
    let diff = parse(SAMPLE);
    assert_eq!(diff.header.len(), 4);
    assert_eq!(diff.hunks.len(), 2);
    assert!(!diff.new_file);
    assert!(!diff.deleted_file);
    assert!(!diff.binary);
    assert_eq!(diff.additions, 2);
    assert_eq!(diff.deletions, 1);
    assert_eq!(diff.hunks[0].lines.len(), 5);
    assert_eq!(diff.hunks[1].lines.len(), 4);
}

#[test]
fn numbers_lines_on_both_sides() {
    let diff = parse(SAMPLE);
    let first = &diff.hunks[0];
    assert_eq!(
        (first.lines[0].old_no, first.lines[0].new_no),
        (Some(1), Some(1))
    );
    assert_eq!(
        (first.lines[1].old_no, first.lines[1].new_no),
        (Some(2), None)
    );
    assert_eq!(
        (first.lines[2].old_no, first.lines[2].new_no),
        (None, Some(2))
    );
    assert_eq!(
        (first.lines[3].old_no, first.lines[3].new_no),
        (Some(3), Some(3))
    );
}

#[test]
fn strips_the_line_prefix_and_expands_tabs() {
    let diff = parse(SAMPLE);
    assert_eq!(diff.hunks[0].lines[1].text(), "line 2");
    assert_eq!(diff.hunks[0].lines[2].text(), "line 2 CHANGED");
    assert_eq!(diff.hunks[0].lines[0].text(), "line 1");

    assert_eq!(expand_tabs("\tfoo"), "    foo");
    assert_eq!(expand_tabs("a\tb"), "a   b");
}

#[test]
fn rebuilds_a_single_hunk_patch() {
    let diff = parse(SAMPLE);
    let patch = diff.hunk_patch(1).expect("hunk 1");
    assert!(patch.starts_with("diff --git a/f.txt b/f.txt\n"));
    assert!(patch.contains("@@ -20,3 +20,4 @@\n"));
    assert!(!patch.contains("@@ -1,4 +1,4 @@"));
    assert!(patch.contains("+line 20.5\n"));
}

#[test]
fn pairs_deletions_with_additions_and_computes_intraline() {
    let diff = parse(SAMPLE);
    let rows = split_rows(&diff.hunks[0]);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows[0].left.as_ref().map(|c| c.kind),
        Some(LineKind::Context)
    );
    assert_eq!(rows[1].left.as_ref().map(|c| c.kind), Some(LineKind::Del));
    assert_eq!(rows[1].right.as_ref().map(|c| c.kind), Some(LineKind::Add));

    let right = rows[1].right.as_ref().unwrap();
    assert!(!right.inline_spans.is_empty());
    assert_eq!(
        right.display_text[right.inline_spans[0].clone()].trim(),
        "CHANGED"
    );
}

#[test]
fn pads_a_pure_addition_block() {
    let diff = parse(SAMPLE);
    let rows = split_rows(&diff.hunks[1]);
    let added = rows
        .iter()
        .find(|r| r.right.as_ref().is_some_and(|c| c.kind == LineKind::Add))
        .expect("an addition row");
    assert!(added.left.is_none());
}

#[test]
fn detects_new_and_deleted_and_renamed_files() {
    let diff_new = parse(
        "diff --git a/new.rs b/new.rs\n--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1 @@\n+fn main() {}\n",
    );
    assert!(diff_new.new_file);
    assert!(!diff_new.deleted_file);

    let diff_del = parse(
        "diff --git a/old.rs b/old.rs\n--- a/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-fn main() {}\n",
    );
    assert!(diff_del.deleted_file);
    assert!(!diff_del.new_file);

    let diff_rename = parse(
        "diff --git a/a.rs b/b.rs\nsimilarity index 98%\nrename from a.rs\nrename to b.rs\n--- a/a.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-a\n+b\n",
    );
    assert_eq!(diff_rename.renamed_from.as_deref(), Some("a.rs"));
    assert_eq!(diff_rename.renamed_to.as_deref(), Some("b.rs"));
    let patch = diff_rename.hunk_patch(0).unwrap();
    assert!(!patch.contains("rename from"));
    assert!(!patch.contains("similarity index"));
}

#[test]
fn hunk_header_splits_coords_and_context() {
    let (old_s, new_s, coords, ctx) = parse_hunk_header("@@ -73,7 +73,11 @@ impl BridgeClient {");
    assert_eq!((old_s, new_s), (73, 73));
    assert_eq!(coords, "@@ -73,7 +73,11 @@");
    assert_eq!(ctx, "impl BridgeClient {");

    let (old_s2, new_s2, coords2, ctx2) = parse_hunk_header("@@ -7 +7 @@");
    assert_eq!((old_s2, new_s2), (7, 7));
    assert_eq!(coords2, "@@ -7 +7 @@");
    assert_eq!(ctx2, "");
}

#[test]
fn tokenize_ranges_captures_keywords_and_comments() {
    let palette = Palette {
        comment: hsla(0., 0., 0.5, 1.),
        string: hsla(0.1, 0.8, 0.5, 1.),
        number: hsla(0.5, 0.8, 0.5, 1.),
        keyword: hsla(0.7, 0.8, 0.5, 1.),
    };
    let code = "pub fn test() { // comment\n}";
    let ranges = tokenize_ranges(Lang::Rust, code, &palette);
    assert!(ranges.len() >= 3);
    assert_eq!(&code[ranges[0].0.clone()], "pub");
    assert_eq!(&code[ranges[1].0.clone()], "fn");
}

#[test]
fn combines_highlights_cleanly() {
    let syntax = vec![(0..3, hsla(0.7, 0.8, 0.5, 1.))];
    let intraline = vec![2..6];
    let combined = combine_line_highlights(
        10,
        &syntax,
        &intraline,
        Some(hsla(0.3, 0.5, 0.5, 0.3)),
        hsla(0., 0., 1., 1.),
    );
    assert!(!combined.is_empty());
    for i in 0..combined.len() - 1 {
        assert!(combined[i].0.end <= combined[i + 1].0.start);
    }
}

fn test_palette() -> Palette {
    Palette {
        comment: hsla(0., 0., 0.5, 1.),
        string: hsla(0.1, 0.8, 0.5, 1.),
        number: hsla(0.5, 0.8, 0.5, 1.),
        keyword: hsla(0.7, 0.8, 0.5, 1.),
    }
}

fn tokens(lang: Lang, text: &str) -> Vec<(Range<usize>, Hsla)> {
    tokenize_ranges(lang, text, &test_palette())
}

fn texts<'a>(code: &'a str, ranges: &[(Range<usize>, Hsla)], color: Hsla) -> Vec<&'a str> {
    ranges
        .iter()
        .filter(|(_, c)| *c == color)
        .map(|(r, _)| &code[r.clone()])
        .collect()
}

#[test]
fn rust_raw_strings_hide_comment_markers() {
    let palette = test_palette();
    let code = r##"let s = r#"a // b"#; // real"##;
    let ranges = tokenize_ranges(Lang::Rust, code, &palette);
    assert_eq!(
        texts(code, &ranges, palette.string),
        vec![r##"r#"a // b"#"##]
    );
    assert_eq!(texts(code, &ranges, palette.comment), vec!["// real"]);
}

#[test]
fn rust_char_literal_is_not_a_lifetime() {
    let palette = test_palette();
    let code = "fn f<'a>(x: &'a str) -> char { 'x' }";
    let ranges = tokenize_ranges(Lang::Rust, code, &palette);
    assert_eq!(texts(code, &ranges, palette.string), vec!["'x'"]);
}

#[test]
fn rust_block_comments_span_lines_and_nest() {
    let palette = test_palette();
    let mut state = SyntaxState::default();

    let first = tokenize_line(
        Lang::Rust,
        "/* outer /* inner */ still",
        &palette,
        &mut state,
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].0, 0.."/* outer /* inner */ still".len());
    assert_ne!(state, SyntaxState::default());

    let second_line = "*/ let x = 1;";
    let second = tokenize_line(Lang::Rust, second_line, &palette, &mut state);
    assert_eq!(state, SyntaxState::default());
    assert_eq!(second[0].0, 0..2);
    assert!(texts(second_line, &second, palette.keyword).contains(&"let"));
}

#[test]
fn python_triple_quoted_strings_span_lines() {
    let palette = test_palette();
    let mut state = SyntaxState::default();

    let first = tokenize_line(Lang::Python, "x = \"\"\"doc", &palette, &mut state);
    assert_eq!(
        texts("x = \"\"\"doc", &first, palette.string),
        vec!["\"\"\"doc"]
    );
    assert_ne!(state, SyntaxState::default());

    let second_line = "still doc\"\"\" + y";
    let second = tokenize_line(Lang::Python, second_line, &palette, &mut state);
    assert_eq!(state, SyntaxState::default());
    assert_eq!(
        texts(second_line, &second, palette.string),
        vec!["still doc\"\"\""]
    );
}

#[test]
fn js_template_literals_span_lines() {
    let palette = test_palette();
    let mut state = SyntaxState::default();

    let first = tokenize_line(Lang::Js, "const s = `hi", &palette, &mut state);
    assert_eq!(texts("const s = `hi", &first, palette.string), vec!["`hi"]);
    assert_ne!(state, SyntaxState::default());

    let second_line = "there ${x}` + y";
    let second = tokenize_line(Lang::Js, second_line, &palette, &mut state);
    assert_eq!(state, SyntaxState::default());
    assert_eq!(
        texts(second_line, &second, palette.string),
        vec!["there ${x}`"]
    );
}

#[test]
fn shell_hash_only_comments_at_word_boundaries() {
    let palette = test_palette();
    let code = "echo ${#arr[@]} # real";
    let ranges = tokenize_ranges(Lang::Shell, code, &palette);
    assert_eq!(texts(code, &ranges, palette.comment), vec!["# real"]);
}

#[test]
fn numbers_do_not_swallow_range_operators() {
    let palette = test_palette();
    let code = "for i in 0..=10 {}";
    let ranges = tokenize_ranges(Lang::Rust, code, &palette);
    assert_eq!(texts(code, &ranges, palette.number), vec!["0", "10"]);
}

#[test]
fn lang_detection_handles_names_and_special_files() {
    assert_eq!(lang_for("Dockerfile"), Lang::Shell);
    assert_eq!(lang_for("infra/Dockerfile.dev"), Lang::Shell);
    assert_eq!(lang_for("scripts/Makefile"), Lang::Shell);
    assert_eq!(lang_for(".gitignore"), Lang::Config);
    assert_eq!(lang_for(".env.local"), Lang::Config);
    assert_eq!(lang_for("src/app.tsx"), Lang::Js);
    assert_eq!(lang_for("src/main.rs"), Lang::Rust);
    assert_eq!(lang_for("Cargo.lock"), Lang::Config);
    assert_eq!(lang_for("index.html"), Lang::Markup);
    assert_eq!(lang_for("README.md"), Lang::Plain);
}
