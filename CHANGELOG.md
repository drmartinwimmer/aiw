# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.6.0...ai-workspace-v0.7.0) (2026-11-13)


### Features

* add global --fence flag to support unsandboxed non-permissive launches ([#49](https://github.com/drmartinwimmer/aiw/issues/49)) ([9a8b64a](https://github.com/drmartinwimmer/aiw/commit/9a8b64a738d657680f9d8254d08537c9c6ad4063))
* harden Fence sandbox policy and workspace launch isolation ([#46](https://github.com/drmartinwimmer/aiw/issues/46)) ([00eb0df](https://github.com/drmartinwimmer/aiw/commit/00eb0df1c27a3169081af32a94190a180f6a6b2e))

## [0.6.0](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.5.0...ai-workspace-v0.6.0) (2026-11-12)


### Features

* add herdr-core (hc) environment integration ([#41](https://github.com/drmartinwimmer/aiw/issues/41)) ([c5c857e](https://github.com/drmartinwimmer/aiw/commit/c5c857e9bfbe090e16900243dbbeeac39584f158))


### Bug Fixes

* open workspace tabs in Herdr spaces via hc layout document ([#44](https://github.com/drmartinwimmer/aiw/issues/44)) ([db43446](https://github.com/drmartinwimmer/aiw/commit/db43446f1d0cd5325733e63cb6c2001e44814ad1))
* sync API.md on release PRs and track package version in flake.nix ([#45](https://github.com/drmartinwimmer/aiw/issues/45)) ([c47eb66](https://github.com/drmartinwimmer/aiw/commit/c47eb6634351556e3363c100a213cff0e8beda12))

## [0.5.0](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.4.0...ai-workspace-v0.5.0) (2026-10-17)


### Features

* close newly created Herdr tab pane when agent exits ([#30](https://github.com/drmartinwimmer/aiw/issues/30)) ([28ebf05](https://github.com/drmartinwimmer/aiw/commit/28ebf05eafda4f0849a0bf4a638cfabbb252a5dd))

## [0.4.0](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.3.0...ai-workspace-v0.4.0) (2026-10-03)


### Features

* add shell completion subcommand and dynamic workspace autocompletion ([#14](https://github.com/drmartinwimmer/aiw/issues/14)) ([408299e](https://github.com/drmartinwimmer/aiw/commit/408299ed45de4ac96b79f09ed64022fbf6f43ad0))

## [0.3.0](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.2.1...ai-workspace-v0.3.0) (2026-10-01)


### Features

* add list subcommand to list all available workspaces ([#10](https://github.com/drmartinwimmer/aiw/issues/10)) ([a9b7882](https://github.com/drmartinwimmer/aiw/commit/a9b7882690d5febbf8ccb411a4e3c3610a31f743))

## [0.2.1](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.2.0...ai-workspace-v0.2.1) (2026-09-29)


### Bug Fixes

* allow reading /nix in default fence template for NixOS ([ef90fa6](https://github.com/drmartinwimmer/aiw/commit/ef90fa6f8632648433217ff2a1a648b81753aafa))
* allow reading /nix in default fence template for NixOS ([640b433](https://github.com/drmartinwimmer/aiw/commit/640b433852ff1807f63bcc298eba1e4f4decfb61)), closes [#7](https://github.com/drmartinwimmer/aiw/issues/7)

## [0.2.0](https://github.com/drmartinwimmer/aiw/compare/ai-workspace-v0.1.3...ai-workspace-v0.2.0) (2026-09-29)


### Features

* add aiw config init, aiw-specific config discovery, and build.rs installer ([145e62d](https://github.com/drmartinwimmer/aiw/commit/145e62d45253bea3d55683875e0751f363896222))
* add forget subcommand, direnv support, and agy auth endpoints ([e070f81](https://github.com/drmartinwimmer/aiw/commit/e070f812428642f460d5cb490499e40efddec72b))
* add herdr module and integrate tab creation and agent reporting ([e656d49](https://github.com/drmartinwimmer/aiw/commit/e656d4977bcb8827386bc988627f7cb581646925))
* adopt jsonc format, alphabetize config fields, and discover shared aiw template ([c3beb0b](https://github.com/drmartinwimmer/aiw/commit/c3beb0bcf3578e768acb92769387134d59b3b7c7))
* canonicalize aiw.jsonc path, add config init subcommands, and restrict build.rs to cargo install ([3faca8a](https://github.com/drmartinwimmer/aiw/commit/3faca8aa693680618667a7d9a8b972202d963d9c))
* **config:** add default_tools and network options ([5828d83](https://github.com/drmartinwimmer/aiw/commit/5828d8390dee634154402f8c014d093a333954e9))
* **sandbox:** launch agy in yolo mode and bind .git for jj compatibility ([1a3dab0](https://github.com/drmartinwimmer/aiw/commit/1a3dab0f75acc6a57e124b52cd8e81fc49e9c406))
* **sandbox:** replace bubblewrap sandbox with fence ([15827c5](https://github.com/drmartinwimmer/aiw/commit/15827c586a1c476bd13f4af07f4c71e37a177d8c))
* support Git repositories and worktrees with auto-detected VCS ([1b0f1c3](https://github.com/drmartinwimmer/aiw/commit/1b0f1c300ea0a010164de341b637cb19aa2c009a))


### Bug Fixes

* clean up disk directory on workspace forget, check jj registration, and use path:. in .envrc for flakes ([b1d6740](https://github.com/drmartinwimmer/aiw/commit/b1d6740179cf063e8d3b29097eeae9757583037e))
* **direnv:** check direnv once, only allow on new workspace, and prepend command iff allowed ([1ed670b](https://github.com/drmartinwimmer/aiw/commit/1ed670b565ffc78ded7682328eb38ac36056d2f8))
* **direnv:** support legacy text status output in authorization check ([00a2ed1](https://github.com/drmartinwimmer/aiw/commit/00a2ed17235c18472f8eaa23c4067e90f700d5a5))
* **sandbox:** allow Google avatar hosts (lh1..lh6.googleusercontent.com) for agy eligibility check ([5f409e5](https://github.com/drmartinwimmer/aiw/commit/5f409e5154c2451f503945684d73d3778272cc81))
* **sandbox:** allow www.googleapis.com for agy oauth eligibility check ([d048592](https://github.com/drmartinwimmer/aiw/commit/d048592526b721e41f6ad8bc6b318b71fe68c9cb))
* **sandbox:** mount DBus, keyring, and SSL certificate paths to retain OAuth token ([8b5b1e5](https://github.com/drmartinwimmer/aiw/commit/8b5b1e5f1187b3b8bfac52e4b880d549947a9763))
* **sandbox:** only auto-allow direnv if jj root is allowed ([b0bedf8](https://github.com/drmartinwimmer/aiw/commit/b0bedf8daeaf419a270cce76a33e53431f6a0fd3))
* **sandbox:** remove host ~/.gemini creation and eliminate unsafe PATH locks in tests ([261c9fc](https://github.com/drmartinwimmer/aiw/commit/261c9fcbc4956a71cd1e3d181223cb35c91196e7))

## [0.1.3] - 2026-09-29

### Changed

- Renamed crate to `ai-workspace` on crates.io, while maintaining the binary executable name as `aiw` (`[[bin]] name = "aiw"`).
- Updated installation instructions and Crates.io/Docs.rs badges in `README.md`.

## [0.1.2] - 2026-09-28

### Added

- GitHub Actions CI workflow (`.github/workflows/ci.yml`) running formatting checks, Clippy lints, crates.io packaging dry-run, unit/integration tests with Fence & Jujutsu, and Nix flake check/build.
- GitHub Actions Release workflow (`.github/workflows/release.yml`) for automated publishing to crates.io on tag pushes (`v*`) and manual workflow dispatch.
- CI, Crates.io, and Docs.rs status badges to `README.md`.
- Repository, homepage, and documentation URLs in `Cargo.toml`.
- Added `/fence-seccomp` to `.gitignore` to prevent Fence runtime sandbox artifacts from polluting the workspace.

## [0.1.1] - 2026-09-28

### Documentation

- Documented Git repository and worktree support in `README.md` alongside Jujutsu (`jj`).

## [0.1.0] - 2026-09-28

### Added

- Initial release of AI Workspace (`aiw`).
- Automatic Jujutsu (`jj`) and Git workspace provisioning under `.workspaces/<workspace-name>`.
- Fence sandbox isolation with restricted filesystem and network permissions for autonomous agent runs.
- Automatic `direnv` environment variable propagation and workspace authorization.
- Herdr terminal multiplexer integration with automated workspace tabs and agent state reporting.
- Two-layer configuration system supporting shared base templates and project-specific `fence.jsonc` overrides.
- `aiw config init` command for initializing project and user configuration files.
