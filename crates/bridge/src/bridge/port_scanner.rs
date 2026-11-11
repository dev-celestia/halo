//! Port Scanner page bridge: the ports mini-language, scan lifecycle, and
//! result streaming from `port-scan-*` events.

use super::*;

#[allow(dead_code)]
pub const PORT_PRESETS: &str = "quick|web|top100|full|custom";

/// Preset port lists (reference `PORT_PRESETS`).
pub fn preset_ports(preset: &str) -> Option<&'static str> {
    match preset {
        "quick" => Some("21,22,25,53,80,110,143,443,445,587,993,995,3306,3389,5432,6379,8080,8443"),
        "web" => Some("80,443,8000,8080,8081,8443,8888,9000,9443"),
        "top100" => Some(
            "7,9,13,21,22,23,25,26,37,53,79,80,81,88,106,110,111,113,119,135,139,143,144,179,199,389,427,443,445,465,513,514,515,543,544,548,554,587,631,646,873,990,993,995,1025,1026,1027,1028,1029,1110,1433,1720,1723,1755,1900,2000,2001,2049,2121,2717,3000,3128,3306,3389,3986,4899,5000,5009,5051,5060,5101,5190,5357,5432,5631,5666,5800,5900,6000,6001,6646,7070,8000,8008,8009,8080,8081,8443,8888,9100,9999,10000,32768,49152,49153,49154,49155,49156,49157",
        ),
        "full" => Some("1-65535"),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn preset_label(preset: &str) -> &'static str {
    match preset {
        "quick" => "Quick",
        "web" => "Web",
        "top100" => "Top 100",
        "full" => "Full",
        "custom" => "Custom",
        _ => "Quick",
    }
}

