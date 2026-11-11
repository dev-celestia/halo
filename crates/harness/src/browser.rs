//! The browser block handed to a harness.
//!
//! Halo routes browser work through one CLI (`halo-browser`), not through MCP:
//! every harness already has a shell tool, so a single binary covers Pi,
//! OpenCode and Antigravity with the same generation guard and the same budget.
//!
//! The only per-harness difference is *where* the instructions go. Pi takes
//! `--append-system-prompt`; OpenCode and Antigravity have no such hook, so the
//! caller prepends this block to the prompt instead. Both callers go through
//! [`instructions`], so the text cannot drift between them.

use std::path::PathBuf;

/// The Halo browser CLI, when it is installed *and* the engine behind it can
/// actually run.
///
/// Advertising a browser the agent cannot actually run is worse than saying
/// nothing: the agent would spend turns on a tool that answers "not found".
/// `halo-browser` is the gate the bundle controls; the `agent-browser` binary
/// the user installs is the second gate.
pub fn cli() -> Option<PathBuf> {
    if !reachable() {
        return None;
    }
    halo_browser_tools::agent_browser::resolve_cli()
}

fn reachable() -> bool {
    halo_browser_tools::agent_browser::ready()
}

/// The instructions, with the CLI's real path substituted.
///
/// The path is baked in because the bundled binary is not on `PATH` and a model
/// copies the literal command from the text.
pub fn instructions() -> Option<String> {
    let cli = cli()?;
    Some(
        include_str!("browser_prompt.md")
            .replace("__HALO_BROWSER_CLI__", &cli.display().to_string()),
    )
}

/// The block as a prompt suffix: `\n\n…`, or empty when there is no browser.
pub fn prompt_suffix() -> String {
    match instructions() {
        Some(text) => format!("\n\n{text}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_never_leak_the_placeholder() {
        // Either the CLI is missing (no block) or the path replaced the token.
        if let Some(text) = instructions() {
            assert!(!text.contains("__HALO_BROWSER_CLI__"), "{text}");
        }
    }

    #[test]
    fn empty_suffix_when_no_browser() {
        // No assertion on the environment: both shapes must be well-formed.
        let suffix = prompt_suffix();
        assert!(suffix.is_empty() || suffix.starts_with("\n\n"));
    }
}
