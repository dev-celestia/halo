//! Doctor probes — one function per dependency, each returning a [`DoctorItem`].
//!
//! Split out of `doctor.rs` so both halves stay under the 1k-line budget: the
//! reactive view and the action dispatch live there, the environment inspection
//! lives here.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::doctor::{DoctorCategory, DoctorItem, DoctorStatus};
use super::doctor_actions;
use crate::backend::Backend;

mod app;
mod ca;
mod dependencies;
mod desktop;
mod orphans;
mod runtimes;
mod storage;
#[cfg(test)]
mod tests;

pub(crate) use app::*;
pub(crate) use ca::*;
pub(crate) use dependencies::*;
pub(crate) use desktop::*;
pub(crate) use orphans::*;
pub(crate) use runtimes::*;
pub(crate) use storage::*;
