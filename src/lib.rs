//! `aiw` library crate.
//!
//! Provides workspace management and Fence sandbox execution for AI developer workflows.

mod cli;
mod config;
mod direnv;
mod sandbox;
mod workspace;

pub(crate) mod tools;
pub(crate) use tools::herdr;
pub(crate) mod vcs;

pub use cli::{AppError, Cli};
pub use config::{ConfigInitError, ConfigInitializer};
pub use direnv::Direnv;
pub use sandbox::{SandboxBuilder, SandboxError};
pub use workspace::{Workspace, WorkspaceError};
