use anyhow::{Context, Result};
use schemars::{JsonSchema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use toml_edit::DocumentMut;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlzConfig {
    /// Tasks to run, keyed by name (e.g. [tasks.build]). Run with `plz <name>`.
    pub tasks: HashMap<String, Task>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Task {
    /// A single shell command to run
    #[serde(default)]
    pub run: Option<String>,
    /// Multiple commands to run one after another (stops on first failure)
    #[serde(default)]
    pub run_serial: Option<Vec<String>>,
    /// Multiple commands to run concurrently
    #[serde(default)]
    pub run_parallel: Option<Vec<String>>,
    /// Tool environment wrapper: "pnpm" (uses `pnpm exec`) or "uv" (uses `uv run`)
    #[serde(default, rename = "env")]
    #[schemars(rename = "env")]
    pub tool_env: Option<String>,
    /// Working directory (relative to plz.toml)
    #[serde(default)]
    pub dir: Option<String>,
    /// Action to take when the task fails: a command string, { suggest_command = "..." }, or { message = "..." }
    #[serde(default)]
    pub fail_hook: Option<FailHook>,
    /// Description shown in `plz list`
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug)]
pub enum FailHook {
    Command(String),
    Suggest { suggest_command: String },
    Message(String),
}

impl JsonSchema for FailHook {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("FailHook")
    }

    fn json_schema(_: &mut SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "oneOf": [
                {
                    "type": "string",
                    "description": "Shell command to run on failure"
                },
                {
                    "type": "object",
                    "properties": {
                        "suggest_command": {
                            "type": "string",
                            "description": "Command to suggest to the user on failure"
                        }
                    },
                    "required": ["suggest_command"],
                    "additionalProperties": false
                },
                {
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Message to display on failure"
                        }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }
            ]
        })
    }
}

impl<'de> Deserialize<'de> for FailHook {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FailHookVisitor;

        impl<'de> Visitor<'de> for FailHookVisitor {
            type Value = FailHook;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string or a map with suggest_command")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<FailHook, E> {
                Ok(FailHook::Command(v.to_string()))
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<FailHook, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let mut suggest_command = None;
                let mut message = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "suggest_command" => suggest_command = Some(map.next_value::<String>()?),
                        "message" => message = Some(map.next_value::<String>()?),
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                if let Some(cmd) = suggest_command {
                    Ok(FailHook::Suggest {
                        suggest_command: cmd,
                    })
                } else if let Some(msg) = message {
                    Ok(FailHook::Message(msg))
                } else {
                    Err(de::Error::missing_field("suggest_command or message"))
                }
            }
        }

        deserializer.deserialize_any(FailHookVisitor)
    }
}

pub fn load(path: &Path) -> Result<PlzConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let doc: DocumentMut = content
        .parse()
        .with_context(|| "Failed to parse plz.toml")?;

    let mut config: PlzConfig = toml_edit::de::from_document(doc.clone())
        .with_context(|| "Failed to deserialize config")?;

    // Extract comments above [tasks.*] as descriptions (fallback when no explicit description)
    if let Some(tasks_table) = doc.get("tasks").and_then(|v| v.as_table()) {
        for (key, item) in tasks_table.iter() {
            if let Some(task) = config.tasks.get_mut(key)
                && task.description.is_none()
                && let Some(decor) = item.as_table().map(|t| t.decor())
                && let Some(prefix) = decor.prefix().and_then(|p| p.as_str())
            {
                task.description = extract_comment(prefix);
            }
        }
    }

    Ok(config)
}

pub fn extract_comment(prefix: &str) -> Option<String> {
    let lines: Vec<&str> = prefix
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                Some(trimmed.trim_start_matches('#').trim())
            } else {
                None
            }
        })
        .collect();

    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}
