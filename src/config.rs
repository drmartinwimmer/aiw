use std::path::{Path, PathBuf};

/// Default embedded `fence.jsonc` template content for aiw.
pub const DEFAULT_AIW_TEMPLATE: &str = include_str!("../templates/fence.jsonc");

#[derive(Debug, thiserror::Error)]
pub enum ConfigInitError {
    #[error("I/O error during config initialization: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ConfigInitializer;

impl ConfigInitializer {
    /// Returns the canonical user config path (e.g. ~/.config/aiw/fence.jsonc).
    #[must_use]
    pub fn user_config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "aiw")
            .map(|proj| proj.config_dir().join("fence.jsonc"))
    }

    /// Initializes user configuration (~/.config/aiw/fence.jsonc) if it does not exist yet (or if force is true).
    /// Returns `Ok(Some(path))` if created, `Ok(None)` if already exists and not forced.
    pub fn init_user_config(force: bool) -> Result<Option<PathBuf>, ConfigInitError> {
        let Some(path) = Self::user_config_path() else {
            return Ok(None);
        };

        if path.exists() && !force {
            return Ok(None);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, DEFAULT_AIW_TEMPLATE)?;
        Ok(Some(path))
    }

    /// Initializes project configuration (fence.jsonc) in `target_dir` if it does not exist yet (or if force is true).
    /// Returns `Ok(Some(path))` if created, `Ok(None)` if already exists and not forced.
    pub fn init_project_config(
        target_dir: &Path,
        force: bool,
    ) -> Result<Option<PathBuf>, ConfigInitError> {
        let fence_jsonc = target_dir.join("fence.jsonc");
        let fence_json = target_dir.join("fence.json");

        if (fence_jsonc.exists() || fence_json.exists()) && !force {
            return Ok(None);
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
        Ok(Some(fence_jsonc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn default_aiw_template_contains_schema_and_extends() {
        expect_that!(DEFAULT_AIW_TEMPLATE, contains_substring("fence.schema.json"));
        expect_that!(DEFAULT_AIW_TEMPLATE, contains_substring("\"extends\": \"code\""));
    }

    #[googletest::test]
    fn init_project_config_creates_fence_jsonc_with_extends() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let project_dir = temp_dir.path();

        let created = ConfigInitializer::init_project_config(project_dir, false)
            .expect("init_project_config");
        expect_that!(created, some(anything()));

        let fence_file = project_dir.join("fence.jsonc");
        expect_that!(fence_file.exists(), is_true());

        let content = std::fs::read_to_string(&fence_file).expect("read fence.jsonc");
        expect_that!(content.as_str(), contains_substring("fence.schema.json"));
        expect_that!(content.as_str(), contains_substring("\"extends\":"));

        // Second call without force returns None
        let second = ConfigInitializer::init_project_config(project_dir, false)
            .expect("second init");
        expect_that!(second, none());

        // Call with force returns Some
        let forced = ConfigInitializer::init_project_config(project_dir, true)
            .expect("forced init");
        expect_that!(forced, some(anything()));
    }
}
