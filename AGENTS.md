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

## 3. Commit Messages & Conventional Commits

- Do NOT manually bump crate versions in `Cargo.toml` or manually edit `CHANGELOG.md`. Releases, SemVer version bumps, and changelog updates are automated via Release Please.
- Always write commit messages adhering to the [Conventional Commits](https://www.conventionalcommits.org/) specification so Release Please can compute the correct SemVer release:
  - `fix: ...` for bug fixes (triggers patch release).
  - `feat: ...` for new features or CLI additions (triggers minor release).
  - `feat!: ...` or `fix!: ...` (or `BREAKING CHANGE:` in description) for breaking changes (triggers major release).
  - `docs: ...`, `refactor: ...`, `chore: ...`, `test: ...`, `ci: ...` for maintenance and internal changes.

## 4. Pull Requests & Merging

- Always use **Squash and merge** (or **Rebase and merge**) when merging pull requests into `main`. Never use "Create a merge commit". Standard merge commits copy the branch commit message into the merge commit body, which causes Release Please to parse both commits and generate duplicate entries in `CHANGELOG.md` and release PRs.
- Ensure the Pull Request title adheres to the Conventional Commits specification, as GitHub defaults to using the PR title as the squashed commit subject on `main`.
