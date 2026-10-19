use super::common::{can_run_fence, init_test_jj_repo, run_aiw, write_test_fence_json};
use aiw::{Direnv, SandboxBuilder, Workspace};
use googletest::prelude::*;
use std::path::Path;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[googletest::test]
fn sandbox_command_runs_in_correct_working_directory_and_loads_direnv() -> TestResult {
    if !can_run_fence() || which::which("direnv").is_err() {
        eprintln!(
            "Skipping sandbox_command_runs_in_correct_working_directory_and_loads_direnv: fence sandbox or direnv unavailable in this environment"
        );
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Write .envrc in repo_root and allow it so repo root is allowed
    let repo_envrc = repo_root.join(".envrc");
    std::fs::write(
        &repo_envrc,
        "export PATH=\"/bin:/usr/bin:$PATH\"\nexport AIW_DIRENV_LOADED=sandbox_direnv_ok\n",
    )?;

    let allow_repo = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_root)
        .output()?;
    expect_that!(allow_repo.status.success(), is_true());

    let ws = Workspace::new(repo_root, "direnv-ws")?;
    ws.ensure()?;
    let ws_path = ws.path().to_path_buf();

    // Write a .envrc inside the workspace setting an environment variable
    let envrc_path = ws_path.join(".envrc");
    std::fs::write(
        &envrc_path,
        "export PATH=\"/bin:/usr/bin:$PATH\"\nexport AIW_DIRENV_LOADED=sandbox_direnv_ok\n",
    )?;

    // Allow direnv in workspace for direct SandboxBuilder execution
    let direnv = Direnv::new(repo_root);
    direnv.allow_workspace(&ws_path);

    let sh_bin = if Path::new("/bin/sh").exists() {
        "/bin/sh"
    } else {
        "sh"
    };
    let test_cmd = vec![
        sh_bin.to_string(),
        "-c".to_string(),
        "echo CWD=$PWD; echo VAL=$AIW_DIRENV_LOADED".to_string(),
    ];
    let builder = SandboxBuilder::new(&ws_path, repo_root, &test_cmd).with_direnv(true);
    let mut cmd = builder.build_command()?;
    let output = cmd.output()?;

    expect_that!(output.status.success(), is_true());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let canonical_ws = ws_path.canonicalize().unwrap_or_else(|_| ws_path.clone());
    let canonical_ws_str = canonical_ws.display().to_string();
    let ws_str = ws_path.display().to_string();

    expect_that!(
        stdout.as_ref(),
        predicate(|s: &str| s.contains(&format!("CWD={canonical_ws_str}"))
            || s.contains(&format!("CWD={ws_str}")))
    );
    expect_that!(stdout.as_ref(), contains_substring("VAL=sandbox_direnv_ok"));
    Ok(())
}

#[googletest::test]
fn sandbox_does_not_auto_allow_direnv_when_repo_root_is_not_allowed() -> TestResult {
    if which::which("fence").is_err() || which::which("direnv").is_err() {
        eprintln!(
            "Skipping sandbox_does_not_auto_allow_direnv_when_repo_root_is_not_allowed: fence or direnv not found"
        );
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Write .envrc in repo_root but do NOT allow it
    let repo_envrc = repo_root.join(".envrc");
    std::fs::write(&repo_envrc, "export BLOCKED=1\n")?;

    let ws = Workspace::new(repo_root, "unallowed-ws")?;
    ws.ensure()?;
    let ws_path = ws.path().to_path_buf();
    let ws_envrc = ws_path.join(".envrc");
    std::fs::write(&ws_envrc, "export BLOCKED=1\n")?;

    // repo_root is NOT allowed
    expect_that!(Direnv::is_dir_allowed(repo_root), is_false());

    // Call allow_workspace with Direnv
    let direnv = Direnv::new(repo_root);
    direnv.allow_workspace(&ws_path);

    // ws_path should NOT have been allowed!
    expect_that!(Direnv::is_dir_allowed(&ws_path), is_false());
    Ok(())
}

#[googletest::test]
fn direnv_allow_called_on_new_workspace_iff_allowed_in_root() -> TestResult {
    if which::which("direnv").is_err() {
        eprintln!(
            "Skipping direnv_allow_called_on_new_workspace_iff_allowed_in_root: direnv not found"
        );
        return Ok(());
    }

    // Case 1: Root is allowed -> new workspace gets auto-allowed
    let dir_allowed = tempfile::tempdir()?;
    let repo_allowed = dir_allowed.path();
    init_test_jj_repo(repo_allowed);
    write_test_fence_json(repo_allowed);

    let envrc_allowed = repo_allowed.join(".envrc");
    std::fs::write(&envrc_allowed, "export ROOT_VAR=allowed\n")?;
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_allowed)
        .status()?;
    expect_that!(status.success(), is_true());

    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_allowed)
        .output()?;
    expect_that!(allow_res.status.success(), is_true());
    expect_that!(Direnv::is_dir_allowed(repo_allowed), is_true());

    // Run aiw to create new workspace
    let out_allowed = run_aiw(repo_allowed, &["agy", "new-ws-allowed", "--dry-run"]);
    expect_that!(out_allowed.status.success(), is_true());

    let ws_allowed = repo_allowed.join(".workspaces").join("new-ws-allowed");
    expect_that!(ws_allowed.join(".envrc").exists(), is_true());
    // direnv allow was called on new workspace because root was allowed:
    expect_that!(Direnv::is_dir_allowed(&ws_allowed), is_true());

    // Case 2: Root is NOT allowed -> new workspace does NOT get auto-allowed
    let dir_unallowed = tempfile::tempdir()?;
    let repo_unallowed = dir_unallowed.path();
    init_test_jj_repo(repo_unallowed);
    write_test_fence_json(repo_unallowed);

    let envrc_unallowed = repo_unallowed.join(".envrc");
    std::fs::write(&envrc_unallowed, "export ROOT_VAR=unallowed\n")?;
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_unallowed)
        .status()?;
    expect_that!(status.success(), is_true());
    expect_that!(Direnv::is_dir_allowed(repo_unallowed), is_false());

    // Run aiw to create new workspace in unallowed repo
    let out_unallowed = run_aiw(repo_unallowed, &["agy", "new-ws-unallowed", "--dry-run"]);
    expect_that!(out_unallowed.status.success(), is_true());

    let ws_unallowed = repo_unallowed.join(".workspaces").join("new-ws-unallowed");
    expect_that!(ws_unallowed.join(".envrc").exists(), is_true());
    // direnv allow was NOT called because root was not allowed:
    expect_that!(Direnv::is_dir_allowed(&ws_unallowed), is_false());
    Ok(())
}

