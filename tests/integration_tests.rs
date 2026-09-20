use googletest::prelude::*;
use std::path::Path;
use std::process::{Command, Output};

fn can_run_fence() -> bool {
    if which::which("fence").is_err() {
        return false;
    }
    let output = Command::new("fence").args(["--", "true"]).output();
    matches!(output, Ok(out) if out.status.success())
}

fn init_test_git_repo(path: &Path) {
    let output = Command::new("git")
        .args(["init", "-b", "main"])
        .arg(path)
        .output();

    let success = matches!(output, Ok(ref out) if out.status.success());
    if !success {
        let fallback = Command::new("git")
            .arg("init")
            .arg(path)
            .output()
            .expect("git init");
        assert!(fallback.status.success(), "Failed to initialize Git repository");
    }

    drop(
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output(),
    );
    drop(
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output(),
    );

    let dummy_file = path.join(".gitignore");
    std::fs::write(&dummy_file, ".workspaces/\n").expect("write .gitignore");
    drop(
        Command::new("git")
            .args(["add", ".gitignore"])
            .current_dir(path)
            .output(),
    );
    drop(
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(path)
            .output(),
    );
}

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
    let tpl_dir = repo_root.join("templates");
    std::fs::create_dir_all(&tpl_dir).expect("create templates dir");
    let tpl_content = r#"{
  "extends": "code",
  "network": {
    "allowLocalOutbound": false,
    "allowedDomains": [
      "cloudcode-pa.googleapis.com",
      "daily-cloudcode-pa.googleapis.com",
      "aicode.googleapis.com",
      "aiplatform.googleapis.com",
      "oauth2.googleapis.com",
      "accounts.google.com"
    ]
  },
  "filesystem": {
    "allowWrite": [
      ".",
      ".jj/**",
      "../../.jj/**",
      ".workspaces/**",
      "~/.gemini/**",
      "~/.local/share/**",
      "~/.local/share/keyrings/**"
    ]
  },
  "command": {
    "acceptSharedBinaryCannotRuntimeDeny": ["chroot"]
  }
}"#;
    std::fs::write(tpl_dir.join("aiw.json"), tpl_content).expect("write templates/aiw.json");

    let fence_content = r#"{
  "extends": "./templates/aiw.json",
  "filesystem": {
    "allowRead": ["/nix"],
    "allowWrite": [
      ".git/**",
      "../../.git/**"
    ]
  }
}"#;
    std::fs::write(repo_root.join("fence.json"), fence_content).expect("write fence.json");
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

    let fence_json_path = repo_root.join("fence.json");
    let fence_json_str = fence_json_path.to_string_lossy();
    expect_that!(stdout.as_ref(), contains_substring(fence_json_str.as_ref()));
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
        contains_substring("Error: Not inside a Jujutsu or Git repository")
    );
}

#[googletest::test]
fn live_fence_execution_in_temp_workspace_runs_and_verifies_containment() {
    if which::which("fence").is_err() || which::which("agy").is_err() || !can_run_fence() {
        eprintln!("Skipping live_fence_execution test: fence cannot execute in this environment");
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
    if which::which("fence").is_err() || which::which("jj").is_err() || !can_run_fence() {
        eprintln!("Skipping jj_commands_in_fence_sandbox test: fence cannot execute in this environment");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    let ws = aiw::workspace::Workspace::new(repo_root, "jj-fence-test")
        .expect("Workspace::new");
    ws.ensure().expect("ensure workspace");
    let ws_path = ws.path();

    let status_cmd = vec!["jj".into(), "--no-pager".into(), "status".into()];
    let builder = aiw::sandbox::SandboxBuilder::new(ws_path, repo_root, &status_cmd);
    let mut status_cmd = builder.build_command().expect("build_command");
    let status_output = status_cmd.output().expect("execute fence jj status");

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
    let builder2 = aiw::sandbox::SandboxBuilder::new(ws_path, repo_root, &new_cmd);
    let mut new_cmd = builder2.build_command().expect("build_command");
    let new_output = new_cmd.output().expect("execute fence jj new");

    expect_that!(new_output.status.success(), is_true());

    // Verify change is recorded and visible to host jj
    let log_output = Command::new("jj")
        .args(["--no-pager", "log", "-r", "@", "--ignore-working-copy"])
        .current_dir(ws_path)
        .output()
        .expect("host jj log");

    expect_that!(log_output.status.success(), is_true());
    let log_stdout = String::from_utf8_lossy(&log_output.stdout);
    expect_that!(
        log_stdout.as_ref(),
        contains_substring("commit from inside fence")
    );
}

#[googletest::test]
fn forget_subcommand_removes_workspace_from_jj_list() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
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
        .output()
        .expect("jj workspace list");
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
        .output()
        .expect("jj workspace list");
    expect_that!(
        String::from_utf8_lossy(&list_after.stdout).as_ref(),
        not(contains_substring("ws-to-forget"))
    );
    expect_that!(repo_root.join(".workspaces").join("ws-to-forget").exists(), is_false());
}

#[googletest::test]
fn recreating_forgotten_workspace_calls_direnv_allow() {
    if which::which("direnv").is_err() {
        eprintln!("Skipping recreating_forgotten_workspace_calls_direnv_allow: direnv not found");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let repo_root = dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    let root_envrc = repo_root.join(".envrc");
    std::fs::write(&root_envrc, "export ROOT_VAR=ok\n").expect("write root .envrc");
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_root)
        .status()
        .expect("jj commit");
    assert!(status.success());

    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_root)
        .output()
        .expect("allow root");
    assert!(allow_res.status.success());

    // 1. Initial creation
    let out1 = run_aiw(repo_root, &["agy", "recreated-ws", "--dry-run"]);
    assert!(out1.status.success());
    let ws_path = repo_root.join(".workspaces").join("recreated-ws");
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_path), is_true());

    // 2. Forget workspace
    let forget_out = run_aiw(repo_root, &["forget", "recreated-ws"]);
    assert!(forget_out.status.success());
    expect_that!(ws_path.exists(), is_false());

    // 3. Re-create workspace with aiw
    let out2 = run_aiw(repo_root, &["agy", "recreated-ws", "--dry-run"]);
    assert!(out2.status.success());

    // 4. Must have called direnv allow again because it's a re-created new workspace!
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_path), is_true());
}

