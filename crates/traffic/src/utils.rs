use std::process::Command;
use std::thread;
use std::time::Duration;

/// PIDs listening on `port`.
pub fn listening_pids(port: u16) -> Result<Vec<String>, String> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-tiTCP:{}", port), "-sTCP:LISTEN"])
        .output()
        .map_err(|e| format!("Failed to run lsof: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|pid| !pid.is_empty())
        .map(str::to_string)
        .collect())
}

/// `(uid, command name)` for `pid`, or `None` when the process is gone or
/// `ps` is unavailable.
fn process_owner(pid: &str) -> Option<(String, String)> {
    let output = Command::new("ps")
        .args(["-o", "uid=,comm=", "-p", pid])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    let mut parts = line.splitn(2, char::is_whitespace);
    let uid = parts.next()?.trim().to_string();
    let name = parts.next().unwrap_or("").trim().to_string();
    if uid.is_empty() {
        return None;
    }
    Some((uid, name))
}

fn current_uid() -> Option<String> {
    process_owner(&std::process::id().to_string()).map(|(uid, _)| uid)
}

fn describe(pids: &[String]) -> String {
    pids.iter()
        .map(|pid| match process_owner(pid) {
            Some((uid, name)) => format!("PID {pid} ({name}, uid {uid})"),
            None => format!("PID {pid}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ensure `port` is free, without ever signalling a process the user did not
/// ask us to touch.
///
/// The previous implementation ran `kill` and then `kill -9` on every PID
/// holding the port. Because the port is user-editable in Settings, pointing
/// it at 5432 / 3306 / 22 destroyed unrelated local services with no
/// confirmation and no audit trail. Now:
///
/// * `allow_kill == false` (the default, and what every caller passes): report
///   the owning PIDs and let the user decide.
/// * `allow_kill == true`: only `SIGTERM` processes owned by the **current
///   uid**, never escalate to `SIGKILL`, and never touch another user's
///   process.
pub fn ensure_port_free(port: u16, allow_kill: bool) -> Result<(), String> {
    let pids = listening_pids(port)?;
    if pids.is_empty() {
        return Ok(());
    }

    let current_pid = std::process::id().to_string();
    let foreign: Vec<String> = pids.into_iter().filter(|pid| pid != &current_pid).collect();
    if foreign.is_empty() {
        return Ok(());
    }

    if !allow_kill {
        return Err(format!(
            "Port {port} is already in use by {}.\nChoose a different port, or stop that process yourself.",
            describe(&foreign)
        ));
    }

    let my_uid = current_uid();
    let mut signalled: Vec<String> = Vec::new();

    for pid in &foreign {
        match process_owner(pid) {
            Some((uid, name)) if Some(&uid) == my_uid.as_ref() => {
                println!("Sending SIGTERM to {name} (PID {pid}) on port {port}");
                let output = Command::new("kill")
                    .arg(pid)
                    .output()
                    .map_err(|e| format!("Failed to signal PID {pid}: {e}"))?;
                if !output.status.success() {
                    return Err(format!(
                        "Failed to signal PID {pid} on port {port}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                signalled.push(format!("{pid} ({name})"));
            }
            Some((uid, name)) => {
                return Err(format!(
                    "Refusing to signal PID {pid} ({name}): it belongs to uid {uid}, \
                     not the current user. Choose a different port."
                ));
            }
            None => {
                return Err(format!(
                    "PID {pid} on port {port} could not be inspected. Choose a different port."
                ));
            }
        }
    }

    // Give the processes a moment to release the socket, then report — do not
    // escalate to SIGKILL.
    for _ in 0..20 {
        let remaining = listening_pids(port)?
            .into_iter()
            .filter(|pid| pid != &current_pid)
            .collect::<Vec<_>>();
        if remaining.is_empty() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    let remaining = listening_pids(port)?
        .into_iter()
        .filter(|pid| pid != &current_pid)
        .collect::<Vec<_>>();

    if remaining.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Port {port} is still held by {} after SIGTERM (sent to: {}). \
             Not escalating to SIGKILL — choose a different port.",
            describe(&remaining),
            signalled.join(", ")
        ))
    }
}

// ponytail: delegate body re-compression to hexbuffer_proxy::decoder
pub fn encode_body(encoding: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    let full = hexbuffer_proxy::Body::Full(bytes::Bytes::copy_from_slice(body));
    match hexbuffer_proxy::decoder::encode_body(full, encoding, None) {
        Ok(hexbuffer_proxy::Body::Full(bytes)) => Ok(bytes.to_vec()),
        Ok(_) => Err("expected Body::Full from encoder".to_string()),
        Err(e) => Err(format!("{e}")),
    }
}

pub fn is_captive_portal(uri: &str) -> bool {
    let lower = uri.to_lowercase();
    lower.contains("generate_204")
        || lower.contains("canonical.html")
        || lower.contains("connecttest.txt")
        || lower.contains("ncsi.txt")
        || lower.contains("captive.apple.com")
        || lower.contains("connectivitycheck.gstatic.com")
        || lower.contains("msftconnecttest.com")
}
