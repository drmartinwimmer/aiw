use std::path::Path;
use std::process::{Command, Output};

fn init_test_jj_repo(path: &Path) {
    let output = Command::new("jj")
        .args(["--no-pager", "git", "init"])
        .arg(path)
        .output()
        .expect("execute jj git init");

    if !output.status.success() {
        let fallback = Command::new("jj")
            .args(["--no-pager", "init", "--git"])
            .arg(path)
            .output()
            .expect("execute fallback jj init --git");
        assert!(
            fallback.status.success(),
            "Failed to initialize Jujutsu repository: {}",
            String::from_utf8_lossy(&fallback.stderr)
        );
    }
}

fn run_aiw(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("execute aiw binary")
}

#[test]
fn workspace_creation_and_dry_run_in_real_jj_repo_succeeds() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo", "rustc"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
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
}

#[test]
fn idempotent_workspace_reuse_in_real_jj_repo_succeeds() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    // Initial run creates workspace
    let first_output = run_aiw(repo_root, &["agy", "reused-workspace", "--dry-run"]);
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
    let second_output = run_aiw(repo_root, &["agy", "reused-workspace", "--dry-run"]);
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
        .output()
        .expect("jj workspace list");
    assert!(ws_list.status.success());
    let list_stdout = String::from_utf8_lossy(&ws_list.stdout);
    assert!(
        list_stdout.contains("reused-workspace"),
        "jj workspace list must contain reused-workspace"
    );
}

#[test]
fn missing_jj_repository_fails_with_clear_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let non_repo = temp_dir.path();

    let output = run_aiw(non_repo, &["agy", "test-workspace", "--dry-run"]);
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
}

#[test]
fn missing_tool_in_config_fails_with_diagnostic_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["non_existent_tool_xyz123"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
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
}

#[test]
fn invalid_config_json_fails_with_parse_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    std::fs::write(repo_root.join("aiw.json"), "not valid json {").expect("write bad json");

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
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
}

#[test]
fn live_bwrap_execution_in_temp_workspace_runs_and_verifies_containment() {
    if which::which("bwrap").is_err() || which::which("agy").is_err() {
        eprintln!("Skipping live_bwrap_execution test: bwrap or agy not found in PATH");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    // Invoke live container execution forwarding `--version` to agy inside the sandbox
    let output = run_aiw(repo_root, &["agy", "live-workspace", "--", "--version"]);

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
}

#[test]
fn config_with_default_tools_and_network_flag_in_real_repo() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"default_tools": true, "network": true}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "default-tools-ws", "--dry-run"]);
    assert!(
        output.status.success(),
        "aiw agy failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--share-net"),
        "Dry-run output must contain --share-net when network is true"
    );
}

#[test]
fn config_with_network_false_omits_share_net_in_dry_run() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo"], "network": false}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "no-net-ws", "--dry-run"]);
    assert!(
        output.status.success(),
        "aiw agy failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("--share-net"),
        "Dry-run output must omit --share-net when network is false"
    );
}
