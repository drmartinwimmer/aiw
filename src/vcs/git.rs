use super::normalize_search_dir;
use crate::workspace::WorkspaceError;
use std::path::{Path, PathBuf};

/// Encapsulates Git repository operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Git {
    repo_root: PathBuf,
}

impl Git {
    /// Attempts to discover a Git repository enclosing `dir`.
    pub fn from_dir(dir: &Path) -> Result<Self, WorkspaceError> {
        let normalized = normalize_search_dir(dir)?;
        if !normalized.exists() {
            return Err(WorkspaceError::NotInGitRepo);
        }

        let output = std::process::Command::new("git")
            .args(["--no-pager", "rev-parse", "--show-toplevel"])
            .current_dir(&normalized)
            .output();

        let output = match output {
            Ok(out) => out,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(WorkspaceError::NotInGitRepo);
            }
            Err(err) => return Err(WorkspaceError::Io(err)),
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("not a git repository") {
                return Err(WorkspaceError::NotInGitRepo);
            }
            return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Err(WorkspaceError::GitCommandFailed(
                "Empty output from git rev-parse --show-toplevel".to_string(),
            ));
        }

        let path = PathBuf::from(trimmed);
        let mut repo_root = path.canonicalize().unwrap_or(path);
        if let Some(parent) = repo_root.parent()
            && parent.file_name() == Some(std::ffi::OsStr::new(".workspaces"))
            && let Some(grandparent) = parent.parent()
            && grandparent.join(".git").exists()
        {
            repo_root = grandparent.to_path_buf();
        }
        Ok(Self { repo_root })
    }

    /// Returns the repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Checks if a Git worktree for `workspace_name` is currently registered.
    pub fn is_workspace_registered(&self, workspace_name: &str) -> Result<bool, WorkspaceError> {
        // Prune stale worktrees first so missing directories are not reported as registered
        drop(
            std::process::Command::new("git")
                .args(["--no-pager", "worktree", "prune"])
                .current_dir(&self.repo_root)
                .output(),
        );

        let output = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("not a git repository") {
                return Err(WorkspaceError::NotInGitRepo);
            }
            return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
        }

        let target_path = self.repo_root.join(".workspaces").join(workspace_name);
        let target_canonical = target_path.canonicalize().ok();

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                let wt_path = PathBuf::from(rest.trim());
                if wt_path == target_path {
                    return Ok(true);
                }
                if let (Some(target_c), Ok(wt_c)) = (&target_canonical, wt_path.canonicalize())
                    && wt_c == *target_c
                {
                    return Ok(true);
                }
                let suffix = format!(".workspaces/{workspace_name}");
                if wt_path.ends_with(&suffix) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Adds a new Git worktree under `<repo_root>/<rel_workspace_path>`.
    pub fn add_workspace(
        &self,
        rel_workspace_path: &Path,
        _workspace_name: &str,
    ) -> Result<(), WorkspaceError> {
        drop(
            std::process::Command::new("git")
                .args(["--no-pager", "worktree", "prune"])
                .current_dir(&self.repo_root)
                .output(),
        );

        let output = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "add"])
            .arg(rel_workspace_path)
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("not a git repository") {
                return Err(WorkspaceError::NotInGitRepo);
            }
            return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
        }

        Ok(())
    }

    /// Forgets/removes a Git worktree from the repository.
    pub fn forget_workspace(&self, workspace_name: &str) -> Result<(), WorkspaceError> {
        let ws_path = self.repo_root.join(".workspaces").join(workspace_name);

        let output = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "remove", "--force"])
            .arg(&ws_path)
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("not a git repository") {
                return Err(WorkspaceError::NotInGitRepo);
            }
            if !stderr_lower.contains("is not a working tree")
                && !stderr_lower.contains("not a valid path")
            {
                return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
            }
        }

        drop(
            std::process::Command::new("git")
                .args(["--no-pager", "worktree", "prune"])
                .current_dir(&self.repo_root)
                .output(),
        );

        Ok(())
    }

    /// Checks if a Git workspace exists on disk by checking for `.git`.
    #[must_use]
    pub fn workspace_exists(&self, workspace_path: &Path) -> bool {
        workspace_path.join(".git").exists()
    }

    /// Lists all available workspaces in the Git repository.
    pub fn list_workspaces(&self) -> Result<Vec<String>, WorkspaceError> {
        // Prune stale worktrees first so missing directories are not reported as registered
        drop(
            std::process::Command::new("git")
                .args(["--no-pager", "worktree", "prune"])
                .current_dir(&self.repo_root)
                .output(),
        );

        let output = std::process::Command::new("git")
            .args(["--no-pager", "worktree", "list", "--porcelain"])
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("not a git repository") {
                return Err(WorkspaceError::NotInGitRepo);
            }
            return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
        }

        let workspaces_dir = self.repo_root.join(".workspaces");
        let workspaces_dir_canonical = workspaces_dir.canonicalize().ok();

        let mut workspaces = Vec::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                let wt_path = PathBuf::from(rest.trim());
                let is_in_workspaces = if let Some(parent) = wt_path.parent() {
                    parent == workspaces_dir
                        || (workspaces_dir_canonical.is_some()
                            && parent.canonicalize().ok() == workspaces_dir_canonical)
                        || parent.ends_with(".workspaces")
                } else {
                    false
                };

                if is_in_workspaces
                    && let Some(file_name) = wt_path.file_name().and_then(|n| n.to_str())
                    && crate::workspace::validate_workspace_name(file_name).is_ok()
                    && self.workspace_exists(&wt_path)
                {
                    workspaces.push(file_name.to_string());
                }
            }
        }
        Ok(workspaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    fn init_test_git_repo(path: &Path) {
        let output = std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .arg(path)
            .output()
            .expect("git init");
        assert!(output.status.success());

        drop(
            std::process::Command::new("git")
                .args(["config", "user.name", "Test User"])
                .current_dir(path)
                .output(),
        );
        drop(
            std::process::Command::new("git")
                .args(["config", "user.email", "test@example.com"])
                .current_dir(path)
                .output(),
        );

        std::fs::write(path.join("README.md"), "# Test\n").expect("write file");
        drop(
            std::process::Command::new("git")
                .args(["add", "README.md"])
                .current_dir(path)
                .output(),
        );
        drop(
            std::process::Command::new("git")
                .args(["commit", "-m", "Initial commit"])
                .current_dir(path)
                .output(),
        );
    }

    #[googletest::test]
    fn from_dir_at_repo_root_discovers_git_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let git = Git::from_dir(repo_root).expect("from_dir");
        let canonical_root = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(git.repo_root(), eq(&canonical_root));
    }

    #[googletest::test]
    fn from_dir_in_deep_subdirectory_discovers_git_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let sub = repo_root.join("deep").join("nested");
        std::fs::create_dir_all(&sub).expect("create deep dir");

        let git = Git::from_dir(&sub).expect("from_dir in sub");
        let canonical_root = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(git.repo_root(), eq(&canonical_root));
    }

    #[googletest::test]
    fn from_dir_outside_repo_returns_not_in_git_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = Git::from_dir(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInGitRepo)))
        );
    }

    #[googletest::test]
    fn git_worktree_lifecycle_operations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let git = Git::from_dir(repo_root).expect("from_dir");
        let ws_name = "test-ws";
        let rel_ws_path = Path::new(".workspaces").join(ws_name);
        let abs_ws_path = git.repo_root().join(&rel_ws_path);

        expect_that!(
            git.is_workspace_registered(ws_name).expect("is_registered"),
            is_false()
        );
        expect_that!(git.workspace_exists(&abs_ws_path), is_false());

        std::fs::create_dir_all(git.repo_root().join(".workspaces")).expect("create .workspaces");
        git.add_workspace(&rel_ws_path, ws_name)
            .expect("add_workspace");

        expect_that!(
            git.is_workspace_registered(ws_name).expect("is_registered"),
            is_true()
        );
        expect_that!(git.workspace_exists(&abs_ws_path), is_true());

        git.forget_workspace(ws_name).expect("forget_workspace");
        expect_that!(
            git.is_workspace_registered(ws_name).expect("is_registered"),
            is_false()
        );
    }

    #[googletest::test]
    fn re_adding_workspace_after_forget_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let git = Git::from_dir(repo_root).expect("from_dir");
        let ws_name = "readd-ws";
        let rel_ws_path = Path::new(".workspaces").join(ws_name);
        std::fs::create_dir_all(git.repo_root().join(".workspaces")).expect("create .workspaces");

        git.add_workspace(&rel_ws_path, ws_name).expect("first add");
        git.forget_workspace(ws_name).expect("forget");

        // Re-adding the worktree with the same name must succeed
        let second_add = git.add_workspace(&rel_ws_path, ws_name);
        expect_that!(second_add, ok(anything()));
        expect_that!(
            git.is_workspace_registered(ws_name).expect("is_registered"),
            is_true()
        );
    }

    #[googletest::test]
    fn git_list_workspaces_lists_added_worktrees() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let git = Git::from_dir(repo_root).expect("from_dir");
        let initial = git.list_workspaces().expect("list initial");
        expect_that!(initial, is_empty());

        let rel1 = Path::new(".workspaces").join("git-ws-1");
        let rel2 = Path::new(".workspaces").join("git-ws-2");
        std::fs::create_dir_all(git.repo_root().join(".workspaces")).expect("create .workspaces");
        git.add_workspace(&rel1, "git-ws-1").expect("add git-ws-1");
        git.add_workspace(&rel2, "git-ws-2").expect("add git-ws-2");

        let listed = git.list_workspaces().expect("list after adding");
        expect_that!(
            listed,
            unordered_elements_are![eq("git-ws-1"), eq("git-ws-2")]
        );

        // When inside a worktree, Git::from_dir still resolves to the main repo root
        let ws1_path = git.repo_root().join(&rel1);
        let git_from_ws = Git::from_dir(&ws1_path).expect("from_dir inside ws");
        expect_that!(git_from_ws.repo_root(), eq(git.repo_root()));
        let listed_from_ws = git_from_ws.list_workspaces().expect("list from ws");
        expect_that!(
            listed_from_ws,
            unordered_elements_are![eq("git-ws-1"), eq("git-ws-2")]
        );

        // After forgetting git-ws-1, only git-ws-2 remains
        git.forget_workspace("git-ws-1").expect("forget ws-1");
        let listed_after_forget = git.list_workspaces().expect("list after forget");
        expect_that!(listed_after_forget, elements_are![eq("git-ws-2")]);
    }
}
