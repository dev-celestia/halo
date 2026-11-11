//! Halo storage engine: SQLite database, repositories,
//! chunked payload store, and history bridge.

pub mod db;
pub mod history;

pub use db::payload_store::PayloadStore;
pub use db::promotion::promote_session;
pub use db::repository::{
    ChronicleLogRecord, CollectionRecord, ContextRecord, Database, DocumentRecord,
    HaloAgentMessageRecord, HaloAgentSessionRecord, NoteRecord, PaginatedResponse,
    StashEndpointRecord, StashRecord, TreeNode, TreePath,
};
pub use history::{
    HistoryBridge, ProxyLogSummary, WebSocketConnectionDetail, WebSocketConnectionSummary,
};
