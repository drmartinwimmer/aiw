//! `aiw` library crate.
//!
//! Provides configuration parsing, Jujutsu workspace management,
//! and Fence sandbox execution for AI developer workflows.

pub mod config;
pub use config::{AiwConfig, ConfigError};
pub mod workspace;
pub use workspace::{
    WorkspaceError, ensure_workspace, find_jj_root, forget_workspace, workspace_exists,
};
pub mod sandbox;
pub use sandbox::{
    SandboxBuilder, SandboxConfig, SandboxError, allow_direnv, allow_direnv_if_repo_root_allowed,
    is_direnv_allowed,
};