#[googletest::test]
fn existing_workspace_does_not_call_direnv_allow() -> TestResult {
    if which::which("direnv").is_err() {
        eprintln!("Skipping existing_workspace_does_not_call_direnv_allow: direnv not found");
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
    expect_that!(Direnv::is_dir_allowed(repo_root), is_true());

    // Create the workspace initially
    let out1 = run_aiw(repo_root, &["agy", "existing-ws", "--dry-run"]);
    expect_that!(out1.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("existing-ws");
    expect_that!(Direnv::is_dir_allowed(&ws_path), is_true());

    // Invalidate the workspace allow status by modifying its .envrc
    std::fs::write(
        ws_path.join(".envrc"),
        "export ROOT_VAR=ok\nexport MODIFIED=1\n",
    )?;
    // Verify direnv now considers it unallowed/blocked:
    expect_that!(Direnv::is_dir_allowed(&ws_path), is_false());

    // Run aiw again on the existing workspace
    let out2 = run_aiw(repo_root, &["agy", "existing-ws", "--dry-run"]);
    expect_that!(out2.status.success(), is_true());

    // direnv allow must NOT have been called because this is an existing workspace!
    expect_that!(Direnv::is_dir_allowed(&ws_path), is_false());
    Ok(())
}

#[googletest::test]
fn direnv_prepended_to_command_iff_allowed() -> TestResult {
    if which::which("direnv").is_err() {
        eprintln!("Skipping direnv_prepended_to_command_iff_allowed: direnv not found");
        return Ok(());
    }

    // Case 1: root allowed -> direnv exec . is prepended
    let dir_allowed = tempfile::tempdir()?;
    let repo_allowed = dir_allowed.path();
    init_test_jj_repo(repo_allowed);
    write_test_fence_json(repo_allowed);

    std::fs::write(repo_allowed.join(".envrc"), "export V=1\n")?;
    let status = Command::new("jj")
        .args(["--no-pager", "status"])
        .current_dir(repo_allowed)
        .status()?;
    expect_that!(status.success(), is_true());
    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_allowed)
        .output()?;
    expect_that!(allow_res.status.success(), is_true());

    let out_allowed = run_aiw(repo_allowed, &["agy", "ws-cmd-allowed", "--dry-run"]);
    expect_that!(out_allowed.status.success(), is_true());
    let stdout_allowed = String::from_utf8_lossy(&out_allowed.stdout);
    expect_that!(stdout_allowed.as_ref(), contains_substring("direnv exec ."));

    // Case 2: root NOT allowed -> direnv is omitted from command
    let dir_unallowed = tempfile::tempdir()?;
    let repo_unallowed = dir_unallowed.path();
    init_test_jj_repo(repo_unallowed);
    write_test_fence_json(repo_unallowed);

    std::fs::write(repo_unallowed.join(".envrc"), "export V=1\n")?;
    let status = Command::new("jj")
        .args(["--no-pager", "status"])
        .current_dir(repo_unallowed)
        .status()?;
    expect_that!(status.success(), is_true());

    let out_unallowed = run_aiw(repo_unallowed, &["agy", "ws-cmd-unallowed", "--dry-run"]);
    expect_that!(out_unallowed.status.success(), is_true());
    let stdout_unallowed = String::from_utf8_lossy(&out_unallowed.stdout);
    expect_that!(
        stdout_unallowed.as_ref(),
        not(contains_substring("direnv exec ."))
    );
    expect_that!(stdout_unallowed.as_ref(), contains_substring("agy"));

    // Case 3: root has no .envrc -> direnv is omitted from command
    let dir_no_rc = tempfile::tempdir()?;
    let repo_no_rc = dir_no_rc.path();
    init_test_jj_repo(repo_no_rc);
    write_test_fence_json(repo_no_rc);

    let out_no_rc = run_aiw(repo_no_rc, &["agy", "ws-cmd-no-rc", "--dry-run"]);
    expect_that!(out_no_rc.status.success(), is_true());
    let stdout_no_rc = String::from_utf8_lossy(&out_no_rc.stdout);
    expect_that!(
        stdout_no_rc.as_ref(),
        not(contains_substring("direnv exec ."))
    );
    expect_that!(stdout_no_rc.as_ref(), contains_substring("agy"));
    Ok(())
}
