use super::common::{init_test_jj_repo, write_test_fence_json};
use googletest::prelude::*;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[googletest::test]
fn aiw_in_hc_creates_workspace_terminal_and_attaches() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-hc-ws"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("HC_TERMINAL_ID", "t1")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", log_file.path())
        .output()?;

    expect_that!(output.status.success(), is_true());
    let logged = std::fs::read_to_string(log_file.path())?;
    expect_that!(
        &logged,
        contains_substring("run --json --bg --no-parent --rm --name my-hc-ws --cwd ")
    );
    expect_that!(
        &logged,
        contains_substring("--label source=aiw --label agent=agy --label workspace=my-hc-ws fence")
    );
    expect_that!(
        &logged,
        contains_substring("agy --dangerously-skip-permissions")
    );
    expect_that!(&logged, contains_substring("attach t10"));
    Ok(())
}

#[googletest::test]
fn aiw_in_hc_reattaches_when_live_workspace_terminal_already_exists() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "existing-ws"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("HC_TERMINAL_ID", "t1")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", log_file.path())
        .output()?;

    expect_that!(output.status.success(), is_true());
    let logged = std::fs::read_to_string(log_file.path())?;
    expect_that!(&logged, not(contains_substring("run --json")));
    expect_that!(&logged, contains_substring("attach t9"));
    Ok(())
}

#[googletest::test]
fn aiw_in_hc_with_dry_run_does_not_call_hc() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-hc-ws", "--dry-run"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("HC_TERMINAL_ID", "t1")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", log_file.path())
        .output()?;

    expect_that!(output.status.success(), is_true());
    let logged = std::fs::read_to_string(log_file.path())?;
    expect_that!(&logged, eq(""));
    Ok(())
}

#[googletest::test]
fn aiw_in_live_hc_session_spawns_workspace_and_runs_in_place() -> TestResult {
    let Ok(hc_bin) = which::which("hc") else {
        eprintln!("Skipping live hc session test: hc binary unavailable");
        return Ok(());
    };

    let scratch = tempfile::tempdir()?;
    let run_dir = scratch.path().join("run");
    let cfg_dir = scratch.path().join("config");
    let data_dir = scratch.path().join("data");
    let state_dir = scratch.path().join("state");
    let bin_dir = scratch.path().join("bin");
    for d in [&run_dir, &cfg_dir, &data_dir, &state_dir, &bin_dir] {
        std::fs::create_dir_all(d)?;
    }

    let mock_fence = bin_dir.join("fence");
    std::fs::write(
        &mock_fence,
        "#!/bin/sh\necho \"pwd=$(pwd) args=$*\" > .live_hc_marker\n",
    )?;
    std::fs::set_permissions(&mock_fence, std::fs::Permissions::from_mode(0o755))?;

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{orig_path}", bin_dir.display());

    let repo_dir = tempfile::tempdir()?;
    let repo = repo_dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let aiw_exe = env!("CARGO_BIN_EXE_aiw");

    // 1. Launching from a non-workspace hc terminal ("launcher") spawns "live-hc-ws" and attaches.
    let spawn_cmd = format!("{aiw_exe} agy live-hc-ws -- --version");
    let spawn_res = Command::new(&hc_bin)
        .args(["run", "--rm", "--name", "launcher", &spawn_cmd])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env_remove("HC_TERMINAL_ID")
        .env_remove("HC_TERMINAL_SOCKET")
        .env_remove("HC_SCOPE")
        .env("PATH", &test_path)
        .env("HC_BIN_PATH", &hc_bin)
        .env("HC_SESSION", "aiw-it")
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("XDG_CONFIG_HOME", &cfg_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_STATE_HOME", &state_dir)
        .output();

    // 2. Launching from inside a terminal already named "live-hc-ws" labels it and runs in place.
    let ws_dir = repo.join(".workspaces").join("live-hc-ws");
    let inplace_marker = ws_dir.join(".live_hc_inplace");
    std::fs::write(
        &mock_fence,
        format!(
            "#!/bin/sh\necho \"inplace args=$*\" > {}\n",
            inplace_marker.display()
        ),
    )?;
    let inplace_res = Command::new(&hc_bin)
        .args(["run", "--name", "live-hc-ws", &spawn_cmd])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env_remove("HC_TERMINAL_ID")
        .env_remove("HC_TERMINAL_SOCKET")
        .env_remove("HC_SCOPE")
        .env("PATH", &test_path)
        .env("HC_BIN_PATH", &hc_bin)
        .env("HC_SESSION", "aiw-it")
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("XDG_CONFIG_HOME", &cfg_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_STATE_HOME", &state_dir)
        .output();

    let inspect_res = Command::new(&hc_bin)
        .args(["get", "--json", "live-hc-ws"])
        .env_remove("HC_TERMINAL_ID")
        .env_remove("HC_TERMINAL_SOCKET")
        .env_remove("HC_SCOPE")
        .env("HC_SESSION", "aiw-it")
        .env("XDG_RUNTIME_DIR", &run_dir)
        .env("XDG_CONFIG_HOME", &cfg_dir)
        .env("XDG_DATA_HOME", &data_dir)
        .env("XDG_STATE_HOME", &state_dir)
        .output();

    drop(
        Command::new(&hc_bin)
            .args(["server", "stop"])
            .env_remove("HC_TERMINAL_ID")
            .env_remove("HC_TERMINAL_SOCKET")
            .env_remove("HC_SCOPE")
            .env("HC_SESSION", "aiw-it")
            .env("XDG_RUNTIME_DIR", &run_dir)
            .env("XDG_CONFIG_HOME", &cfg_dir)
            .env("XDG_DATA_HOME", &data_dir)
            .env("XDG_STATE_HOME", &state_dir)
            .output(),
    );

    let spawn_out = spawn_res?;
    expect_that!(
        spawn_out.status.success(),
        is_true(),
        "spawn stderr: {}",
        String::from_utf8_lossy(&spawn_out.stderr)
    );
    expect_that!(ws_dir.join(".live_hc_marker").exists(), is_true());

    let inplace_out = inplace_res?;
    expect_that!(
        inplace_out.status.success(),
        is_true(),
        "inplace stderr: {}",
        String::from_utf8_lossy(&inplace_out.stderr)
    );
    expect_that!(inplace_marker.exists(), is_true());

    let inspect_out = inspect_res?;
    expect_that!(inspect_out.status.success(), is_true());
    let inspect_json = String::from_utf8_lossy(&inspect_out.stdout);
    expect_that!(&inspect_json, contains_substring("\"agent\":\"agy\""));
    expect_that!(&inspect_json, contains_substring("\"source\":\"aiw\""));
    expect_that!(
        &inspect_json,
        contains_substring("\"workspace\":\"live-hc-ws\"")
    );

    Ok(())
}
