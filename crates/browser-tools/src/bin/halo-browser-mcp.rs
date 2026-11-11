//! MCP stdio front. Never writes to stdout outside the protocol — diagnostics go
//! to stderr so a harness reading the stream cannot be corrupted.

use halo_browser_tools::agent_browser::AgentBrowserEngine;
use halo_browser_tools::mcp;
use halo_browser_tools::Browser;

fn main() {
    let engine = match AgentBrowserEngine::from_env() {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("halo-browser-mcp: {error}");
            std::process::exit(2);
        }
    };

    let session = engine.session().to_string();
    let mut browser = Browser::new(engine)
        .with_state(halo_browser_tools::state_path(&session))
        .with_domains(halo_browser_tools::policy::allowed_domains_from_env());
    if let Err(error) = mcp::serve(&mut browser) {
        eprintln!("halo-browser-mcp: {error}");
        std::process::exit(1);
    }
}
