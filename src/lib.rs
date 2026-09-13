//! `aiw` library crate.
//!
//! Provides configuration parsing, Jujutsu workspace management,
//! and Bubblewrap sandbox execution for AI developer workflows.

pub mod config;
pub use config::{AiwConfig, ConfigError};
pub mod workspace;
pub use workspace::{WorkspaceError, ensure_workspace, find_jj_root};
pub mod sandbox;
pub use sandbox::{SandboxBuilder, SandboxConfig, SandboxError};
