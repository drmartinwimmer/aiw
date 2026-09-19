use std::path::{Path, PathBuf};
use crate::workspace::WorkspaceError;

pub(crate) fn normalize_search_dir(start_dir: &Path) -> Result<PathBuf, std::io::Error> {
    let abs_dir = if start_dir.is_absolute() {
        start_dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(start_dir)
    };

    Ok(if abs_dir.is_file() {
        abs_dir.parent().map(Path::to_path_buf).unwrap_or(abs_dir)
    } else {
        abs_dir
    })
}

/// Finds the root directory of the enclosing Git repository.
///
/// If inside a linked worktree, this returns the main repository root where
/// shared `.workspaces` are housed.
pub fn find_root(start_dir: &Path) -> Result<PathBuf, WorkspaceError> {
    let dir = normalize_search_dir(start_dir)?;
    if !dir.exists() {
        return Err(WorkspaceError::NotInGitRepo);
    }
    execute_git_root(&dir)
}

fn execute_git_root(dir: &Path) -> Result<PathBuf, WorkspaceError> {
    let output = std::process::Command::new("git")
        .args(["--no-pager", "rev-parse", "--git-common-dir"])
        .current_dir(dir)
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
        dir.join(path)
    };

    let normalized = abs_common_dir.canonicalize().unwrap_or(abs_common_dir);
    if normalized.file_name() == Some(std::ffi::OsStr::new(".git"))
        && let Some(parent) = normalized.parent()
    {
        return Ok(parent.to_path_buf());
    }

    // Fallback to git rev-parse --show-toplevel
    let top_output = std::process::Command::new("git")
        .args(["--no-pager", "rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()?;

    if top_output.status.success() {
        let top_str = String::from_utf8_lossy(&top_output.stdout);
        let top_trimmed = top_str.trim();
        if !top_trimmed.is_empty() {
            let top_path = PathBuf::from(top_trimmed);
            return Ok(top_path.canonicalize().unwrap_or(top_path));
        }
    }

    Ok(normalized)
}

/// Checks if a Git worktree for `workspace_name` is currently registered.
pub fn is_workspace_registered(
    repo_root: &Path,
    workspace_name: &str,
) -> Result<bool, WorkspaceError> {
    // Prune stale worktrees first so missing directories are not reported as registered
    drop(
        std::process::Command::new("git")
            .args(["--no-pager", "worktree", "prune"])
            .current_dir(repo_root)
            .output(),
    );

    let output = std::process::Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("not a git repository") {
            return Err(WorkspaceError::NotInGitRepo);
        }
        return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
    }

    let target_path = repo_root.join(".workspaces").join(workspace_name);
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
    repo_root: &Path,
    rel_workspace_path: &Path,
    _workspace_name: &str,
) -> Result<(), WorkspaceError> {
    // Prune any stale metadata before creating new worktree
    drop(
        std::process::Command::new("git")
            .args(["--no-pager", "worktree", "prune"])
            .current_dir(repo_root)
            .output(),
    );

    let output = std::process::Command::new("git")
        .args(["--no-pager", "worktree", "add"])
        .arg(rel_workspace_path)
        .current_dir(repo_root)
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
pub fn forget_workspace(
    repo_root: &Path,
    workspace_name: &str,
) -> Result<(), WorkspaceError> {
    let ws_path = repo_root.join(".workspaces").join(workspace_name);

    let output = std::process::Command::new("git")
        .args(["--no-pager", "worktree", "remove", "--force"])
        .arg(&ws_path)
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("not a git repository") {
            return Err(WorkspaceError::NotInGitRepo);
        }
        // If the path was not registered as a working tree, ignore error and prune
        if !stderr_lower.contains("is not a working tree")
            && !stderr_lower.contains("not a valid path")
        {
            return Err(WorkspaceError::GitCommandFailed(stderr.trim().to_string()));
        }
    }

    drop(
        std::process::Command::new("git")
            .args(["--no-pager", "worktree", "prune"])
            .current_dir(repo_root)
            .output(),
    );

    Ok(())
}

/// Checks if a Git workspace exists on disk by checking for `.git`.
#[must_use]
pub fn workspace_exists(workspace_path: &Path) -> bool {
    workspace_path.join(".git").exists()
}

/// Returns `true` if `dir` is inside a Git repository.
#[must_use]
pub fn is_repo(dir: &Path) -> bool {
    find_root(dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

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

        let readme = path.join("README.md");
        std::fs::write(&readme, "# Test Repo\n").expect("write README");
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
    fn find_root_from_root_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let found = find_root(repo_root).expect("find_root");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(found, eq(&canonical_root));
    }

    #[googletest::test]
    fn find_root_from_deep_subdirectory_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let sub = repo_root.join("sub1").join("sub2");
        std::fs::create_dir_all(&sub).expect("create_dir_all");

        let found = find_root(&sub).expect("find_root");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(found, eq(&canonical_root));
    }

    #[googletest::test]
    fn find_root_from_worktree_returns_main_repo_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let rel_ws = Path::new(".workspaces").join("nested-ws");
        let ws_path = repo_root.join(&rel_ws);
        std::fs::create_dir_all(repo_root.join(".workspaces")).expect("create .workspaces");
        add_workspace(repo_root, &rel_ws, "nested-ws").expect("add_workspace");

        let found = find_root(&ws_path).expect("find_root inside worktree");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(found, eq(&canonical_root));
    }

    #[googletest::test]
    fn find_root_outside_repo_returns_not_in_git_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = find_root(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInGitRepo)))
        );
    }

    #[googletest::test]
    fn git_workspace_lifecycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let ws_name = "git-ws-test";
        let rel_path = Path::new(".workspaces").join(ws_name);
        let abs_path = repo_root.join(&rel_path);

        expect_that!(is_workspace_registered(repo_root, ws_name).expect("is_registered"), is_false());
        expect_that!(workspace_exists(&abs_path), is_false());

        std::fs::create_dir_all(repo_root.join(".workspaces")).expect("create .workspaces");
        add_workspace(repo_root, &rel_path, ws_name).expect("add_workspace");

        expect_that!(is_workspace_registered(repo_root, ws_name).expect("is_registered"), is_true());
        expect_that!(workspace_exists(&abs_path), is_true());

        forget_workspace(repo_root, ws_name).expect("forget_workspace");
        expect_that!(is_workspace_registered(repo_root, ws_name).expect("is_registered"), is_false());
    }

    #[googletest::test]
    fn re_adding_after_forget_reuses_existing_branch() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let ws_name = "readd-ws";
        let rel_path = Path::new(".workspaces").join(ws_name);
        std::fs::create_dir_all(repo_root.join(".workspaces")).expect("create .workspaces");

        add_workspace(repo_root, &rel_path, ws_name).expect("first add");
        forget_workspace(repo_root, ws_name).expect("forget");

        // Re-adding the worktree with the same branch name must succeed
        let second_add = add_workspace(repo_root, &rel_path, ws_name);
        expect_that!(second_add, ok(anything()));
        expect_that!(is_workspace_registered(repo_root, ws_name).expect("is_registered"), is_true());
    }
}
