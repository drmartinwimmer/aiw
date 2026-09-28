# Agent Guidelines

## 1. Version Control via Jujutsu (`jj`)

- Do NOT use `git` commands.
- Use `jj` for all repository operations.
- Always use `--no-pager` for commands displaying output (e.g., `jj --no-pager status`, `jj --no-pager diff`, `jj --no-pager log`).
- Set commit messages with `jj --no-pager describe -m "..."`.
- Start new logical changes with `jj new`.

## 2. Coding & Cleanliness

- Maintain documentation and existing comments unless requested otherwise.
- Keep the codebase lightweight without unnecessary dependencies.
- Follow Rust best practices and maintain clean compiler/clippy runs.

## 3. Versioning & Changelog

- Increment the crate version in `Cargo.toml` (according to Rust SemVer rules) based on changes compared to `main`:
  - Increment patch version for backwards-compatible bug fixes, refactoring, and maintenance.
  - Increment minor version for new backwards-compatible functionality or CLI additions.
  - Increment major version for breaking public API or CLI behavior changes.
- Maintain a changelog (`CHANGELOG.md`) that summarizes the changes between versions.
