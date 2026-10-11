use super::common::{init_test_git_repo, init_test_jj_repo, run_aiw, write_test_fence_json};
use googletest::prelude::*;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[googletest::test]
fn workspace_creation_and_dry_run_in_real_git_repo_succeeds() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    let output = run_aiw(repo_root, &["agy", "test-git-workspace", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("test-git-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".git").exists(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), starts_with("fence "));
    expect_that!(stdout.as_ref(), contains_substring("--settings"));
    expect_that!(stdout.as_ref(), contains_substring("agy"));
    expect_that!(
        stdout.as_ref(),
        contains_substring("--dangerously-skip-permissions")
    );

    let wt_list = Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(wt_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&wt_list.stdout);
    expect_that!(
        list_stdout.as_ref(),
        contains_substring("test-git-workspace")
    );
    Ok(())
}

#[googletest::test]
fn idempotent_workspace_reuse_in_real_git_repo_succeeds() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    let first_output = run_aiw(repo_root, &["agy", "reused-git-workspace", "--dry-run"]);
    expect_that!(first_output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("reused-git-workspace");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".git").exists(), is_true());

    let second_output = run_aiw(repo_root, &["agy", "reused-git-workspace", "--dry-run"]);
    expect_that!(second_output.status.success(), is_true());

    let wt_list = Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(wt_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&wt_list.stdout);
    expect_that!(
        list_stdout.as_ref(),
        contains_substring("reused-git-workspace")
    );
    Ok(())
}

#[googletest::test]
fn forget_subcommand_removes_workspace_from_git_worktree_list() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    let create_output = run_aiw(repo_root, &["agy", "ws-to-forget-git", "--dry-run"]);
    expect_that!(create_output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("ws-to-forget-git");
    expect_that!(ws_path.exists(), is_true());

    let list_before = Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(
        String::from_utf8_lossy(&list_before.stdout).as_ref(),
        contains_substring("ws-to-forget-git")
    );

    let forget_output = run_aiw(repo_root, &["forget", "ws-to-forget-git"]);
    expect_that!(forget_output.status.success(), is_true());

    let list_after = Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(
        String::from_utf8_lossy(&list_after.stdout).as_ref(),
        not(contains_substring("ws-to-forget-git"))
    );
    expect_that!(ws_path.exists(), is_false());
    Ok(())
}

#[googletest::test]
fn git_repo_in_subdirectory_discovers_root_and_creates_workspace() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    let sub = repo_root.join("deep").join("nested");
    std::fs::create_dir_all(&sub)?;

    let output = run_aiw(&sub, &["agy", "from-sub-git", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("from-sub-git");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".git").exists(), is_true());
    Ok(())
}

#[googletest::test]
fn git_repo_nested_inside_jj_repo_identifies_git_root_and_creates_git_workspace() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let outer_jj = temp_dir.path();
    init_test_jj_repo(outer_jj);

    let nested_git = outer_jj.join("sub_git_project");
    std::fs::create_dir_all(&nested_git)?;
    init_test_git_repo(&nested_git);
    write_test_fence_json(&nested_git);

    // 1. Run aiw directly inside the nested git repo root
    let out = run_aiw(&nested_git, &["agy", "nested-git-ws", "--dry-run"]);
    expect_that!(out.status.success(), is_true());

    let nested_ws = nested_git.join(".workspaces").join("nested-git-ws");
    expect_that!(nested_ws.exists(), is_true());
    // Must be a Git worktree, containing .git file/pointer
    expect_that!(nested_ws.join(".git").exists(), is_true());
    // Outer JJ repo must NOT have created .workspaces
    expect_that!(outer_jj.join(".workspaces").exists(), is_false());

    // Verify git worktree list inside nested git includes the workspace
    let wt_list = Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(&nested_git)
        .output()?;
    expect_that!(wt_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&wt_list.stdout);
    expect_that!(list_stdout.as_ref(), contains_substring("nested-git-ws"));

    // 2. Also run aiw from a deeper subdirectory within the nested git repo
    let deep_nested = nested_git.join("src").join("components");
    std::fs::create_dir_all(&deep_nested)?;
    let deep_out = run_aiw(&deep_nested, &["agy", "from-deep-sub", "--dry-run"]);
    expect_that!(deep_out.status.success(), is_true());

    let deep_ws = nested_git.join(".workspaces").join("from-deep-sub");
    expect_that!(deep_ws.exists(), is_true());
    expect_that!(deep_ws.join(".git").exists(), is_true());
    expect_that!(outer_jj.join(".workspaces").exists(), is_false());
    Ok(())
}

#[googletest::test]
fn list_subcommand_in_git_repo_lists_all_workspaces() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    // Initial list is empty
    let out_empty = run_aiw(repo_root, &["list"]);
    expect_that!(out_empty.status.success(), is_true());
    expect_that!(String::from_utf8_lossy(&out_empty.stdout).as_ref(), eq(""));

    // Create two workspaces
    let out1 = run_aiw(repo_root, &["agy", "git-one", "--dry-run"]);
    expect_that!(out1.status.success(), is_true());
    let out2 = run_aiw(repo_root, &["agy", "git-two", "--dry-run"]);
    expect_that!(out2.status.success(), is_true());

    // list outputs sorted workspaces
    let out_list = run_aiw(repo_root, &["list"]);
    expect_that!(out_list.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_list.stdout).as_ref(),
        eq("git-one\ngit-two\n")
    );

    // ls alias works
    let out_ls = run_aiw(repo_root, &["ls"]);
    expect_that!(out_ls.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_ls.stdout).as_ref(),
        eq("git-one\ngit-two\n")
    );

    // Running list from inside a worktree also lists all workspaces
    let ws_path = repo_root.join(".workspaces").join("git-one");
    let out_from_ws = run_aiw(&ws_path, &["list"]);
    expect_that!(out_from_ws.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_from_ws.stdout).as_ref(),
        eq("git-one\ngit-two\n")
    );

    // Forget first workspace
    let out_forget = run_aiw(repo_root, &["forget", "git-one"]);
    expect_that!(out_forget.status.success(), is_true());

    // Only git-two remains
    let out_after_forget = run_aiw(repo_root, &["list"]);
    expect_that!(out_after_forget.status.success(), is_true());
    expect_that!(
        String::from_utf8_lossy(&out_after_forget.stdout).as_ref(),
        eq("git-two\n")
    );
    Ok(())
}
