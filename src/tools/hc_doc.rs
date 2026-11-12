use super::hc::HcError;
use clapback::{Command, CommandExt as _};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{BuildHasher as _, Hasher as _};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const HERDR_APP_DOC: &str = "herdr-app";
const MAX_CAS_RETRIES: usize = 5;

#[derive(Command, Debug)]
#[command(arg = "connect", ensure_user_path)]
struct HcConnectCli {
    #[command(program)]
    binary: PathBuf,
}

#[derive(Debug, Serialize)]
struct RpcRequest<'a, P> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: &'a P,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<R> {
    result: Option<R>,
    error: Option<RpcErrorPayload>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorPayload {
    message: String,
    #[serde(default)]
    data: Option<RpcErrorData>,
}

#[derive(Debug, Deserialize)]
struct RpcErrorData {
    #[serde(default)]
    kind: String,
}

#[derive(Debug, Serialize)]
struct DocGetParams<'a> {
    name: &'a str,
}

#[derive(Debug, Deserialize)]
struct DocGetResult {
    #[serde(default)]
    version: u64,
    #[serde(default)]
    value: Option<HerdrAppDoc>,
}

#[derive(Debug, Serialize)]
struct DocSetParams<'a> {
    name: &'a str,
    version: u64,
    value: &'a HerdrAppDoc,
}

#[derive(Debug, Deserialize)]
struct DocSetResult {}

