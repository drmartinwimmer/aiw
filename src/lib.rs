//! `aiw` library crate.
//!
//! Provides configuration parsing, Jujutsu and Git workspace management,
//! and Fence sandbox execution for AI developer workflows.

pub mod config;
pub use config::{AiwConfig, ConfigError};
pub mod vcs;
pub use vcs::{VcsType, detect as detect_vcs, git, jj};
pub mod workspace;
pub use workspace::{Workspace, WorkspaceError, find_git_root, find_jj_root, find_root};
pub mod direnv;
pub use direnv::Direnv;
pub mod sandbox;
pub use sandbox::{SandboxBuilder, SandboxError};
pub mod herdr;
pub use herdr::{Herdr, HerdrError};
