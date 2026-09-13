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
