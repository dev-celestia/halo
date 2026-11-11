//! Postman import tests.

use super::*;

pub(crate) const SCHEMA_V21: &str =
    "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";
pub(crate) const SCHEMA_V20: &str =
    "https://schema.getpostman.com/json/collection/v2.0.0/collection.json";

fn find_folder<'a>(folders: &'a [ParsedFolder], name: &str) -> Option<&'a ParsedFolder> {
    for folder in folders {
        if folder.name == name {
            return Some(folder);
        }
        if let Some(found) = find_folder(&folder.folders, name) {
            return Some(found);
        }
    }
    None
}

fn find_request<'a>(folders: &'a [ParsedFolder], name: &str) -> Option<&'a ParsedRequest> {
    for folder in folders {
        if let Some(request) = folder.requests.iter().find(|request| request.name == name) {
            return Some(request);
        }
        if let Some(request) = find_request(&folder.folders, name) {
            return Some(request);
        }
    }
    None
}

fn headers_of(request: &ParsedRequest) -> Value {
    serde_json::from_str(&request.headers_json).unwrap()
}

#[test]
fn nested_folder_root_request_auth_and_variables() {
    let json = format!(
        r#"{{
            "info": {{"name": "Payments API", "schema": "{SCHEMA_V21}"}},
            "auth": {{
                "type": "bearer",
                "bearer": [{{"key": "token", "value": "{{{{token}}}}", "type": "string"}}]
            }},
            "event": [{{
                "listen": "prerequest",
                "script": {{"exec": ["collection-pre"]}}
            }}],
            "variable": [
                {{"key": "baseUrl", "value": "https://api.example.com"}},
                {{"key": "debug", "value": "0", "disabled": true}}
            ],
            "item": [
                {{
                    "name": "Users",
                    "event": [{{
                        "listen": "prerequest",
                        "script": {{"exec": ["folder-pre"]}}
                    }}],
                    "item": [{{
                        "name": "Admin",
                        "item": [{{
                            "name": "List",
                            "request": {{
                                "method": "GET",
                                "header": [
                                    {{"key": "Accept", "value": "application/json"}},
                                    {{"key": "Accept", "value": "text/plain"}},
                                    {{"key": "X-Debug", "value": "1", "disabled": true}}
                                ],
                                "url": {{"raw": "{{{{baseUrl}}}}/users"}}
                            }},
                            "event": [
                                {{"listen": "prerequest", "script": {{"exec": ["request-pre"]}}}},
                                {{"listen": "test", "script": {{"exec": ["pm.test('ok', function () {{}})"]}}}}
                            ]
                        }}]
                    }}]
                }},
                {{
                    "name": "Health",
                    "request": {{
                        "method": "get",
                        "url": {{
                            "protocol": "https",
                            "host": ["example", "com"],
                            "path": ["health"],
                            "query": [
                                {{"key": "verbose", "value": "1", "disabled": true}},
                                {{"key": "ok", "value": "yes"}}
                            ]
                        }}
                    }}
                }}
            ]
        }}"#
    );
    let import = parse_postman_export(&json).unwrap();
    let collection = &import.collections[0];
    assert_eq!(collection.name, "Payments API");

    let users = find_folder(&collection.folders, "Users").unwrap();
    let admin = find_folder(&users.folders, "Admin").unwrap();
    assert!(admin.requests.iter().any(|request| request.name == "List"));

    let list = find_request(&collection.folders, "List").unwrap();
    assert!(list.url.contains("{{baseUrl}}"));
    let headers = headers_of(list);
    assert_eq!(headers["Accept"], "application/json, text/plain");
    assert!(headers.get("X-Debug").is_none());
    assert_eq!(headers["Authorization"], "Bearer {{token}}");
    assert_eq!(list.pre_script, "collection-pre\nfolder-pre\nrequest-pre");
    assert!(list.test_script.contains("pm.test"));

    let requests = find_folder(&collection.folders, "Requests").unwrap();
    assert!(requests.folders.is_empty());
    let health = requests
        .requests
        .iter()
        .find(|request| request.name == "Health")
        .unwrap();
    assert_eq!(health.method, "GET");
    assert_eq!(health.url, "https://example.com/health?ok=yes");

    let variables: Value =
        serde_json::from_str(collection.variables_json.as_deref().unwrap()).unwrap();
    assert_eq!(variables["baseUrl"]["value"], "https://api.example.com");
    assert_eq!(variables["baseUrl"]["enabled"], true);
    assert_eq!(variables["debug"]["enabled"], false);
}

