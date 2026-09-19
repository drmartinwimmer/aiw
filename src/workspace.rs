use std::path::{Path, PathBuf};

use crate::vcs::VcsType;

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("Not inside a Jujutsu or Git repository")]
    NotInRepo,
    #[error("Not inside a Jujutsu repository")]
    NotInJjRepo,
    #[error("Not inside a Git repository")]
    NotInGitRepo,
    #[error("Failed to execute Jujutsu command: {0}")]
    JjCommandFailed(String),
    #[error("Failed to execute Git command: {0}")]
    GitCommandFailed(String),
    #[error("Invalid workspace name: {0}")]
    InvalidWorkspaceName(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Finds the root directory of the enclosing Jujutsu repository by invoking `jj --no-pager root`.
pub fn find_jj_root(start_dir: &Path) -> Result<PathBuf, WorkspaceError> {
    crate::vcs::jj::find_root(start_dir)
}

/// Finds the root directory of the enclosing Git repository.
pub fn find_git_root(start_dir: &Path) -> Result<PathBuf, WorkspaceError> {
    crate::vcs::git::find_root(start_dir)
}

/// Auto-detects the repository root and VCS type for `start_dir`.
pub fn find_root(start_dir: &Path) -> Result<(PathBuf, VcsType), WorkspaceError> {
    crate::vcs::detect(start_dir)
}

fn validate_workspace_name(workspace_name: &str) -> Result<(), WorkspaceError> {
    if workspace_name.is_empty()
        || workspace_name == "."
        || workspace_name == ".."
        || workspace_name.contains('/')
        || workspace_name.contains('\\')
    {
        return Err(WorkspaceError::InvalidWorkspaceName(
            workspace_name.to_string(),
        ));
    }
    Ok(())
}

/// Encapsulates state and operations for a workspace under `<repo_root>/.workspaces/<name>`.
///
/// Supports both Jujutsu workspaces and Git worktrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    repo_root: PathBuf,
    name: String,
    path: PathBuf,
    vcs: VcsType,
}

impl Workspace {
    /// Creates a new `Workspace` instance for a given repository root and workspace name.
    ///
    /// Validates that the name is valid and auto-detects whether `repo_root` is a Jujutsu or Git repository.
    pub fn new(repo_root: &Path, name: &str) -> Result<Self, WorkspaceError> {
        validate_workspace_name(name)?;

        let abs_repo_root = if repo_root.is_absolute() {
            repo_root.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(repo_root)
        } else {
            repo_root.to_path_buf()
        };

        let vcs = if abs_repo_root.join(".jj").exists() {
            VcsType::Jj
        } else if abs_repo_root.join(".git").exists() {
            VcsType::Git
        } else {
            let (_, detected) = crate::vcs::detect(&abs_repo_root)?;
            detected
        };

        let path = abs_repo_root.join(".workspaces").join(name);
        Ok(Self {
            repo_root: abs_repo_root,
            name: name.to_string(),
            path,
            vcs,
        })
    }

    /// Creates a new `Workspace` instance with an explicitly specified VCS type.
    pub fn new_with_vcs(repo_root: &Path, name: &str, vcs: VcsType) -> Result<Self, WorkspaceError> {
        validate_workspace_name(name)?;

        let abs_repo_root = if repo_root.is_absolute() {
            repo_root.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(repo_root)
        } else {
            repo_root.to_path_buf()
        };

        let path = abs_repo_root.join(".workspaces").join(name);
        Ok(Self {
            repo_root: abs_repo_root,
            name: name.to_string(),
            path,
            vcs,
        })
    }

    /// Discovers the repository root and VCS enclosing `dir` and returns a `Workspace` handle.
    pub fn from_dir(dir: &Path, name: &str) -> Result<Self, WorkspaceError> {
        let (repo_root, vcs) = find_root(dir)?;
        Self::new_with_vcs(&repo_root, name, vcs)
    }

    /// Returns the detected or configured VCS type.
    #[must_use]
    pub fn vcs(&self) -> VcsType {
        self.vcs
    }

