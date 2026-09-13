# Module Specification: `main` (`src/main.rs`)

## 1. Module Purpose
The `main` module is the command-line entrypoint for the `aiw` utility. It parses CLI options with `clap`, coordinates repository discovery, workspace validation/creation, configuration loading, and sandbox execution, and formats user-facing errors cleanly with proper exit codes.

## 2. Public API / CLI Contract
```bash
aiw agy <workspace-name> [--dry-run] [-- <extra-agy-args>...]
```

### CLI Options:
- Subcommand `agy`:
  - `workspace_name` (positional, required): The target workspace name under `.workspaces/<workspace-name>`.
  - `--dry-run` (flag, optional): When specified, prints the generated Bubblewrap command and arguments instead of executing `bwrap`.
  - `extra_args` (trailing args, optional): Forwarded directly to `agy` inside the sandbox.

### Exit Codes:
- `0`: Success (sandbox process exited with 0, or `--dry-run` succeeded).
- `1`: Validation, configuration, repository, or sandbox setup failure.
- `N`: Non-zero exit code propagated from the sandboxed `agy` process.

## 3. Invariants
- **Clean Error Output:** Errors must be printed to `stderr` with a user-friendly message (e.g. `Error: ...`). Stack traces or panics must never be shown to the user on standard failure cases.
- **Fail-Fast:** Execution sequence:
  1. Determine Jujutsu root (`workspace::find_jj_root`).
  2. Ensure workspace exists (`workspace::ensure_workspace`).
  3. Load configuration (`config::AiwConfig::find_and_load`).
  4. Construct sandbox (`sandbox::SandboxBuilder`).
  5. If `--dry-run`, print command line and exit `0`.
  6. Execute `bwrap` and exit with child process exit code.

## 4. Verification Plan
- Hermetic integration test: Invoking `aiw agy <workspace-name> --dry-run` prints the expected `bwrap` invocation and outputs exit code 0.
- Hermetic integration test: Invoking outside a Jujutsu repo prints `Error: Not inside a Jujutsu repository` and exits with code 1.
- Hermetic integration test: Invoking with invalid `aiw.json` prints `Error: Failed to parse JSON...` and exits with code 1.
