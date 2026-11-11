//! Top-level export parsing: bundle detection, collection walk, request items.

use super::*;

/// Parse one Postman export. `text` may be a collection, an environment, or a
/// dump that contains `collections` and/or `environments` arrays.
pub(crate) fn parse_postman_export(text: &str) -> Result<PostmanImport, String> {
    let text = text.trim().trim_start_matches('\u{feff}');
    let value: Value = serde_json::from_str(text).map_err(|_| "Invalid JSON".to_string())?;
    let import = parse_value(&value)?;
    if import.collections.is_empty() && import.environments.is_empty() {
        return Err("Not a Postman collection or environment".into());
    }
    Ok(import)
}

pub(crate) fn parse_value(value: &Value) -> Result<PostmanImport, String> {
    if is_collection(value) {
        return Ok(PostmanImport {
            collections: vec![parse_collection(value)],
            environments: Vec::new(),
        });
    }
    if is_explicit_environment(value) {
        return Ok(PostmanImport {
            collections: Vec::new(),
            environments: vec![parse_environment(value)],
        });
    }
    if value.get("collections").and_then(Value::as_array).is_some()
        || value
            .get("environments")
            .and_then(Value::as_array)
            .is_some()
    {
        return parse_bundle(value);
    }
    if is_loose_environment(value) {
        return Ok(PostmanImport {
            collections: Vec::new(),
            environments: vec![parse_environment(value)],
        });
    }
    Err("Not a Postman collection or environment".into())
}

pub(crate) fn parse_bundle(value: &Value) -> Result<PostmanImport, String> {
    let mut import = PostmanImport::default();
    if let Some(collections) = value.get("collections").and_then(Value::as_array) {
        for item in collections {
            if !is_collection(item) {
                return Err("Not a Postman collection or environment".into());
            }
            import.collections.push(parse_collection(item));
        }
    }
    if let Some(environments) = value.get("environments").and_then(Value::as_array) {
        for item in environments {
            if !is_explicit_environment(item) && !is_loose_environment(item) {
                return Err("Not a Postman collection or environment".into());
            }
            import.environments.push(parse_environment(item));
        }
    }
    if import.collections.is_empty() && import.environments.is_empty() {
        return Err("Not a Postman collection or environment".into());
    }
    Ok(import)
}

pub(crate) fn is_collection(value: &Value) -> bool {
    value
        .get("info")
        .and_then(|info| info.get("schema"))
        .and_then(Value::as_str)
        .is_some_and(|schema| schema.contains("schema.getpostman.com/json/collection"))
}

pub(crate) fn is_explicit_environment(value: &Value) -> bool {
    value
        .get("_postman_variable_scope")
        .and_then(Value::as_str)
        .is_some_and(|scope| scope.eq_ignore_ascii_case("environment"))
}

pub(crate) fn is_loose_environment(value: &Value) -> bool {
    if is_collection(value) {
        return false;
    }
    let has_name = value.get("name").and_then(Value::as_str).is_some();
    let has_values = value.get("values").and_then(Value::as_array).is_some();
    has_name && has_values
}

pub(crate) fn parse_collection(value: &Value) -> ParsedCollection {
    let name = non_empty_name(
        value.get("info").and_then(|info| info.get("name")),
        "Imported collection",
    );
    let variables = variable_list(value.get("variable"));
    let variables_json = if variables.is_empty() {
        None
    } else {
        Some(variables_to_json(&variables))
    };
    let inherited = Inherited {
        auth: value.get("auth"),
        pre: "",
        test: "",
    };
    let pre = script_text(value.get("event"), "prerequest");
    let test = script_text(value.get("event"), "test");
    let inherited = Inherited {
        auth: inherited.auth,
        pre: &pre,
        test: &test,
    };
    let items = value.get("item").and_then(Value::as_array);
    let (mut folders, loose) = match items {
        Some(items) => walk_items(items, &inherited),
        None => (Vec::new(), Vec::new()),
    };
    if !loose.is_empty() {
        folders.push(ParsedFolder {
            name: REQUESTS_FOLDER.into(),
            folders: Vec::new(),
            requests: loose,
        });
    }
    ParsedCollection {
        name,
        folders,
        variables_json,
    }
}

pub(crate) fn parse_environment(value: &Value) -> ParsedEnvironment {
    let name = non_empty_name(value.get("name"), "Environment");
    let variables = variable_list(value.get("values"));
    ParsedEnvironment {
        name,
        variables_json: variables_to_json(&variables),
    }
}

pub(crate) fn walk_items(
    items: &[Value],
    inherited: &Inherited<'_>,
) -> (Vec<ParsedFolder>, Vec<ParsedRequest>) {
    let mut folders = Vec::new();
    let mut requests = Vec::new();
    for item in items {
        if is_request_item(item) {
            requests.push(parse_request_item(item, inherited));
            continue;
        }
        let Some(children) = item.get("item").and_then(Value::as_array) else {
            continue;
        };
        let folder_pre = join_scripts(inherited.pre, &script_text(item.get("event"), "prerequest"));
        let folder_test = join_scripts(inherited.test, &script_text(item.get("event"), "test"));
        let child_inherited = Inherited {
            auth: resolve_auth(item.get("auth"), inherited.auth),
            pre: &folder_pre,
            test: &folder_test,
        };
        let (subfolders, child_requests) = walk_items(children, &child_inherited);
        folders.push(ParsedFolder {
            name: non_empty_name(item.get("name"), "Folder"),
            folders: subfolders,
            requests: child_requests,
        });
    }
    (folders, requests)
}

pub(crate) fn is_request_item(item: &Value) -> bool {
    matches!(
        item.get("request"),
        Some(Value::String(_)) | Some(Value::Object(_))
    )
}

pub(crate) fn parse_request_item(item: &Value, inherited: &Inherited<'_>) -> ParsedRequest {
    let name = non_empty_name(item.get("name"), "Request");
    let request = item.get("request");
    let (method, mut url, mut headers, body, body_type) = match request {
        Some(Value::String(url)) => (
            "GET".into(),
            url.clone(),
            Map::new(),
            String::new(),
            "none".into(),
        ),
        Some(Value::Object(_)) => {
            let request = request.unwrap();
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .unwrap_or("GET")
                .to_ascii_uppercase();
            let url = request.get("url").map(rebuild_url).unwrap_or_default();
            let mut headers = parse_headers(request.get("header"));
            let (body, body_type) = parse_body(request.get("body"), &mut headers);
            (method, url, headers, body, body_type)
        }
        _ => (
            "GET".into(),
            String::new(),
            Map::new(),
            String::new(),
            "none".into(),
        ),
    };

    let own_auth = request
        .and_then(|r| r.get("auth"))
        .or_else(|| item.get("auth"));
    apply_auth(
        &mut headers,
        &mut url,
        resolve_auth(own_auth, inherited.auth),
    );

    let events = item
        .get("event")
        .or_else(|| request.and_then(|r| r.get("event")));
    ParsedRequest {
        name,
        method,
        url,
        headers_json: Value::Object(headers).to_string(),
        body,
        body_type,
        pre_script: join_scripts(inherited.pre, &script_text(events, "prerequest")),
        test_script: join_scripts(inherited.test, &script_text(events, "test")),
    }
}
