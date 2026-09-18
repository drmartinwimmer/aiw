use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

pub use crate::direnv::{
    allow_direnv, allow_direnv_if_repo_root_allowed, is_direnv_allowed,
};
use crate::direnv::ensure_user_profile_bin_paths;
use crate::workspace::Workspace;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Fence executable 'fence' not found in PATH")]
    FenceNotFound,
    #[error("I/O error during sandbox execution: {0}")]
    Io(#[from] std::io::Error),
}

/// Formats a `Command` into a shell-style command line string suitable for display or dry-run.
#[must_use]
pub fn format_command(cmd: &Command) -> String {
    let mut parts = Vec::new();
    parts.push(cmd.get_program().to_string_lossy().into_owned());
    for arg in cmd.get_args() {
        parts.push(arg.to_string_lossy().into_owned());
    }
    parts.join(" ")
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
    workspace_path: &'a Path,
    repo_root: &'a Path,
    command: Option<&'a [String]>,
    extra_args: &'a [String],
    settings_path: Option<&'a Path>,
    use_direnv: bool,
}

impl<'a> SandboxBuilder<'a> {
    /// Creates a new `SandboxBuilder` for the specified workspace and repository root.
    #[must_use]
    pub fn new(workspace_path: &'a Path, repo_root: &'a Path) -> Self {
        Self {
            workspace_path,
            repo_root,
            command: None,
            extra_args: &[],
            settings_path: None,
            use_direnv: false,
        }
    }

    /// Creates a new `SandboxBuilder` configured for the given `Workspace`.
    #[must_use]
    pub fn for_workspace(workspace: &'a Workspace) -> Self {
        Self::new(workspace.path(), workspace.repo_root())
    }

    /// Constructs a `SandboxBuilder` from a prefilled `SandboxConfig`.
    #[must_use]
    pub fn from_config(config: SandboxConfig<'a>) -> Self {
        Self {
            workspace_path: config.workspace_path,
            repo_root: config.repo_root,
            command: config.command,
            extra_args: config.extra_args,
            settings_path: config.settings_path,
            use_direnv: config.use_direnv,
        }
    }

    /// Sets the command payload to execute inside the fence container.
    #[must_use]
    pub fn command(mut self, command: &'a [String]) -> Self {
        self.command = Some(command);
        self
    }

    /// Sets the optional command payload to execute inside the fence container.
    #[must_use]
    pub fn optional_command(mut self, command: Option<&'a [String]>) -> Self {
        self.command = command;
        self
    }

    /// Extra arguments forwarded to the payload command (such as agy).
    #[must_use]
    pub fn extra_args(mut self, extra_args: &'a [String]) -> Self {
        self.extra_args = extra_args;
        self
    }

    /// Explicit path to fence settings file (`fence.json` or `fence.jsonc`).
    #[must_use]
    pub fn settings_path(mut self, settings_path: &'a Path) -> Self {
        self.settings_path = Some(settings_path);
        self
    }

    /// Sets the optional fence settings file path.
    #[must_use]
    pub fn optional_settings_path(mut self, settings_path: Option<&'a Path>) -> Self {
        self.settings_path = settings_path;
        self
    }

    /// Configures whether `direnv exec .` is prepended to the container payload command.
    #[must_use]
    pub fn use_direnv(mut self, use_direnv: bool) -> Self {
        self.use_direnv = use_direnv;
        self
    }

    fn append_settings_args(&self, cmd: &mut Command) {
        if let Some(settings) = self.resolve_settings_path() {
            cmd.args(["--settings", &settings.to_string_lossy()]);
        } else {
            cmd.args(["--template", "code"]);
        }
    }

    fn append_direnv_args(&self, cmd: &mut Command) {
        if self.use_direnv {
            cmd.args(["direnv", "exec", "."]);
        }
    }

    fn append_command_args(&self, cmd: &mut Command) {
        if let Some(command) = self.command {
            cmd.args(command);
        } else {
            cmd.arg("agy");
            if !self
                .extra_args
                .iter()
                .any(|a| a == "--dangerously-skip-permissions")
            {
                cmd.arg("--dangerously-skip-permissions");
            }
        }
    }

    fn append_extra_args(&self, cmd: &mut Command) {
        cmd.args(self.extra_args);
    }

