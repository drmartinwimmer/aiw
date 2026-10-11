use super::common::{
    init_test_git_repo, init_test_jj_repo, run_aiw, run_aiw_with_env, write_test_fence_json,
};
use googletest::prelude::*;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[googletest::test]
fn workspace_without_fence_json_defaults_to_shared_template() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path().join("repo");
    init_test_jj_repo(&repo_root);

    // Setup mock XDG_CONFIG_HOME
    let mock_config = temp_dir.path().join("config");
    let aiw_dir = mock_config.join("aiw");
    std::fs::create_dir_all(&aiw_dir)?;
    std::fs::write(aiw_dir.join("fence.jsonc"), r#"{"extends": "code"}"#)?;

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "auto-ws", "--dry-run"])
        .current_dir(&repo_root)
        .env("XDG_CONFIG_HOME", &mock_config)
        .output()?;

    expect_that!(output.status.success(), is_true());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_tpl = aiw_dir.join("fence.jsonc").to_string_lossy().to_string();
    let expected_arg = format!("--settings {expected_tpl}");
    expect_that!(stdout.as_ref(), contains_substring(expected_arg.as_ref()));
    Ok(())
}

#[googletest::test]
fn aiw_config_init_defaults_to_project_and_user_subcommand_inits_user() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path().join("repo");
    init_test_jj_repo(&repo_root);

    let mock_config = temp_dir.path().join("config");
    let user_config = mock_config.join("aiw").join("fence.jsonc");
    let project_config = repo_root.join("fence.jsonc");

    // 1. `config init` without subcommand defaults to project only, silent on success
    let output1 = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["config", "init"])
        .current_dir(&repo_root)
        .env("XDG_CONFIG_HOME", &mock_config)
        .output()?;

    expect_that!(output1.status.success(), is_true());
    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stderr1 = String::from_utf8_lossy(&output1.stderr);
    expect_that!(stdout1.as_ref(), eq(""));
    expect_that!(stderr1.as_ref(), eq(""));
    expect_that!(user_config.exists(), is_false());
    expect_that!(project_config.exists(), is_true());

    // 2. Second run without --force reports warning on stderr and does not overwrite
    let output2 = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["config", "init"])
        .current_dir(&repo_root)
        .env("XDG_CONFIG_HOME", &mock_config)
        .output()?;

    expect_that!(output2.status.success(), is_true());
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    expect_that!(stdout2.as_ref(), eq(""));
    expect_that!(
        stderr2.as_ref(),
        contains_substring("Project configuration already exists")
    );
    expect_that!(stderr2.as_ref(), contains_substring("No files written"));

    // 3. `config init user` initializes only user configuration, silent on success
    let output_user = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["config", "init", "user"])
        .current_dir(&repo_root)
        .env("XDG_CONFIG_HOME", &mock_config)
        .output()?;

    expect_that!(output_user.status.success(), is_true());
    let stdout_user = String::from_utf8_lossy(&output_user.stdout);
    let stderr_user = String::from_utf8_lossy(&output_user.stderr);
    expect_that!(stdout_user.as_ref(), eq(""));
    expect_that!(stderr_user.as_ref(), eq(""));
    expect_that!(user_config.exists(), is_true());

    // 4. Run `config init user` again without --force reports warning on stderr
    let output_user2 = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["config", "init", "user"])
        .current_dir(&repo_root)
        .env("XDG_CONFIG_HOME", &mock_config)
        .output()?;

    expect_that!(output_user2.status.success(), is_true());
    let stdout_user2 = String::from_utf8_lossy(&output_user2.stdout);
    let stderr_user2 = String::from_utf8_lossy(&output_user2.stderr);
    expect_that!(stdout_user2.as_ref(), eq(""));
    expect_that!(
        stderr_user2.as_ref(),
        contains_substring("User configuration already exists")
    );
    expect_that!(
        stderr_user2.as_ref(),
        contains_substring("No files written")
    );

    // 5. Run with --force overwrites silently on success
    let output3 = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["config", "init", "--force", "project"])
        .current_dir(&repo_root)
        .env("XDG_CONFIG_HOME", &mock_config)
        .output()?;

    expect_that!(output3.status.success(), is_true());
    let stdout3 = String::from_utf8_lossy(&output3.stdout);
    let stderr3 = String::from_utf8_lossy(&output3.stderr);
    expect_that!(stdout3.as_ref(), eq(""));
    expect_that!(stderr3.as_ref(), eq(""));
    expect_that!(project_config.exists(), is_true());
    Ok(())
}

#[googletest::test]
fn list_subcommand_outside_repo_fails() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let non_repo = temp_dir.path();

    let output = run_aiw(non_repo, &["list"]);
    expect_that!(output.status.success(), is_false());
    let stderr = String::from_utf8_lossy(&output.stderr);
    expect_that!(
        stderr.as_ref(),
        contains_substring("Error: Not inside a Jujutsu or Git repository")
    );
    Ok(())
}

