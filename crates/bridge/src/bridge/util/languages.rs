//! File-path to language / highlighter mapping.

/// Tree-sitter / highlighter language id (must match gpui-component registry names).
pub(crate) fn coder_highlighter_for_path(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "md" | "markdown" => "markdown",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "py" | "pyi" | "pyw" => "python",
        "go" => "go",
        "json" => "json",
        "css" | "scss" => "css",
        "html" | "htm" => "html",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" | "zsh" | "ksh" => "bash",
        "http" => "http",
        _ => "plaintext",
    }
}

/// LSP `textDocument/didOpen` languageId (VS Code / tsserver conventions for JSX/TSX).
pub(crate) fn coder_lsp_language_for_path(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascriptreact",
        "py" | "pyi" | "pyw" => "python",
        "go" => "go",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "sh" | "bash" | "zsh" | "ksh" => "shellscript",
        _ => "plaintext",
    }
}

/// Back-compat alias: highlighter id used when a single language string is needed.
#[allow(dead_code)]
pub(crate) fn coder_language_for_path(path: &str) -> &'static str {
    coder_highlighter_for_path(path)
}

#[cfg(test)]
mod language_mapping_tests {
    use super::{coder_highlighter_for_path, coder_lsp_language_for_path};

    #[test]
    fn shell_scripts_use_bash_grammar_and_shellscript_lsp() {
        for path in ["run.sh", "build.bash", "env.zsh", "aliases.ksh"] {
            assert_eq!(coder_highlighter_for_path(path), "bash", "{path}");
            assert_eq!(coder_lsp_language_for_path(path), "shellscript", "{path}");
        }
    }

    #[test]
    fn python_files_map_to_python() {
        for path in ["main.py", "stubs.pyi", "gui.pyw"] {
            assert_eq!(coder_highlighter_for_path(path), "python", "{path}");
            assert_eq!(coder_lsp_language_for_path(path), "python", "{path}");
        }
    }

    #[test]
    fn javascript_files_keep_server_language_ids() {
        for path in ["index.js", "module.mjs", "legacy.cjs"] {
            assert_eq!(coder_highlighter_for_path(path), "javascript", "{path}");
            assert_eq!(coder_lsp_language_for_path(path), "javascript", "{path}");
        }
        assert_eq!(coder_lsp_language_for_path("view.jsx"), "javascriptreact");
    }
}
