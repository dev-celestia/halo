use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use halo_core::tauri::{AppHandle, Emitter, Manager};
use hexbuffer_proxy::WebSocketMessage as Message;
use hexbuffer_proxy::{Body, HttpContext, HttpHandler, RequestOrResponse};
use http_body_util::BodyExt;
use hyper::{Method, Request, Response, StatusCode, Uri, header::HeaderName, header::HeaderValue};

use crate::completion::save_and_emit;
use crate::mock_forge;
use crate::state::{WebSocketMessageDirection, WebSocketMessageRecord, WebSocketMessageType};
use crate::websocket;

/// Responses larger than this are streamed straight through to the client
/// without being buffered: they are not recorded and not intercept-editable.
const MAX_BUFFERED_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
/// Bodies on records sent to the persistence worker are capped so the
/// log channel and any in-memory record retention stay bounded.
/// Matches `PayloadStore::MAX_BODY_SIZE` (2 MiB).
pub const MAX_RECORDED_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Maximum in-flight request contexts retained waiting for `handle_response`.
const MAX_PENDING_CTXS: usize = 512;
/// Maximum WebSocket frame payload bytes stored per recorded message (128 KiB).
const MAX_WS_RECORDED_PAYLOAD_BYTES: usize = 128 * 1024;

/// Flattens a header into the recording map. Duplicate names are preserved by
/// joining values (", " per RFC 9110 §5.2; "; " for Set-Cookie, which must not
/// be comma-merged) instead of silently overwritten.
fn record_header(headers: &mut std::collections::HashMap<String, String>, name: &str, value: &str) {
    match headers.get_mut(name) {
        Some(existing) => {
            let separator = if name.eq_ignore_ascii_case("set-cookie") {
                "; "
            } else {
                ", "
            };
            existing.push_str(separator);
            existing.push_str(value);
        }
        None => {
            headers.insert(name.to_string(), value.to_string());
        }
    }
}

/// Poll until the user resolves a paused request, or the deadline expires.
///
/// On timeout the request is forwarded **unchanged** — the same outcome as
/// "Forward without edits" — so a client is never left hanging on a paused
/// request the user forgot about, and a page that opens hundreds of
/// connections cannot pin an unbounded number of tasks and connections.
async fn wait_for_intercept_decision(
    proxy_state: &crate::ProxyState,
    paused_id: &uuid::Uuid,
    uri: &str,
    is_response: bool,
) {
    use crate::state::PAUSE_TIMEOUT;

    let deadline = std::time::Instant::now() + PAUSE_TIMEOUT;

    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;

        if proxy_state.get_paused_request(paused_id).is_none() {
            return;
        }

        if std::time::Instant::now() >= deadline {
            eprintln!(
                "[intercept] {} paused for more than {}s; forwarding unchanged: {}",
                if is_response { "response" } else { "request" },
                PAUSE_TIMEOUT.as_secs(),
                uri
            );
            if is_response {
                proxy_state.forward_paused_response(paused_id, None);
            } else {
                proxy_state.forward_paused_request(paused_id, None, false);
            }
            return;
        }
    }
}

#[derive(Clone)]
pub struct Ctx {
    pub transaction_id: uuid::Uuid,
    pub client_addr: String,
    pub server_addr: String,
    pub req_method: String,
    pub req_uri: String,
    pub req_http_version: String,
    pub req_headers: std::collections::HashMap<String, String>,
    pub req_body: Vec<u8>,
    pub res_status_code: u16,
    pub res_status_text: String,
    pub res_http_version: String,
    pub res_headers: std::collections::HashMap<String, String>,
    pub res_body: Vec<u8>,
    pub req_content_decoded: bool,
    pub res_content_decoded: bool,
    pub paused_id: Option<uuid::Uuid>,
    pub app_handle: AppHandle,
    pub response_recorded: bool,
    pub is_mitm_loopback: bool,
    pub sni_override: Option<String>,
    pub intercept_response: bool,
    pub intercept_tab_id: Option<String>,
}

