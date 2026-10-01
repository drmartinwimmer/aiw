use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

#[cfg(not(test))]
use crate::config::ConfigInitializer;
use crate::tools::ensure_user_profile_bin_paths;
use crate::workspace::Workspace;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Fence executable 'fence' not found in PATH")]
    FenceNotFound,
    #[error("Invalid argument ordering: {0}")]
    InvalidArgOrder(String),
    #[error("Sandbox command failed with exit status: {0}")]
    ExecutionFailed(ExitStatus),
    #[error("I/O error during sandbox execution: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug)]
pub struct SandboxBuilder<'a> {
    workspace_path: &'a Path,
    repo_root: &'a Path,
    command: &'a [String],
    settings_path: Option<&'a Path>,
    use_direnv: bool,
    #[cfg(test)]
    shared_config_dir: Option<PathBuf>,
}

impl<'a> SandboxBuilder<'a> {
    /// Creates a new `SandboxBuilder` for the specified workspace, repository root, and required payload command.
    #[must_use]
    pub fn new(workspace_path: &'a Path, repo_root: &'a Path, command: &'a [String]) -> Self {
        Self {
            workspace_path,
            repo_root,
            command,
            settings_path: None,
            use_direnv: false,
            #[cfg(test)]
            shared_config_dir: None,
        }
    }

    /// Creates a new `SandboxBuilder` configured for the given `Workspace` and required payload command.
    #[must_use]
    pub fn for_workspace(workspace: &'a Workspace, command: &'a [String]) -> Self {
        Self::new(workspace.path(), workspace.repo_root(), command)
    }

    /// Explicit path to fence settings file (`fence.json` or `fence.jsonc`).
    #[must_use]
    pub fn with_settings_path(mut self, settings_path: &'a Path) -> Self {
        self.settings_path = Some(settings_path);
        self
    }

    /// Configures whether `direnv exec .` is prepended to the container payload command.
    #[must_use]
    pub fn with_direnv(mut self, use_direnv: bool) -> Self {
        self.use_direnv = use_direnv;
        self
    }

    /// Configures whether `direnv exec .` is prepended to the container payload command.
    #[must_use]
    pub fn with_use_direnv(self, use_direnv: bool) -> Self {
        self.with_direnv(use_direnv)
    }

    /// Overrides the shared configuration directory for test isolation.
    #[cfg(test)]
    #[must_use]
    pub fn with_shared_config_dir(mut self, dir: PathBuf) -> Self {
        self.shared_config_dir = Some(dir);
        self
    }

    fn append_settings_args(&self, cmd: &mut Command) {
        if let Some(settings) = self.resolve_settings_path() {
            cmd.args(["--settings", &settings.to_string_lossy()]);
        }
    }

    pub(crate) fn append_direnv_args(&self, cmd: &mut Command) -> Result<(), SandboxError> {
        if !self.use_direnv {
            return Ok(());
        }
        if cmd.get_args().last() != Some(std::ffi::OsStr::new("--")) {
            return Err(SandboxError::InvalidArgOrder(
                "direnv arguments must be appended immediately after '--'".to_string(),
            ));
        }
        cmd.args(["direnv", "exec", "."]);
        Ok(())
    }

    fn resolve_settings_path(&self) -> Option<PathBuf> {
        self.settings_path
            .filter(|p| p.exists())
            .map(Path::to_path_buf)
            .or_else(|| {
                [
                    self.workspace_path.join("fence.jsonc"),
                    self.workspace_path.join("fence.json"),
                    self.repo_root.join("fence.jsonc"),
                    self.repo_root.join("fence.json"),
                ]
                .into_iter()
                .find(|p| p.exists())
            })
            .or_else(|| {
                self.shared_template_candidates()
                    .into_iter()
                    .find(|p| p.exists())
            })
    }

