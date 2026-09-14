use googletest::prelude::*;
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

#[googletest::test]
fn workspace_creation_and_dry_run_in_real_jj_repo_succeeds() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo", "rustc"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("test-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".jj").exists(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), starts_with("bwrap "));
    expect_that!(stdout.as_ref(), contains_substring("--bind"));

    let ws_path_str = ws_path.to_string_lossy();
    expect_that!(stdout.as_ref(), contains_substring(ws_path_str.as_ref()));

    let repo_jj_str = repo_root.join(".jj").to_string_lossy().to_string();
    expect_that!(stdout.as_ref(), contains_substring(repo_jj_str.as_str()));

    let repo_git_str = repo_root.join(".git").to_string_lossy().to_string();
    expect_that!(stdout.as_ref(), contains_substring(repo_git_str.as_str()));

    if std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .is_some_and(|h| h.join(".gemini").exists())
    {
        expect_that!(stdout.as_ref(), contains_substring(".gemini"));
    }
    expect_that!(stdout.as_ref(), contains_substring("--chdir"));
    expect_that!(stdout.as_ref(), contains_substring("agy"));
    expect_that!(
        stdout.as_ref(),
        contains_substring("--dangerously-skip-permissions")
    );
}

#[googletest::test]
fn idempotent_workspace_reuse_in_real_jj_repo_succeeds() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    // Initial run creates workspace
    let first_output = run_aiw(repo_root, &["agy", "reused-workspace", "--dry-run"]);
    expect_that!(first_output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("reused-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".jj").exists(), is_true());

    // Second run should idempotently succeed and reuse workspace
    let second_output = run_aiw(repo_root, &["agy", "reused-workspace", "--dry-run"]);
    expect_that!(second_output.status.success(), is_true());

    let stdout = String::from_utf8_lossy(&second_output.stdout);
    expect_that!(
        stdout.as_ref(),
        contains_substring(ws_path.to_string_lossy().as_ref())
    );

    // Verify Jujutsu lists this workspace
    let ws_list = Command::new("jj")
        .args(["--no-pager", "workspace", "list"])
        .current_dir(repo_root)
        .output()
        .expect("jj workspace list");
    expect_that!(ws_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&ws_list.stdout);
    expect_that!(list_stdout.as_ref(), contains_substring("reused-workspace"));
}

#[googletest::test]
fn missing_jj_repository_fails_with_clear_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let non_repo = temp_dir.path();

    let output = run_aiw(non_repo, &["agy", "test-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_false());
    expect_that!(output.status.code(), eq(Some(1)));

    let stderr = String::from_utf8_lossy(&output.stderr);
    expect_that!(
        stderr.as_ref(),
        contains_substring("Error: Not inside a Jujutsu repository")
    );
}

#[googletest::test]
fn missing_tool_in_config_fails_with_diagnostic_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["non_existent_tool_xyz123"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_false());
    expect_that!(output.status.code(), eq(Some(1)));

    let stderr = String::from_utf8_lossy(&output.stderr);
    expect_that!(
        stderr.as_ref(),
        contains_substring(
            "Error: Required host tool 'non_existent_tool_xyz123' could not be found in PATH"
        )
    );
}

#[googletest::test]
fn invalid_config_json_fails_with_parse_error() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    std::fs::write(repo_root.join("aiw.json"), "not valid json {").expect("write bad json");

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_false());
    expect_that!(output.status.code(), eq(Some(1)));

    let stderr = String::from_utf8_lossy(&output.stderr);
    expect_that!(
        stderr.as_ref(),
        contains_substring("Error: Failed to parse JSON in configuration file")
    );
}

#[googletest::test]
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

    expect_that!(output.status.success(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.trim().is_empty(), is_false());

    let ws_path = repo_root.join(".workspaces").join("live-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".jj").exists(), is_true());
}

#[googletest::test]
fn config_with_default_tools_and_network_flag_in_real_repo() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"default_tools": true, "network": true}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "default-tools-ws", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), contains_substring("--share-net"));
}

#[googletest::test]
fn config_with_network_false_omits_share_net_in_dry_run() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["cargo"], "network": false}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let output = run_aiw(repo_root, &["agy", "no-net-ws", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), not(contains_substring("--share-net")));
}

#[googletest::test]
fn jj_commands_in_sandbox_execute_successfully_and_persist_commits() {
    if which::which("bwrap").is_err() || which::which("jj").is_err() {
        eprintln!("Skipping jj_commands_in_sandbox test: bwrap or jj not found in PATH");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);

    let config_content = r#"{"tools": ["jj"]}"#;
    std::fs::write(repo_root.join("aiw.json"), config_content).expect("write aiw.json");

    let ws_path = aiw::workspace::ensure_workspace(repo_root, "jj-sandbox-test")
        .expect("ensure_workspace");

    let tools = vec!["jj".to_string()];
    let config = aiw::sandbox::SandboxConfig {
        repo_root,
        workspace_path: &ws_path,
        tools: &tools,
        extra_args: &[],
        home_dir: None,
        network: false,
    };
    let builder = aiw::sandbox::SandboxBuilder::new(config);
    let mut bwrap_args = builder.build_args().expect("build_args");

    // Verify .git is mounted
    let repo_git_str = repo_root.join(".git").display().to_string();
    expect_that!(bwrap_args, contains(eq(&repo_git_str)));

    // Replace trailing agy invocation with jj --no-pager status
    let dash_pos = bwrap_args.iter().position(|a| a == "--").expect("contains --");
    bwrap_args.truncate(dash_pos);
    bwrap_args.extend(["--".into(), "jj".into(), "--no-pager".into(), "status".into()]);

    let status_output = Command::new("bwrap")
        .args(&bwrap_args)
        .output()
        .expect("execute bwrap jj status");

    expect_that!(status_output.status.success(), is_true());
    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    expect_that!(
        status_stdout.as_ref(),
        contains_substring("The working copy has no changes")
    );

    // Test mutating jj command inside sandbox: jj --no-pager new -m "sandbox-commit"
    bwrap_args.truncate(dash_pos);
    bwrap_args.extend([
        "--".into(),
        "jj".into(),
        "--no-pager".into(),
        "new".into(),
        "-m".into(),
        "commit from inside sandbox".into(),
    ]);

    let new_output = Command::new("bwrap")
        .args(&bwrap_args)
        .output()
        .expect("execute bwrap jj new");

    expect_that!(new_output.status.success(), is_true());

    // Verify change is recorded and visible to host jj
    let log_output = Command::new("jj")
        .args(["--no-pager", "log", "-r", "@", "--ignore-working-copy"])
        .current_dir(&ws_path)
        .output()
        .expect("host jj log");

    expect_that!(log_output.status.success(), is_true());
    let log_stdout = String::from_utf8_lossy(&log_output.stdout);
    expect_that!(
        log_stdout.as_ref(),
        contains_substring("commit from inside sandbox")
    );
}

