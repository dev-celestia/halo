//! Syntax tokenization and highlight composition for diff code rendering.
//!
//! Tokenizing is line-oriented so it can run during rendering, but constructs that
//! legitimately span lines (block comments, Rust raw strings, Python triple-quoted
//! strings, JS template literals, quoted shell strings) carry a [`SyntaxState`] from
//! one line to the next. Callers rendering a run of lines keep one state per side of
//! the diff; callers that only need a single isolated line can use
//! [`tokenize_ranges`].

use std::ops::Range;

use celestia_ui::theme::ActiveTheme as _;
use gpui::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lang {
    Rust,
    /// C, C++, C#, Java, Kotlin, Swift, Scala, Dart, PHP, Go, plus JSON/CSS-ish.
    CLike,
    /// JavaScript and TypeScript (template literals, no char literals).
    Js,
    /// Python (and Ruby, which shares enough keywords/comments to look right).
    Python,
    /// TOML, YAML, INI, dotenv and locking files: `#` comments, no keyword soup.
    Config,
    /// Shell and shell-adjacent build files (Makefile, Dockerfile).
    Shell,
    /// HTML, XML, SVG: `<!-- -->` comments and `<tag` names.
    Markup,
    Plain,
}

/// Best-effort language for a repository path. Detection prefers exact file names
/// (Dockerfile, Makefile, dotfiles) before falling back to the extension.
pub(crate) fn lang_for(path: &str) -> Lang {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let lower = file.to_ascii_lowercase();

    match lower.as_str() {
        "dockerfile" | "containerfile" | "makefile" | "gnumakefile" => return Lang::Shell,
        "cmakelists.txt" => return Lang::Config,
        "gemfile" | "rakefile" | "podfile" | "vagrantfile" => return Lang::Python,
        ".bashrc" | ".bash_profile" | ".zshrc" | ".zprofile" | ".profile" => {
            return Lang::Shell;
        }
        ".gitignore" | ".gitattributes" | ".dockerignore" | ".npmrc" | ".yarnrc"
        | ".editorconfig" | ".prettierrc" | ".eslintrc" | ".babelrc" | ".env" => {
            return Lang::Config;
        }
        _ => {}
    }

    if lower.starts_with("dockerfile.") || lower.starts_with("makefile.") {
        return Lang::Shell;
    }
    if lower.starts_with(".env.") {
        return Lang::Config;
    }

    let ext = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    match ext {
        "rs" => Lang::Rust,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::Js,
        "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "cs" | "java" | "kt" | "kts"
        | "swift" | "scala" | "dart" | "php" | "go" | "gradle" | "groovy" | "css" | "scss"
        | "sass" | "less" | "json" | "jsonc" | "json5" => Lang::CLike,
        "py" | "pyw" | "pyi" | "rb" | "rake" | "gemspec" => Lang::Python,
        "toml" | "lock" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "properties" | "env" | "tf"
        | "hcl" | "nix" => Lang::Config,
        "sh" | "bash" | "zsh" | "fish" | "mk" | "mak" | "ksh" => Lang::Shell,
        "html" | "htm" | "xhtml" | "xml" | "svg" | "svelte" | "vue" => Lang::Markup,
        _ => Lang::Plain,
    }
}