#[googletest::test]
fn sandbox_command_runs_in_correct_working_directory_and_loads_direnv() {
    if which::which("fence").is_err() || which::which("direnv").is_err() || !can_run_fence() {
        eprintln!("Skipping sandbox_command_runs_in_correct_working_directory_and_loads_direnv: fence cannot execute in this environment");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Write .envrc in repo_root and allow it so repo root is allowed
    let repo_envrc = repo_root.join(".envrc");
    std::fs::write(
        &repo_envrc,
        "export PATH=\"/bin:/usr/bin:$PATH\"\nexport AIW_DIRENV_LOADED=sandbox_direnv_ok\n",
    )
    .expect("write repo .envrc");

    let allow_repo = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_root)
        .output()
        .expect("direnv allow on repo root");
    assert!(allow_repo.status.success(), "allow repo root failed");

    let ws = aiw::workspace::Workspace::new(repo_root, "direnv-ws").expect("Workspace::new");
    ws.ensure().expect("ensure_workspace");
    let ws_path = ws.path().to_path_buf();

    // Write a .envrc inside the workspace setting an environment variable
    let envrc_path = ws_path.join(".envrc");
    std::fs::write(
        &envrc_path,
        "export PATH=\"/bin:/usr/bin:$PATH\"\nexport AIW_DIRENV_LOADED=sandbox_direnv_ok\n",
    )
    .expect("write .envrc");

    // Allow direnv in workspace for direct SandboxBuilder execution
    aiw::direnv::Direnv::allow_dir(&ws_path);

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
    let builder = aiw::sandbox::SandboxBuilder::new(&ws_path, repo_root, &test_cmd)
        .with_direnv(true);
    let mut cmd = builder.build_command().expect("build_command");
    let output = cmd.output().expect("execute sandbox command");

    assert!(
        output.status.success(),
        "sandbox command execution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let canonical_ws = ws_path.canonicalize().unwrap_or_else(|_| ws_path.clone());
    let canonical_ws_str = canonical_ws.display().to_string();
    let ws_str = ws_path.display().to_string();

    expect_that!(
        stdout.as_ref(),
        predicate(|s: &str| s.contains(&format!("CWD={canonical_ws_str}")) || s.contains(&format!("CWD={ws_str}")))
    );
    expect_that!(
        stdout.as_ref(),
        contains_substring("VAL=sandbox_direnv_ok")
    );
}

#[googletest::test]
fn sandbox_does_not_auto_allow_direnv_when_repo_root_is_not_allowed() {
    if which::which("fence").is_err() || which::which("direnv").is_err() {
        eprintln!("Skipping sandbox_does_not_auto_allow_direnv_when_repo_root_is_not_allowed: fence or direnv not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Write .envrc in repo_root but do NOT allow it
    let repo_envrc = repo_root.join(".envrc");
    std::fs::write(&repo_envrc, "export BLOCKED=1\n").expect("write repo .envrc");

    let ws = aiw::workspace::Workspace::new(repo_root, "unallowed-ws")
        .expect("Workspace::new");
    ws.ensure().expect("ensure workspace");
    let ws_path = ws.path().to_path_buf();
    let ws_envrc = ws_path.join(".envrc");
    std::fs::write(&ws_envrc, "export BLOCKED=1\n").expect("write ws .envrc");

    // repo_root is NOT allowed
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(repo_root), is_false());

    // Call allow_workspace with Direnv
    let direnv = aiw::direnv::Direnv::new(repo_root);
    direnv.allow_workspace(&ws_path);

    // ws_path should NOT have been allowed!
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_path), is_false());
}

