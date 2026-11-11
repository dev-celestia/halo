use std::collections::HashMap;

use crate::types::{InvokerHttpRequest, InvokerPayloadPosition};

pub fn position_name(positions: &[InvokerPayloadPosition], index: usize) -> String {
    positions
        .get(index)
        .map(|position| position.name.clone())
        .unwrap_or_else(|| format!("position_{}", index + 1))
}

pub fn count_markers(request: &InvokerHttpRequest) -> usize {
    marker_defaults(request).len()
}

pub fn marker_defaults(request: &InvokerHttpRequest) -> Vec<String> {
    let mut defaults = Vec::new();
    collect_marked_values(&request.url, &mut defaults);
    let mut sorted_headers: Vec<_> = request
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("host"))
        .collect();
    sorted_headers.sort_by_key(|(k, _)| k.to_lowercase());
    for (name, value) in sorted_headers {
        collect_marked_values(name, &mut defaults);
        collect_marked_values(value, &mut defaults);
    }
    collect_marked_values(&request.body, &mut defaults);
    defaults
}

pub fn collect_marked_values(text: &str, values: &mut Vec<String>) {
    let mut search_start = 0;
    while let Some(start) = text[search_start..].find('§') {
        let absolute_start = search_start + start;
        if let Some(end) = text[absolute_start + '§'.len_utf8()..].find('§') {
            let absolute_end = absolute_start + '§'.len_utf8() + end;
            values.push(text[absolute_start + '§'.len_utf8()..absolute_end].to_string());
            search_start = absolute_end + '§'.len_utf8();
        } else {
            break;
        }
    }
}

pub fn replace_marked_values_tracked(
    text: &str,
    payload_values: &HashMap<String, String>,
    defaults: &[String],
    position_index: &mut usize,
) -> String {
    let mut output = String::with_capacity(text.len());
    let mut search_start = 0;

    while let Some(start) = text[search_start..].find('§') {
        let absolute_start = search_start + start;
        let Some(end) = text[absolute_start + '§'.len_utf8()..].find('§') else {
            break;
        };
        let absolute_end = absolute_start + '§'.len_utf8() + end;
        let position_key = format!("position_{}", *position_index + 1);
        let replacement = payload_values
            .get(&position_key)
            .cloned()
            .unwrap_or_else(|| defaults.get(*position_index).cloned().unwrap_or_default());

        output.push_str(&text[search_start..absolute_start]);
        output.push_str(&replacement);
        search_start = absolute_end + '§'.len_utf8();
        *position_index += 1;
    }

    output.push_str(&text[search_start..]);
    output
}