    fn resolve_settings_path(&self) -> Option<PathBuf> {
        if let Some(path) = self.settings_path
            && path.exists()
        {
            return Some(path.to_path_buf());
        }

        let candidates = [
            self.workspace_path.join("fence.jsonc"),
            self.workspace_path.join("fence.json"),
            self.repo_root.join("fence.jsonc"),
            self.repo_root.join("fence.json"),
        ];

        candidates.into_iter().find(|p| p.exists())
    }

    /// Builds the `std::process::Command` prepared to execute fence.
    pub fn build_command(&self) -> Result<Command, SandboxError> {
        if which::which("fence").is_err() {
            return Err(SandboxError::FenceNotFound);
        }

        let mut cmd = Command::new("fence");
        cmd.current_dir(self.workspace_path);

        self.append_settings_args(&mut cmd);
        cmd.arg("--");
        self.append_direnv_args(&mut cmd);
        self.append_command_args(&mut cmd);
        self.append_extra_args(&mut cmd);

        ensure_user_profile_bin_paths(&mut cmd);

        // In environments where TMPDIR points to a non-existent directory, fallback to /tmp
        if let Ok(tmp) = std::env::var("TMPDIR")
            && !Path::new(&tmp).exists()
        {
            cmd.env("TMPDIR", "/tmp");
        }

        Ok(cmd)
    }

    /// Builds and formats the command into a string for dry run or display.
    pub fn format_command(&self) -> Result<String, SandboxError> {
        let cmd = self.build_command()?;
        Ok(format_command(&cmd))
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

    #[cfg(test)]
    fn build_args(&self) -> Result<Vec<String>, SandboxError> {
        let cmd = self.build_command()?;
        Ok(cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect())
    }
}

impl<'a> From<SandboxConfig<'a>> for SandboxBuilder<'a> {
    fn from(config: SandboxConfig<'a>) -> Self {
        Self::from_config(config)
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

        let builder = SandboxBuilder::new(&workspace_path, &repo_root);
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

        let builder = SandboxBuilder::new(&workspace_path, &repo_root);
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

        let builder = SandboxBuilder::new(&workspace_path, &repo_root);
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

        let builder = SandboxBuilder::new(&workspace_path, &repo_root)
            .settings_path(&custom_fence);
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
        let builder = SandboxBuilder::new(&workspace_path, &repo_root)
            .extra_args(&extra_args);
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
        let builder = SandboxBuilder::new(&workspace_path, &repo_root)
            .extra_args(&extra_args);
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

        let builder = SandboxBuilder::new(&workspace_path, &repo_root)
            .use_direnv(true);
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
        let builder = SandboxBuilder::new(&workspace_path, &repo_root)
            .command(&custom_cmd)
            .use_direnv(true);
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

        let builder = SandboxBuilder::new(&workspace_path, &repo_root);
        let cmd = builder.build_command().expect("build_command");

        expect_that!(cmd.get_program(), eq("fence"));
        expect_that!(cmd.get_current_dir(), some(eq(&workspace_path)));
    }

    #[googletest::test]
    fn format_command_produces_readable_shell_string() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let custom_cmd = vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root)
            .command(&custom_cmd)
            .use_direnv(true);

        let formatted = builder.format_command().expect("format_command");
        expect_that!(
            formatted.as_str(),
            eq("fence --template code -- direnv exec . sh -c echo hi")
        );
    }

    #[googletest::test]
    fn builder_for_workspace_and_fluent_setters() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path();
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "git", "init"])
            .arg(repo_root)
            .output()
            .expect("init jj repo");
        assert!(output.status.success());

        let ws = Workspace::new(repo_root, "builder-ws").expect("new workspace");
        let extra = vec!["--extra-flag".to_string()];
        let builder = SandboxBuilder::for_workspace(&ws)
            .extra_args(&extra)
            .use_direnv(true);

        let cmd = builder.build_command().expect("build_command");
        expect_that!(cmd.get_current_dir(), some(eq(ws.path())));
        let formatted = format_command(&cmd);
        expect_that!(
            formatted.as_str(),
            contains_substring("direnv exec . agy --dangerously-skip-permissions --extra-flag")
        );
    }
}
