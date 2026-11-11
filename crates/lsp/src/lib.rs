//! Stdio LSP client for Halo Coder.
//!
//! Owns process lifecycle and JSON-RPC framing. Hosts (the bridge) implement
//! editor UI adapters on top of [`LanguageClient`].

pub mod client;
mod registry;
mod stdio;

pub use client::{
    DiagnosticsHandler, LanguageClient, LanguageServerSpec, LspError, LspResult, ProgressHandler,
    apply_text_edits, path_to_uri, uri_to_path,
};
pub use registry::{
    LanguageServerCatalogEntry, bundled_bin_dirs, bundled_lsp_dir, catalog, catalog_entry,
    install_server, is_command_available, is_registered, path_with_bundled_bins,
    rust_toolchain_missing, server_for_language, server_id_for_language, which,
};
pub use stdio::{FrameError, encode_frame, read_frame, write_frame};
