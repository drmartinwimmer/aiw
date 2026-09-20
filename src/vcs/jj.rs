use std::path::{Path, PathBuf};
use crate::workspace::WorkspaceError;
use super::normalize_search_dir;

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
