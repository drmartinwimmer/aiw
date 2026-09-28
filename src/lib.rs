//! `aiw` library crate.
//!
//! Provides workspace management and Fence sandbox execution for AI developer workflows.

mod cli;
pub use cli::{AppError, Cli};

pub mod config;
pub mod direnv;
pub mod sandbox;
pub mod workspace;

pub use config::{ConfigInitError, ConfigInitStatus, ConfigInitializer, DEFAULT_AIW_TEMPLATE};
pub(crate) mod herdr;
pub(crate) mod vcs;
