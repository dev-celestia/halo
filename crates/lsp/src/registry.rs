//! Built-in language-server catalog, PATH detection, and install helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{LanguageServerSpec, LspError, LspResult};

/// One entry in the Coder “Language Servers” settings list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageServerCatalogEntry {
    /// Stable id (`rust-analyzer`, `typescript-language-server`, …).
    pub id: &'static str,
    pub display_name: &'static str,
    /// Language ids this server handles (highlighter / LSP `languageId`).
    pub language_ids: &'static [&'static str],
    /// Human-readable language labels for the UI.
    pub language_labels: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// Shell commands tried in order for “Install” (first success wins).
    pub install_commands: &'static [&'static str],
    /// Shown when auto-install is unavailable or fails.
    pub install_hint: &'static str,
}

/// Full catalog of servers Coder knows how to detect / install.
pub fn catalog() -> &'static [LanguageServerCatalogEntry] {
    &CATALOG
}

#[cfg(unix)]
const RUST_ANALYZER_INSTALL_COMMANDS: &[&str] = &[
    "rustup component add rust-analyzer",
    // No rustup yet — install the toolchain non-interactively, then the LSP.
    "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && . \"$HOME/.cargo/env\" && rustup component add rust-analyzer",
];

#[cfg(windows)]
const RUST_ANALYZER_INSTALL_COMMANDS: &[&str] = &["rustup component add rust-analyzer"];

const CATALOG: &[LanguageServerCatalogEntry] = &[
    LanguageServerCatalogEntry {
        id: "rust-analyzer",
        display_name: "rust-analyzer",
        language_ids: &["rust"],
        language_labels: "Rust",
        command: "rust-analyzer",
        args: &[],
        install_commands: RUST_ANALYZER_INSTALL_COMMANDS,
        install_hint: "Install Rust from https://rustup.rs, then: rustup component add rust-analyzer",
    },
    LanguageServerCatalogEntry {
        id: "typescript-language-server",
        display_name: "TypeScript Language Server",
        language_ids: &[
            "typescript",
            "javascript",
            "typescriptreact",
            "javascriptreact",
        ],
        language_labels: "TypeScript, JavaScript, TSX, JSX",
        command: "typescript-language-server",
        args: &["--stdio"],
        // Pin TypeScript 5: 7.x dropped `tsserver.js`, which the language
        // server still needs to start.
        install_commands: &["npm install -g typescript@5 typescript-language-server"],
        install_hint: "Install via npm: npm install -g typescript@5 typescript-language-server",
    },
    LanguageServerCatalogEntry {
        id: "pyright",
        display_name: "Pyright",
        language_ids: &["python"],
        language_labels: "Python",
        command: "pyright-langserver",
        args: &["--stdio"],
        install_commands: &[
            "npm install -g pyright",
            "pip3 install pyright",
            "pip install pyright",
        ],
        install_hint: "Install via npm: npm install -g pyright (or pip install pyright)",
    },
    LanguageServerCatalogEntry {
        id: "bash-language-server",
        display_name: "Bash Language Server",
        language_ids: &["shellscript"],
        language_labels: "Shell (Bash, sh, Zsh, Ksh)",
        command: "bash-language-server",
        args: &["start"],
        install_commands: &[
            "npm install -g bash-language-server",
            "brew install bash-language-server",
        ],
        install_hint: "Install via npm: npm install -g bash-language-server. Install shellcheck too for lint diagnostics.",
    },
    LanguageServerCatalogEntry {
        id: "gopls",
        display_name: "gopls",
        language_ids: &["go"],
        language_labels: "Go",
        command: "gopls",
        args: &[],
        install_commands: &["go install golang.org/x/tools/gopls@latest"],
        install_hint: "Install via Go: go install golang.org/x/tools/gopls@latest",
    },
    LanguageServerCatalogEntry {
        id: "vscode-html-language-server",
        display_name: "HTML Language Server",
        language_ids: &["html"],
        language_labels: "HTML",
        command: "vscode-html-language-server",
        args: &["--stdio"],
        install_commands: &["npm install -g vscode-langservers-extracted"],
        install_hint: "Install via npm: npm install -g vscode-langservers-extracted",
    },
    LanguageServerCatalogEntry {
        id: "vscode-css-language-server",
        display_name: "CSS Language Server",
        language_ids: &["css", "scss"],
        language_labels: "CSS, SCSS",
        command: "vscode-css-language-server",
        args: &["--stdio"],
        install_commands: &["npm install -g vscode-langservers-extracted"],
        install_hint: "Install via npm: npm install -g vscode-langservers-extracted",
    },
];

/// Look up a catalog entry by id.
pub fn catalog_entry(id: &str) -> Option<&'static LanguageServerCatalogEntry> {
    catalog().iter().find(|e| e.id == id)
}

/// Look up a built-in server spec for an LSP language id.
pub fn server_for_language(language_id: &str) -> Option<LanguageServerSpec> {
    for entry in catalog() {
        if entry.language_ids.contains(&language_id) {
            return Some(LanguageServerSpec {
                id: entry.id.into(),
                command: entry.command.into(),
                args: entry.args.iter().map(|s| (*s).to_string()).collect(),
                language_ids: entry
                    .language_ids
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect(),
            });
        }
    }
    None
}

