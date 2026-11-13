use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use clapback::{CommandError, CommandExt as _, EitherCommand, ToArgs};

#[cfg(not(test))]
use crate::config::ConfigInitializer;
use crate::tools::{DirenvExecCommand, FenceCommand};
use crate::workspace::Workspace;

#[derive(Debug, Clone, Copy)]
struct PayloadCommand<'a> {
    workspace_path: &'a Path,
    args: &'a [String],
}

impl ToArgs for PayloadCommand<'_> {
    fn append_args(&self, args: &mut Vec<OsString>) {
        if let Some((_program, rest)) = self.args.split_first() {
            args.extend(rest.iter().map(OsString::from));
        }
    }
}

impl clapback::Command for PayloadCommand<'_> {
    fn program(&self) -> &OsStr {
        self.args.first().map_or_else(|| OsStr::new(""), OsStr::new)
    }

    fn current_dir_path(&self) -> Option<&Path> {
        Some(self.workspace_path)
    }

    fn ensure_user_path(&self) -> bool {
        true
    }
}

type InnerCommand<'a> = EitherCommand<DirenvExecCommand<PayloadCommand<'a>>, PayloadCommand<'a>>;
type ConfiguredCommand<'a> = EitherCommand<FenceCommand<InnerCommand<'a>>, InnerCommand<'a>>;

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
    fence: bool,
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
            fence: true,
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
    #[cfg(test)]
    #[must_use]
    fn with_settings_path(mut self, settings_path: &'a Path) -> Self {
        self.settings_path = Some(settings_path);
        self
    }

    /// Configures whether `direnv exec .` is prepended to the container payload command.
    #[must_use]
    pub fn with_direnv(mut self, use_direnv: bool) -> Self {
        self.use_direnv = use_direnv;
        self
    }

    /// Configures whether the payload command is wrapped inside `fence` (defaults to `true`).
    #[must_use]
    pub fn with_fence(mut self, fence: bool) -> Self {
        self.fence = fence;
        self
    }

    /// Overrides the shared configuration directory for test isolation.
    #[cfg(test)]
    #[must_use]
    fn with_shared_config_dir(mut self, dir: PathBuf) -> Self {
        self.shared_config_dir = Some(dir);
        self
    }

    pub(crate) fn has_settings(&self) -> bool {
        self.resolve_settings_path().is_some()
    }

    fn resolve_settings_path(&self) -> Option<PathBuf> {
        self.settings_path
            .filter(|p| p.exists())
            .map(Path::to_path_buf)
            .or_else(|| {
                // Only resolve project settings from the read-only repository root, never from
                // the agent-writable `.workspaces/<name>` directory.
                [
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
            candidates.extend(ConfigInitializer::system_template_paths());

            candidates
        }
    }

    fn configured_command(&self) -> ConfiguredCommand<'a> {
        PayloadCommand {
            workspace_path: self.workspace_path,
            args: self.command,
        }
        .wrap_if(self.use_direnv, DirenvExecCommand::dot)
        .wrap_if(self.fence, |cmd| FenceCommand {
            workspace_path: self.workspace_path.to_path_buf(),
            settings: self.resolve_settings_path(),
            command: cmd,
        })
    }

    /// Builds the `std::process::Command` prepared to execute fence (or the direct payload when fence is disabled).
    pub fn build_command(&self) -> Result<Command, SandboxError> {
        if self.fence && which::which("fence").is_err() {
            return Err(SandboxError::FenceNotFound);
        }

        Ok(match self.configured_command() {
            EitherCommand::Left(fence_cmd) => fence_cmd.to_sanitized_std_command(),
            EitherCommand::Right(direct_cmd) => direct_cmd.to_std_command(),
        })
    }

    /// Executes fence (or the direct payload when fence is disabled) synchronously, forwarding standard I/O.
    /// Returns `Ok(())` on success, or `Err(SandboxError::ExecutionFailed)` if the command exited with a non-zero status.
    pub fn run(&self) -> Result<(), SandboxError> {
        if self.fence && which::which("fence").is_err() {
            return Err(SandboxError::FenceNotFound);
        }

        let result = match self.configured_command() {
            EitherCommand::Left(fence_cmd) => fence_cmd.run_sanitized(),
            EitherCommand::Right(direct_cmd) => direct_cmd.run(),
        };

        result.map_err(|err| match err {
            CommandError::ProgramNotFound(_) if self.fence => SandboxError::FenceNotFound,
            CommandError::ProgramNotFound(program) => SandboxError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Executable '{program}' not found in PATH"),
            )),
            CommandError::ExecutionFailed { status, .. } => SandboxError::ExecutionFailed(status),
            CommandError::Io(io_err) => SandboxError::Io(io_err),
        })
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
        write!(f, "{}", self.configured_command())
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[googletest::test]
    fn build_args_with_existing_fence_json_in_repo_root_passes_settings_flag() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("my-ws");
        std::fs::create_dir_all(&workspace_path)?;

        let fence_json = repo_root.join("fence.json");
        std::fs::write(&fence_json, r#"{"extends": "code"}"#)?;

        let cmd = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd);
        let args = builder.build_args()?;

        let fence_str = fence_json.display().to_string();
        expect_that!(&args[..], contains_subslice(&["--settings", &fence_str]));
        expect_that!(
            &args[..],
            contains_subslice(&["--", "agy", "--dangerously-skip-permissions"])
        );
        Ok(())
    }

    #[googletest::test]
    fn build_args_ignores_fence_jsonc_inside_agent_writable_workspace() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        let shared_dir = temp_dir.path().join("shared");
        std::fs::create_dir_all(&workspace_path)?;
        std::fs::create_dir_all(&shared_dir)?;

        let root_fence = repo_root.join("fence.json");
        std::fs::write(&root_fence, r#"{"allowPty": true}"#)?;
        let ws_fence = workspace_path.join("fence.jsonc");
        std::fs::write(&ws_fence, r#"{"allowPty": true}"#)?;

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd);
        let args = builder.build_args()?;

        let root_fence_str = root_fence.display().to_string();
        expect_that!(
            &args[..],
            contains_subslice(&["--settings", &root_fence_str])
        );

        // Even when no repo root config exists, workspace config must be ignored in favor of shared template.
        std::fs::remove_file(&root_fence)?;
        let shared_template = shared_dir.join("fence.jsonc");
        std::fs::write(&shared_template, r#"{"allowPty": true}"#)?;

        let builder_no_root = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_shared_config_dir(shared_dir);
        let args_no_root = builder_no_root.build_args()?;
        let shared_str = shared_template.display().to_string();
        expect_that!(
            &args_no_root[..],
            contains_subslice(&["--settings", &shared_str])
        );
        Ok(())
    }

    #[googletest::test]
    fn build_args_without_any_fence_config_delegates_to_fence_discovery() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let cmd = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd);
        let args = builder.build_args()?;

        expect_that!(
            &args[..],
            elements_are![eq("--"), eq("agy"), eq("--dangerously-skip-permissions")]
        );
        Ok(())
    }

    #[googletest::test]
    fn build_args_discovers_shared_template_when_no_local_fence_config_exists() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        let shared_dir = temp_dir.path().join("shared");
        std::fs::create_dir_all(&workspace_path)?;
        std::fs::create_dir_all(&shared_dir)?;

        let shared_template = shared_dir.join("fence.jsonc");
        std::fs::write(&shared_template, r#"{"extends": "code"}"#)?;

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_shared_config_dir(shared_dir.clone());
        let args = builder.build_args()?;

        let tpl_str = shared_template.display().to_string();
        expect_that!(&*args, contains_subslice(&["--settings", &tpl_str]));
        Ok(())
    }

    #[googletest::test]
    fn build_args_local_fence_config_takes_precedence_over_shared_template() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        let shared_dir = temp_dir.path().join("shared");
        std::fs::create_dir_all(&workspace_path)?;
        std::fs::create_dir_all(&shared_dir)?;

        let shared_template = shared_dir.join("fence.jsonc");
        std::fs::write(&shared_template, r#"{"extends": "code"}"#)?;

        let local_fence = repo_root.join("fence.jsonc");
        std::fs::write(&local_fence, r#"{"extends": "code"}"#)?;

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_shared_config_dir(shared_dir);
        let args = builder.build_args()?;

        let local_str = local_fence.display().to_string();
        expect_that!(&*args, contains_subslice(&["--settings", &local_str]));
        Ok(())
    }

    #[googletest::test]
    fn build_args_with_explicit_settings_path_uses_specified_path() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let custom_fence = temp_dir.path().join("custom-fence.json");
        std::fs::write(&custom_fence, r#"{"extends": "code"}"#)?;

        let cmd = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd)
            .with_settings_path(&custom_fence);
        let args = builder.build_args()?;

        let custom_str = custom_fence.display().to_string();
        expect_that!(&args[..], contains_subslice(&["--settings", &custom_str]));
        Ok(())
    }

    #[googletest::test]
    fn build_args_with_use_direnv_true_prepends_direnv_exec() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let cmd = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd).with_direnv(true);
        let args = builder.build_args()?;

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
        Ok(())
    }

    #[googletest::test]
    fn build_args_with_custom_command_executes_specified_command() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let custom_cmd = vec!["sh".to_string(), "-c".to_string(), "echo ok".to_string()];
        let builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &custom_cmd).with_direnv(true);
        let args = builder.build_args()?;

        expect_that!(
            &args[..],
            contains_subslice(&["--", "direnv", "exec", ".", "sh", "-c", "echo ok"])
        );
        Ok(())
    }

    #[googletest::test]
    fn build_command_sets_fence_program_and_workspace_cwd() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let cmd_payload = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd_payload);
        let cmd = builder.build_command()?;

        expect_that!(cmd.get_program(), eq("fence"));
        expect_that!(cmd.get_current_dir(), some(eq(&workspace_path)));
        Ok(())
    }

    #[googletest::test]
    fn display_trait_produces_readable_shell_string() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let custom_cmd = vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()];
        let builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &custom_cmd).with_direnv(true);

        let formatted = format!("{builder}");
        expect_that!(&formatted, eq("fence -- direnv exec . sh -c 'echo hi'"));
        expect_that!(&builder.to_string(), eq(&formatted));
        Ok(())
    }

    #[googletest::test]
    fn for_workspace_configures_workspace_cwd_and_preserves_command() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path();
        let output = std::process::Command::new("jj")
            .args(["--no-pager", "git", "init"])
            .arg(repo_root)
            .output()?;
        expect_that!(output.status.success(), is_true());

        let ws = Workspace::new(repo_root, "builder-ws")?;
        let cmd_payload = vec![
            "agy".to_string(),
            "--dangerously-skip-permissions".to_string(),
            "--extra-flag".to_string(),
        ];
        let builder = SandboxBuilder::for_workspace(&ws, &cmd_payload).with_direnv(true);

        let cmd = builder.build_command()?;
        expect_that!(cmd.get_current_dir(), some(eq(ws.path())));
        let formatted = format!("{builder}");
        expect_that!(
            &formatted,
            contains_substring("direnv exec . agy --dangerously-skip-permissions --extra-flag")
        );
        Ok(())
    }
    #[googletest::test]
    fn build_command_scrubs_host_multiplexer_env_vars() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let cmd_payload = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd_payload);
        let cmd = builder.build_command()?;

        let removed_vars: Vec<&OsStr> = cmd
            .get_envs()
            .filter_map(|(k, v)| v.is_none().then_some(k))
            .collect();

        for expected in [
            "HC_TERMINAL_ID",
            "HC_TERMINAL_SOCKET",
            "HERDR_ENV",
            "HERDR_SOCKET_PATH",
            "TMUX",
            "ZELLIJ",
            "KITTY_LISTEN_ON",
            "WEZTERM_UNIX_SOCKET",
        ] {
            expect_that!(removed_vars, contains(eq(&OsStr::new(expected))));
        }
        Ok(())
    }

    #[googletest::test]
    fn with_fence_false_builds_direct_payload_command_in_workspace_cwd() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let fence_json = repo_root.join("fence.jsonc");
        std::fs::write(&fence_json, r#"{"extends": "code"}"#)?;

        let cmd_payload = vec![
            "agy".to_string(),
            "--model".to_string(),
            "gemini-2.5".to_string(),
        ];
        let builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &cmd_payload).with_fence(false);

        let cmd = builder.build_command()?;
        expect_that!(cmd.get_program(), eq("agy"));
        expect_that!(cmd.get_current_dir(), some(eq(&workspace_path)));
        let args = builder.build_args()?;
        expect_that!(&args[..], elements_are![eq("--model"), eq("gemini-2.5")]);

        let tmpdir_env = cmd.get_envs().find(|(k, _)| *k == OsStr::new("TMPDIR"));
        expect_that!(tmpdir_env, none());
        expect_that!(format!("{builder}"), eq("agy --model gemini-2.5"));
        Ok(())
    }

    #[googletest::test]
    fn with_fence_false_and_direnv_true_wraps_payload_in_direnv_exec() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let cmd_payload = vec!["agy".to_string()];
        let builder = SandboxBuilder::new(&workspace_path, &repo_root, &cmd_payload)
            .with_direnv(true)
            .with_fence(false);

        let cmd = builder.build_command()?;
        expect_that!(cmd.get_program(), eq("direnv"));
        expect_that!(cmd.get_current_dir(), some(eq(&workspace_path)));
        let args = builder.build_args()?;
        expect_that!(&args[..], elements_are![eq("exec"), eq("."), eq("agy")]);
        expect_that!(format!("{builder}"), eq("direnv exec . agy"));
        Ok(())
    }

    #[googletest::test]
    fn run_with_fence_false_executes_command_and_reports_missing_binary_cleanly() -> TestResult {
        let temp_dir = tempfile::tempdir()?;
        let repo_root = temp_dir.path().join("repo");
        let workspace_path = repo_root.join(".workspaces").join("ws");
        std::fs::create_dir_all(&workspace_path)?;

        let marker = workspace_path.join("ran.txt");
        let ok_payload = vec![
            "sh".to_string(),
            "-c".to_string(),
            "printf ok > ran.txt".to_string(),
        ];
        let ok_builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &ok_payload).with_fence(false);
        expect_that!(ok_builder.run(), ok(anything()));
        expect_that!(std::fs::read_to_string(&marker)?, eq("ok"));

        let missing_payload = vec!["/nonexistent/aiw-test-binary".to_string()];
        let missing_builder =
            SandboxBuilder::new(&workspace_path, &repo_root, &missing_payload).with_fence(false);
        let res = missing_builder.run();
        expect_that!(
            res,
            err(matches_pattern!(SandboxError::Io(predicate(
                |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound
            ))))
        );
        Ok(())
    }
}