impl Ctx {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            transaction_id: uuid::Uuid::new_v4(),
            client_addr: String::new(),
            server_addr: String::new(),
            req_method: String::new(),
            req_uri: String::new(),
            req_http_version: String::new(),
            req_headers: std::collections::HashMap::new(),
            req_body: Vec::new(),
            res_status_code: 0,
            res_status_text: String::new(),
            res_http_version: String::new(),
            res_headers: std::collections::HashMap::new(),
            res_body: Vec::new(),
            req_content_decoded: false,
            res_content_decoded: false,
            paused_id: None,
            app_handle,
            response_recorded: false,
            is_mitm_loopback: false,
            sni_override: None,
            intercept_response: false,
            intercept_tab_id: None,
        }
    }
}

#[derive(Clone)]
pub struct AppHandler {
    app_handle: AppHandle,
    pending_ctxs: Arc<Mutex<HashMap<u64, Ctx>>>,
    ws_connections: Arc<Mutex<HashMap<String, uuid::Uuid>>>,
}

impl AppHandler {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            pending_ctxs: Arc::new(Mutex::new(HashMap::new())),
            ws_connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn insert_pending_ctx(&self, id: u64, ctx: Ctx) {
        if ctx.req_method.eq_ignore_ascii_case("CONNECT") {
            return;
        }
        let mut guard = self.pending_ctxs.lock();
        while guard.len() >= MAX_PENDING_CTXS {
            if let Some(&oldest_id) = guard.keys().min() {
                guard.remove(&oldest_id);
            } else {
                break;
            }
        }
        guard.insert(id, ctx);
    }
}

#[async_trait::async_trait]
impl HttpHandler for AppHandler {
    async fn should_intercept_tls(&self, host: &str) -> bool {
        let proxy_state = self.app_handle.state::<crate::ProxyState>();
        let is_bypassed = proxy_state.should_bypass_uri(host);
        !is_bypassed
    }

    async fn handle_request(
        &self,
        http_ctx: &mut HttpContext,
        req: Request<Body>,
    ) -> hexbuffer_proxy::Result<RequestOrResponse> {
        use crate::state::{InterceptMode, PausedRequest, ProxyState};

        let mut ctx = Ctx::new(self.app_handle.clone());

        ctx.client_addr = http_ctx.client_addr.to_string();
        ctx.req_method = req.method().to_string();
        ctx.req_http_version = format!("{:?}", req.version());

        let (mut parts, body) = req.into_parts();
        if self
            .app_handle
            .state::<crate::ProxyState>()
            .strip_cache_validators()
        {
            parts.headers.remove("if-none-match");
            parts.headers.remove("if-modified-since");
        }

        for (name, value) in parts.headers.iter() {
            if let Ok(v) = value.to_str() {
                record_header(&mut ctx.req_headers, name.as_str(), v);
            }
        }

        let raw_uri = parts.uri.to_string();
        let (host, full_url) = if raw_uri.starts_with("http://") || raw_uri.starts_with("https://")
        {
            let parsed_host = parts
                .uri
                .host()
                .map(|h| {
                    if let Some(port) = parts.uri.port_u16() {
                        if (raw_uri.starts_with("https://") && port == 443)
                            || (raw_uri.starts_with("http://") && port == 80)
                        {
                            h.to_string()
                        } else {
                            format!("{}:{}", h, port)
                        }
                    } else {
                        h.to_string()
                    }
                })
                .unwrap_or_default();
            (parsed_host, raw_uri)
        } else {
            let host_header = ctx
                .req_headers
                .get("host")
                .or_else(|| ctx.req_headers.get("Host"))
                .or_else(|| ctx.req_headers.get(":authority"))
                .cloned()
                .unwrap_or_else(|| {
                    if !http_ctx.host.is_empty() {
                        http_ctx.host.clone()
                    } else {
                        String::new()
                    }
                });

            let scheme = if ctx
                .req_headers
                .get("x-forwarded-proto")
                .map(|v| v.eq_ignore_ascii_case("http"))
                .unwrap_or(false)
                || host_header.ends_with(":80")
            {
                "http"
            } else {
                "https"
            };

            let path = if raw_uri.starts_with('/') {
                raw_uri
            } else if raw_uri.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", raw_uri)
            };

            let normalized_url = if !host_header.is_empty() {
                format!("{}://{}{}", scheme, host_header, path)
            } else {
                path
            };

            (host_header, normalized_url)
        };

