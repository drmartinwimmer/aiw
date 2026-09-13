use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("Required host tool '{0}' could not be found in PATH")]
    ToolNotFound(String),
    #[error("Bubblewrap executable 'bwrap' not found in PATH")]
    BubblewrapNotFound,
    #[error("Failed to resolve symlink for path {0}: {1}")]
    SymlinkResolutionFailed(PathBuf, std::io::Error),
    #[error("I/O error during sandbox execution: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct SandboxConfig<'a> {
    pub repo_root: &'a Path,
    pub workspace_path: &'a Path,
    pub tools: &'a [String],
    pub extra_args: &'a [String],
    pub home_dir: Option<&'a Path>,
    pub network: bool,
}

#[derive(Debug)]
pub struct SandboxBuilder<'a> {
    config: SandboxConfig<'a>,
}

fn resolve_tool_dir(tool: &str) -> Result<Option<PathBuf>, SandboxError> {
    let tool_bin = match which::which(tool) {
        Ok(path) => path,
        Err(_) => return Err(SandboxError::ToolNotFound(tool.to_string())),
    };
    let tool_canonical = match std::fs::canonicalize(&tool_bin) {
        Ok(canon) => canon,
        Err(err) => return Err(SandboxError::SymlinkResolutionFailed(tool_bin, err)),
    };
    Ok(tool_canonical.parent().map(Path::to_path_buf))
}

impl<'a> SandboxBuilder<'a> {
    pub fn new(config: SandboxConfig<'a>) -> Self {
        Self { config }
    }

    /// Resolves required tools and system paths, returning the list of bwrap CLI arguments.
    pub fn build_args(&self) -> Result<Vec<String>, SandboxError> {
        // 1. Verify bwrap exists on the host
        if which::which("bwrap").is_err() {
            return Err(SandboxError::BubblewrapNotFound);
        }

        // 2. Resolve agy and all configured tools
        let tool_dirs = self.resolve_all_tool_dirs()?;
        let home_path = self.resolve_home_dir();

        // 3. Assemble Bubblewrap CLI arguments
        let mut args = Vec::new();
        self.append_base_namespaces(&mut args);
        Self::append_ro_system_mounts(&mut args);
        Self::append_nix_socket_mount(&mut args);
        Self::append_tool_mounts(&mut args, &tool_dirs);
        self.append_writable_mounts(&mut args, home_path.as_deref());
        Self::append_runtime_auth_mounts(&mut args);
        Self::append_env_vars(&mut args, &tool_dirs, home_path.as_deref());
        self.append_workspace_and_command(&mut args);

        Ok(args)
    }

    fn resolve_all_tool_dirs(&self) -> Result<Vec<PathBuf>, SandboxError> {
        let mut tool_dirs: Vec<PathBuf> = Vec::new();
        for tool in std::iter::once("agy").chain(self.config.tools.iter().map(String::as_str)) {
            if let Some(dir) = resolve_tool_dir(tool)?
                && !tool_dirs.contains(&dir)
            {
                tool_dirs.push(dir);
            }
        }
        Ok(tool_dirs)
    }

    fn resolve_home_dir(&self) -> Option<PathBuf> {
        self.config
            .home_dir
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }

    fn append_base_namespaces(&self, args: &mut Vec<String>) {
        args.extend([
            "--die-with-parent".to_string(),
            "--proc".to_string(),
            "/proc".to_string(),
            "--dev".to_string(),
            "/dev".to_string(),
            "--tmpfs".to_string(),
            "/tmp".to_string(),
        ]);

        if self.config.network {
            args.push("--share-net".to_string());
        }
    }

    fn append_ro_system_mounts(args: &mut Vec<String>) {
        let ro_system_paths = [
            "/nix",
            "/bin",
            "/lib",
            "/lib64",
            "/usr",
            "/etc/resolv.conf",
            "/etc/hosts",
            "/etc/ssl",
            "/etc/pki",
            "/etc/static",
            "/etc/passwd",
            "/etc/group",
            "/etc/nix",
            "/etc/profiles/per-user",
            "/run/systemd/resolve",
            "/var/run/nscd",
        ];

        for path_str in &ro_system_paths {
            if Path::new(path_str).exists() {
                args.push("--ro-bind".to_string());
                args.push((*path_str).to_string());
                args.push((*path_str).to_string());
            }
        }
    }

    fn append_nix_socket_mount(args: &mut Vec<String>) {
        let nix_socket = Path::new("/nix/var/nix/daemon-socket");
        if nix_socket.exists() {
            args.push("--bind".to_string());
            args.push("/nix/var/nix/daemon-socket".to_string());
            args.push("/nix/var/nix/daemon-socket".to_string());
        }
    }

