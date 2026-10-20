use super::common::{can_run_fence, init_test_jj_repo, run_aiw, write_test_fence_json};
use aiw::{Direnv, SandboxBuilder, Workspace};
use googletest::prelude::*;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[googletest::test]
fn workspace_creation_and_dry_run_in_real_jj_repo_succeeds() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
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

    let fence_json_str = repo_root.join("fence.jsonc").to_string_lossy().to_string();
    expect_that!(stdout.as_ref(), contains_substring(fence_json_str.as_ref()));
    expect_that!(stdout.as_ref(), contains_substring("agy"));
    expect_that!(
        stdout.as_ref(),
        contains_substring("--dangerously-skip-permissions")
    );
    Ok(())
}

#[googletest::test]
fn idempotent_workspace_reuse_in_real_jj_repo_succeeds() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
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
        .output()?;
    expect_that!(ws_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&ws_list.stdout);
    expect_that!(list_stdout.as_ref(), contains_substring("reused-workspace"));
    Ok(())
}

#[googletest::test]
fn missing_jj_repository_fails_with_clear_error() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let non_repo = temp_dir.path();

    let output = run_aiw(non_repo, &["agy", "test-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_false());
    expect_that!(output.status.code(), eq(Some(1)));

    let stderr = String::from_utf8_lossy(&output.stderr);
    expect_that!(
        stderr.as_ref(),
        contains_substring("Error: Not inside a Jujutsu or Git repository")
    );
    Ok(())
}

#[googletest::test]
fn live_fence_execution_in_temp_workspace_runs_and_verifies_containment() -> TestResult {
    if !can_run_fence() || which::which("agy").is_err() {
        eprintln!(
            "Skipping live_fence_execution test: fence sandbox or agy unavailable in this environment"
        );
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Invoke live container execution forwarding `--version` to agy inside fence
    let output = run_aiw(repo_root, &["agy", "live-workspace", "--", "--version"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.trim().is_empty(), is_false());

    let ws_path = repo_root.join(".workspaces").join("live-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".jj").exists(), is_true());
    Ok(())
}

#[googletest::test]
fn jj_commands_in_fence_sandbox_execute_successfully_and_persist_commits() -> TestResult {
    if !can_run_fence() || which::which("jj").is_err() {
        eprintln!(
            "Skipping jj_commands_in_fence_sandbox test: fence sandbox or jj unavailable in this environment"
        );
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    let ws = Workspace::new(repo_root, "jj-fence-test")?;
    ws.ensure()?;
    let ws_path = ws.path();

    let status_cmd = vec!["jj".into(), "--no-pager".into(), "status".into()];
    let builder = SandboxBuilder::new(ws_path, repo_root, &status_cmd);
    let mut status_cmd = builder.build_command()?;
    let status_output = status_cmd.output()?;

    expect_that!(status_output.status.success(), is_true());
    let status_stdout = String::from_utf8_lossy(&status_output.stdout);
    expect_that!(
        status_stdout.as_ref(),
        contains_substring("The working copy has no changes")
    );

    // Test mutating jj command inside fence: jj --no-pager new -m "commit from inside fence"
    let new_cmd = vec![
        "jj".into(),
        "--no-pager".into(),
        "new".into(),
        "-m".into(),
        "commit from inside fence".into(),
    ];
    let builder2 = SandboxBuilder::new(ws_path, repo_root, &new_cmd);
    let mut new_cmd = builder2.build_command()?;
    let new_output = new_cmd.output()?;

    expect_that!(new_output.status.success(), is_true());

    // Verify change is recorded and visible to host jj
    let log_output = Command::new("jj")
        .args(["--no-pager", "log", "-r", "@", "--ignore-working-copy"])
        .current_dir(ws_path)
        .output()?;

    expect_that!(log_output.status.success(), is_true());
    let log_stdout = String::from_utf8_lossy(&log_output.stdout);
    expect_that!(
        log_stdout.as_ref(),
        contains_substring("commit from inside fence")
    );
    Ok(())
}

#[googletest::test]
fn forget_subcommand_removes_workspace_from_jj_list() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Create workspace with aiw agy --dry-run
    let create_output = run_aiw(repo_root, &["agy", "ws-to-forget", "--dry-run"]);
    expect_that!(create_output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("ws-to-forget");
    expect_that!(ws_path.exists(), is_true());

    // Verify jj lists the workspace
    let list_before = Command::new("jj")
        .args(["--no-pager", "workspace", "list"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(
        String::from_utf8_lossy(&list_before.stdout).as_ref(),
        contains_substring("ws-to-forget")
    );

    // Run aiw forget ws-to-forget
    let forget_output = run_aiw(repo_root, &["forget", "ws-to-forget"]);
    expect_that!(forget_output.status.success(), is_true());

    // Verify jj no longer lists the workspace and directory was removed from disk
    let list_after = Command::new("jj")
        .args(["--no-pager", "workspace", "list"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(
        String::from_utf8_lossy(&list_after.stdout).as_ref(),
        not(contains_substring("ws-to-forget"))
    );
    expect_that!(
        repo_root.join(".workspaces").join("ws-to-forget").exists(),
        is_false()
    );
    Ok(())
}

#[googletest::test]
fn recreating_forgotten_workspace_calls_direnv_allow() -> TestResult {
    if which::which("direnv").is_err() {
        eprintln!("Skipping recreating_forgotten_workspace_calls_direnv_allow: direnv not found");
        return Ok(());
    }

    let dir = tempfile::tempdir()?;
    let repo_root = dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    let root_envrc = repo_root.join(".envrc");
    std::fs::write(&root_envrc, "export ROOT_VAR=ok\n")?;
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_root)
        .status()?;
    expect_that!(status.success(), is_true());

    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(allow_res.status.success(), is_true());

    // 1. Initial creation
    let out1 = run_aiw(repo_root, &["agy", "recreated-ws", "--dry-run"]);
    expect_that!(out1.status.success(), is_true());
    let ws_path = repo_root.join(".workspaces").join("recreated-ws");
    expect_that!(Direnv::is_dir_allowed(&ws_path), is_true());

    // 2. Forget workspace
    let forget_out = run_aiw(repo_root, &["forget", "recreated-ws"]);
    expect_that!(forget_out.status.success(), is_true());
    expect_that!(ws_path.exists(), is_false());

    // 3. Re-create workspace with aiw
    let out2 = run_aiw(repo_root, &["agy", "recreated-ws", "--dry-run"]);
    expect_that!(out2.status.success(), is_true());

    // 4. Must have called direnv allow again because it's a re-created new workspace!
    expect_that!(Direnv::is_dir_allowed(&ws_path), is_true());
    Ok(())
}

#[googletest::test]
fn list_subcommand_in_jj_repo_lists_all_workspaces() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Initial list is empty
    let out_empty = run_aiw(repo_root, &["list"]);
    expect_that!(out_empty.status.success(), is_true());
    expect_that!(String::from_utf8_lossy(&out_empty.stdout).as_ref(), eq(""));

    // Create two workspaces
    let out1 = run_aiw(repo_root, &["agy", "ws-first", "--dry-run"]);
    expect_that!(out1.status.success(), is_true());
    let out2 = run_aiw(repo_root, &["agy", "ws-second", "--dry-run"]);
    expect_that!(out2.status.success(), is_true());

    // list outputs sorted workspaces
    let out_list = run_aiw(repo_root, &["list"]);
    expect_that!(out_list.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_list.stdout).as_ref(),
        eq("ws-first\nws-second\n")
    );

    // ls alias outputs identical content
    let out_ls = run_aiw(repo_root, &["ls"]);
    expect_that!(out_ls.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_ls.stdout).as_ref(),
        eq("ws-first\nws-second\n")
    );

    // Running list from inside a workspace also lists all workspaces
    let ws_path = repo_root.join(".workspaces").join("ws-first");
    let out_from_ws = run_aiw(&ws_path, &["list"]);
    expect_that!(out_from_ws.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_from_ws.stdout).as_ref(),
        eq("ws-first\nws-second\n")
    );

    // Forget first workspace
    let out_forget = run_aiw(repo_root, &["forget", "ws-first"]);
    expect_that!(out_forget.status.success(), is_true());

    // Only ws-second remains
    let out_after_forget = run_aiw(repo_root, &["list"]);
    expect_that!(out_after_forget.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_after_forget.stdout).as_ref(),
        eq("ws-second\n")
    );
    Ok(())
}