        ctx.server_addr = if !host.is_empty() {
            host
        } else {
            http_ctx.host.clone()
        };
        ctx.req_uri = full_url;

        let body_bytes: bytes::Bytes = match body {
            Body::Full(bytes) => bytes,
            Body::Streaming(boxed) => {
                match http_body_util::Limited::new(boxed, MAX_BUFFERED_RESPONSE_BODY_BYTES)
                    .collect()
                    .await
                {
                    Ok(collected) => collected.to_bytes(),
                    Err(e) => {
                        eprintln!("[lifecycle] Failed or oversized request body: {}", e);
                        // The body may be partially consumed; dropping the stale
                        // content-length keeps the upstream framing consistent for
                        // the now-empty body.
                        parts.headers.remove(hyper::header::CONTENT_LENGTH);
                        return Ok(RequestOrResponse::Request(Request::from_parts(
                            parts,
                            Body::from(bytes::Bytes::new()),
                        )));
                    }
                }
            }
        };
        let req_cap = body_bytes.len().min(MAX_BUFFERED_RESPONSE_BODY_BYTES);
        ctx.req_body = body_bytes[..req_cap].to_vec();

        let mut body_modified = false;

        // ponytail: delegate MockForge interception to its own module
        if ctx.req_method != "CONNECT" {
            if let Some(resp) = mock_forge::try_intercept(&self.app_handle, &ctx).await {
                return Ok(resp);
            }
        }

        let proxy_state = self.app_handle.state::<ProxyState>();
        let should_bypass_uri = proxy_state.should_bypass_uri(&ctx.req_uri);

        if ctx.req_method != "CONNECT" && !should_bypass_uri {
            let mode = proxy_state.get_mode();

            if mode == InterceptMode::Enabled {
                let intercept_tab_id = proxy_state.matching_intercept_tab_id(&ctx.req_uri);

                if intercept_tab_id.is_none() {
                    self.insert_pending_ctx(http_ctx.id, ctx);
                    // Bytes clone is a refcount bump, not a full body copy.
                    let req = Request::from_parts(parts, Body::from(body_bytes.clone()));
                    return Ok(RequestOrResponse::Request(req));
                }

                let paused_id = ctx.transaction_id;
                ctx.paused_id = Some(paused_id);
                ctx.intercept_tab_id = intercept_tab_id.clone();

                let paused_req = PausedRequest {
                    id: paused_id,
                    timestamp: chrono::Utc::now(),
                    client_addr: ctx.client_addr.clone(),
                    server_addr: ctx.server_addr.clone(),
                    tab_id: intercept_tab_id,
                    request: crate::state::ProxyRequest {
                        method: ctx.req_method.clone(),
                        uri: ctx.req_uri.clone(),
                        http_version: ctx.req_http_version.clone(),
                        headers: ctx.req_headers.clone(),
                        body: ctx.req_body.clone(),
                        content_decoded: ctx.req_content_decoded,
                    },
                    response: None,
                };

                // Fail open when the queue is full: forwarding unmodified is
                // always better than OOM-ing the proxy.
                if proxy_state.add_paused_request(paused_req.clone()) {
                    crate::automation::ingest_intercept_paused_request(
                        &self.app_handle,
                        &paused_req,
                    );

                    wait_for_intercept_decision(&proxy_state, &paused_id, &ctx.req_uri, false)
                        .await;

                    let action = proxy_state.take_paused_action(&paused_id);

                    match action {
                        Some(crate::state::InterceptAction::Drop) => {
                            return Ok(RequestOrResponse::Response(
                                Response::builder()
                                    .status(502)
                                    .body(Body::from(bytes::Bytes::from("Dropped by intercept")))
                                    .unwrap_or_else(|_| {
                                        Response::new(Body::from(bytes::Bytes::new()))
                                    }),
                            ));
                        }
                        Some(crate::state::InterceptAction::Forward {
                            request: Some(modified),
                            intercept_response,
                        }) => {
                            ctx.intercept_response = intercept_response;

                            if let Ok(method) = modified.method.parse::<Method>() {
                                ctx.req_method = method.to_string();
                                parts.method = method;
                            }

                            if let Ok(uri) = modified.uri.parse::<Uri>() {
                                ctx.req_uri = uri.to_string();
                                ctx.server_addr = uri.to_string();
                                parts.uri = uri;
                            }

                            parts.headers.clear();
                            ctx.req_headers = modified.headers.clone();
                            for (name, value) in modified.headers.iter() {
                                let Ok(header_name) = HeaderName::from_bytes(name.as_bytes())
                                else {
                                    continue;
                                };
                                let Ok(header_value) = HeaderValue::from_str(value) else {
                                    continue;
                                };
                                parts.headers.insert(header_name, header_value);
                            }

                            ctx.req_body = modified.body;
                            body_modified = true;
                        }
                        Some(crate::state::InterceptAction::Forward {
                            request: None,
                            intercept_response,
                        }) => {
                            ctx.intercept_response = intercept_response;
                        }
                        _ => {}
                    }
                } else {
                    eprintln!(
                        "[intercept] pause queue full ({} requests); forwarding without pausing: {}",
                        crate::state::MAX_PAUSED_REQUESTS,
                        ctx.req_uri
                    );
                }
            }
        }

