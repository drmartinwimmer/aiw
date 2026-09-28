# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-09-28

### Added

- GitHub Actions CI workflow (`.github/workflows/ci.yml`) running formatting checks, Clippy lints, crates.io packaging dry-run, unit/integration tests with Fence & Jujutsu, and Nix flake check/build.
- GitHub Actions Release workflow (`.github/workflows/release.yml`) for automated publishing to crates.io on tag pushes (`v*`) and manual workflow dispatch.
- CI, Crates.io, and Docs.rs status badges to `README.md`.
- Repository, homepage, and documentation URLs in `Cargo.toml`.
- Added `/fence-seccomp` to `.gitignore` to prevent Fence runtime sandbox artifacts from polluting the workspace.

## [0.1.1] - 2026-09-28

### Documentation

- Documented Git repository and worktree support in `README.md` alongside Jujutsu (`jj`).

## [0.1.0] - 2026-09-28

### Added

- Initial release of AI Workspace (`aiw`).
- Automatic Jujutsu (`jj`) and Git workspace provisioning under `.workspaces/<workspace-name>`.
- Fence sandbox isolation with restricted filesystem and network permissions for autonomous agent runs.
- Automatic `direnv` environment variable propagation and workspace authorization.
- Herdr terminal multiplexer integration with automated workspace tabs and agent state reporting.
- Two-layer configuration system supporting shared base templates and project-specific `fence.jsonc` overrides.
- `aiw config init` command for initializing project and user configuration files.