/// Catalog id for the server that handles `language_id`, if any.
pub fn server_id_for_language(language_id: &str) -> Option<&'static str> {
    catalog()
        .iter()
        .find(|e| e.language_ids.contains(&language_id))
        .map(|e| e.id)
}

/// Whether Coder should attach LSP providers for this language id.
pub fn is_registered(language_id: &str) -> bool {
    server_for_language(language_id).is_some()
}

/// True when `command` resolves on `PATH` (or a well-known user bin).
pub fn is_command_available(command: &str) -> bool {
    which(command).is_some()
}

/// True when neither `rustc` nor `rustup` is available (Rust toolchain missing).
pub fn rust_toolchain_missing() -> bool {
    !is_command_available("rustc") && !is_command_available("rustup")
}

/// Resolve `command` to an absolute path if it ships with the app, is on
/// `PATH`, or lives in a well-known user bin.
///
/// Bundled tools win over `PATH` so a GUI install behaves the same as the
/// offline bundle advertises. GUI apps on macOS often lack `~/.cargo/bin` / npm
/// globals in `PATH`; we also probe those locations so install detection and
/// spawn still work.
pub fn which(command: &str) -> Option<String> {
    for dir in bundled_bin_dirs() {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{command}.exe"));
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }

    #[cfg(windows)]
    let output = Command::new("where").arg(command).output().ok();
    #[cfg(not(windows))]
    let output = Command::new("which").arg(command).output().ok();
    if let Some(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let path = stdout.lines().next().unwrap_or("").trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    which_well_known(command)
}

/// Root of the language-server payload shipped with the app, if present.
///
/// Probed in order:
/// 1. `HALO_LSP_DIR` — explicit override (tests, portable installs).
/// 2. macOS bundle: `Halo.app/Contents/Resources/lsp`.
/// 3. Beside the executable: `<exe dir>/lsp`.
/// 4. Debug builds only: the repository's `vendor/lsp` staging dir.
pub fn bundled_lsp_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("HALO_LSP_DIR") {
        let dir = PathBuf::from(dir);
        // Accept both the payload root (`.../lsp`) and its `bin` directory.
        if dir.join("bin").is_dir() {
            return Some(dir);
        }
        if dir.file_name().is_some_and(|name| name == "bin") && dir.is_dir() {
            return dir.parent().map(Path::to_path_buf);
        }
    }

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))?;
    if let Some(root) = bundled_lsp_dir_for(&exe_dir) {
        return Some(root);
    }

    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/lsp");
        if dev.join("bin").is_dir() {
            return Some(dev);
        }
    }

    None
}

fn bundled_lsp_dir_for(exe_dir: &Path) -> Option<PathBuf> {
    bundled_roots_for_exe(exe_dir)
        .into_iter()
        .find(|root| root.join("bin").is_dir())
}

fn bundled_roots_for_exe(exe_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(contents) = exe_dir.parent() {
        roots.push(contents.join("Resources").join("lsp"));
    }
    roots.push(exe_dir.join("lsp"));
    roots
}

/// Directories prepended to `PATH` for language-server processes.
///
/// Includes the payload's `bin` (wrappers, ShellCheck, rust-analyzer) and the
/// bundled Node runtime so npm shims and servers that shell out find it.
pub fn bundled_bin_dirs() -> Vec<PathBuf> {
    let Some(root) = bundled_lsp_dir() else {
        return Vec::new();
    };
    vec![root.join("bin"), root.join("node").join("bin")]
}

