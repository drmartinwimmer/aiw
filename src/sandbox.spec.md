# Module Specification: `sandbox` (`src/sandbox.rs`)

## 1. Module Purpose
The `sandbox` module is responsible for:
1. Resolving the host paths for `agy` and all developer tools configured in `aiw.json`.
2. Discovering relevant system directories (including `/nix`, Nix profiles, SSL certificates, dynamic loader libraries, `/etc` files, and the Nix daemon socket).
3. Constructing the Bubblewrap (`bwrap`) command arguments and environment variables.
4. Spawning the sandboxed process.

## 2. Public API / Contracts
```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Required host tool '{0}' could not be found in PATH")]
    ToolNotFound(String),
    #[error("Bubblewrap executable 'bwrap' not found in PATH")]
    BubblewrapNotFound,
    #[error("Failed to resolve symlink for path {0}: {1}")]
    SymlinkResolutionFailed(PathBuf, std::io::Error),
    #[error("I/O error during sandbox execution: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct SandboxConfig<'a> {
    pub repo_root: &'a Path,
    pub workspace_path: &'a Path,
    pub tools: &'a [String],
    pub extra_args: &'a [String],
    pub home_dir: Option<&'a Path>,
}

#[derive(Debug)]
pub struct SandboxBuilder<'a> {
    config: SandboxConfig<'a>,
}

impl<'a> SandboxBuilder<'a> {
    pub fn new(config: SandboxConfig<'a>) -> Self;

    /// Resolves required tools and system paths, returning the list of bwrap CLI arguments.
    pub fn build_args(&self) -> Result<Vec<String>, SandboxError>;

    /// Builds the std::process::Command prepared to execute bwrap with proper env and args.
    pub fn build_command(&self) -> Result<Command, SandboxError>;

    /// Executes the container synchronously, forwarding standard I/O and returning its ExitStatus.
    pub fn run(&self) -> Result<ExitStatus, SandboxError>;
}
```

## 3. Invariants
- **Fail-Fast Resolution:** If `bwrap` or any tool in `config.tools` is not found, `SandboxError` is returned immediately before executing any command.
- **Strict Read-Only System Mounts:** System directories (`/nix`, `/lib`, `/usr`, etc.) are mounted strictly with `--ro-bind`.
- **Targeted Read-Write Bind Mounts:** Read-write access is granted strictly to:
  - `<workspace_path>`
  - `<repo_root>/.jj`
  - `<home>/.gemini` (if present, or created in home)
  - `/tmp` (ephemeral `tmpfs`)
  - `/nix/var/nix/daemon-socket` (if present, to communicate with Nix daemon)
- **Networking & Process Tree:** Always includes `--die-with-parent`, `--share-net`, `--dev /dev`, `--proc /proc`.
- **Working Directory:** Set to `--chdir <workspace_path>`.
- **Tool Directories in PATH:** All resolved tool directories (and standard `/bin`, `/usr/bin`) are joined into `PATH`.

## 4. Verification Plan
- Unit test: Resolves tools present in `$PATH` (e.g. `cargo`, `rustc`).
- Unit test: Returns `SandboxError::ToolNotFound` when a configured tool does not exist.
- Unit test: Generated `build_args` includes required flags (`--die-with-parent`, `--proc /proc`, `--dev /dev`, `--tmpfs /tmp`, `--share-net`, `--chdir`, `--ro-bind`, `--bind`).
- Unit test: Verifies `/nix` is bound if `/nix` exists on host.
- Unit test: Verifies `<workspace>` and `<repo_root>/.jj` are bound read-write.
