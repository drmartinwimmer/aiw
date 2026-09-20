use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "aiw", about = "AI Workspace Launcher")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn run() -> Result<(), AppError> {
        let cli = Self::parse();
        cli.command.run()
    }
}

pub fn run() -> Result<(), AppError> {
    Cli::run()
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum Commands {
    /// Launch agy in an isolated Fence sandbox for a workspace
    Agy(AgyCommand),
    /// Forget a workspace
    Forget(ForgetCommand),
}

impl Commands {
    fn run(&self) -> Result<(), AppError> {
        match self {
            Commands::Agy(cmd) => cmd.run(),
            Commands::Forget(cmd) => cmd.run(),
        }
    }
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ForgetCommand {
    /// Workspace name under .workspaces/<workspace-name>
    workspace_name: String,
}

impl ForgetCommand {
    fn run(&self) -> Result<(), AppError> {
        let current_dir = std::env::current_dir()?;
        let workspace = crate::workspace::Workspace::from_dir(&current_dir, &self.workspace_name)?;
        workspace.forget()?;
        Ok(())
    }
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct AgyCommand {
    /// Workspace name under .workspaces/<workspace-name>
    workspace_name: String,

    /// Print generated Fence command without executing
    #[arg(long)]
    dry_run: bool,

    /// Extra arguments forwarded directly to agy
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    extra_args: Vec<String>,
}

impl AgyCommand {
    fn build_payload_command(&self) -> Vec<String> {
        let mut cmd = vec!["agy".to_string()];
        if !self.extra_args.iter().any(|a| a == "--dangerously-skip-permissions") {
            cmd.push("--dangerously-skip-permissions".to_string());
        }
        cmd.extend(self.extra_args.clone());
        cmd
    }

    fn run(&self) -> Result<(), AppError> {
        let current_dir = std::env::current_dir()?;
        let workspace = crate::workspace::Workspace::from_dir(&current_dir, &self.workspace_name)?;
        let repo_root = workspace.repo_root();

        if repo_root.join("aiw.json").exists() {
            let _ = crate::config::AiwConfig::find_and_load(repo_root)?;
        }

        let direnv = crate::direnv::Direnv::new(repo_root);
        let is_new_workspace = workspace.ensure()?;

        if is_new_workspace {
            direnv.allow_workspace(workspace.path());
        }

        let payload = self.build_payload_command();
        let builder = crate::sandbox::SandboxBuilder::for_workspace(&workspace, &payload)
            .with_direnv(direnv.is_allowed());

        if self.dry_run {
            println!("{builder}");
            Ok(())
        } else if let Some(herdr) = crate::herdr::Herdr::from_env() {
            let cmd = format!("{builder}");
            herdr.run_command(
                &self.workspace_name,
                &cmd,
                "agy",
                Some(workspace.path()),
            )?;
            Ok(())
        } else {
            builder.run()?;
            Ok(())
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Workspace(#[from] crate::workspace::WorkspaceError),
    #[error("{0}")]
    Config(#[from] crate::config::ConfigError),
    #[error("{0}")]
    Sandbox(#[from] crate::sandbox::SandboxError),
    #[error("{0}")]
    Herdr(#[from] crate::herdr::HerdrError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn parse_agy_subcommand_minimal_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace"]).expect("parse minimal agy");
        if let Commands::Agy(cmd) = cli.command {
            expect_that!(cmd.workspace_name.as_str(), eq("my-workspace"));
            expect_that!(cmd.dry_run, is_false());
            expect_that!(cmd.extra_args.as_slice(), is_empty());
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_agy_subcommand_with_dry_run_flag_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace", "--dry-run"])
            .expect("parse agy with dry run");
        if let Commands::Agy(cmd) = cli.command {
            expect_that!(cmd.workspace_name.as_str(), eq("my-workspace"));
            expect_that!(cmd.dry_run, is_true());
            expect_that!(cmd.extra_args.as_slice(), is_empty());
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_agy_subcommand_with_dry_run_and_extra_args_succeeds() {
        let cli = Cli::try_parse_from([
            "aiw",
            "agy",
            "feature-1",
            "--dry-run",
            "--",
            "--model",
            "gemini-2.5",
        ])
        .expect("parse agy with extra args");
        if let Commands::Agy(cmd) = cli.command {
            expect_that!(cmd.workspace_name.as_str(), eq("feature-1"));
            expect_that!(cmd.dry_run, is_true());
            expect_that!(cmd.extra_args.as_slice(), elements_are![eq("--model"), eq("gemini-2.5")]);
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_missing_subcommand_fails_with_missing_subcommand_kind() {
        let res = Cli::try_parse_from(["aiw"]);
        expect_that!(
            res,
            err(predicate(|err: &clap::Error| {
                err.kind() == clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            }))
        );
    }

    #[googletest::test]
    fn parse_missing_workspace_name_fails_with_missing_required_argument_kind() {
        let res = Cli::try_parse_from(["aiw", "agy"]);
        expect_that!(
            res,
            err(predicate(|err: &clap::Error| {
                err.kind() == clap::error::ErrorKind::MissingRequiredArgument
            }))
        );
    }

    #[googletest::test]
    fn parse_forget_subcommand_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "forget", "old-ws"]).expect("parse forget");
        if let Commands::Forget(cmd) = cli.command {
            expect_that!(cmd.workspace_name.as_str(), eq("old-ws"));
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_forget_missing_workspace_name_fails() {
        let res = Cli::try_parse_from(["aiw", "forget"]);
        expect_that!(
            res,
            err(predicate(|err: &clap::Error| {
                err.kind() == clap::error::ErrorKind::MissingRequiredArgument
            }))
        );
    }
}
