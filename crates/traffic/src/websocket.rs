use std::collections::HashMap;
use url::Url;

pub fn parse_websocket_target(
    req_uri: &str,
    headers: &HashMap<String, String>,
) -> (String, String, String) {
    if let Ok(parsed) = Url::parse(req_uri) {
        let host = parsed.host_str().unwrap_or_default().to_string();
        let mut path = parsed.path().to_string();
        if let Some(query) = parsed.query() {
            path.push('?');
            path.push_str(query);
        }
        let scheme = match parsed.scheme() {
            "https" | "wss" => "wss",
            _ => "ws",
        };
        let url = format!("{scheme}://{host}{path}");
        return (host, path, url);
    }

    let host = header_value(headers, "host").cloned().unwrap_or_default();
    let path = if req_uri.is_empty() {
        "/".to_string()
    } else {
        req_uri.to_string()
    };
    let scheme = if header_value(headers, "x-forwarded-proto")
        .map(|value| value.eq_ignore_ascii_case("https"))
        .unwrap_or(false)
    {
        "wss"
    } else {
        "ws"
    };
    let url = if host.is_empty() {
        path.clone()
    } else {
        format!("{scheme}://{host}{path}")
    };

    (host, path, url)
}

fn header_value<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a String> {
    headers
        .iter()
        .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_websocket_target_from_host_header() {
        let mut headers = HashMap::new();
        headers.insert("Host".to_string(), "example.test".to_string());

        let (host, path, url) = parse_websocket_target("/socket?room=1", &headers);

        assert_eq!(host, "example.test");
        assert_eq!(path, "/socket?room=1");
        assert_eq!(url, "ws://example.test/socket?room=1");
    }

    #[test]
    fn parses_absolute_websocket_target() {
        let headers = HashMap::new();

        let (host, path, url) = parse_websocket_target("wss://example.test/ws?q=1", &headers);

        assert_eq!(host, "example.test");
        assert_eq!(path, "/ws?q=1");
        assert_eq!(url, "wss://example.test/ws?q=1");
    }
}
