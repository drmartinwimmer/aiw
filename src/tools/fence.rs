use clapback::Command;
use std::path::PathBuf;

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