/// Typed representation of the `"herdr-app"` shared layout document stored in `herdr-core`.
///
/// Extra styling or mod attributes set by Herdr Desktop are preserved losslessly across
/// `doc.get` / `doc.set` round-trips via flattened `extra` maps.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct HerdrAppDoc {
    #[serde(default)]
    pub(crate) spaces: Vec<SharedSpace>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedSpace {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) folder: String,
    #[serde(default)]
    pub(crate) tabs: Vec<SharedTab>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedTab {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default = "empty_root_node")]
    pub(crate) root: LayoutNode,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LayoutNode {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) node_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<NodeContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) children: Option<Vec<LayoutNode>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) floating: Vec<FloatingNode>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct NodeContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) terminal: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct FloatingNode {
    #[serde(default)]
    pub(crate) id: String,
    pub(crate) content: NodeContent,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

fn empty_root_node() -> LayoutNode {
    LayoutNode {
        id: String::new(),
        node_type: "row".to_string(),
        content: None,
        children: Some(Vec::new()),
        floating: Vec::new(),
        extra: BTreeMap::new(),
    }
}

#[derive(Debug)]
pub(crate) struct HcDocSnapshot {
    pub(crate) version: u64,
    pub(crate) doc: HerdrAppDoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HerdrDocLocation {
    pub(crate) space_id: String,
    pub(crate) tab_id: Option<String>,
    pub(crate) tab_name: Option<String>,
}

impl LayoutNode {
    #[must_use]
    fn contains_terminal(&self, terminal_id: &str) -> bool {
        if self.content.as_ref().and_then(|c| c.terminal.as_deref()) == Some(terminal_id) {
            return true;
        }

        if self
            .floating
            .iter()
            .any(|f| f.content.terminal.as_deref() == Some(terminal_id))
        {
            return true;
        }

        self.children.as_deref().is_some_and(|children| {
            children
                .iter()
                .any(|child| child.contains_terminal(terminal_id))
        })
    }
}

impl SharedTab {
    #[must_use]
    fn new_terminal_tab(tab_id: &str, tab_name: &str, pane_id: &str, terminal_id: &str) -> Self {
        let pane = LayoutNode {
            id: pane_id.to_owned(),
            node_type: "block".to_owned(),
            content: Some(NodeContent {
                terminal: Some(terminal_id.to_owned()),
                extra: BTreeMap::new(),
            }),
            children: None,
            floating: Vec::new(),
            extra: BTreeMap::new(),
        };

        Self {
            id: tab_id.to_owned(),
            name: tab_name.to_owned(),
            root: LayoutNode {
                id: tab_id.to_owned(),
                node_type: "row".to_owned(),
                content: None,
                children: Some(vec![pane]),
                floating: Vec::new(),
                extra: BTreeMap::new(),
            },
            extra: BTreeMap::new(),
        }
    }
}

impl HerdrAppDoc {
    /// Returns `true` if `terminal_id` is already placed in any space/tab of the `"herdr-app"` document.
    #[must_use]
    pub(crate) fn contains_terminal(&self, terminal_id: &str) -> bool {
        self.spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .any(|tab| tab.root.contains_terminal(terminal_id))
    }

    /// Resolves the space ID, tab ID, and explicit tab title (if non-empty) for `terminal_id`
    /// in the `"herdr-app"` document, falling back to `space_id_hint` / `tab_id_hint` when the
    /// terminal is not yet listed inside a pane node.
    #[must_use]
    pub(crate) fn locate_terminal(
        &self,
        terminal_id: &str,
        space_id_hint: Option<&str>,
        tab_id_hint: Option<&str>,
    ) -> Option<HerdrDocLocation> {
        for space in &self.spaces {
            if space.id.is_empty() {
                continue;
            }
            if let Some(tab) = space
                .tabs
                .iter()
                .find(|t| t.root.contains_terminal(terminal_id))
            {
                return Some(HerdrDocLocation {
                    space_id: space.id.clone(),
                    tab_id: (!tab.id.is_empty()).then(|| tab.id.clone()),
                    tab_name: (!tab.name.is_empty()).then(|| tab.name.clone()),
                });
            }
        }

        let space_id = space_id_hint.filter(|s| !s.is_empty())?;
        let tab_id = tab_id_hint.filter(|s| !s.is_empty());
        let tab_name = self
            .spaces
            .iter()
            .find(|s| s.id == space_id)
            .and_then(|space| {
                let hint = tab_id?;
                space.tabs.iter().find(|t| t.id == hint)
            })
            .and_then(|tab| (!tab.name.is_empty()).then(|| tab.name.clone()));

        Some(HerdrDocLocation {
            space_id: space_id.to_owned(),
            tab_id: tab_id.map(str::to_owned),
            tab_name,
        })
    }

    /// Inserts a new tab into `space_id` (right after `after_tab_id` when present),
    /// preserving all existing spaces, tabs, split layouts, and fields untouched.
    /// Returns `true` if the document was modified and needs to be persisted via `doc.set`.
    #[must_use]
    fn insert_tab(
        &mut self,
        space_id: &str,
        after_tab_id: Option<&str>,
        new_tab: SharedTab,
        terminal_id: &str,
    ) -> bool {
        if self.contains_terminal(terminal_id) {
            return false;
        }

        let Some(space) = self.spaces.iter_mut().find(|s| s.id == space_id) else {
            return false;
        };

        let insert_at = after_tab_id
            .and_then(|after_id| {
                space
                    .tabs
                    .iter()
                    .position(|t| t.id == after_id)
                    .map(|idx| idx + 1)
            })
            .unwrap_or(space.tabs.len());

        space.tabs.insert(insert_at, new_tab);
        true
    }
}

/// Generates a random Herdr layout node identifier (`<prefix><32 hex chars>`),
/// matching `herdr-desktop`'s `Model::fresh_id` format.
#[must_use]
pub(crate) fn fresh_id(prefix: &str) -> String {
    let random = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    format!("{prefix}{:016x}{:016x}", random(), random())
}

fn rpc_call<P: Serialize, R: DeserializeOwned>(
    binary: &Path,
    method: &str,
    params: &P,
) -> Result<R, HcError> {
    let cli = HcConnectCli {
        binary: binary.to_path_buf(),
    };
    let mut cmd = cli.to_std_command();
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|io_err| {
        if io_err.kind() == std::io::ErrorKind::NotFound {
            HcError::BinaryNotFound(binary.display().to_string())
        } else {
            HcError::Io(io_err)
        }
    })?;

    let req = RpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method,
        params,
    };
    let req_line = serde_json::to_string(&req)?;
    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{req_line}")?;
    }

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(HcError::CommandFailed {
            command: format!("{} connect", binary.display()),
            code: output.status.code(),
            stderr: if stderr.trim().is_empty() {
                stdout.into_owned()
            } else {
                stderr.into_owned()
            },
        });
    }

    let first_line = stdout.lines().next().unwrap_or_default();
    let resp: RpcResponse<R> = serde_json::from_str(first_line)?;
    if let Some(err) = resp.error {
        let kind = err.data.map(|d| d.kind).unwrap_or_default();
        return Err(HcError::Api {
            kind,
            message: err.message,
        });
    }

    resp.result.ok_or_else(|| HcError::Api {
        kind: "protocol".to_string(),
        message: format!("hc {method} response missing result"),
    })
}

/// Reads the `"herdr-app"` shared layout document from the local `hc` daemon.
pub(crate) fn get_doc(binary: &Path) -> Result<HcDocSnapshot, HcError> {
    let parsed: DocGetResult = rpc_call(
        binary,
        "doc.get",
        &DocGetParams {
            name: HERDR_APP_DOC,
        },
    )?;
    Ok(HcDocSnapshot {
        version: parsed.version,
        doc: parsed.value.unwrap_or_default(),
    })
}

