use super::common::{init_test_jj_repo, write_test_fence_json};
use googletest::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, Serialize, Deserialize)]
struct TestRpcRequest<P> {
    jsonrpc: String,
    id: u64,
    method: String,
    params: P,
}

#[derive(Debug, Deserialize)]
struct TestRpcResponse<R> {
    result: R,
}

#[derive(Debug, Serialize)]
struct TestDocGetParams<'a> {
    name: &'a str,
}

#[derive(Debug, Deserialize)]
struct TestDocGetResult {
    value: TestDoc,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestDocSetParams {
    name: String,
    version: u64,
    value: TestDoc,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestDoc {
    spaces: Vec<TestSpace>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestSpace {
    id: String,
    name: String,
    folder: String,
    tabs: Vec<TestTab>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestTab {
    id: String,
    name: String,
    root: TestNode,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestNode {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<TestContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TestNode>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestContent {
    terminal: String,
}

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
fn aiw_in_herdr_space_creates_kept_terminal_and_writes_named_tab_to_doc() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let hc_log = NamedTempFile::new()?;
    let doc_set_out = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "my-hc-ws"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("HC_TERMINAL_ID", "t2")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", hc_log.path())
        .env("HC_MOCK_DOC_SET_OUT", doc_set_out.path())
        .output()?;

    expect_that!(output.status.success(), is_true());
    let hc_logged = std::fs::read_to_string(hc_log.path())?;
    expect_that!(
        &hc_logged,
        contains_substring("run --json --bg --no-parent --rm --keep --name my-hc-ws --cwd ")
    );
    expect_that!(&hc_logged, not(contains_substring("attach ")));

    let doc_set_raw = std::fs::read_to_string(doc_set_out.path())?;
    let rpc: TestRpcRequest<TestDocSetParams> = serde_json::from_str(&doc_set_raw)?;
    let space = rpc.params.value.spaces.first();
    expect_that!(space.map(|s| s.tabs.len()), some(eq(4)));
    let inserted = space.and_then(|s| s.tabs.get(1));
    expect_that!(inserted.map(|t| t.name.as_str()), some(eq("my-hc-ws")));
    let pane = inserted
        .and_then(|t| t.root.children.as_deref())
        .and_then(|children| children.first())
        .and_then(|node| node.content.as_ref());
    expect_that!(pane.map(|c| c.terminal.as_str()), some(eq("t10")));
    Ok(())
}

#[googletest::test]
fn aiw_in_herdr_space_places_unplaced_existing_terminal_and_skips_already_open_tab() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let hc_log = NamedTempFile::new()?;
    let doc_set_out = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    // 1. Unplaced live terminal `t9` ("existing-ws") gets marked keep and added to `"herdr-app"`.
    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "existing-ws"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("HC_TERMINAL_ID", "t2")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", hc_log.path())
        .env("HC_MOCK_DOC_SET_OUT", doc_set_out.path())
        .output()?;

    expect_that!(output.status.success(), is_true());
    let hc_logged = std::fs::read_to_string(hc_log.path())?;
    expect_that!(&hc_logged, contains_substring("keep --json t9"));
    expect_that!(&hc_logged, not(contains_substring("run --json")));
    expect_that!(&hc_logged, not(contains_substring("attach ")));
    let doc_set_raw = std::fs::read_to_string(doc_set_out.path())?;
    expect_that!(&doc_set_raw, contains_substring("\"name\":\"existing-ws\""));
    expect_that!(&doc_set_raw, contains_substring("\"terminal\":\"t9\""));

    // 2. Already-open terminal `t11` ("already-open-ws") is not duplicated in `"herdr-app"`.
    let hc_log_open = NamedTempFile::new()?;
    let rpc_log_open = NamedTempFile::new()?;
    let open_out = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "already-open-ws"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("HC_TERMINAL_ID", "t2")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", hc_log_open.path())
        .env("HC_MOCK_RPC_LOG", rpc_log_open.path())
        .output()?;

    expect_that!(open_out.status.success(), is_true());
    let open_logged = std::fs::read_to_string(hc_log_open.path())?;
    expect_that!(&open_logged, not(contains_substring("keep --json")));
    expect_that!(&open_logged, not(contains_substring("run --json")));
    let open_rpc = std::fs::read_to_string(rpc_log_open.path())?;
    expect_that!(&open_rpc, not(contains_substring("doc.set")));
    Ok(())
}

#[googletest::test]
fn aiw_in_herdr_space_runs_in_place_when_in_matching_tab() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let bin_dir = tempfile::tempdir()?;
    let mock_fence = bin_dir.path().join("fence");
    std::fs::write(
        &mock_fence,
        "#!/bin/sh\necho \"args=$* hc_id=$HC_TERMINAL_ID hc_sock=$HC_TERMINAL_SOCKET tmux=$TMUX\" > .desktop_inplace_marker\n",
    )?;
    std::fs::set_permissions(&mock_fence, std::fs::Permissions::from_mode(0o755))?;
    let orig_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{orig_path}", bin_dir.path().display());

    let hc_log = NamedTempFile::new()?;
    let rpc_log = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["agy", "desktop-tab-ws"])
        .current_dir(repo)
        .env_remove("HERDR_ENV")
        .env("PATH", &test_path)
        .env("HC_TERMINAL_ID", "t6")
        .env("HC_TERMINAL_SOCKET", "/tmp/secret-hc.sock")
        .env("TMUX", "/tmp/tmux-1000/default,123,0")
        .env("HC_BIN_PATH", &mock_hc)
        .env("HC_MOCK_LOG", hc_log.path())
        .env("HC_MOCK_RPC_LOG", rpc_log.path())
        .output()?;

    expect_that!(output.status.success(), is_true());
    let hc_logged = std::fs::read_to_string(hc_log.path())?;
    expect_that!(
        &hc_logged,
        contains_substring("label --json t6 source=aiw agent=agy workspace=desktop-tab-ws")
    );
    expect_that!(&hc_logged, not(contains_substring("run --json")));
    expect_that!(&hc_logged, not(contains_substring("attach ")));

    let rpc_logged = std::fs::read_to_string(rpc_log.path())?;
    expect_that!(&rpc_logged, not(contains_substring("doc.set")));
    let marker_path = repo.join(".workspaces/desktop-tab-ws/.desktop_inplace_marker");
    expect_that!(marker_path.exists(), is_true());
    let marker_content = std::fs::read_to_string(&marker_path)?;
    expect_that!(
        &marker_content,
        contains_substring("hc_id= hc_sock= tmux=\n")
    );
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

#[googletest::test]
fn aiw_in_live_hc_herdr_space_creates_named_tab_in_doc() -> TestResult {
    let Ok(hc_bin) = which::which("hc") else {
        eprintln!("Skipping live hc Herdr space test: hc binary unavailable");
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
        "#!/bin/sh\necho \"pwd=$(pwd) args=$*\" > .remote_space_marker\n",
    )?;
    std::fs::set_permissions(&mock_fence, std::fs::Permissions::from_mode(0o755))?;

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let test_path = format!("{}:{orig_path}", bin_dir.display());

    let repo_dir = tempfile::tempdir()?;
    let repo = repo_dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let hc_cmd = |args: &[&str]| {
        let mut cmd = Command::new(&hc_bin);
        cmd.args(args)
            .current_dir(repo)
            .env_remove("HERDR_ENV")
            .env_remove("HC_TERMINAL_ID")
            .env_remove("HC_TERMINAL_SOCKET")
            .env_remove("HC_SCOPE")
            .env("PATH", &test_path)
            .env("HC_BIN_PATH", &hc_bin)
            .env("HC_SESSION", "aiw-space-it")
            .env("XDG_RUNTIME_DIR", &run_dir)
            .env("XDG_CONFIG_HOME", &cfg_dir)
            .env("XDG_DATA_HOME", &data_dir)
            .env("XDG_STATE_HOME", &state_dir);
        cmd
    };

    // Start the isolated hc daemon and seed `"herdr-app"` with a remote space `sp1` and caller tab `tb1`.
    drop(hc_cmd(&["server", "start"]).output());

    let seed_req = TestRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "doc.set".to_string(),
        params: TestDocSetParams {
            name: "herdr-app".to_string(),
            version: 0,
            value: TestDoc {
                spaces: vec![TestSpace {
                    id: "sp1".to_string(),
                    name: "aiw".to_string(),
                    folder: repo.display().to_string(),
                    tabs: vec![TestTab {
                        id: "tb1".to_string(),
                        name: String::new(),
                        root: TestNode {
                            id: "tb1".to_string(),
                            node_type: "row".to_string(),
                            content: None,
                            children: Some(vec![TestNode {
                                id: "p1".to_string(),
                                node_type: "block".to_string(),
                                content: Some(TestContent {
                                    terminal: "t1".to_string(),
                                }),
                                children: None,
                            }]),
                        },
                    }],
                }],
            },
        },
    };

    let mut seed_child = hc_cmd(&["connect"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = seed_child.stdin.take() {
        writeln!(stdin, "{}", serde_json::to_string(&seed_req)?)?;
    }
    let seed_out = seed_child.wait_with_output()?;

    // Run `aiw agy remote-ws` from inside a launcher terminal labeled as part of `sp1` / `tb1`.
    let aiw_exe = env!("CARGO_BIN_EXE_aiw");
    let spawn_cmd = format!("{aiw_exe} agy remote-ws -- --version");
    let spawn_res = hc_cmd(&[
        "run",
        "--rm",
        "--label",
        "herdr.space=sp1",
        "--label",
        "herdr.tab=tb1",
        &spawn_cmd,
    ])
    .output();

    // Read `"herdr-app"` back from the live `hc` daemon.
    let get_req = TestRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "doc.get".to_string(),
        params: TestDocGetParams { name: "herdr-app" },
    };
    let mut get_child = hc_cmd(&["connect"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = get_child.stdin.take() {
        writeln!(stdin, "{}", serde_json::to_string(&get_req)?)?;
    }
    let doc_get_out = get_child.wait_with_output();

    // Wait briefly for the background `--bg` workspace terminal to write its marker file.
    let marker = repo
        .join(".workspaces")
        .join("remote-ws")
        .join(".remote_space_marker");
    for _ in 0..20 {
        if marker.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    drop(hc_cmd(&["server", "stop"]).output());

    expect_that!(seed_out.status.success(), is_true());
    let spawn_out = spawn_res?;
    expect_that!(
        spawn_out.status.success(),
        is_true(),
        "spawn stderr: {}",
        String::from_utf8_lossy(&spawn_out.stderr)
    );
    expect_that!(marker.exists(), is_true());

    let doc_out = doc_get_out?;
    expect_that!(doc_out.status.success(), is_true());
    let doc_resp: TestRpcResponse<TestDocGetResult> = serde_json::from_str(
        String::from_utf8_lossy(&doc_out.stdout)
            .lines()
            .next()
            .unwrap_or("{}"),
    )?;
    let space = doc_resp.result.value.spaces.first();
    expect_that!(space.map(|s| s.tabs.len()), some(eq(2)));
    expect_that!(
        space.and_then(|s| s.tabs.first()).map(|t| t.id.as_str()),
        some(eq("tb1"))
    );
    expect_that!(
        space.and_then(|s| s.tabs.get(1)).map(|t| t.name.as_str()),
        some(eq("remote-ws"))
    );

    Ok(())
}

#[googletest::test]
fn aiw_in_hc_with_fence_false_runs_agent_without_fence_or_permissive_flag() -> TestResult {
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    init_test_jj_repo(repo);
    write_test_fence_json(repo);

    let log_file = NamedTempFile::new()?;
    let mock_hc = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mock_hc.sh");

    let output = Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(["--fence", "false", "agy", "my-fenceless-hc-ws"])
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
        contains_substring(
            "--label source=aiw --label agent=agy --label workspace=my-fenceless-hc-ws agy"
        )
    );
    expect_that!(&logged, not(contains_substring("fence ")));
    expect_that!(
        &logged,
        not(contains_substring("--dangerously-skip-permissions"))
    );
    expect_that!(&logged, contains_substring("attach t10"));
    Ok(())
}