#[googletest::test]
fn direnv_allow_called_on_new_workspace_iff_allowed_in_root() {
    if which::which("direnv").is_err() {
        eprintln!("Skipping direnv_allow_called_on_new_workspace_iff_allowed_in_root: direnv not found");
        return;
    }

    // Case 1: Root is allowed -> new workspace gets auto-allowed
    let dir_allowed = tempfile::tempdir().expect("tempdir");
    let repo_allowed = dir_allowed.path();
    init_test_jj_repo(repo_allowed);
    write_test_fence_json(repo_allowed);

    let envrc_allowed = repo_allowed.join(".envrc");
    std::fs::write(&envrc_allowed, "export ROOT_VAR=allowed\n").expect("write root .envrc");
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_allowed)
        .status()
        .expect("jj commit");
    assert!(status.success());

    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_allowed)
        .output()
        .expect("allow root");
    assert!(allow_res.status.success());
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(repo_allowed), is_true());

    // Run aiw to create new workspace
    let out_allowed = run_aiw(repo_allowed, &["agy", "new-ws-allowed", "--dry-run"]);
    assert!(out_allowed.status.success());

    let ws_allowed = repo_allowed.join(".workspaces").join("new-ws-allowed");
    expect_that!(ws_allowed.join(".envrc").exists(), is_true());
    // direnv allow was called on new workspace because root was allowed:
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_allowed), is_true());

    // Case 2: Root is NOT allowed -> new workspace does NOT get auto-allowed
    let dir_unallowed = tempfile::tempdir().expect("tempdir");
    let repo_unallowed = dir_unallowed.path();
    init_test_jj_repo(repo_unallowed);
    write_test_fence_json(repo_unallowed);

    let envrc_unallowed = repo_unallowed.join(".envrc");
    std::fs::write(&envrc_unallowed, "export ROOT_VAR=unallowed\n").expect("write root .envrc");
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_unallowed)
        .status()
        .expect("jj commit");
    assert!(status.success());
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(repo_unallowed), is_false());

    // Run aiw to create new workspace in unallowed repo
    let out_unallowed = run_aiw(repo_unallowed, &["agy", "new-ws-unallowed", "--dry-run"]);
    assert!(out_unallowed.status.success());

    let ws_unallowed = repo_unallowed.join(".workspaces").join("new-ws-unallowed");
    expect_that!(ws_unallowed.join(".envrc").exists(), is_true());
    // direnv allow was NOT called because root was not allowed:
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_unallowed), is_false());
}

#[googletest::test]
fn existing_workspace_does_not_call_direnv_allow() {
    if which::which("direnv").is_err() {
        eprintln!("Skipping existing_workspace_does_not_call_direnv_allow: direnv not found");
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let repo_root = dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    let root_envrc = repo_root.join(".envrc");
    std::fs::write(&root_envrc, "export ROOT_VAR=ok\n").expect("write root .envrc");
    let status = Command::new("jj")
        .args(["--no-pager", "commit", "-m", "initial"])
        .current_dir(repo_root)
        .status()
        .expect("jj commit");
    assert!(status.success());

    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_root)
        .output()
        .expect("allow root");
    assert!(allow_res.status.success());
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(repo_root), is_true());

    // Create the workspace initially
    let out1 = run_aiw(repo_root, &["agy", "existing-ws", "--dry-run"]);
    assert!(out1.status.success());

    let ws_path = repo_root.join(".workspaces").join("existing-ws");
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_path), is_true());

    // Invalidate the workspace allow status by modifying its .envrc
    std::fs::write(ws_path.join(".envrc"), "export ROOT_VAR=ok\nexport MODIFIED=1\n")
        .expect("modify ws .envrc");
    // Verify direnv now considers it unallowed/blocked:
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_path), is_false());

    // Run aiw again on the existing workspace
    let out2 = run_aiw(repo_root, &["agy", "existing-ws", "--dry-run"]);
    assert!(out2.status.success());

    // direnv allow must NOT have been called because this is an existing workspace!
    expect_that!(aiw::direnv::Direnv::is_dir_allowed(&ws_path), is_false());
}

