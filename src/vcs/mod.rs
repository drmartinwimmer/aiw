pub(crate) mod git;
pub(crate) mod jj;

use std::path::{Path, PathBuf};
use crate::workspace::WorkspaceError;
pub(crate) use git::Git;
pub(crate) use jj::Jj;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum VcsKind {
    Jj(Jj),
    Git(Git),
}

/// Encapsulates operations for the detected Version Control System (Jujutsu or Git).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vcs {
    kind: VcsKind,
}

impl Vcs {
    /// Initializes a `Vcs` object based on the given path.
    ///
    /// Automatically detects whether the path belongs to a Jujutsu or Git repository.
    /// If a Git repository is nested inside a Jujutsu repository (or vice versa), the more
    /// specific (nearest enclosing) repository is selected.
    pub fn from_path(path: &Path) -> Result<Self, WorkspaceError> {
        let abs_dir = normalize_search_dir(path)?;
        if !abs_dir.exists() {
            return Err(WorkspaceError::NotInRepo);
        }

        let jj_res = Jj::from_dir(&abs_dir);
        let git_res = Git::from_dir(&abs_dir);

        match (jj_res, git_res) {
            (Ok(jj), Ok(git)) => {
                let jj_canonical = jj
                    .repo_root()
                    .canonicalize()
                    .unwrap_or_else(|_| jj.repo_root().to_path_buf());
                let git_canonical = git
                    .repo_root()
                    .canonicalize()
                    .unwrap_or_else(|_| git.repo_root().to_path_buf());

                if git_canonical.starts_with(&jj_canonical) && git_canonical != jj_canonical {
                    // Git repo is nested inside a JJ repo
                    Ok(Self {
                        kind: VcsKind::Git(git),
                    })
                } else if jj_canonical.starts_with(&git_canonical) && jj_canonical != git_canonical {
                    // JJ repo is nested inside a Git repo
                    Ok(Self {
                        kind: VcsKind::Jj(jj),
                    })
                } else {
                    // Colocated repository at the same path: Jujutsu takes precedence
                    Ok(Self {
                        kind: VcsKind::Jj(jj),
                    })
                }
            }
            (Ok(jj), Err(_)) => Ok(Self {
                kind: VcsKind::Jj(jj),
            }),
            (Err(_), Ok(git)) => Ok(Self {
                kind: VcsKind::Git(git),
            }),
            (Err(_), Err(_)) => Err(WorkspaceError::NotInRepo),
        }
    }

