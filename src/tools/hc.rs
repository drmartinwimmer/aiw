use clapback::{Command, CommandError, CommandExt as _};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Command, Debug)]
#[command(ensure_user_path)]
struct HcCli {
    #[command(program)]
    binary: PathBuf,

    #[command(subcommand)]
    subcommand: HcSubcommand,
}

#[derive(clapback::Subcommand, Debug)]
enum HcSubcommand {
    #[command(name = "get", arg = "--json")]
    Get {
        terminal: String,
    },

    #[command(name = "ls", arg = "--json")]
    Ls {
        #[arg(long)]
        label: Vec<String>,
    },

    #[command(name = "label", arg = "--json")]
    Label {
        terminal: String,
        labels: Vec<String>,
    },

    #[command(name = "run", args = ["--json", "--bg", "--no-parent", "--rm"])]
    Run {
        #[arg(long)]
        name: Option<String>,

        #[arg(long)]
        cwd: Option<PathBuf>,

        #[arg(long)]
        label: Vec<String>,

        command: String,
    },

    Attach {
        terminal: String,
    },
}

#[derive(Command, Debug)]
#[command(program = "sh", arg = "-c", ensure_user_path)]
struct ShCommand {
    #[command(current_dir)]
    current_dir: Option<PathBuf>,