impl Lang {
    fn line_comment(self, bytes: &[u8], i: usize) -> Option<usize> {
        match self {
            Lang::Rust | Lang::CLike | Lang::Js => {
                (bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/')).then_some(2)
            }
            Lang::Python | Lang::Config => (bytes[i] == b'#').then_some(1),
            Lang::Shell => {
                if bytes[i] != b'#' {
                    return None;
                }
                // `#` only starts a comment at a word boundary; `$#`, `${#arr}`
                // and `foo#bar` are ordinary shell text.
                if i == 0 {
                    return Some(1);
                }
                let prev = bytes[i - 1];
                let boundary = prev.is_ascii_whitespace()
                    || matches!(
                        prev,
                        b';' | b'|' | b'&' | b'(' | b')' | b'{' | b'}' | b'[' | b']'
                    );
                if !boundary {
                    return None;
                }
                if prev == b'{' && i >= 2 && bytes[i - 2] == b'$' {
                    return None;
                }
                Some(1)
            }
            Lang::Markup | Lang::Plain => None,
        }
    }

    fn has_block_comment(self) -> bool {
        matches!(self, Lang::Rust | Lang::CLike | Lang::Js)
    }

    fn is_keyword(self, word: &str) -> bool {
        super::keywords::for_lang(self).contains(&word)
    }
}

pub(crate) struct Palette {
    pub(crate) comment: Hsla,
    pub(crate) string: Hsla,
    pub(crate) number: Hsla,
    pub(crate) keyword: Hsla,
}

pub(crate) fn palette(cx: &App) -> Palette {
    let theme = cx.theme();
    Palette {
        comment: theme.muted_foreground,
        string: theme.warning,
        number: theme.info,
        keyword: theme.primary,
    }
}

/// Lexer state carried from one line to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SyntaxState {
    open: Open,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Open {
    #[default]
    None,
    BlockComment {
        depth: u32,
    },
    MarkupComment,
    Quoted {
        quote: u8,
        escapes: bool,
    },
    Triple {
        quote: u8,
    },
    RustRaw {
        hashes: u8,
    },
    Template,
}

/// Tokenize a single line in isolation (no multi-line context).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn tokenize_ranges(
    lang: Lang,
    text: &str,
    palette: &Palette,
) -> Vec<(Range<usize>, Hsla)> {
    let mut state = SyntaxState::default();
    tokenize_line(lang, text, palette, &mut state)
}

/// Tokenize one line, resuming any construct left open by the previous line and
/// updating `state` if this line ends inside one.
pub(crate) fn tokenize_line(
    lang: Lang,
    text: &str,
    palette: &Palette,
    state: &mut SyntaxState,
) -> Vec<(Range<usize>, Hsla)> {
    if lang == Lang::Plain || text.is_empty() {
        *state = SyntaxState::default();
        return Vec::new();
    }

    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut out = Vec::new();

    let mut open = std::mem::take(&mut state.open);
    let mut i = if open == Open::None {
        0
    } else {
        resume_open(lang, bytes, palette, &mut open, &mut out)
    };
    if open != Open::None {
        state.open = open;
        return out;
    }

    while i < len {
        let b = bytes[i];

        if lang.line_comment(bytes, i).is_some() {
            push_token(&mut out, i, len, palette.comment);
            break;
        }

        if lang.has_block_comment() && b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let nested = lang == Lang::Rust;
            let mut depth = 1u32;
            let mut j = i + 2;
            let mut closed = false;
            while j + 1 < len {
                if nested && bytes[j] == b'/' && bytes[j + 1] == b'*' {
                    depth += 1;
                    j += 2;
                    continue;
                }
                if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                    continue;
                }
                j += 1;
            }
            let end = if closed { j } else { len };
            push_token(&mut out, i, end, palette.comment);
            i = end;
            if !closed {
                state.open = Open::BlockComment { depth };
                break;
            }
            continue;
        }

        if lang == Lang::Markup && text[i..].starts_with("<!--") {
            match find_subslice(&bytes[i + 4..], b"-->") {
                Some(pos) => {
                    let end = i + 4 + pos + 3;
                    push_token(&mut out, i, end, palette.comment);
                    i = end;
                }
                None => {
                    push_token(&mut out, i, len, palette.comment);
                    state.open = Open::MarkupComment;
                    break;
                }
            }
            continue;
        }

        // C-family preprocessor line: colour `#directive` at the start of a line.
        if lang == Lang::CLike && b == b'#' && text[..i].bytes().all(|c| c == b' ' || c == b'\t') {
            let mut j = i + 1;
            while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > i + 1 {
                push_token(&mut out, i, j, palette.keyword);
                i = j;
                continue;
            }
        }

        if let Some(end) = scan_string(lang, bytes, i, palette, state, &mut out) {
            i = end;
            continue;
        }

        // Shell variables: `$name`, `${...}`, `$?`, `$#`, `$@`, ...
        if lang == Lang::Shell && b == b'$' {
            let mut j = i + 1;
            if j < len && bytes[j] == b'{' {
                while j < len && bytes[j] != b'}' {
                    j += 1;
                }
                if j < len {
                    j += 1;
                }
                push_token(&mut out, i, j, palette.keyword);
                i = j;
                continue;
            }
            if j < len
                && (bytes[j].is_ascii_alphanumeric()
                    || matches!(
                        bytes[j],
                        b'_' | b'?' | b'#' | b'@' | b'*' | b'!' | b'$' | b'-'
                    ))
            {
                j += 1;
                while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                push_token(&mut out, i, j, palette.keyword);
                i = j;
                continue;
            }
        }

        // Markup tag names: `<div`, `</div`, `<!DOCTYPE`, `<?xml`.
        if lang == Lang::Markup && b == b'<' {
            let mut j = i + 1;
            if bytes.get(j) == Some(&b'/') {
                j += 1;
            }
            if matches!(bytes.get(j), Some(&b'!') | Some(&b'?')) {
                j += 1;
            }
            let name_start = j;
            while j < len
                && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b'-' | b':' | b'_'))
            {
                j += 1;
            }
            if j > name_start {
                push_token(&mut out, i, j, palette.keyword);
                i = j;
                continue;
            }
        }

        if b.is_ascii_digit()
            || (b == b'.'
                && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)
                && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')))
        {
            let end = scan_number(bytes, i);
            push_token(&mut out, i, end, palette.number);
            i = end;
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            let mut j = i + 1;
            while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if lang.is_keyword(&text[start..j]) {
                push_token(&mut out, start, j, palette.keyword);
            }
            i = j;
            continue;
        }

        if !b.is_ascii() {
            i += text[i..].chars().next().map_or(1, |c| c.len_utf8());
        } else {
            i += 1;
        }
    }

    out
}