    fn append_tool_mounts(args: &mut Vec<String>, tool_dirs: &[PathBuf]) {
        let nix_mounted = Path::new("/nix").exists();
        let usr_mounted = Path::new("/usr").exists();

        for dir in tool_dirs {
            let covered_by_nix = nix_mounted && dir.starts_with("/nix");
            let covered_by_usr = usr_mounted && dir.starts_with("/usr");
            if !covered_by_nix && !covered_by_usr {
                let dir_str = dir.display().to_string();
                args.push("--ro-bind".to_string());
                args.push(dir_str.clone());
                args.push(dir_str);
            }
        }
    }

    fn append_writable_mounts(&self, args: &mut Vec<String>, home_path: Option<&Path>) {
        let ws_str = self.config.workspace_path.display().to_string();
        args.push("--bind".to_string());
        args.push(ws_str.clone());
        args.push(ws_str);

        let repo_jj = self.config.repo_root.join(".jj");
        let repo_jj_str = repo_jj.display().to_string();
        args.push("--bind".to_string());
        args.push(repo_jj_str.clone());
        args.push(repo_jj_str);

        if let Some(home) = home_path {
            let gemini_dir = home.join(".gemini");
            if !gemini_dir.exists() {
                match std::fs::create_dir_all(&gemini_dir) {
                    Ok(()) | Err(_) => {}
                }
            }
            if gemini_dir.exists() {
                let gemini_str = gemini_dir.display().to_string();
                args.push("--bind".to_string());
                args.push(gemini_str.clone());
                args.push(gemini_str);
            }

            let keyrings_dir = home.join(".local/share/keyrings");
            if keyrings_dir.exists() {
                let keyrings_str = keyrings_dir.display().to_string();
                args.push("--bind".to_string());
                args.push(keyrings_str.clone());
                args.push(keyrings_str);
            }
        }
    }