/// Parse the ports mini-language into a sorted, de-duplicated port list.
///
/// Supported syntax (reference `parsePorts`): `80,443` · ranges `1-4`,
/// `3000..3010`, `3...5`, `3 : 5` · `80/tcp` · named keywords `web|http`,
/// `db|database`, `top100`, `quick`, `all|full` · `#` and `//` line comments ·
/// any separator mix (comma, space, semicolon, tab, pipe, newline). Ranges are
/// clamped to 1–65535, reversed ranges normalized, ranges starting at 0 are
/// dropped entirely.
pub fn parse_ports(value: &str) -> Vec<u16> {
    if !value.trim().is_empty() {
        // fallthrough to parsing below
    } else {
        return Vec::new();
    }

    let mut ports = std::collections::BTreeSet::new();

    // 1. Strip line comments.
    let cleaned: String = value
        .lines()
        .map(|line| {
            let cut = line
                .find('#')
                .or_else(|| line.find("//"))
                .unwrap_or(line.len());
            line[..cut].to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    // 2. Collapse whitespace around range operators first (reference
    //    normalizes `3 : 5` / `3 - 5` / `3 .. 5` before splitting).
    let collapsed = cleaned
        .replace("- ", "-")
        .replace(" -", "-")
        .replace(": ", ":")
        .replace(" :", ":")
        .replace(".. ", "..")
        .replace(" ..", "..");

    // 3. Normalize separators.
    let normalized = collapsed
        .replace(
            |c: char| c == ';' || c == '\t' || c == '|' || c == '\n' || c == '\r',
            ",",
        )
        .replace(' ', ",")
        .replace(",,", ",");

    for part in normalized.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let lower = token.to_lowercase();

        // Named keywords.
        match lower.as_str() {
            "web" | "http" => {
                for p in [80, 443, 8080, 8443, 8000, 8008, 8081, 3000, 5000, 8888] {
                    ports.insert(p);
                }
                continue;
            }
            "db" | "database" => {
                for p in [3306, 5432, 1433, 1521, 27017, 6379, 9200, 5984] {
                    ports.insert(p);
                }
                continue;
            }
            "top100" => {
                if let Some(list) = preset_ports("top100") {
                    for p in parse_ports(list) {
                        ports.insert(p);
                    }
                }
                continue;
            }
            "quick" => {
                if let Some(list) = preset_ports("quick") {
                    for p in parse_ports(list) {
                        ports.insert(p);
                    }
                }
                continue;
            }
            "all" | "full" => {
                for p in 1..=65535u16 {
                    ports.insert(p);
                }
                continue;
            }
            _ => {}
        }

        // Strip decorations: `80/tcp`, `80(http)`.
        let token = token
            .split('(')
            .next()
            .unwrap_or(token)
            .trim()
            .trim_end_matches("/tcp")
            .trim_end_matches("/udp")
            .trim_end_matches("/http")
            .trim_end_matches("/https")
            .to_string();

        // Range operators: `-`, `..`, `...`, `:` (longest first).
        let token = token.replace("...", "-").replace("..", "-");
        let range_split = token.split_once('-').or_else(|| token.split_once(':'));

        if let Some((start_s, end_s)) = range_split {
            let (Ok(start), Ok(end)) = (start_s.trim().parse::<u32>(), end_s.trim().parse::<u32>())
            else {
                continue;
            };
            if start == 0 || end == 0 {
                continue; // ranges starting at 0 are dropped
            }
            let (lo, hi) = (start.min(end), start.max(end));
            let lo = lo.max(1);
            let hi = hi.min(65535);
            for p in lo..=hi {
                if let Ok(p) = u16::try_from(p) {
                    ports.insert(p);
                }
            }
            continue;
        }

        if let Ok(p) = token.parse::<u16>() {
            if p >= 1 {
                ports.insert(p);
            }
        }
    }

    ports.into_iter().collect()
}

pub const PORT_PRESET_COUNTS: [(&str, &str); 5] = [
    ("quick", "Quick"),
    ("web", "Web"),
    ("top100", "Top 100"),
    ("full", "Full"),
    ("custom", "Custom"),
];

impl Bridge {
    // ── field setters ──────────────────────────────────────────────────────

    pub fn port_scan_set_target(target: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.target = target.to_string());
    }

    pub fn port_scan_set_preset(preset: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.port_scanner.preset = preset.to_string();
            if let Some(list) = preset_ports(preset) {
                s.port_scanner.ports_text = list.to_string();
            }
        });
    }

    pub fn port_scan_set_ports_text(text: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.ports_text = text.to_string());
    }

    pub fn port_scan_set_timeout(v: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.timeout_ms = v.to_string());
    }

    pub fn port_scan_set_concurrency(v: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.concurrency = v.to_string());
    }

    pub fn port_scan_set_banner_grab(v: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.banner_grab = v);
    }

    pub fn port_scan_set_stealth(v: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.port_scanner.stealth_mode = v;
            if !v {
                let notice: String =
                    "Noisy - High Performance mode enabled: high probe rate without delay \
                     may trigger SIEM/IDS alerts."
                        .to_string();
                s.push_debug_log(crate::types::DebugLogEntry::new(
                    crate::types::DebugLogLevel::Warn,
                    "port-scanner",
                    notice,
                ));
                s.last_message = Some("Noisy - High Performance mode enabled".into());
            }
        });
    }

    pub fn port_scan_set_delay(v: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.delay_ms = v.to_string());
    }

    pub fn port_scan_set_jitter(v: &str, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.jitter_ms = v.to_string());
    }

    pub fn port_scan_set_randomize(v: bool, cx: &mut App) {
        Self::state(cx).update(cx, |s| s.port_scanner.randomize_ports = v);
    }

    // ── lifecycle ──────────────────────────────────────────────────────────

    /// Start a scan. The backend call blocks until the scan completes and is
    /// therefore run off the UI thread; results stream in via events while
    /// it runs. `override_ports` lets welcome-screen quick starts scan a
    /// preset without touching the editor contents.
    pub fn port_scan_start(override_ports: Option<Vec<u16>>, cx: &mut App) -> BridgeResult<()> {
        let (request, scan_id) = {
            let state = Self::state(cx).read(cx).port_scanner.clone();
            if state.is_running {
                return Ok(());
            }
            let ports = override_ports.unwrap_or_else(|| parse_ports(&state.ports_text));
            if state.target.trim().is_empty() || ports.is_empty() {
                return Err(BridgeError::message(
                    "Set a target and at least one port before scanning",
                ));
            }
            let scan_id = uuid::Uuid::new_v4().to_string();
            let request = PortScanRequestDto {
                target: state.target.trim().to_string(),
                ports,
                timeout_ms: state.timeout_ms.parse().unwrap_or(800).clamp(1, 120_000),
                concurrency: state.concurrency.parse().unwrap_or(100).clamp(1, 200),
                banner_grab: state.banner_grab,
                stealth_mode: state.stealth_mode,
                delay_ms: state
                    .stealth_mode
                    .then_some(state.delay_ms.parse().unwrap_or(300)),
                jitter_ms: state
                    .stealth_mode
                    .then_some(state.jitter_ms.parse().unwrap_or(200)),
                randomize_ports: state.stealth_mode.then_some(state.randomize_ports),
            };
            (request, scan_id)
        };

        if !Self::state(cx).read(cx).port_scanner.stealth_mode {
            // Noisy scan about to start — warn like the reference notification.
            Self::state(cx).update(cx, |s| {
                s.push_debug_log(crate::types::DebugLogEntry::new(
                    crate::types::DebugLogLevel::Warn,
                    "port-scanner",
                    format!(
                        "Noisy - High Performance scan started on {} — may trigger SIEM/IDS alerts.",
                        request.target
                    ),
                ));
                s.last_message =
                    Some("Noisy - High Performance scan started".into());
            });
        }

        Self::state(cx).update(cx, |s| {
            s.port_scanner.results.clear();
            s.port_scanner.progress_current = 0;
            s.port_scanner.progress_total = request.ports.len();
            s.port_scanner.is_running = true;
            s.port_scanner.has_run = true;
            s.port_scanner.error = None;
            s.port_scanner.active_scan_id = Some(scan_id.clone());
        });

        let backend = Self::backend(cx)?;
        let executor = cx.background_executor().clone();
        let scan_id_for_task = scan_id.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let result = {
                let scan_id = scan_id_for_task.clone();
                // Backend calls are blocking; hop to a worker thread.
                let (tx, rx) = std::sync::mpsc::channel();
                let backend = backend.clone();
                let _ = std::thread::Builder::new()
                    .name("halo-port-scan".into())
                    .spawn(move || {
                        let _ = tx.send(backend.port_scan_start(&request, &scan_id));
                    });
                loop {
                    match rx.try_recv() {
                        Ok(result) => break result,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            executor.timer(std::time::Duration::from_millis(50)).await;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            break Err(BridgeError::message("port scan worker died"));
                        }
                    }
                }
            };

            let _ = cx.update(|cx| {
                Self::state(cx).update(cx, |s| {
                    if s.port_scanner.active_scan_id.as_deref() != Some(scan_id_for_task.as_str()) {
                        return; // superseded by a newer scan
                    }
                    s.port_scanner.is_running = false;
                    match result {
                        Ok(final_rows) => {
                            // Authoritative final list replaces streamed rows.
                            s.port_scanner.results = final_rows;
                            s.port_scanner.progress_current = s.port_scanner.progress_total;
                        }
                        Err(err) => {
                            s.port_scanner.error = Some(err.to_string());
                        }
                    }
                });
            });
        })
        .detach();
        Ok(())
    }

    pub fn port_scan_stop(cx: &mut App) -> BridgeResult<()> {
        let scan_id = Self::state(cx).read(cx).port_scanner.active_scan_id.clone();
        let Some(scan_id) = scan_id else {
            return Ok(());
        };
        let backend = Self::backend(cx)?;
        backend.port_scan_stop(&scan_id)?;
        Self::state(cx).update(cx, |s| {
            s.port_scanner.is_running = false;
        });
        Ok(())
    }

    pub fn port_scan_clear(cx: &mut App) {
        Self::state(cx).update(cx, |s| {
            s.port_scanner.results.clear();
            s.port_scanner.progress_current = 0;
            s.port_scanner.progress_total = 0;
            s.port_scanner.has_run = false;
            s.port_scanner.error = None;
        });
    }

    // ── event application (called from the event pump) ─────────────────────

    pub(super) fn apply_port_scan_result(
        state: &mut BridgeState,
        scan_id: String,
        row: PortScanResultDto,
    ) {
        if state.port_scanner.active_scan_id.as_deref() != Some(scan_id.as_str()) {
            return;
        }
        if row.state != "open" {
            return;
        }
        state.port_scanner.results.push(row);
        state
            .port_scanner
            .results
            .sort_by_key(|r| (r.host.clone(), r.port));
    }

    pub(super) fn apply_port_scan_progress(
        state: &mut BridgeState,
        scan_id: String,
        kind: &str,
        current: usize,
        total: usize,
    ) {
        if state.port_scanner.active_scan_id.as_deref() != Some(scan_id.as_str()) {
            return;
        }
        match kind {
            "Update" => {
                state.port_scanner.progress_current = current;
                state.port_scanner.progress_total = total;
            }
            "Complete" | "Cancelled" => {
                state.port_scanner.is_running = false;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_separators_and_dedupes() {
        assert_eq!(parse_ports("443 80 443"), vec![80, 443]);
        assert_eq!(
            parse_ports("22,80\n443;8080\t9000"),
            vec![22, 80, 443, 8080, 9000]
        );
    }

    #[test]
    fn parses_range_operators() {
        assert_eq!(parse_ports("3-5"), vec![3, 4, 5]);
        assert_eq!(parse_ports("3 .. 5"), vec![3, 4, 5]);
        assert_eq!(parse_ports("3...5"), vec![3, 4, 5]);
        assert_eq!(parse_ports("3 : 5"), vec![3, 4, 5]);
    }

    #[test]
    fn normalizes_reversed_ranges_and_clamps() {
        assert_eq!(parse_ports("5-3"), vec![3, 4, 5]);
        assert_eq!(parse_ports("0-2"), Vec::<u16>::new());
        assert_eq!(
            parse_ports("65530-70000"),
            vec![65530, 65531, 65532, 65533, 65534, 65535]
        );
    }

    #[test]
    fn understands_named_keywords() {
        assert_eq!(parse_ports("web"), parse_ports("http"));
        assert!(parse_ports("web").contains(&443));
        assert!(parse_ports("db").contains(&3306));
        assert!(parse_ports("db").contains(&6379));
        assert_eq!(parse_ports("full").len(), 65535);
        assert!(parse_ports("top100").contains(&22));
    }

    #[test]
    fn strips_decorations_and_comments() {
        assert_eq!(parse_ports("80/tcp 443/https 25//smtp"), vec![25, 80, 443]);
        assert_eq!(parse_ports("port 80"), vec![80]);
        assert_eq!(parse_ports("80(http)"), vec![80]);
        assert_eq!(parse_ports("80 # web\n443 // tls"), vec![80, 443]);
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(parse_ports("").is_empty());
        assert!(parse_ports("   \n  ").is_empty());
    }
}
