# Agent Guidelines (AGENTS.md)

If you are an AI assistant (like Claude Code or Antigravity) working in this repository, you must read and follow these rules.

## 1. Version Control via Jujutsu (`jj`)

The user uses **Jujutsu (`jj`)** for version control.

- **Do NOT use `git` commands** (e.g., `git commit`, `git add`, `git checkout`).
- Use `jj` for repository operations. Every Jujutsu command displaying output (specifically `log`, `status`, `diff`, and `bookmark` commands) **MUST** use the `--no-pager` global option to prevent terminal interactive hangs (e.g., `jj --no-pager status`, `jj --no-pager diff`, `jj --no-pager log`).
- Jujutsu automatically tracks file changes as you write them. When you want to set a commit message for the active change, use:
  ```bash
  jj --no-pager describe -m "Your descriptive commit message"
  ```
- If you need to start a new logical change, use `jj new`.

## 2. Spec-Driven Development

We prioritize planning and specifications over rapid code generation.

- **Research First:** Carefully analyze requirements, dependencies, and files before drafting changes.
- **Specifications:** A project must have a global `SPEC.md` file (defining invariants and core concepts) and each module must have a corresponding module-level specification (e.g., `<module>.spec.md` for single-file modules or a folder-level `SPEC.md` for directory modules). Follow the `dev:writing-specs` skill.
- **Lifecycle:** Specifications must be written or updated before starting any milestone implementation. Once implemented, a subagent must verify compliance with the specs and add any remediation tasks to the milestone before it can be completed.
- **Compliance Checking:** You **MUST** run the `dev:verifying-spec-compliance` skill to run compliance tests before finishing or merging any development branch.
- **Implementation Plans:** For non-trivial modifications, create or update an implementation plan and wait for human approval before editing codebase files.
- **Incremental Verification:** Test changes frequently to ensure they meet specifications.

## 3. Skills Architecture & Token Efficiency

This repository is a collection of development skills.

- Reusable skills live in `skills/<skill-name>/SKILL.md`.
- **Skill Discovery:** When starting a new conversation or task, you **MUST** load and follow the `dev:bootstrap` skill to establish the discovery and invocation rules for other skills.
- **Subagent Skill Compliance:** Subagents skip the `dev:bootstrap` skill, but they MUST comply with and follow relevant domain-level skills (e.g., `dev:using-jj`, `dev:test-driven-development`, `dev:writing-specs`) when performing matching actions. Subagents MUST explicitly announce their skill usage (e.g., "Using [skill] to [purpose]") before execution.
- **Writing Skills:** When creating, editing, or refining skills in this repository, you **MUST** load and follow the `dev:writing-skills` skill verbatim. Do not repeat its rules here.

## 4. Coding & Cleanliness

- Maintain documentation and existing comments unless explicitly requested to alter them.
- Do not introduce unnecessary dependencies. Keep the codebase lightweight.
