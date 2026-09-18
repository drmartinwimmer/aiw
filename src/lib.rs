//! `aiw` library crate.
//!
//! Provides configuration parsing, Jujutsu workspace management,
//! and Fence sandbox execution for AI developer workflows.

pub mod config;
pub use config::{AiwConfig, ConfigError};
pub mod workspace;
pub use workspace::{
    Workspace, WorkspaceError, ensure_workspace, find_jj_root, forget_workspace, workspace_exists,
};
pub mod direnv;
pub use direnv::{
    Direnv, allow_direnv, allow_direnv_if_repo_root_allowed, ensure_user_profile_bin_paths,
    has_envrc, is_direnv_allowed,
};
pub mod sandbox;
pub use sandbox::{
    SandboxBuilder, SandboxConfig, SandboxError, format_command,
};
