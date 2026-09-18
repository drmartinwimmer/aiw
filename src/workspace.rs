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
    let output = std::process::Command::new("jj")
        .args(["--no-pager", "root"])
        .current_dir(dir)
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

/// Encapsulates state and operations for a Jujutsu workspace under `<repo_root>/.workspaces/<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    repo_root: PathBuf,
    name: String,
    path: PathBuf,
}

impl Workspace {
    /// Creates a new `Workspace` instance for a given repository root and workspace name.
    ///
    /// Validates that the name is valid and that `repo_root` points inside a Jujutsu repository.
    pub fn new(repo_root: &Path, name: &str) -> Result<Self, WorkspaceError> {
        validate_workspace_name(name)?;

        let abs_repo_root = if repo_root.is_absolute() {
            repo_root.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(repo_root)
        } else {
            repo_root.to_path_buf()
        };

        if !abs_repo_root.join(".jj").exists() {
            return Err(WorkspaceError::NotInJjRepo);
        }

        let path = abs_repo_root.join(".workspaces").join(name);
        Ok(Self {
            repo_root: abs_repo_root,
            name: name.to_string(),
            path,
        })
    }

    /// Discovers the Jujutsu repository root enclosing `dir` and returns a `Workspace` handle.
    pub fn from_dir(dir: &Path, name: &str) -> Result<Self, WorkspaceError> {
        let repo_root = find_jj_root(dir)?;
        Self::new(&repo_root, name)
    }

    /// Returns the workspace name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the workspace directory path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the enclosing repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Returns `true` if the workspace currently exists on disk and contains a `.jj` directory.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.join(".jj").exists()
    }

    /// Ensures the workspace exists on disk and is registered in Jujutsu.
    ///
    /// Returns `Ok(true)` if the workspace was newly created, or `Ok(false)` if it already existed.
    pub fn ensure(&self) -> Result<bool, WorkspaceError> {
        if self.exists() {
            return Ok(false);
        }

        let workspaces_dir = self.repo_root.join(".workspaces");
        std::fs::create_dir_all(&workspaces_dir)?;

        let rel_workspace_path = Path::new(".workspaces").join(&self.name);
        execute_jj_workspace_add(&self.repo_root, &rel_workspace_path, &self.name)?;

        Ok(true)
    }

    /// Forgets the workspace in Jujutsu by invoking `jj --no-pager workspace forget <workspace-name>`.
    pub fn forget(&self) -> Result<(), WorkspaceError> {
        execute_jj_workspace_forget(&self.repo_root, &self.name)
    }
}

/// Returns `true` if a workspace named `workspace_name` already exists under `<repo_root>/.workspaces/<workspace-name>`.
#[must_use]
pub fn workspace_exists(repo_root: &Path, workspace_name: &str) -> bool {
    Workspace::new(repo_root, workspace_name).is_ok_and(|ws| ws.exists())
}

/// Ensures a workspace named `workspace_name` exists under `<repo_root>/.workspaces/<workspace-name>`.
/// If it already exists, returns its absolute path.
/// If not, invokes `jj --no-pager workspace add .workspaces/<workspace-name> --name <workspace-name>`.
pub fn ensure_workspace(repo_root: &Path, workspace_name: &str) -> Result<PathBuf, WorkspaceError> {
    let ws = Workspace::new(repo_root, workspace_name)?;
    ws.ensure()?;
    Ok(ws.path().to_path_buf())
}

fn execute_jj_workspace_add(
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

/// Forgets a workspace named `workspace_name` in Jujutsu by invoking
/// `jj --no-pager workspace forget <workspace-name>`.
pub fn forget_workspace(repo_root: &Path, workspace_name: &str) -> Result<(), WorkspaceError> {
    let ws = Workspace::new(repo_root, workspace_name)?;
    ws.forget()
}

fn execute_jj_workspace_forget(
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
    fn workspace_exists_checks_workspace_presence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let ws_name = "ws-check";
        expect_that!(workspace_exists(repo_root, ws_name), is_false());

        let _ = ensure_workspace(repo_root, ws_name).expect("ensure");
        expect_that!(workspace_exists(repo_root, ws_name), is_true());
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

    #[googletest::test]
    fn workspace_struct_lifecycle_and_methods() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_repo(repo_root);

        let ws = Workspace::new(repo_root, "struct-ws").expect("new Workspace");
        expect_that!(ws.name(), eq("struct-ws"));
        expect_that!(ws.repo_root(), eq(repo_root));
        expect_that!(ws.path(), eq(&repo_root.join(".workspaces").join("struct-ws")));
        expect_that!(ws.exists(), is_false());

        // First ensure returns Ok(true) indicating newly created
        let is_new = ws.ensure().expect("ensure workspace");
        expect_that!(is_new, is_true());
        expect_that!(ws.exists(), is_true());

        // Second ensure returns Ok(false) indicating already existed
        let is_new_again = ws.ensure().expect("ensure workspace again");
        expect_that!(is_new_again, is_false());

        // Test from_dir
        let sub = repo_root.join("subdir");
        std::fs::create_dir_all(&sub).expect("create_dir_all");
        let ws_from_sub = Workspace::from_dir(&sub, "from-sub").expect("from_dir");
        expect_that!(ws_from_sub.repo_root(), eq(repo_root));
        expect_that!(ws_from_sub.name(), eq("from-sub"));

        // Forget workspace
        ws.forget().expect("forget workspace");
        let list_output = std::process::Command::new("jj")
            .args(["--no-pager", "workspace", "list"])
            .current_dir(repo_root)
            .output()
            .expect("jj workspace list");
        expect_that!(
            String::from_utf8_lossy(&list_output.stdout).as_ref(),
            not(contains_substring("struct-ws"))
        );
    }
}
