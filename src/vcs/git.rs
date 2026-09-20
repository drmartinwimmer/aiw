use std::path::{Path, PathBuf};
use crate::workspace::WorkspaceError;
use super::normalize_search_dir;

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
            .args(["--no-pager", "rev-parse", "--git-common-dir"])
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
                "Empty output from git rev-parse --git-common-dir".to_string(),
            ));
        }

        let path = PathBuf::from(trimmed);
        let abs_common_dir = if path.is_absolute() {
            path
        } else {
            normalized.join(path)
        };

        let abs_norm = abs_common_dir.canonicalize().unwrap_or(abs_common_dir);
        if abs_norm.file_name() == Some(std::ffi::OsStr::new(".git"))
            && let Some(parent) = abs_norm.parent()
        {
            return Ok(Self {
                repo_root: parent.to_path_buf(),
            });
        }

        // Fallback to git rev-parse --show-toplevel
        let top_output = std::process::Command::new("git")
            .args(["--no-pager", "rev-parse", "--show-toplevel"])
            .current_dir(&normalized)
            .output()?;

        if top_output.status.success() {
            let top_str = String::from_utf8_lossy(&top_output.stdout);
            let top_trimmed = top_str.trim();
            if !top_trimmed.is_empty() {
                let top_path = PathBuf::from(top_trimmed);
                return Ok(Self {
                    repo_root: top_path.canonicalize().unwrap_or(top_path),
                });
            }
        }

        Ok(Self {
            repo_root: abs_norm,
        })
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
}
