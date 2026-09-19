pub mod git;
pub mod jj;

use std::path::{Path, PathBuf};
use crate::workspace::WorkspaceError;

/// Supported Version Control System types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VcsType {
    Jj,
    Git,
}

impl std::fmt::Display for VcsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcsType::Jj => write!(f, "Jujutsu"),
            VcsType::Git => write!(f, "Git"),
        }
    }
}

impl VcsType {
    /// Returns `true` if the workspace with the given name is registered in this VCS.
    pub fn is_workspace_registered(
        self,
        repo_root: &Path,
        name: &str,
    ) -> Result<bool, WorkspaceError> {
        match self {
            VcsType::Jj => jj::is_workspace_registered(repo_root, name),
            VcsType::Git => git::is_workspace_registered(repo_root, name),
        }
    }

    /// Adds/registers a new workspace in this VCS.
    pub fn add_workspace(
        self,
        repo_root: &Path,
        rel_workspace_path: &Path,
        name: &str,
    ) -> Result<(), WorkspaceError> {
        match self {
            VcsType::Jj => jj::add_workspace(repo_root, rel_workspace_path, name),
            VcsType::Git => git::add_workspace(repo_root, rel_workspace_path, name),
        }
    }

    /// Forgets/unregisters a workspace in this VCS.
    pub fn forget_workspace(self, repo_root: &Path, name: &str) -> Result<(), WorkspaceError> {
        match self {
            VcsType::Jj => jj::forget_workspace(repo_root, name),
            VcsType::Git => git::forget_workspace(repo_root, name),
        }
    }

    /// Returns `true` if the workspace exists on disk according to this VCS's metadata.
    #[must_use]
    pub fn workspace_exists(self, workspace_path: &Path) -> bool {
        match self {
            VcsType::Jj => jj::workspace_exists(workspace_path),
            VcsType::Git => git::workspace_exists(workspace_path),
        }
    }
}

/// Auto-detects the repository root and VCS type for `start_dir`.
///
/// Jujutsu is checked first (including colocated Git repositories).
/// If not in a Jujutsu repository, Git is checked next.
/// If neither is detected, returns `Err(WorkspaceError::NotInRepo)`.
pub fn detect(start_dir: &Path) -> Result<(PathBuf, VcsType), WorkspaceError> {
    let abs_dir = if start_dir.is_absolute() {
        start_dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(start_dir)
    };

    let dir = if abs_dir.is_file() {
        abs_dir.parent().map(Path::to_path_buf).unwrap_or(abs_dir)
    } else {
        abs_dir
    };

    if !dir.exists() {
        return Err(WorkspaceError::NotInRepo);
    }

    // Try Jujutsu first
    match jj::find_root(&dir) {
        Ok(root) => return Ok((root, VcsType::Jj)),
        Err(WorkspaceError::NotInJjRepo) => {}
        Err(err) => return Err(err),
    }

    // Try Git next
    match git::find_root(&dir) {
        Ok(root) => Ok((root, VcsType::Git)),
        Err(WorkspaceError::NotInGitRepo) => Err(WorkspaceError::NotInRepo),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    fn init_test_jj_repo(path: &Path) {
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "git", "init"])
            .arg(path)
            .output();

        let success = matches!(output, Ok(out) if out.status.success());
        if !success {
            let fallback = std::process::Command::new("jj")
                .args(["--no-pager", "init", "--git"])
                .arg(path)
                .output()
                .expect("failed to run jj init");
            assert!(fallback.status.success());
        }
    }

    fn init_test_git_repo(path: &Path) {
        let output = std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .arg(path)
            .output();

        let success = matches!(output, Ok(ref out) if out.status.success());
        if !success {
            let fallback = std::process::Command::new("git")
                .arg("init")
                .arg(path)
                .output()
                .expect("failed to run git init");
            assert!(fallback.status.success());
        }

        drop(
            std::process::Command::new("git")
                .args(["config", "user.name", "Test"])
                .current_dir(path)
                .output(),
        );
        drop(
            std::process::Command::new("git")
                .args(["config", "user.email", "test@example.com"])
                .current_dir(path)
                .output(),
        );

        let f = path.join("f.txt");
        std::fs::write(&f, "ok\n").expect("write file");
        drop(
            std::process::Command::new("git")
                .args(["add", "f.txt"])
                .current_dir(path)
                .output(),
        );
        drop(
            std::process::Command::new("git")
                .args(["commit", "-m", "init"])
                .current_dir(path)
                .output(),
        );
    }

    #[googletest::test]
    fn detect_in_jj_repo_identifies_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let (detected_root, vcs) = detect(repo_root).expect("detect");
        expect_that!(detected_root, eq(repo_root));
        expect_that!(vcs, eq(VcsType::Jj));
    }

    #[googletest::test]
    fn detect_in_git_repo_identifies_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let (detected_root, vcs) = detect(repo_root).expect("detect");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(detected_root, eq(&canonical_root));
        expect_that!(vcs, eq(VcsType::Git));
    }

    #[googletest::test]
    fn detect_outside_repo_returns_not_in_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = detect(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInRepo)))
        );
    }
}
