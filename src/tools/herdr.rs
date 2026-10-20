use clapback::{Command, CommandError, CommandExt as _};
use std::path::{Path, PathBuf};

#[derive(Command, Debug, Clone, PartialEq, Eq)]
#[command(ensure_user_path)]
struct HerdrCli {
    #[command(program)]
    binary: PathBuf,

    #[command(subcommand)]
    subcommand: HerdrSubcommand,
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum HerdrSubcommand {
    #[command(subcommand)]
    Tab(HerdrTabAction),
    #[command(subcommand)]
    Pane(HerdrPaneAction),
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum HerdrTabAction {
    Get {
        tab_id: String,
    },
    Create {
        #[arg(long)]
        label: String,

        #[arg(long, false_long = "no-focus")]
        focus: bool,

        #[arg(long)]
        cwd: Option<PathBuf>,

        #[arg(long)]
        workspace: Option<String>,
    },
}

#[derive(clapback::Subcommand, Debug, Clone, PartialEq, Eq)]
enum HerdrPaneAction {
    ReportAgent {
        #[arg(long)]
        source: String,

        #[arg(long)]
        agent: String,

        #[arg(long)]
        state: String,

        pane_id: String,
    },
    Run {
        pane_id: String,
        command: String,
    },
}

#[derive(Command, Debug, Clone, PartialEq, Eq)]
#[command(program = "sh", arg = "-c", ensure_user_path)]
struct ShCommand {
    #[command(current_dir)]
    current_dir: Option<PathBuf>,

    command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HerdrError {
    #[error("Not running inside Herdr (HERDR_ENV != 1)")]
    NotInsideHerdr,
    #[error("Herdr binary not found: {0}")]
    BinaryNotFound(String),
    #[error("Herdr command '{command}' failed (exit code {code:?}): {stderr}")]
    CommandFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("Failed to parse Herdr JSON output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Herdr API returned error: {0}")]
    Api(String),
    #[error("Missing expected field '{0}' in Herdr response")]
    MissingResponseField(&'static str),
    #[error("Missing context from Herdr environment: {0}")]
    MissingContext(String),
    #[error("I/O error while interacting with Herdr: {0}")]
    Io(#[from] std::io::Error),
}

impl From<CommandError> for HerdrError {
    fn from(err: CommandError) -> Self {
        match err {
            CommandError::ProgramNotFound(name) => Self::BinaryNotFound(name),
            CommandError::ExecutionFailed {
                program,
                status,
                stdout,
                stderr,
            } => Self::CommandFailed {
                command: program,
                code: status.code(),
                stderr: if stderr.trim().is_empty() {
                    stdout
                } else {
                    stderr
                },
            },
            CommandError::Io(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
                Self::BinaryNotFound(io_err.to_string())
            }
            CommandError::Io(io_err) => Self::Io(io_err),
        }
    }
}

/// Represents the result of creating a new Herdr tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedTab {
    pub(crate) tab_id: String,
    pub(crate) root_pane_id: String,
    pub(crate) label: String,
    pub(crate) workspace_id: Option<String>,
}

/// Represents information about an existing Herdr tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabInfo {
    pub(crate) tab_id: String,
    pub(crate) label: Option<String>,
    pub(crate) workspace_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrTabCreateResponse {
    result: Option<HerdrTabCreateResult>,
    error: Option<HerdrErrorPayload>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrTabCreateResult {
    root_pane: Option<HerdrPaneInfo>,
    tab: Option<HerdrTabInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrPaneInfo {
    pane_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrTabInfo {
    tab_id: String,
    label: Option<String>,
    workspace_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrTabGetResponse {
    result: Option<HerdrTabGetResult>,
    error: Option<HerdrErrorPayload>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrTabGetResult {
    tab: Option<HerdrTabInfo>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrResponseEnvelope {
    error: Option<HerdrErrorPayload>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct HerdrErrorPayload {
    message: String,
}

/// Interface for interacting with a Herdr multiplexer session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Herdr {
    binary: PathBuf,
    workspace_id: Option<String>,
    tab_id: Option<String>,
    pane_id: Option<String>,
    socket_path: Option<PathBuf>,
}

impl Herdr {
    /// Returns `true` if `aiw` is currently running inside a Herdr-managed pane (`HERDR_ENV=1`).
    #[must_use]
    pub fn is_inside_herdr() -> bool {
        Self::is_inside_herdr_with(|k| std::env::var(k).ok())
    }

    /// Checks if running inside Herdr using a custom environment variable lookup function.
    #[must_use]
    pub(crate) fn is_inside_herdr_with<F>(var_getter: F) -> bool
    where
        F: Fn(&str) -> Option<String>,
    {
        var_getter("HERDR_ENV").is_some_and(|val| val == "1")
    }

    /// Constructs a `Herdr` instance using configuration and identifiers from the environment.
    /// Returns `None` if `aiw` is not running inside a Herdr environment (`HERDR_ENV != "1"`).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        if !Self::is_inside_herdr() {
            return None;
        }
        Self::from_env_with(|k| std::env::var(k).ok())
    }

    /// Constructs a `Herdr` instance using a custom environment variable lookup function.
    #[must_use]
    pub(crate) fn from_env_with<F>(var_getter: F) -> Option<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        if !Self::is_inside_herdr_with(&var_getter) {
            return None;
        }

        let binary = var_getter("HERDR_BIN_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("herdr"));

        Some(Self {
            binary,
            workspace_id: var_getter("HERDR_WORKSPACE_ID"),
            tab_id: var_getter("HERDR_TAB_ID"),
            pane_id: var_getter("HERDR_PANE_ID"),
            socket_path: var_getter("HERDR_SOCKET_PATH").map(PathBuf::from),
        })
    }

    /// Caller pane ID, if known.
    #[must_use]
    pub(crate) fn pane_id(&self) -> Option<&str> {
        self.pane_id.as_deref()
    }

    fn execute_cmd(&self, subcommand: HerdrSubcommand) -> Result<String, HerdrError> {
        let cmd = HerdrCli {
            binary: self.binary.clone(),
            subcommand,
        };

        let output = cmd.output_unchecked()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            if let Ok(err_envelope) = serde_json::from_str::<HerdrResponseEnvelope>(&stderr)
                .or_else(|_| serde_json::from_str::<HerdrResponseEnvelope>(&stdout))
                && let Some(err_obj) = err_envelope.error
            {
                return Err(HerdrError::Api(err_obj.message));
            }

            return Err(HerdrError::CommandFailed {
                command: cmd.to_string(),
                code: output.status.code(),
                stderr: if stderr.trim().is_empty() {
                    stdout
                } else {
                    stderr
                },
            });
        }

        Ok(stdout)
    }

    /// Fetches information about a specific tab by tab ID.
    pub(crate) fn get_tab(&self, tab_id: &str) -> Result<TabInfo, HerdrError> {
        let stdout = self.execute_cmd(HerdrSubcommand::Tab(HerdrTabAction::Get {
            tab_id: tab_id.to_string(),
        }))?;
        let resp: HerdrTabGetResponse = serde_json::from_str(&stdout)?;

        if let Some(err) = resp.error {
            return Err(HerdrError::Api(err.message));
        }

        let tab_info = resp
            .result
            .and_then(|r| r.tab)
            .ok_or(HerdrError::MissingResponseField("result.tab"))?;

        Ok(TabInfo {
            tab_id: tab_info.tab_id,
            label: tab_info.label,
            workspace_id: tab_info.workspace_id,
        })
    }

    /// Fetches information about the current caller tab (`HERDR_TAB_ID`).
    pub(crate) fn current_tab(&self) -> Result<TabInfo, HerdrError> {
        let tab_id = self
            .tab_id
            .as_deref()
            .ok_or_else(|| HerdrError::MissingContext("HERDR_TAB_ID is not set".to_string()))?;
        self.get_tab(tab_id)
    }

    /// Returns the label of the current tab, if available.
    pub(crate) fn current_tab_label(&self) -> Result<Option<String>, HerdrError> {
        Ok(self.current_tab()?.label)
    }

    /// Returns `true` if the current tab's label matches `expected_name`.
    #[must_use]
    pub(crate) fn is_current_tab_named(&self, expected_name: &str) -> bool {
        self.current_tab_label()
            .ok()
            .flatten()
            .is_some_and(|label| label == expected_name)
    }

    /// Creates a new tab with the specified label, optional working directory, and focus setting.
    pub(crate) fn create_tab(
        &self,
        label: &str,
        cwd: Option<&Path>,
        focus: bool,
    ) -> Result<CreatedTab, HerdrError> {
        let stdout = self.execute_cmd(HerdrSubcommand::Tab(HerdrTabAction::Create {
            label: label.to_string(),
            focus,
            cwd: cwd.map(Path::to_path_buf),
            workspace: self.workspace_id.clone(),
        }))?;
        let resp: HerdrTabCreateResponse = serde_json::from_str(&stdout)?;

        if let Some(err) = resp.error {
            return Err(HerdrError::Api(err.message));
        }

        let result = resp
            .result
            .ok_or(HerdrError::MissingResponseField("result"))?;

        let root_pane = result
            .root_pane
            .ok_or(HerdrError::MissingResponseField("result.root_pane"))?;

        let tab = result
            .tab
            .ok_or(HerdrError::MissingResponseField("result.tab"))?;

        Ok(CreatedTab {
            tab_id: tab.tab_id,
            root_pane_id: root_pane.pane_id,
            label: tab.label.unwrap_or_else(|| label.to_string()),
            workspace_id: tab.workspace_id,
        })
    }

    /// Reports agent lifecycle state to Herdr for the given pane.
    pub(crate) fn report_agent(
        &self,
        pane_id: &str,
        agent: &str,
        state: &str,
    ) -> Result<(), HerdrError> {
        self.execute_cmd(HerdrSubcommand::Pane(HerdrPaneAction::ReportAgent {
            source: "aiw".to_string(),
            agent: agent.to_string(),
            state: state.to_string(),
            pane_id: pane_id.to_string(),
        }))?;
        Ok(())
    }

    /// Informs Herdr that the specified agent is running in the given pane.
    pub(crate) fn inform_agent(&self, pane_id: &str, agent: &str) -> Result<(), HerdrError> {
        self.report_agent(pane_id, agent, "working")
    }

    /// Executes a command in the specified pane via `herdr pane run`.
    pub(crate) fn execute_in_pane(&self, pane_id: &str, command: &str) -> Result<(), HerdrError> {
        self.execute_cmd(HerdrSubcommand::Pane(HerdrPaneAction::Run {
            pane_id: pane_id.to_string(),
            command: command.to_string(),
        }))?;
        Ok(())
    }

    /// Creates a tab named after `workspace_name`, informs Herdr about the agent being run,
    /// and executes `command` in the newly created tab, closing the pane when the command exits.
    pub(crate) fn open_workspace_tab_and_execute(
        &self,
        workspace_name: &str,
        command: &str,
        agent: &str,
        cwd: Option<&Path>,
    ) -> Result<CreatedTab, HerdrError> {
        let tab = self.create_tab(workspace_name, cwd, true)?;
        self.inform_agent(&tab.root_pane_id, agent)?;

        // Give the shell process in the newly created pane a brief moment to initialize its prompt
        std::thread::sleep(std::time::Duration::from_millis(300));

        let pane_command = format!("{command}; exit");
        self.execute_in_pane(&tab.root_pane_id, &pane_command)?;
        Ok(tab)
    }

    fn execute_in_place(&self, command: &str, cwd: Option<&Path>) -> Result<(), HerdrError> {
        ShCommand {
            current_dir: cwd.map(Path::to_path_buf),
            command: command.to_string(),
        }
        .run()
        .map_err(|err| match err {
            CommandError::ExecutionFailed { status, .. } => HerdrError::CommandFailed {
                command: command.to_string(),
                code: status.code(),
                stderr: String::new(),
            },
            other => HerdrError::from(other),
        })
    }

    /// Takes a command, decides how to run it based on Herdr context, and executes it.
    ///
    /// - If currently inside a tab named after `workspace_name`: informs Herdr about the agent on the current pane and executes the command in place.
    /// - If not inside the workspace tab: creates a new tab named after `workspace_name`, informs Herdr about the agent, and executes the command in that tab.
    pub fn run_command(
        &self,
        workspace_name: &str,
        command: &str,
        agent: &str,
        cwd: Option<&Path>,
    ) -> Result<(), HerdrError> {
        if self.is_current_tab_named(workspace_name) {
            if let Some(pane_id) = self.pane_id() {
                self.inform_agent(pane_id, agent)?;
            }
            self.execute_in_place(command, cwd)
        } else {
            self.open_workspace_tab_and_execute(workspace_name, command, agent, cwd)?;
            Ok(())
        }
    }
}

#[cfg(test)]
impl Herdr {
    pub(crate) fn mock() -> Self {
        Self::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_herdr.sh"))
    }

    pub(crate) fn new(binary: PathBuf) -> Self {
        Self {
            binary,
            workspace_id: None,
            tab_id: None,
            pane_id: None,
            socket_path: None,
        }
    }

    pub(crate) fn with_tab_id(mut self, tab_id: impl Into<String>) -> Self {
        self.tab_id = Some(tab_id.into());
        self
    }

    pub(crate) fn with_pane_id(mut self, pane_id: impl Into<String>) -> Self {
        self.pane_id = Some(pane_id.into());
        self
    }

    pub(crate) fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    pub(crate) fn binary(&self) -> &Path {
        &self.binary
    }

    pub(crate) fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub(crate) fn tab_id(&self) -> Option<&str> {
        self.tab_id.as_deref()
    }

    pub(crate) fn socket_path(&self) -> Option<&Path> {
        self.socket_path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[googletest::test]
    fn is_inside_herdr_detects_herdr_env_variable() {
        expect_that!(
            Herdr::is_inside_herdr_with(|k| if k == "HERDR_ENV" {
                Some("1".to_string())
            } else {
                None
            }),
            is_true()
        );
        expect_that!(
            Herdr::is_inside_herdr_with(|k| if k == "HERDR_ENV" {
                Some("0".to_string())
            } else {
                None
            }),
            is_false()
        );
        expect_that!(Herdr::is_inside_herdr_with(|_| None), is_false());
    }

    #[googletest::test]
    fn from_env_returns_some_when_herdr_env_is_1_and_none_otherwise() {
        let vars = [
            ("HERDR_ENV", "1"),
            ("HERDR_PANE_ID", "w1:p1"),
            ("HERDR_TAB_ID", "w1:t1"),
            ("HERDR_WORKSPACE_ID", "w1"),
        ];
        let get_var = |k: &str| {
            vars.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_string())
        };

        let detected = Herdr::from_env_with(get_var);
        expect_that!(detected, some(anything()));
        let Some(herdr) = detected else {
            return;
        };
        expect_that!(herdr.pane_id(), some(eq("w1:p1")));
        expect_that!(herdr.tab_id(), some(eq("w1:t1")));
        expect_that!(herdr.workspace_id(), some(eq("w1")));

        expect_that!(Herdr::from_env_with(|_| None), none());
    }

    #[googletest::test]
    fn from_env_populates_all_environment_context() {
        let vars = [
            ("HERDR_ENV", "1"),
            ("HERDR_BIN_PATH", "/custom/bin/herdr"),
            ("HERDR_WORKSPACE_ID", "ws-42"),
            ("HERDR_TAB_ID", "ws-42:t9"),
            ("HERDR_PANE_ID", "ws-42:p9"),
            ("HERDR_SOCKET_PATH", "/custom/socket.sock"),
        ];
        let get_var = |k: &str| {
            vars.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| (*v).to_string())
        };

        let detected = Herdr::from_env_with(get_var);
        expect_that!(detected, some(anything()));
        let Some(herdr) = detected else {
            return;
        };
        expect_that!(herdr.binary(), eq(Path::new("/custom/bin/herdr")));
        expect_that!(herdr.workspace_id(), some(eq("ws-42")));
        expect_that!(herdr.tab_id(), some(eq("ws-42:t9")));
        expect_that!(herdr.pane_id(), some(eq("ws-42:p9")));
        expect_that!(
            herdr.socket_path(),
            some(eq(Path::new("/custom/socket.sock")))
        );
    }

    #[googletest::test]
    fn create_tab_parses_json_response_and_returns_created_tab() -> TestResult {
        let herdr = Herdr::mock().with_workspace_id("w1");

        let res = herdr.create_tab("my-workspace", Some(Path::new("/my/cwd")), true);
        expect_that!(res, ok(anything()));
        let created = res?;
        expect_that!(&created.tab_id, eq("w1:t8"));
        expect_that!(&created.root_pane_id, eq("w1:p8"));
        expect_that!(&created.label, eq("my-workspace"));
        expect_that!(created.workspace_id.as_deref(), some(eq("w1")));
        Ok(())
    }

    #[googletest::test]
    fn get_tab_and_is_current_tab_named_checks_label() -> TestResult {
        let herdr = Herdr::mock().with_tab_id("w1:t3");

        expect_that!(herdr.is_current_tab_named("feature-branch"), is_true());
        expect_that!(herdr.is_current_tab_named("different-branch"), is_false());

        let tab = herdr.get_tab("w1:t3")?;
        expect_that!(tab.label.as_deref(), some(eq("feature-branch")));
        Ok(())
    }

    #[googletest::test]
    fn report_agent_and_inform_agent_sends_expected_cli_arguments() -> TestResult {
        let herdr = Herdr::mock();
        expect_that!(herdr.inform_agent("w1:p99", "agy"), ok(anything()));
        Ok(())
    }

    #[googletest::test]
    fn execute_in_pane_sends_command_via_pane_run() -> TestResult {
        let herdr = Herdr::mock();
        expect_that!(
            herdr.execute_in_pane("w1:p55", "echo hello world"),
            ok(anything())
        );
        Ok(())
    }

    #[googletest::test]
    fn run_command_decides_to_create_tab_when_not_in_workspace_tab() -> TestResult {
        let herdr = Herdr::mock().with_tab_id("w1:t1");

        let res = herdr.run_command(
            "ws-test",
            "fence -- echo 1",
            "agy",
            Some(Path::new("/tmp/test")),
        );
        expect_that!(res, ok(anything()));
        Ok(())
    }

    #[googletest::test]
    fn run_command_decides_to_run_in_place_when_already_in_workspace_tab() -> TestResult {
        let herdr = Herdr::mock().with_tab_id("w1:t5").with_pane_id("w1:p5");

        let res = herdr.run_command("ws-current", "true", "agy", None);
        expect_that!(res, ok(anything()));
        Ok(())
    }

    #[googletest::test]
    fn command_failure_with_api_error_parses_error_message() {
        let herdr = Herdr::mock();

        let res = herdr.get_tab("w1:t99");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(HerdrError::Api(eq(
                "tab w1:t99 not found"
            )))))
        );
    }

    #[googletest::test]
    fn binary_not_found_returns_clear_error() {
        let herdr = Herdr::new(PathBuf::from("/nonexistent/path/to/herdr"));
        let res = herdr.get_tab("w1:t1");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(
                HerdrError::BinaryNotFound(anything())
            )))
        );
    }
}
