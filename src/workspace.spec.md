# Module Specification: `workspace` (`src/workspace.rs`)

## 1. Module Purpose
The `workspace` module manages Jujutsu (`jj`) repository root detection and the idempotent verification and creation of workspaces under `<repo_root>/.workspaces/<workspace-name>`.

## 2. Public API / Contracts
```rust
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("Not inside a Jujutsu repository")]
    NotInJjRepo,
    #[error("Failed to execute Jujutsu command: {0}")]
    JjCommandFailed(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Finds the root directory of the enclosing Jujutsu repository.
/// Scans parent directories for `.jj` or invokes `jj --no-pager root`.
pub fn find_jj_root(start_dir: &Path) -> Result<PathBuf, WorkspaceError>;

/// Ensures a workspace named `workspace_name` exists under `<repo_root>/.workspaces/<workspace-name>`.
/// If it already exists, returns its absolute path.
/// If not, invokes `jj --no-pager workspace add .workspaces/<workspace-name> --name <workspace-name>`.
pub fn ensure_workspace(repo_root: &Path, workspace_name: &str) -> Result<PathBuf, WorkspaceError>;
```

## 3. Invariants
- **No Git Commands:** Only `jj` is used.
- **Global Flag:** Any Jujutsu invocation must pass `--no-pager`.
- **Idempotence:** If `.workspaces/<workspace-name>` already exists, no new workspace is created; the path is returned directly.
- **Path Isolation:** Workspaces are strictly placed in `<repo_root>/.workspaces/<workspace-name>`.

## 4. Verification Plan
- Unit test: `find_jj_root` correctly locates repo root given a path inside the repo or a subfolder.
- Unit test: `find_jj_root` returns `WorkspaceError::NotInJjRepo` when called outside a jj repo.
- Integration test in tempdir:
  - Initialize a new jj repository using `jj init`.
  - Call `ensure_workspace(repo_root, "ws-1")`: verifies `.workspaces/ws-1` directory exists and has a `.jj` reference file.
  - Call `ensure_workspace(repo_root, "ws-1")` a second time: succeeds immediately without running `jj workspace add` again.
