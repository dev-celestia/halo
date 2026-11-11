//! Parse Postman Collection v2.0/v2.1 JSON, environment exports, and bundled
//! `{ collections, environments }` dumps into a tree the Repeater can save.
//!
//! IDs are assigned later, on save, so importing the same file twice adds a
//! second copy instead of overwriting the first.

use base64::Engine as _;
use serde_json::{Map, Value};

mod auth;
mod body;
mod parse;
mod scripts;
#[cfg(test)]
mod tests;
mod types;
mod url;
mod variables;

pub(crate) use auth::*;
pub(crate) use body::*;
pub(crate) use parse::*;
pub(crate) use scripts::*;
pub(crate) use types::*;
pub(crate) use url::*;
pub(crate) use variables::*;