    fn shared_template_candidates(&self) -> Vec<PathBuf> {
        #[cfg(test)]
        {
            self.shared_config_dir
                .as_ref()
                .map(|dir| vec![dir.join("fence.jsonc")])
                .unwrap_or_default()
        }

        #[cfg(not(test))]
        {
            let mut candidates = Vec::new();

            // 1. User configuration directory
            if let Some(user_config) = ConfigInitializer::user_config_path() {
                candidates.push(user_config);
            }

            // 2. System / package data directories ($XDG_DATA_DIRS/.../aiw/fence.jsonc)
            if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
                for data_dir in data_dirs.split(':').filter(|s| !s.is_empty()) {
                    candidates.push(Path::new(data_dir).join("aiw").join("fence.jsonc"));
                }
            }

            candidates.push(PathBuf::from("/usr/local/share/aiw/fence.jsonc"));
            candidates.push(PathBuf::from("/usr/share/aiw/fence.jsonc"));

            candidates
        }
    }

    fn construct_command(&self) -> Result<Command, SandboxError> {
        let mut cmd = Command::new("fence");
        cmd.current_dir(self.workspace_path);

        self.append_settings_args(&mut cmd);
        cmd.arg("--");
        self.append_direnv_args(&mut cmd)?;
        cmd.args(self.command);

        ensure_user_profile_bin_paths(&mut cmd);

        // Ensure TMPDIR points to the workspace directory inside the sandbox so that
        // temporary files are created on the same filesystem/mount, preventing EXDEV
        // (Invalid cross-device link) errors during atomic renames in fence.
        cmd.env("TMPDIR", self.workspace_path);

        Ok(cmd)
    }

    /// Builds the `std::process::Command` prepared to execute fence.
    pub fn build_command(&self) -> Result<Command, SandboxError> {
        if which::which("fence").is_err() {
            return Err(SandboxError::FenceNotFound);
        }

        self.construct_command()
    }

    /// Executes fence synchronously, forwarding standard I/O.
    /// Returns `Ok(())` on success, or `Err(SandboxError::ExecutionFailed)` if the command exited with a non-zero status.
    pub fn run(&self) -> Result<(), SandboxError> {
        let mut cmd = self.build_command()?;
        cmd.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        let status = cmd.status()?;
        if !status.success() {
            return Err(SandboxError::ExecutionFailed(status));
        }
        Ok(())
    }

    #[cfg(test)]
    fn build_args(&self) -> Result<Vec<String>, SandboxError> {
        let cmd = self.build_command()?;
        Ok(cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect())
    }
}

