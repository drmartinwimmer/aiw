# AI Workspace (`aiw`)

> **An easy-to-use, opinionated tool for creating sandboxed AI workspaces.**

[![CI](https://github.com/drmartinwimmer/aiw/actions/workflows/ci.yml/badge.svg)](https://github.com/drmartinwimmer/aiw/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/ai-workspace.svg)](https://crates.io/crates/ai-workspace)
[![Docs.rs](https://docs.rs/ai-workspace/badge.svg)](https://docs.rs/ai-workspace)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)

**AI Workspace** (`aiw`) streamlines running autonomous AI coding agents (such as Google Antigravity / `agy`) in dedicated, secure, and isolated development environments. It pairs the isolated workflow of [Jujutsu (`jj`)](https://github.com/martinvonz/jj) workspaces and [Git](https://git-scm.com/) worktrees with OS-level containment powered by [Fence](https://github.com/fencesandbox/fence), automatic [direnv](https://direnv.net/) environment propagation, and seamless [Herdr](https://github.com/herdr/herdr) multiplexer integration.

---

> [!IMPORTANT]
> **Disclaimer**: This is a personal project and does not represent the author's current or past employer.

---

## Why AI Workspace?

Running autonomous coding agents directly inside your primary working copy carries risks:

- Agents can accidentally modify uncommitted files or unrelated project modules.
- Overly broad filesystem and network access can lead to unintended changes or data leakage.
- Prompting for every single tool invocation hampers agent autonomy and workflow speed.

`aiw` provides an **opinionated, zero-friction workflow**:

1. **Isolated Workspaces (Jujutsu & Git)**: Automatically provisions lightweight working copies under `.workspaces/<workspace-name>` using `jj workspace add` (for Jujutsu repositories) or `git worktree add` (for Git repositories), keeping your primary working tree pristine.
2. **Fence Sandbox Containment**: Executes the agent inside a sandbox powered by [Fence](https://github.com/fencesandbox/fence)—a lightweight, container-free sandbox tool that enforces network filtering and filesystem boundaries (backed by Bubblewrap and Landlock on Linux, and Seatbelt on macOS).
3. **Safe Agent Autonomy**: Safely runs agents with execution permissions enabled inside the container (`--dangerously-skip-permissions`), providing full autonomous velocity while guaranteeing strict containment at the OS kernel level.
4. **Automated `direnv` Support**: Automatically discovers and authorizes `.envrc` in newly spawned workspaces if the root repository has direnv active and allowed, prepending `direnv exec .` inside the sandbox.
5. **Native `herdr` Integration**: If you run inside a [Herdr](https://github.com/herdr/herdr) terminal multiplexer session, `aiw` automatically creates workspace tabs, focuses them, and reports live agent states.

---

## Prerequisites

- **[Fence](https://github.com/fencesandbox/fence)**: Lightweight command sandbox providing network filtering and filesystem isolation (supports Linux and macOS)
- **[Jujutsu (`jj`)](https://github.com/martinvonz/jj)** or **[Git](https://git-scm.com/)**: Version control system for workspace management (`aiw` automatically detects whether your project uses Jujutsu or Git)
- **AI Agent CLI**: e.g., `agy` (Antigravity CLI)
- _(Optional)_ **[direnv](https://direnv.net/)**: Directory-based environment variable management
- _(Optional)_ **[Herdr](https://github.com/herdr/herdr)**: Terminal multiplexer with agent status reporting

---

## Installation

### From crates.io

```bash
cargo install ai-workspace
aiw config init user
```

### From Source (Cargo)

Ensure you have Rust and Cargo installed (edition 2024 supported):

```bash
git clone https://github.com/drmartinwimmer/aiw.git
cd aiw
cargo install --path .
aiw config init user
```

### With Nix / Flakes

If using [Nix](https://nixos.org/):

```bash
# Enter development shell with all dependencies
nix develop

# Build the package
nix build

# Run directly
nix run github:drmartinwimmer/aiw -- agy my-workspace --dry-run
```

---

## Usage

### 1. Launch an AI Workspace

To create (or resume) an isolated workspace and launch `agy` inside a sandbox:

```bash
aiw agy <workspace-name>
```

For example:

```bash
aiw agy feature-login
```

This command:

- Checks for an existing workspace named `feature-login` or provisions `.workspaces/feature-login` using `jj workspace add` (in Jujutsu repositories) or `git worktree add` (in Git repositories).
- Authorizes and activates `direnv` if active in the repository root.
- Wraps the agent inside `fence` using root or workspace settings.
- Automatically handles tab creation and agent reporting if running inside Herdr.

### 2. Dry Run Mode

To inspect the generated `fence` command without actually executing it, use `--dry-run`:

```bash
aiw agy feature-login --dry-run
```

Output example:

```text
fence --settings /path/to/repo/fence.jsonc -- direnv exec . agy --dangerously-skip-permissions
```

### 3. Forward Arguments to the Agent

Pass additional flags to the underlying agent CLI after `--`:

```bash
aiw agy feature-login -- --model gemini-2.5-pro --verbose
```

### 4. Forget / Clean Up a Workspace

When work in a workspace is completed and merged, forget and delete it:

```bash
aiw forget <workspace-name>
```

This cleans up the workspace registration (`jj workspace forget` for Jujutsu or `git worktree remove` for Git) and removes `.workspaces/<workspace-name>` from disk.

### 5. Initialize Configuration

To initialize starter configuration files:

```bash
# Initialize project fence.jsonc in current repo root (default)
aiw config init
# Or explicitly:
aiw config init project

# Initialize user-level template in ~/.config/aiw/fence.jsonc
aiw config init user
```

This command:

- `aiw config init` (or `aiw config init project`): Creates `fence.jsonc` in the current project root, extending your user `~/.config/aiw/fence.jsonc`.
- `aiw config init user`: Creates `~/.config/aiw/fence.jsonc` with the recommended base template.

Pass `--force` (or `-f`) to overwrite existing files.

---

## Configuration

`aiw` uses a clean, two-layer configuration model powered by [Fence](https://github.com/fencesandbox/fence):

### 1. Global AI Workspace Template (`fence.jsonc`)

The base template defines rules common to all `aiw` workspaces (inheriting from Fence's built-in `code` template). It resides in your user configuration directory at `~/.config/aiw/fence.jsonc` (or system/package share at `<data_dir>/aiw/fence.jsonc`):

```bash
aiw config init user
```

_(When installing via Nix/Home Manager, it is installed in `$out/share/aiw/fence.jsonc` and discovered via `$XDG_DATA_DIRS`. When installing via `cargo install`, run `aiw config init user` to place it in `~/.config/aiw/fence.jsonc`.)_

- **Zero-Config Workspaces**: When no `fence.jsonc` or `fence.json` exists in the local workspace or repository root, `aiw` automatically discovers and defaults to `fence.jsonc` in `~/.config/aiw/fence.jsonc` (or `$XDG_DATA_DIRS/aiw/fence.jsonc`) and passes it to Fence.
- **Hardened Agent Scope**: Extends Fence's baseline `code` template while explicitly restricting network access to essential Google agent and authentication endpoints (`accounts.google.com`, `aicode.googleapis.com`, `aiplatform.googleapis.com`, `cloudcode-pa.googleapis.com`, `daily-cloudcode-pa.googleapis.com`, `oauth2.googleapis.com`) without opening broad wildcards.
- **Jujutsu & Git Workspace Paths**: Grants write access to `.jj/**` and `.git/**` in the active workspace, as well as `../../.jj/**` and `../../.git/**` at the repository root where Jujutsu/Git stores live.
- **Credential & State Persistence**: Preserves Antigravity agent transcripts and cache in `~/.gemini/**` and keyring credentials in `~/.local/share/keyrings/**`.

### 2. Project-Specific Overrides (`fence.jsonc`)

If a repository requires project-specific settings (such as exposing an SDK or custom read/write directory), place `fence.jsonc` at your repository root:

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/fencesandbox/fence/main/docs/schema/fence.schema.json",
  "extends": "~/.config/aiw/fence.jsonc",
  "filesystem": {
    "allowRead": ["/nix"],
  },
}
```

---

## Multiplexer Integration (`Herdr`)

`aiw` features built-in support for the [Herdr](https://github.com/herdr/herdr) terminal multiplexer:

- **Automatic Tab Creation**: When executed outside the workspace tab, `aiw` opens a new tab labeled after the workspace and launches the sandbox inside it.
- **Agent Lifecycle Reporting**: Reports `working` state directly to Herdr via `herdr pane report-agent`.
- **In-Place Execution**: If already inside a tab named after the workspace, `aiw` executes directly in place without creating duplicate tabs.

---

## Contributing

Contributions, bug reports, and suggestions are welcome! Please feel free to open an issue or submit a pull request.

---

## License

This project is licensed under the [MIT License](LICENSE).

---

> [!NOTE]
> **Disclaimer**: This is a personal project and does not represent the author's current or past employer.
