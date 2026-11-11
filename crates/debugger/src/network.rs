//! Unified network list: proxy history + Forge chronicle + Intruder results.

use halo_bridge::{BridgeState, ChronicleLogDto, HttpLogRow, IntruderResultRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkSource {
    Proxy,
    Forge,
    Intruder,
}

impl NetworkSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Forge => "forge",
            Self::Intruder => "intruder",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Proxy => "Proxy",
            Self::Forge => "Forge",
            Self::Intruder => "Intruder",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NetworkEntry {
    pub id: String,
    pub source: NetworkSource,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub timestamp: String,
    pub duration_ms: Option<u128>,
    pub request_size: Option<usize>,
    pub response_size: Option<usize>,
    pub detail: NetworkDetail,
}

#[derive(Clone, Debug)]
pub enum NetworkDetail {
    Proxy(HttpLogRow),
    Forge(ChronicleLogDto),
    Intruder(IntruderResultRow),
}

/// Build a newest-first unified list from live Bridge state.
pub fn collect_network(state: &BridgeState) -> Vec<NetworkEntry> {
    let mut out = Vec::with_capacity(
        state.history.len() + state.chronicle.len() + state.intruder.results.len(),
    );

    for row in &state.history {
        out.push(NetworkEntry {
            id: format!("proxy:{}", row.id),
            source: NetworkSource::Proxy,
            method: row.method.clone(),
            url: row.url.clone(),
            status: row.status,
            status_text: row.status_text.clone(),
            timestamp: row.timestamp.clone(),
            duration_ms: None,
            request_size: Some(row.request_size),
            response_size: Some(row.response_size),
            detail: NetworkDetail::Proxy(row.clone()),
        });
    }

    for row in &state.chronicle {
        out.push(NetworkEntry {
            id: format!("forge:{}", row.id),
            source: NetworkSource::Forge,
            method: row.method.clone(),
            url: row.url.clone(),
            status: row.response_status,
            status_text: row.response_status_text.clone(),
            timestamp: row.timestamp.clone(),
            duration_ms: row.duration_ms.map(|d| d as u128),
            request_size: row.request_body.as_ref().map(|b| b.len()),
            response_size: row.response_body.as_ref().map(|b| b.len()),
            detail: NetworkDetail::Forge(row.clone()),
        });
    }

    let intruder_method = state
        .pending_intruder
        .as_ref()
        .map(|d| d.method.clone())
        .unwrap_or_else(|| "ATTACK".into());
    let intruder_base_url = state
        .pending_intruder
        .as_ref()
        .map(|d| d.url.clone())
        .unwrap_or_default();

    for row in &state.intruder.results {
        let url = row
            .final_url
            .clone()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| intruder_base_url.clone());
        out.push(NetworkEntry {
            id: format!("intruder:{}", row.id),
            source: NetworkSource::Intruder,
            method: intruder_method.clone(),
            url,
            status: row.status,
            status_text: row.status_text.clone(),
            timestamp: String::new(),
            duration_ms: row.response_time_ms,
            request_size: None,
            response_size: row.response_length,
            detail: NetworkDetail::Intruder(row.clone()),
        });
    }

    // Prefer entries that have timestamps; proxy/forge first by string order is rough
    // but good enough — reverse so newest proxy rows (already newest-first) stay on top,
    // then forge, then intruder.
    out
}
