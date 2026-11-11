//! Links out of a web-search result.
//!
//! OpenCode's `websearch` returns Exa's plain-text block format:
//!
//! ```text
//! Title: TcpStream in tokio::net - Rust
//! URL: https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html
//! Published: N/A
//! Author: N/A
//! Highlights:
//! …page text…
//! ```
//!
//! The transcript wants the title and URL only. Parsing is structural, not
//! keyed on a tool name, so any harness or future `halo-browser search` that
//! emits the same blocks gets a source list too — and a page fetch, which
//! does not, is left alone.

use crate::work::HaloAgentSource;

/// More than any search returns (Exa defaults to 8); keeps a transcript row
/// bounded even if a model pages through results.
const MAX_SOURCES: usize = 12;

pub fn parse_search_sources(text: &str) -> Vec<HaloAgentSource> {
    // A search result starts with its first block; a fetched page does not, and
    // must not grow a source list out of whatever "Title:/URL:" lines it holds.
    let text = text.trim_start();
    if !text.starts_with("Title: ") {
        return Vec::new();
    }

    let mut sources: Vec<HaloAgentSource> = Vec::new();
    let mut title: Option<&str> = None;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Title: ") {
            title = Some(rest.trim());
            continue;
        }
        let Some(rest) = line.strip_prefix("URL: ") else {
            // A block can carry the title but an unparsable URL; drop it.
            if line.starts_with("URL:") {
                title = None;
            }
            continue;
        };
        let Some(title) = title.take() else { continue };
        let url = rest.trim();
        if title.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
            continue;
        }
        if sources.iter().any(|source| source.url == url) {
            continue;
        }
        sources.push(HaloAgentSource {
            title: title.to_string(),
            url: url.to_string(),
        });
        if sources.len() >= MAX_SOURCES {
            break;
        }
    }

    sources
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Title: TcpStream in tokio::net - Rust\n\
URL: https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html\n\
Published: N/A\n\
Author: N/A\n\
Highlights:\n\
# Struct TcpStream\n\n\
Title: tokio-rs/tokio: A runtime for writing reliable network applications\n\
URL: https://github.com/tokio-rs/tokio\n\
Published: N/A\n\
Author: N/A\n\
Highlights:\n\
Tokio is an asynchronous runtime.\n";

    #[test]
    fn parses_every_block() {
        let sources = parse_search_sources(SAMPLE);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].title, "TcpStream in tokio::net - Rust");
        assert_eq!(sources[1].url, "https://github.com/tokio-rs/tokio");
    }

    #[test]
    fn ignores_text_without_blocks() {
        assert!(parse_search_sources("just page content\nno markers here").is_empty());
        assert!(parse_search_sources("").is_empty());
        // A fetched page that happens to contain marker lines is not a result
        // list: Exa's output always opens with its first block.
        assert!(
            parse_search_sources("# Docs\n\nTitle: Example\nURL: https://example.com\n").is_empty()
        );
    }

    #[test]
    fn drops_duplicate_urls() {
        let text = "Title: A\nURL: https://a.example/x\n\
                    Title: A again\nURL: https://a.example/x\n";
        assert_eq!(parse_search_sources(text).len(), 1);
    }

    #[test]
    fn drops_non_http_urls_and_orphan_titles() {
        let text = "Title: Local\nURL: file:///etc/passwd\n\
                    Title: Orphan\n\
                    URL: https://ok.example/page\n";
        let sources = parse_search_sources(text);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].url, "https://ok.example/page");
        assert_eq!(sources[0].title, "Orphan");
    }

    #[test]
    fn caps_the_list() {
        let mut text = String::new();
        for index in 0..40 {
            text.push_str(&format!(
                "Title: T{index}\nURL: https://e.example/{index}\n"
            ));
        }
        assert_eq!(parse_search_sources(&text).len(), MAX_SOURCES);
    }
}
