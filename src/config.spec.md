# Module Specification: `config` (`src/config.rs`)

## 1. Module Purpose
The `config` module is responsible for locating, reading, deserializing, and validating the project-specific configuration file (`aiw.json`).

## 2. Public API / Contracts
```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AiwConfig {
    pub tools: Vec<String>,
}

#[derive(Debug, thiserror::Error)] // or custom std::fmt::Display
pub enum ConfigError {
    #[error("Configuration file not found at {0}")]
    NotFound(std::path::PathBuf),
    #[error("Failed to read configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse JSON in configuration file: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Configuration 'tools' list cannot be empty")]
    EmptyTools,
}

impl AiwConfig {
    /// Loads configuration directly from a specific file path.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigError>;

    /// Searches for `aiw.json` at `<repo_root>/aiw.json` and loads it.
    pub fn find_and_load(repo_root: &std::path::Path) -> Result<Self, ConfigError>;
}
```

## 3. Invariants
- **Non-Empty Tools:** `tools` must contain at least one tool name. Empty lists return `ConfigError::EmptyTools`.
- **Explicit Schema:** Validates standard JSON format matching `{"tools": ["tool1", "tool2"]}`.
- **Pure Loading:** The module does not attempt to execute or resolve paths to tools; it only parses and validates the configuration data structure.

## 4. Verification Plan
- Unit test: Parsing a valid `aiw.json` string returns expected struct.
- Unit test: Missing file returns `ConfigError::NotFound`.
- Unit test: Malformed JSON syntax returns `ConfigError::InvalidJson`.
- Unit test: Empty `tools` array returns `ConfigError::EmptyTools`.
