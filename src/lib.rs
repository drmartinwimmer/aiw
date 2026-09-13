//! `aiw` library crate.
//!
//! Provides configuration parsing, Jujutsu workspace management,
//! and Bubblewrap sandbox execution for AI developer workflows.

pub mod config;
pub use config::{AiwConfig, ConfigError};
pub mod workspace;
pub use workspace::{ensure_workspace, find_jj_root, WorkspaceError};