    /// Returns the workspace name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the workspace directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the enclosing repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Returns `true` if the workspace currently exists on disk and contains appropriate VCS metadata.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.vcs.workspace_exists(&self.path)
    }

    /// Ensures the workspace exists on disk and is registered in the underlying VCS.
    ///
    /// Returns `Ok(true)` if the workspace was newly created, or `Ok(false)` if it already existed.
    pub fn ensure(&self) -> Result<bool, WorkspaceError> {
        let is_registered = self.vcs.is_workspace_registered(&self.repo_root, &self.name)?;
        if is_registered && self.exists() {
            return Ok(false);
        }

        // Clean up any stale/forgotten directory on disk before adding
        if self.path.exists() {
            std::fs::remove_dir_all(&self.path)?;
        }

        let workspaces_dir = self.repo_root.join(".workspaces");
        std::fs::create_dir_all(&workspaces_dir)?;

        let rel_workspace_path = Path::new(".workspaces").join(&self.name);
        self.vcs.add_workspace(&self.repo_root, &rel_workspace_path, &self.name)?;

        Ok(true)
    }

    /// Forgets the workspace in the underlying VCS and removes the workspace directory from disk.
    pub fn forget(&self) -> Result<(), WorkspaceError> {
        self.vcs.forget_workspace(&self.repo_root, &self.name)?;
        if self.path.exists() {
            std::fs::remove_dir_all(&self.path)?;
        }
        Ok(())
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
                .expect("failed to run jj init or jj git init");
            assert!(
                fallback.status.success(),
                "Failed to init jj repo: {}",
                String::from_utf8_lossy(&fallback.stderr)
            );
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
            assert!(fallback.status.success(), "Failed to init git repo");
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

        let f = path.join("file.txt");
        std::fs::write(&f, "content\n").expect("write file");
        drop(
            std::process::Command::new("git")
                .args(["add", "file.txt"])
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
    fn find_jj_root_from_root_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let found = find_jj_root(repo_root).expect("find_jj_root");
        expect_that!(found, eq(repo_root));
    }

    #[googletest::test]
    fn find_jj_root_from_deep_subdirectory_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let sub = repo_root.join("sub1").join("sub2");
        std::fs::create_dir_all(&sub).expect("create_dir_all");

        let found = find_jj_root(&sub).expect("find_jj_root");
        expect_that!(found, eq(repo_root));
    }

    #[googletest::test]
    fn find_jj_root_outside_repo_returns_not_in_jj_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = find_jj_root(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInJjRepo)))
        );
    }

    #[googletest::test]
    fn find_git_root_from_root_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let found = find_git_root(repo_root).expect("find_git_root");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(found, eq(&canonical_root));
    }

    #[googletest::test]
    fn find_git_root_outside_repo_returns_not_in_git_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = find_git_root(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInGitRepo)))
        );
    }

    #[googletest::test]
    fn find_root_auto_detects_jj_and_git() {
        let dir_jj = tempfile::tempdir().expect("tempdir jj");
        init_test_jj_repo(dir_jj.path());
        let (root_jj, vcs_jj) = find_root(dir_jj.path()).expect("detect jj");
        expect_that!(root_jj, eq(dir_jj.path()));
        expect_that!(vcs_jj, eq(VcsType::Jj));

        let dir_git = tempfile::tempdir().expect("tempdir git");
        init_test_git_repo(dir_git.path());
        let (root_git, vcs_git) = find_root(dir_git.path()).expect("detect git");
        let canonical_git = dir_git.path().canonicalize().unwrap_or_else(|_| dir_git.path().to_path_buf());
        expect_that!(root_git, eq(&canonical_git));
        expect_that!(vcs_git, eq(VcsType::Git));
    }

    #[googletest::test]
    fn workspace_ensure_creates_workspace_and_registers_in_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws_name = "ws-alpha";
        let ws = Workspace::new(repo_root, ws_name).expect("Workspace::new");
        let expected_path = repo_root.join(".workspaces").join(ws_name);

        expect_that!(ws.vcs(), eq(VcsType::Jj));
        expect_that!(ws.path(), eq(&expected_path));
        expect_that!(ws.exists(), is_false());

        let created = ws.ensure().expect("ensure");
        expect_that!(created, is_true());
        expect_that!(ws.exists(), is_true());
        expect_that!(expected_path.join(".jj").exists(), is_true());

        let output = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(repo_root)
            .output()
            .expect("jj workspace list");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        expect_that!(stdout.as_ref(), contains_substring(ws_name));
    }

    #[googletest::test]
    fn workspace_ensure_creates_workspace_and_registers_in_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let ws_name = "git-ws-alpha";
        let ws = Workspace::new(repo_root, ws_name).expect("Workspace::new");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        let expected_path = canonical_root.join(".workspaces").join(ws_name);

        expect_that!(ws.vcs(), eq(VcsType::Git));
        expect_that!(ws.exists(), is_false());

        let created = ws.ensure().expect("ensure");
        expect_that!(created, is_true());
        expect_that!(ws.exists(), is_true());
        expect_that!(expected_path.join(".git").exists(), is_true());

        let output = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "list", "--porcelain"])
            .current_dir(repo_root)
            .output()
            .expect("git worktree list");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        expect_that!(stdout.as_ref(), contains_substring(ws_name));
    }

    #[googletest::test]
    fn workspace_ensure_when_already_exists_succeeds_idempotently() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws_name = "ws-beta";
        let ws = Workspace::new(repo_root, ws_name).expect("Workspace::new");
        let created1 = ws.ensure().expect("first ensure");
        let created2 = ws.ensure().expect("second ensure");

        expect_that!(created1, is_true());
        expect_that!(created2, is_false());
        expect_that!(ws.exists(), is_true());
    }

    #[googletest::test]
    fn workspace_exists_reflects_presence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws_name = "ws-check";
        let ws = Workspace::new(repo_root, ws_name).expect("Workspace::new");
        expect_that!(ws.exists(), is_false());

        ws.ensure().expect("ensure");
        expect_that!(ws.exists(), is_true());
    }

    #[googletest::test]
    fn workspace_new_outside_repo_returns_not_in_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = Workspace::new(non_repo, "ws-gamma");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInRepo)))
        );
        expect_that!(non_repo.join(".workspaces").exists(), is_false());
    }

    #[googletest::test]
    fn workspace_new_with_empty_name_returns_invalid_workspace_name_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let res = Workspace::new(repo_root, "");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::InvalidWorkspaceName(anything()))))
        );
    }

    #[googletest::test]
    fn workspace_new_with_path_traversal_name_returns_invalid_workspace_name_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        for bad_name in [".", "..", "foo/bar", "foo\\bar", "../escape"] {
            let res = Workspace::new(repo_root, bad_name);
            expect_that!(
                res,
                matches_pattern!(Err(matches_pattern!(WorkspaceError::InvalidWorkspaceName(anything()))))
            );
        }
    }

    #[googletest::test]
    fn workspace_forget_removes_workspace_from_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws_name = "ws-to-forget";
        let ws = Workspace::new(repo_root, ws_name).expect("Workspace::new");
        ws.ensure().expect("ensure");

        let before = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(repo_root)
            .output()
            .expect("jj workspace list");
        expect_that!(
            String::from_utf8_lossy(&before.stdout).as_ref(),
            contains_substring(ws_name)
        );

        ws.forget().expect("forget");

        let after = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(repo_root)
            .output()
            .expect("jj workspace list");
        expect_that!(
            String::from_utf8_lossy(&after.stdout).as_ref(),
            not(contains_substring(ws_name))
        );
    }

    #[googletest::test]
    fn workspace_forget_removes_workspace_from_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let ws_name = "git-ws-to-forget";
        let ws = Workspace::new(repo_root, ws_name).expect("Workspace::new");
        ws.ensure().expect("ensure");

        let before = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "list", "--porcelain"])
            .current_dir(repo_root)
            .output()
            .expect("git worktree list");
        expect_that!(
            String::from_utf8_lossy(&before.stdout).as_ref(),
            contains_substring(ws_name)
        );

        ws.forget().expect("forget");

        let after = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "list", "--porcelain"])
            .current_dir(repo_root)
            .output()
            .expect("git worktree list");
        expect_that!(
            String::from_utf8_lossy(&after.stdout).as_ref(),
            not(contains_substring(ws_name))
        );
        expect_that!(ws.path().exists(), is_false());
    }

    #[googletest::test]
    fn workspace_struct_lifecycle_and_methods() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws = Workspace::new(repo_root, "struct-ws").expect("new Workspace");
        expect_that!(ws.name(), eq("struct-ws"));
        expect_that!(ws.repo_root(), eq(repo_root));
        expect_that!(ws.path(), eq(&repo_root.join(".workspaces").join("struct-ws")));
        expect_that!(ws.exists(), is_false());

        // First ensure returns Ok(true) indicating newly created
        let is_new = ws.ensure().expect("ensure workspace");
        expect_that!(is_new, is_true());
        expect_that!(ws.exists(), is_true());

        // Second ensure returns Ok(false) indicating already existed
        let is_new_again = ws.ensure().expect("ensure workspace again");
        expect_that!(is_new_again, is_false());

        // Test from_dir
        let sub = repo_root.join("subdir");
        std::fs::create_dir_all(&sub).expect("create_dir_all");
        let ws_from_sub = Workspace::from_dir(&sub, "from-sub").expect("from_dir");
        expect_that!(ws_from_sub.repo_root(), eq(repo_root));
        expect_that!(ws_from_sub.name(), eq("from-sub"));

        // Forget workspace
        ws.forget().expect("forget workspace");
        expect_that!(ws.path().exists(), is_false());
        let list_output = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(repo_root)
            .output()
            .expect("jj workspace list");
        expect_that!(
            String::from_utf8_lossy(&list_output.stdout).as_ref(),
            not(contains_substring("struct-ws"))
        );
    }

    #[googletest::test]
    fn workspace_ensure_after_forget_recreates_and_returns_true() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws = Workspace::new(repo_root, "recreate-ws").expect("Workspace::new");
        let first_ensure = ws.ensure().expect("first ensure");
        expect_that!(first_ensure, is_true());
        expect_that!(ws.path().exists(), is_true());

        ws.forget().expect("forget");
        expect_that!(ws.path().exists(), is_false());

        // Re-creating the forgotten workspace must return Ok(true)
        let second_ensure = ws.ensure().expect("second ensure");
        expect_that!(second_ensure, is_true());
        expect_that!(ws.path().exists(), is_true());
    }
}
