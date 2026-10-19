use clapback::{Command, CommandError, CommandExt as _, CommandOutput};
use std::path::{Path, PathBuf};

/// Errors that can occur when executing `git` commands.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Not inside a Git repository")]
    NotInRepo,
    #[error("Failed to execute Git command: {0}")]
    CommandFailed(String),
    #[error("I/O error while executing Git: {0}")]
    Io(#[from] std::io::Error),
}

impl From<CommandError> for GitError {
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
#[command(program = "git", arg = "--no-pager", ensure_user_path)]
struct GitCli {
    #[command(current_dir)]
    current_dir: Option<PathBuf>,

    #[command(subcommand)]
    subcommand: GitSubcommand,
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum GitSubcommand {
    RevParse {
        #[arg(long)]
        show_toplevel: bool,
    },
    #[command(subcommand)]
    Worktree(GitWorktreeAction),
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum GitWorktreeAction {
    List {
        #[arg(long)]
        porcelain: bool,
    },
    Prune,
    Add {
        path: PathBuf,
    },
    Remove {
        #[arg(long)]
        force: bool,
        path: PathBuf,
    },
}

/// Parsed entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreeEntry {
    pub path: PathBuf,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Builder for executing `git` commands and parsing output.
#[derive(Debug, Default, Clone)]
pub struct GitCommand {
    current_dir: Option<PathBuf>,
}

impl GitCommand {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.current_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    fn exec(&self, subcommand: GitSubcommand) -> Result<CommandOutput, GitError> {
        Ok(GitCli {
            current_dir: self.current_dir.clone(),
            subcommand,
        }
        .output_unchecked()?)
    }

    /// Runs `git rev-parse --show-toplevel` and returns the worktree root.
    pub fn show_toplevel(&self) -> Result<PathBuf, GitError> {
        let output = self.exec(GitSubcommand::RevParse {
            show_toplevel: true,
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_lowercase().contains("not a git repository") {
                return Err(GitError::NotInRepo);
            }
            return Err(GitError::CommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Err(GitError::CommandFailed(
                "Empty output from git rev-parse --show-toplevel".to_string(),
            ));
        }

        let path = PathBuf::from(trimmed);
        Ok(path.canonicalize().unwrap_or(path))
    }

    /// Runs `git worktree list --porcelain` and parses each entry into [`GitWorktreeEntry`].
    pub fn worktree_list(&self) -> Result<Vec<GitWorktreeEntry>, GitError> {
        let output = self.exec(GitSubcommand::Worktree(GitWorktreeAction::List {
            porcelain: true,
        }))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_lowercase().contains("not a git repository") {
                return Err(GitError::NotInRepo);
            }
            return Err(GitError::CommandFailed(stderr.trim().to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parse_porcelain_worktree_list(&stdout))
    }

    /// Parses the raw output of `git worktree list --porcelain`.
    pub fn parse_porcelain_worktree_list(stdout: &str) -> Vec<GitWorktreeEntry> {
        let mut entries = Vec::new();
        let mut current_entry: Option<GitWorktreeEntry> = None;

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                entries.extend(current_entry.take());
                continue;
            }

            if let Some(path_str) = line.strip_prefix("worktree ") {
                entries.extend(current_entry.take());
                let raw_path = PathBuf::from(path_str.trim());
                let canonical_path = raw_path.canonicalize().unwrap_or(raw_path);
                current_entry = Some(GitWorktreeEntry {
                    path: canonical_path,
                    head: None,
                    branch: None,
                    bare: false,
                    detached: false,
                    locked: false,
                    prunable: false,
                });
            } else if let Some(ref mut entry) = current_entry {
                if let Some(head) = line.strip_prefix("HEAD ") {
                    entry.head = Some(head.trim().to_string());
                } else if let Some(branch) = line.strip_prefix("branch ") {
                    entry.branch = Some(branch.trim().to_string());
                } else {
                    match line {
                        "bare" => entry.bare = true,
                        "detached" => entry.detached = true,
                        "locked" => entry.locked = true,
                        "prunable" => entry.prunable = true,
                        _ if line.starts_with("locked ") => entry.locked = true,
                        _ if line.starts_with("prunable ") => entry.prunable = true,
                        _ => {}
                    }
                }
            }
        }

        entries.extend(current_entry);

        entries
    }

    /// Runs `git worktree prune`.
    pub fn worktree_prune(&self) -> Result<(), GitError> {
        let output = self.exec(GitSubcommand::Worktree(GitWorktreeAction::Prune))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_lowercase().contains("not a git repository") {
                return Err(GitError::NotInRepo);
            }
            return Err(GitError::CommandFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    /// Runs `git worktree add <path>`.
    pub fn worktree_add(&self, path: &Path) -> Result<(), GitError> {
        let output = self.exec(GitSubcommand::Worktree(GitWorktreeAction::Add {
            path: path.to_path_buf(),
        }))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_lowercase().contains("not a git repository") {
                return Err(GitError::NotInRepo);
            }
            return Err(GitError::CommandFailed(stderr.trim().to_string()));
        }
        Ok(())
    }

    /// Runs `git worktree remove --force <path>`.
    pub fn worktree_force_remove(&self, path: &Path) -> Result<(), GitError> {
        let output = self.exec(GitSubcommand::Worktree(GitWorktreeAction::Remove {
            force: true,
            path: path.to_path_buf(),
        }))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr_lower = stderr.to_lowercase();
            if stderr_lower.contains("not a git repository") {
                return Err(GitError::NotInRepo);
            }
            if !stderr_lower.contains("is not a working tree")
                && !stderr_lower.contains("not a valid path")
            {
                return Err(GitError::CommandFailed(stderr.trim().to_string()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn parse_porcelain_worktree_list_parses_multiple_worktrees() {
        let sample = "\
worktree /home/user/repo
HEAD abcdef123456
branch refs/heads/main

worktree /home/user/repo/.workspaces/feature-1
HEAD 123456abcdef
branch refs/heads/feature-1

worktree /home/user/repo/.workspaces/detached-ws
HEAD 999999999999
detached

worktree /home/user/repo/.workspaces/bare-ws
bare
";
        let entries = GitCommand::parse_porcelain_worktree_list(sample);
        expect_that!(entries.len(), eq(4));
        let [e0, e1, e2, e3] = entries.as_slice() else {
            return;
        };
        expect_that!(e0.path, eq(&PathBuf::from("/home/user/repo")));
        expect_that!(e0.branch, some(eq("refs/heads/main")));
        expect_that!(e0.bare, is_false());

        expect_that!(
            e1.path,
            eq(&PathBuf::from("/home/user/repo/.workspaces/feature-1"))
        );
        expect_that!(e1.branch, some(eq("refs/heads/feature-1")));

        expect_that!(e2.detached, is_true());

        expect_that!(e3.bare, is_true());
    }
}
