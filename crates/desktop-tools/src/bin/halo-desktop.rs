//! Command-line front over the same core.
//!
//! This is the cheap route: any harness with a shell tool can drive the desktop
//! without MCP wiring, and it shares the guard, the budget and the policy with
//! any future front because they all sit on [`halo_desktop_tools::Desktop`].
//!
//! The engine is a separate process the CLI starts itself (`agent-desktop`), so
//! nothing here depends on the Halo app being open.

use std::process::ExitCode;

use halo_desktop_tools::agent_desktop::exec::open_permission_guide;
use halo_desktop_tools::agent_desktop::probe::{resolve_bin, resolve_cli, version};
use halo_desktop_tools::agent_desktop::AgentDesktopEngine;
use halo_desktop_tools::desktop::{cache_dir, state_path};
use halo_desktop_tools::lease;
use halo_desktop_tools::permissions;
use halo_desktop_tools::policy::Policy;
use halo_desktop_tools::types::{ActAction, ActRequest, FindRequest, SnapshotRequest};
use halo_desktop_tools::{Desktop, Engine};

const USAGE: &str = "\
halo-desktop — computer use for Halo agents

  halo-desktop status
  halo-desktop apps [--app <name>]
  halo-desktop windows [--app <name>]
  halo-desktop snapshot [--app <name>] [--window-id <id>] [--surface <kind>]
                        [--root <ref>] [--skeleton] [--all] [--compact]
                        [--depth <n>] [--timeout <ms>] [--max <chars>]
  halo-desktop find [--app <name>] [--role <role>] [--name <text>] [--text <text>]
                    [--root <ref>] [--exact] [--first]
  halo-desktop read --ref <ref> [--property text|value|title|bounds|role|states]
  halo-desktop act --ref <ref> --gen <n> --action click|type|press|select|scroll|focus
                   [--value <text>]
  halo-desktop permissions [--open]
  halo-desktop lease <status|acquire|release>
  halo-desktop doctor

The loop: status → snapshot --skeleton → find / snapshot --root → act --gen.
Do not pass --headed. Physical input is not available.

Environment:
  HALO_DESKTOP_BIN              path to the agent-desktop binary
  HALO_DESKTOP_SESSION          session namespace (refs only resolve in scope)
  HALO_DESKTOP_HOME             state root (absolute; keeps Halo's sessions apart)
  HALO_DESKTOP_CHAT             chat id that owns the lease
  HALO_DESKTOP_RUN_PID          agent process that holds the lease for the turn
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
        "--help" | "-h" | "help" => return Ok(USAGE.to_string()),
        "status" => return status(),
        "doctor" => return doctor(),
        "permissions" => return permissions_command(rest),
        "lease" => return lease_command(rest),
        _ => {}
    }

    let session = session_id();
    let mut desktop = Desktop::new(AgentDesktopEngine::from_env()?)
        .with_state(state_path(&session))
        .with_policy(Policy::default().with_headed(headed_allowed()));
    dispatch(&mut desktop, command, rest)
}

