use anyhow::{Context, Result, bail};
use schemars::{JsonSchema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct PlzSection {
    /// Semver version requirement for plz (e.g. ">=0.1.0", "^0.2")
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlzConfig {
    /// plz tool settings (e.g. required version)
    #[serde(default)]
    pub plz: Option<PlzSection>,
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

#[derive(Debug, Clone)]
pub struct StringOrVec(pub Vec<String>);

impl<'de> Deserialize<'de> for StringOrVec {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct StringOrVecVisitor;

        impl<'de> Visitor<'de> for StringOrVecVisitor {
            type Value = StringOrVec;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string or array of strings")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<StringOrVec, E> {
                Ok(StringOrVec(vec![v.to_string()]))
            }

            fn visit_seq<S: SeqAccess<'de>>(
                self,
                mut seq: S,
            ) -> std::result::Result<StringOrVec, S::Error> {
                let mut vec = Vec::new();
                while let Some(item) = seq.next_element::<String>()? {
                    vec.push(item);
                }
                Ok(StringOrVec(vec))
            }
        }

        deserializer.deserialize_any(StringOrVecVisitor)
    }
}

impl JsonSchema for StringOrVec {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("StringOrVec")
    }

    fn json_schema(_: &mut SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "oneOf": [
                { "type": "string" },
                { "type": "array", "items": { "type": "string" } }
            ]
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Task {
    /// A shell command (or list of commands to run serially) to run
    #[serde(default)]
    pub run: Option<StringOrVec>,
    /// Multiple commands to run one after another (stops on first failure)
    #[serde(default)]
    pub run_serial: Option<Vec<String>>,
    /// Multiple commands to run concurrently
    #[serde(default)]
    pub run_parallel: Option<Vec<String>>,
    /// Prerequisite tasks to run before this task. Use dot notation for group tasks (e.g. "group.task").
    #[serde(default)]
    pub depends: Option<StringOrVec>,
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
    /// Hide this task from interactive pickers and listings
    #[serde(default)]
    pub hide: bool,
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

    if config.tasks.contains_key("plz") {
        bail!("\"plz\" is a reserved name and cannot be used as a task name.");
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

    // Validate depends references exist
    validate_depends(&config)?;

    // Detect circular dependencies
    detect_cycles(&config)?;

    Ok(config)
}

fn resolve_dep_ref(config: &PlzConfig, dep: &str) -> bool {
    if let Some((group, task)) = dep.split_once('.') {
        config.get_group_task(group, task).is_some()
    } else {
        config.tasks.contains_key(dep)
    }
}

fn validate_depends(config: &PlzConfig) -> Result<()> {
    for (name, task) in &config.tasks {
        if let Some(ref deps) = task.depends {
            for dep in &deps.0 {
                if !resolve_dep_ref(config, dep) {
                    bail!("Task \"{name}\" has depends \"{dep}\", but no task \"{dep}\" exists");
                }
            }
        }
    }
    if let Some(ref groups) = config.taskgroup {
        for (group_name, group) in groups {
            for (task_name, task) in &group.tasks {
                if let Some(ref deps) = task.depends {
                    for dep in &deps.0 {
                        if !resolve_dep_ref(config, dep) {
                            bail!(
                                "Task \"{group_name}.{task_name}\" has depends \"{dep}\", but no task \"{dep}\" exists"
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn detect_cycles(config: &PlzConfig) -> Result<()> {
    // Build adjacency list: node_id -> [dep_ids]
    // node_id for top-level: task_name, for group: "group.task"
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

    for (name, task) in &config.tasks {
        if let Some(ref deps) = task.depends {
            adj.insert(name.clone(), deps.0.clone());
        }
    }
    if let Some(ref groups) = config.taskgroup {
        for (group_name, group) in groups {
            for (task_name, task) in &group.tasks {
                if let Some(ref deps) = task.depends {
                    let key = format!("{group_name}.{task_name}");
                    adj.insert(key, deps.0.clone());
                }
            }
        }
    }

    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();
    let mut path = Vec::new();

    for node in adj.keys() {
        if !visited.contains(node) {
            dfs_cycle(node, &adj, &mut visited, &mut in_stack, &mut path)?;
        }
    }

    Ok(())
}

fn dfs_cycle(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Result<()> {
    visited.insert(node.to_string());
    in_stack.insert(node.to_string());
    path.push(node.to_string());

    if let Some(deps) = adj.get(node) {
        for dep in deps {
            if in_stack.contains(dep) {
                path.push(dep.clone());
                let cycle_start = path.iter().position(|n| n == dep).unwrap();
                let cycle = path[cycle_start..].join(" → ");
                bail!("Circular dependency: {cycle}");
            }
            if !visited.contains(dep) {
                dfs_cycle(dep, adj, visited, in_stack, path)?;
            }
        }
    }

    path.pop();
    in_stack.remove(node);
    Ok(())
}

impl PlzConfig {
    pub fn get_group(&self, name: &str) -> Option<&TaskGroup> {
        self.taskgroup.as_ref()?.get(name)
    }

    pub fn get_group_task(&self, group: &str, task: &str) -> Option<&Task> {
        self.get_group(group)?.tasks.get(task)
    }

    pub fn check_version(&self) {
        let req_str = match self.plz.as_ref().and_then(|p| p.version.as_deref()) {
            Some(v) => v,
            None => return,
        };
        let current = env!("CARGO_PKG_VERSION");
        let Ok(version) = semver::Version::parse(current) else {
            return;
        };
        let Ok(req) = semver::VersionReq::parse(req_str) else {
            eprintln!(
                "\x1b[33mwarning:\x1b[0m [plz] version \"{req_str}\" is not a valid semver requirement"
            );
            return;
        };
        if !req.matches(&version) {
            eprintln!(
                "\x1b[33mwarning:\x1b[0m plz {current} does not match version requirement \"{req_str}\" in plz.toml. Run `plz update` to update."
            );
        }
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
