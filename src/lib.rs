//! `aiw` library crate.
//!
//! Provides configuration parsing, Jujutsu workspace management,
//! and Fence sandbox execution for AI developer workflows.

pub mod config;
pub use config::{AiwConfig, ConfigError};
pub mod workspace;
pub use workspace::{Workspace, WorkspaceError, find_jj_root};
pub mod direnv;
pub use direnv::Direnv;
pub mod sandbox;
pub use sandbox::{SandboxBuilder, SandboxError};