    command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HcError {
    #[error("herdr-core (hc) binary not found: {0}")]
    BinaryNotFound(String),
    #[error("hc command '{command}' failed (exit code {code:?}): {stderr}")]
    CommandFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("Failed to parse hc JSON output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("hc API returned error ({kind}): {message}")]
    Api { kind: String, message: String },
    #[error("I/O error while interacting with herdr-core: {0}")]
    Io(#[from] std::io::Error),
}

impl From<CommandError> for HcError {
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

#[derive(Debug, serde::Deserialize)]
struct TerminalInfo {
    address: String,
    #[serde(default)]
    name: Option<String>,
    state: String,
    #[serde(default)]
    labels: HashMap<String, String>,
}

#[derive(Debug, serde::Deserialize)]
struct HcListResponse {
    terminals: Vec<HcListedTerminal>,
}

#[derive(Debug, serde::Deserialize)]
struct HcListedTerminal {
    address: String,
    state: String,
}

#[derive(Debug, serde::Deserialize)]
struct HcRunResponse {
    address: String,
}

#[derive(Debug, serde::Deserialize)]
struct HcErrorEnvelope {
    error: Option<HcErrorPayload>,
}

#[derive(Debug, serde::Deserialize)]
struct HcErrorPayload {
    #[serde(default)]
    kind: String,
    message: String,
}

/// Interface for interacting with a `herdr-core` (`hc`) terminal session.
#[derive(Debug)]
pub(crate) struct Hc {
    binary: PathBuf,
    terminal_id: String,
}

impl Hc {
    /// Constructs an `Hc` instance from the environment if running inside a `herdr-core` terminal
    /// (`HC_TERMINAL_ID` is set and non-empty).
    #[must_use]
    pub(crate) fn from_env() -> Option<Self> {
        let terminal_id = std::env::var("HC_TERMINAL_ID")
            .ok()
            .filter(|val| !val.trim().is_empty())?;
        let binary =
            std::env::var_os("HC_BIN_PATH").map_or_else(|| PathBuf::from("hc"), PathBuf::from);

        Some(Self {
            binary,
            terminal_id,
        })
    }

    /// Normalizes an arbitrary workspace name into a valid `hc` `--name` identifier:
    /// `1-32` characters of `a-z`, `0-9`, `-`, `_`, `.`, starting with a letter or digit,
    /// and not shaped like a core terminal address (`t7`, `t1b`).
    #[must_use]
    fn terminal_name(workspace_name: &str) -> String {
        let mapped: String = workspace_name
            .chars()
            .map(|c| {
                let lower = c.to_ascii_lowercase();
                if lower.is_ascii_lowercase()
                    || lower.is_ascii_digit()
                    || matches!(lower, '-' | '_' | '.')
                {
                    lower
                } else {
                    '-'
                }
            })
            .skip_while(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit())
            .collect();

        let prefixed = if mapped.is_empty() {
            "ws".to_string()
        } else if Self::is_shaped_like_terminal_id(&mapped) {
            format!("ws-{mapped}")
        } else {
            mapped
        };

        prefixed.chars().take(32).collect()
    }

    #[must_use]
    fn is_shaped_like_terminal_id(candidate: &str) -> bool {
        let mut chars = candidate.chars();
        if chars.next() != Some('t') {
            return false;
        }
        let Some(second) = chars.next() else {
            return false;
        };
        second.is_ascii_digit() && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    }

    fn execute_cmd(&self, subcommand: HcSubcommand) -> Result<String, HcError> {
        let cmd = HcCli {
            binary: self.binary.clone(),
            subcommand,
        };

        let output = cmd.output_unchecked()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            if let Ok(err_envelope) = serde_json::from_str::<HcErrorEnvelope>(&stderr)
                .or_else(|_| serde_json::from_str::<HcErrorEnvelope>(&stdout))
                && let Some(err_obj) = err_envelope.error
            {
                return Err(HcError::Api {
                    kind: err_obj.kind,
                    message: err_obj.message,
                });
            }

            return Err(HcError::CommandFailed {
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

    fn get_terminal(&self, terminal: &str) -> Result<TerminalInfo, HcError> {
        let stdout = self.execute_cmd(HcSubcommand::Get {
            terminal: terminal.to_string(),
        })?;
        Ok(serde_json::from_str(&stdout)?)
    }

    #[must_use]
    fn is_current_terminal_for_workspace(&self, workspace_name: &str) -> bool {
        let Ok(term) = self.get_terminal(&self.terminal_id) else {
            return false;
        };

        if let Some(ws_label) = term.labels.get("workspace") {
            return ws_label == workspace_name;
        }

        let sanitized = Self::terminal_name(workspace_name);
        term.name
            .as_deref()
            .is_some_and(|name| name == workspace_name || name == sanitized)
    }

    fn find_live_workspace_terminal(
        &self,
        workspace_name: &str,
    ) -> Result<Option<String>, HcError> {
        let stdout = self.execute_cmd(HcSubcommand::Ls {
            label: vec![format!("workspace={workspace_name}")],
        })?;
        let list_resp: HcListResponse = serde_json::from_str(&stdout)?;
        if let Some(found) = list_resp.terminals.into_iter().find(|t| t.state == "live") {
            return Ok(Some(found.address));
        }

        let candidate_name = Self::terminal_name(workspace_name);
        match self.get_terminal(&candidate_name) {
            Ok(term)
                if term.state == "live"
                    && term
                        .labels
                        .get("workspace")
                        .is_none_or(|ws| ws == workspace_name) =>
            {
                Ok(Some(term.address))
            }
            Ok(_) => Ok(None),
            Err(HcError::Api { ref kind, .. }) if kind == "not_found" => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn is_terminal_name_available(
        &self,
        candidate_name: &str,
        workspace_name: &str,
    ) -> Result<bool, HcError> {
        match self.get_terminal(candidate_name) {
            Ok(term) if term.state == "live" => Ok(term
                .labels
                .get("workspace")
                .is_none_or(|ws| ws == workspace_name)),
            Ok(_) => Ok(true),
            Err(HcError::Api { ref kind, .. }) if kind == "not_found" => Ok(true),
            Err(err) => Err(err),
        }
    }

    fn inform_agent(
        &self,
        terminal: &str,
        workspace_name: &str,
        agent: &str,
    ) -> Result<(), HcError> {
        self.execute_cmd(HcSubcommand::Label {
            terminal: terminal.to_string(),
            labels: vec![
                "source=aiw".to_string(),
                format!("agent={agent}"),
                format!("workspace={workspace_name}"),
            ],
        })?;
        Ok(())
    }

    fn create_terminal(
        &self,
        workspace_name: &str,
        command: &str,
        agent: &str,
        cwd: Option<&Path>,
    ) -> Result<String, HcError> {
        let candidate_name = Self::terminal_name(workspace_name);
        let name = if self.is_terminal_name_available(&candidate_name, workspace_name)? {
            Some(candidate_name)
        } else {
            None
        };

        let stdout = self.execute_cmd(HcSubcommand::Run {
            name,
            cwd: cwd.map(Path::to_path_buf),
            label: vec![
                "source=aiw".to_string(),
                format!("agent={agent}"),
                format!("workspace={workspace_name}"),
            ],
            command: command.to_string(),
        })?;
        let resp: HcRunResponse = serde_json::from_str(&stdout)?;
        Ok(resp.address)
    }

    fn attach_terminal(&self, terminal: &str) -> Result<(), HcError> {
        let cmd = HcCli {
            binary: self.binary.clone(),
            subcommand: HcSubcommand::Attach {
                terminal: terminal.to_string(),
            },
        };

        cmd.run().map_err(|err| match err {
            CommandError::ExecutionFailed { status, .. } => HcError::CommandFailed {
                command: cmd.to_string(),
                code: status.code(),
                stderr: String::new(),
            },
            other => HcError::from(other),
        })
    }

    /// Attaches to a newly spawned `--rm` terminal (`hc attach <terminal>`), treating exit codes
    /// `3` (`not_found`) and `11` (`finished`) as clean completion in case a short-lived command
    /// finished before `hc attach` connected.
    fn attach_spawned_terminal(&self, terminal: &str) -> Result<(), HcError> {
        match self.attach_terminal(terminal) {
            Err(HcError::CommandFailed {
                code: Some(3 | 11), ..
            }) => Ok(()),
            other => other,
        }
    }

    fn execute_in_place(&self, command: &str, cwd: Option<&Path>) -> Result<(), HcError> {
        ShCommand {
            current_dir: cwd.map(Path::to_path_buf),
            command: command.to_string(),
        }
        .run()
        .map_err(|err| match err {
            CommandError::ExecutionFailed { status, .. } => HcError::CommandFailed {
                command: command.to_string(),
                code: status.code(),
                stderr: String::new(),
            },
            other => HcError::from(other),
        })
    }

    /// Decides how to run `command` based on the current `herdr-core` context and executes it:
    ///
    /// - If already inside the terminal for `workspace_name`: labels the terminal with agent metadata and runs `command` in place.
    /// - If another live terminal for `workspace_name` already exists: attaches to it.
    /// - Otherwise: spawns a new independent `--rm` terminal named after `workspace_name` and attaches to it.
    pub(crate) fn run_command(
        &self,
        workspace_name: &str,
        command: &str,
        agent: &str,
        cwd: Option<&Path>,
    ) -> Result<(), HcError> {
        if self.is_current_terminal_for_workspace(workspace_name) {
            self.inform_agent(&self.terminal_id, workspace_name, agent)?;
            self.execute_in_place(command, cwd)
        } else if let Some(existing_address) = self.find_live_workspace_terminal(workspace_name)? {
            self.attach_terminal(&existing_address)
        } else {
            let address = self.create_terminal(workspace_name, command, agent, cwd)?;
            self.attach_spawned_terminal(&address)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn mock_hc(terminal_id: &str) -> Hc {
        Hc {
            binary: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh"),
            terminal_id: terminal_id.to_string(),
        }
    }

    #[googletest::test]
    fn terminal_name_normalizes_invalid_characters_length_and_id_shapes() {
        expect_that!(Hc::terminal_name("my-workspace"), eq("my-workspace"));
        expect_that!(Hc::terminal_name("My_Feature.1"), eq("my_feature.1"));
        expect_that!(Hc::terminal_name("---leading"), eq("leading"));
        expect_that!(Hc::terminal_name("..."), eq("ws"));
        expect_that!(Hc::terminal_name("t7"), eq("ws-t7"));
        expect_that!(Hc::terminal_name("t1b"), eq("ws-t1b"));
        expect_that!(Hc::terminal_name("tab"), eq("tab"));

        let long = Hc::terminal_name("abcdefghijklmnopqrstuvwxyz0123456789");
        expect_that!(long.len(), eq(32));
        expect_that!(&long, eq("abcdefghijklmnopqrstuvwxyz012345"));
    }

    #[googletest::test]
    fn create_terminal_parses_json_response_and_reuses_finished_names() -> TestResult {
        let hc = mock_hc("t1");
        let address = hc.create_terminal(
            "my-workspace",
            "fence -- echo 1",
            "agy",
            Some(Path::new("/my/cwd")),
        )?;
        expect_that!(&address, eq("t8"));

        let reused_address = hc.create_terminal("finished-ws", "fence -- echo 1", "agy", None)?;
        expect_that!(&reused_address, eq("t14"));
        Ok(())
    }

    #[googletest::test]
    fn is_current_terminal_for_workspace_checks_name_and_label() {
        let hc_by_name = mock_hc("t3");
        expect_that!(
            hc_by_name.is_current_terminal_for_workspace("feature-branch"),
            is_true()
        );
        expect_that!(
            hc_by_name.is_current_terminal_for_workspace("different-branch"),
            is_false()
        );

        let hc_by_label = mock_hc("t5");
        expect_that!(
            hc_by_label.is_current_terminal_for_workspace("ws-current"),
            is_true()
        );
    }

    #[googletest::test]
    fn run_command_creates_and_attaches_when_not_in_workspace_terminal() -> TestResult {
        let hc = mock_hc("t1");
        let res = hc.run_command(
            "ws-test",
            "fence -- echo 1",
            "agy",
            Some(Path::new("/tmp/test")),
        );
        expect_that!(res, ok(anything()));
        Ok(())
    }

    #[googletest::test]
    fn run_command_attaches_to_existing_live_terminal_when_present() -> TestResult {
        let hc = mock_hc("t1");
        let res = hc.run_command("existing-ws", "fence -- echo 1", "agy", None);
        expect_that!(res, ok(anything()));
        Ok(())
    }

    #[googletest::test]
    fn run_command_runs_in_place_when_already_in_workspace_terminal() -> TestResult {
        let hc = mock_hc("t5");
        let res = hc.run_command("ws-current", "true", "agy", None);
        expect_that!(res, ok(anything()));
        Ok(())
    }

    #[googletest::test]
    fn attach_spawned_terminal_tolerates_already_completed_terminal() {
        let hc = mock_hc("t1");
        expect_that!(hc.attach_spawned_terminal("t404"), ok(anything()));
        expect_that!(
            hc.attach_terminal("t404"),
            err(matches_pattern!(HcError::CommandFailed {
                code: some(eq(&3)),
                ..
            }))
        );
    }

    #[googletest::test]
    fn command_failure_with_api_error_parses_error_kind_and_message() {
        let hc = mock_hc("t1");
        let res = hc.get_terminal("t99");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(HcError::Api {
                kind: eq("not_found"),
                message: eq("no terminal \"t99\""),
            })))
        );
    }

    #[googletest::test]
    fn binary_not_found_returns_clear_error() {
        let hc = Hc {
            binary: PathBuf::from("/nonexistent/path/to/hc"),
            terminal_id: "t1".to_string(),
        };
        let res = hc.get_terminal("t1");
        expect_that!(
            res,
            matches_pattern!(Err(matches_pattern!(HcError::BinaryNotFound(anything()))))
        );
    }
}
