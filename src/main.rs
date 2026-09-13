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
    /// Launch agy in an isolated Bubblewrap sandbox for a Jujutsu workspace
    Agy(AgyCommand),
}

impl Commands {
    fn run(&self) -> Result<ExitCode, AppError> {
        match self {
            Commands::Agy(cmd) => cmd.run(),
        }
    }
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct AgyCommand {
    /// Workspace name under .workspaces/<workspace-name>
    workspace_name: String,

    /// Print generated Bubblewrap command without executing
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
        let config = aiw::config::AiwConfig::find_and_load(&repo_root)?;
        let effective_tools = config.effective_tools();

        let sandbox_config = aiw::sandbox::SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &effective_tools,
            extra_args: &self.extra_args,
            home_dir: None,
            network: config.network,
        };

        let builder = aiw::sandbox::SandboxBuilder::new(sandbox_config);

        if self.dry_run {
            let args = builder.build_args()?;
            println!("bwrap {}", args.join(" "));
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
#[expect(
    clippy::panic_in_result_fn,
    reason = "Test functions use assertions alongside ? error propagation"
)]
mod tests {
    use super::*;

    #[test]
    fn parse_agy_subcommand_minimal_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let Cli {
            command: Commands::Agy(cmd),
        } = Cli::try_parse_from(["aiw", "agy", "my-workspace"])?;

        assert_eq!(cmd.workspace_name(), "my-workspace");
        assert!(!cmd.dry_run());
        assert!(cmd.extra_args().is_empty());
        Ok(())
    }

    #[test]
    fn parse_agy_subcommand_with_dry_run_flag_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let Cli {
            command: Commands::Agy(cmd),
        } = Cli::try_parse_from(["aiw", "agy", "my-workspace", "--dry-run"])?;

        assert_eq!(cmd.workspace_name(), "my-workspace");
        assert!(cmd.dry_run());
        assert!(cmd.extra_args().is_empty());
        Ok(())
    }

    #[test]
    fn parse_agy_subcommand_with_dry_run_and_extra_args_succeeds()
    -> Result<(), Box<dyn std::error::Error>> {
        let Cli {
            command: Commands::Agy(cmd),
        } = Cli::try_parse_from([
            "aiw",
            "agy",
            "feature-1",
            "--dry-run",
            "--",
            "--model",
            "gemini-2.5",
        ])?;

        assert_eq!(cmd.workspace_name(), "feature-1");
        assert!(cmd.dry_run());
        assert_eq!(cmd.extra_args(), &["--model", "gemini-2.5"]);
        Ok(())
    }

    #[test]
    fn parse_missing_subcommand_fails_with_missing_subcommand_kind()
    -> Result<(), Box<dyn std::error::Error>> {
        let Err(err) = Cli::try_parse_from(["aiw"]) else {
            return Err("expected parse failure for missing subcommand".into());
        };
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
        Ok(())
    }

    #[test]
    fn parse_missing_workspace_name_fails_with_missing_required_argument_kind()
    -> Result<(), Box<dyn std::error::Error>> {
        let Err(err) = Cli::try_parse_from(["aiw", "agy"]) else {
            return Err("expected parse failure for missing workspace name".into());
        };
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        Ok(())
    }
}
