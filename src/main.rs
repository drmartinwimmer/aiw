use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "aiw", about = "AI Workspace Launcher")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum Commands {
    /// Launch agy in an isolated Fence sandbox for a Jujutsu workspace
    Agy(AgyCommand),
    /// Forget a Jujutsu workspace
    Forget(ForgetCommand),
}

impl Commands {
    fn run(&self) -> Result<ExitCode, AppError> {
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
    fn run(&self) -> Result<ExitCode, AppError> {
        let current_dir = std::env::current_dir()?;
        let repo_root = aiw::workspace::find_jj_root(&current_dir)?;
        aiw::workspace::forget_workspace(&repo_root, &self.workspace_name)?;
        Ok(ExitCode::SUCCESS)
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
    fn run(&self) -> Result<ExitCode, AppError> {
        let current_dir = std::env::current_dir()?;
        let repo_root = aiw::workspace::find_jj_root(&current_dir)?;
        let workspace_path = aiw::workspace::ensure_workspace(&repo_root, &self.workspace_name)?;

        if repo_root.join("aiw.json").exists() {
            let _ = aiw::config::AiwConfig::find_and_load(&repo_root)?;
        }

        let use_direnv = which::which("direnv").is_ok();
        if use_direnv {
            aiw::sandbox::allow_direnv_if_repo_root_allowed(&repo_root, &workspace_path);
        }

        let sandbox_config = aiw::sandbox::SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            command: None,
            extra_args: &self.extra_args,
            settings_path: None,
            use_direnv,
        };

        let builder = aiw::sandbox::SandboxBuilder::new(sandbox_config);

        if self.dry_run {
            let args = builder.build_args()?;
            println!("fence {}", args.join(" "));
            Ok(ExitCode::SUCCESS)
        } else {
            let status = builder.run()?;
            let code = match status.code() {
                Some(c) => u8::try_from(c).unwrap_or(1),
                None => 1,
            };
            Ok(ExitCode::from(code))
        }
    }
}

#[cfg(test)]
impl AgyCommand {
    fn workspace_name(&self) -> &str {
        &self.workspace_name
    }

    fn dry_run(&self) -> bool {
        self.dry_run
    }

    fn extra_args(&self) -> &[String] {
        &self.extra_args
    }
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("{0}")]
    Workspace(#[from] aiw::WorkspaceError),
    #[error("{0}")]
    Config(#[from] aiw::ConfigError),
    #[error("{0}")]
    Sandbox(#[from] aiw::SandboxError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

fn run() -> Result<ExitCode, AppError> {
    let cli = Cli::parse();
    cli.command.run()
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn parse_agy_subcommand_minimal_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace"]).expect("parse minimal agy");
        if let Commands::Agy(cmd) = cli.command {
            expect_that!(cmd.workspace_name(), eq("my-workspace"));
            expect_that!(cmd.dry_run(), is_false());
            expect_that!(cmd.extra_args(), is_empty());
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_agy_subcommand_with_dry_run_flag_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace", "--dry-run"])
            .expect("parse agy with dry run");
        if let Commands::Agy(cmd) = cli.command {
            expect_that!(cmd.workspace_name(), eq("my-workspace"));
            expect_that!(cmd.dry_run(), is_true());
            expect_that!(cmd.extra_args(), is_empty());
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
            expect_that!(cmd.workspace_name(), eq("feature-1"));
            expect_that!(cmd.dry_run(), is_true());
            expect_that!(cmd.extra_args(), elements_are![eq("--model"), eq("gemini-2.5")]);
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
