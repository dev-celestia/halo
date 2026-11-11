//! Halo core foundation: filesystem security, app data paths,
//! in-process event broker (`AppHandle` / `State` / `Emitter`), and core types.

pub mod automation;
pub mod fs_secure;
pub mod paths;
pub mod tauri;
pub mod types;

pub use fs_secure::{create_private_dir, harden_file, harden_file_if_exists, set_mode};
pub use paths::{get_shared_app_dir, get_shared_db_path, harden_app_data_permissions};
pub use tauri::{AppHandle, Emitter, Manager, State};
pub use types::*;