#[googletest::test]
fn direnv_prepended_to_command_iff_allowed() {
    if which::which("direnv").is_err() {
        eprintln!("Skipping direnv_prepended_to_command_iff_allowed: direnv not found");
        return;
    }

    // Case 1: root allowed -> direnv exec . is prepended
    let dir_allowed = tempfile::tempdir().expect("tempdir");
    let repo_allowed = dir_allowed.path();
    init_test_jj_repo(repo_allowed);
    write_test_fence_json(repo_allowed);

    std::fs::write(repo_allowed.join(".envrc"), "export V=1\n").expect("write .envrc");
    let status = Command::new("jj")
        .args(["--no-pager", "status"])
        .current_dir(repo_allowed)
        .status()
        .expect("jj status");
    assert!(status.success());
    let allow_res = Command::new("direnv")
        .args(["allow"])
        .current_dir(repo_allowed)
        .output()
        .expect("allow root");
    assert!(allow_res.status.success());

    let out_allowed = run_aiw(repo_allowed, &["agy", "ws-cmd-allowed", "--dry-run"]);
    assert!(out_allowed.status.success());
    let stdout_allowed = String::from_utf8_lossy(&out_allowed.stdout);
    expect_that!(stdout_allowed.as_ref(), contains_substring("direnv exec ."));

    // Case 2: root NOT allowed -> direnv is omitted from command
    let dir_unallowed = tempfile::tempdir().expect("tempdir");
    let repo_unallowed = dir_unallowed.path();
    init_test_jj_repo(repo_unallowed);
    write_test_fence_json(repo_unallowed);

    std::fs::write(repo_unallowed.join(".envrc"), "export V=1\n").expect("write .envrc");
    let status = Command::new("jj")
        .args(["--no-pager", "status"])
        .current_dir(repo_unallowed)
        .status()
        .expect("jj status");
    assert!(status.success());

    let out_unallowed = run_aiw(repo_unallowed, &["agy", "ws-cmd-unallowed", "--dry-run"]);
    assert!(out_unallowed.status.success());
    let stdout_unallowed = String::from_utf8_lossy(&out_unallowed.stdout);
    expect_that!(stdout_unallowed.as_ref(), not(contains_substring("direnv exec .")));
    expect_that!(stdout_unallowed.as_ref(), contains_substring("agy"));

    // Case 3: root has no .envrc -> direnv is omitted from command
    let dir_no_rc = tempfile::tempdir().expect("tempdir");
    let repo_no_rc = dir_no_rc.path();
    init_test_jj_repo(repo_no_rc);
    write_test_fence_json(repo_no_rc);

    let out_no_rc = run_aiw(repo_no_rc, &["agy", "ws-cmd-no-rc", "--dry-run"]);
    assert!(out_no_rc.status.success());
    let stdout_no_rc = String::from_utf8_lossy(&out_no_rc.stdout);
    expect_that!(stdout_no_rc.as_ref(), not(contains_substring("direnv exec .")));
    expect_that!(stdout_no_rc.as_ref(), contains_substring("agy"));
}

#[googletest::test]
fn aiw_in_herdr_creates_workspace_tab_informs_agent_and_executes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = tempfile::NamedTempFile::new().expect("temp file");
    let log_path = log_file.path().to_string_lossy().to_string();

    let script_path = dir.path().join("mock_herdr.sh");
    let script_content = format!(
        r#"#!/bin/sh
echo "$*" >> "{log_path}"
if [ "$1" = "tab" ] && [ "$2" = "create" ]; then
    echo '{{"id":"cli:tab:create","result":{{"root_pane":{{"pane_id":"w1:p10","tab_id":"w1:t10","workspace_id":"w1"}},"tab":{{"label":"my-herdr-ws","tab_id":"w1:t10","workspace_id":"w1"}},"type":"tab_created"}}}}'
    exit 0
fi
echo '{{"result":{{"type":"ok"}}}}'
exit 0
"#
    );
    std::fs::write(&script_path, script_content).expect("write mock herdr");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)
            .expect("metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).expect("set permissions");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-herdr-ws"])
        .current_dir(repo)
        .env("HERDR_ENV", "1")
        .env("HERDR_BIN_PATH", &script_path)
        .env("HERDR_WORKSPACE_ID", "w1")
        .output()
        .expect("execute aiw");

    assert!(output.status.success());
    let logged = std::fs::read_to_string(&log_path).expect("read log");
    expect_that!(
        &logged,
        contains_substring("tab create --label my-herdr-ws --focus")
    );
    expect_that!(
        &logged,
        contains_substring("pane report-agent --source aiw --agent agy --state working w1:p10")
    );
    expect_that!(
        &logged,
        contains_substring("pane run w1:p10 fence")
    );
    expect_that!(
        &logged,
        contains_substring("agy --dangerously-skip-permissions")
    );
}

