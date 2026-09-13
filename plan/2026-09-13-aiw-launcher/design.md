# Design Document: AI Workspace Launcher (`aiw`)

## 1. Overview & Goals
`aiw` is a CLI utility written in Rust that streamlines running coding agent CLIs (such as `agy`) inside an isolated, unprivileged Linux sandbox using Bubblewrap (`bwrap`). 

When invoked inside a Jujutsu (`jj`) repository:
1. It verifies whether a workspace exists under `.workspaces/<workspace-name>`. If not, it creates and registers a new Jujutsu workspace using `jj workspace add`.
2. It reads project-specific developer tools configuration from `aiw.json` at the repository root.
3. It constructs and runs a minimal Bubblewrap container in default allow mode with network access, isolating the host filesystem while bind-mounting:
   - The Jujutsu workspace directory (read-write)
   - The parent repository's `.jj` database (read-write, required by Jujutsu for workspace operation)
   - The user's agent directory `~/.gemini` (read-write, for credentials, sessions, and configuration)
   - System directories and Nix store paths (`/nix`, `/lib`, `/usr`, `/etc/ssl`, `/etc/resolv.conf`, `/etc/static`, `/dev`, `/proc`, `/tmp`)
   - The Nix daemon socket (`/nix/var/nix/daemon-socket`) if available
   - The binary executable paths for `agy` and all configured tools in `aiw.json`

## 2. Command-Line Interface
```bash
aiw agy <workspace-name> [extra-agy-args...]
```

### Arguments
- `agy`: The subcommand indicating launching of the `agy` coding agent.
- `<workspace-name>`: The name of the workspace directory inside `.workspaces/`.
- `[extra-agy-args...]`: Optional trailing arguments passed directly to the `agy` invocation inside the container.
- Flags:
  - `--dry-run`: Prints the computed `bwrap` command and environment without executing it.

## 3. Configuration Format (`aiw.json`)
The configuration lives at `<repo_root>/aiw.json`.

Example:
```json
{
  "tools": [
    "cargo",
    "rustc",
    "git",
    "jj"
  ]
}
```

### Schema & Semantics
- `tools`: An array of tool binary names (e.g. `"cargo"`, `"rustc"`, `"git"`, `"jj"`, `"nix"`).
- Each tool is resolved from the host `$PATH`.
- If `aiw.json` does not exist, `aiw` reports an error instructing the user to create `aiw.json` or provides standard fallback tools.

## 4. Architecture & Module Structure

```
src/
├── main.rs          # CLI entrypoint, argument parsing, error reporting
├── lib.rs           # Core library module re-exports
├── config.rs        # Config file loading, deserialization, and validation
├── workspace.rs     # Jujutsu repository detection and workspace creation/verification
└── sandbox.rs       # Tool resolution, Nix environment detection, and bwrap command construction
```

### 4.1 Module: `workspace`
- Finds repository root: searches parent directories for `.jj` or executes `jj --no-pager root`.
- Determines target workspace path: `<repo_root>/.workspaces/<workspace-name>`.
- Checks if the workspace directory exists:
  - If existing and contains `.jj`, reuses it.
  - If missing:
    - Creates `.workspaces/` parent directory if absent.
    - Executes `jj --no-pager workspace add .workspaces/<workspace-name> --name <workspace-name>` in the repository root.
    - Returns an error if Jujutsu command fails.

### 4.2 Module: `config`
- Loads `aiw.json` from `<repo_root>/aiw.json`.
- Deserializes into `AiwConfig { tools: Vec<String> }`.
- Validates that tool names are non-empty.

### 4.3 Module: `sandbox`
- **Tool Resolution:**
  - For `agy` and every tool in `config.tools`:
    - Searches `$PATH` for the executable.
    - Resolves symlinks to identify underlying directories (critical for Nix profiles and symlinked wrappers).
  - Collects all unique directory paths containing tools to include in the container's `$PATH`.
- **Bubblewrap Arguments (`bwrap`):**
  - Process lifecycle: `--die-with-parent`
  - Virtual filesystems: `--proc /proc`, `--dev /dev`, `--tmpfs /tmp`
  - Network: `--share-net`
  - Nix & System Paths (read-only):
    - `--ro-bind /nix /nix` (if `/nix` exists)
    - `--ro-bind /lib /lib`, `--ro-bind /lib64 /lib64`, `--ro-bind /usr /usr` (if they exist)
    - `--ro-bind /etc/resolv.conf /etc/resolv.conf`
    - `--ro-bind /etc/hosts /etc/hosts`
    - `--ro-bind /etc/ssl /etc/ssl` (if exists)
    - `--ro-bind /etc/static /etc/static` (if exists, required for NixOS `/etc` symlinks)
    - `--ro-bind /etc/passwd /etc/passwd`
    - `--ro-bind /etc/group /etc/group`
    - `--ro-bind /etc/nix /etc/nix` (if exists)
    - Host profiles e.g. `/etc/profiles/per-user` (if exists)
  - Nix Daemon Socket:
    - If `/nix/var/nix/daemon-socket` exists, `--bind /nix/var/nix/daemon-socket /nix/var/nix/daemon-socket`
  - Writable Mounts:
    - Workspace: `--bind <workspace_abs_path> <workspace_abs_path>`
    - Repo `.jj` database: `--bind <repo_root>/.jj <repo_root>/.jj`
    - Agent Home data: `--bind <home>/.gemini <home>/.gemini`
  - Working Directory:
    - `--chdir <workspace_abs_path>`
  - Command:
    - Sets container `PATH` containing all resolved tool directories.
    - Preserves/sets `HOME`, `USER`, `SSL_CERT_FILE`, `NIX_SSL_CERT_FILE`.
    - Spawns `agy` with forwarded arguments.

## 5. Invariants & Security
1. **Isolation Boundary:** Host user directories outside `<workspace>`, `<repo_root>/.jj`, and `~/.gemini` are NOT mounted into the sandbox.
2. **Deterministic Workspace Layout:** All workspaces live under `<repo_root>/.workspaces/<workspace-name>`.
3. **Fail-Fast Verification:** If `bwrap`, `jj`, `agy`, or any configured tool cannot be found, `aiw` fails before launching the container.
4. **Clean Error Handling:** No panics on invalid configuration or missing files. All errors return formatted, descriptive messages.

## 6. Testing Strategy
- **Unit Tests:**
  - Configuration deserialization and validation.
  - Bubblewrap argument builder verification (ensuring correct mounts, paths, and environment settings are generated).
- **Hermetic Integration Tests:**
  - Executed using `tempfile::TempDir`.
  - Creates a real Jujutsu repo using `jj init`.
  - Verifies workspace addition and duplicate handling.
  - Verifies config discovery and tool resolution.
  - Tests `aiw` execution in `--dry-run` mode and live `bwrap` execution with lightweight commands when running on Linux with unprivileged user namespaces enabled.
