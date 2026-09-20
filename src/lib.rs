//! `aiw` library crate.
//!
//! Provides workspace management and Fence sandbox execution for AI developer workflows.

mod cli;
pub use cli::{AppError, run};

pub mod direnv;
pub mod sandbox;
pub mod workspace;

pub(crate) mod config;
pub(crate) mod herdr;
pub(crate) mod vcs;
