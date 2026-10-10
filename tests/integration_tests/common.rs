use std::path::Path;
use std::process::{Command, Output};

#[must_use]
pub fn can_run_fence() -> bool {
    // Nested container/LSM sandboxing inside the Nix daemon build sandbox
    // is unsupported due to multi-layer namespace filesystem isolation.
    if std::env::var("NIX_BUILD_TOP").is_ok() || which::which("fence").is_err() {
        return false;
    }
    let output = Command::new("fence").args(["--", "true"]).output();
    matches!(output, Ok(out) if out.status.success())
}

pub fn init_test_git_repo(path: &Path) {
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
        assert!(
            fallback.status.success(),
            "Failed to initialize Git repository"
        );
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

pub fn init_test_jj_repo(path: &Path) {
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

pub fn write_test_fence_json(repo_root: &Path) {
    let tpl_dir = repo_root.join("templates");
    std::fs::create_dir_all(&tpl_dir).expect("create templates dir");
    let tpl_content = r#"{
  "$schema": "https://raw.githubusercontent.com/fencesandbox/fence/main/docs/schema/fence.schema.json",
  "command": {
    "acceptSharedBinaryCannotRuntimeDeny": ["chroot"]
  },
  "extends": "code",
  "filesystem": {
    "allowWrite": [
      ".",
      "../../.git/**",
      "../../.jj/**",
      ".git/**",
      ".jj/**",
      ".workspaces/**",
      "~/.gemini/**",
      "~/.local/share/**",
      "~/.local/share/keyrings/**"
    ]
  },
  "network": {
    "allowLocalOutbound": false,
    "allowedDomains": [
      "accounts.google.com",
      "aicode.googleapis.com",
      "aiplatform.googleapis.com",
      "cloudcode-pa.googleapis.com",
      "daily-cloudcode-pa.googleapis.com",
      "lh1.googleusercontent.com",
      "lh2.googleusercontent.com",
      "lh3.googleusercontent.com",
      "lh4.googleusercontent.com",
      "lh5.googleusercontent.com",
      "lh6.googleusercontent.com",
      "oauth2.googleapis.com",
      "www.googleapis.com"
    ]
  }
}"#;
    std::fs::write(tpl_dir.join("fence.jsonc"), tpl_content).expect("write templates/fence.jsonc");

    let fence_content = r#"{
  "$schema": "https://raw.githubusercontent.com/fencesandbox/fence/main/docs/schema/fence.schema.json",
  "extends": "./templates/fence.jsonc",
  "filesystem": {
    "allowRead": ["/nix"]
  }
}"#;
    std::fs::write(repo_root.join("fence.jsonc"), fence_content).expect("write fence.jsonc");
}

pub fn run_aiw(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aiw"))
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("execute aiw binary")
}

pub fn run_aiw_with_env(cwd: &Path, envs: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aiw"));
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.args(args).current_dir(cwd).output().expect("run aiw")
}