impl std::fmt::Display for SandboxBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cmd = self.construct_command().map_err(|_err| std::fmt::Error)?;
        write!(f, "{}", cmd.get_program().to_string_lossy())?;
        for arg in cmd.get_args() {
            write!(f, " {}", arg.to_string_lossy())?;
        }
        Ok(())
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
            actual
                .windows(expected.len())
                .any(|window| window.iter().zip(expected.iter()).all(|(a, b)| a == b))
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

        let cmd = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd);
        let args = builder.build_args().expect("build_args should succeed");

        let fence_str = fence_json.display().to_string();
        expect_that!(&args[..], contains_subslice(&["--settings", &fence_str]));
        expect_that!(
            &args[..],
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

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd);
        let args = builder.build_args().expect("build_args");

        let ws_fence_str = ws_fence.display().to_string();
        expect_that!(&args[..], contains_subslice(&["--settings", &ws_fence_str]));
    }

    #[googletest::test]
    fn build_args_without_any_fence_config_delegates_to_fence_discovery() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let cmd = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            &args[..],
            elements_are![eq("--"), eq("agy"), eq("--dangerously-skip-permissions")]
        );
    }

    #[googletest::test]
    fn build_args_discovers_shared_template_when_no_local_fence_config_exists() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        let shared_dir = temp_dir.path().join("shared");
        std::fs::create_dir_all(&workspace_path).expect("create ws");
        std::fs::create_dir_all(&shared_dir).expect("create shared dir");

        let shared_template = shared_dir.join("fence.jsonc");
        std::fs::write(&shared_template, r#"{"extends": "code"}"#).expect("write shared template");

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_shared_config_dir(shared_dir.clone());
        let args = builder.build_args().expect("build_args");

        let tpl_str = shared_template.display().to_string();
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--settings", &tpl_str])
        );
    }

    #[googletest::test]
    fn build_args_local_fence_config_takes_precedence_over_shared_template() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        let shared_dir = temp_dir.path().join("shared");
        std::fs::create_dir_all(&workspace_path).expect("create ws");
        std::fs::create_dir_all(&shared_dir).expect("create shared dir");

        let shared_template = shared_dir.join("fence.jsonc");
        std::fs::write(&shared_template, r#"{"extends": "code"}"#).expect("write shared template");

        let local_fence = workspace_path.join("fence.jsonc");
        std::fs::write(&local_fence, r#"{"extends": "code"}"#).expect("write local fence");

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_shared_config_dir(shared_dir);
        let args = builder.build_args().expect("build_args");

        let local_str = local_fence.display().to_string();
        expect_that!(
            args.as_slice(),
            contains_subslice(&["--settings", &local_str])
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

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_settings_path(&custom_fence);
        let args = builder.build_args().expect("build_args");

        let custom_str = custom_fence.display().to_string();
        expect_that!(&args[..], contains_subslice(&["--settings", &custom_str]));
    }

    #[googletest::test]
    fn build_args_with_use_direnv_true_prepends_direnv_exec() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let cmd = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd).with_direnv(true);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            &args[..],
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
        let builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &custom_cmd).with_direnv(true);
        let args = builder.build_args().expect("build_args");

        expect_that!(
            &args[..],
            contains_subslice(&["--", "direnv", "exec", ".", "sh", "-c", "echo ok"])
        );
    }

    #[googletest::test]
    fn build_command_sets_fence_program_and_workspace_cwd() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let cmd_payload = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd_payload);
        let cmd = builder.build_command().expect("build_command");

        expect_that!(cmd.get_program(), eq("fence"));
        expect_that!(cmd.get_current_dir(), some(eq(&workspace_path)));
    }

    #[googletest::test]
    fn append_direnv_args_called_out_of_order_returns_error() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path();
        let ws_path = repo_root.join("ws");
        let cmd_payload = vec!["echo".to_string()];

        let builder = SandboxBuilder::new(&ws_path, repo_root, &cmd_payload).with_direnv(true);

        // Case 1: called on Command without "--" separator
        let mut cmd1 = Command::new("fence");
        let res1 = builder.append_direnv_args(&mut cmd1);
        expect_that!(
            res1,
            matches_pattern!(Err(matches_pattern!(SandboxError::InvalidArgOrder(
                anything()
            ))))
        );

        // Case 2: called after command arguments already present
        let mut cmd2 = Command::new("fence");
        cmd2.arg("--");
        cmd2.arg("echo");
        let res2 = builder.append_direnv_args(&mut cmd2);
        expect_that!(
            res2,
            matches_pattern!(Err(matches_pattern!(SandboxError::InvalidArgOrder(
                anything()
            ))))
        );

        // Case 3: called immediately after "--" succeeds
        let mut cmd3 = Command::new("fence");
        cmd3.arg("--");
        let res3 = builder.append_direnv_args(&mut cmd3);
        expect_that!(res3, ok(anything()));
    }

    #[googletest::test]
    fn display_trait_produces_readable_shell_string() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let custom_cmd = vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()];
        let builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &custom_cmd).with_direnv(true);

        let formatted = format!("{builder}");
        expect_that!(&formatted, eq("fence -- direnv exec . sh -c echo hi"));
        expect_that!(&builder.to_string(), eq(&formatted));
    }

    #[googletest::test]
    fn for_workspace_configures_workspace_cwd_and_preserves_command() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path();
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "git", "init"])
            .arg(repo_root)
            .output()
            .expect("init jj repo");
        assert!(output.status.success());

        let ws = Workspace::new(repo_root, "builder-ws").expect("new workspace");
        let cmd_payload = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
            "--extra-flag".to_string(),
        ];
        let builder = SandboxBuilder::for_workspace(&ws, &cmd_payload).with_direnv(true);

        let cmd = builder.build_command().expect("build_command");
        expect_that!(cmd.get_current_dir(), some(eq(ws.path())));
        let formatted = format!("{builder}");
        expect_that!(
            &formatted,
            contains_substring("direnv exec . agy --dangerously-skip-permissions --extra-flag")
        );
    }
}