/// Continue a token opened on an earlier line. Returns the byte offset where normal
/// scanning should resume (or the line length when the token is still open).
fn resume_open(
    lang: Lang,
    bytes: &[u8],
    palette: &Palette,
    open: &mut Open,
    out: &mut Vec<(Range<usize>, Hsla)>,
) -> usize {
    let len = bytes.len();
    match *open {
        Open::None => 0,
        Open::BlockComment { mut depth } => {
            let nested = lang == Lang::Rust;
            let mut j = 0;
            let mut closed = false;
            while j + 1 < len {
                if nested && bytes[j] == b'/' && bytes[j + 1] == b'*' {
                    depth += 1;
                    j += 2;
                    continue;
                }
                if bytes[j] == b'*' && bytes[j + 1] == b'/' {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                    continue;
                }
                j += 1;
            }
            let end = if closed { j } else { len };
            push_token(out, 0, end, palette.comment);
            if closed {
                *open = Open::None;
            } else {
                *open = Open::BlockComment {
                    depth: depth.max(1),
                };
            }
            end
        }
        Open::MarkupComment => match find_subslice(bytes, b"-->") {
            Some(pos) => {
                let end = pos + 3;
                push_token(out, 0, end, palette.comment);
                *open = Open::None;
                end
            }
            None => {
                push_token(out, 0, len, palette.comment);
                len
            }
        },
        Open::Quoted { quote, escapes } => match scan_quoted_from(bytes, 0, quote, escapes) {
            Some(end) => {
                push_token(out, 0, end, palette.string);
                *open = Open::None;
                end
            }
            None => {
                push_token(out, 0, len, palette.string);
                len
            }
        },
        Open::Triple { quote } => match scan_triple_from(bytes, 0, quote) {
            Some(end) => {
                push_token(out, 0, end, palette.string);
                *open = Open::None;
                end
            }
            None => {
                push_token(out, 0, len, palette.string);
                len
            }
        },
        Open::RustRaw { hashes } => match scan_raw_from(bytes, 0, hashes) {
            Some(end) => {
                push_token(out, 0, end, palette.string);
                *open = Open::None;
                end
            }
            None => {
                push_token(out, 0, len, palette.string);
                len
            }
        },
        Open::Template => match scan_quoted_from(bytes, 0, b'`', true) {
            Some(end) => {
                push_token(out, 0, end, palette.string);
                *open = Open::None;
                end
            }
            None => {
                push_token(out, 0, len, palette.string);
                len
            }
        },
    }
}

