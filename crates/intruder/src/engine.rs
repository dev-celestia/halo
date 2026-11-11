use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use halo_core::tauri::{AppHandle, Emitter, Manager, State};
use parking_lot::Mutex;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::markers::{count_markers, marker_defaults, replace_marked_values_tracked};
use crate::matching::{extract_grep_value, extract_session_value, response_matches};
use crate::payloads::{
    build_invoker_payload_rows, build_payload_source, build_position_payload_sources,
};
use crate::types::{
    InvokerAttackConfig, InvokerAttackResult, InvokerProgress, InvokerResponse, InvokerState,
};

pub async fn start_invoker_attack(
    app: AppHandle,
    state: State<'_, InvokerState>,
    config: InvokerAttackConfig,
) -> Result<String, String> {
    let _ = state;
    begin_invoker_attack(app, config)
}

/// Sync entry point for in-process bridge callers (no Tauri `State` wrapper).
pub fn begin_invoker_attack(app: AppHandle, config: InvokerAttackConfig) -> Result<String, String> {
    validate_invoker_config(&config)?;

    let attack_id = Uuid::new_v4().to_string();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let state = app.state::<InvokerState>();
    state
        .cancellations
        .lock()
        .insert(attack_id.clone(), cancel_flag.clone());

    let event_id = attack_id.clone();
    let cancellations = state.cancellations.clone();
    tokio::spawn(async move {
        run_invoker_attack(app, event_id.clone(), config, cancel_flag).await;
        cancellations.lock().remove(&event_id);
    });

    Ok(attack_id)
}

pub async fn stop_invoker_attack(
    state: State<'_, InvokerState>,
    attack_id: String,
) -> Result<(), String> {
    cancel_invoker_attack(&state, &attack_id);
    Ok(())
}

pub fn cancel_invoker_attack(state: &InvokerState, attack_id: &str) {
    if let Some(cancel_flag) = state.cancellations.lock().remove(attack_id) {
        cancel_flag.store(true, Ordering::Relaxed);
    }
}

pub fn validate_invoker_config(config: &InvokerAttackConfig) -> Result<(), String> {
    if config.base_request.url.trim().is_empty() {
        return Err("Base request URL is required".to_string());
    }

    if count_markers(&config.base_request) == 0 {
        return Err("Add at least one payload position with § markers".to_string());
    }

    if config
        .positions
        .iter()
        .any(|position| position.end < position.start)
    {
        return Err("Payload position ranges are invalid".to_string());
    }

    if config
        .position_payloads
        .as_ref()
        .is_some_and(|position_payloads| !position_payloads.is_empty())
    {
        let sources = build_position_payload_sources(config, count_markers(&config.base_request))?;
        if sources.is_empty() {
            return Err("Add payloads for every marked position".to_string());
        }
    } else if build_payload_source(&config.payload_config)?.is_empty() {
        return Err("Add at least one payload".to_string());
    }

    Ok(())
}