        // Optionally strip conditional request headers so servers return full
        // bodies instead of 304 — off by default, because it changes the
        // traffic the user is trying to observe.
        if self
            .app_handle
            .state::<crate::ProxyState>()
            .strip_cache_validators()
        {
            parts.headers.remove("if-none-match");
            parts.headers.remove("if-modified-since");
        }

        let request_body = if body_modified {
            bytes::Bytes::from(ctx.req_body.clone())
        } else {
            // Bytes clone is a refcount bump, not a full body copy.
            body_bytes.clone()
        };
        self.insert_pending_ctx(http_ctx.id, ctx);
        let req = Request::from_parts(parts, Body::from(request_body));

        Ok(RequestOrResponse::Request(req))
    }

    async fn handle_response(
        &self,
        http_ctx: &mut HttpContext,
        res: Response<Body>,
    ) -> hexbuffer_proxy::Result<Response<Body>> {
        let mut ctx = match self.pending_ctxs.lock().remove(&http_ctx.id) {
            Some(c) => c,
            None => {
                eprintln!(
                    "[lifecycle] ERROR: No ctx found for req_id={} in handle_response",
                    http_ctx.id
                );
                return Ok(res);
            }
        };

        ctx.res_status_code = res.status().as_u16();
        ctx.res_status_text = res
            .status()
            .canonical_reason()
            .unwrap_or("Unknown")
            .to_string();
        ctx.res_http_version = format!("{:?}", res.version());

        for (name, value) in res.headers().iter() {
            if let Ok(v) = value.to_str() {
                record_header(&mut ctx.res_headers, name.as_str(), v);
            }
        }

        let (mut parts, body) = res.into_parts();

        // Buffering exists for recording/interception only; the crate streams
        // whatever body we return back to the client. Responses declared
        // larger than the cap are streamed through untouched — not recorded
        // and not intercept-editable — instead of being buffered in full.
        let content_length: Option<u64> = parts
            .headers
            .get(hyper::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        if ctx.req_method != "CONNECT"
            && content_length.unwrap_or(0) > MAX_BUFFERED_RESPONSE_BODY_BYTES as u64
        {
            eprintln!(
                "[lifecycle] Response body of {} bytes exceeds buffer cap ({}); streaming through unrecorded: {}",
                content_length.unwrap_or(0),
                MAX_BUFFERED_RESPONSE_BODY_BYTES,
                ctx.req_uri
            );
            ctx.res_body = Vec::new();
            save_and_emit(&ctx, &self.app_handle).await;
            return Ok(Response::from_parts(parts, body));
        }

        let body_bytes: bytes::Bytes = match body {
            Body::Full(bytes) => bytes,
            Body::Streaming(boxed) => {
                if content_length.is_some() {
                    // Bounded by the declared content-length (checked against
                    // the cap above).
                    match boxed.collect().await {
                        Ok(collected) => collected.to_bytes(),
                        Err(e) => {
                            eprintln!("[lifecycle] Failed to read response body: {}", e);
                            // The body may be partially consumed; dropping the
                            // stale content-length keeps the client-side
                            // framing consistent.
                            parts.headers.remove(hyper::header::CONTENT_LENGTH);
                            return Ok(Response::from_parts(
                                parts,
                                Body::from(bytes::Bytes::new()),
                            ));
                        }
                    }
                } else {
                    // No content-length (chunked): cap the collection so a
                    // huge or unbounded stream can't exhaust memory. Exceeding
                    // the cap fails the transaction with a visible 502.
                    match http_body_util::Limited::new(boxed, MAX_BUFFERED_RESPONSE_BODY_BYTES)
                        .collect()
                        .await
                    {
                        Ok(collected) => collected.to_bytes(),
                        Err(e) => {
                            if e.is::<http_body_util::LengthLimitError>() {
                                eprintln!(
                                    "[lifecycle] Chunked response exceeded buffer cap ({} bytes) — {}",
                                    MAX_BUFFERED_RESPONSE_BODY_BYTES, ctx.req_uri
                                );
                                return Ok(Response::builder()
                                    .status(StatusCode::BAD_GATEWAY)
                                    .header("content-type", "text/plain; charset=utf-8")
                                    .body(Body::from(bytes::Bytes::from(
                                        "Response body exceeded proxy buffer limit",
                                    )))
                                    .unwrap_or_else(|_| {
                                        Response::new(Body::from(bytes::Bytes::new()))
                                    }));
                            }
                            eprintln!("[lifecycle] Failed to read response body: {}", e);
                            parts.headers.remove(hyper::header::CONTENT_LENGTH);
                            return Ok(Response::from_parts(
                                parts,
                                Body::from(bytes::Bytes::new()),
                            ));
                        }
                    }
                }
            }
        };

        ctx.res_body = body_bytes.to_vec();

        // Bytes clone is a refcount bump, not a full body copy.
        let mut response_body = body_bytes.clone();

        if ctx.req_method != "CONNECT" {
            let proxy_state = self.app_handle.state::<crate::ProxyState>();
            let should_bypass_uri = proxy_state.should_bypass_uri(&ctx.req_uri);
            let mode = proxy_state.get_mode();

            if ctx.intercept_response
                && mode == crate::state::InterceptMode::Enabled
                && !should_bypass_uri
                && ctx.intercept_tab_id.is_some()
            {
                let paused_id = ctx.transaction_id;
                ctx.paused_id = Some(paused_id);

                let paused_req = crate::state::PausedRequest {
                    id: paused_id,
                    timestamp: chrono::Utc::now(),
                    client_addr: ctx.client_addr.clone(),
                    server_addr: ctx.server_addr.clone(),
                    tab_id: ctx.intercept_tab_id.clone(),
                    request: crate::state::ProxyRequest {
                        method: ctx.req_method.clone(),
                        uri: ctx.req_uri.clone(),
                        http_version: ctx.req_http_version.clone(),
                        headers: ctx.req_headers.clone(),
                        body: ctx.req_body.clone(),
                        content_decoded: ctx.req_content_decoded,
                    },
                    response: Some(crate::state::ProxyResponse {
                        status_code: ctx.res_status_code,
                        status_text: ctx.res_status_text.clone(),
                        http_version: ctx.res_http_version.clone(),
                        headers: ctx.res_headers.clone(),
                        body: ctx.res_body.clone(),
                        content_decoded: ctx.res_content_decoded,
                    }),
                };

                if proxy_state.add_paused_request(paused_req.clone()) {
                    crate::automation::ingest_intercept_paused_request(
                        &self.app_handle,
                        &paused_req,
                    );

                    wait_for_intercept_decision(&proxy_state, &paused_id, &ctx.req_uri, true).await;

                    let action = proxy_state.take_paused_action(&paused_id);

                    match action {
                        Some(crate::state::InterceptAction::Drop) => {
                            return Ok(Response::builder()
                                .status(502)
                                .body(Body::from(bytes::Bytes::from("Dropped by intercept")))
                                .unwrap_or_else(|_| {
                                    Response::new(Body::from(bytes::Bytes::new()))
                                }));
                        }
                        Some(crate::state::InterceptAction::ForwardResponse(Some(modified))) => {
                            if let Ok(status) = StatusCode::from_u16(modified.status_code) {
                                parts.status = status;
                                ctx.res_status_code = modified.status_code;
                                ctx.res_status_text = modified.status_text.clone();
                            }

                            parts.headers.clear();
                            ctx.res_headers = modified.headers.clone();
                            for (name, value) in modified.headers.iter() {
                                let Ok(header_name) = HeaderName::from_bytes(name.as_bytes())
                                else {
                                    continue;
                                };
                                let Ok(header_value) = HeaderValue::from_str(value) else {
                                    continue;
                                };
                                parts.headers.insert(header_name, header_value);
                            }

                            ctx.res_body = modified.body.clone();
                            ctx.res_content_decoded = false;
                            response_body = bytes::Bytes::from(modified.body);
                        }
                        _ => {}
                    }
                } else {
                    eprintln!(
                        "[intercept] pause queue full ({} requests); forwarding response without pausing: {}",
                        crate::state::MAX_PAUSED_REQUESTS,
                        ctx.req_uri
                    );
                }
            }
        }

        if ctx.req_method != "CONNECT" {
            save_and_emit(&ctx, &self.app_handle).await;
        }

        Ok(Response::from_parts(parts, Body::from(response_body)))
    }
}