/// Try to consume a string/character literal starting at `i`.
fn scan_string(
    lang: Lang,
    bytes: &[u8],
    i: usize,
    palette: &Palette,
    state: &mut SyntaxState,
    out: &mut Vec<(Range<usize>, Hsla)>,
) -> Option<usize> {
    let len = bytes.len();
    let b = bytes[i];

    match lang {
        Lang::Rust => {
            if b == b'"' {
                return Some(quoted_literal(
                    bytes, i, i, b'"', true, true, palette, state, out,
                ));
            }
            if b == b'\'' {
                return rust_char_end(bytes, i).map(|end| {
                    push_token(out, i, end, palette.string);
                    end
                });
            }
            if b == b'r' || b == b'b' {
                if let Some((end, carry)) = rust_raw_at(bytes, i) {
                    push_token(out, i, end, palette.string);
                    if let Some(hashes) = carry {
                        state.open = Open::RustRaw { hashes };
                    }
                    return Some(end);
                }
                if b == b'b' {
                    match bytes.get(i + 1) {
                        Some(b'"') => {
                            return Some(quoted_literal(
                                bytes,
                                i,
                                i + 1,
                                b'"',
                                true,
                                true,
                                palette,
                                state,
                                out,
                            ));
                        }
                        Some(b'\'') => {
                            if let Some(end) = rust_char_end(bytes, i + 1) {
                                push_token(out, i, end, palette.string);
                                return Some(end);
                            }
                        }
                        _ => {}
                    }
                }
            }
            None
        }
        Lang::Js => {
            if b == b'"' || b == b'\'' {
                let carry = bytes.last() == Some(&b'\\');
                return Some(quoted_literal(
                    bytes, i, i, b, true, carry, palette, state, out,
                ));
            }
            if b == b'`' {
                return Some(match scan_quoted_from(bytes, i + 1, b'`', true) {
                    Some(end) => {
                        push_token(out, i, end, palette.string);
                        end
                    }
                    None => {
                        push_token(out, i, len, palette.string);
                        state.open = Open::Template;
                        len
                    }
                });
            }
            None
        }
        Lang::CLike => {
            if b == b'"' {
                let carry = bytes.last() == Some(&b'\\');
                return Some(quoted_literal(
                    bytes, i, i, b'"', true, carry, palette, state, out,
                ));
            }
            if b == b'\'' {
                if let Some(end) = rust_char_end(bytes, i) {
                    push_token(out, i, end, palette.string);
                    return Some(end);
                }
                // C++ digit separators and apostrophes inside identifiers.
                if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_') {
                    return None;
                }
                let carry = bytes.last() == Some(&b'\\');
                return Some(quoted_literal(
                    bytes, i, i, b'\'', true, carry, palette, state, out,
                ));
            }
            None
        }
        Lang::Python => {
            let (start, quote_pos, quote, triple) = python_prefix(bytes, i)?;
            if triple {
                return Some(match scan_triple_from(bytes, quote_pos + 3, quote) {
                    Some(end) => {
                        push_token(out, start, end, palette.string);
                        end
                    }
                    None => {
                        push_token(out, start, len, palette.string);
                        state.open = Open::Triple { quote };
                        len
                    }
                });
            }
            let carry = bytes.last() == Some(&b'\\');
            Some(quoted_literal(
                bytes, start, quote_pos, quote, true, carry, palette, state, out,
            ))
        }
        Lang::Config => {
            if b != b'"' && b != b'\'' {
                return None;
            }
            if bytes.get(i + 1) == Some(&b) && bytes.get(i + 2) == Some(&b) {
                return Some(match scan_triple_from(bytes, i + 3, b) {
                    Some(end) => {
                        push_token(out, i, end, palette.string);
                        end
                    }
                    None => {
                        push_token(out, i, len, palette.string);
                        state.open = Open::Triple { quote: b };
                        len
                    }
                });
            }
            Some(quoted_literal(
                bytes, i, i, b, true, false, palette, state, out,
            ))
        }
        Lang::Shell => {
            if b == b'$' && bytes.get(i + 1) == Some(&b'\'') {
                return Some(quoted_literal(
                    bytes,
                    i,
                    i + 1,
                    b'\'',
                    true,
                    true,
                    palette,
                    state,
                    out,
                ));
            }
            if b == b'"' || b == b'\'' || b == b'`' {
                let escapes = b != b'\'';
                return Some(quoted_literal(
                    bytes, i, i, b, escapes, true, palette, state, out,
                ));
            }
            None
        }
        Lang::Markup => {
            if b == b'"' || b == b'\'' {
                return Some(quoted_literal(
                    bytes, i, i, b, true, false, palette, state, out,
                ));
            }
            None
        }
        Lang::Plain => None,
    }
}

