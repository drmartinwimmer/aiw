use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("Not inside a Jujutsu repository")]
    NotInJjRepo,
    #[error("Failed to execute Jujutsu command: {0}")]
    JjCommandFailed(String),
    #[error("Invalid workspace name: {0}")]
    InvalidWorkspaceName(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Finds the root directory of the enclosing Jujutsu repository by invoking `jj --no-pager root`.
pub fn find_jj_root(start_dir: &Path) -> Result<PathBuf, WorkspaceError> {
    let dir = normalize_search_dir(start_dir)?;
    if !dir.exists() {
        return Err(WorkspaceError::NotInJjRepo);
    }
    execute_jj_root(&dir)
}

fn normalize_search_dir(start_dir: &Path) -> Result<PathBuf, std::io::Error> {
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

fn execute_jj_root(dir: &Path) -> Result<PathBuf, WorkspaceError> {
    let output = match std::process::Command::new("jj")
        .args(["--no-pager", "root"])
        .current_dir(dir)
        .output()
    {
        Ok(out) => out,
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

/// Ensures a workspace named `workspace_name` exists under `<repo_root>/.workspaces/<workspace-name>`.
/// If it already exists, returns its absolute path.
/// If not, invokes `jj --no-pager workspace add .workspaces/<workspace-name> --name <workspace-name>`.
pub fn ensure_workspace(repo_root: &Path, workspace_name: &str) -> Result<PathBuf, WorkspaceError> {
    validate_workspace_name(workspace_name)?;

    let abs_repo_root = if repo_root.is_absolute() {
        repo_root.to_path_buf()
    } else {
        std::env::current_dir()?.join(repo_root)
    };

    if !abs_repo_root.join(".jj").exists() {
        return Err(WorkspaceError::NotInJjRepo);
    }

    let workspace_path = abs_repo_root.join(".workspaces").join(workspace_name);
    if workspace_path.join(".jj").exists() {
        return Ok(workspace_path);
    }

    let workspaces_dir = abs_repo_root.join(".workspaces");
    std::fs::create_dir_all(&workspaces_dir)?;

    let rel_workspace_path = Path::new(".workspaces").join(workspace_name);
    execute_jj_workspace_add(&abs_repo_root, &rel_workspace_path, workspace_name)?;

    Ok(workspace_path)
}

fn execute_jj_workspace_add(
    repo_root: &Path,
    rel_workspace_path: &Path,
    workspace_name: &str,
) -> Result<(), WorkspaceError> {
    let output = match std::process::Command::new("jj")
        .args(["--no-pager", "workspace", "add"])
        .arg(rel_workspace_path)
        .args(["--name", workspace_name])
        .current_dir(repo_root)
        .output()
    {
        Ok(out) => out,
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

    Ok(())
}

/// Forgets a workspace named `workspace_name` in Jujutsu by invoking
/// `jj --no-pager workspace forget <workspace-name>`.
pub fn forget_workspace(repo_root: &Path, workspace_name: &str) -> Result<(), WorkspaceError> {
    validate_workspace_name(workspace_name)?;

    let abs_repo_root = if repo_root.is_absolute() {
        repo_root.to_path_buf()
    } else {
        std::env::current_dir()?.join(repo_root)
    };

    if !abs_repo_root.join(".jj").exists() {
        return Err(WorkspaceError::NotInJjRepo);
    }

    execute_jj_workspace_forget(&abs_repo_root, workspace_name)
}

fn execute_jj_workspace_forget(
    repo_root: &Path,
    workspace_name: &str,
) -> Result<(), WorkspaceError> {
    let output = match std::process::Command::new("jj")
        .args(["--no-pager", "workspace", "forget", workspace_name])
        .current_dir(repo_root)
        .output()
    {
        Ok(out) => out,
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    fn init_test_repo(path: &Path) {
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
    fn find_jj_root_from_root_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let found = find_jj_root(repo_root).expect("find_jj_root");
        expect_that!(found, eq(repo_root));
    }

    #[googletest::test]
    fn find_jj_root_from_deep_subdirectory_returns_root_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

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
    fn ensure_workspace_for_new_name_creates_workspace_and_registers_in_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let ws_name = "ws-alpha";
        let res = ensure_workspace(repo_root, ws_name).expect("ensure_workspace");
        let expected_path = repo_root.join(".workspaces").join(ws_name);

        expect_that!(res, eq(&expected_path));
        expect_that!(expected_path.exists(), is_true());
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
    fn ensure_workspace_when_already_exists_succeeds_idempotently() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let ws_name = "ws-beta";
        let path1 = ensure_workspace(repo_root, ws_name).expect("first ensure");
        let path2 = ensure_workspace(repo_root, ws_name).expect("second ensure");

        expect_that!(path1, eq(&path2));
        expect_that!(path1.join(".jj").exists(), is_true());
    }

    #[googletest::test]
    fn ensure_workspace_outside_repo_returns_not_in_jj_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = ensure_workspace(non_repo, "ws-gamma");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInJjRepo)))
        );
        expect_that!(non_repo.join(".workspaces").exists(), is_false());
    }

    #[googletest::test]
    fn ensure_workspace_with_empty_name_returns_invalid_workspace_name_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let res = ensure_workspace(repo_root, "");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::InvalidWorkspaceName(anything()))))
        );
    }

    #[googletest::test]
    fn ensure_workspace_with_path_traversal_name_returns_invalid_workspace_name_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        for bad_name in [".", "..", "foo/bar", "foo\\bar", "../escape"] {
            let res = ensure_workspace(repo_root, bad_name);
            expect_that!(
                res,
                matches_pattern!(Err(matches_pattern!(WorkspaceError::InvalidWorkspaceName(anything()))))
            );
        }
    }

    #[googletest::test]
    fn forget_workspace_removes_workspace_from_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let ws_name = "ws-to-forget";
        let _ = ensure_workspace(repo_root, ws_name).expect("ensure_workspace");

        let before = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(repo_root)
            .output()
            .expect("jj workspace list");
        expect_that!(
            String::from_utf8_lossy(&before.stdout).as_ref(),
            contains_substring(ws_name)
        );

        forget_workspace(repo_root, ws_name).expect("forget_workspace");

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
    fn forget_workspace_outside_repo_returns_not_in_jj_repo_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = forget_workspace(non_repo, "ws-forget");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInJjRepo)))
        );
    }

    #[googletest::test]
    fn forget_workspace_with_empty_name_returns_invalid_workspace_name_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let res = forget_workspace(repo_root, "");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::InvalidWorkspaceName(anything()))))
        );
    }

    #[googletest::test]
    fn forget_workspace_with_path_traversal_name_returns_invalid_workspace_name_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        for bad_name in [".", "..", "foo/bar", "foo\\bar", "../escape"] {
            let res = forget_workspace(repo_root, bad_name);
            expect_that!(
                res,
                matches_pattern!(Err(matches_pattern!(WorkspaceError::InvalidWorkspaceName(anything()))))
            );
        }
    }
}
