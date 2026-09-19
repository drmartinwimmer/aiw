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

/// Finds the root directory of the enclosing Jujutsu repository by invoking `jj --no-pager root`.
pub fn find_root(start_dir: &Path) -> Result<PathBuf, WorkspaceError> {
    let dir = normalize_search_dir(start_dir)?;
    if !dir.exists() {
        return Err(WorkspaceError::NotInJjRepo);
    }
    execute_jj_root(&dir)
}

fn execute_jj_root(dir: &Path) -> Result<PathBuf, WorkspaceError> {
    let output = std::process::Command::new("jj")
        .args(["--no-pager", "root"])
        .current_dir(dir)
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

    Ok(PathBuf::from(trimmed))
}

/// Checks if a Jujutsu workspace with `workspace_name` is registered in Jujutsu.
pub fn is_workspace_registered(
    repo_root: &Path,
    workspace_name: &str,
) -> Result<bool, WorkspaceError> {
    let output = std::process::Command::new("jj")
        .args(["--no-pager", "workspace", "list"])
        .current_dir(repo_root)
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
    repo_root: &Path,
    rel_workspace_path: &Path,
    workspace_name: &str,
) -> Result<(), WorkspaceError> {
    let output = std::process::Command::new("jj")
        .args(["--no-pager", "workspace", "add"])
        .arg(rel_workspace_path)
        .args(["--name", workspace_name])
        .current_dir(repo_root)
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
pub fn forget_workspace(
    repo_root: &Path,
    workspace_name: &str,
) -> Result<(), WorkspaceError> {
    let output = std::process::Command::new("jj")
        .args(["--no-pager", "workspace", "forget", workspace_name])
        .current_dir(repo_root)
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
pub fn workspace_exists(workspace_path: &Path) -> bool {
    workspace_path.join(".jj").exists()
}

/// Returns `true` if `dir` is inside a Jujutsu repository.
#[must_use]
pub fn is_repo(dir: &Path) -> bool {
    find_root(dir).is_ok()
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

    #[googletest::test]
    fn find_root_from_root_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let found = find_root(repo_root).expect("find_root");
        expect_that!(found, eq(repo_root));
    }

    #[googletest::test]
    fn find_root_from_deep_subdirectory_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let sub = repo_root.join("sub1").join("sub2");
        std::fs::create_dir_all(&sub).expect("create_dir_all");

        let found = find_root(&sub).expect("find_root");
        expect_that!(found, eq(repo_root));
    }

    #[googletest::test]
    fn find_root_outside_repo_returns_not_in_jj_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = find_root(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInJjRepo)))
        );
    }

    #[googletest::test]
    fn jj_workspace_lifecycle() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let ws_name = "jj-ws-test";
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
}
