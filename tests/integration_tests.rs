#![expect(
    clippy::panic_in_result_fn,
    reason = "Integration tests use assertions alongside ? error propagation"
)]

use std::path::Path;
use std::process::{Command, Output};

fn init_test_jj_repo(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("jj")
        .args(["--no-pager", "git", "init"])
        .arg(path)
        .output()?;

    if !output.status.success() {
        let fallback = Command::new("jj")
            .args(["--no-pager", "init", "--git"])
            .arg(path)
            .output()?;
        if !fallback.status.success() {
            return Err(format!(
                "Failed to initialize Jujutsu repository: {}",
                String::from_utf8_lossy(&fallback.stderr)
            )
            .into());
        }
    }
    Ok(())
}

fn run_aiw(cwd: &Path, args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(args)
        .current_dir(cwd)
        .output()?;
    Ok(output)
}

#[test]
fn workspace_creation_and_dry_run_in_real_jj_repo_succeeds()
-> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root)?;

    let config_content = r#"{"tools": ["cargo", "rustc"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content)?;

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"])?;
    assert!(
        output.status.success(),
        "aiw agy --dry-run failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let ws_path = repo_root.join(".workspaces").join("test-workspace");
    assert!(ws_path.exists(), "Workspace directory must be created");
    assert!(
        ws_path.join(".jj").exists(),
        "Workspace must contain a .jj reference"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("bwrap "),
        "Dry-run output must start with 'bwrap '"
    );
    assert!(
        stdout.contains("--bind"),
        "Dry-run output must contain --bind"
    );

    let ws_path_str = ws_path.to_string_lossy();
    assert!(
        stdout.contains(ws_path_str.as_ref()),
        "Dry-run output must bind the workspace path: {}",
        ws_path_str
    );

    let repo_jj_str = repo_root.join(".jj").to_string_lossy().to_string();
    assert!(
        stdout.contains(&repo_jj_str),
        "Dry-run output must bind repo .jj path: {}",
        repo_jj_str
    );

    assert!(
        stdout.contains(".gemini"),
        "Dry-run output must bind ~/.gemini"
    );
    assert!(
        stdout.contains("--chdir"),
        "Dry-run output must set working directory"
    );
    assert!(
        stdout.contains("agy"),
        "Dry-run output must invoke target agy"
    );

    Ok(())
}

#[test]
fn idempotent_workspace_reuse_in_real_jj_repo_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root)?;

    let config_content = r#"{"tools": ["cargo"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content)?;

    // Initial run creates workspace
    let first_output = run_aiw(repo_root, &["agy", "reused-workspace", "--dry-run"])?;
    assert!(
        first_output.status.success(),
        "First run failed: {}",
        String::from_utf8_lossy(&first_output.stderr)
    );

    let ws_path = repo_root.join(".workspaces").join("reused-workspace");
    assert!(ws_path.exists(), "Workspace directory must exist");
    assert!(
        ws_path.join(".jj").exists(),
        "Workspace must have .jj reference"
    );

    // Second run should idempotently succeed and reuse workspace
    let second_output = run_aiw(repo_root, &["agy", "reused-workspace", "--dry-run"])?;
    assert!(
        second_output.status.success(),
        "Second run failed: {}",
        String::from_utf8_lossy(&second_output.stderr)
    );

    let stdout = String::from_utf8_lossy(&second_output.stdout);
    assert!(
        stdout.contains(ws_path.to_string_lossy().as_ref()),
        "Second run must bind the existing workspace path"
    );

    // Verify Jujutsu lists this workspace
    let ws_list = Command::new("jj")
        .args(["--no-pager", "workspace", "list"])
        .current_dir(repo_root)
        .output()?;
    assert!(ws_list.status.success());
    let list_stdout = String::from_utf8_lossy(&ws_list.stdout);
    assert!(
        list_stdout.contains("reused-workspace"),
        "jj workspace list must contain reused-workspace"
    );

    Ok(())
}

#[test]
fn missing_jj_repository_fails_with_clear_error() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let non_repo = temp_dir.path();

    let output = run_aiw(non_repo, &["agy", "test-workspace", "--dry-run"])?;
    assert!(
        !output.status.success(),
        "aiw must fail outside a Jujutsu repository"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "aiw must exit with status 1 on repository discovery failure"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error: Not inside a Jujutsu repository"),
        "stderr must contain clear error message, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn missing_tool_in_config_fails_with_diagnostic_error() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root)?;

    let config_content = r#"{"tools": ["non_existent_tool_xyz123"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content)?;

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"])?;
    assert!(
        !output.status.success(),
        "aiw must fail when a configured tool is missing"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "aiw must exit with status 1 on missing tool"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "Error: Required host tool 'non_existent_tool_xyz123' could not be found in PATH"
        ),
        "stderr must indicate the specific missing tool, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn invalid_config_json_fails_with_parse_error() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root)?;

    std::fs::write(repo_root.join("aiw.json"), "not valid json {")?;

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"])?;
    assert!(
        !output.status.success(),
        "aiw must fail when aiw.json has invalid JSON"
    );
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error: Failed to parse JSON in configuration file"),
        "stderr must contain JSON parsing diagnostic, got: {}",
        stderr
    );

    Ok(())
}

#[test]
fn live_bwrap_execution_in_temp_workspace_runs_and_verifies_containment()
-> Result<(), Box<dyn std::error::Error>> {
    if which::which("bwrap").is_err() || which::which("agy").is_err() {
        eprintln!("Skipping live_bwrap_execution test: bwrap or agy not found in PATH");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root)?;

    let config_content = r#"{"tools": ["cargo"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content)?;

    // Invoke live container execution forwarding `--version` to agy inside the sandbox
    let output = run_aiw(repo_root, &["agy", "live-workspace", "--", "--version"])?;

    assert!(
        output.status.success(),
        "Live container execution failed with status {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.trim().is_empty(),
        "Live container execution should output agy version"
    );

    let ws_path = repo_root.join(".workspaces").join("live-workspace");
    assert!(
        ws_path.exists(),
        "Workspace directory must exist after live execution"
    );
    assert!(
        ws_path.join(".jj").exists(),
        "Workspace must contain valid .jj repository link"
    );

    Ok(())
}
