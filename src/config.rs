use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// Default embedded `fence.jsonc` template content for aiw.
const DEFAULT_AIW_TEMPLATE: &str = include_str!("../templates/fence.jsonc");

#[derive(Debug, thiserror::Error)]
pub enum ConfigInitError {
    #[error("Configuration already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("Could not determine user configuration directory")]
    UserConfigPathNotFound,
    #[error("I/O error during config initialization: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ConfigInitializer;

impl ConfigInitializer {
    /// Returns the canonical user config path (e.g. ~/.config/aiw/fence.jsonc).
    #[must_use]
    pub(crate) fn user_config_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "aiw").map(|proj| proj.config_dir().join("fence.jsonc"))
    }

    /// Initializes user configuration (~/.config/aiw/fence.jsonc) if it does not exist yet (or if force is true).
    pub fn init_user_config(force: bool) -> Result<PathBuf, ConfigInitError> {
        let path = Self::user_config_path().ok_or(ConfigInitError::UserConfigPathNotFound)?;

        if path.exists() && !force {
            return Err(ConfigInitError::AlreadyExists(path));
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, DEFAULT_AIW_TEMPLATE)?;
        Ok(path)
    }

    /// Initializes project configuration (fence.jsonc) in `target_dir` if it does not exist yet (or if force is true).
    pub fn init_project_config(target_dir: &Path, force: bool) -> Result<PathBuf, ConfigInitError> {
        let fence_jsonc = target_dir.join("fence.jsonc");
        let fence_json = target_dir.join("fence.json");

        if !force {
            if fence_jsonc.exists() {
                return Err(ConfigInitError::AlreadyExists(fence_jsonc));
            }
            if fence_json.exists() {
                return Err(ConfigInitError::AlreadyExists(fence_json));
            }
        }

        let base_path_str = Self::user_config_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "~/.config/aiw/fence.jsonc".to_string());

        let content = format!(
            r#"{{
  "$schema": "https://raw.githubusercontent.com/fencesandbox/fence/main/docs/schema/fence.schema.json",
  "extends": "{base_path_str}",
  "filesystem": {{
    "allowRead": []
  }}
}}
"#
        );

        std::fs::write(&fence_jsonc, content)?;
        Ok(fence_jsonc)
    }

    /// Returns template candidate paths from system data directories (`$XDG_DATA_DIRS`).
    #[must_use]
    pub(crate) fn system_template_paths() -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
            for data_dir in data_dirs.split(':').filter(|s| !s.is_empty()) {
                candidates.push(Path::new(data_dir).join("aiw").join("fence.jsonc"));
            }
        }
        candidates.push(PathBuf::from("/usr/local/share/aiw/fence.jsonc"));
        candidates.push(PathBuf::from("/usr/share/aiw/fence.jsonc"));
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn default_aiw_template_contains_schema_and_extends() {
        expect_that!(
            DEFAULT_AIW_TEMPLATE,
            contains_substring("fence.schema.json")
        );
        expect_that!(
            DEFAULT_AIW_TEMPLATE,
            contains_substring("\"extends\": \"code\"")
        );
        expect_that!(DEFAULT_AIW_TEMPLATE, contains_substring("\"/nix\""));
    }

    #[googletest::test]
    fn system_template_paths_includes_standard_locations() {
        let paths = ConfigInitializer::system_template_paths();
        expect_that!(
            paths,
            superset_of([
                &PathBuf::from("/usr/local/share/aiw/fence.jsonc"),
                &PathBuf::from("/usr/share/aiw/fence.jsonc"),
            ])
        );
    }

    #[googletest::test]
    fn init_project_config_creates_fence_jsonc_with_extends()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let project_dir = temp_dir.path();

        let created = ConfigInitializer::init_project_config(project_dir, false)?;
        expect_that!(created, eq(&project_dir.join("fence.jsonc")));

        let fence_file = project_dir.join("fence.jsonc");
        expect_that!(fence_file.exists(), is_true());

        let content = std::fs::read_to_string(&fence_file)?;
        expect_that!(&content, contains_substring("fence.schema.json"));
        expect_that!(&content, contains_substring("\"extends\":"));

        // Second call without force returns AlreadyExists error
        let second = ConfigInitializer::init_project_config(project_dir, false);
        expect_that!(
            second,
            err(matches_pattern!(ConfigInitError::AlreadyExists(_)))
        );

        // Call with force returns Ok(path)
        let forced = ConfigInitializer::init_project_config(project_dir, true)?;
        expect_that!(forced, eq(&project_dir.join("fence.jsonc")));
        Ok(())
    }
}
