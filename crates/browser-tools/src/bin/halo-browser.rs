//! Command-line front over the same core.
//!
//! This is the cheap route: any harness with a shell tool can drive the browser
//! without MCP wiring, and it shares the guard and the budget with the MCP front
//! because both sit on [`Browser`].
//!
//! The engine is a separate browser the CLI starts itself (`agent-browser`), so
//! nothing depends on the Halo app being open.

use std::process::ExitCode;

use halo_browser_tools::agent_browser::{resolve_bin, resolve_cli, AgentBrowserEngine};
use halo_browser_tools::types::{ActRequest, Action, SnapshotRequest};
use halo_browser_tools::{Browser, Engine};

const USAGE: &str = "\
halo-browser — browser tools for Halo agents

  halo-browser open <url>
  halo-browser snapshot [--scope <css>] [--max <chars>] [--all]
  halo-browser act --ref <@eN> --gen <n> --action <click|fill|press> [--value <text>]
  halo-browser read [--selector <css>] [--max <chars>]
  halo-browser watch [--seconds <n>]
  halo-browser close
  halo-browser status
  halo-browser doctor

Environment:
  HALO_BROWSER_BIN              path to the agent-browser binary
  HALO_BROWSER_SESSION          session name
  HALO_BROWSER_CDP              attach to an existing CDP endpoint
  HALO_BROWSER_ALLOWED_DOMAINS  comma-separated host allowlist
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        print!("{USAGE}");
        return ExitCode::FAILURE;
    };

    match run(command, &args[1..]) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: &str, rest: &[String]) -> Result<String, String> {
    match command {
        "status" => return status(),
        "doctor" => return doctor(),
        // Not for the agent: this is the pane's data path, exercised from a
        // shell so the stream can be verified without any UI.
        "watch" => return watch(rest),
        "--help" | "-h" | "help" => return Ok(USAGE.to_string()),
        _ => {}
    }

    let engine = AgentBrowserEngine::from_env()?;
    let session = engine.session().to_string();
    let mut browser = Browser::new(engine)
        .with_state(halo_browser_tools::state_path(&session))
        .with_domains(halo_browser_tools::policy::allowed_domains_from_env());
    dispatch(&mut browser, command, rest)
}

fn dispatch<E: Engine>(
    browser: &mut Browser<E>,
    command: &str,
    rest: &[String],
) -> Result<String, String> {
    match command {
        "open" => {
            let url = rest.first().ok_or("open needs a url")?;
            let outcome = browser.open(url)?;
            Ok(format!(
                "opened {}\ntitle: {}\ngeneration: {}\nengine: agent-browser",
                outcome.url, outcome.title, outcome.generation
            ))
        }
        "snapshot" => {
            let request = SnapshotRequest {
                scope: flag(rest, "--scope"),
                interactive: Some(!rest.iter().any(|arg| arg == "--all")),
                max_chars: flag(rest, "--max").and_then(|value| value.parse().ok()),
            };
            let snapshot = browser.snapshot(&request)?;
            let mut out = format!("generation: {}\n{}", snapshot.generation, snapshot.text);
            if let Some(truncation) = snapshot.truncated {
                out.push_str(&format!("\n[{}]", truncation.hint));
            }
            Ok(out)
        }
        "act" => {
            let reference = flag(rest, "--ref").unwrap_or_default();
            let generation = flag(rest, "--gen")
                .ok_or("act needs --gen (the generation from your last snapshot)")?
                .parse()
                .map_err(|_| "--gen must be a number")?;
            let action = match flag(rest, "--action").as_deref() {
                Some("click") => Action::Click,
                Some("fill") => Action::Fill,
                Some("press") => Action::Press,
                other => return Err(format!("unknown action {other:?}; use click, fill or press")),
            };
            let outcome = browser.act(&ActRequest {
                reference,
                generation,
                action,
                value: flag(rest, "--value"),
            })?;
            Ok(format!(
                "{} -> {}\nurl: {}\ntitle: {}\ngeneration: {}\nevidence: {}\nnote: {}",
                action.as_str(),
                if outcome.changed {
                    "page changed"
                } else {
                    "page unchanged"
                },
                outcome.url,
                outcome.title,
                outcome.generation,
                outcome.evidence,
                outcome.note
            ))
        }
        "read" => browser.read(
            flag(rest, "--selector").as_deref(),
            flag(rest, "--max").and_then(|value| value.parse().ok()),
        ),
        "close" => {
            browser.close()?;
            Ok("browser closed".into())
        }
        other => Err(format!("unknown command {other}; run halo-browser --help")),
    }
}

fn status() -> Result<String, String> {
    let bin = resolve_bin(None)?;
    let session = std::env::var("HALO_BROWSER_SESSION").unwrap_or_else(|_| "halo".into());
    let cdp = std::env::var("HALO_BROWSER_CDP").unwrap_or_else(|_| "none".into());
    // The front's own path is part of the report: when the app says "Not
    // installed" and this command works, the difference is where each one
    // looked, and this line is the answer.
    let cli = resolve_cli()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "not found".into());
    Ok(format!(
        "cli: {cli}\nbinary: {}\nsession: {}\ncdp: {}\nengine: agent-browser",
        bin.display(),
        session,
        cdp
    ))
}

/// Everything that can be wrong, in one command.
fn doctor() -> Result<String, String> {
    let mut out = String::new();
    match resolve_bin(None) {
        Ok(bin) => out.push_str(&format!("agent-browser: {}\n", bin.display())),
        Err(error) => out.push_str(&format!("agent-browser: {error}\n")),
    }
    Ok(out)
}

fn watch(rest: &[String]) -> Result<String, String> {
    let seconds: u64 = flag(rest, "--seconds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
    let mut engine = AgentBrowserEngine::from_env()?;
    let url = engine.stream_url()?;
    let stream = halo_browser_tools::stream::Stream::connect(&url)?;
    let status = stream.status().clone();
    let mut out = format!(
        "stream {url}\nviewport: {}x{}\nscreencasting: {}",
        status.width, status.height, status.screencasting
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let mut count = 0;
    let mut bytes = 0usize;
    while std::time::Instant::now() < deadline {
        match stream.next_within(std::time::Duration::from_millis(500)) {
            Ok(Some(frame)) => {
                count += 1;
                bytes += frame.jpeg.len();
                if count == 1 {
                    out.push_str(&format!(
                        "\nfirst frame: {} bytes, {}x{}",
                        frame.jpeg.len(),
                        frame.width,
                        frame.height
                    ));
                }
            }
            Ok(None) => {}
            Err(error) => {
                out.push_str(&format!("\nstopped: {error}"));
                break;
            }
        }
    }
    out.push_str(&format!(
        "\nframes in {seconds}s: {count} ({:.1} fps, {} KiB)",
        count as f64 / seconds.max(1) as f64,
        bytes / 1024
    ));
    Ok(out)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let position = args.iter().position(|arg| arg == name)?;
    args.get(position + 1).cloned()
}
