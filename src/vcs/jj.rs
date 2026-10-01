use super::normalize_search_dir;
use crate::tools::JjCommand;
use crate::workspace::WorkspaceError;
use std::path::{Path, PathBuf};

/// Encapsulates Jujutsu repository operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jj {
    repo_root: PathBuf,
    workspace_root: PathBuf,
}

impl Jj {
    /// Attempts to discover a Jujutsu repository enclosing `dir`.
    pub fn from_dir(dir: &Path) -> Result<Self, WorkspaceError> {
        let normalized = normalize_search_dir(dir)?;
        if !normalized.exists() {
            return Err(WorkspaceError::NotInJjRepo);
        }

        let workspace_root = JjCommand::new()
            .current_dir(&normalized)
            .workspace_root(None)?;

        // The default workspace always corresponds to the root Jujutsu repository
        let repo_root = JjCommand::new()
            .current_dir(&workspace_root)
            .workspace_root(Some("default"))
            .unwrap_or_else(|_| workspace_root.clone());

        Ok(Self {
            repo_root,
            workspace_root,
        })
    }

    /// Returns the repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Returns the active workspace root path.
    #[must_use]
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Checks if a Jujutsu workspace with `workspace_name` is registered.
    pub fn is_workspace_registered(&self, workspace_name: &str) -> Result<bool, WorkspaceError> {
        let entries = JjCommand::new()
            .current_dir(&self.repo_root)
            .workspace_list()?;

        Ok(entries.iter().any(|entry| entry.name == workspace_name))
    }

    /// Adds a new workspace to Jujutsu.
    pub fn add_workspace(
        &self,
        rel_workspace_path: &Path,
        workspace_name: &str,
    ) -> Result<(), WorkspaceError> {
        JjCommand::new()
            .current_dir(&self.repo_root)
            .workspace_add(rel_workspace_path, workspace_name)
    }

    /// Forgets a workspace in Jujutsu.
    pub fn forget_workspace(&self, workspace_name: &str) -> Result<(), WorkspaceError> {
        JjCommand::new()
            .current_dir(&self.repo_root)
            .workspace_forget(workspace_name)
    }

    /// Checks if a Jujutsu workspace exists on disk by checking for `.jj`.
    #[must_use]
    pub fn workspace_exists(&self, workspace_path: &Path) -> bool {
        workspace_path.join(".jj").exists()
    }

    /// Lists all available workspaces in the Jujutsu repository.
    pub fn list_workspaces(&self) -> Result<Vec<String>, WorkspaceError> {
        let entries = JjCommand::new()
            .current_dir(&self.repo_root)
            .workspace_list()?;

        let workspaces_dir = self.repo_root.join(".workspaces");
        let workspaces_dir_canonical = workspaces_dir.canonicalize().ok();

        let workspaces = entries
            .into_iter()
            .filter(|entry| entry.name != "default")
            .filter(|entry| {
                entry
                    .root
                    .parent()
                    .map(|parent| {
                        parent == workspaces_dir
                            || (workspaces_dir_canonical.is_some()
                                && parent.canonicalize().ok() == workspaces_dir_canonical)
                            || parent.ends_with(".workspaces")
                    })
                    .unwrap_or_default()
            })
            .filter_map(|entry| {
                crate::workspace::validate_workspace_name(&entry.name).ok()?;
                self.workspace_exists(&entry.root).then_some(entry.name)
            })
            .collect();

        Ok(workspaces)
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

    #[googletest::test]
    fn jj_list_workspaces_lists_added_workspaces_and_ignores_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let jj = Jj::from_dir(repo_root).expect("from_dir");
        let initial = jj.list_workspaces().expect("list initial");
        expect_that!(initial, is_empty());

        let rel1 = Path::new(".workspaces").join("ws-1");
        let rel2 = Path::new(".workspaces").join("ws-2");
        std::fs::create_dir_all(repo_root.join(".workspaces")).expect("create .workspaces");
        jj.add_workspace(&rel1, "ws-1").expect("add ws-1");
        jj.add_workspace(&rel2, "ws-2").expect("add ws-2");

        let listed = jj.list_workspaces().expect("list after adding");
        expect_that!(listed, unordered_elements_are![eq("ws-1"), eq("ws-2")]);

        // When inside a workspace, Jj::from_dir still resolves to the main repo root
        let ws1_path = repo_root.join(&rel1);
        let jj_from_ws = Jj::from_dir(&ws1_path).expect("from_dir inside ws");
        expect_that!(jj_from_ws.repo_root(), eq(repo_root));
        let listed_from_ws = jj_from_ws.list_workspaces().expect("list from ws");
        expect_that!(
            listed_from_ws,
            unordered_elements_are![eq("ws-1"), eq("ws-2")]
        );

        // After forgetting ws-1, only ws-2 remains
        jj.forget_workspace("ws-1").expect("forget ws-1");
        let listed_after_forget = jj.list_workspaces().expect("list after forget");
        expect_that!(listed_after_forget, elements_are![eq("ws-2")]);
    }
}
