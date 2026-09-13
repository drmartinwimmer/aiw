use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "aiw", about = "AI Workspace Launcher")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Launch agy in an isolated Bubblewrap sandbox for a Jujutsu workspace
    Agy {
        /// Workspace name under .workspaces/<workspace-name>
        workspace_name: String,

        /// Print generated Bubblewrap command without executing
        #[arg(long)]
        dry_run: bool,

        /// Extra arguments forwarded directly to agy
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra_args: Vec<String>,
    },
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
    match cli.command {
        Commands::Agy {
            workspace_name,
            dry_run,
            extra_args,
        } => {
            let current_dir = std::env::current_dir()?;
            let repo_root = aiw::workspace::find_jj_root(&current_dir)?;
            let workspace_path = aiw::workspace::ensure_workspace(&repo_root, &workspace_name)?;
            let config = aiw::config::AiwConfig::find_and_load(&repo_root)?;

            let sandbox_config = aiw::sandbox::SandboxConfig {
                repo_root: &repo_root,
                workspace_path: &workspace_path,
                tools: &config.tools,
                extra_args: &extra_args,
                home_dir: None,
            };

            let builder = aiw::sandbox::SandboxBuilder::new(sandbox_config);

            if dry_run {
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

    #[test]
    fn test_parse_agy_subcommand_minimal() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace"]);
        match cli {
            Ok(Cli {
                command:
                    Commands::Agy {
                        workspace_name,
                        dry_run,
                        extra_args,
                    },
            }) => {
                assert_eq!(workspace_name, "my-workspace");
                assert!(!dry_run);
                assert!(extra_args.is_empty());
            }
            Err(err) => {
                assert_eq!(err.to_string(), "");
            }
        }
    }

    #[test]
    fn test_parse_agy_subcommand_with_dry_run_flag() {
        let cli = Cli::try_parse_from(["aiw", "agy", "my-workspace", "--dry-run"]);
        match cli {
            Ok(Cli {
                command:
                    Commands::Agy {
                        workspace_name,
                        dry_run,
                        extra_args,
                    },
            }) => {
                assert_eq!(workspace_name, "my-workspace");
                assert!(dry_run);
                assert!(extra_args.is_empty());
            }
            Err(err) => {
                assert_eq!(err.to_string(), "");
            }
        }
    }

    #[test]
    fn test_parse_agy_subcommand_with_dry_run_and_extra_args() {
        let cli = Cli::try_parse_from([
            "aiw",
            "agy",
            "feature-1",
            "--dry-run",
            "--",
            "--model",
            "gemini-2.5",
        ]);
        match cli {
            Ok(Cli {
                command:
                    Commands::Agy {
                        workspace_name,
                        dry_run,
                        extra_args,
                    },
            }) => {
                assert_eq!(workspace_name, "feature-1");
                assert!(dry_run);
                assert_eq!(extra_args, vec!["--model", "gemini-2.5"]);
            }
            Err(err) => {
                assert_eq!(err.to_string(), "");
            }
        }
    }

    #[test]
    fn test_parse_missing_subcommand_fails() {
        let cli = Cli::try_parse_from(["aiw"]);
        match cli {
            Err(_) => {}
            Ok(_) => assert_eq!("expected parse failure", "parsed successfully"),
        }
    }

    #[test]
    fn test_parse_missing_workspace_name_fails() {
        let cli = Cli::try_parse_from(["aiw", "agy"]);
        match cli {
            Err(_) => {}
            Ok(_) => assert_eq!("expected parse failure", "parsed successfully"),
        }
    }
}