/// Scan a quoted literal from `quote_pos + 1`. When it does not close, the rest of
/// the line is coloured and, if `carry`, the state is left open for the next line.
#[allow(clippy::too_many_arguments)]
fn quoted_literal(
    bytes: &[u8],
    color_start: usize,
    quote_pos: usize,
    quote: u8,
    escapes: bool,
    carry: bool,
    palette: &Palette,
    state: &mut SyntaxState,
    out: &mut Vec<(Range<usize>, Hsla)>,
) -> usize {
    match scan_quoted_from(bytes, quote_pos + 1, quote, escapes) {
        Some(end) => {
            push_token(out, color_start, end, palette.string);
            end
        }
        None => {
            let end = bytes.len();
            push_token(out, color_start, end, palette.string);
            if carry {
                state.open = Open::Quoted { quote, escapes };
            }
            end
        }
    }
}

/// Python/Config string prefix handling (`f"…"`, `rb'…'`, `b"""…"""`).
fn python_prefix(bytes: &[u8], i: usize) -> Option<(usize, usize, u8, bool)> {
    let len = bytes.len();
    let mut j = i;
    let mut prefix = 0;
    while prefix < 2
        && j < len
        && matches!(
            bytes[j],
            b'r' | b'R' | b'b' | b'B' | b'f' | b'F' | b'u' | b'U'
        )
    {
        j += 1;
        prefix += 1;
    }
    let quote = match bytes.get(j) {
        Some(b'"') => b'"',
        Some(b'\'') => b'\'',
        _ => return None,
    };
    let triple = j + 2 < len && bytes[j + 1] == quote && bytes[j + 2] == quote;
    Some((i, j, quote, triple))
}

/// Rust raw/byte-raw string at `start`: `r"…"`, `r#"…"#`, `br##"…"##`.
/// Returns the end offset and, when the literal runs past the line, the hash count.
fn rust_raw_at(bytes: &[u8], start: usize) -> Option<(usize, Option<u8>)> {
    let mut j = start;
    if bytes.get(j) == Some(&b'b') {
        j += 1;
    }
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0u8;
    while bytes.get(j) == Some(&b'#') {
        hashes = hashes.saturating_add(1);
        j += 1;
    }
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    match scan_raw_from(bytes, j + 1, hashes) {
        Some(end) => Some((end, None)),
        None => Some((bytes.len(), Some(hashes))),
    }
}

/// Rust char literal (`'x'`, `'\n'`, `'\u{1F600}'`). Lifetimes (`'a`) return `None`.
fn rust_char_end(bytes: &[u8], start: usize) -> Option<usize> {
    let len = bytes.len();
    let mut j = start + 1;
    if j >= len {
        return None;
    }

    if bytes[j] == b'\\' {
        j += 1;
        match bytes.get(j) {
            Some(b'u') => {
                if bytes.get(j + 1) != Some(&b'{') {
                    return None;
                }
                let mut k = j + 2;
                while k < len && bytes[k] != b'}' {
                    k += 1;
                }
                if k >= len {
                    return None;
                }
                j = k + 1;
            }
            Some(b'x') => j += 3,
            Some(_) => j += 1,
            None => return None,
        }
        return (bytes.get(j) == Some(&b'\'')).then_some(j + 1);
    }

    // Exactly one Unicode scalar followed by the closing quote.
    let ch_len = std::str::from_utf8(&bytes[j..])
        .ok()?
        .chars()
        .next()?
        .len_utf8();
    j += ch_len;
    (bytes.get(j) == Some(&b'\'')).then_some(j + 1)
}

