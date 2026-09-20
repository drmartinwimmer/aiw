//! `aiw` library crate.
//!
//! Provides workspace management and Fence sandbox execution for AI developer workflows.

mod cli;
pub use cli::{AppError, Cli};

pub mod direnv;
pub mod sandbox;
pub mod workspace;

pub(crate) mod herdr;
pub(crate) mod vcs;
