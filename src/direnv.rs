use std::path::{Path, PathBuf};
use std::process::Command;

use directories::BaseDirs;

/// Ensures user profile binary paths (such as Nix user, home-manager, and local bin profiles)
/// are present in PATH so that commands can locate tools like direnv and fence.
pub(crate) fn ensure_user_profile_bin_paths(cmd: &mut Command) {
    let Ok(path_var) = std::env::var("PATH") else {
        return;
    };

    let mut paths: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    let mut updated = false;

    if let Some(base_dirs) = BaseDirs::new() {
        let home = base_dirs.home_dir();
        let local_bin = home.join(".local/bin");
        if local_bin.exists() && !paths.contains(&local_bin) {
            paths.insert(0, local_bin);
            updated = true;
        }
        let nix_bin = home.join(".nix-profile/bin");
        if nix_bin.exists() && !paths.contains(&nix_bin) {
            paths.push(nix_bin);
            updated = true;
        }
    }

    if let Ok(user) = std::env::var("USER") {
        let user_bin = PathBuf::from(format!("/etc/profiles/per-user/{user}/bin"));
        if user_bin.exists() && !paths.contains(&user_bin) {
            paths.push(user_bin);
            updated = true;
        }
    }

    if updated && let Ok(new_path) = std::env::join_paths(paths) {
        cmd.env("PATH", new_path);
    }
}

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

    /// Checks if `direnv` is allowed in the specified directory by inspecting `direnv status --json`.
    /// Returns `true` only if `direnv` finds an `.envrc` or `.env` in `dir` and its `allowed` status is 0.
    #[must_use]
    pub fn is_dir_allowed(dir: &Path) -> bool {
        if !Self::has_envrc(dir) {
            return false;
        }

        let mut cmd = Command::new("direnv");
        cmd.args(["status", "--json"]);
        cmd.current_dir(dir);
        ensure_user_profile_bin_paths(&mut cmd);

        let Ok(output) = cmd.output() else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        #[derive(serde::Deserialize)]
        struct DirenvStatus {
            state: Option<DirenvState>,
        }

        #[derive(serde::Deserialize)]
        struct DirenvState {
            #[serde(rename = "foundRC")]
            found_rc: Option<FoundRc>,
        }

        #[derive(serde::Deserialize)]
        struct FoundRc {
            allowed: i32,
        }

        serde_json::from_slice::<DirenvStatus>(&output.stdout)
            .ok()
            .and_then(|s| s.state)
            .and_then(|s| s.found_rc)
            .is_some_and(|rc| rc.allowed == 0)
    }

    /// If the directory contains `.envrc` or `.env`, runs `direnv allow` on that directory.
    pub fn allow_dir(dir: &Path) {
        if Self::has_envrc(dir) {
            let mut cmd = Command::new("direnv");
            cmd.args(["allow"]).arg(dir);
            ensure_user_profile_bin_paths(&mut cmd);
            drop(cmd.output());
        }
    }

    /// If direnv is allowed for the repository and the workspace directory contains an `.envrc` or `.env`,
    /// runs `direnv allow` on the workspace directory.
    pub fn allow_workspace(&self, workspace_path: &Path) {
        if self.allowed {
            Self::allow_dir(workspace_path);
        }
    }

    /// Runs `direnv allow` on the specified directory if it contains `.envrc` or `.env`.
    pub fn allow(&self, dir: &Path) {
        Self::allow_dir(dir);
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
        let allow_res = Command::new("direnv")
            .args(["allow"])
            .current_dir(temp_dir.path())
            .output()
            .expect("direnv allow");
        assert!(allow_res.status.success());

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
        let allow_res = Command::new("direnv")
            .args(["allow"])
            .current_dir(&repo_root)
            .output()
            .expect("allow repo");
        assert!(allow_res.status.success());

        // When repo is allowed:
        let allowed_direnv = Direnv::new(&repo_root);
        expect_that!(allowed_direnv.is_allowed(), is_true());
        allowed_direnv.allow_workspace(&ws_dir);
        expect_that!(Direnv::is_dir_allowed(&ws_dir), is_true());
    }
}