fn scan_quoted_from(bytes: &[u8], mut j: usize, quote: u8, escapes: bool) -> Option<usize> {
    let len = bytes.len();
    while j < len {
        if escapes && bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

fn scan_triple_from(bytes: &[u8], mut j: usize, quote: u8) -> Option<usize> {
    let len = bytes.len();
    while j + 2 < len {
        if bytes[j] == quote && bytes[j + 1] == quote && bytes[j + 2] == quote {
            return Some(j + 3);
        }
        j += 1;
    }
    None
}

fn scan_raw_from(bytes: &[u8], mut j: usize, hashes: u8) -> Option<usize> {
    let len = bytes.len();
    let h = hashes as usize;
    while j < len {
        if bytes[j] == b'"' {
            let after = j + 1;
            if after + h <= len && bytes[after..after + h].iter().all(|&c| c == b'#') {
                return Some(after + h);
            }
        }
        j += 1;
    }
    None
}

fn scan_number(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    let mut j = start;
    if bytes[j] == b'.' {
        j += 1;
    }
    while j < len {
        let c = bytes[j];
        if c.is_ascii_alphanumeric() || c == b'_' {
            j += 1;
            continue;
        }
        // A single decimal point, but not the start of `..`/`..=`.
        if c == b'.' && bytes.get(j + 1).is_some_and(u8::is_ascii_digit) {
            j += 1;
            continue;
        }
        // Exponent sign (`1e-9`, `0x1p-3`).
        if matches!(c, b'+' | b'-')
            && j > start
            && matches!(bytes[j - 1], b'e' | b'E' | b'p' | b'P')
        {
            j += 1;
            continue;
        }
        break;
    }
    j
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn push_token(out: &mut Vec<(Range<usize>, Hsla)>, start: usize, end: usize, color: Hsla) {
    if start < end {
        out.push((start..end, color));
    }
}

/// Combine syntax foreground highlights with intraline diff background highlights
/// into non-overlapping `HighlightStyle` ranges for `StyledText`.
pub(crate) fn combine_line_highlights(
    text_len: usize,
    syntax: &[(Range<usize>, Hsla)],
    intraline_spans: &[Range<usize>],
    intraline_bg: Option<Hsla>,
    fallback_fg: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    if text_len == 0 {
        return Vec::new();
    }

    let mut points: Vec<usize> = Vec::with_capacity((syntax.len() + intraline_spans.len()) * 2 + 2);
    points.push(0);
    points.push(text_len);
    for (r, _) in syntax {
        if r.start < text_len {
            points.push(r.start);
        }
        if r.end <= text_len {
            points.push(r.end);
        }
    }
    for r in intraline_spans {
        if r.start < text_len {
            points.push(r.start);
        }
        if r.end <= text_len {
            points.push(r.end);
        }
    }
    points.sort_unstable();
    points.dedup();

    let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::with_capacity(points.len());

    for w in points.windows(2) {
        let start = w[0];
        let end = w[1];
        if start >= end || end > text_len {
            continue;
        }

        let color = syntax
            .iter()
            .find(|(r, _)| r.start <= start && end <= r.end)
            .map(|(_, c)| *c)
            .unwrap_or(fallback_fg);

        let bg = if let Some(bg_color) = intraline_bg {
            if intraline_spans
                .iter()
                .any(|r| r.start <= start && end <= r.end)
            {
                Some(bg_color)
            } else {
                None
            }
        } else {
            None
        };

        let style = HighlightStyle {
            color: Some(color),
            background_color: bg,
            ..Default::default()
        };

        if let Some((last_range, last_style)) = out.last_mut() {
            if *last_style == style && last_range.end == start {
                last_range.end = end;
                continue;
            }
        }

        out.push((start..end, style));
    }

    out
}
