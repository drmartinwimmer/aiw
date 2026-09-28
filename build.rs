use std::path::{Path, PathBuf};

fn is_cargo_install() -> bool {
    if std::env::var("AIW_SKIP_CONFIG_INSTALL").is_ok() {
        return false;
    }

    // Walk ancestor processes up to pid 1 to check if any ancestor command was `cargo install`
    let mut pid = std::os::unix::process::parent_id();
    while pid > 1 {
        let cmdline_path = format!("/proc/{pid}/cmdline");
        if let Ok(content) = std::fs::read_to_string(&cmdline_path) {
            let args: Vec<&str> = content.split('\0').filter(|s| !s.is_empty()).collect();
            if let Some(first) = args.first()
                && (first.ends_with("cargo") || first.ends_with("cargo.exe") || *first == "cargo")
                && args.iter().skip(1).any(|&arg| arg == "install")
            {
                return true;
            }
        }

        let stat_path = format!("/proc/{pid}/stat");
        if let Ok(stat) = std::fs::read_to_string(&stat_path)
            && let Some(idx) = stat.rfind(')')
            && let Some(rest) = stat.get(idx.saturating_add(1)..)
        {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(ppid_str) = parts.get(1)
                && let Ok(next_pid) = ppid_str.parse::<u32>()
            {
                if next_pid == pid || next_pid == 0 {
                    break;
                }
                pid = next_pid;
                continue;
            }
        }
        break;
    }

    false
}

fn main() {
    println!("cargo:rerun-if-changed=templates/aiw.jsonc");
    println!("cargo:rerun-if-env-changed=AIW_SKIP_CONFIG_INSTALL");

    // Only place the default template into user config during `cargo install`, never during `cargo build`
    if !is_cargo_install() {
        return;
    }

    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(&home).join(".config"));
        let aiw_dir = config_home.join("aiw");
        let dest = aiw_dir.join("aiw.jsonc");

        if !dest.exists() && std::fs::create_dir_all(&aiw_dir).is_ok() {
            let src = Path::new("templates/aiw.jsonc");
            if src.exists()
                && let Err(err) = std::fs::copy(src, &dest)
            {
                println!(
                    "cargo:warning=Failed to copy default template to {}: {err}",
                    dest.display()
                );
            }
        }
    }
}
