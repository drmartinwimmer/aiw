pub(crate) mod direnv;
pub(crate) mod git;
pub(crate) mod jj;

pub(crate) use direnv::DirenvCommand;
pub(crate) use git::GitCommand;
pub(crate) use jj::JjCommand;

use directories::BaseDirs;
use std::path::PathBuf;
use std::process::Command;

/// Ensures common user binary directories (`~/.local/bin`, `~/.nix-profile/bin`, `/etc/profiles/per-user/...`)
/// are included in the command's `PATH`.
pub(crate) fn ensure_user_profile_bin_paths(cmd: &mut Command) {
    let Ok(path_var) = std::env::var("PATH") else {
        return;
    };

    let mut paths: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    let mut updated = false;

    if let Some(base_dirs) = BaseDirs::new() {
        let home = base_dirs.home_dir();
        let local_bin = home.join(".local/bin");
        if local_bin.exists() && !paths.contains(&local_bin) {
            paths.insert(0, local_bin);
            updated = true;
        }
        let nix_bin = home.join(".nix-profile/bin");
        if nix_bin.exists() && !paths.contains(&nix_bin) {
            paths.push(nix_bin);
            updated = true;
        }
    }

    if let Ok(user) = std::env::var("USER") {
        let user_bin = PathBuf::from(format!("/etc/profiles/per-user/{user}/bin"));
        if user_bin.exists() && !paths.contains(&user_bin) {
            paths.push(user_bin);
            updated = true;
        }
    }

    if updated && let Ok(new_path) = std::env::join_paths(paths) {
        cmd.env("PATH", new_path);
    }
}
