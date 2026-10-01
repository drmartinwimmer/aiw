use crate::tools::ensure_user_profile_bin_paths;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parsed output from `direnv status`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirenvStatusOutput {
    pub allowed: Option<bool>,
    pub found_rc: Option<PathBuf>,
    pub loaded_rc: Option<PathBuf>,
}

/// Builder for executing `direnv` commands and parsing output.
#[derive(Debug, Default, Clone)]
pub struct DirenvCommand {
    current_dir: Option<PathBuf>,
}

impl DirenvCommand {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn current_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.current_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    fn build_cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("direnv");
        cmd.args(args);
        if let Some(ref dir) = self.current_dir {
            cmd.current_dir(dir);
        }
        ensure_user_profile_bin_paths(&mut cmd);
        cmd
    }

    /// Checks the status of direnv in the target directory.
    pub fn status(&self) -> Result<DirenvStatusOutput, std::io::Error> {
        // Try `direnv status --json` first (direnv >= 2.37.1)
        if let Ok(out) = self.build_cmd(&["status", "--json"]).output()
            && out.status.success()
        {
            return Ok(Self::parse_status(&out.stdout));
        }

        // Fallback to plain human-readable `direnv status` (direnv < 2.37.1)
        let plain_out = self.build_cmd(&["status"]).output()?;
        if plain_out.status.success() {
            return Ok(Self::parse_status(&plain_out.stdout));
        }

        Ok(DirenvStatusOutput::default())
    }

    /// Runs `direnv allow` in the target directory.
    pub fn allow(&self) -> Result<(), std::io::Error> {
        let mut cmd = self.build_cmd(&["allow"]);
        if let Some(ref dir) = self.current_dir {
            cmd.arg(dir);
        }
        let output = cmd.output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(format!(
                "direnv allow failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Parses JSON output from `direnv status --json`.
    pub fn parse_json_status(stdout: &[u8]) -> Option<DirenvStatusOutput> {
        #[derive(Deserialize)]
        struct DirenvStatus {
            state: Option<DirenvState>,
        }

        #[derive(Deserialize)]
        struct DirenvState {
            #[serde(rename = "foundRC")]
            found_rc: Option<FoundRc>,
            #[serde(rename = "loadedRC")]
            loaded_rc: Option<FoundRc>,
        }

        #[derive(Deserialize)]
        struct FoundRc {
            allowed: Option<serde_json::Value>,
            path: Option<PathBuf>,
        }

        let status = serde_json::from_slice::<DirenvStatus>(stdout).ok()?;
        let state = status.state?;

        let allowed = state.found_rc.as_ref().and_then(|rc| {
            rc.allowed.as_ref().and_then(|val| {
                if val.as_i64() == Some(0) || val.as_bool() == Some(true) {
                    Some(true)
                } else if val.as_i64() == Some(1) || val.as_bool() == Some(false) {
                    Some(false)
                } else {
                    None
                }
            })
        });

        let found_rc = state.found_rc.and_then(|rc| rc.path);
        let loaded_rc = state.loaded_rc.and_then(|rc| rc.path);

        Some(DirenvStatusOutput {
            allowed,
            found_rc,
            loaded_rc,
        })
    }

    /// Parses standard text output from `direnv status`.
    pub fn parse_plain_status(stdout: &[u8]) -> DirenvStatusOutput {
        let text = String::from_utf8_lossy(stdout);
        let mut result = DirenvStatusOutput::default();

        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("Found RC allowed") {
                let val = val.trim().trim_start_matches(':').trim();
                result.allowed = match val {
                    "0" | "true" => Some(true),
                    "1" | "false" => Some(false),
                    _ => None,
                };
            } else if let Some(val) = trimmed.strip_prefix("Found RC path") {
                let path_str = val.trim().trim_start_matches(':').trim();
                result.found_rc = Some(PathBuf::from(path_str));
            } else if let Some(val) = trimmed.strip_prefix("Loaded RC path") {
                let path_str = val.trim().trim_start_matches(':').trim();
                result.loaded_rc = Some(PathBuf::from(path_str));
            }
        }

        result
    }

    /// Parses either JSON or plain text status output.
    pub fn parse_status(stdout: &[u8]) -> DirenvStatusOutput {
        Self::parse_json_status(stdout).unwrap_or_else(|| Self::parse_plain_status(stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[googletest::test]
    fn parse_json_status_allowed() {
        let json = br#"{"state":{"foundRC":{"allowed":0,"path":"/tmp/.envrc"}}}"#;
        let parsed = DirenvCommand::parse_json_status(json).expect("parse json");
        expect_that!(parsed.allowed, some(is_true()));
        expect_that!(parsed.found_rc, some(eq(&PathBuf::from("/tmp/.envrc"))));
    }

    #[googletest::test]
    fn parse_plain_status_allowed() {
        let text = b"Found RC allowed 0\nFound RC path /tmp/.envrc\nLoaded RC path /tmp/.envrc\n";
        let parsed = DirenvCommand::parse_plain_status(text);
        expect_that!(parsed.allowed, some(is_true()));
        expect_that!(parsed.found_rc, some(eq(&PathBuf::from("/tmp/.envrc"))));
    }

    #[googletest::test]
    fn parse_plain_status_blocked() {
        let text = b"Found RC allowed 1\nFound RC path /tmp/.envrc\n";
        let parsed = DirenvCommand::parse_plain_status(text);
        expect_that!(parsed.allowed, some(is_false()));
    }

    #[googletest::test]
    fn parse_status_handles_json_and_text_formats() {
        // 1. Modern JSON format with integer allowed (direnv >= 2.37.1)
        let json_allowed = br#"{
            "state": {
                "foundRC": {
                    "allowed": 0,
                    "path": "/project/.envrc"
                }
            }
        }"#;
        expect_that!(
            DirenvCommand::parse_status(json_allowed).allowed,
            eq(Some(true))
        );

        let json_unallowed = br#"{
            "state": {
                "foundRC": {
                    "allowed": 1,
                    "path": "/project/.envrc"
                }
            }
        }"#;
        expect_that!(
            DirenvCommand::parse_status(json_unallowed).allowed,
            eq(Some(false))
        );

        // 2. JSON format with boolean allowed
        let json_bool_allowed = br#"{"state":{"foundRC":{"allowed":true}}}"#;
        expect_that!(
            DirenvCommand::parse_status(json_bool_allowed).allowed,
            eq(Some(true))
        );

        let json_bool_unallowed = br#"{"state":{"foundRC":{"allowed":false}}}"#;
        expect_that!(
            DirenvCommand::parse_status(json_bool_unallowed).allowed,
            eq(Some(false))
        );

        // 3. Human-readable text format with integer (direnv ~ 2.32 - 2.36)
        let text_allowed = b"direnv exec path /usr/bin/direnv\nFound RC path /app/.envrc\nFound RC allowed 0\nFound RC allowPath /allow/hash";
        expect_that!(
            DirenvCommand::parse_status(text_allowed).allowed,
            eq(Some(true))
        );

        let text_unallowed = b"direnv exec path /usr/bin/direnv\nFound RC path /app/.envrc\nFound RC allowed 1\nFound RC allowPath /allow/hash";
        expect_that!(
            DirenvCommand::parse_status(text_unallowed).allowed,
            eq(Some(false))
        );

        // 4. Legacy text format with boolean
        let text_legacy_allowed = b"Found RC path /app/.envrc\nFound RC allowed true\n";
        expect_that!(
            DirenvCommand::parse_status(text_legacy_allowed).allowed,
            eq(Some(true))
        );

        let text_legacy_unallowed = b"Found RC path /app/.envrc\nFound RC allowed false\n";
        expect_that!(
            DirenvCommand::parse_status(text_legacy_unallowed).allowed,
            eq(Some(false))
        );

        // 5. Loaded RC present but no Found RC (should return None, not be confused with Loaded RC)
        let text_loaded_only = b"Loaded RC allowed 0\nLoaded RC path /app/.envrc\n";
        expect_that!(
            DirenvCommand::parse_status(text_loaded_only).allowed,
            none()
        );

        // 6. Non-matching or empty output
        expect_that!(DirenvCommand::parse_status(b"").allowed, none());
        expect_that!(DirenvCommand::parse_status(b"random text").allowed, none());
    }
}