fn dispatch<E: Engine>(desktop: &mut Desktop<E>, command: &str, rest: &[String]) -> Result<String, String> {
    if has(rest, "--headed") {
        return Err(
            "Physical input is off. Do not pass --headed. Use a headless action: click, type, \
             press, select, scroll, or focus."
                .to_string(),
        );
    }
    match command {
        "apps" => desktop.apps(flag(rest, "--app").as_deref()),
        "windows" => {
            hold_lease()?;
            desktop.windows(flag(rest, "--app").as_deref())
        }
        "snapshot" => {
            hold_lease()?;
            if has(rest, "--skeleton") && has(rest, "--all") {
                return Err(
                    "Do not combine --skeleton with --all. Take a skeleton snapshot, then drill \
                     with --root."
                        .to_string(),
                );
            }
            let request = SnapshotRequest {
                app: flag(rest, "--app"),
                window_id: flag(rest, "--window-id"),
                surface: flag(rest, "--surface"),
                root: flag(rest, "--root"),
                skeleton: has(rest, "--skeleton"),
                // --skeleton is the first look. Pairing it with -i hides the
                // structure the skeleton is for.
                interactive: !has(rest, "--skeleton") && !has(rest, "--all"),
                compact: has(rest, "--compact"),
                max_depth: number(rest, "--depth"),
                timeout_ms: number(rest, "--timeout"),
                max_chars: number::<usize>(rest, "--max"),
            };
            let snapshot = desktop.snapshot(&request)?;
            if let Some(app) = request.app.as_deref() {
                lease::note_app(&lease::path_for(&cache_dir()), &chat_id(), app);
            }
            Ok(format!("generation: {}\n{}", snapshot.generation, snapshot.text))
        }
        "find" => {
            hold_lease()?;
            let request = FindRequest {
                app: flag(rest, "--app"),
                role: flag(rest, "--role"),
                name: flag(rest, "--name"),
                text: flag(rest, "--text"),
                root: flag(rest, "--root"),
                surface: flag(rest, "--surface"),
                exact: has(rest, "--exact"),
                first: has(rest, "--first"),
                limit: number(rest, "--limit"),
            };
            desktop.find(&request)
        }
        "read" => {
            hold_lease()?;
            let reference = flag(rest, "--ref")
                .ok_or("read needs --ref (a qualified ref from your last snapshot)")?;
            desktop.read(&reference, flag(rest, "--property").as_deref())
        }
        "act" => {
            hold_lease()?;
            let action = flag(rest, "--action")
                .ok_or("act needs --action click|type|press|select|scroll|focus")?;
            let action = ActAction::parse(&action).ok_or_else(|| {
                format!("unknown action {action}; use click, type, press, select, scroll, or focus")
            })?;
            let generation = number(rest, "--gen")
                .ok_or("act needs --gen (the generation printed by the last snapshot)")?;
            let outcome = desktop.act(&ActRequest {
                reference: flag(rest, "--ref"),
                generation,
                action,
                value: flag(rest, "--value"),
            })?;
            let mut text = format!(
                "generation: {}\nchanged: {}\nevidence: {}",
                outcome.generation, outcome.changed, outcome.evidence
            );
            if let Some(delivery) = outcome.delivery {
                text.push_str(&format!("\ndelivery: {delivery}"));
            }
            if let Some(retry) = outcome.retry {
                text.push_str(&format!("\nretry: {retry}"));
            }
            Ok(text)
        }
        other => Err(format!("unknown command {other}; run halo-desktop --help")),
    }
}

fn status() -> Result<String, String> {
    let bin = resolve_bin(None)?;
    let version = version(&bin).unwrap_or_else(|| "unknown".into());
    let cli = resolve_cli()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "not found".into());

    let session = session_id();
    let mut desktop = Desktop::new(AgentDesktopEngine::from_env()?);
    let status = desktop.status()?;

    let mut out = format!(
        "engine: {version} at {}\ncli: {cli}\nplatform: {}\nstate root: {}\n",
        bin.display(),
        status.platform,
        status.state_root
    );
    out.push_str(&permissions::summary(&status));
    out.push('\n');
    out.push_str(&format!(
        "surfaces: {}\nsession: {session}\n",
        if status.supported_surfaces.is_empty() {
            "none reported".to_string()
        } else {
            status.supported_surfaces.join(", ")
        }
    ));
    out.push_str(&lease_line());

    // Only when something is missing: a user who has already granted it does
    // not need the recipe again.
    if let Some(guidance) = permissions::guidance(&status) {
        out.push('\n');
        out.push_str(&guidance);
    }
    Ok(out)
}

