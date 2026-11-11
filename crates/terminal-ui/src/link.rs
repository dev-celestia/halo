//! Clickable-URL discovery over one terminal row.
//!
//! The VT grid is cell-addressed, so a link has to be reported in *columns*
//! rather than bytes or chars. The caller therefore hands us exactly one char
//! per column (spacer/blank cells included), and every index we return can be
//! used straight as a `Column`.

/// A half-open column span `[start, end)` that holds one `http(s)://` URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkSpan {
    /// Column of the URL's first character.
    pub start: usize,
    /// Column just past the URL's last character.
    pub end: usize,
    /// The URL as printed, scheme included.
    pub url: String,
}

/// Schemes we recognise. Deliberately narrow: `ftp://` and bare domains are
/// left alone so a path like `httpx://` never turns into a browser trip.
const SCHEMES: [&str; 2] = ["http://", "https://"];

/// Characters that can appear inside a URL body (RFC 3986 unreserved plus the
/// sub-delims/gen-delims terminals actually print).
fn is_url_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '-' | '.'
                | '_'
                | '~'
                | ':'
                | '/'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '%'
        )
}

/// Trailing punctuation that is far more likely to end the sentence than the
/// link — `See https://example.com.` should not open `example.com.`.
fn is_trailing_punct(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | ')' | ']' | '}'
    )
}

/// Length of a scheme starting at `at`, case-insensitively.
fn scheme_len(cols: &[char], at: usize) -> Option<usize> {
    SCHEMES.iter().find_map(|scheme| {
        let len = scheme.len();
        if at + len > cols.len() {
            return None;
        }
        let matched = cols[at..at + len]
            .iter()
            .zip(scheme.chars())
            .all(|(c, s)| c.eq_ignore_ascii_case(&s));
        matched.then_some(len)
    })
}

/// Drop sentence punctuation from the tail, keeping a `)` that closes a `(`
/// inside the URL (`/Rust_(language)`) and dropping one that does not
/// (`(see https://example.com)`).
fn trim_tail(cols: &[char], body_start: usize, mut end: usize) -> usize {
    let body = &cols[body_start..end];
    let opens = body.iter().filter(|c| **c == '(').count();
    let mut closes = body.iter().filter(|c| **c == ')').count();
    while end > body_start && is_trailing_punct(cols[end - 1]) {
        if cols[end - 1] == ')' {
            if closes > opens {
                closes -= 1;
            } else {
                break;
            }
        }
        end -= 1;
    }
    end
}

/// Every URL in one row of cells, left to right and non-overlapping.
pub fn find_links(cols: &[char]) -> Vec<LinkSpan> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < cols.len() {
        let Some(scheme) = scheme_len(cols, i) else {
            i += 1;
            continue;
        };
        let body_start = i + scheme;
        let mut end = body_start;
        while end < cols.len() && is_url_char(cols[end]) {
            end += 1;
        }
        // A bare scheme with nothing after it is not a link.
        if end == body_start {
            i += 1;
            continue;
        }
        let end = trim_tail(cols, body_start, end);
        if end > body_start {
            spans.push(LinkSpan {
                start: i,
                end,
                url: cols[i..end].iter().collect(),
            });
            // Continue past the link so its own text cannot seed a second hit.
            i = end;
            continue;
        }
        i = body_start;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(text: &str) -> Vec<LinkSpan> {
        let cols: Vec<char> = text.chars().collect();
        find_links(&cols)
    }

    #[test]
    fn finds_a_plain_url() {
        let hits = spans("run https://example.com/a?b=1 now");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://example.com/a?b=1");
        assert_eq!(hits[0].start, 4);
        assert_eq!(hits[0].end, 29);
    }

    #[test]
    fn finds_http_and_is_case_insensitive() {
        assert_eq!(spans("HTTP://Example.COM")[0].url, "HTTP://Example.COM");
        assert_eq!(spans("hTtP://x.dev")[0].url, "hTtP://x.dev");
    }

    #[test]
    fn reports_columns_not_byte_offsets() {
        // Two wide cells ahead of the URL: the span must still land on 'h'.
        let hits = spans("中文 https://a.io");
        assert_eq!(hits[0].start, 3);
        assert_eq!(hits[0].url, "https://a.io");
    }

    #[test]
    fn trims_sentence_punctuation() {
        assert_eq!(
            spans("see https://example.com.")[0].url,
            "https://example.com"
        );
        assert_eq!(spans("at https://x.io/a,")[0].url, "https://x.io/a");
        assert_eq!(spans("https://x.io?q=1!")[0].url, "https://x.io?q=1");
    }

    #[test]
    fn keeps_balanced_parens_and_drops_unbalanced() {
        assert_eq!(
            spans("https://en.wikipedia.org/wiki/Rust_(language)").len(),
            1
        );
        assert_eq!(
            spans("https://en.wikipedia.org/wiki/Rust_(language)")[0].url,
            "https://en.wikipedia.org/wiki/Rust_(language)"
        );
        assert_eq!(spans("(from https://x.io/a)")[0].url, "https://x.io/a");
    }

    #[test]
    fn needs_something_after_the_scheme() {
        assert!(spans("http://").is_empty());
        assert!(spans("https://  spaced").is_empty());
    }

    #[test]
    fn finds_several_links_on_one_row() {
        let hits = spans("GET https://a.io/x POST https://b.io/y");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://a.io/x");
        assert_eq!(hits[1].url, "https://b.io/y");
        assert!(hits[1].start > hits[0].end);
    }

    #[test]
    fn ignores_non_http_schemes() {
        assert!(spans("ftp://files.example.com").is_empty());
        assert!(spans("example.com/path").is_empty());
    }

    #[test]
    fn blank_row_yields_nothing() {
        assert!(spans("").is_empty());
        assert!(spans("       ").is_empty());
    }
}
