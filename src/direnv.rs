use std::path::{Path, PathBuf};

use crate::tools::DirenvCommand;

/// Encapsulates direnv detection and authorization state for a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Direnv {
    repo_root: PathBuf,
    allowed: bool,
}

impl Direnv {
    /// Inspects the given repository root once and determines whether direnv is active and allowed.
    #[must_use]
    pub fn new(repo_root: &Path) -> Self {
        let allowed = Self::is_dir_allowed(repo_root);
        Self {
            repo_root: repo_root.to_path_buf(),
            allowed,
        }
    }

    /// Returns `true` if direnv is active and allowed for the repository root.
    #[must_use]
    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    /// Returns the repository root path associated with this `Direnv` instance.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Checks if a directory contains a `.envrc` or `.env` file.
    #[must_use]
    pub fn has_envrc(dir: &Path) -> bool {
        dir.join(".envrc").exists() || dir.join(".env").exists()
    }

    /// Checks if `direnv` is allowed in the specified directory by inspecting `direnv status`.
    ///
    /// Supports both structured JSON output (`direnv status --json`, direnv >= 2.37.1)
    /// and standard human-readable text output (`Found RC allowed 0` or `true`, direnv < 2.37.1).
    #[must_use]
    pub fn is_dir_allowed(dir: &Path) -> bool {
        if !Self::has_envrc(dir) {
            return false;
        }

        DirenvCommand::new()
            .current_dir(dir)
            .status()
            .ok()
            .and_then(|s| s.allowed)
            .unwrap_or_default()
    }

    /// If the directory contains `.envrc` or `.env`, runs `direnv allow` on that directory.
    fn allow_dir(dir: &Path) {
        if Self::has_envrc(dir) {
            drop(DirenvCommand::new().current_dir(dir).allow());
        }
    }

    /// If direnv is allowed for the repository and the workspace directory contains an `.envrc` or `.env`,
    /// runs `direnv allow` on the workspace directory.
    pub fn allow_workspace(&self, workspace_path: &Path) {
        if self.allowed {
            Self::allow_dir(workspace_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn has_envrc_detects_envrc_and_env() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let path = temp_dir.path();
        expect_that!(Direnv::has_envrc(path), is_false());

        std::fs::write(path.join(".env"), "A=1\n").expect("write .env");
        expect_that!(Direnv::has_envrc(path), is_true());

        std::fs::remove_file(path.join(".env")).expect("remove .env");
        std::fs::write(path.join(".envrc"), "export A=1\n").expect("write .envrc");
        expect_that!(Direnv::has_envrc(path), is_true());
    }

    #[googletest::test]
    fn is_direnv_allowed_returns_false_for_directory_without_envrc() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        expect_that!(Direnv::is_dir_allowed(temp_dir.path()), is_false());
    }

    #[googletest::test]
    fn is_direnv_allowed_checks_authorization_status() {
        if which::which("direnv").is_err() {
            return;
        }
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let envrc = temp_dir.path().join(".envrc");
        std::fs::write(&envrc, "export TEST_VAR=1\n").expect("write .envrc");

        // Before allow, should be false
        expect_that!(Direnv::is_dir_allowed(temp_dir.path()), is_false());

        // Allow it
        DirenvCommand::new()
            .current_dir(temp_dir.path())
            .allow()
            .expect("direnv allow");

        // After allow, should be true
        expect_that!(Direnv::is_dir_allowed(temp_dir.path()), is_true());
    }

    #[googletest::test]
    fn direnv_struct_encapsulates_state_and_allows_workspace() {
        if which::which("direnv").is_err() {
            return;
        }
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let ws_dir = temp_dir.path().join("ws");
        std::fs::create_dir_all(&repo_root).expect("create repo");
        std::fs::create_dir_all(&ws_dir).expect("create ws");

        let repo_envrc = repo_root.join(".envrc");
        std::fs::write(&repo_envrc, "export R=1\n").expect("write repo .envrc");
        let ws_envrc = ws_dir.join(".envrc");
        std::fs::write(&ws_envrc, "export W=1\n").expect("write ws .envrc");

        // When repo is unallowed:
        let unallowed_direnv = Direnv::new(&repo_root);
        expect_that!(unallowed_direnv.is_allowed(), is_false());
        unallowed_direnv.allow_workspace(&ws_dir);
        expect_that!(Direnv::is_dir_allowed(&ws_dir), is_false());

        // Allow repo:
        DirenvCommand::new()
            .current_dir(&repo_root)
            .allow()
            .expect("allow repo");

        // When repo is allowed:
        let allowed_direnv = Direnv::new(&repo_root);
        expect_that!(allowed_direnv.is_allowed(), is_true());
        allowed_direnv.allow_workspace(&ws_dir);
        expect_that!(Direnv::is_dir_allowed(&ws_dir), is_true());
    }
}
