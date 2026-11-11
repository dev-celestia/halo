//! Diff model and parser: turns raw `git diff` text into structured files,
//! hunks, and lines.

use gpui::*;

/// Diffs longer than this render without syntax colouring. The tokenizer runs
/// per line per frame, and past a few hundred lines the colour is not worth the
/// layout cost.
pub(crate) const HIGHLIGHT_LINE_LIMIT: usize = 600;

/// Fixed row height when No Wrap is on so left/right split panes stay aligned
/// like a raw unified diff (one visual line = one source line).
pub(crate) const DIFF_LINE_PX: f32 = 20.;

/// Approximate monospace advance at `text_sm` — used to size No Wrap content
/// wider than the pane so `overflow_x_scrollbar` actually has overflow.
pub(crate) const DIFF_CHAR_PX: f32 = 8.0;

/// Gutter + `+/-` prefix + padding reserved before the code text.
pub(crate) const DIFF_GUTTER_PAD_PX: f32 = 72.0;

/// Content width for a No Wrap stack: long enough that the pane must scroll.
pub(crate) fn nowrap_content_width(max_chars: usize) -> Pixels {
    px(DIFF_GUTTER_PAD_PX + (max_chars as f32) * DIFF_CHAR_PX)
}

/// Expand tabs to 4-space tab stops for consistent text display and layout measurement.
pub(crate) fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut col = 0;
    for c in s.chars() {
        if c == '\t' {
            let spaces = 4 - (col % 4);
            out.extend(std::iter::repeat(' ').take(spaces));
            col += spaces;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    Context,
    Add,
    Del,
    /// `\ No newline at end of file` and friends.
    Meta,
}

#[derive(Clone, Debug)]
pub(crate) struct DiffLine {
    /// The line exactly as git printed it, prefix included.
    pub(crate) raw: String,
    pub(crate) kind: LineKind,
    pub(crate) old_no: Option<u32>,
    pub(crate) new_no: Option<u32>,
}

impl DiffLine {
    pub(crate) fn text(&self) -> &str {
        match self.kind {
            // The prefix is a single ASCII byte, so slicing is boundary-safe.
            LineKind::Context | LineKind::Add | LineKind::Del => self.raw.get(1..).unwrap_or(""),
            LineKind::Meta => self.raw.as_str(),
        }
    }

    pub(crate) fn display_text(&self) -> String {
        expand_tabs(self.text())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Hunk {
    /// Verbatim header for patch building.
    pub(crate) header: String,
    /// Coordinate range (`@@ -a,b +c,d @@`).
    pub(crate) coords: String,
    /// Optional function/class signature following `@@`.
    pub(crate) context: String,
    pub(crate) lines: Vec<DiffLine>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileDiff {
    /// `diff --git`, `index`, `---`, `+++` — replayed verbatim when building a
    /// one-hunk patch.
    pub(crate) header: Vec<String>,
    pub(crate) hunks: Vec<Hunk>,
    /// `--- /dev/null`: new file in working tree.
    pub(crate) new_file: bool,
    /// `+++ /dev/null`: deleted file.
    pub(crate) deleted_file: bool,
    /// Binary file change.
    pub(crate) binary: bool,
    pub(crate) renamed_from: Option<String>,
    pub(crate) renamed_to: Option<String>,
    pub(crate) mode_change: Option<String>,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
}

impl FileDiff {
    /// Patch text for a single hunk, ready for `git apply`.
    pub(crate) fn hunk_patch(&self, index: usize) -> Option<String> {
        let hunk = self.hunks.get(index)?;
        let mut patch = String::new();
        for line in &self.header {
            // Drop entire-file operations so git apply --cached applies individual hunks cleanly
            if line.starts_with("rename from ")
                || line.starts_with("rename to ")
                || line.starts_with("similarity index ")
                || line.starts_with("old mode ")
                || line.starts_with("new mode ")
                || line.starts_with("new file mode ")
                || line.starts_with("deleted file mode ")
            {
                continue;
            }
            patch.push_str(line);
            patch.push('\n');
        }
        patch.push_str(&hunk.header);
        patch.push('\n');
        for line in &hunk.lines {
            patch.push_str(&line.raw);
            patch.push('\n');
        }
        Some(patch)
    }
}

pub(crate) fn parse(text: &str) -> FileDiff {
    let mut header = Vec::new();
    let mut hunks = Vec::new();
    let mut new_file = false;
    let mut deleted_file = false;
    let mut binary = false;
    let mut renamed_from = None;
    let mut renamed_to = None;
    let mut mode_change = None;
    let mut additions = 0;
    let mut deletions = 0;

    let mut lines = text.lines().peekable();

    // Everything before the first `@@` is the file header.
    while let Some(line) = lines.peek() {
        if line.starts_with("@@") {
            break;
        }
        if line.starts_with("--- /dev/null") {
            new_file = true;
        } else if line.starts_with("+++ /dev/null") {
            deleted_file = true;
        } else if line.starts_with("rename from ") {
            renamed_from = Some(line.trim_start_matches("rename from ").trim().to_string());
        } else if line.starts_with("rename to ") {
            renamed_to = Some(line.trim_start_matches("rename to ").trim().to_string());
        } else if line.starts_with("old mode ") || line.starts_with("new mode ") {
            mode_change = Some((*line).to_string());
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            binary = true;
        }
        header.push((*line).to_string());
        lines.next();
    }

    while let Some(line) = lines.next() {
        if !line.starts_with("@@") {
            continue;
        }
        let (old_start, new_start, coords, context) = parse_hunk_header(line);

        let mut body = Vec::new();
        while let Some(next) = lines.peek() {
            if next.starts_with("@@") || next.starts_with("diff ") {
                break;
            }
            body.push(lines.next().unwrap_or_default().to_string());
        }

        let mut old_no = old_start;
        let mut new_no = new_start;
        let mut parsed = Vec::with_capacity(body.len());
        for raw in body {
            let kind = match raw.chars().next() {
                Some('+') => {
                    additions += 1;
                    LineKind::Add
                }
                Some('-') => {
                    deletions += 1;
                    LineKind::Del
                }
                Some('\\') => LineKind::Meta,
                _ => LineKind::Context,
            };
            let (old, new) = match kind {
                LineKind::Context => {
                    let pair = (Some(old_no), Some(new_no));
                    old_no += 1;
                    new_no += 1;
                    pair
                }
                LineKind::Add => {
                    let pair = (None, Some(new_no));
                    new_no += 1;
                    pair
                }
                LineKind::Del => {
                    let pair = (Some(old_no), None);
                    old_no += 1;
                    pair
                }
                LineKind::Meta => (None, None),
            };
            parsed.push(DiffLine {
                raw,
                kind,
                old_no: old,
                new_no: new,
            });
        }

        hunks.push(Hunk {
            header: line.to_string(),
            coords,
            context,
            lines: parsed,
        });
    }

    FileDiff {
        header,
        hunks,
        new_file,
        deleted_file,
        binary,
        renamed_from,
        renamed_to,
        mode_change,
        additions,
        deletions,
    }
}

/// `@@ -12,7 +12,9 @@ context` → `(12, 12, "@@ -12,7 +12,9 @@", "context")`.
pub(crate) fn parse_hunk_header(line: &str) -> (u32, u32, String, String) {
    let mut old_start = 1;
    let mut new_start = 1;
    for part in line.split_whitespace() {
        if let Some(rest) = part.strip_prefix('-') {
            old_start = rest
                .split(',')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(1);
        } else if let Some(rest) = part.strip_prefix('+') {
            new_start = rest
                .split(',')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(1);
        }
    }

    let (coords, context) = if let Some(first_at) = line.find("@@") {
        if let Some(second_at) = line[first_at + 2..].find("@@") {
            let end_coords = first_at + 2 + second_at + 2;
            let c = line[..end_coords].to_string();
            let ctx = line[end_coords..].trim().to_string();
            (c, ctx)
        } else {
            (line.to_string(), String::new())
        }
    } else {
        (line.to_string(), String::new())
    };

    (old_start, new_start, coords, context)
}
