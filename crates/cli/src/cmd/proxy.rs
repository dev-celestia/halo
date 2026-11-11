//! `halo-cli proxy …`
//!
//! The proxy lives in a thread with process-global state, so a started proxy
//! keeps its process alive. `--detach` re-execs this binary as `serve` and
//! records where it landed in `cli/proxy.json`, which is what `status` and
//! `stop` read.

use std::io::Write as _;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use halo_backend::commands::proxy as proxy_cmd;
use serde::{Deserialize, Serialize};

use crate::context::{Ctx, run_dir};
use crate::output::emit_json;

#[derive(Debug, Serialize, Deserialize)]
struct ProxyState {
    pid: u32,
    port: u16,
    tls_port: u16,
}

const READY_TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(100);

pub async fn start(ctx: &Ctx, port: u16, tls_port: u16, detach: bool) -> anyhow::Result<()> {
    if let Some(state) = read_state()? {
        if pid_alive(state.pid) {
            bail!(
                "proxy already running (pid {}, http {}, https {}) — run `halo-cli proxy stop` first",
                state.pid,
                state.port,
                state.tls_port
            );
        }
        clear_state()?;
    }

    if detach {
        return start_detached(port, tls_port).await;
    }

    let message = proxy_cmd::start_proxy(ctx.ctx(), port, tls_port)
        .await
        .map_err(anyhow::Error::msg)?;
    let actual = proxy_cmd::get_proxy_status(ctx.ctx())
        .await
        .map_err(anyhow::Error::msg)?;
    write_state(&ProxyState {
        pid: std::process::id(),
        port: actual.port.unwrap_or(port),
        tls_port,
    })?;

    if ctx.json {
        emit_json(&serde_json::json!({
            "running": true,
            "port": actual.port,
            "tls_port": tls_port,
            "message": message,
        }))?;
    } else {
        println!("{message}");
        println!("press Ctrl-C to stop");
    }

    wait_for_shutdown().await;
    let _ = proxy_cmd::stop_proxy(ctx.ctx()).await;
    clear_state()?;
    if !ctx.json {
        println!("proxy stopped");
    }
    Ok(())
}

/// The detached child: boot, listen, then hold until SIGTERM or Ctrl-C.
pub async fn serve(port: u16, tls_port: u16) -> anyhow::Result<()> {
    let ctx = Ctx::boot(false)?;
    let message = proxy_cmd::start_proxy(ctx.ctx(), port, tls_port)
        .await
        .map_err(anyhow::Error::msg)?;
    let actual = proxy_cmd::get_proxy_status(ctx.ctx())
        .await
        .map_err(anyhow::Error::msg)?;
    write_state(&ProxyState {
        pid: std::process::id(),
        port: actual.port.unwrap_or(port),
        tls_port,
    })?;
    let _ = message;

    wait_for_shutdown().await;
    let _ = proxy_cmd::stop_proxy(ctx.ctx()).await;
    clear_state()?;
    Ok(())
}

pub async fn stop(ctx: &Ctx) -> anyhow::Result<()> {
    let Some(state) = read_state()? else {
        if ctx.json {
            emit_json(&serde_json::json!({ "running": false, "stopped": false }))?;
        } else {
            println!("no detached proxy recorded");
        }
        return Ok(());
    };

    if !pid_alive(state.pid) {
        clear_state()?;
        if ctx.json {
            emit_json(&serde_json::json!({ "running": false, "stopped": false }))?;
        } else {
            println!("stale pidfile removed (pid {})", state.pid);
        }
        return Ok(());
    }

    // SIGTERM: the child's handler stops the proxy and unlinks the state file.
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(state.pid.to_string())
        .status()
        .context("send SIGTERM")?;
    if !status.success() {
        bail!("could not signal pid {}", state.pid);
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if !pid_alive(state.pid) {
            clear_state()?;
            if ctx.json {
                emit_json(&serde_json::json!({ "running": false, "stopped": true }))?;
            } else {
                println!("proxy stopped (pid {})", state.pid);
            }
            return Ok(());
        }
        tokio::time::sleep(POLL).await;
    }

    bail!("pid {} did not exit within 10s", state.pid)
}

pub async fn status(ctx: &Ctx) -> anyhow::Result<()> {
    let recorded = read_state()?;
    let live = recorded.as_ref().filter(|s| pid_alive(s.pid));

    let (running, port, tls_port) = match live {
        Some(state) => (probe(state.port), Some(state.port), Some(state.tls_port)),
        None => {
            // Nothing recorded: the desktop app may be the one holding the port.
            let default = halo_backend::default_proxy_port();
            (probe(default), Some(default).filter(|_| probe(default)), None)
        }
    };

    if ctx.json {
        emit_json(&serde_json::json!({
            "running": running,
            "port": if running { port } else { None },
            "tls_port": tls_port,
            "pid": live.map(|s| s.pid),
            "default_port": halo_backend::default_proxy_port(),
        }))?;
        return Ok(());
    }

    if running {
        println!(
            "running   http 127.0.0.1:{}   https 127.0.0.1:{}",
            port.unwrap_or_default(),
            tls_port
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
    } else {
        println!(
            "stopped   default port {}",
            halo_backend::default_proxy_port()
        );
    }
    Ok(())
}

// ── detached child ─────────────────────────────────────────────────

async fn start_detached(port: u16, tls_port: u16) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("resolve current exe")?;
    let mut command = Command::new(exe);
    command
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--tls-port")
        .arg(tls_port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // A daemon that shares the spawning shell's process group dies with it.
    // `setsid` gives the child its own session, so `proxy start -d` outlives
    // the terminal it was launched from.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid` is async-signal-safe and touches no allocator state.
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    command.spawn().context("spawn detached proxy")?;

    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(state) = read_state()? {
            // A leftover file from a dead run can name a port some other
            // process now owns; only trust it while its pid is alive.
            if pid_alive(state.pid) && probe(state.port) {
                println!(
                    "proxy started (pid {}) — http 127.0.0.1:{}  https 127.0.0.1:{}",
                    state.pid, state.port, state.tls_port
                );
                return Ok(());
            }
        }
        tokio::time::sleep(POLL).await;
    }

    bail!(
        "detached proxy did not become ready within {}s — check `halo-cli proxy status`",
        READY_TIMEOUT.as_secs()
    )
}

async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = term.recv() => {}
                _ = tokio::signal::ctrl_c() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

// ── state file ─────────────────────────────────────────────────────

fn state_file() -> anyhow::Result<PathBuf> {
    Ok(run_dir()?.join("proxy.json"))
}

fn read_state() -> anyhow::Result<Option<ProxyState>> {
    let path = state_file()?;
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    match serde_json::from_str(&raw) {
        Ok(state) => Ok(Some(state)),
        Err(_) => {
            // A half-written file from a killed process is not worth keeping.
            let _ = std::fs::remove_file(path);
            Ok(None)
        }
    }
}

fn write_state(state: &ProxyState) -> anyhow::Result<()> {
    let path = state_file()?;
    let mut file = std::fs::File::create(&path)
        .with_context(|| format!("write {}", path.display()))?;
    file.write_all(serde_json::to_string(state)?.as_bytes())?;
    file.flush()?;
    Ok(())
}

fn clear_state() -> anyhow::Result<()> {
    let path = state_file()?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn probe(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}
