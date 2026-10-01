use crate::tools::ensure_user_profile_bin_paths;
use crate::workspace::WorkspaceError;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parsed entry representing a Jujutsu workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JjWorkspaceEntry {
    pub name: String,
    pub root: PathBuf,
}

/// Builder for executing `jj` commands and parsing output.
#[derive(Debug, Default, Clone)]
pub struct JjCommand {
    current_dir: Option<PathBuf>,
}

impl JjCommand {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.current_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    fn build_cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("jj");
        cmd.arg("--no-pager");
        cmd.args(args);
        if let Some(ref dir) = self.current_dir {
            cmd.current_dir(dir);
        }
        ensure_user_profile_bin_paths(&mut cmd);
        cmd
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output, WorkspaceError> {
        self.build_cmd(args).output().map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                WorkspaceError::NotInJjRepo
            } else {
                WorkspaceError::Io(err)
            }
        })
    }

    /// Runs `jj workspace root [--name <name>]` and returns the workspace root.
    pub fn workspace_root(&self, name: Option<&str>) -> Result<PathBuf, WorkspaceError> {
        let mut args = vec!["workspace", "root"];
        if let Some(n) = name {
            args.extend(["--name", n]);
        }
        let output = self.run(&args)?;
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
                "Empty output from jj workspace root".to_string(),
            ));
        }

        let path = PathBuf::from(trimmed);
        Ok(path.canonicalize().unwrap_or(path))
    }

    /// Runs `jj workspace list` and parses entries into [`JjWorkspaceEntry`].
    pub fn workspace_list(&self) -> Result<Vec<JjWorkspaceEntry>, WorkspaceError> {
        let templated =
            self.run(&["workspace", "list", "-T", r#"name ++ "\t" ++ root ++ "\n""#])?;
        if templated.status.success() {
            let stdout = String::from_utf8_lossy(&templated.stdout);
            let mut entries = Vec::new();
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some((name, root_str)) = line.split_once('\t') {
                    let root = PathBuf::from(root_str.trim());
                    let canonical = root.canonicalize().unwrap_or(root);
                    entries.push(JjWorkspaceEntry {
                        name: name.trim().to_string(),
                        root: canonical,
                    });
                }
            }
            return Ok(entries);
        }

        let stderr = String::from_utf8_lossy(&templated.stderr);
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
            return Err(WorkspaceError::NotInJjRepo);
        }

        // Fallback to plain workspace list if template output failed
        let fallback = self.run(&["workspace", "list"])?;
        if !fallback.status.success() {
            let fb_stderr = String::from_utf8_lossy(&fallback.stderr);
            let fb_lower = fb_stderr.to_lowercase();
            if fb_lower.contains("no jj repo") || fb_lower.contains("there is no jj repo") {
                return Err(WorkspaceError::NotInJjRepo);
            }
            return Err(WorkspaceError::JjCommandFailed(
                fb_stderr.trim().to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&fallback.stdout);
        let mut entries = Vec::new();
        let base_dir = self
            .current_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        for line in stdout.lines() {
            if let Some((name, _)) = line.split_once(':') {
                let name = name.trim().to_string();
                let root = base_dir.join(".workspaces").join(&name);
                entries.push(JjWorkspaceEntry { name, root });
            }
        }
        Ok(entries)
    }

    /// Runs `jj workspace add <path> --name <name>`.
    pub fn workspace_add(&self, path: &Path, name: &str) -> Result<(), WorkspaceError> {
        let path_str = path.to_string_lossy();
        let output = self.run(&["workspace", "add", &path_str, "--name", name])?;
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

    /// Runs `jj workspace forget <name>`.
    pub fn workspace_forget(&self, name: &str) -> Result<(), WorkspaceError> {
        let output = self.run(&["workspace", "forget", name])?;
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
}
