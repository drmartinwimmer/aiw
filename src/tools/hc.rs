use super::hc_doc::{self, HerdrAppDoc};
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

    #[command(name = "keep", arg = "--json")]
    Keep {
        terminal: String,
    },

    #[command(name = "run", args = ["--json", "--bg", "--no-parent", "--rm"])]
    Run {
        #[arg(long)]
        keep: bool,

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

#[derive(Debug)]
struct HerdrSpaceContext {
    space_id: String,
    tab_id: Option<String>,
    tab_name: Option<String>,
    doc: Option<HerdrAppDoc>,
}

/// Interface for interacting with a `herdr-core` (`hc`) terminal session, including
/// remote and local Herdr Desktop spaces synchronized through `hc`'s `"herdr-app"` document.
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

    /// Resolves the enclosing Herdr Desktop space and tab context from the local `hc` daemon
    /// when the caller terminal belongs to a Herdr space (including remote spaces connected
    /// via `hc machine add`), returning `None` in plain headless `hc` sessions.
    #[must_use]
    fn herdr_space_context(&self, caller: Option<&TerminalInfo>) -> Option<HerdrSpaceContext> {
        let caller = caller?;
        let space_hint = caller
            .labels
            .get("herdr.space")
            .map(String::as_str)
            .filter(|s| !s.is_empty());
        let tab_hint = caller
            .labels
            .get("herdr.tab")
            .map(String::as_str)
            .filter(|s| !s.is_empty());
        let has_where = caller
            .labels
            .get("where")
            .is_some_and(|place| !place.is_empty());

        if space_hint.is_none() && !has_where {
            return None;
        }

        let doc = hc_doc::get_doc(&self.binary).ok().map(|snap| snap.doc);
        if let Some(ref d) = doc
            && let Some(loc) = d.locate_terminal(&self.terminal_id, space_hint, tab_hint)
        {
            return Some(HerdrSpaceContext {
                space_id: loc.space_id,
                tab_id: loc.tab_id,
                tab_name: loc.tab_name,
                doc,
            });
        }

        space_hint.map(|space_id| HerdrSpaceContext {
            space_id: space_id.to_owned(),
            tab_id: tab_hint.map(str::to_owned),
            tab_name: None,
            doc,
        })
    }

    #[must_use]
    fn is_caller_for_workspace(
        caller: Option<&TerminalInfo>,
        current_tab_name: Option<&str>,
        workspace_name: &str,
    ) -> bool {
        let Some(term) = caller else {
            return false;
        };

        if let Some(ws_label) = term.labels.get("workspace") {
            return ws_label == workspace_name;
        }

        let sanitized = Self::terminal_name(workspace_name);
        if term
            .name
            .as_deref()
            .is_some_and(|name| name == workspace_name || name == sanitized)
        {
            return true;
        }

        current_tab_name.is_some_and(|tab_name| tab_name == workspace_name)
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

    fn keep_terminal(&self, terminal: &str) -> Result<(), HcError> {
        match self.execute_cmd(HcSubcommand::Keep {
            terminal: terminal.to_string(),
        }) {
            Ok(_) => Ok(()),
            Err(HcError::Api { ref kind, .. }) if kind == "not_found" => Ok(()),
            Err(err) => Err(err),
        }
    }

    fn create_terminal(
        &self,
        workspace_name: &str,
        command: &str,
        agent: &str,
        cwd: Option<&Path>,
        keep: bool,
    ) -> Result<String, HcError> {
        let candidate_name = Self::terminal_name(workspace_name);
        let name = if self.is_terminal_name_available(&candidate_name, workspace_name)? {
            Some(candidate_name)
        } else {
            None
        };

        let stdout = self.execute_cmd(HcSubcommand::Run {
            keep,
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

    /// Places `terminal` as a named `<workspace_name>` tab inside `space_ctx.space_id` via `hc`'s
    /// `"herdr-app"` shared layout document, falling back to `herdr.space` / `herdr.tab` labels
    /// if the space has not yet been written to `"herdr-app"`.
    fn place_terminal_in_herdr_space(
        &self,
        space_ctx: &HerdrSpaceContext,
        workspace_name: &str,
        terminal: &str,
        ensure_keep: bool,
    ) -> Result<(), HcError> {
        if space_ctx
            .doc
            .as_ref()
            .is_some_and(|d| d.contains_terminal(terminal))
        {
            return Ok(());
        }

        if ensure_keep {
            self.keep_terminal(terminal)?;
        }

        let new_tab_id = hc_doc::fresh_id("tb");
        let added = hc_doc::add_terminal_tab(
            &self.binary,
            &space_ctx.space_id,
            space_ctx.tab_id.as_deref(),
            &new_tab_id,
            workspace_name,
            terminal,
        )?;

        if !added {
            let label_res = self.execute_cmd(HcSubcommand::Label {
                terminal: terminal.to_string(),
                labels: vec![
                    format!("herdr.space={}", space_ctx.space_id),
                    format!("herdr.tab={new_tab_id}"),
                ],
            });
            match label_res {
                Ok(_) => {}
                Err(HcError::Api { ref kind, .. }) if kind == "not_found" => {}
                Err(err) => return Err(err),
            }
        }

        Ok(())
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

    /// Decides how to run `command` based on the current `herdr-core` / Herdr space context
    /// and executes it:
    ///
    /// - If already inside the terminal or Herdr tab for `workspace_name`: labels the terminal
    ///   with agent metadata and runs `in_place` directly without shell round-tripping.
    /// - If another live terminal for `workspace_name` already exists: ensures it has a tab in
    ///   the current Herdr space when inside one, or attaches to it in headless `hc`.
    /// - Otherwise: spawns a new independent `--rm` terminal named after `workspace_name` and
    ///   adds it as a named tab in the Herdr space's `"herdr-app"` layout document (or attaches
    ///   to it in headless `hc`).
    pub(crate) fn run_with_in_place<F, E>(
        &self,
        workspace_name: &str,
        command: &str,
        agent: &str,
        cwd: Option<&Path>,
        in_place: F,
    ) -> Result<(), E>
    where
        F: FnOnce() -> Result<(), E>,
        E: From<HcError>,
    {
        let caller = self.get_terminal(&self.terminal_id).ok();
        let space_ctx = self.herdr_space_context(caller.as_ref());
        let current_tab_name = space_ctx.as_ref().and_then(|ctx| ctx.tab_name.as_deref());

        if Self::is_caller_for_workspace(caller.as_ref(), current_tab_name, workspace_name) {
            self.inform_agent(&self.terminal_id, workspace_name, agent)?;
            in_place()
        } else if let Some(existing_address) = self.find_live_workspace_terminal(workspace_name)? {
            if let Some(ref ctx) = space_ctx {
                self.place_terminal_in_herdr_space(ctx, workspace_name, &existing_address, true)?;
            } else {
                self.attach_terminal(&existing_address)?;
            }
            Ok(())
        } else if let Some(ref ctx) = space_ctx {
            let address = self.create_terminal(workspace_name, command, agent, cwd, true)?;
            self.place_terminal_in_herdr_space(ctx, workspace_name, &address, false)?;
            Ok(())
        } else {
            let address = self.create_terminal(workspace_name, command, agent, cwd, false)?;
            self.attach_spawned_terminal(&address)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn mock_hc(terminal_id: &str) -> Hc {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        Hc {
            binary: manifest_dir.join("tests/fixtures/mock_hc.sh"),
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
            false,
        )?;
        expect_that!(&address, eq("t8"));

        let reused_address =
            hc.create_terminal("finished-ws", "fence -- echo 1", "agy", None, false)?;
        expect_that!(&reused_address, eq("t14"));
        Ok(())
    }

    #[googletest::test]
    fn is_caller_for_workspace_checks_label_terminal_name_and_herdr_tab_name() -> TestResult {
        let hc_by_name = mock_hc("t3");
        let caller_t3 = hc_by_name.get_terminal("t3")?;
        expect_that!(
            Hc::is_caller_for_workspace(Some(&caller_t3), None, "feature-branch"),
            is_true()
        );
        expect_that!(
            Hc::is_caller_for_workspace(Some(&caller_t3), None, "different-branch"),
            is_false()
        );

        let hc_by_label = mock_hc("t5");
        let caller_t5 = hc_by_label.get_terminal("t5")?;
        expect_that!(
            Hc::is_caller_for_workspace(Some(&caller_t5), Some("renamed-tab"), "ws-current"),
            is_true()
        );
        expect_that!(
            Hc::is_caller_for_workspace(Some(&caller_t5), Some("renamed-tab"), "renamed-tab"),
            is_false()
        );

        let hc_space = mock_hc("t6");
        let caller_t6 = hc_space.get_terminal("t6")?;
        let space_ctx = hc_space.herdr_space_context(Some(&caller_t6));
        expect_that!(space_ctx, some(anything()));
        if let Some(ctx) = space_ctx {
            expect_that!(ctx.tab_name.as_deref(), some(eq("desktop-tab-ws")));
            expect_that!(
                Hc::is_caller_for_workspace(
                    Some(&caller_t6),
                    ctx.tab_name.as_deref(),
                    "desktop-tab-ws"
                ),
                is_true()
            );
        }
        Ok(())
    }

    #[googletest::test]
    fn run_with_in_place_creates_and_attaches_when_headless() {
        let hc = mock_hc("t1");
        let mut ran_in_place = false;
        let res = hc.run_with_in_place::<_, HcError>(
            "ws-test",
            "fence -- echo 1",
            "agy",
            Some(Path::new("/tmp/test")),
            || {
                ran_in_place = true;
                Ok(())
            },
        );
        expect_that!(res, ok(anything()));
        expect_that!(ran_in_place, is_false());
    }

    #[googletest::test]
    fn run_with_in_place_opens_tab_in_herdr_space_doc_when_inside_herdr_space() {
        let hc = mock_hc("t2");
        let mut ran_in_place = false;
        let res = hc.run_with_in_place::<_, HcError>(
            "ws-test",
            "fence -- echo 1",
            "agy",
            Some(Path::new("/tmp/test")),
            || {
                ran_in_place = true;
                Ok(())
            },
        );
        expect_that!(res, ok(anything()));
        expect_that!(ran_in_place, is_false());
    }

    #[googletest::test]
    fn run_with_in_place_places_existing_terminal_in_herdr_space_doc() {
        let hc = mock_hc("t2");
        let mut ran_in_place = false;
        let res = hc.run_with_in_place::<_, HcError>(
            "existing-ws",
            "fence -- echo 1",
            "agy",
            None,
            || {
                ran_in_place = true;
                Ok(())
            },
        );
        expect_that!(res, ok(anything()));
        expect_that!(ran_in_place, is_false());
    }

    #[googletest::test]
    fn run_with_in_place_runs_closure_when_in_matching_herdr_space_tab() {
        let hc = mock_hc("t6");
        let mut ran_in_place = false;
        let res = hc.run_with_in_place::<_, HcError>("desktop-tab-ws", "true", "agy", None, || {
            ran_in_place = true;
            Ok(())
        });
        expect_that!(res, ok(anything()));
        expect_that!(ran_in_place, is_true());
    }

    #[googletest::test]
    fn run_with_in_place_attaches_to_existing_live_terminal_when_headless() {
        let hc = mock_hc("t1");
        let mut ran_in_place = false;
        let res = hc.run_with_in_place::<_, HcError>(
            "existing-ws",
            "fence -- echo 1",
            "agy",
            None,
            || {
                ran_in_place = true;
                Ok(())
            },
        );
        expect_that!(res, ok(anything()));
        expect_that!(ran_in_place, is_false());
    }

    #[googletest::test]
    fn run_with_in_place_runs_closure_when_already_in_workspace_terminal() {
        let hc = mock_hc("t5");
        let mut ran_in_place = false;
        let res = hc.run_with_in_place::<_, HcError>("ws-current", "true", "agy", None, || {
            ran_in_place = true;
            Ok(())
        });
        expect_that!(res, ok(anything()));
        expect_that!(ran_in_place, is_true());
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
