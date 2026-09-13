# Global Specification: `aiw` (AI Workspace Launcher)

## 1. System Overview
`aiw` is a lightweight Rust binary utility that automates running coding agent CLIs (specifically Google Antigravity `agy`) inside a secure, unprivileged Bubblewrap (`bwrap`) container. It ensures development tasks occur within dedicated Jujutsu (`jj`) workspaces under `.workspaces/<workspace-name>` and provides the coding agent with necessary developer tools specified in a project-level `aiw.json` file, full networking, and support for Nix/NixOS environments.

## 2. System Invariants
1. **Repository Boundary:** `aiw` must only execute within a valid Jujutsu repository. If invoked outside a Jujutsu repository, it must immediately abort with exit code 1 and a descriptive error message without performing any side-effects.
2. **Workspace Containment:** All workspaces managed by `aiw` must reside exclusively in `<repo_root>/.workspaces/<workspace-name>`.
3. **Workspace Idempotence:** If `<repo_root>/.workspaces/<workspace-name>` already exists, `aiw` must reuse it without re-running `jj workspace add` or erroring. If it does not exist, `aiw` must create it via `jj workspace add`.
4. **Sandbox Non-Destructiveness:** The Bubblewrap sandbox must mount all host filesystems read-only except:
   - `<repo_root>/.workspaces/<workspace-name>` (read-write)
   - `<repo_root>/.jj` (read-write, required by Jujutsu to record working copy revisions)
   - `<home>/.gemini` (read-write, required by `agy` for authentication, session transcripts, and skills)
   - `/tmp` (ephemeral `tmpfs`)
   - The Nix daemon socket `/nix/var/nix/daemon-socket` (read-write if present, to communicate with nix-daemon)
5. **Fail-Fast Tool Validation:** If any tool configured in `aiw.json` or required host binaries (`bwrap`, `jj`, `agy`) cannot be found on the host system, execution must abort with an informative error before any sandbox is spawned.
6. **Zero Silent Failures / Panic Freedom:** No `unwrap()` or `expect()` calls in production code paths. All errors must be handled gracefully through typed errors implementing `std::error::Error` and displayed cleanly to the user.

## 3. Core Concepts & Terminology
- **Jujutsu Root (`repo_root`):** The top-level root directory of the parent Jujutsu repository containing the `.jj` directory.
- **Workspace Directory:** `<repo_root>/.workspaces/<workspace-name>`, a separate working copy linked to the parent Jujutsu repository database.
- **Project Configuration (`aiw.json`):** A JSON file located at `<repo_root>/aiw.json` specifying the required host tools (e.g. `{"tools": ["cargo", "rustc", "git", "jj"]}`).
- **Bubblewrap Sandbox:** An unprivileged Linux user/mount namespace container launched via `bwrap` with `--die-with-parent` and `--share-net`.
- **Target Command (`agy`):** The coding agent command invoked inside the container with working directory set to `<repo_root>/.workspaces/<workspace-name>`.

## 4. Cross-Cutting Concerns
- **Error Handling:** Centralized typed errors using custom error enums with formatted output via `thiserror` (or `std::fmt::Display`).
- **Nix Compatibility:** Detects `/nix` directory on host; if present, mounts `/nix` read-only and `/nix/var/nix/daemon-socket` (if exists), preserving Nix-specific environment variables (`NIX_SSL_CERT_FILE`, `NIX_PATH`).
- **Testing:** Hermetic integration tests using `tempfile` operating on real temporary directories with live Jujutsu commands.