#[googletest::test]
fn completion_subcommand_outputs_bash_script() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let output = run_aiw(temp_dir.path(), &["completion", "bash"]);
    expect_that!(output.status.success(), is_true());
    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), contains_substring("_clap_complete_aiw"));
    expect_that!(stdout.as_ref(), contains_substring("complete -o nospace"));
    Ok(())
}

#[googletest::test]
fn completion_subcommand_outputs_zsh_script() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let output = run_aiw(temp_dir.path(), &["completion", "zsh"]);
    expect_that!(output.status.success(), is_true());
    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(
        stdout.as_ref(),
        contains_substring("_clap_dynamic_completer_aiw")
    );
    expect_that!(stdout.as_ref(), contains_substring("compdef"));
    Ok(())
}

#[googletest::test]
fn completion_subcommand_outputs_fish_script() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let output = run_aiw(temp_dir.path(), &["completion", "fish"]);
    expect_that!(output.status.success(), is_true());
    let stdout = String::from_utf8_lossy(&output.stdout);
    expect_that!(stdout.as_ref(), contains_substring("complete"));
    expect_that!(stdout.as_ref(), contains_substring("--command aiw"));
    Ok(())
}

#[googletest::test]
fn dynamic_completion_for_forget_in_jj_repo_completes_workspaces() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_jj_repo(repo_root);
    write_test_fence_json(repo_root);

    // Create workspaces
    let out1 = run_aiw(repo_root, &["agy", "ws-alpha", "--dry-run"]);
    expect_that!(out1.status.success(), is_true());
    let out2 = run_aiw(repo_root, &["agy", "ws-beta", "--dry-run"]);
    expect_that!(out2.status.success(), is_true());

    // Complete aiw forget ""
    let comp_output = run_aiw_with_env(
        repo_root,
        &[("COMPLETE", "bash"), ("_CLAP_COMPLETE_INDEX", "2")],
        &["--", "aiw", "forget", ""],
    );
    expect_that!(comp_output.status.success(), is_true());
    let stdout = String::from_utf8_lossy(&comp_output.stdout);
    expect_that!(stdout.as_ref(), contains_substring("ws-alpha\n"));
    expect_that!(stdout.as_ref(), contains_substring("ws-beta\n"));

    // Prefix filtering: aiw forget "ws-a"
    let prefix_output = run_aiw_with_env(
        repo_root,
        &[("COMPLETE", "bash"), ("_CLAP_COMPLETE_INDEX", "2")],
        &["--", "aiw", "forget", "ws-a"],
    );
    expect_that!(prefix_output.status.success(), is_true());
    let prefix_stdout = String::from_utf8_lossy(&prefix_output.stdout);
    expect_that!(prefix_stdout.as_ref(), contains_substring("ws-alpha"));
    expect_that!(prefix_stdout.as_ref(), not(contains_substring("ws-beta")));
    Ok(())
}

#[googletest::test]
fn dynamic_completion_for_agy_in_git_repo_completes_workspaces() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let repo_root = temp_dir.path();
    init_test_git_repo(repo_root);
    write_test_fence_json(repo_root);

    // Create workspaces
    let out1 = run_aiw(repo_root, &["agy", "git-feat1", "--dry-run"]);
    expect_that!(out1.status.success(), is_true());
    let out2 = run_aiw(repo_root, &["agy", "git-feat2", "--dry-run"]);
    expect_that!(out2.status.success(), is_true());

    // Complete aiw agy ""
    let comp_output = run_aiw_with_env(
        repo_root,
        &[("COMPLETE", "bash"), ("_CLAP_COMPLETE_INDEX", "2")],
        &["--", "aiw", "agy", ""],
    );
    expect_that!(comp_output.status.success(), is_true());
    let stdout = String::from_utf8_lossy(&comp_output.stdout);
    expect_that!(stdout.as_ref(), contains_substring("git-feat1\n"));
    expect_that!(stdout.as_ref(), contains_substring("git-feat2\n"));
    expect_that!(stdout.as_ref(), contains_substring("--dry-run\n"));
    Ok(())
}

#[googletest::test]
fn dynamic_completion_outside_repo_succeeds_gracefully() -> TestResult {
    let temp_dir = tempfile::tempdir()?;
    let non_repo = temp_dir.path();

    let comp_output = run_aiw_with_env(
        non_repo,
        &[("COMPLETE", "bash"), ("_CLAP_COMPLETE_INDEX", "2")],
        &["--", "aiw", "forget", ""],
    );
    expect_that!(comp_output.status.success(), is_true());
    Ok(())
}