/// The current `PATH` with bundled tool directories prepended.
pub fn path_with_bundled_bins() -> std::ffi::OsString {
    let mut dirs = bundled_bin_dirs();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if dirs.is_empty() {
        return std::env::var_os("PATH").unwrap_or_default();
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

fn which_well_known(command: &str) -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = std::path::PathBuf::from(home);
    #[cfg(windows)]
    let names = [
        format!("{command}.exe"),
        format!("{command}.cmd"),
        command.to_string(),
    ];
    #[cfg(not(windows))]
    let names = [command.to_string()];

    let mut dirs = vec![
        home.join(".cargo").join("bin"),
        home.join(".local").join("bin"),
        home.join(".bun").join("bin"),
        home.join(".volta").join("bin"),
        home.join(".local").join("share").join("pnpm"),
        #[cfg(target_os = "macos")]
        home.join("Library").join("pnpm"),
        home.join(".fnm").join("current").join("bin"),
        #[cfg(target_os = "macos")]
        home.join("Library")
            .join("Application Support")
            .join("fnm")
            .join("aliases")
            .join("default")
            .join("bin"),
        home.join(".asdf").join("shims"),
        home.join(".local").join("share").join("mise").join("shims"),
        #[cfg(target_os = "macos")]
        std::path::PathBuf::from("/opt/homebrew/bin"),
        #[cfg(target_os = "macos")]
        std::path::PathBuf::from("/usr/local/bin"),
    ];

    let nvm_nodes = home.join(".nvm").join("versions").join("node");
    if let Ok(entries) = std::fs::read_dir(nvm_nodes) {
        let mut versions: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        versions.sort();
        if let Some(latest) = versions.last() {
            dirs.push(latest.join("bin"));
        }
    }
    for dir in dirs {
        for name in &names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

/// Run the catalog entry’s install commands until one succeeds.
pub fn install_server(id: &str) -> LspResult<String> {
    let entry =
        catalog_entry(id).ok_or_else(|| LspError::message(format!("unknown server: {id}")))?;
    if entry.install_commands.is_empty() {
        return Err(LspError::message(entry.install_hint.to_string()));
    }

    let mut last_err = String::new();
    for cmd in entry.install_commands {
        match run_shell(cmd) {
            Ok(msg) => {
                if is_command_available(entry.command) {
                    return Ok(format!("Installed {}: {msg}", entry.display_name));
                }
                last_err = format!(
                    "`{cmd}` finished but `{}` is still not on PATH",
                    entry.command
                );
            }
            Err(err) => last_err = err.to_string(),
        }
    }
    Err(LspError::message(format!(
        "Could not install {}. {last_err}. Hint: {}",
        entry.display_name, entry.install_hint
    )))
}

fn run_shell(command: &str) -> LspResult<String> {
    #[cfg(windows)]
    let output = Command::new("cmd")
        .args(["/C", command])
        .output()
        .map_err(|e| LspError::message(format!("failed to run install: {e}")))?;
    #[cfg(not(windows))]
    let output = Command::new("sh")
        .args(["-lc", command])
        .output()
        .map_err(|e| LspError::message(format!("failed to run install: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        let mut msg = stdout;
        if msg.is_empty() {
            msg = if stderr.is_empty() {
                "ok".into()
            } else {
                stderr
            };
        }
        Ok(msg)
    } else {
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("exit {}", output.status)
        };
        Err(LspError::message(format!("`{command}` failed: {detail}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_rust() {
        assert!(server_for_language("rust").is_some());
        assert_eq!(
            server_for_language("rust").unwrap().command,
            "rust-analyzer"
        );
    }

    #[test]
    fn typescript_maps_js_and_ts() {
        let a = server_for_language("typescript").unwrap();
        let b = server_for_language("javascript").unwrap();
        let c = server_for_language("typescriptreact").unwrap();
        let d = server_for_language("javascriptreact").unwrap();
        assert_eq!(a.command, b.command);
        assert_eq!(a.id, c.id);
        assert_eq!(a.id, d.id);
        assert!(a.args.iter().any(|a| a == "--stdio"));

        // The server needs `tsserver.js`, which TypeScript 7 no longer ships.
        let entry = catalog_entry("typescript-language-server").unwrap();
        assert!(
            entry
                .install_commands
                .iter()
                .any(|cmd| cmd.contains("typescript@5")),
            "install must pin TypeScript 5"
        );
    }

    #[test]
    fn bash_maps_to_bash_language_server() {
        let bash = server_for_language("shellscript").unwrap();
        assert_eq!(bash.command, "bash-language-server");
        assert_eq!(bash.args, vec!["start".to_string()]);
        assert!(catalog_entry("bash-language-server").is_some());
    }

    #[test]
    fn python_maps_to_pyright() {
        let py = server_for_language("python").unwrap();
        assert_eq!(py.command, "pyright-langserver");
        assert!(py.args.iter().any(|a| a == "--stdio"));
    }

    #[test]
    fn unknown_language_is_none() {
        assert!(server_for_language("cobol").is_none());
    }

    #[test]
    fn bundle_roots_probe_contents_resources_lsp() {
        let roots = bundled_roots_for_exe(Path::new("/Applications/Halo.app/Contents/MacOS"));
        assert_eq!(
            roots[0],
            PathBuf::from("/Applications/Halo.app/Contents/Resources/lsp")
        );
        assert_eq!(
            roots[1],
            PathBuf::from("/Applications/Halo.app/Contents/MacOS/lsp")
        );
    }

    #[test]
    fn bundled_dir_is_found_below_contents_resources() {
        let root = tempfile::tempdir().expect("tempdir");
        let contents = root.path().join("Halo.app").join("Contents");
        let macos = contents.join("MacOS");
        let lsp = contents.join("Resources").join("lsp");
        std::fs::create_dir_all(lsp.join("bin")).expect("mkdir");

        assert_eq!(bundled_lsp_dir_for(&macos), Some(lsp));
    }

    #[test]
    fn html_and_css_are_registered() {
        let html = server_for_language("html").unwrap();
        assert_eq!(html.command, "vscode-html-language-server");
        assert!(html.args.iter().any(|a| a == "--stdio"));

        let css = server_for_language("css").unwrap();
        let scss = server_for_language("scss").unwrap();
        assert_eq!(css.command, "vscode-css-language-server");
        assert_eq!(css.id, scss.id);
        assert!(css.args.iter().any(|a| a == "--stdio"));
    }
}
