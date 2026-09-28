fn main() {
    println!("cargo:rerun-if-changed=templates/aiw.jsonc");

    // During cargo install or builds, place default template into user config directory if missing
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        let config_home = std::env::var("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from(&home).join(".config"));
        let aiw_dir = config_home.join("aiw");
        let dest = aiw_dir.join("aiw.jsonc");

        if !dest.exists() && std::fs::create_dir_all(&aiw_dir).is_ok() {
            let src = std::path::Path::new("templates/aiw.jsonc");
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