    fn append_runtime_auth_mounts(args: &mut Vec<String>) {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let runtime_path = PathBuf::from(&runtime_dir);
            if runtime_path.exists() {
                args.push("--bind".to_string());
                args.push(runtime_dir.clone());
                args.push(runtime_dir);
            }
        }
    }

    fn append_env_vars(
        args: &mut Vec<String>,
        tool_dirs: &[PathBuf],
        home_path: Option<&Path>,
    ) {
        let mut path_entries = tool_dirs.to_vec();
        for std_dir in [PathBuf::from("/usr/bin"), PathBuf::from("/bin")] {
            if !path_entries.contains(&std_dir) {
                path_entries.push(std_dir);
            }
        }
        let path_val = path_entries
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");

        args.push("--setenv".to_string());
        args.push("PATH".to_string());
        args.push(path_val);

        if let Some(home) = home_path {
            args.push("--setenv".to_string());
            args.push("HOME".to_string());
            args.push(home.display().to_string());
        }

        for var_name in [
            "USER",
            "TERM",
            "COLORTERM",
            "SSL_CERT_FILE",
            "NIX_SSL_CERT_FILE",
            "NIX_PATH",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_RUNTIME_DIR",
        ] {
            if let Ok(val) = std::env::var(var_name) {
                args.push("--setenv".to_string());
                args.push(var_name.to_string());
                args.push(val);
            }
        }
    }

    fn append_workspace_and_command(&self, args: &mut Vec<String>) {
        args.push("--chdir".to_string());
        args.push(self.config.workspace_path.display().to_string());

        args.push("--".to_string());
        args.push("agy".to_string());
        for extra in self.config.extra_args {
            args.push(extra.clone());
        }
    }

    /// Builds the std::process::Command prepared to execute bwrap with proper env and args.
    pub fn build_command(&self) -> Result<Command, SandboxError> {
        let args = self.build_args()?;
        let mut cmd = Command::new("bwrap");
        cmd.args(args);
        Ok(cmd)
    }

    /// Executes the container synchronously, forwarding standard I/O and returning its ExitStatus.
    pub fn run(&self) -> Result<ExitStatus, SandboxError> {
        let mut cmd = self.build_command()?;
        cmd.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        let status = cmd.status()?;
        Ok(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct TestToolGuard {
        _temp_dir: Option<tempfile::TempDir>,
        orig_path: Option<String>,
    }

    impl Drop for TestToolGuard {
        fn drop(&mut self) {
            if let Some(ref orig) = self.orig_path {
                // SAFETY: Restoring PATH in drop after hermetic test execution with TEST_ENV_MUTEX held.
                unsafe {
                    std::env::set_var("PATH", orig);
                }
            }
        }
    }

    fn ensure_test_tools() -> (std::sync::MutexGuard<'static, ()>, TestToolGuard) {
        let guard = TEST_ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        let needs_bwrap = which::which("bwrap").is_err();
        let needs_agy = which::which("agy").is_err();

        if !needs_bwrap && !needs_agy {
            return (
                guard,
                TestToolGuard {
                    _temp_dir: None,
                    orig_path: None,
                },
            );
        }

        let temp_bin = tempfile::tempdir().expect("tempdir");
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        if needs_bwrap {
            let bwrap_path = temp_bin.path().join("bwrap");
            std::fs::write(&bwrap_path, "#!/bin/sh\nexit 0\n").expect("write bwrap stub");
            let mut perms = std::fs::metadata(&bwrap_path)
                .expect("metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bwrap_path, perms).expect("set_permissions");
        }

        if needs_agy {
            let agy_path = temp_bin.path().join("agy");
            std::fs::write(&agy_path, "#!/bin/sh\nexit 0\n").expect("write agy stub");
            let mut perms = std::fs::metadata(&agy_path)
                .expect("metadata")
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&agy_path, perms).expect("set_permissions");
        }

        let orig_path = std::env::var("PATH").ok();
        let new_path = match orig_path {
            Some(ref p) => format!("{}:{}", temp_bin.path().display(), p),
            None => temp_bin.path().display().to_string(),
        };

        // SAFETY: Mutex guard is held during test execution, preventing concurrent PATH modification.
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        (
            guard,
            TestToolGuard {
                _temp_dir: Some(temp_bin),
                orig_path,
            },
        )
    }

    fn contains_subslice(args: &[String], expected: &[&str]) -> bool {
        if expected.is_empty() {
            return true;
        }
        args.windows(expected.len())
            .any(|window| window.iter().zip(expected.iter()).all(|(a, b)| a == b))
    }

    #[test]
    fn resolve_tools_with_missing_tool_returns_tool_not_found() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let dummy_repo = Path::new("/dummy/repo");
        let dummy_ws = Path::new("/dummy/ws");
        let missing_tools = vec!["non_existent_tool_xyz_98765".to_string()];
        let extra_args = vec![];

        let config = SandboxConfig {
            repo_root: dummy_repo,
            workspace_path: dummy_ws,
            tools: &missing_tools,
            extra_args: &extra_args,
            home_dir: None,
            network: false,
        };

        let builder = SandboxBuilder::new(config);
        let res = builder.build_args();
        assert!(
            matches!(res, Err(SandboxError::ToolNotFound(ref tool)) if tool == "non_existent_tool_xyz_98765")
        );
    }

    #[test]
    fn build_args_with_valid_tools_produces_required_flags_and_mounts() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        let repo_jj = repo_root.join(".jj");
        std::fs::create_dir_all(&repo_jj).expect("create repo .jj");

        let workspace_path = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_path).expect("create workspace");

        let home_dir = temp_dir.path().join("home");
        std::fs::create_dir_all(&home_dir).expect("create home");

        let tools = vec!["cargo".to_string()];
        let extra_args = vec![];

        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &tools,
            extra_args: &extra_args,
            home_dir: Some(&home_dir),
            network: true,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args should succeed");

        // Bubblewrap core arguments
        assert!(args.contains(&"--die-with-parent".to_string()));
        assert!(args.contains(&"--share-net".to_string()));
        assert!(contains_subslice(&args, &["--proc", "/proc"]));
        assert!(contains_subslice(&args, &["--dev", "/dev"]));
        assert!(contains_subslice(&args, &["--tmpfs", "/tmp"]));

        // Writable mounts
        let ws_str = workspace_path.display().to_string();
        assert!(contains_subslice(&args, &["--bind", &ws_str, &ws_str]));

        let jj_str = repo_jj.display().to_string();
        assert!(contains_subslice(&args, &["--bind", &jj_str, &jj_str]));

        let gemini_str = home_dir.join(".gemini").display().to_string();
        assert!(contains_subslice(
            &args,
            &["--bind", &gemini_str, &gemini_str]
        ));

        // Working directory
        assert!(contains_subslice(&args, &["--chdir", &ws_str]));

        // Command
        assert!(contains_subslice(&args, &["--", "agy"]));

        // Environment variables
        assert!(contains_subslice(&args, &["--setenv", "PATH"]));
        let path_entry = args
            .windows(3)
            .find(|w| {
                w.first().map(|s| s.as_str()) == Some("--setenv")
                    && w.get(1).map(|s| s.as_str()) == Some("PATH")
            })
            .and_then(|w| w.get(2));
        assert!(path_entry.is_some());
        if let Some(path_val) = path_entry {
            let cargo_path = which::which("cargo").expect("cargo in path");
            let cargo_canonical = std::fs::canonicalize(&cargo_path).expect("canonicalize cargo");
            let cargo_dir = cargo_canonical
                .parent()
                .expect("cargo parent")
                .display()
                .to_string();
            assert!(path_val.split(':').any(|dir| dir == cargo_dir));
        }

        let home_str = home_dir.display().to_string();
        assert!(contains_subslice(&args, &["--setenv", "HOME", &home_str]));

        if let Ok(nix_path) = std::env::var("NIX_PATH") {
            assert!(contains_subslice(
                &args,
                &["--setenv", "NIX_PATH", &nix_path]
            ));
        }
        if let Ok(term) = std::env::var("TERM") {
            assert!(contains_subslice(&args, &["--setenv", "TERM", &term]));
        }
        if let Ok(colorterm) = std::env::var("COLORTERM") {
            assert!(contains_subslice(
                &args,
                &["--setenv", "COLORTERM", &colorterm]
            ));
        }
    }

    #[test]
    fn build_args_with_extra_args_passes_args_to_agy() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join(".jj")).expect("create .jj");
        let workspace_path = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let tools = vec!["cargo".to_string()];
        let extra_args = vec!["--prompt".to_string(), "hello world".to_string()];

        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &tools,
            extra_args: &extra_args,
            home_dir: None,
            network: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        assert!(contains_subslice(
            &args,
            &["--", "agy", "--prompt", "hello world"]
        ));
    }

    #[test]
    fn build_args_without_network_omits_share_net() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join(".jj")).expect("create .jj");
        let workspace_path = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let tools = vec!["cargo".to_string()];
        let extra_args = vec![];

        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &tools,
            extra_args: &extra_args,
            home_dir: None,
            network: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        assert!(!args.contains(&"--share-net".to_string()));
    }

    #[test]
    fn build_args_with_missing_gemini_dir_creates_gemini_directory() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join(".jj")).expect("create .jj");
        let workspace_path = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let home_dir = temp_dir.path().join("home_clean");
        std::fs::create_dir_all(&home_dir).expect("create home");
        let gemini_dir = home_dir.join(".gemini");
        assert!(!gemini_dir.exists());

        let tools = vec!["cargo".to_string()];
        let extra_args = vec![];

        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &tools,
            extra_args: &extra_args,
            home_dir: Some(&home_dir),
            network: false,
        };

        let builder = SandboxBuilder::new(config);
        let _args = builder.build_args().expect("build_args");

        assert!(gemini_dir.exists());
    }

    #[test]
    fn build_command_with_valid_config_constructs_bwrap_command() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join(".jj")).expect("create .jj");
        let workspace_path = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let tools = vec!["cargo".to_string()];
        let extra_args = vec![];

        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &tools,
            extra_args: &extra_args,
            home_dir: None,
            network: false,
        };

        let builder = SandboxBuilder::new(config);
        let cmd = builder.build_command().expect("build_command");
        assert_eq!(cmd.get_program(), "bwrap");
    }

    #[test]
    fn build_args_with_nix_paths_binds_nix_mounts_when_present() {
        let (_lock, _tools_guard) = ensure_test_tools();

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let repo_root = temp_dir.path().join("repo");
        std::fs::create_dir_all(repo_root.join(".jj")).expect("create .jj");
        let workspace_path = temp_dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_path).expect("create ws");

        let tools = vec!["cargo".to_string()];
        let extra_args = vec![];

        let config = SandboxConfig {
            repo_root: &repo_root,
            workspace_path: &workspace_path,
            tools: &tools,
            extra_args: &extra_args,
            home_dir: None,
            network: false,
        };

        let builder = SandboxBuilder::new(config);
        let args = builder.build_args().expect("build_args");

        if Path::new("/nix").exists() {
            assert!(contains_subslice(&args, &["--ro-bind", "/nix", "/nix"]));
        }
        if Path::new("/bin").exists() {
            assert!(contains_subslice(&args, &["--ro-bind", "/bin", "/bin"]));
        }
        if Path::new("/run/systemd/resolve").exists() {
            assert!(contains_subslice(
                &args,
                &["--ro-bind", "/run/systemd/resolve", "/run/systemd/resolve"]
            ));
        }
        if Path::new("/nix/var/nix/daemon-socket").exists() {
            assert!(contains_subslice(
                &args,
                &[
                    "--bind",
                    "/nix/var/nix/daemon-socket",
                    "/nix/var/nix/daemon-socket"
                ]
            ));
        }
    }
}
