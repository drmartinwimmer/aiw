use crate::direnv::ensure_user_profile_bin_paths;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    pub(crate) fn is_inside_herdr() -> bool {
        std::env::var("HERDR_ENV").is_ok_and(|val| val == "1")
    }

    /// Constructs a `Herdr` instance using configuration and identifiers from the environment.
    /// Returns `None` if `aiw` is not running inside a Herdr environment (`HERDR_ENV != "1"`).
    #[must_use]
    pub fn from_env() -> Option<Self> {
        if !Self::is_inside_herdr() {
            return None;
        }

        let binary = std::env::var_os("HERDR_BIN_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("herdr"));

        Some(Self {
            binary,
            workspace_id: std::env::var("HERDR_WORKSPACE_ID").ok(),
            tab_id: std::env::var("HERDR_TAB_ID").ok(),
            pane_id: std::env::var("HERDR_PANE_ID").ok(),
            socket_path: std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from),
        })
    }

    /// Caller pane ID, if known.
    #[must_use]
    pub(crate) fn pane_id(&self) -> Option<&str> {
        self.pane_id.as_deref()
    }

    fn execute_cmd(&self, args: &[&str]) -> Result<String, HerdrError> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(args);
        ensure_user_profile_bin_paths(&mut cmd);

        let output = {
            let mut attempts = 0;
            loop {
                match cmd.output() {
                    Ok(out) => break Ok(out),
                    Err(err) if err.raw_os_error() == Some(26) && attempts < 10 => {
                        attempts += 1;
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        break Err(HerdrError::BinaryNotFound(
                            self.binary.to_string_lossy().into_owned(),
                        ));
                    }
                    Err(err) => break Err(HerdrError::Io(err)),
                }
            }
        }?;

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
                command: format!("{} {}", self.binary.to_string_lossy(), args.join(" ")),
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
        let stdout = self.execute_cmd(&["tab", "get", tab_id])?;
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
        let mut args = vec!["tab", "create", "--label", label];
        if focus {
            args.push("--focus");
        } else {
            args.push("--no-focus");
        }

        let cwd_str;
        if let Some(c) = cwd {
            cwd_str = c.to_string_lossy().to_string();
            args.extend(["--cwd", &cwd_str]);
        }

        if let Some(ws) = &self.workspace_id {
            args.extend(["--workspace", ws]);
        }

        let stdout = self.execute_cmd(&args)?;
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
        self.execute_cmd(&[
            "pane",
            "report-agent",
            "--source",
            "aiw",
            "--agent",
            agent,
            "--state",
            state,
            pane_id,
        ])?;
        Ok(())
    }

    /// Informs Herdr that the specified agent is running in the given pane.
    pub(crate) fn inform_agent(&self, pane_id: &str, agent: &str) -> Result<(), HerdrError> {
        self.report_agent(pane_id, agent, "working")
    }

    /// Executes a command in the specified pane via `herdr pane run`.
    pub(crate) fn execute_in_pane(&self, pane_id: &str, command: &str) -> Result<(), HerdrError> {
        self.execute_cmd(&["pane", "run", pane_id, command])?;
        Ok(())
    }

    /// Creates a tab named after `workspace_name`, informs Herdr about the agent being run,
    /// and executes `command` in the newly created tab.
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

        self.execute_in_pane(&tab.root_pane_id, command)?;
        Ok(tab)
    }

    fn execute_in_place(&self, command: &str, cwd: Option<&Path>) -> Result<(), HerdrError> {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        cmd.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        ensure_user_profile_bin_paths(&mut cmd);

        let status = cmd.status()?;
        if !status.success() {
            return Err(HerdrError::CommandFailed {
                command: command.to_string(),
                code: status.code(),
                stderr: String::new(),
            });
        }
        Ok(())
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
            if let Some(pane_id) = self.pane_id()
                && let Err(err) = self.inform_agent(pane_id, agent)
            {
                eprintln!("Warning: failed to inform Herdr about agent: {err}");
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
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn set_env(key: &str, val: &str) {
        // SAFETY: Test functions serialize environment modifications via ENV_MUTEX.
        unsafe {
            std::env::set_var(key, val);
        }
    }

    fn remove_env(key: &str) {
        // SAFETY: Test functions serialize environment modifications via ENV_MUTEX.
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn create_mock_script(script_content: &str) -> (tempfile::TempDir, PathBuf) {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let script_path = temp_dir.path().join("mock_herdr.sh");
        std::fs::write(&script_path, script_content).expect("write mock script");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&script_path, perms).expect("set permissions");
        }

        (temp_dir, script_path)
    }

    #[googletest::test]
    fn is_inside_herdr_detects_herdr_env_variable() {
        let _guard = ENV_MUTEX.lock().expect("lock env mutex");

        set_env("HERDR_ENV", "1");
        expect_that!(Herdr::is_inside_herdr(), is_true());

        set_env("HERDR_ENV", "0");
        expect_that!(Herdr::is_inside_herdr(), is_false());

        remove_env("HERDR_ENV");
        expect_that!(Herdr::is_inside_herdr(), is_false());
    }

    #[googletest::test]
    fn from_env_returns_some_when_herdr_env_is_1_and_none_otherwise() {
        let _guard = ENV_MUTEX.lock().expect("lock env mutex");

        set_env("HERDR_ENV", "1");
        set_env("HERDR_PANE_ID", "w1:p1");
        set_env("HERDR_TAB_ID", "w1:t1");
        set_env("HERDR_WORKSPACE_ID", "w1");

        let detected = Herdr::from_env();
        expect_that!(detected, some(anything()));
        let herdr = detected.expect("detected");
        expect_that!(herdr.pane_id(), some(eq("w1:p1")));
        expect_that!(herdr.tab_id(), some(eq("w1:t1")));
        expect_that!(herdr.workspace_id(), some(eq("w1")));

        remove_env("HERDR_ENV");
        expect_that!(Herdr::from_env(), none());
    }

    #[googletest::test]
    fn from_env_populates_all_environment_context() {
        let _guard = ENV_MUTEX.lock().expect("lock env mutex");

        set_env("HERDR_ENV", "1");
        set_env("HERDR_BIN_PATH", "/custom/bin/herdr");
        set_env("HERDR_WORKSPACE_ID", "ws-42");
        set_env("HERDR_TAB_ID", "ws-42:t9");
        set_env("HERDR_PANE_ID", "ws-42:p9");
        set_env("HERDR_SOCKET_PATH", "/custom/socket.sock");

        let detected = Herdr::from_env();
        expect_that!(detected, some(anything()));
        let herdr = detected.expect("from_env");
        expect_that!(herdr.binary(), eq(Path::new("/custom/bin/herdr")));
        expect_that!(herdr.workspace_id(), some(eq("ws-42")));
        expect_that!(herdr.tab_id(), some(eq("ws-42:t9")));
        expect_that!(herdr.pane_id(), some(eq("ws-42:p9")));
        expect_that!(
            herdr.socket_path(),
            some(eq(Path::new("/custom/socket.sock")))
        );

        remove_env("HERDR_ENV");
        remove_env("HERDR_BIN_PATH");
        remove_env("HERDR_WORKSPACE_ID");
        remove_env("HERDR_TAB_ID");
        remove_env("HERDR_PANE_ID");
        remove_env("HERDR_SOCKET_PATH");
    }

    #[googletest::test]
    fn create_tab_parses_json_response_and_returns_created_tab() {
        let script = r#"#!/bin/sh
if [ "$1" = "tab" ] && [ "$2" = "create" ]; then
    echo '{"id":"cli:tab:create","result":{"root_pane":{"pane_id":"w1:p8"},"tab":{"label":"my-workspace","tab_id":"w1:t8","workspace_id":"w1"},"type":"tab_created"}}'
    exit 0
fi
echo "Unexpected args: $*" >&2
exit 1
"#;
        let (_dir, script_path) = create_mock_script(script);
        let herdr = Herdr::new(script_path).with_workspace_id("w1");

        let res = herdr.create_tab("my-workspace", Some(Path::new("/my/cwd")), true);
        expect_that!(res, ok(anything()));
        let created = res.expect("created tab");
        expect_that!(&created.tab_id, eq("w1:t8"));
        expect_that!(&created.root_pane_id, eq("w1:p8"));
        expect_that!(&created.label, eq("my-workspace"));
        expect_that!(created.workspace_id.as_deref(), some(eq("w1")));
    }

    #[googletest::test]
    fn get_tab_and_is_current_tab_named_checks_label() {
        let script = r#"#!/bin/sh
if [ "$1" = "tab" ] && [ "$2" = "get" ] && [ "$3" = "w1:t3" ]; then
    echo '{"id":"cli:tab:get","result":{"tab":{"label":"feature-branch","number":3,"tab_id":"w1:t3","workspace_id":"w1"},"type":"tab_info"}}'
    exit 0
fi
echo "Unexpected args: $*" >&2
exit 1
"#;
        let (_dir, script_path) = create_mock_script(script);
        let herdr = Herdr::new(script_path).with_tab_id("w1:t3");

        expect_that!(herdr.is_current_tab_named("feature-branch"), is_true());
        expect_that!(herdr.is_current_tab_named("different-branch"), is_false());

        let tab = herdr.get_tab("w1:t3").expect("get_tab");
        expect_that!(tab.label.as_deref(), some(eq("feature-branch")));
    }

    #[googletest::test]
    fn report_agent_and_inform_agent_sends_expected_cli_arguments() {
        let log_file = tempfile::NamedTempFile::new().expect("temp file");
        let log_path = log_file.path().to_string_lossy().to_string();

        let script = format!(
            r#"#!/bin/sh
echo "$*" >> "{log_path}"
echo '{{"id":"cli:pane:report_agent","result":{{"type":"ok"}}}}'
exit 0
"#
        );
        let (_dir, script_path) = create_mock_script(&script);
        let herdr = Herdr::new(script_path);

        herdr
            .inform_agent("w1:p99", "agy")
            .expect("inform_agent should succeed");

        let logged = std::fs::read_to_string(&log_path).expect("read log");
        expect_that!(
            &logged,
            contains_substring("pane report-agent --source aiw --agent agy --state working w1:p99")
        );
    }

    #[googletest::test]
    fn execute_in_pane_sends_command_via_pane_run() {
        let log_file = tempfile::NamedTempFile::new().expect("temp file");
        let log_path = log_file.path().to_string_lossy().to_string();

        let script = format!(
            r#"#!/bin/sh
echo "$*" >> "{log_path}"
echo '{{"id":"cli:pane:run","result":{{"type":"ok"}}}}'
exit 0
"#
        );
        let (_dir, script_path) = create_mock_script(&script);
        let herdr = Herdr::new(script_path);

        herdr
            .execute_in_pane("w1:p55", "echo hello world")
            .expect("execute_in_pane should succeed");

        let logged = std::fs::read_to_string(&log_path).expect("read log");
        expect_that!(
            &logged,
            contains_substring("pane run w1:p55 echo hello world")
        );
    }

    #[googletest::test]
    fn run_command_decides_to_create_tab_when_not_in_workspace_tab() {
        let log_file = tempfile::NamedTempFile::new().expect("temp file");
        let log_path = log_file.path().to_string_lossy().to_string();

        let script = format!(
            r#"#!/bin/sh
echo "$*" >> "{log_path}"
if [ "$1" = "tab" ] && [ "$2" = "get" ]; then
    echo '{{"id":"cli:tab:get","result":{{"tab":{{"label":"other-tab","tab_id":"w1:t1","workspace_id":"w1"}},"type":"tab_info"}}}}'
    exit 0
fi
if [ "$1" = "tab" ] && [ "$2" = "create" ]; then
    echo '{{"id":"cli:tab:create","result":{{"root_pane":{{"pane_id":"w1:p12"}},"tab":{{"label":"ws-test","tab_id":"w1:t12","workspace_id":"w1"}},"type":"tab_created"}}}}'
    exit 0
fi
echo '{{"result":{{"type":"ok"}}}}'
exit 0
"#
        );
        let (_dir, script_path) = create_mock_script(&script);
        let herdr = Herdr::new(script_path).with_tab_id("w1:t1");

        let res = herdr.run_command(
            "ws-test",
            "fence -- echo 1",
            "agy",
            Some(Path::new("/tmp/test")),
        );
        expect_that!(res, ok(anything()));

        let logged = std::fs::read_to_string(&log_path).expect("read log");
        expect_that!(
            &logged,
            contains_substring("tab create --label ws-test --focus --cwd /tmp/test")
        );
        expect_that!(
            &logged,
            contains_substring("pane report-agent --source aiw --agent agy --state working w1:p12")
        );
        expect_that!(
            &logged,
            contains_substring("pane run w1:p12 fence -- echo 1")
        );
    }

    #[googletest::test]
    fn run_command_decides_to_run_in_place_when_already_in_workspace_tab() {
        let log_file = tempfile::NamedTempFile::new().expect("temp file");
        let log_path = log_file.path().to_string_lossy().to_string();

        let script = format!(
            r#"#!/bin/sh
echo "$*" >> "{log_path}"
if [ "$1" = "tab" ] && [ "$2" = "get" ]; then
    echo '{{"id":"cli:tab:get","result":{{"tab":{{"label":"ws-current","tab_id":"w1:t5","workspace_id":"w1"}},"type":"tab_info"}}}}'
    exit 0
fi
echo '{{"result":{{"type":"ok"}}}}'
exit 0
"#
        );
        let (_dir, script_path) = create_mock_script(&script);
        let herdr = Herdr::new(script_path)
            .with_tab_id("w1:t5")
            .with_pane_id("w1:p5");

        let res = herdr.run_command("ws-current", "true", "agy", None);
        expect_that!(res, ok(anything()));

        let logged = std::fs::read_to_string(&log_path).expect("read log");
        // Must inform Herdr about agent on current pane
        expect_that!(
            &logged,
            contains_substring("pane report-agent --source aiw --agent agy --state working w1:p5")
        );
        // Must NOT create a new tab
        expect_that!(&logged, not(contains_substring("tab create")));
    }

    #[googletest::test]
    fn command_failure_with_api_error_parses_error_message() {
        let script = r#"#!/bin/sh
echo '{"error":{"code":"tab_not_found","message":"tab w1:t99 not found"},"id":"cli:tab:get"}' >&2
exit 1
"#;
        let (_dir, script_path) = create_mock_script(script);
        let herdr = Herdr::new(script_path);

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