#[googletest::test]
fn aiw_in_herdr_with_dry_run_does_not_call_herdr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = tempfile::NamedTempFile::new().expect("temp file");
    let log_path = log_file.path().to_string_lossy().to_string();

    let script_path = dir.path().join("mock_herdr.sh");
    let script_content = format!(
        r#"#!/bin/sh
echo "$*" >> "{log_path}"
exit 0
"#
    );
    std::fs::write(&script_path, script_content).expect("write mock herdr");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)
            .expect("metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).expect("set permissions");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-herdr-ws", "--dry-run"])
        .current_dir(repo)
        .env("HERDR_ENV", "1")
        .env("HERDR_BIN_PATH", &script_path)
        .output()
        .expect("execute aiw");

    assert!(output.status.success());
    let logged = std::fs::read_to_string(&log_path).expect("read log");
    expect_that!(&logged, eq(""));
}

#[googletest::test]
fn workspace_creation_and_dry_run_in_real_git_repo_succeeds() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
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
        .output()
        .expect("git worktree list");
    expect_that!(wt_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&wt_list.stdout);
    expect_that!(list_stdout.as_ref(), contains_substring("test-git-workspace"));
}

#[googletest::test]
fn idempotent_workspace_reuse_in_real_git_repo_succeeds() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
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
        .output()
        .expect("git worktree list");
    expect_that!(wt_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&wt_list.stdout);
    expect_that!(list_stdout.as_ref(), contains_substring("reused-git-workspace"));
}

#[googletest::test]
fn forget_subcommand_removes_workspace_from_git_worktree_list() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
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
        .output()
        .expect("git worktree list");
    expect_that!(
        String::from_utf8_lossy(&list_before.stdout).as_ref(),
        contains_substring("ws-to-forget-git")
    );

    let forget_output = run_aiw(repo_root, &["forget", "ws-to-forget-git"]);
    expect_that!(forget_output.status.success(), is_true());

    let list_after = Command::new("git")
        .args(["--no-pager", "worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .expect("git worktree list");
    expect_that!(
        String::from_utf8_lossy(&list_after.stdout).as_ref(),
        not(contains_substring("ws-to-forget-git"))
    );
    expect_that!(ws_path.exists(), is_false());
}

#[googletest::test]
fn git_repo_in_subdirectory_discovers_root_and_creates_workspace() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    let sub = repo_root.join("deep").join("nested");
    std::fs::create_dir_all(&sub).expect("create deep sub");

    let output = run_aiw(&sub, &["agy", "from-sub-git", "--dry-run"]);
    expect_that!(output.status.success(), is_true());

    let ws_path = repo_root.join(".workspaces").join("from-sub-git");
    expect_that!(ws_path.exists(), is_true());
    expect_that!(ws_path.join(".git").exists(), is_true());
}


#[googletest::test]
fn git_repo_nested_inside_jj_repo_identifies_git_root_and_creates_git_workspace() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let outer_jj = temp_dir.path();
    init_test_jj_repo(outer_jj);

    let nested_git = outer_jj.join("sub_git_project");
    std::fs::create_dir_all(&nested_git).expect("create nested git project directory");
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
        .output()
        .expect("git worktree list in nested git");
    expect_that!(wt_list.status.success(), is_true());
    let list_stdout = String::from_utf8_lossy(&wt_list.stdout);
    expect_that!(list_stdout.as_ref(), contains_substring("nested-git-ws"));

    // 2. Also run aiw from a deeper subdirectory within the nested git repo
    let deep_nested = nested_git.join("src").join("components");
    std::fs::create_dir_all(&deep_nested).expect("create deep nested directory");
    let deep_out = run_aiw(&deep_nested, &["agy", "from-deep-sub", "--dry-run"]);
    expect_that!(deep_out.status.success(), is_true());

    let deep_ws = nested_git.join(".workspaces").join("from-deep-sub");
    expect_that!(deep_ws.exists(), is_true());
    expect_that!(deep_ws.join(".git").exists(), is_true());
    expect_that!(outer_jj.join(".workspaces").exists(), is_false());
}