#[async_trait::async_trait]
impl hexbuffer_proxy::WebSocketHandler for AppHandler {
    async fn on_upgrade(&self, ctx: &mut HttpContext, request: Request<Body>) -> Request<Body> {
        let connection_id = uuid::Uuid::new_v4();
        let client_addr = ctx.client_addr.to_string();
        let req_uri = request.uri().to_string();

        let req_headers: HashMap<String, String> = {
            let mut headers = HashMap::new();
            for (name, value) in request.headers().iter() {
                if let Ok(v) = value.to_str() {
                    record_header(&mut headers, name.as_str(), v);
                }
            }
            headers
        };

        let (host, path, url) = websocket::parse_websocket_target(&req_uri, &req_headers);
        let key = format!("{}|{}|{}", client_addr, host, path);

        let should_record =
            if let Some(proxy_state) = self.app_handle.try_state::<crate::ProxyState>() {
                proxy_state.should_record_ws_to_db(&host, &req_uri, &req_headers)
            } else {
                true
            };

        if should_record {
            self.ws_connections.lock().insert(key, connection_id);

            let now = chrono::Utc::now();
            let record = crate::state::WebSocketConnectionRecord {
                id: connection_id,
                session_id: String::new(),
                timestamp: now,
                url,
                host,
                path,
                handshake_request_headers: req_headers,
                handshake_response_status: Some(101),
                handshake_response_headers: HashMap::new(),
                client_addr: client_addr.clone(),
                server_addr: req_uri,
                state: crate::state::WebSocketConnectionState::Open,
                message_count: 0,
                last_activity_at: now,
            };

            // SQLite session lookup + insert are blocking; keep them off the
            // async runtime and emit once the record is persisted.
            let app_handle = self.app_handle.clone();
            let saved = halo_core::tauri::async_runtime::spawn_blocking(move || {
                let mut record = record;
                if let Some(history) = app_handle.try_state::<halo_storage::HistoryBridge>() {
                    if let Ok(Some(active)) = history.get_active_http_session() {
                        record.session_id = active.id;
                    }
                    if let Err(e) = history.insert_websocket_connection(&record) {
                        eprintln!("[websocket] failed to insert WS connection: {}", e);
                    }
                }
                record
            })
            .await;

            if let Ok(record) = saved {
                if let Err(e) = self.app_handle.emit("websocket-connection", &record) {
                    eprintln!("[websocket] failed to emit WS connection event: {}", e);
                }
            }
        }

        request
    }

