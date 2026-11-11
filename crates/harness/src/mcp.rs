//! MCP server definitions and the OpenCode inline-config merge.
//!
//! Halo stores MCP servers in `coder.json` and only injects the ones a chat
//! enabled. OpenCode reads a JSON object from `OPENCODE_CONFIG_CONTENT`; this
//! module merges Halo's `mcp` entries without dropping whatever the user
//! already had in that variable.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    #[default]
    Stdio,
    Remote,
}

impl McpTransport {
    pub fn id(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Remote => "remote",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stdio => "Local",
            Self::Remote => "Remote",
        }
    }
}

/// One environment variable or HTTP header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpKeyValue {
    pub key: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransport,
    /// Executable for local (stdio) servers.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<McpKeyValue>,
    /// Endpoint for remote servers.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<McpKeyValue>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            transport: McpTransport::Stdio,
            command: String::new(),
            args: Vec::new(),
            env: Vec::new(),
            url: String::new(),
            headers: Vec::new(),
            enabled: true,
        }
    }
}

impl McpServerConfig {
    /// A row is usable when it has a name and the field its transport needs.
    pub fn is_configured(&self) -> bool {
        if self.name.trim().is_empty() {
            return false;
        }
        match self.transport {
            McpTransport::Stdio => !self.command.trim().is_empty(),
            McpTransport::Remote => !self.url.trim().is_empty(),
        }
    }

    /// One-line summary for settings and the context sidebar.
    pub fn summary(&self) -> String {
        match self.transport {
            McpTransport::Stdio => {
                let args = self.args.join(" ");
                if args.is_empty() {
                    self.command.trim().to_string()
                } else {
                    format!("{} {args}", self.command.trim())
                }
            }
            McpTransport::Remote => self.url.trim().to_string(),
        }
    }

    /// OpenCode `mcp.<name>` entry.
    pub fn to_opencode_value(&self) -> Value {
        match self.transport {
            McpTransport::Stdio => {
                let mut command = Vec::with_capacity(self.args.len() + 1);
                command.push(Value::String(self.command.trim().to_string()));
                command.extend(
                    self.args
                        .iter()
                        .map(|arg| Value::String(arg.trim().to_string()))
                        .filter(|arg| arg.as_str().is_some_and(|arg| !arg.is_empty())),
                );
                json!({
                    "type": "local",
                    "command": command,
                    "environment": key_values(self.env.iter()),
                    "enabled": true,
                })
            }
            McpTransport::Remote => json!({
                "type": "remote",
                "url": self.url.trim(),
                "enabled": true,
                "oauth": false,
                "headers": key_values(self.headers.iter()),
            }),
        }
    }
}

fn key_values<'a>(entries: impl Iterator<Item = &'a McpKeyValue>) -> Map<String, Value> {
    let mut map = Map::new();
    for entry in entries {
        let key = entry.key.trim();
        if key.is_empty() {
            continue;
        }
        map.insert(key.to_string(), Value::String(entry.value.clone()));
    }
    map
}

/// Merge enabled servers into `OPENCODE_CONFIG_CONTENT`.
///
/// `base` is the value the process already carries (if any); unknown keys are
/// preserved and same-named `mcp` entries are replaced by Halo's definition.
/// Returns `None` when no server qualifies, so callers can leave the env alone.
pub fn opencode_config_with_mcp(
    base: Option<&str>,
    servers: &[McpServerConfig],
) -> Result<Option<String>, String> {
    let usable: Vec<&McpServerConfig> = servers
        .iter()
        .filter(|server| server.enabled && server.is_configured())
        .collect();
    if usable.is_empty() {
        return Ok(None);
    }

    let mut root = match base.map(str::trim).filter(|raw| !raw.is_empty()) {
        Some(raw) => serde_json::from_str::<Value>(raw)
            .map_err(|error| format!("OPENCODE_CONFIG_CONTENT is not valid JSON: {error}"))?,
        None => json!({}),
    };
    let config = root
        .as_object_mut()
        .ok_or_else(|| "OpenCode inline config must be a JSON object".to_string())?;
    let mcp = config.entry("mcp").or_insert_with(|| json!({}));
    let mcp = mcp
        .as_object_mut()
        .ok_or_else(|| "OpenCode inline config has a non-object mcp section".to_string())?;
    for server in usable {
        mcp.insert(server.name.trim().to_string(), server.to_opencode_value());
    }
    serde_json::to_string(&root)
        .map(Some)
        .map_err(|error| format!("Could not encode OpenCode inline config: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            command: "npx".into(),
            args: vec!["-y".into(), "server".into()],
            env: vec![McpKeyValue {
                key: "TOKEN".into(),
                value: "secret".into(),
            }],
            ..McpServerConfig::default()
        }
    }

    fn remote(name: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.into(),
            transport: McpTransport::Remote,
            url: "https://mcp.example.com".into(),
            headers: vec![McpKeyValue {
                key: "Authorization".into(),
                value: "Bearer x".into(),
            }],
            ..McpServerConfig::default()
        }
    }

    #[test]
    fn merges_local_and_remote_and_keeps_base_keys() {
        let base = r#"{"theme":"dark","mcp":{"user":{"type":"remote","url":"https://u"}}}"#;
        let encoded = opencode_config_with_mcp(Some(base), &[local("fs"), remote("web")])
            .expect("valid merge")
            .expect("servers present");
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcp"]["user"]["url"], "https://u");
        assert_eq!(value["mcp"]["fs"]["type"], "local");
        assert_eq!(
            value["mcp"]["fs"]["command"],
            json!(["npx", "-y", "server"])
        );
        assert_eq!(value["mcp"]["fs"]["environment"]["TOKEN"], "secret");
        assert_eq!(value["mcp"]["web"]["type"], "remote");
        assert_eq!(value["mcp"]["web"]["oauth"], false);
        assert_eq!(value["mcp"]["web"]["headers"]["Authorization"], "Bearer x");
    }

    #[test]
    fn skips_disabled_and_incomplete_rows() {
        let mut disabled = local("off");
        disabled.enabled = false;
        let blank = McpServerConfig {
            name: "blank".into(),
            ..McpServerConfig::default()
        };
        assert!(
            opencode_config_with_mcp(None, &[disabled, blank])
                .expect("no error")
                .is_none()
        );
    }

    #[test]
    fn rejects_broken_base() {
        assert!(opencode_config_with_mcp(Some("{"), &[local("fs")]).is_err());
    }
}
