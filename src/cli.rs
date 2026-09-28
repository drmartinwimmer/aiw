use clap::{Parser, Subcommand};
use crate::direnv::Direnv;
use crate::herdr::{Herdr, HerdrError};
use crate::sandbox::{SandboxBuilder, SandboxError};
use crate::workspace::{Workspace, WorkspaceError};

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

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum Commands {
    /// Launch agy in an isolated Fence sandbox for a workspace
    Agy(AgyCommand),
    /// Forget a workspace
    Forget(ForgetCommand),
    /// Manage aiw configuration
    Config(ConfigCommand),
}

impl Commands {
    fn run(&self) -> Result<(), AppError> {
        match self {
            Commands::Agy(cmd) => cmd.run(),
            Commands::Forget(cmd) => cmd.run(),
            Commands::Config(cmd) => cmd.run(),
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
        let workspace = Workspace::from_dir(&current_dir, &self.workspace_name)?;
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
        let workspace = Workspace::from_dir(&current_dir, &self.workspace_name)?;
        let repo_root = workspace.repo_root();

        let direnv = Direnv::new(repo_root);
        let is_new_workspace = workspace.ensure()?;

        if is_new_workspace {
            direnv.allow_workspace(workspace.path());
        }

        let payload = self.build_payload_command();
        let builder = SandboxBuilder::for_workspace(&workspace, &payload)
            .with_direnv(direnv.is_allowed());

        if self.dry_run {
            println!("{builder}");
            Ok(())
        } else if let Some(herdr) = Herdr::from_env() {
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


#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ConfigCommand {
    #[command(subcommand)]
    command: ConfigSubcommands,
}

impl ConfigCommand {
    fn run(&self) -> Result<(), AppError> {
        match &self.command {
            ConfigSubcommands::Init(args) => args.run(),
        }
    }
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum ConfigSubcommands {
    /// Initialize aiw.jsonc and fence.jsonc configuration files
    Init(ConfigInitArgs),
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ConfigInitArgs {
    /// Target configuration to initialize: user, project, or all (default: all)
    #[command(subcommand)]
    target: Option<ConfigInitTarget>,

    /// Overwrite existing configuration files
    #[arg(long, short, global = true)]
    force: bool,
}

#[derive(Subcommand, Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigInitTarget {
    /// Initialize both user (~/.config/aiw/aiw.jsonc) and project (fence.jsonc) configurations
    All,
    /// Initialize user configuration (~/.config/aiw/aiw.jsonc)
    User,
    /// Initialize project configuration (fence.jsonc)
    Project,
}

impl ConfigInitArgs {
    fn run(&self) -> Result<(), AppError> {
        let current_dir = std::env::current_dir()?;
        let project_dir = crate::vcs::Vcs::from_path(&current_dir)
            .map(|v| v.repo_root().to_path_buf())
            .unwrap_or(current_dir);

        let target = self.target.unwrap_or(ConfigInitTarget::All);
        let mut actions_taken = false;

        if target == ConfigInitTarget::User || target == ConfigInitTarget::All {
            match crate::config::ConfigInitializer::init_user_config(self.force)? {
                Some(path) => {
                    println!("Initialized user configuration: {}", path.display());
                    actions_taken = true;
                }
                None => {
                    if let Some(path) = crate::config::ConfigInitializer::user_config_path() {
                        println!("User configuration already exists: {}", path.display());
                    }
                }
            }
        }

        if target == ConfigInitTarget::Project || target == ConfigInitTarget::All {
            match crate::config::ConfigInitializer::init_project_config(&project_dir, self.force)? {
                Some(path) => {
                    println!("Initialized project configuration: {}", path.display());
                    actions_taken = true;
                }
                None => {
                    println!(
                        "Project configuration already exists: {}",
                        project_dir.join("fence.jsonc").display()
                    );
                }
            }
        }

        if !actions_taken && !self.force {
            println!("No files written. Use --force to overwrite existing configurations.");
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Workspace(#[from] WorkspaceError),
    #[error("{0}")]
    Sandbox(#[from] SandboxError),
    #[error("{0}")]
    Herdr(#[from] HerdrError),
    #[error("{0}")]
    Config(#[from] crate::config::ConfigInitError),
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
            expect_that!(&cmd.workspace_name, eq("my-workspace"));
            expect_that!(cmd.dry_run, is_false());
            expect_that!(&cmd.extra_args, is_empty());
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_agy_subcommand_with_dry_run_flag_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace", "--dry-run"])
            .expect("parse agy with dry run");
        if let Commands::Agy(cmd) = cli.command {
            expect_that!(&cmd.workspace_name, eq("my-workspace"));
            expect_that!(cmd.dry_run, is_true());
            expect_that!(&cmd.extra_args, is_empty());
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
            expect_that!(&cmd.workspace_name, eq("feature-1"));
            expect_that!(cmd.dry_run, is_true());
            expect_that!(&cmd.extra_args, elements_are![eq("--model"), eq("gemini-2.5")]);
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
            expect_that!(&cmd.workspace_name, eq("old-ws"));
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

    #[googletest::test]
    fn parse_config_init_default_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "config", "init"]).expect("parse config init");
        if let Commands::Config(cmd) = cli.command {
            let ConfigSubcommands::Init(args) = cmd.command;
            expect_that!(args.force, is_false());
            expect_that!(args.target, none());
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_config_init_user_subcommand_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "config", "init", "user"]).expect("parse config init user");
        if let Commands::Config(cmd) = cli.command {
            let ConfigSubcommands::Init(args) = cmd.command;
            expect_that!(args.force, is_false());
            expect_that!(args.target, some(eq(ConfigInitTarget::User)));
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_config_init_project_with_force_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "config", "init", "--force", "project"])
            .expect("parse config init project with force");
        if let Commands::Config(cmd) = cli.command {
            let ConfigSubcommands::Init(args) = cmd.command;
            expect_that!(args.force, is_true());
            expect_that!(args.target, some(eq(ConfigInitTarget::Project)));
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_config_init_all_with_force_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "config", "init", "all", "-f"])
            .expect("parse config init all with force");
        if let Commands::Config(cmd) = cli.command {
            let ConfigSubcommands::Init(args) = cmd.command;
            expect_that!(args.force, is_true());
            expect_that!(args.target, some(eq(ConfigInitTarget::All)));
        } else {
            expect_that!(false, is_true());
        }
    }
}
