use crate::tools::ensure_user_profile_bin_paths;
use std::path::Path;
use std::process::Command;

/// Command builder for the `fence` sandbox tool.
pub struct FenceCommand;

impl FenceCommand {
    /// Creates a base `Command` configured for running `fence`.
    #[must_use]
    pub fn create(workspace_path: &Path) -> Command {
        let mut cmd = Command::new("fence");
        cmd.current_dir(workspace_path);
        ensure_user_profile_bin_paths(&mut cmd);
        cmd.env("TMPDIR", workspace_path);
        cmd
    }
}
