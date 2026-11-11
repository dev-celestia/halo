//! § marker helpers for Intruder request templates.

/// Strip every section marker from `text`.
pub fn clear_markers(text: &str) -> String {
    text.replace('§', "")
}

/// Wrap URL query values and simple `key=value` body fields in §markers§.
pub fn auto_mark_request(url: &str, body: &str) -> (String, String) {
    (auto_mark_url(url), auto_mark_form_body(body))
}

fn auto_mark_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    if query.is_empty() {
        return url.to_string();
    }
    let marked: Vec<String> = query
        .split('&')
        .map(|pair| {
            if let Some((key, value)) = pair.split_once('=') {
                if value.contains('§') {
                    pair.to_string()
                } else {
                    format!("{key}=§{value}§")
                }
            } else {
                pair.to_string()
            }
        })
        .collect();
    format!("{base}?{}", marked.join("&"))
}

fn auto_mark_form_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed.starts_with('{') || trimmed.starts_with('[') {
        return auto_mark_json_values(body);
    }
    if !trimmed.contains('=') {
        return body.to_string();
    }
    body.lines()
        .map(|line| {
            let line = line.trim_end();
            if let Some((key, value)) = line.split_once('=') {
                if value.contains('§') || key.contains(':') {
                    line.to_string()
                } else {
                    format!("{key}=§{value}§")
                }
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Best-effort JSON string value wrapping for simple `"key": "value"` pairs.
fn auto_mark_json_values(body: &str) -> String {
    let mut output = String::with_capacity(body.len() + 16);
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            // Find closing quote of a string.
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            if i >= bytes.len() {
                output.push_str(&body[start..]);
                break;
            }
            let string_end = i;
            i += 1;
            // Skip whitespace
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b':' {
                // This was a key — copy as-is, then mark the following string value.
                output.push_str(&body[start..=string_end]);
                i = j + 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    output.push(bytes[i] as char);
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'"' {
                    let val_start = i;
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if bytes[i] == b'"' {
                            break;
                        }
                        i += 1;
                    }
                    if i < bytes.len() {
                        let inner = &body[val_start + 1..i];
                        if inner.contains('§') {
                            output.push_str(&body[val_start..=i]);
                        } else {
                            output.push('"');
                            output.push('§');
                            output.push_str(inner);
                            output.push('§');
                            output.push('"');
                        }
                        i += 1;
                    }
                }
                continue;
            }
            output.push_str(&body[start..=string_end]);
            continue;
        }
        output.push(bytes[i] as char);
        i += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_query_values() {
        let (url, _) = auto_mark_request("https://ex.test/a?u=admin&p=secret", "");
        assert_eq!(url, "https://ex.test/a?u=§admin§&p=§secret§");
    }

    #[test]
    fn clears_markers() {
        assert_eq!(clear_markers("u=§admin§"), "u=admin");
    }
}
