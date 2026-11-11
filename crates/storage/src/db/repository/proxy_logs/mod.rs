use crate::db::payload_store::PayloadStore;
use halo_core::types::{ProxyFilter, ProxyRecord, ProxyRequest, ProxyResponse};
use rusqlite::{Result as SqlResult, params};
use std::cmp::Reverse;
use uuid::Uuid;

use super::Database;
use super::types::{ProxySummaryRow, TreeNode, TreePath};

fn build_scope_sql_clause(
    scope: &[String],
    params_vec: &mut Vec<Box<dyn rusqlite::ToSql>>,
) -> Option<String> {
    let mut clause_parts: Vec<String> = Vec::new();
    for pattern in scope {
        let value = pattern.trim();
        if value.is_empty() {
            continue;
        }

        let clean = value
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_start_matches("*.")
            .trim_end_matches('/');

        if clean.is_empty() {
            continue;
        }

        let domain_no_port = clean.split(':').next().unwrap_or(clean);

        clause_parts.push(
            "(url LIKE ? OR url LIKE ? OR server_addr LIKE ? OR server_addr LIKE ?)".to_string(),
        );
        params_vec.push(Box::new(format!("%{clean}%")));
        params_vec.push(Box::new(format!("%{domain_no_port}%")));
        params_vec.push(Box::new(format!("%{clean}%")));
        params_vec.push(Box::new(format!("%{domain_no_port}%")));
    }

    if clause_parts.is_empty() {
        None
    } else {
        Some(format!(" AND ({})", clause_parts.join(" OR ")))
    }
}

const SELECT_PROXY_RECORD_COLS: &str = "id, timestamp, method, url, request_headers, request_body, response_status, response_status_text, response_headers, response_body, client_addr, server_addr, req_payload_ref, res_payload_ref";

fn parse_seg_id_from_ref(reference: &str) -> Option<u64> {
    let rest = reference.strip_prefix("seg:")?;
    let mut parts = rest.split(':');
    let _session_id = parts.next()?;
    parts.next()?.parse::<u64>().ok()
}

mod mappers;
mod queries;
mod summaries;
mod writes;
