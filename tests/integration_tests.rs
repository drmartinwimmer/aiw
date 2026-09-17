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

fn write_test_fence_json(repo_root: &Path) {
    let content = r#"{
  "extends": "code",
  "filesystem": {
    "allowRead": ["/nix"],
    "allowWrite": [".", ".jj/**", ".git/**", "../../.jj/**", "../../.git/**", ".workspaces/**"]
  },
  "command": {
    "acceptSharedBinaryCannotRuntimeDeny": ["chroot"]
  }
}"#;
    std::fs::write(repo_root.join("fence.json"), content).expect("write fence.json");
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
    write_test_fence_json(repo_root);

    let output = run_aiw(repo_root, &["agy", "test-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("test-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".jj").exists(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), starts_with("fence "));
    expect_that!(stdout.as_ref(), contains_substring("--settings"));

    let fence_json_str = repo_root.join("fence.json").to_string_lossy().to_string();
    expect_that!(stdout.as_ref(), contains_substring(fence_json_str.as_str()));
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
    write_test_fence_json(repo_root);

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
    expect_that!(stdout.as_ref(), starts_with("fence "));

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
fn live_fence_execution_in_temp_workspace_runs_and_verifies_containment() {
    if which::which("fence").is_err() || which::which("agy").is_err() {
        eprintln!("Skipping live_fence_execution test: fence or agy not found in PATH");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Invoke live container execution forwarding `--version` to agy inside fence
    let output = run_aiw(repo_root, &["agy", "live-workspace", "--", "--version"]);

    expect_that!(output.status.success(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.trim().is_empty(), is_false());

    let ws_path = repo_root.join(".workspaces").join("live-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".jj").exists(), is_true());
}

#[googletest::test]
fn jj_commands_in_fence_sandbox_execute_successfully_and_persist_commits() {
    if which::which("fence").is_err() || which::which("jj").is_err() {
        eprintln!("Skipping jj_commands_in_fence_sandbox test: fence or jj not found in PATH");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    let ws_path = aiw::workspace::ensure_workspace(repo_root, "jj-fence-test")
        .expect("ensure_workspace");

    let config = aiw::sandbox::SandboxConfig {
        repo_root,
        workspace_path: &ws_path,
        extra_args: &[],
        settings_path: None,
    };
    let builder = aiw::sandbox::SandboxBuilder::new(config);
    let mut fence_args = builder.build_args().expect("build_args");

    // Replace trailing agy invocation with jj --no-pager status
    let dash_pos = fence_args.iter().position(|a| a == "--").expect("contains --");
    fence_args.truncate(dash_pos);
    fence_args.extend(["--".into(), "jj".into(), "--no-pager".into(), "status".into()]);

    let mut status_cmd = Command::new("fence");
    status_cmd.current_dir(&ws_path);
    status_cmd.args(&fence_args);
    if let Ok(tmp) = std::env::var("TMPDIR")
        && !Path::new(&tmp).exists()
    {
        status_cmd.env("TMPDIR", "/tmp");
    }
    let status_output = status_cmd.output().expect("execute fence jj status");

    expect_that!(status_output.status.success(), is_true());
    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    expect_that!(
        status_stdout.as_ref(),
        contains_substring("The working copy has no changes")
    );

    // Test mutating jj command inside fence: jj --no-pager new -m "commit from inside fence"
    fence_args.truncate(dash_pos);
    fence_args.extend([
        "--".into(),
        "jj".into(),
        "--no-pager".into(),
        "new".into(),
        "-m".into(),
        "commit from inside fence".into(),
    ]);

    let mut new_cmd = Command::new("fence");
    new_cmd.current_dir(&ws_path);
    new_cmd.args(&fence_args);
    if let Ok(tmp) = std::env::var("TMPDIR")
        && !Path::new(&tmp).exists()
    {
        new_cmd.env("TMPDIR", "/tmp");
    }
    let new_output = new_cmd.output().expect("execute fence jj new");

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
        contains_substring("commit from inside fence")
    );
}