pub async fn run_invoker_attack(
    app: AppHandle,
    attack_id: String,
    config: InvokerAttackConfig,
    cancel_flag: Arc<AtomicBool>,
) {
    let defaults = marker_defaults(&config.base_request);
    let position_count = defaults.len();
    let payloads = match build_invoker_payload_rows(&config, position_count) {
        Ok(values) => values,
        Err(error) => {
            let _ = app.emit(
                &format!("invoker-result-{}", attack_id),
                InvokerAttackResult::error(error),
            );
            let _ = app.emit(
                &format!("invoker-progress-{}", attack_id),
                InvokerProgress::Complete,
            );
            return;
        }
    };

    let total = payloads.len();
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let semaphore = Arc::new(Semaphore::new(config.concurrency.clamp(1, 200)));
    let client = match reqwest::Client::builder()
        .redirect(if config.base_request.follow_redirects {
            reqwest::redirect::Policy::limited(config.base_request.max_hops.max(1))
        } else {
            reqwest::redirect::Policy::none()
        })
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            let _ = app.emit(
                &format!("invoker-result-{}", attack_id),
                InvokerAttackResult::error(format!("Failed to build HTTP client: {}", error)),
            );
            let _ = app.emit(
                &format!("invoker-progress-{}", attack_id),
                InvokerProgress::Complete,
            );
            return;
        }
    };

    let session_value = Arc::new(Mutex::new(None::<String>));
    let mut handles = Vec::with_capacity(total);

    for (index, payload_values) in payloads.into_iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let permit = match semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => break,
        };
        let app = app.clone();
        let attack_id = attack_id.clone();
        let config = config.clone();
        let defaults = defaults.clone();
        let client = client.clone();
        let completed = completed.clone();
        let cancel_flag = cancel_flag.clone();
        let session_value = session_value.clone();

        handles.push(tokio::spawn(async move {
            let _permit = permit;
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }

            apply_invoker_delay(&config, index).await;
            let result = send_invoker_request(
                &client,
                &config,
                &defaults,
                payload_values,
                session_value,
                cancel_flag.clone(),
            )
            .await;

            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(&format!("invoker-result-{}", attack_id), result);
            let _ = app.emit(
                &format!("invoker-progress-{}", attack_id),
                InvokerProgress::Update { current, total },
            );
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let _ = app.emit(
        &format!("invoker-progress-{}", attack_id),
        InvokerProgress::Complete,
    );
}

pub async fn apply_invoker_delay(config: &InvokerAttackConfig, index: usize) {
    let base_delay = match config.delay_max_ms {
        Some(max_delay) if max_delay > config.delay_ms => {
            let span = max_delay - config.delay_ms;
            config.delay_ms + ((index as u64 * 1_103_515_245 + 12_345) % (span + 1))
        }
        _ => config.delay_ms,
    };

    if base_delay > 0 {
        tokio::time::sleep(Duration::from_millis(base_delay)).await;
    }
}

pub async fn send_invoker_request(
    client: &reqwest::Client,
    config: &InvokerAttackConfig,
    defaults: &[String],
    payload_values: HashMap<String, String>,
    session_value: Arc<Mutex<Option<String>>>,
    cancel_flag: Arc<AtomicBool>,
) -> InvokerAttackResult {
    let mut last_error = None;

    for attempt in 0..=config.retries {
        if cancel_flag.load(Ordering::Relaxed) {
            last_error = Some("Attack stopped".to_string());
            break;
        }

        match send_invoker_request_once(client, config, defaults, &payload_values, &session_value)
            .await
        {
            Ok(result) => return result,
            Err(error) => {
                last_error = Some(error);
                if attempt < config.retries {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
        }
    }

    InvokerAttackResult {
        id: Uuid::new_v4().to_string(),
        payload_values,
        status: None,
        response_length: None,
        response_time_ms: None,
        error: last_error,
        comment: Some(config.name.clone()),
        response: None,
        grep_match: false,
        grep_extracted: None,
    }
}

pub async fn send_invoker_request_once(
    client: &reqwest::Client,
    config: &InvokerAttackConfig,
    defaults: &[String],
    payload_values: &HashMap<String, String>,
    session_value: &Arc<Mutex<Option<String>>>,
) -> Result<InvokerAttackResult, String> {
    let method = reqwest::Method::from_bytes(config.base_request.method.as_bytes())
        .map_err(|error| format!("Invalid HTTP method: {}", error))?;
    let mut position_index = 0;
    let url = replace_marked_values_tracked(
        &config.base_request.url,
        payload_values,
        defaults,
        &mut position_index,
    );
    let mut sorted_headers: Vec<_> = config
        .base_request
        .headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("host"))
        .collect();
    sorted_headers.sort_by_key(|(k, _)| k.to_lowercase());
    let mut headers = HashMap::new();

    for (name, value) in sorted_headers {
        headers.insert(
            replace_marked_values_tracked(name, payload_values, defaults, &mut position_index),
            replace_marked_values_tracked(value, payload_values, defaults, &mut position_index),
        );
    }

    let body = replace_marked_values_tracked(
        &config.base_request.body,
        payload_values,
        defaults,
        &mut position_index,
    );

    if config.session_handling.enabled {
        if let Some(header_name) = config.session_handling.update_header_name.as_ref() {
            let current_session = session_value.lock();
            if let Some(value) = current_session.as_ref() {
                headers.insert(header_name.clone(), value.clone());
            }
        }
    }

    let mut builder = client.request(method, &url);
    for (name, value) in &headers {
        if name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(name, value);
    }

    if !body.is_empty() {
        builder = builder.body(body);
    }

    let started_at = Instant::now();
    let response = builder
        .send()
        .await
        .map_err(|error| format!("Failed to send request: {}", error))?;
    let elapsed_ms = started_at.elapsed().as_millis();
    let status = response.status();
    let final_url = response.url().to_string();
    let response_headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    let response_body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read response body: {}", error))?;
    let response_length = response_body.len();

    let grep_match = response_matches(&response_body, &config.grep_match);
    let grep_extracted = extract_grep_value(&response_body, &config.grep_extract);

    if config.session_handling.enabled {
        if let Some(next_session) = extract_session_value(&response_body, &config.session_handling)
        {
            let mut current_session = session_value.lock();
            {
                *current_session = Some(next_session);
            }
        }
    }

    const MAX_INTRUDER_BODY_CHARS: usize = 64 * 1024;
    let mut stored_body = response_body;
    if stored_body.len() > MAX_INTRUDER_BODY_CHARS {
        let mut end = MAX_INTRUDER_BODY_CHARS;
        while end > 0 && !stored_body.is_char_boundary(end) {
            end -= 1;
        }
        stored_body.truncate(end);
    }

    Ok(InvokerAttackResult {
        id: Uuid::new_v4().to_string(),
        payload_values: payload_values.clone(),
        status: Some(status.as_u16()),
        response_length: Some(response_length),
        response_time_ms: Some(elapsed_ms),
        error: None,
        comment: None,
        response: Some(InvokerResponse {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or_default().to_string(),
            headers: response_headers,
            body: stored_body,
            time_ms: elapsed_ms,
            final_url,
        }),
        grep_match,
        grep_extracted,
    })
}
