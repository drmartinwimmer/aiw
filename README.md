# AI Workspace (`aiw`)

> **An easy-to-use, opinionated tool for creating sandboxed AI workspaces.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)](https://www.rust-lang.org/)

**AI Workspace** (`aiw`) streamlines running autonomous AI coding agents (such as Google Antigravity / `agy`) in dedicated, secure, and isolated development environments. It pairs the branchless workflow of [Jujutsu (`jj`)](https://github.com/martinvonz/jj) workspaces with OS-level containment powered by [Fence](https://github.com/fencesandbox/fence), automatic [direnv](https://direnv.net/) environment propagation, and seamless [Herdr](https://github.com/herdr/herdr) multiplexer integration.

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

1. **Jujutsu (`jj`) Workspaces**: Automatically provisions lightweight working copies under `.workspaces/<workspace-name>`, keeping your main working tree pristine.
2. **Fence Sandbox Containment**: Executes the agent inside a sandbox powered by [Fence](https://github.com/fencesandbox/fence)—a lightweight, container-free sandbox tool that enforces network filtering and filesystem boundaries (backed by Bubblewrap and Landlock on Linux, and Seatbelt on macOS).
3. **Safe Agent Autonomy**: Safely runs agents with execution permissions enabled inside the container (`--dangerously-skip-permissions`), providing full autonomous velocity while guaranteeing strict containment at the OS kernel level.
4. **Automated `direnv` Support**: Automatically discovers and authorizes `.envrc` in newly spawned workspaces if the root repository has direnv active and allowed, prepending `direnv exec .` inside the sandbox.
5. **Native `herdr` Integration**: If you run inside a [Herdr](https://github.com/herdr/herdr) terminal multiplexer session, `aiw` automatically creates workspace tabs, focuses them, and reports live agent states.

---

## Prerequisites

- **[Fence](https://github.com/fencesandbox/fence)**: Lightweight command sandbox providing network filtering and filesystem isolation (supports Linux and macOS)
- **[Jujutsu (`jj`)](https://github.com/martinvonz/jj)**: Version control system for workspace management
- **AI Agent CLI**: e.g., `agy` (Antigravity CLI)
- _(Optional)_ **[direnv](https://direnv.net/)**: Directory-based environment variable management
- _(Optional)_ **[Herdr](https://github.com/herdr/herdr)**: Terminal multiplexer with agent status reporting

---

## Installation

### From Source (Cargo)

Ensure you have Rust and Cargo installed (edition 2024 supported):

```bash
git clone https://github.com/<your-username>/aiw.git
cd aiw
cargo install --path .
```

### With Nix / Flakes

If using [Nix](https://nixos.org/):

```bash
# Enter development shell with all dependencies
nix develop

# Build the package
nix build

# Run directly
nix run github:<your-username>/aiw -- agy my-workspace --dry-run
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

- Checks for an existing workspace named `feature-login` or creates `.workspaces/feature-login` via `jj workspace add`.
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

This cleans up the Jujutsu workspace registration (`jj workspace forget`) and removes `.workspaces/<workspace-name>` from disk.

### 5. Initialize Configuration

To initialize starter configuration files:

```bash
aiw config init
```

This command:

- Initializes `~/.config/aiw/aiw.jsonc` (the user-level configuration template) if it does not exist yet.
- Initializes `fence.jsonc` in the current project root (extending your user `aiw.jsonc`) if it does not exist yet.

Pass `--force` to overwrite existing files, `--user-only` to only initialize the user template, or `--project-only` to only initialize `fence.jsonc`.

---

## Configuration

`aiw` uses a clean, two-layer configuration model powered by [Fence](https://github.com/fencesandbox/fence):

### 1. Global AI Workspace Template (`aiw.jsonc`)

The base template defines rules common to all `aiw` workspaces (inheriting from Fence's built-in `code` template). It resides in your user configuration directory at `~/.config/aiw/aiw.jsonc` (or system directory `/etc/aiw/aiw.jsonc` / Nix package shares):

```bash
aiw config init --user-only
```

_(When installing via `cargo install --path .` or the Nix flake, the template is automatically placed in the appropriate config directory and auto-discovered.)_

- **Zero-Config Workspaces**: When no `fence.jsonc` or `fence.json` exists in the local workspace or repository root, `aiw` automatically discovers and defaults to `aiw.jsonc` in `~/.config/aiw/` (or system directories) and passes it to Fence.
- **Hardened Agent Scope**: Extends Fence's baseline `code` template while explicitly restricting network access to essential Google agent and authentication endpoints (`accounts.google.com`, `aicode.googleapis.com`, `aiplatform.googleapis.com`, `cloudcode-pa.googleapis.com`, `daily-cloudcode-pa.googleapis.com`, `oauth2.googleapis.com`) without opening broad wildcards.
- **Jujutsu & Git Workspace Paths**: Grants write access to `.jj/**` and `.git/**` in the active workspace, as well as `../../.jj/**` and `../../.git/**` at the repository root where Jujutsu/Git stores live.
- **Credential & State Persistence**: Preserves Antigravity agent transcripts and cache in `~/.gemini/**` and keyring credentials in `~/.local/share/keyrings/**`.

### 2. Project-Specific Overrides (`fence.jsonc`)

If a repository requires project-specific settings (such as exposing an SDK or custom read/write directory), place `fence.jsonc` at your repository root:

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/fencesandbox/fence/main/docs/schema/fence.schema.json",
  "extends": "./templates/aiw.jsonc",
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