    /// Returns the repository root path.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        match &self.kind {
            VcsKind::Jj(jj) => jj.repo_root(),
            VcsKind::Git(git) => git.repo_root(),
        }
    }

    /// Checks if a workspace is registered.
    pub fn is_workspace_registered(&self, name: &str) -> Result<bool, WorkspaceError> {
        match &self.kind {
            VcsKind::Jj(jj) => jj.is_workspace_registered(name),
            VcsKind::Git(git) => git.is_workspace_registered(name),
        }
    }

    /// Adds a new workspace under the given relative path.
    pub fn add_workspace(
        &self,
        rel_workspace_path: &Path,
        name: &str,
    ) -> Result<(), WorkspaceError> {
        match &self.kind {
            VcsKind::Jj(jj) => jj.add_workspace(rel_workspace_path, name),
            VcsKind::Git(git) => git.add_workspace(rel_workspace_path, name),
        }
    }

    /// Forgets a workspace.
    pub fn forget_workspace(&self, name: &str) -> Result<(), WorkspaceError> {
        match &self.kind {
            VcsKind::Jj(jj) => jj.forget_workspace(name),
            VcsKind::Git(git) => git.forget_workspace(name),
        }
    }

    /// Returns `true` if the workspace exists on disk according to this VCS's metadata.
    #[must_use]
    pub fn workspace_exists(&self, workspace_path: &Path) -> bool {
        match &self.kind {
            VcsKind::Jj(jj) => jj.workspace_exists(workspace_path),
            VcsKind::Git(git) => git.workspace_exists(workspace_path),
        }
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

    fn init_test_git_repo(path: &Path) {
        let output = std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .arg(path)
            .output()
            .expect("git init");
        assert!(output.status.success());

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

        std::fs::write(path.join("f.txt"), "hello\n").expect("write test file");
        drop(
            std::process::Command::new("git")
                .args(["add", "f.txt"])
                .current_dir(path)
                .output(),
        );
        drop(
            std::process::Command::new("git")
                .args(["commit", "-m", "init"])
                .current_dir(path)
                .output(),
        );
    }

    #[googletest::test]
    fn vcs_from_path_in_jj_repo_identifies_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_jj_repo(repo_root);

        let vcs = Vcs::from_path(repo_root).expect("from_path");
        expect_that!(vcs.repo_root(), eq(repo_root));
        expect_that!(matches!(vcs.kind, VcsKind::Jj(_)), is_true());
    }

    #[googletest::test]
    fn vcs_from_path_in_git_repo_identifies_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_root = dir.path();
        init_test_git_repo(repo_root);

        let vcs = Vcs::from_path(repo_root).expect("from_path");
        let canonical_root = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        expect_that!(vcs.repo_root(), eq(&canonical_root));
        expect_that!(matches!(vcs.kind, VcsKind::Git(_)), is_true());
    }

    #[googletest::test]
    fn nested_git_repo_inside_jj_repo_is_identified_as_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outer_jj = dir.path();
        init_test_jj_repo(outer_jj);

        let nested_git = outer_jj.join("sub_git_repo");
        std::fs::create_dir_all(&nested_git).expect("create nested git dir");
        init_test_git_repo(&nested_git);

        // Inside the nested git repo root:
        let vcs_nested = Vcs::from_path(&nested_git).expect("Vcs inside nested git");
        let canonical_nested = nested_git.canonicalize().unwrap_or_else(|_| nested_git.clone());
        expect_that!(matches!(vcs_nested.kind, VcsKind::Git(_)), is_true());
        expect_that!(vcs_nested.repo_root(), eq(&canonical_nested));

        // In a deep subdirectory of the nested git repo:
        let deep_sub = nested_git.join("src").join("module");
        std::fs::create_dir_all(&deep_sub).expect("create deep sub");
        let vcs_deep = Vcs::from_path(&deep_sub).expect("Vcs inside deep sub of nested git");
        expect_that!(matches!(vcs_deep.kind, VcsKind::Git(_)), is_true());
        expect_that!(vcs_deep.repo_root(), eq(&canonical_nested));

        // Outside in the outer JJ repo:
        let vcs_outer = Vcs::from_path(outer_jj).expect("Vcs inside outer jj");
        expect_that!(matches!(vcs_outer.kind, VcsKind::Jj(_)), is_true());
        expect_that!(vcs_outer.repo_root(), eq(outer_jj));
    }

    #[googletest::test]
    fn nested_jj_repo_inside_git_repo_is_identified_as_jj() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outer_git = dir.path();
        init_test_git_repo(outer_git);

        let nested_jj = outer_git.join("sub_jj_repo");
        std::fs::create_dir_all(&nested_jj).expect("create nested jj dir");
        init_test_jj_repo(&nested_jj);

        // Inside the nested JJ repo:
        let vcs_nested = Vcs::from_path(&nested_jj).expect("Vcs inside nested jj");
        expect_that!(matches!(vcs_nested.kind, VcsKind::Jj(_)), is_true());
        expect_that!(vcs_nested.repo_root(), eq(&nested_jj));

        // Outside in the outer Git repo:
        let vcs_outer = Vcs::from_path(outer_git).expect("Vcs inside outer git");
        let canonical_outer = outer_git.canonicalize().unwrap_or_else(|_| outer_git.to_path_buf());
        expect_that!(matches!(vcs_outer.kind, VcsKind::Git(_)), is_true());
        expect_that!(vcs_outer.repo_root(), eq(&canonical_outer));
    }

    #[googletest::test]
    fn vcs_outside_repo_returns_not_in_repo() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_repo = dir.path();

        let res = Vcs::from_path(non_repo);
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(WorkspaceError::NotInRepo)))
        );
    }
}