/// The permission state, plus the exact steps when one is missing.
///
/// `--open` is the only path that touches System Settings, and it exists
/// because the user asked for it on this invocation — never automatically.
fn permissions_command(rest: &[String]) -> Result<String, String> {
    let mut out = String::new();
    if has(rest, "--open") {
        let bin = resolve_bin(None)?;
        out.push_str(&open_permission_guide(&bin)?);
        out.push_str("\n\n");
    }

    let mut desktop = Desktop::new(AgentDesktopEngine::from_env()?);
    let status = desktop.status()?;
    out.push_str(&permissions::summary(&status));
    out.push('\n');

    // Blocking first, then optional. `permissions` always offers both, because
    // it is the command someone runs when they cannot find the switch.
    let mut offered = false;
    for block in [
        permissions::guidance(&status),
        permissions::screen_recording_guidance(&status),
    ]
    .into_iter()
    .flatten()
    {
        out.push('\n');
        out.push_str(&block);
        offered = true;
    }
    if !offered {
        out.push_str("\nEvery permission is granted; nothing to approve.\n");
    }
    Ok(out)
}

fn doctor() -> Result<String, String> {
    let mut out = String::new();
    match resolve_bin(None) {
        Ok(bin) => {
            out.push_str(&format!("agent-desktop: {}\n", bin.display()));
            out.push_str(&format!(
                "version: {}\n",
                version(&bin).unwrap_or_else(|| "unknown".into())
            ));
        }
        Err(error) => out.push_str(&format!("agent-desktop: {error}\n")),
    }
    out.push_str(&format!(
        "cli: {}\n",
        resolve_cli()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not found".into())
    ));

    // Best effort: a missing engine is the reason someone ran this.
    match AgentDesktopEngine::from_env().and_then(|engine| {
        let mut desktop = Desktop::new(engine);
        desktop.status()
    }) {
        Ok(status) => {
            out.push_str(&permissions::summary(&status));
            out.push('\n');
            if permissions::guidance(&status).is_some() {
                out.push_str("permissions: run `halo-desktop permissions` for the steps\n");
            }
        }
        Err(error) => out.push_str(&format!("permissions: unknown ({error})\n")),
    }
    out.push_str(&lease_line());
    Ok(out)
}

fn lease_command(rest: &[String]) -> Result<String, String> {
    let path = lease::path_for(&cache_dir());
    match rest.first().map(String::as_str) {
        Some("status") | None => Ok(lease_line()),
        Some("acquire") => {
            let lease = lease::acquire(&path, &chat_id(), &session_id(), run_pid())?;
            Ok(format!(
                "lease: chat {} owns the desktop (session {})",
                lease.chat_id, lease.session_id
            ))
        }
        Some("release") => {
            let released = lease::release(&path, &chat_id())?;
            Ok(if released {
                "lease: released".to_string()
            } else {
                "lease: not held by this chat".to_string()
            })
        }
        Some(other) => Err(format!("unknown lease command {other}; use status, acquire or release")),
    }
}

fn lease_line() -> String {
    match lease::current(&lease::path_for(&cache_dir())) {
        Some(holder) => format!(
            "lease: chat {} (session {}, pid {}, since {})",
            holder.chat_id, holder.session_id, holder.pid, holder.since
        ),
        None => "lease: none".to_string(),
    }
}

fn hold_lease() -> Result<(), String> {
    lease::acquire(
        &lease::path_for(&cache_dir()),
        &chat_id(),
        &session_id(),
        run_pid(),
    )?;
    Ok(())
}

/// The long-lived agent process. Falling back to this command's pid only
/// covers a one-shot CLI call made outside a turn.
fn run_pid() -> u32 {
    std::env::var("HALO_DESKTOP_RUN_PID")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|pid| *pid > 0)
        .unwrap_or_else(std::process::id)
}

fn session_id() -> String {
    std::env::var("HALO_DESKTOP_SESSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "halo".into())
}

fn chat_id() -> String {
    std::env::var("HALO_DESKTOP_CHAT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "cli".into())
}

fn headed_allowed() -> bool {
    std::env::var("HALO_DESKTOP_ALLOW_HEADED")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let position = args.iter().position(|arg| arg == name)?;
    args.get(position + 1).cloned()
}

fn number<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    flag(args, name).and_then(|value| value.parse().ok())
}