#[test]
fn urlencoded_body_and_environment_file() {
    let collection = format!(
        r#"{{
            "info": {{"name": "Login", "schema": "{SCHEMA_V20}"}},
            "item": [{{
                "name": "Sign in",
                "request": {{
                    "method": "POST",
                    "header": [],
                    "url": "https://example.com/login",
                    "body": {{
                        "mode": "urlencoded",
                        "urlencoded": [
                            {{"key": "user", "value": "ada"}},
                            {{"key": "token", "value": "{{{{token}}}}"}},
                            {{"key": "skip", "value": "1", "disabled": true}}
                        ]
                    }}
                }}
            }}]
        }}"#
    );
    let import = parse_postman_export(&collection).unwrap();
    let request = find_request(&import.collections[0].folders, "Sign in").unwrap();
    assert_eq!(request.body, "user=ada&token={{token}}");
    assert_eq!(request.body_type, "raw");
    assert_eq!(
        headers_of(request)["Content-Type"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(
        find_folder(&import.collections[0].folders, "Requests")
            .unwrap()
            .name,
        "Requests"
    );

    let environment = r#"{
        "name": "Dev",
        "values": [
            {"key": "host", "value": "localhost", "enabled": true},
            {"key": "off", "value": "x", "enabled": false}
        ],
        "_postman_variable_scope": "environment"
    }"#;
    let import = parse_postman_export(environment).unwrap();
    assert!(import.collections.is_empty());
    let env = &import.environments[0];
    assert_eq!(env.name, "Dev");
    let variables: Value = serde_json::from_str(&env.variables_json).unwrap();
    assert_eq!(variables["host"]["value"], "localhost");
    assert_eq!(variables["host"]["enabled"], true);
    assert_eq!(variables["off"]["enabled"], false);
}

#[test]
fn rejects_unrelated_json() {
    let err = parse_postman_export(r#"{"hello":"world"}"#).unwrap_err();
    assert!(err.contains("Not a Postman collection or environment"));
    assert!(
        parse_postman_export("{")
            .unwrap_err()
            .contains("Invalid JSON")
    );
}

#[test]
fn bundled_export_basic_auth_and_bodies() {
    let json = format!(
        r#"{{
            "collections": [{{
                "info": {{"name": "Ops", "schema": "{SCHEMA_V21}"}},
                "item": [
                    {{
                        "name": "Whoami",
                        "request": {{
                            "method": "GET",
                            "url": "https://example.com/me",
                            "auth": {{
                                "type": "basic",
                                "basic": [
                                    {{"key": "username", "value": "ada"}},
                                    {{"key": "password", "value": "s:ecret"}}
                                ]
                            }}
                        }}
                    }},
                    {{
                        "name": "Search",
                        "request": {{
                            "method": "GET",
                            "url": "https://example.com/search?q=1",
                            "auth": {{
                                "type": "apikey",
                                "apikey": [
                                    {{"key": "key", "value": "token"}},
                                    {{"key": "value", "value": "{{{{token}}}}"}},
                                    {{"key": "in", "value": "query"}}
                                ]
                            }}
                        }}
                    }},
                    {{
                        "name": "Keep header",
                        "request": {{
                            "method": "GET",
                            "header": [{{"key": "Authorization", "value": "Token already"}}],
                            "url": "https://example.com/keep",
                            "auth": {{
                                "type": "bearer",
                                "bearer": [{{"key": "token", "value": "nope"}}]
                            }}
                        }}
                    }},
                    {{
                        "name": "Upload",
                        "request": {{
                            "method": "POST",
                            "url": "https://example.com/upload",
                            "body": {{
                                "mode": "formdata",
                                "formdata": [
                                    {{"key": "name", "value": "ada", "type": "text"}},
                                    {{"key": "avatar", "type": "file", "src": "/tmp/a.png"}}
                                ]
                            }}
                        }}
                    }},
                    {{
                        "name": "Query",
                        "request": {{
                            "method": "POST",
                            "url": "https://example.com/graphql",
                            "body": {{
                                "mode": "graphql",
                                "graphql": {{
                                    "query": "{{ hello }}",
                                    "variables": "{{\"id\":1}}"
                                }}
                            }}
                        }}
                    }}
                ]
            }}],
            "environments": [{{
                "name": "Staging",
                "values": [{{"key": "host", "value": "staging", "enabled": true}}]
            }}]
        }}"#
    );
    let import = parse_postman_export(&json).unwrap();
    assert_eq!(import.collections[0].name, "Ops");
    assert_eq!(import.environments[0].name, "Staging");

    let whoami = find_request(&import.collections[0].folders, "Whoami").unwrap();
    let expected = base64::engine::general_purpose::STANDARD.encode("ada:s:ecret");
    assert_eq!(
        headers_of(whoami)["Authorization"],
        format!("Basic {expected}")
    );

    let search = find_request(&import.collections[0].folders, "Search").unwrap();
    assert_eq!(search.url, "https://example.com/search?q=1&token={{token}}");

    let keep = find_request(&import.collections[0].folders, "Keep header").unwrap();
    assert_eq!(headers_of(keep)["Authorization"], "Token already");

    let upload = find_request(&import.collections[0].folders, "Upload").unwrap();
    assert_eq!(upload.body_type, "raw");
    assert!(upload.body.contains("name=\"name\""));
    assert!(upload.body.contains("ada"));
    assert!(upload.body.contains("[file: /tmp/a.png]"));
    assert!(
        headers_of(upload)["Content-Type"]
            .as_str()
            .unwrap()
            .contains("multipart/form-data")
    );

    let query = find_request(&import.collections[0].folders, "Query").unwrap();
    assert_eq!(query.body_type, "json");
    let payload: Value = serde_json::from_str(&query.body).unwrap();
    assert_eq!(payload["query"], "{ hello }");
    assert_eq!(payload["variables"]["id"], 1);
}
