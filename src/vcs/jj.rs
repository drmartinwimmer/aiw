use super::normalize_search_dir;
use crate::workspace::WorkspaceError;
use std::path::{Path, PathBuf};

/// Encapsulates Jujutsu repository operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jj {
    repo_root: PathBuf,
}

impl Jj {
    /// Attempts to discover a Jujutsu repository enclosing `dir`.
    pub fn from_dir(dir: &Path) -> Result<Self, WorkspaceError> {
        let normalized = normalize_search_dir(dir)?;
        if !normalized.exists() {
            return Err(WorkspaceError::NotInJjRepo);
        }

        let output = std::process::Command::new("jj")
            .args(["--no-pager", "root"])
            .current_dir(&normalized)
            .output();

        let output = match output {
            Ok(out) => out,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(WorkspaceError::NotInJjRepo);
            }
            Err(err) => return Err(WorkspaceError::Io(err)),
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(WorkspaceError::NotInJjRepo);
            }
            return Err(WorkspaceError::JjCommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Err(WorkspaceError::JjCommandFailed(
                "Empty output from jj root".to_string(),
            ));
        }

        Ok(Self {
            repo_root: PathBuf::from(trimmed),
        })
    }

    /// Returns the repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Checks if a Jujutsu workspace with `workspace_name` is registered.
    pub fn is_workspace_registered(&self, workspace_name: &str) -> Result<bool, WorkspaceError> {
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(WorkspaceError::NotInJjRepo);
            }
            return Err(WorkspaceError::JjCommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| {
            line.split_once(':')
                .map(|(name, _)| name.trim() == workspace_name)
                .unwrap_or(false)
        }))
    }

    /// Adds a new workspace to Jujutsu.
    pub fn add_workspace(
        &self,
        rel_workspace_path: &Path,
        workspace_name: &str,
    ) -> Result<(), WorkspaceError> {
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "add"])
            .arg(rel_workspace_path)
            .args(["--name", workspace_name])
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(WorkspaceError::NotInJjRepo);
            }
            return Err(WorkspaceError::JjCommandFailed(stderr.trim().to_string()));
        }

        Ok(())
    }

    /// Forgets a workspace in Jujutsu.
    pub fn forget_workspace(&self, workspace_name: &str) -> Result<(), WorkspaceError> {
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "forget", workspace_name])
            .current_dir(&self.repo_root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(WorkspaceError::NotInJjRepo);
            }
            return Err(WorkspaceError::JjCommandFailed(stderr.trim().to_string()));
        }

        Ok(())
    }

    /// Checks if a Jujutsu workspace exists on disk by checking for `.jj`.
    #[must_use]
    pub fn workspace_exists(&self, workspace_path: &Path) -> bool {
        workspace_path.join(".jj").exists()
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

    #[googletest::test]
    fn from_dir_at_repo_root_discovers_jj_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let jj = Jj::from_dir(repo_root).expect("from_dir");
        expect_that!(jj.repo_root(), eq(repo_root));
    }

    #[googletest::test]
    fn from_dir_in_deep_subdirectory_discovers_jj_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let sub = repo_root.join("deep").join("nested");
        std::fs::create_dir_all(&sub).expect("create deep dir");

        let jj = Jj::from_dir(&sub).expect("from_dir in sub");
        expect_that!(jj.repo_root(), eq(repo_root));
    }

    #[googletest::test]
    fn from_dir_outside_repo_returns_not_in_jj_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = Jj::from_dir(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInJjRepo)))
        );
    }

    #[googletest::test]
    fn jj_workspace_lifecycle_operations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let jj = Jj::from_dir(repo_root).expect("from_dir");
        let ws_name = "test-jj-ws";
        let rel_ws_path = Path::new(".workspaces").join(ws_name);
        let abs_ws_path = repo_root.join(&rel_ws_path);

        expect_that!(
            jj.is_workspace_registered(ws_name).expect("is_registered"),
            is_false()
        );
        expect_that!(jj.workspace_exists(&abs_ws_path), is_false());

        std::fs::create_dir_all(repo_root.join(".workspaces")).expect("create .workspaces");
        jj.add_workspace(&rel_ws_path, ws_name)
            .expect("add_workspace");

        expect_that!(
            jj.is_workspace_registered(ws_name).expect("is_registered"),
            is_true()
        );
        expect_that!(jj.workspace_exists(&abs_ws_path), is_true());

        jj.forget_workspace(ws_name).expect("forget_workspace");
        expect_that!(
            jj.is_workspace_registered(ws_name).expect("is_registered"),
            is_false()
        );
    }
}
