use super::common::{init_test_jj_repo, write_test_fence_json};
use googletest::prelude::*;
use std::process::Command;
use tempfile::NamedTempFile;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[googletest::test]
fn aiw_in_herdr_creates_workspace_tab_informs_agent_and_executes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = NamedTempFile::new()?;
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
    std::fs::write(&script_path, script_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-herdr-ws"])
        .current_dir(repo)
        .env("HERDR_ENV", "1")
        .env("HERDR_BIN_PATH", &script_path)
        .env("HERDR_WORKSPACE_ID", "w1")
        .output()?;

    expect_that!(output.status.success(), is_true());
    let logged = std::fs::read_to_string(&log_path)?;
    expect_that!(
        &logged,
        contains_substring("tab create --label my-herdr-ws --focus")
    );
    expect_that!(
        &logged,
        contains_substring("pane report-agent --source aiw --agent agy --state working w1:p10")
    );
    expect_that!(&logged, contains_substring("pane run w1:p10 fence"));
    expect_that!(
        &logged,
        contains_substring("agy --dangerously-skip-permissions; exit")
    );
    Ok(())
}

#[googletest::test]
fn aiw_in_herdr_with_dry_run_does_not_call_herdr() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = NamedTempFile::new()?;
    let log_path = log_file.path().to_string_lossy().to_string();

    let script_path = dir.path().join("mock_herdr.sh");
    let script_content = format!(
        r#"#!/bin/sh
echo "$*" >> "{log_path}"
exit 0
"#
    );
    std::fs::write(&script_path, script_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-herdr-ws", "--dry-run"])
        .current_dir(repo)
        .env("HERDR_ENV", "1")
        .env("HERDR_BIN_PATH", &script_path)
        .output()?;

    expect_that!(output.status.success(), is_true());
    let logged = std::fs::read_to_string(&log_path)?;
    expect_that!(&logged, eq(""));
    Ok(())
}
