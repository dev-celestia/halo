//! Integration check: a brand-new `.rs` file with invalid Rust must produce
//! `textDocument/publishDiagnostics` from the real `rust-analyzer`.
//!
//! Reproduces the Coder "create new file → no errors shown" report at the
//! transport level, independent of the UI. Skips when `rust-analyzer` is not
//! installed.

use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use halo_lsp::LanguageServerSpec;
use halo_lsp::client::LanguageClient;

fn rust_analyzer_available() -> bool {
    Command::new("rust-analyzer")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn new_file_diagnostics_arrive_from_rust_analyzer() {
    if !rust_analyzer_available() {
        eprintln!("skipping: rust-analyzer not on PATH");
        return;
    }

    let root = tempfile::tempdir().expect("tempdir");
    // No Cargo.toml on purpose: mirrors "created test.rs in a plain folder".
    let file = root.path().join("test.rs");
    std::fs::write(&file, "test /test\n\nfn broken( {\n\nlet = ;\n").expect("write test.rs");

    let spec = LanguageServerSpec {
        id: "rust-analyzer".into(),
        command: "rust-analyzer".into(),
        args: vec![],
        language_ids: vec!["rust".into()],
    };
    let client = LanguageClient::start(spec, root.path()).expect("client start");

    let (tx, rx) = mpsc::channel();
    client.on_diagnostics(std::sync::Arc::new(move |params| {
        let _ = tx.send(params);
    }));

    let (uri, _back) = {
        let uri = halo_lsp::path_to_uri(&file).expect("uri");
        (uri, ())
    };
    client
        .did_open(
            uri.clone(),
            "rust",
            1,
            std::fs::read_to_string(&file).unwrap(),
        )
        .expect("did_open");

    // Link the file as an inline standalone rust-project so rust-analyzer
    // analyzes it even though the folder has no Cargo.toml.
    client
        .set_standalone_project(vec![file.to_path_buf()])
        .expect("didChangeConfiguration");

    // rust-analyzer may take a while on first start (it loads its own
    // toolchain); 120s is generous but bounded.
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(params) => {
                let diags = &params.diagnostics;
                eprintln!(
                    "publishDiagnostics {}: {} entries",
                    params.uri.as_str(),
                    diags.len()
                );
                for d in diags {
                    eprintln!("  [{:?}] {} at {:?}", d.severity, d.message, d.range);
                }
                // The first publish is often an empty clear while analysis
                // warms up; keep waiting for the real diagnostics.
                if params.uri.as_str().contains("test.rs") && !diags.is_empty() {
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("diagnostics channel dropped"),
        }
    }
    panic!("no publishDiagnostics for test.rs within 120s");
}
