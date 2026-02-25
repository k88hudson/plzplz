use anyhow::{Context, Result, bail};
use schemars::{JsonSchema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use toml_edit::DocumentMut;

pub const VALID_GIT_HOOKS: &[&str] = &[
    "applypatch-msg",
    "pre-applypatch",
    "post-applypatch",
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
    "pre-rebase",
    "post-checkout",
    "post-merge",
    "pre-push",
    "pre-receive",
    "update",
    "post-receive",
    "post-update",
    "push-to-checkout",
    "pre-auto-gc",
    "post-rewrite",
    "sendemail-validate",
];

#[derive(Debug, Default, Clone, Deserialize, JsonSchema)]
pub struct GlobalSettings {
    /// Tool environment wrapper applied to all tasks: "pnpm", "npm", "uv", or "uvx"
    #[serde(default, rename = "env")]
    #[schemars(rename = "env")]
    pub tool_env: Option<String>,
    /// Default working directory (relative to plz.toml) for all tasks
    #[serde(default)]
    pub dir: Option<String>,
}

#[derive(Debug)]
pub struct TaskGroup {
    pub extends: Option<GlobalSettings>,
    pub tasks: HashMap<String, Task>,
}

impl<'de> Deserialize<'de> for TaskGroup {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TaskGroupVisitor;

        impl<'de> Visitor<'de> for TaskGroupVisitor {
            type Value = TaskGroup;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a task group table with optional 'extends' and task entries")
            }

            fn visit_map<M>(self, mut map: M) -> std::result::Result<TaskGroup, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let mut extends = None;
                let mut tasks = HashMap::new();

                while let Some(key) = map.next_key::<String>()? {
                    if key == "extends" {
                        extends = Some(map.next_value::<GlobalSettings>()?);
                    } else {
                        tasks.insert(key, map.next_value::<Task>()?);
                    }
                }

                Ok(TaskGroup { extends, tasks })
            }
        }

        deserializer.deserialize_map(TaskGroupVisitor)
    }
}

impl JsonSchema for TaskGroup {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("TaskGroup")
    }

    fn json_schema(generator: &mut SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "type": "object",
            "description": "A group of related tasks with optional shared defaults",
            "properties": {
                "extends": generator.subschema_for::<GlobalSettings>()
            },
            "additionalProperties": generator.subschema_for::<Task>()
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlzConfig {
    /// Global defaults that apply to all tasks (can be overridden per-task)
    #[serde(default)]
    pub extends: Option<GlobalSettings>,
    /// Task groups for namespacing related tasks (e.g. [taskgroup.rust.test])
    #[serde(default)]
    pub taskgroup: Option<HashMap<String, TaskGroup>>,
    /// Tasks to run, keyed by name (e.g. [tasks.build]). Run with `plz <name>`.
    #[serde(default)]
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
    /// Tool environment wrapper: "pnpm" (uses `pnpm exec`), "npm" (uses `npx`), "uv" (uses `uv run`), or "uvx" (uses `uvx`)
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
    /// Git hook stage to associate this task with (e.g. "pre-commit", "pre-push")
    #[serde(default)]
    pub git_hook: Option<String>,
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

    // Apply global defaults from [extends] to tasks.
    // Empty string means "explicitly no value" (opt out of extends).
    if let Some(ref extends) = config.extends {
        for task in config.tasks.values_mut() {
            if task.tool_env.is_none() {
                task.tool_env.clone_from(&extends.tool_env);
            }
            if task.dir.is_none() {
                task.dir.clone_from(&extends.dir);
            }
        }
    }
    for task in config.tasks.values_mut() {
        if task.tool_env.as_deref() == Some("") {
            task.tool_env = None;
        }
        if task.dir.as_deref() == Some("") {
            task.dir = None;
        }
    }

    // Validate git_hook values
    for (name, task) in &config.tasks {
        if let Some(ref hook) = task.git_hook {
            if !VALID_GIT_HOOKS.contains(&hook.as_str()) {
                bail!(
                    "Task \"{name}\" has invalid git_hook \"{hook}\". Valid hooks: {}",
                    VALID_GIT_HOOKS.join(", ")
                );
            }
        }
    }

    // Apply extends cascade to taskgroup tasks:
    // top-level [extends] → [taskgroup.X.extends] → per-task values
    if let Some(ref mut groups) = config.taskgroup {
        for (group_name, group) in groups.iter_mut() {
            // Compute effective extends: top-level, then group overrides
            let effective_env = group
                .extends
                .as_ref()
                .and_then(|e| e.tool_env.clone())
                .or_else(|| config.extends.as_ref().and_then(|e| e.tool_env.clone()));
            let effective_dir = group
                .extends
                .as_ref()
                .and_then(|e| e.dir.clone())
                .or_else(|| config.extends.as_ref().and_then(|e| e.dir.clone()));

            for task in group.tasks.values_mut() {
                if task.tool_env.is_none() {
                    task.tool_env.clone_from(&effective_env);
                }
                if task.dir.is_none() {
                    task.dir.clone_from(&effective_dir);
                }
            }

            // Clear empty-string opt-outs
            for task in group.tasks.values_mut() {
                if task.tool_env.as_deref() == Some("") {
                    task.tool_env = None;
                }
                if task.dir.as_deref() == Some("") {
                    task.dir = None;
                }
            }

            // Validate git_hook values in group tasks
            for (task_name, task) in &group.tasks {
                if let Some(ref hook) = task.git_hook {
                    if !VALID_GIT_HOOKS.contains(&hook.as_str()) {
                        bail!(
                            "Task \"{group_name}:{task_name}\" has invalid git_hook \"{hook}\". Valid hooks: {}",
                            VALID_GIT_HOOKS.join(", ")
                        );
                    }
                }
            }

            // Extract comments from taskgroup tables
            if let Some(group_table) = doc
                .get("taskgroup")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get(group_name.as_str()))
                .and_then(|v| v.as_table())
            {
                for (key, item) in group_table.iter() {
                    if key == "extends" {
                        continue;
                    }
                    if let Some(task) = group.tasks.get_mut(key)
                        && task.description.is_none()
                        && let Some(decor) = item.as_table().map(|t| t.decor())
                        && let Some(prefix) = decor.prefix().and_then(|p| p.as_str())
                    {
                        task.description = extract_comment(prefix);
                    }
                }
            }
        }
    }

    Ok(config)
}

impl PlzConfig {
    pub fn get_group(&self, name: &str) -> Option<&TaskGroup> {
        self.taskgroup.as_ref()?.get(name)
    }

    pub fn get_group_task(&self, group: &str, task: &str) -> Option<&Task> {
        self.get_group(group)?.tasks.get(task)
    }
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
