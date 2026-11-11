//! What the browser is allowed to reach.
//!
//! Two rules, both enforced above the engine so every front inherits them —
//! the CLI and the MCP server:
//!
//! 1. Only `http` and `https`. Everything else is a hole rather than a page:
//!    `file:` reads the local disk (`file:///etc/hosts` is wide open by
//!    default), `javascript:` runs code in the page's origin, and `data:` /
//!    `blob:` sidestep a host allowlist entirely.
//! 2. An optional host allowlist. Empty means "any host", which is the default
//!    because a coding agent legitimately needs the whole web.
//!
//! `agent-browser` has its own `--allowed-domains`; that is defence in depth,
//! not the contract. The check here is what produces a sentence the agent can
//! act on instead of a CLI failure.

/// Schemes that are never a web page.
const DENIED_SCHEMES: &[(&str, &str)] = &[
    (
        "file",
        "file: reads the local disk. Use the read tool for local files.",
    ),
    (
        "javascript",
        "javascript: runs code in the page's origin. Use act/fill instead.",
    ),
    (
        "data",
        "data: pages bypass the host allowlist. Navigate to an http(s) URL.",
    ),
    (
        "blob",
        "blob: pages bypass the host allowlist. Navigate to an http(s) URL.",
    ),
    (
        "chrome",
        "chrome: pages expose browser internals. Not available to the agent.",
    ),
    (
        "devtools",
        "devtools: pages expose browser internals. Not available to the agent.",
    ),
    (
        "view-source",
        "view-source: is not a page. Use snapshot or read instead.",
    ),
    (
        "chrome-extension",
        "chrome-extension: pages are browser internals. Not available to the agent.",
    ),
];

/// The scheme of `value`, or `None` when it has none (a bare host or a path).
///
/// A port is not a scheme: `localhost:3000` must stay a host, and a scheme may
/// not contain a dot, so `example.com:8080` is a host too.
pub fn scheme_of(value: &str) -> Option<&str> {
    let (scheme, rest) = value.trim().split_once(':')?;
    if scheme.is_empty() || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-')
    {
        return None;
    }
    // `localhost:3000` is a host and a port, not a scheme. An empty segment is
    // not a port either: `https://…` splits to "" before the slash.
    let first_segment = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let port_like = !first_segment.is_empty() && first_segment.chars().all(|c| c.is_ascii_digit());
    if port_like {
        return None;
    }
    Some(scheme)
}

/// Reject a URL the agent may not open. `Err` carries the reason, verbatim, to
/// the agent — it is the only feedback it gets.
pub fn check_url(url: &str, allowed: &[String]) -> Result<(), String> {
    if let Some(scheme) = scheme_of(url) {
        let scheme = scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "http" | "https" => {}
            "about" if url.trim().eq_ignore_ascii_case("about:blank") => {}
            "about" => {
                return Err(
                    "about: pages other than about:blank are not available. Navigate to an \
                     http(s) URL."
                        .into(),
                );
            }
            _ => {
                let hint = DENIED_SCHEMES
                    .iter()
                    .find(|(name, _)| *name == scheme)
                    .map(|(_, hint)| *hint)
                    .unwrap_or("Only http and https URLs can be opened.");
                return Err(format!("refusing to open {scheme}: {hint}"));
            }
        }
    }

    if allowed.is_empty() {
        return Ok(());
    }
    match host_of(url) {
        Some(host) if domain_allowed(&host, allowed) => Ok(()),
        Some(host) => Err(format!(
            "refusing to open {host}: it is not in the allowed domains ({}). \
             The list is HALO_BROWSER_ALLOWED_DOMAINS.",
            allowed.join(", ")
        )),
        None => Err(format!(
            "refusing to open {url}: no host to check against the allowed domains ({}).",
            allowed.join(", ")
        )),
    }
}

/// The host of an absolute URL, lowercased, without userinfo or port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        // IPv6 literal: the host is everything up to the closing bracket.
        bracketed.split_once(']')?.0
    } else {
        authority.split(':').next()?
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// `example.com` matches `example.com` and `www.example.com`; `*.example.com`
/// is accepted as the same thing.
fn domain_allowed(host: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|entry| {
        let entry = entry.trim().trim_start_matches("*.").to_ascii_lowercase();
        if entry.is_empty() {
            return false;
        }
        host == entry || host.ends_with(&format!(".{entry}"))
    })
}

/// The allowlist from `HALO_BROWSER_ALLOWED_DOMAINS`, split on commas or
/// whitespace. Empty when unset, which means "any host".
pub fn allowed_domains_from_env() -> Vec<String> {
    std::env::var("HALO_BROWSER_ALLOWED_DOMAINS")
        .ok()
        .map(|value| {
            value
                .split([',', ' ', '\t', '\n'])
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn file_and_javascript_are_refused() {
        for url in [
            "file:///etc/hosts",
            "javascript:alert(1)",
            "data:text/html,<h1>x</h1>",
            "chrome://settings",
            "devtools://devtools/bundled/inspector.html",
            "view-source:https://example.com",
        ] {
            let error = check_url(url, &[]).unwrap_err();
            assert!(error.starts_with("refusing to open"), "{url}: {error}");
        }
    }

    #[test]
    fn http_and_blank_are_allowed() {
        for url in [
            "https://example.com/",
            "http://example.com:8080/a?b=1",
            "about:blank",
        ] {
            assert!(check_url(url, &[]).is_ok(), "{url}");
        }
    }

    #[test]
    fn about_pages_other_than_blank_are_refused() {
        assert!(check_url("about:config", &[]).is_err());
    }

    #[test]
    fn ports_are_not_schemes() {
        assert_eq!(scheme_of("localhost:3000/x"), None);
        assert_eq!(scheme_of("example.com:8080"), None);
        assert_eq!(scheme_of("javascript:void 0"), Some("javascript"));
        assert_eq!(scheme_of("https://a.b"), Some("https"));
    }

    #[test]
    fn allowlist_matches_subdomains_but_not_suffixes() {
        let list = allowed(&["example.com"]);
        assert!(check_url("https://example.com/", &list).is_ok());
        assert!(check_url("https://www.example.com/", &list).is_ok());
        assert!(check_url("https://notexample.com/", &list).is_err());
        assert!(check_url("https://example.com.evil.test/", &list).is_err());
    }

    #[test]
    fn wildcard_entry_is_the_same_as_a_bare_domain() {
        let list = allowed(&["*.example.com"]);
        assert!(check_url("https://api.example.com/", &list).is_ok());
        assert!(check_url("https://example.com/", &list).is_ok());
    }

    #[test]
    fn userinfo_cannot_smuggle_a_host() {
        let list = allowed(&["example.com"]);
        assert!(check_url("https://example.com@evil.test/", &list).is_err());
    }
}