    async fn on_frame(
        &self,
        ctx: &mut HttpContext,
        msg: hexbuffer_proxy::WebSocketMessage,
        direction: hexbuffer_proxy::Direction,
    ) -> Option<hexbuffer_proxy::WebSocketMessage> {
        let ws_direction = match direction {
            hexbuffer_proxy::Direction::ClientToServer => WebSocketMessageDirection::Inbound,
            hexbuffer_proxy::Direction::ServerToClient => WebSocketMessageDirection::Outbound,
        };
        let client_addr = ctx.client_addr.to_string();
        let uri = ctx.host.clone();

        let message_type = match &msg {
            Message::Text(_) => WebSocketMessageType::Text,
            Message::Binary(_) => WebSocketMessageType::Binary,
            Message::Ping(_) => WebSocketMessageType::Ping,
            Message::Pong(_) => WebSocketMessageType::Pong,
            Message::Close(_) => WebSocketMessageType::Close,
            Message::Frame(_) => WebSocketMessageType::Binary,
        };

        let payload = match &msg {
            Message::Text(text) => text.as_bytes().to_vec(),
            Message::Binary(data) => data.to_vec(),
            Message::Ping(data) | Message::Pong(data) => data.to_vec(),
            Message::Close(data) => {
                if let Some(frame) = data {
                    let reason = frame.reason.clone();
                    let code: u16 = frame.code.into();
                    let mut payload = vec![(code >> 8) as u8, (code & 0xFF) as u8];
                    payload.extend_from_slice(reason.as_bytes());
                    payload
                } else {
                    Vec::new()
                }
            }
            Message::Frame(data) => data.payload().to_vec(),
        };

        let uri_str = uri.to_string();
        let empty_headers = HashMap::new();
        let (host, path, url) = websocket::parse_websocket_target(&uri_str, &empty_headers);
        let key = format!("{}|{}|{}", client_addr, host, path);

        let connection_id = self.ws_connections.lock().get(&key).copied();

        if let Some(connection_id) = connection_id {
            let now = chrono::Utc::now();
            let payload_size = payload.len();
            let mut recorded_payload = payload;
            if recorded_payload.len() > MAX_WS_RECORDED_PAYLOAD_BYTES {
                recorded_payload.truncate(MAX_WS_RECORDED_PAYLOAD_BYTES);
            }
            let message_record = WebSocketMessageRecord {
                id: uuid::Uuid::new_v4(),
                connection_id,
                timestamp: now,
                direction: ws_direction,
                message_type: message_type.clone(),
                payload: recorded_payload,
                payload_size,
            };

            {
                // SQLite write is blocking and runs per message; keep it off the
                // async runtime. Awaited so messages persist in arrival order.
                let app_handle = self.app_handle.clone();
                let record = message_record.clone();
                let saved = halo_core::tauri::async_runtime::spawn_blocking(move || {
                    if let Some(history) = app_handle.try_state::<halo_storage::HistoryBridge>() {
                        if let Err(e) = history.insert_websocket_message(&record) {
                            eprintln!("[websocket] failed to save message: {}", e);
                        }
                    }
                });
                let _ = saved.await;
            }

            if let Err(e) = self.app_handle.emit("websocket-message", &message_record) {
                eprintln!("[websocket] failed to emit message event: {}", e);
            }

            crate::automation::ingest_websocket_message(
                &self.app_handle,
                &message_record,
                &host,
                &path,
                &url,
            );

            if matches!(&msg, Message::Close(_)) {
                self.ws_connections.lock().remove(&key);
                eprintln!("[websocket] connection closed conn_id={}", connection_id);
            }
        } else {
            eprintln!(
                "[websocket] no connection mapping found for key={} client={} uri={}",
                key, client_addr, uri_str
            );
        }

        Some(msg)
    }

    async fn on_close(&self, ctx: &mut HttpContext) {
        let client_addr = ctx.client_addr.to_string();
        let uri_str = ctx.host.clone();
        let empty_headers = HashMap::new();
        let (host, path, _) = websocket::parse_websocket_target(&uri_str, &empty_headers);
        let key = format!("{}|{}|{}", client_addr, host, path);
        if let Some(conn_id) = self.ws_connections.lock().remove(&key) {
            eprintln!(
                "[websocket] connection closed via on_close conn_id={}",
                conn_id
            );
        }
    }
}
