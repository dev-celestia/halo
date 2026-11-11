//! `http` wire-text syntax highlighting for the network editors
//! (Repeater request/response, HTTP history detail, Intercept).
//!
//! The editors are constructed as `CodeEditor::new("http", …)`, but that
//! language is not one of gpui-component's built-in grammars, so the text
//! renders uncolored until a grammar is registered under that name at
//! startup ([`install`]).
//!
//! The grammar models REST-client style documents: request/status line,
//! `Key: value` headers, `{{variable}}` placeholders, and JSON/XML bodies.
//! Bodies are injected into the built-in `json` / `html` grammars so they
//! get full editor colors. Compact single-line bodies and other raw
//! payloads stay plain — only the request line, headers, and structured
//! bodies are colored.

use gpui_component::highlighter::{GrammarConfig, LanguageRegistry};

/// Highlights for the HTTP structure itself. Header patterns are field-
/// qualified so error-recovery artifacts (e.g. the placeholder comments
/// shown before the first request) stay uncolored; the URL match requires
/// a path or scheme so orphaned fragments of an unterminated body are not
/// painted as links.
const HIGHLIGHTS_QUERY: &str = r#"
(method) @keyword
(http_version) @constant
(status_code) @constant
(status_text) @type
((request url: (target_url) @link_uri) (#match? @link_uri "^(?:/|[a-zA-Z][a-zA-Z0-9+.-]*://)"))
(header name: (header_entity) @tag)
(header value: (value) @string)
(variable) @variable.special
(variable_declaration name: (identifier) @variable.special)
(variable_declaration value: (value) @string)
(comment) @comment
(request_separator) @comment
":" @punctuation.delimiter
"#;

/// Bodies are highlighted by the injected grammar, not the HTTP grammar.
/// `graphql_json_body` shares the `json_body` node kind, so it is covered
/// by the first pattern too.
const INJECTIONS_QUERY: &str = r#"
((json_body) @injection.content (#set! injection.language "json"))
((xml_body) @injection.content (#set! injection.language "html"))
"#;

/// Register the `http` grammar in the global [`LanguageRegistry`].
/// Idempotent; call once at startup before any editor is created.
pub fn install() {
    LanguageRegistry::singleton().register(
        "http",
        &GrammarConfig::new(
            "http",
            tree_sitter_http::language(),
            vec!["json".into(), "html".into()],
            HIGHLIGHTS_QUERY,
            INJECTIONS_QUERY,
            "",
        ),
    );
}
