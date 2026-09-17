use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Fence executable 'fence' not found in PATH")]
    FenceNotFound,
    #[error("I/O error during sandbox execution: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct SandboxConfig<'a> {
    pub repo_root: &'a Path,
    pub workspace_path: &'a Path,
    pub command: Option<&'a [String]>,
    pub extra_args: &'a [String],
    pub settings_path: Option<&'a Path>,
    pub use_direnv: bool,
}

#[derive(Debug)]
pub struct SandboxBuilder<'a> {
    config: SandboxConfig<'a>,
}

impl<'a> SandboxBuilder<'a> {
    pub fn new(config: SandboxConfig<'a>) -> Self {
        Self { config }
    }

    /// Resolves fence configuration and returns the list of fence CLI arguments.
    pub fn build_args(&self) -> Result<Vec<String>, SandboxError> {
        if which::which("fence").is_err() {
            return Err(SandboxError::FenceNotFound);
        }

        let mut args = Vec::new();

        if let Some(settings) = self.resolve_settings_path() {
            args.push("--settings".to_string());
            args.push(settings.display().to_string());
        } else {
            args.push("--template".to_string());
            args.push("code".to_string());
        }

        args.push("--".to_string());

        if self.config.use_direnv {
            args.push("direnv".to_string());
            args.push("exec".to_string());
            args.push(".".to_string());
        }

        if let Some(cmd) = self.config.command {
            for part in cmd {
                args.push(part.clone());
            }
        } else {
            args.push("agy".to_string());

            if !self
                .config
                .extra_args
                .iter()
                .any(|a| a == "--dangerously-skip-permissions")
            {
                args.push("--dangerously-skip-permissions".to_string());
            }
        }

        for extra in self.config.extra_args {
            args.push(extra.clone());
        }

        Ok(args)
    }

    fn resolve_settings_path(&self) -> Option<PathBuf> {
        if let Some(path) = self.config.settings_path
            && path.exists()
        {
            return Some(path.to_path_buf());
        }

        let candidates = [
            self.config.workspace_path.join("fence.jsonc"),
            self.config.workspace_path.join("fence.json"),
            self.config.repo_root.join("fence.jsonc"),
            self.config.repo_root.join("fence.json"),
        ];

        candidates.into_iter().find(|p| p.exists())
    }

    /// Builds the std::process::Command prepared to execute fence.
    pub fn build_command(&self) -> Result<Command, SandboxError> {
        let args = self.build_args()?;
        let mut cmd = Command::new("fence");
        cmd.current_dir(self.config.workspace_path);
        cmd.args(args);

        ensure_user_profile_bin_paths(&mut cmd);

        // In environments where TMPDIR points to a non-existent directory, fallback to /tmp
        if let Ok(tmp) = std::env::var("TMPDIR")
            && !Path::new(&tmp).exists()
        {
            cmd.env("TMPDIR", "/tmp");
        }

        Ok(cmd)
    }

    /// Executes fence synchronously, forwarding standard I/O and returning its ExitStatus.
    pub fn run(&self) -> Result<ExitStatus, SandboxError> {
        let mut cmd = self.build_command()?;
        cmd.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        let status = cmd.status()?;
        Ok(status)
    }
}