/// Atomically adds a named tab for `terminal_id` to `space_id` in the `"herdr-app"` layout document
/// via CAS `doc.get` / `doc.set`, retrying on concurrent document version conflicts.
///
/// Returns `Ok(true)` if `terminal_id` is present in `"herdr-app"` upon return, or `Ok(false)`
/// if `space_id` does not yet exist in `"herdr-app"`.
pub(crate) fn add_terminal_tab(
    binary: &Path,
    space_id: &str,
    after_tab_id: Option<&str>,
    tab_id: &str,
    tab_name: &str,
    terminal_id: &str,
) -> Result<bool, HcError> {
    let pane_id = fresh_id("p");
    let new_tab = SharedTab::new_terminal_tab(tab_id, tab_name, &pane_id, terminal_id);

    for attempt in 0..MAX_CAS_RETRIES {
        let mut snap = get_doc(binary)?;
        if snap.doc.contains_terminal(terminal_id) {
            return Ok(true);
        }
        if !snap
            .doc
            .insert_tab(space_id, after_tab_id, new_tab.clone(), terminal_id)
        {
            return Ok(false);
        }

        let set_res: Result<DocSetResult, HcError> = rpc_call(
            binary,
            "doc.set",
            &DocSetParams {
                name: HERDR_APP_DOC,
                version: snap.version,
                value: &snap.doc,
            },
        );

        match set_res {
            Ok(_) => return Ok(true),
            Err(HcError::Api { ref kind, .. })
                if kind == "conflict" && attempt + 1 < MAX_CAS_RETRIES => {}
            Err(err) => return Err(err),
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    const SAMPLE_DOC_JSON: &str = r#"{
        "spaces": [
            {
                "id": "sp1",
                "name": "aiw",
                "folder": "/home/dev/aiw",
                "tabs": [
                    {
                        "id": "tb1",
                        "name": "",
                        "root": {
                            "id": "tb1",
                            "type": "row",
                            "children": [
                                {
                                    "id": "p1",
                                    "type": "block",
                                    "size": { "weight": 0.6 },
                                    "content": { "terminal": "t2" }
                                },
                                {
                                    "id": "p_mod",
                                    "type": "block",
                                    "content": { "mod": "board", "props": { "filter": "open" } }
                                }
                            ]
                        }
                    },
                    {
                        "id": "tb2",
                        "name": "desktop-tab-ws",
                        "root": {
                            "id": "tb2",
                            "type": "row",
                            "children": [
                                {
                                    "id": "p2",
                                    "type": "block",
                                    "content": { "terminal": "t6" }
                                }
                            ]
                        }
                    }
                ]
            }
        ]
    }"#;

    fn sample_doc() -> Result<HerdrAppDoc, serde_json::Error> {
        serde_json::from_str(SAMPLE_DOC_JSON)
    }

    #[googletest::test]
    fn fresh_id_uses_prefix_and_32_hex_chars() {
        let id = fresh_id("tb");
        expect_that!(id.starts_with("tb"), is_true());
        expect_that!(id.len(), eq(34));
    }

    #[googletest::test]
    fn locate_terminal_resolves_space_tab_and_named_titles()
    -> Result<(), Box<dyn std::error::Error>> {
        let doc = sample_doc()?;
        expect_that!(
            doc.locate_terminal("t2", Some("sp1"), Some("tb1")),
            some(eq(&HerdrDocLocation {
                space_id: "sp1".to_string(),
                tab_id: Some("tb1".to_string()),
                tab_name: None,
            }))
        );
        expect_that!(
            doc.locate_terminal("t6", None, None),
            some(eq(&HerdrDocLocation {
                space_id: "sp1".to_string(),
                tab_id: Some("tb2".to_string()),
                tab_name: Some("desktop-tab-ws".to_string()),
            }))
        );
        expect_that!(doc.locate_terminal("t1", None, None), none());
        Ok(())
    }

    #[googletest::test]
    fn insert_tab_inserts_after_caller_tab_and_roundtrips_styling_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut doc = sample_doc()?;
        let new_tab = SharedTab::new_terminal_tab("tb_new", "my-ws", "p_new", "t10");
        let modified = doc.insert_tab("sp1", Some("tb1"), new_tab, "t10");
        expect_that!(modified, is_true());
        expect_that!(doc.contains_terminal("t10"), is_true());

        let space = doc.spaces.first();
        expect_that!(space.map(|s| s.tabs.len()), some(eq(3)));
        let inserted = space.and_then(|s| s.tabs.get(1));
        expect_that!(inserted.map(|t| t.id.as_str()), some(eq("tb_new")));
        expect_that!(inserted.map(|t| t.name.as_str()), some(eq("my-ws")));

        // Verify round-trip serialization preserves styling (`size`) and mod panes untouched.
        let serialized = serde_json::to_string(&doc)?;
        expect_that!(&serialized, contains_substring(r#""weight":0.6"#));
        expect_that!(&serialized, contains_substring(r#""mod":"board""#));
        let reparsed: HerdrAppDoc = serde_json::from_str(&serialized)?;
        expect_that!(&reparsed, eq(&doc));

        // Duplicate insertion for the same terminal is a no-op.
        let duplicate = SharedTab::new_terminal_tab("tb_dup", "my-ws", "p_dup", "t10");
        expect_that!(
            doc.insert_tab("sp1", Some("tb1"), duplicate, "t10"),
            is_false()
        );
        Ok(())
    }
}
