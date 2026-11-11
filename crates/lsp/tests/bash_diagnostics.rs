//! Integration check: a shell script with a ShellCheck violation must produce
//! `textDocument/publishDiagnostics` through the bundled bash-language-server
//! (or a PATH install). Skips when the server or ShellCheck is unavailable.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use halo_lsp::client::LanguageClient;

#[test]
fn shellcheck_diagnostics_arrive_for_bash() {
    let Some(spec) = halo_lsp::server_for_language("shellscript") else {
        panic!("shellscript should map to a language server");
    };
    if halo_lsp::which(&spec.command).is_none() {
        eprintln!("skipping: bash-language-server not available");
        return;
    }
    if halo_lsp::which("shellcheck").is_none() {
        eprintln!("skipping: shellcheck not available");
        return;
    }

    let root = tempfile::tempdir().expect("tempdir");
    let root_path = root.path().canonicalize().expect("canonical root");
    let file = root_path.join("lint.sh");
    let text = "#!/bin/sh\nname=\"world\"\necho $name\necho $undefined_var\n";
    std::fs::write(&file, text).expect("write lint.sh");

    let client = LanguageClient::start(spec, &root_path).expect("client start");
    let (tx, rx) = mpsc::channel();
    client.on_diagnostics(Arc::new(move |params| {
        let _ = tx.send(params);
    }));

    let uri = halo_lsp::path_to_uri(&file).expect("uri");
    client
        .did_open(uri, "shellscript", 1, text.to_string())
        .expect("did_open");

    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(params) => {
                for diag in &params.diagnostics {
                    eprintln!(
                        "{} {:?} {:?}",
                        diag.source.as_deref().unwrap_or("-"),
                        diag.code,
                        diag.message
                    );
                }
                if params
                    .diagnostics
                    .iter()
                    .any(|d| d.source.as_deref() == Some("shellcheck"))
                {
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("diagnostics channel dropped"),
        }
    }
    panic!("no shellcheck diagnostics for lint.sh within 60s");
}