/// Ensures user profile binary paths (such as Nix user and home-manager profiles)
/// are present in PATH so that sandboxed environments can locate tools like direnv.
fn ensure_user_profile_bin_paths(cmd: &mut Command) {
    let Ok(path_var) = std::env::var("PATH") else {
        return;
    };

    let mut paths: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    let mut updated = false;

    if let Ok(user) = std::env::var("USER") {
        let user_bin = PathBuf::from(format!("/etc/profiles/per-user/{user}/bin"));
        if user_bin.exists() && !paths.contains(&user_bin) {
            paths.push(user_bin);
            updated = true;
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let nix_bin = PathBuf::from(home).join(".nix-profile/bin");
        if nix_bin.exists() && !paths.contains(&nix_bin) {
            paths.push(nix_bin);
            updated = true;
        }
    }
    if updated
        && let Ok(new_path) = std::env::join_paths(paths)
    {
        cmd.env("PATH", new_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    fn contains_subslice<'a, 'b>(expected: &'a [&'a str]) -> impl Matcher<&'b [String]> + 'a {
        predicate(move |actual: &[String]| {
            if expected.is_empty() {
                return true;
            }
            actual.windows(expected.len()).any(|window| {
                window.iter().zip(expected.iter()).all(|(a, b)| a == b)
            })
        })
        .with_description(
            format!("contains contiguous subslice {expected:?}"),
            format!("does not contain contiguous subslice {expected:?}"),
        )
    }

    #[googletest::test]
    fn build_args_with_existing_fence_json_in_repo_root_passes_settings_flag() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("my-ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let fence_json = repo_root.join("fence.json");
        std::fs::write(&fence_json, r#"{"extends": "code"}"#).expect("write fence.json");

        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args should succeed");

        let fence_str = fence_json.display().to_string();
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--settings", &fence_str])
        );
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--", "agy", "--dangerously-skip-permissions"])
        );
    }

    #[googletest::test]
    fn build_args_with_fence_jsonc_in_workspace_takes_precedence_over_repo_root() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let root_fence = repo_root.join("fence.json");
        std::fs::write(&root_fence, r#"{"extends": "code"}"#).expect("write root fence");
        let ws_fence = workspace_path.join("fence.jsonc");
        std::fs::write(&ws_fence, r#"{"extends": "code"}"#).expect("write ws fence");

        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        let ws_fence_str = ws_fence.display().to_string();
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--settings", &ws_fence_str])
        );
    }

    #[googletest::test]
    fn build_args_without_any_fence_config_falls_back_to_template_code() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            args.as_slice(),
            contains_subslice(&["--template", "code"])
        );
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--", "agy", "--dangerously-skip-permissions"])
        );
    }

    #[googletest::test]
    fn build_args_with_explicit_settings_path_uses_specified_path() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let custom_fence = temp_dir.path().join("custom-fence.json");
        std::fs::write(&custom_fence, r#"{"extends": "code"}"#).expect("write custom fence");

        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: Some(&custom_fence),
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        let custom_str = custom_fence.display().to_string();
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--settings", &custom_str])
        );
    }

    #[googletest::test]
    fn build_args_with_extra_args_forwards_arguments_to_agy() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let extra_args = vec!["--model".to_string(), "gemini-2.5".to_string()];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            args.as_slice(),
            contains_subslice(&[
                "--",
                "agy",
                "--dangerously-skip-permissions",
                "--model",
                "gemini-2.5"
            ])
        );
    }

    #[googletest::test]
    fn build_args_avoids_duplicate_yolo_flag_if_present_in_extra_args() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let extra_args = vec![
            "--dangerously-skip-permissions".to_string(),
            "--prompt".to_string(),
            "hello".to_string(),
        ];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        let yolo_count = args
            .iter()
            .filter(|a| a.as_str() == "--dangerously-skip-permissions")
            .count();
        expect_that!(yolo_count, eq(1));
    }

    #[googletest::test]
    fn build_args_with_use_direnv_true_prepends_direnv_exec() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: true,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            args.as_slice(),
            contains_subslice(&[
                "--",
                "direnv",
                "exec",
                ".",
                "agy",
                "--dangerously-skip-permissions"
            ])
        );
    }

    #[googletest::test]
    fn build_args_with_custom_command_executes_specified_command() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let custom_cmd = vec!["sh".to_string(), "-c".to_string(), "echo ok".to_string()];
        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: Some(&custom_cmd),
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: true,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            args.as_slice(),
            contains_subslice(&["--", "direnv", "exec", ".", "sh", "-c", "echo ok"])
        );
    }

    #[googletest::test]
    fn build_command_sets_fence_program_and_workspace_cwd() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let extra_args = vec![];
        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &extra_args,
            settings_path: None,
            use_direnv: false,
        };

        let builder = SandboxBuilder::new(config);
        let cmd = builder.build_command().expect("build_command");

        expect_that!(cmd.get_program(), eq("fence"));
        expect_that!(cmd.get_current_dir(), some(eq(&workspace_path)));
    }
}
