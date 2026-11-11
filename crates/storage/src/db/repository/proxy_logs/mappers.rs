//! Row mappers and SQL helpers shared by the proxy-log queries.

use super::*;

pub(crate) fn where_sql_push_close(sql: &mut String) {
    sql.push(')');
}

pub(crate) fn row_to_proxy_record(
    row: &rusqlite::Row,
    payload_store: Option<&PayloadStore>,
) -> SqlResult<ProxyRecord> {
    let id: String = row.get(0)?;
    let timestamp: String = row.get(1)?;
    let method: String = row.get(2)?;
    let url: String = row.get(3)?;
    let request_headers: Option<String> = row.get(4)?;
    let legacy_request_body: Option<Vec<u8>> = row.get(5)?;
    let response_status: Option<i64> = row.get(6)?;
    let response_status_text: Option<String> = row.get(7)?;
    let response_headers: Option<String> = row.get(8)?;
    let legacy_response_body: Option<Vec<u8>> = row.get(9)?;
    let client_addr: Option<String> = row.get(10)?;
    let server_addr: Option<String> = row.get(11)?;
    let req_payload_ref: String = row.get(12).unwrap_or_default();
    let res_payload_ref: String = row.get(13).unwrap_or_default();

    // Resolve request body: try payload_store first, fall back to legacy BLOB
    let request_body_bytes = if !req_payload_ref.is_empty() {
        if let Some(ps) = payload_store {
            ps.load_body(&req_payload_ref).unwrap_or(None)
        } else {
            None
        }
    } else {
        legacy_request_body
    };

    // Resolve response body: try payload_store first, fall back to legacy BLOB
    let response_body_bytes = if !res_payload_ref.is_empty() {
        if let Some(ps) = payload_store {
            ps.load_body(&res_payload_ref).unwrap_or(None)
        } else {
            None
        }
    } else {
        legacy_response_body
    };

    let mut request = ProxyRequest {
        method,
        uri: url,
        http_version: String::from("HTTP/1.1"),
        headers: request_headers
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .unwrap_or_default()
            .unwrap_or_default(),
        body: request_body_bytes.unwrap_or_default(),
        content_decoded: false,
    };

    if !request.content_decoded
        && !request.body.is_empty()
        && request
            .headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("content-encoding"))
    {
        request.content_decoded = true;
    }

    let mut response = response_status.map(|status| ProxyResponse {
        status_code: status as u16,
        status_text: response_status_text.unwrap_or_default(),
        http_version: String::from("HTTP/1.1"),
        headers: response_headers
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .unwrap_or_default()
            .unwrap_or_default(),
        body: response_body_bytes.unwrap_or_default(),
        content_decoded: false,
    });

    if let Some(ref mut resp) = response {
        if !resp.content_decoded
            && !resp.body.is_empty()
            && resp
                .headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("content-encoding"))
        {
            resp.content_decoded = true;
        }
    }

    Ok(ProxyRecord {
        id: Uuid::parse_str(&id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|_| rusqlite::Error::InvalidQuery)?
            .with_timezone(&chrono::Utc),
        request,
        response,
        client_addr: client_addr.unwrap_or_default(),
        server_addr: server_addr.unwrap_or_default(),
    })
}

pub(crate) fn row_to_proxy_summary(row: &rusqlite::Row) -> SqlResult<ProxySummaryRow> {
    let session_id: String = row.get(1).unwrap_or_default();
    let req_headers_raw: Option<String> = row.get(10).ok();
    let res_headers_str: Option<String> = row.get(11).ok();

    let mut user_agent = None;
    let mut referrer = None;
    let mut host = None;

    if let Some(ref h_str) = req_headers_raw {
        if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(h_str) {
            for (k, v) in map {
                let lower = k.to_lowercase();
                if host.is_none()
                    && (lower == "host" || lower == ":authority" || lower == "x-forwarded-host")
                {
                    let trimmed = v.trim();
                    if !trimmed.is_empty() {
                        host = Some(trimmed.to_string());
                    }
                }
                if user_agent.is_none() && lower == "user-agent" {
                    let trimmed = v.trim();
                    if !trimmed.is_empty() {
                        user_agent = Some(trimmed.to_string());
                    }
                }
                if referrer.is_none() && (lower == "referer" || lower == "referrer") {
                    let trimmed = v.trim();
                    if !trimmed.is_empty() {
                        referrer = Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    let response_content_type = res_headers_str.as_ref().and_then(|h_str| {
        if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(h_str) {
            map.into_iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v)
        } else {
            None
        }
    });

    Ok(ProxySummaryRow {
        id: row.get(0)?,
        session_id,
        timestamp: row.get(2)?,
        method: row.get(3)?,
        url: row.get(4)?,
        response_status: row.get::<_, Option<i64>>(5)?.map(|v| v as u16),
        response_status_text: row.get(6)?,
        request_body_size: row.get::<_, i64>(7)? as usize,
        response_body_size: row.get::<_, i64>(8)? as usize,
        server_addr: row.get(9)?,
        user_agent,
        host,
        response_content_type,
    })
}

pub(crate) fn collect_records<T, I>(rows: I) -> Vec<T>
where
    I: IntoIterator<Item = SqlResult<T>>,
{
    let mut records = Vec::new();

    for row in rows {
        match row {
            Ok(record) => records.push(record),
            Err(err) => eprintln!("[db] skipping malformed http_logs row: {}", err),
        }
    }

    records
}
