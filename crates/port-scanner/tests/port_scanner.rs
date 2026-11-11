//! Integration: a connect scan against a local listener finds the open port,
//! reports it via events, and completes.

use std::sync::mpsc;

use halo_core::tauri::AppHandle;
use port_scanner::{PortScanRequest, run_scan};

fn request(scan_id: &str, port: u16, stealth: bool) -> PortScanRequest {
    PortScanRequest {
        scan_id: scan_id.into(),
        target: "127.0.0.1".into(),
        ports: vec![port, port + 1, port + 2],
        timeout_ms: 500,
        concurrency: 8,
        banner_grab: false,
        scan_type: "connect".into(),
        stealth_mode: stealth,
        delay_ms: stealth.then_some(10),
        jitter_ms: stealth.then_some(5),
        randomize_ports: Some(false),
    }
}

#[test]
fn scan_finds_open_port_and_completes() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // One listener: exactly one open port among the three probed.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let open_port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            drop(stream);
        }
    });

    let app = AppHandle::new();
    let (tx, rx) = mpsc::channel();
    app.on_event(move |name, payload| {
        if name.starts_with("port-scan-result-") {
            let port = payload.get("port").and_then(|v| v.as_u64());
            let _ = tx.send(("result", port.unwrap_or(0) as u16));
        } else if name.starts_with("port-scan-progress-") {
            let _kind = payload
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let _ = tx.send(("progress", 0u16));
        }
    });

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let results =
        rt.block_on(async { run_scan(app, request("test-scan", open_port, false), cancel).await });

    assert_eq!(results.len(), 1, "exactly one open port: {results:?}");
    assert_eq!(results[0].port, open_port);
    assert_eq!(results[0].state, "open");
    assert!(results[0].response_time_ms.is_some());

    // Events: one result event + a Complete progress event.
    let mut saw_result = false;
    let mut saw_complete = false;
    while let Ok(event) = rx.try_recv() {
        match event.0 {
            "result" if event.1 == open_port => saw_result = true,
            "progress" => saw_complete = true,
            _ => {}
        }
    }
    assert!(saw_result, "result event was emitted");
    assert!(saw_complete, "complete event was emitted");
}

#[test]
fn cancellation_stops_the_scan() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let app = AppHandle::new();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_for_task = cancel.clone();

    // Full-range style scan (65k ports) cancelled almost immediately.
    let handle = rt.spawn(async move {
        let mut req = request("cancel-scan", 1, false);
        req.ports = (1..=65535u16).collect();
        run_scan(app, req, cancel_for_task).await
    });
    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    let results = rt.block_on(handle).unwrap();

    assert!(
        results.is_empty(),
        "cancelled scan returns no final results: {}",
        results.len()
    );
}
