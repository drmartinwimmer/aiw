use std::path::{Path, PathBuf};

pub const STANDARD_DEFAULT_TOOLS: &[&str] = &[
    "grep", "find", "ls", "cat", "cp", "mv", "rm", "mkdir", "sh", "bash", "sed", "awk",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AiwConfig {
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub default_tools: bool,
    #[serde(default)]
    pub network: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found at {0}")]
    NotFound(PathBuf),
    #[error("Failed to read configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse JSON in configuration file: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Configuration 'tools' list cannot be empty")]
    EmptyTools,
}

impl AiwConfig {
    /// Returns the effective list of tools, appending standard Linux tools if `default_tools` is true.
    pub fn effective_tools(&self) -> Vec<String> {
        let mut result = self.tools.clone();
        if self.default_tools {
            for tool in STANDARD_DEFAULT_TOOLS {
                let tool_str = (*tool).to_string();
                if !result.contains(&tool_str) {
                    result.push(tool_str);
                }
            }
        }
        result
    }

    /// Loads configuration directly from a specific file path.
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigError::NotFound(path.to_path_buf()));
            }
            Err(err) => return Err(ConfigError::Io(err)),
        };

        let config: Self = serde_json::from_str(&content)?;
        if config.tools.is_empty() && !config.default_tools {
            return Err(ConfigError::EmptyTools);
        }

        Ok(config)
    }

    /// Searches for `aiw.json` at `<repo_root>/aiw.json` and loads it.
    pub fn find_and_load(repo_root: &Path) -> Result<Self, ConfigError> {
        Self::load_from_path(&repo_root.join("aiw.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_valid_config_load_from_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("aiw.json");
        fs::write(&config_path, r#"{"tools": ["claude", "gemini"]}"#).expect("write");

        let config = AiwConfig::load_from_path(&config_path).expect("load");
        assert_eq!(
            config.tools,
            vec!["claude".to_string(), "gemini".to_string()]
        );
    }

    #[test]
    fn test_valid_config_find_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("aiw.json");
        fs::write(&config_path, r#"{"tools": ["tool1", "tool2"]}"#).expect("write");

        let config = AiwConfig::find_and_load(dir.path()).expect("find_and_load");
        assert_eq!(config.tools, vec!["tool1".to_string(), "tool2".to_string()]);
    }

    #[test]
    fn test_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let non_existent = dir.path().join("aiw.json");

        match AiwConfig::load_from_path(&non_existent) {
            Err(ConfigError::NotFound(p)) => {
                assert_eq!(p, non_existent);
            }
            other => assert!(matches!(other, Err(ConfigError::NotFound(_)))),
        }

        match AiwConfig::find_and_load(dir.path()) {
            Err(ConfigError::NotFound(p)) => {
                assert_eq!(p, non_existent);
            }
            other => assert!(matches!(other, Err(ConfigError::NotFound(_)))),
        }
    }

    #[test]
    fn test_invalid_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let malformed_path = dir.path().join("aiw.json");
        fs::write(&malformed_path, r#"{"tools": ["claude""#).expect("write");

        let res = AiwConfig::load_from_path(&malformed_path);
        assert!(matches!(res, Err(ConfigError::InvalidJson(_))));

        let bad_schema_path = dir.path().join("bad_schema.json");
        fs::write(&bad_schema_path, r#"{"tools": 123}"#).expect("write");

        let bad_schema_res = AiwConfig::load_from_path(&bad_schema_path);
        assert!(matches!(bad_schema_res, Err(ConfigError::InvalidJson(_))));
    }

    #[test]
    fn test_empty_tools() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("aiw.json");
        fs::write(&config_path, r#"{"tools": []}"#).expect("write");

        let res = AiwConfig::load_from_path(&config_path);
        assert!(matches!(res, Err(ConfigError::EmptyTools)));
    }

    #[test]
    fn test_default_tools_and_network_parsing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("aiw.json");
        fs::write(
            &config_path,
            r#"{"default_tools": true, "network": true}"#,
        )
        .expect("write");

        let config = AiwConfig::load_from_path(&config_path).expect("load");
        assert!(config.default_tools);
        assert!(config.network);
        let eff = config.effective_tools();
        assert!(eff.contains(&"grep".to_string()));
        assert!(eff.contains(&"find".to_string()));
        assert!(eff.contains(&"ls".to_string()));
    }

    #[test]
    fn test_network_defaults_to_false() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("aiw.json");
        fs::write(&config_path, r#"{"tools": ["cargo"]}"#).expect("write");

        let config = AiwConfig::load_from_path(&config_path).expect("load");
        assert!(!config.network);
        assert!(!config.default_tools);
        assert_eq!(config.effective_tools(), vec!["cargo".to_string()]);
    }
}
