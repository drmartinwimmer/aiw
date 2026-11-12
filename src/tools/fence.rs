use clapback::{Command, CommandError, CommandExt as _};
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

/// Host terminal multiplexer and pane-control environment variables scrubbed before
/// spawning `fence`, preventing sandboxed processes from discovering outer multiplexer
/// sockets or session identifiers.
pub(crate) const SANITIZED_HOST_ENV_VARS: &[&str] = &[
    "HC_BIN_PATH",
    "HC_SCOPE",
    "HC_SESSION",
    "HC_TERMINAL_ID",
    "HC_TERMINAL_SOCKET",
    "HERDR_BIN_PATH",
    "HERDR_ENV",
    "HERDR_PANE_ID",
    "HERDR_SOCKET_PATH",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
    "KITTY_LISTEN_ON",
    "TMUX",
    "TMUX_PANE",
    "WEZTERM_UNIX_SOCKET",
    "ZELLIJ",
    "ZELLIJ_SESSION_NAME",
];

/// Declarative command definition for the `fence` sandbox tool.
#[derive(Command, Debug, Clone, PartialEq, Eq)]
#[command(program = "fence", ensure_user_path)]
pub struct FenceCommand<C: Command> {
    #[command(current_dir)]
    #[env("TMPDIR")]
    pub workspace_path: PathBuf,

    #[arg(long)]
    pub settings: Option<PathBuf>,

    #[command(wrap, last = true)]
    pub command: C,
}

impl<C: Command> FenceCommand<C> {
    /// Builds a [`std::process::Command`] with host multiplexer environment variables scrubbed.
    #[must_use]
    pub fn to_sanitized_std_command(&self) -> ProcessCommand {
        let mut cmd = self.to_std_command();
        for var in SANITIZED_HOST_ENV_VARS {
            cmd.env_remove(var);
        }
        cmd
    }

    /// Executes `fence` synchronously with inherited standard I/O and a sanitized environment.
    pub fn run_sanitized(&self) -> Result<(), CommandError> {
        let mut cmd = self.to_sanitized_std_command();
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let program_name = self.program().to_string_lossy().into_owned();
        let status = cmd
            .status()
            .map_err(|err| CommandError::from_spawn_io(err, &program_name))?;
        if !status.success() {
            return Err(CommandError::ExecutionFailed {
                program: program_name,
                status,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        Ok(())
    }
}
