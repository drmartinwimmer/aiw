# AI Workspace Launcher (`aiw`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use dev:subagent-driven-development (recommended) or dev:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a small Rust binary utility `aiw` that manages Jujutsu workspaces under `.workspaces/<workspace-name>` and launches the `agy` coding agent inside an unprivileged Bubblewrap sandbox with project-configured developer tools and Nix support.

**Architecture:** A modular CLI application with dedicated modules for Jujutsu workspace management, `aiw.json` configuration parsing, and Bubblewrap sandbox command generation/execution. All modules are tested through unit tests and hermetic integration tests on temporary directories.

**Tech Stack:** Rust (edition 2024), `clap` (derive parser), `serde` & `serde_json` (config parsing), `which` (tool discovery), `tempfile` (hermetic test harness).

---

## Milestones

### Milestone 1: Specifications, Config Loader & Workspace Management
- **Description:** Establish project and module specifications, set up crate dependencies, implement `aiw.json` configuration loading, and build the Jujutsu workspace detection and creation logic.
- **Status:** `[ ] Pending`
- **Target Completion Date:** 2026-09-13
- **Actual Completion Date:** -
- **Dependencies:** None
- **Tasks File:** [M1.md](file:///home/martin/dev/rust/aiw/plan/2026-09-13-aiw-launcher/M1.md)
- **Feedback File:** [FEEDBACK_M1.md](file:///home/martin/dev/rust/aiw/plan/2026-09-13-aiw-launcher/FEEDBACK_M1.md)

### Milestone 2: Sandbox Engine, CLI Entrypoint & Hermetic Integration Tests
- **Description:** Implement tool resolution, Nix compatibility, Bubblewrap argument generation and execution, CLI argument parsing, and end-to-end hermetic integration tests.
- **Status:** `[ ] Pending`
- **Target Completion Date:** 2026-09-13
- **Actual Completion Date:** -
- **Dependencies:** Milestone 1
- **Tasks File:** [M2.md](file:///home/martin/dev/rust/aiw/plan/2026-09-13-aiw-launcher/M2.md)
- **Feedback File:** [FEEDBACK_M2.md](file:///home/martin/dev/rust/aiw/plan/2026-09-13-aiw-launcher/FEEDBACK_M2.md)
