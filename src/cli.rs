use std::path::Path;

use crate::config::{ConfigInitError, ConfigInitializer};
use crate::direnv::Direnv;
use crate::herdr::{Herdr, HerdrError};
use crate::sandbox::{SandboxBuilder, SandboxError};
use crate::vcs::Vcs;
use crate::workspace::{Workspace, WorkspaceError};
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

#[derive(Subcommand, Debug, PartialEq, Eq)]
enum Commands {
    /// Launch agy in an isolated Fence sandbox for a workspace
    Agy(AgyCommand),
    /// Forget a workspace
    Forget(ForgetCommand),
    /// Manage aiw configuration
    Config(ConfigCommand),
    /// List all available workspaces
    #[command(alias = "ls")]
    List(ListCommand),
    /// Generate shell completion script
    Completion(CompletionCommand),
}

impl Commands {
    fn run(&self) -> Result<(), AppError> {
        match self {
            Commands::Agy(cmd) => cmd.run(),
            Commands::Forget(cmd) => cmd.run(),
            Commands::Config(cmd) => cmd.run(),
            Commands::List(cmd) => cmd.run(),
            Commands::Completion(cmd) => cmd.run(),
        }
    }
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ListCommand {}

impl ListCommand {
    fn run(&self) -> Result<(), AppError> {
        let current_dir = std::env::current_dir()?;
        let workspaces = Workspace::list_from_dir(&current_dir)?;
        for ws in workspaces {
            println!("{}", ws.name());
        }
        Ok(())
    }
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct CompletionCommand {
    /// Shell to generate completions for
    shell: ShellChoice,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellChoice {
    /// Bourne Again SHell (bash)
    Bash,
    /// Elvish shell
    Elvish,
    /// Friendly Interactive SHell (fish)
    Fish,
    /// PowerShell
    Powershell,
    /// Z SHell (zsh)
    Zsh,
}

impl CompletionCommand {
    fn run(&self) -> Result<(), AppError> {
        use clap_complete::env::{Bash, Elvish, EnvCompleter, Fish, Powershell, Zsh};
        let mut stdout = std::io::stdout();
        match self.shell {
            ShellChoice::Bash => {
                Bash.write_registration("COMPLETE", "aiw", "aiw", "aiw", &mut stdout)?
            }
            ShellChoice::Elvish => {
                Elvish.write_registration("COMPLETE", "aiw", "aiw", "aiw", &mut stdout)?
            }
            ShellChoice::Fish => {
                Fish.write_registration("COMPLETE", "aiw", "aiw", "aiw", &mut stdout)?
            }
            ShellChoice::Powershell => {
                Powershell.write_registration("COMPLETE", "aiw", "aiw", "aiw", &mut stdout)?
            }
            ShellChoice::Zsh => {
                Zsh.write_registration("COMPLETE", "aiw", "aiw", "aiw", &mut stdout)?
            }
        }
        Ok(())
    }
}

fn complete_workspaces(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    let current_str = current.to_str().unwrap_or("");
    let Ok(current_dir) = std::env::current_dir() else {
        return Vec::new();
    };
    let Ok(workspaces) = Workspace::list_from_dir(&current_dir) else {
        return Vec::new();
    };
    workspaces
        .into_iter()
        .filter(|ws| ws.name().starts_with(current_str))
        .map(|ws| clap_complete::engine::CompletionCandidate::new(ws.name()))
        .collect()
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ForgetCommand {
    /// Workspace name under .workspaces/<workspace-name>
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_workspaces))]
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
    #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_workspaces))]
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
        if !self
            .extra_args
            .iter()
            .any(|a| a == "--dangerously-skip-permissions")
        {
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
        let builder =
            SandboxBuilder::for_workspace(&workspace, &payload).with_direnv(direnv.is_allowed());

        if self.dry_run {
            println!("{builder}");
            Ok(())
        } else if let Some(herdr) = Herdr::from_env() {
            let cmd = format!("{builder}");
            herdr.run_command(&self.workspace_name, &cmd, "agy", Some(workspace.path()))?;
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
    /// Initialize fence.jsonc configuration files
    Init(ConfigInitArgs),
}

#[derive(clap::Args, Debug, PartialEq, Eq)]
struct ConfigInitArgs {
    /// Target configuration to initialize: project (default) or user
    #[command(subcommand)]
    target: Option<ConfigInitSubcommand>,

    /// Overwrite existing configuration files
    #[arg(long, short, global = true)]
    force: bool,
}

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
enum ConfigInitSubcommand {
    /// Initialize project configuration (fence.jsonc) [default]
    Project(ConfigInitProjectSubcommand),
    /// Initialize user configuration (~/.config/aiw/fence.jsonc)
    User(ConfigInitUserSubcommand),
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
struct ConfigInitProjectSubcommand {
    /// Overwrite existing configuration files
    #[arg(long, short)]
    force: bool,
}

fn warn_already_exists(kind: &str, path: &Path) {
    eprintln!(
        "Warning: {kind} configuration already exists: {}. No files written. Use --force to overwrite existing configurations.",
        path.display()
    );
}

impl ConfigInitProjectSubcommand {
    fn run(&self, global_force: bool) -> Result<(), AppError> {
        let force = self.force || global_force;
        let current_dir = std::env::current_dir()?;
        let project_dir = Vcs::from_path(&current_dir)
            .map(|v| v.repo_root().to_path_buf())
            .unwrap_or(current_dir);

        match ConfigInitializer::init_project_config(&project_dir, force) {
            Ok(_) => Ok(()),
            Err(ConfigInitError::AlreadyExists(path)) => {
                warn_already_exists("Project", &path);
                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }
}

#[derive(clap::Args, Debug, Clone, PartialEq, Eq)]
struct ConfigInitUserSubcommand {
    /// Overwrite existing configuration files
    #[arg(long, short)]
    force: bool,
}

impl ConfigInitUserSubcommand {
    fn run(&self, global_force: bool) -> Result<(), AppError> {
        let force = self.force || global_force;
        match ConfigInitializer::init_user_config(force) {
            Ok(_) => Ok(()),
            Err(ConfigInitError::AlreadyExists(path)) => {
                warn_already_exists("User", &path);
                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }
}

impl ConfigInitArgs {
    fn run(&self) -> Result<(), AppError> {
        match &self.target {
            Some(ConfigInitSubcommand::User(cmd)) => cmd.run(self.force),
            Some(ConfigInitSubcommand::Project(cmd)) => cmd.run(self.force),
            None => ConfigInitProjectSubcommand { force: self.force }.run(self.force),
        }
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
    Config(#[from] ConfigInitError),
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
            expect_that!(
                &cmd.extra_args,
                elements_are![eq("--model"), eq("gemini-2.5")]
            );
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
        let cli =
            Cli::try_parse_from(["aiw", "config", "init", "user"]).expect("parse config init user");
        if let Commands::Config(cmd) = cli.command {
            let ConfigSubcommands::Init(args) = cmd.command;
            expect_that!(args.force, is_false());
            expect_that!(
                args.target,
                some(eq(&ConfigInitSubcommand::User(ConfigInitUserSubcommand {
                    force: false
                })))
            );
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
            expect_that!(
                args.target,
                some(eq(&ConfigInitSubcommand::Project(
                    ConfigInitProjectSubcommand { force: true }
                )))
            );
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_config_init_project_subcommand_with_own_force_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "config", "init", "project", "-f"])
            .expect("parse config init project with own force");
        if let Commands::Config(cmd) = cli.command {
            let ConfigSubcommands::Init(args) = cmd.command;
            expect_that!(
                args.target,
                some(eq(&ConfigInitSubcommand::Project(
                    ConfigInitProjectSubcommand { force: true }
                )))
            );
        } else {
            expect_that!(false, is_true());
        }
    }

    #[googletest::test]
    fn parse_list_subcommand_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "list"]).expect("parse list");
        expect_that!(&cli.command, eq(&Commands::List(ListCommand {})));
    }

    #[googletest::test]
    fn parse_list_subcommand_alias_ls_succeeds() {
        let cli = Cli::try_parse_from(["aiw", "ls"]).expect("parse ls");
        expect_that!(&cli.command, eq(&Commands::List(ListCommand {})));
    }

    #[googletest::test]
    fn parse_completion_subcommand_succeeds_for_all_shells() {
        let shells = [
            ("bash", ShellChoice::Bash),
            ("elvish", ShellChoice::Elvish),
            ("fish", ShellChoice::Fish),
            ("powershell", ShellChoice::Powershell),
            ("zsh", ShellChoice::Zsh),
        ];
        for (name, expected) in shells {
            let cli = Cli::try_parse_from(["aiw", "completion", name]).expect("parse completion");
            expect_that!(
                &cli.command,
                eq(&Commands::Completion(CompletionCommand { shell: expected }))
            );
        }
    }

    #[googletest::test]
    fn parse_completion_without_shell_fails() {
        let cli = Cli::try_parse_from(["aiw", "completion"]);
        expect_that!(cli.is_err(), is_true());
    }

    #[googletest::test]
    fn parse_completion_invalid_shell_fails() {
        let cli = Cli::try_parse_from(["aiw", "completion", "invalid_shell"]);
        expect_that!(cli.is_err(), is_true());
    }
}
