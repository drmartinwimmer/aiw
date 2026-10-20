use clapback::{Command, CommandError, CommandExt as _, CommandOutput};
use std::path::{Path, PathBuf};

/// Errors that can occur when executing `jj` commands.
#[derive(Debug, thiserror::Error)]
pub enum JjError {
    #[error("Not inside a Jujutsu repository")]
    NotInRepo,
    #[error("Failed to execute Jujutsu command: {0}")]
    CommandFailed(String),
    #[error("I/O error while executing Jujutsu: {0}")]
    Io(#[from] std::io::Error),
}

impl From<CommandError> for JjError {
    fn from(err: CommandError) -> Self {
        match err {
            CommandError::ProgramNotFound(_) => Self::NotInRepo,
            CommandError::ExecutionFailed { stderr, stdout, .. } => {
                Self::CommandFailed(if stderr.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    stderr.trim().to_string()
                })
            }
            CommandError::Io(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
                Self::NotInRepo
            }
            CommandError::Io(io_err) => Self::Io(io_err),
        }
    }
}

#[derive(Command, Debug, Clone, PartialEq, Eq)]
#[command(program = "jj", arg = "--no-pager", ensure_user_path)]
struct JjCli {
    #[command(current_dir)]
    current_dir: Option<PathBuf>,

    #[command(subcommand)]
    subcommand: JjSubcommand,
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum JjSubcommand {
    #[command(subcommand)]
    Workspace(JjWorkspaceAction),
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum JjWorkspaceAction {
    Root {
        #[arg(long)]
        name: Option<String>,
    },
    List {
        #[arg(short = 'T')]
        template: Option<String>,
    },
    Add {
        path: PathBuf,
        #[arg(long)]
        name: String,
    },
    Forget {
        name: String,
    },
}

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

    fn exec(&self, action: JjWorkspaceAction) -> Result<CommandOutput, JjError> {
        Ok(JjCli {
            current_dir: self.current_dir.clone(),
            subcommand: JjSubcommand::Workspace(action),
        }
        .output_unchecked()?)
    }

    /// Runs `jj workspace root [--name <name>]` and returns the workspace root.
    pub fn workspace_root(&self, name: Option<&str>) -> Result<PathBuf, JjError> {
        let output = self.exec(JjWorkspaceAction::Root {
            name: name.map(str::to_string),
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(JjError::NotInRepo);
            }
            return Err(JjError::CommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Err(JjError::CommandFailed(
                "Empty output from jj workspace root".to_string(),
            ));
        }

        let path = PathBuf::from(trimmed);
        Ok(path.canonicalize().unwrap_or(path))
    }

    /// Runs `jj workspace list` and parses entries into [`JjWorkspaceEntry`].
    pub fn workspace_list(&self) -> Result<Vec<JjWorkspaceEntry>, JjError> {
        let templated = self.exec(JjWorkspaceAction::List {
            template: Some(r#"name ++ "\t" ++ root ++ "\n""#.to_string()),
        })?;
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
            return Err(JjError::NotInRepo);
        }

        // Fallback to plain workspace list if template output failed
        let fallback = self.exec(JjWorkspaceAction::List { template: None })?;
        if !fallback.status.success() {
            let fb_stderr = String::from_utf8_lossy(&fallback.stderr);
            let fb_lower = fb_stderr.to_lowercase();
            if fb_lower.contains("no jj repo") || fb_lower.contains("there is no jj repo") {
                return Err(JjError::NotInRepo);
            }
            return Err(JjError::CommandFailed(fb_stderr.trim().to_string()));
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
    pub fn workspace_add(&self, path: &Path, name: &str) -> Result<(), JjError> {
        let output = self.exec(JjWorkspaceAction::Add {
            path: path.to_path_buf(),
            name: name.to_string(),
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(JjError::NotInRepo);
            }
            return Err(JjError::CommandFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    /// Runs `jj workspace forget <name>`.
    pub fn workspace_forget(&self, name: &str) -> Result<(), JjError> {
        let output = self.exec(JjWorkspaceAction::Forget {
            name: name.to_string(),
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("no jj repo") || stderr_lower.contains("there is no jj repo") {
                return Err(JjError::NotInRepo);
            }
            return Err(JjError::CommandFailed(stderr.trim().to_string()));
        }
        Ok(())
    }
}
