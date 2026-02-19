use crate::config;
use crate::hooks;
use crate::settings;
pub use crate::templates::DEFAULTS;
use anyhow::{Result, bail};
use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub struct DefaultConfig {
    pub name: &'static str,
    pub doc: DocumentMut,
    pub tasks: Vec<(String, Option<String>)>,
}

fn extract_tasks(doc: &DocumentMut) -> Vec<(String, Option<String>)> {
    let Some(tasks_table) = doc.get("tasks").and_then(|t| t.as_table()) else {
        return Vec::new();
    };
    tasks_table
        .iter()
        .map(|(key, item)| {
            let desc = item
                .as_table()
                .and_then(|t| t.decor().prefix())
                .and_then(|p| p.as_str())
                .and_then(config::extract_comment);
            (key.to_string(), desc)
        })
        .collect()
}

pub fn parse_default(toml: &str) -> Option<(DocumentMut, Vec<(String, Option<String>)>)> {
    let doc: DocumentMut = toml.parse().ok()?;
    doc.get("tasks").and_then(|t| t.as_table())?;
    let tasks = extract_tasks(&doc);
    Some((doc, tasks))
}

fn config_dir() -> Option<PathBuf> {
    settings::config_dir()
}

pub fn merge_defaults(
    embedded: &str,
    user_toml: &str,
) -> Option<(DocumentMut, Vec<(String, Option<String>)>)> {
    let mut doc: DocumentMut = embedded.parse().ok()?;
    let user_doc: DocumentMut = user_toml.parse().ok()?;

    if let Some(user_tasks) = user_doc.get("tasks").and_then(|t| t.as_table()) {
        let tasks_table = doc.get_mut("tasks")?.as_table_mut()?;
        for (key, user_item) in user_tasks.iter() {
            let is_blank = user_item.as_table().is_some_and(|t| t.iter().count() == 0);

            if is_blank {
                tasks_table.remove(key);
            } else {
                tasks_table.insert(key, user_item.clone());
            }
        }
    }

    let tasks = extract_tasks(&doc);
    Some((doc, tasks))
}

pub fn generate_scaffold(content: &str) -> String {
    let mut out = String::new();
    out.push_str("# These defaults extend the built-ins.\n");
    out.push_str("# Uncomment to override. Leave blank to omit from the list.\n\n");

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !out.ends_with("\n\n") {
                out.push('\n');
            }
            continue;
        }
        if !trimmed.starts_with('#') {
            out.push_str("# ");
        }
        out.push_str(line);
        out.push('\n');
    }

    out
}

pub fn add_suffix_to_toml(
    toml: &str,
    suffix: &str,
    task_names: &[(String, Option<String>)],
) -> String {
    let mut result = toml.to_string();
    for (name, _) in task_names {
        result = result.replace(
            &format!("[tasks.{name}]"),
            &format!("[tasks.{name}-{suffix}]"),
        );
        result = result.replace(
            &format!("\"plz:{name}\""),
            &format!("\"plz:{name}-{suffix}\""),
        );
        result = result.replace(
            &format!("\"plz {name}\""),
            &format!("\"plz {name}-{suffix}\""),
        );
    }
    result
}

pub fn detect_project_types(cwd: &Path) -> Vec<DefaultConfig> {
    DEFAULTS
        .iter()
        .filter(|(_, detect, _)| cwd.join(detect).exists())
        .filter_map(|(name, _, embedded)| {
            let user_path = config_dir()?.join(format!("defaults/{name}.plz.toml"));
            let (doc, tasks) = if user_path.exists() {
                let user_toml = std::fs::read_to_string(&user_path).ok()?;
                merge_defaults(embedded, &user_toml)?
            } else {
                parse_default(embedded)?
            };
            Some(DefaultConfig { name, doc, tasks })
        })
        .collect()
}

pub fn run() -> Result<()> {
    let cwd = env::current_dir()?;
    let config_path = cwd.join("plz.toml");

    if config_path.exists() {
        bail!("plz.toml already exists in this directory");
    }

    let project_types = detect_project_types(&cwd);
    if project_types.is_empty() {
        bail!(
            "No supported project types detected (looked for pnpm-lock.yaml, uv.lock, Cargo.toml)"
        );
    }

    cliclack::intro("🙏 Initializing plz.toml 🙏")?;

    let detected: Vec<&str> = project_types.iter().map(|p| p.name).collect();
    cliclack::log::info(format!("Detected: {}", detected.join(", ")))?;

    let mut output = String::new();
    let needs_suffix = project_types.len() > 1;

    for pt in &project_types {
        let first_value: Vec<&str> = pt
            .tasks
            .first()
            .map(|(name, _)| name.as_str())
            .into_iter()
            .collect();
        let max_name_len = pt
            .tasks
            .iter()
            .map(|(name, _)| name.len())
            .max()
            .unwrap_or(0);

        let selected: Vec<&str> = cliclack::multiselect("Include default tasks?")
            .items(
                &pt.tasks
                    .iter()
                    .map(|(name, desc)| {
                        let desc = desc.as_deref().unwrap_or("");
                        let padding = " ".repeat(max_name_len - name.len() + 2);
                        (name.as_str(), format!("{name}{padding}{desc}"), "")
                    })
                    .collect::<Vec<_>>(),
            )
            .initial_values(first_value)
            .required(false)
            .interact()?;

        if selected.is_empty() {
            continue;
        }

        // Build output by removing unselected tasks from a clone of the document
        let mut doc = pt.doc.clone();
        if let Some(tasks_table) = doc.get_mut("tasks").and_then(|v| v.as_table_mut()) {
            let all_keys: Vec<String> = tasks_table.iter().map(|(k, _)| k.to_string()).collect();
            for key in all_keys {
                if !selected.contains(&key.as_str()) {
                    tasks_table.remove(&key);
                }
            }
        }

        let doc_str = if needs_suffix {
            let selected_tasks: Vec<(String, Option<String>)> = selected
                .iter()
                .map(|name| (name.to_string(), None))
                .collect();
            add_suffix_to_toml(&doc.to_string(), pt.name, &selected_tasks)
        } else {
            doc.to_string()
        };

        writeln!(output, "# {}", pt.name)?;
        write!(output, "{}", doc_str.trim())?;
        writeln!(output)?;
        writeln!(output)?;
    }

    if output.is_empty() {
        cliclack::outro("No tasks selected, skipping plz.toml creation")?;
        return Ok(());
    }

    // Offer git hook setup if in a git repo
    let in_git_repo = hooks::find_git_hooks_dir(&cwd).is_ok();
    if in_git_repo && let Ok(mut doc) = output.parse::<DocumentMut>() {
        let task_names: Vec<String> = doc
            .get("tasks")
            .and_then(|t| t.as_table())
            .map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default();

        if !task_names.is_empty() {
            let hook_stages: Vec<&str> = cliclack::multiselect("Add git hooks?")
                .items(&[
                    ("pre-commit", "pre-commit", "Run before each commit"),
                    ("pre-push", "pre-push", "Run before each push"),
                ])
                .required(false)
                .interact()?;

            // Track which tasks are already assigned to a stage so we can
            // duplicate them with a suffix when they appear in a second stage.
            let mut first_stage: HashMap<String, String> = HashMap::new();
            let mut changed = false;
            for stage in &hook_stages {
                let selected_hooks: Vec<&str> =
                    cliclack::multiselect(format!("Which tasks for {stage}?"))
                        .items(
                            &task_names
                                .iter()
                                .map(|n| (n.as_str(), n.as_str(), ""))
                                .collect::<Vec<_>>(),
                        )
                        .required(false)
                        .interact()?;

                if let Some(tasks_table) = doc.get_mut("tasks").and_then(|v| v.as_table_mut()) {
                    for task_name in &selected_hooks {
                        if let Some(prev_stage) = first_stage.get(*task_name) {
                            // Task already assigned to another stage — duplicate it
                            // with the previous stage as suffix, and keep the original
                            // name for this stage.
                            let suffixed = format!("{task_name}-{prev_stage}");
                            if let Some(original) = tasks_table.get(task_name).cloned() {
                                tasks_table.insert(&suffixed, original);
                            }
                            if let Some(task) = tasks_table
                                .get_mut(task_name)
                                .and_then(|v| v.as_table_mut())
                            {
                                task.insert("git_hook", toml_edit::value(stage.to_string()));
                            }
                            if let Some(dup) = tasks_table
                                .get_mut(&suffixed)
                                .and_then(|v| v.as_table_mut())
                            {
                                dup.insert("git_hook", toml_edit::value(prev_stage.to_string()));
                            }
                        } else {
                            if let Some(task) = tasks_table
                                .get_mut(task_name)
                                .and_then(|v| v.as_table_mut())
                            {
                                task.insert("git_hook", toml_edit::value(stage.to_string()));
                            }
                            first_stage.insert(task_name.to_string(), stage.to_string());
                        }
                        changed = true;
                    }
                }
            }

            if changed {
                output = doc.to_string();
            }
        }
    }

    std::fs::write(&config_path, output.trim_end())?;

    let defaults_dir = config_dir().map(|d| d.join("defaults"));
    if defaults_dir.as_ref().is_some_and(|d| d.exists()) {
        cliclack::log::info(format!(
            "💡 Edit defaults in: {}",
            defaults_dir.unwrap().display()
        ))?;
    } else {
        cliclack::log::info("💡 Customize defaults with `plz plz`")?;
    }

    cliclack::outro("Created plz.toml".to_string())?;

    // Install hooks if any were configured
    if in_git_repo
        && let Ok(ref cfg) = config::load(&config_path)
        && !hooks::tasks_by_stage(cfg).is_empty()
    {
        hooks::install(cfg, &cwd)?;
    }

    Ok(())
}

struct Template {
    name: &'static str,
    description: &'static str,
    content: &'static str,
}

const TEMPLATES: &[Template] = &[
    Template {
        name: "Basic task",
        description: "Simple single command",
        content: r#"[tasks.build]
run = "cargo build""#,
    },
    Template {
        name: "pnpm task",
        description: "Task using pnpm exec",
        content: r#"[tasks.dev]
env = "pnpm"
run = "vite""#,
    },
    Template {
        name: "uv task",
        description: "Task using uv run",
        content: r#"[tasks.test]
env = "uv"
run = "pytest""#,
    },
    Template {
        name: "Serial tasks",
        description: "Run commands one after another",
        content: r#"[tasks.lint]
run_serial = ["cargo clippy", "cargo fmt --check"]"#,
    },
    Template {
        name: "Parallel tasks",
        description: "Run commands at the same time",
        content: r#"[tasks.check]
run_parallel = ["plz:lint", "plz:test"]"#,
    },
    Template {
        name: "Fail hook: suggest",
        description: "Suggest a fix command on failure",
        content: r#"[tasks.lint]
run_serial = ["cargo clippy", "cargo fmt --check"]
fail_hook = { suggest_command = "plz fix" }"#,
    },
    Template {
        name: "Fail hook: message",
        description: "Show a message on failure",
        content: r#"[tasks.deploy]
run = "deploy.sh"
fail_hook = { message = "Check the deploy logs at /var/log/deploy.log" }"#,
    },
    Template {
        name: "Fail hook: command",
        description: "Run a command on failure",
        content: r#"[tasks.test]
run = "cargo test"
fail_hook = "notify-send 'Tests failed'"#,
    },
    Template {
        name: "Working directory",
        description: "Run a task in a subdirectory",
        content: r#"[tasks.frontend]
dir = "packages/web"
run = "pnpm dev""#,
    },
];

fn pick_template(footer_hint: &str) -> Result<Option<usize>> {
    use crate::utils::{PickItem, pick_from_list};
    let items: Vec<PickItem> = TEMPLATES
        .iter()
        .map(|t| PickItem {
            label: t.name.to_string(),
            description: t.description.to_string(),
            preview: Some(t.content.to_string()),
        })
        .collect();
    pick_from_list(&items, footer_hint)
}

pub fn help_templates() -> Result<()> {
    match pick_template("Enter to copy · Esc to cancel")? {
        Some(idx) => {
            let template = &TEMPLATES[idx];
            if copy_to_clipboard(template.content) {
                println!("\x1b[32m✓\x1b[0m  Copied to clipboard!");
            } else {
                println!("Copy the snippet above into your plz.toml");
            }
        }
        None => {
            println!("\x1b[2m✕  Cancelled\x1b[0m");
        }
    }

    Ok(())
}

fn rewrite_template(content: &str, task_name: &str) -> String {
    let mut result = String::new();
    for line in content.lines() {
        if !result.is_empty() {
            result.push('\n');
        }
        // Replace [tasks.xxx] and [env.xxx] headers with the new task name
        if line.starts_with("[tasks.") {
            result.push_str(&format!("[tasks.{task_name}]"));
        } else {
            result.push_str(line);
        }
    }
    result
}

pub fn add_task(name: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let config_path = cwd.join("plz.toml");
    let dotconfig_path = cwd.join(".plz.toml");

    let target_path = if config_path.exists() {
        config_path
    } else if dotconfig_path.exists() {
        dotconfig_path
    } else {
        bail!("No plz.toml found. Run `plz init` first.");
    };

    let task_name = match name {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => {
            let input: String = cliclack::input("Task name?")
                .placeholder("e.g. build, test, deploy")
                .interact()?;
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() {
                bail!("Task name cannot be empty");
            }
            trimmed
        }
    };

    match pick_template("Enter to add · Esc to cancel")? {
        Some(idx) => {
            let template = &TEMPLATES[idx];
            let snippet = rewrite_template(template.content, &task_name);

            let mut existing = std::fs::read_to_string(&target_path)?;
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push('\n');
            existing.push_str(&snippet);
            existing.push('\n');

            std::fs::write(&target_path, existing)?;
            println!(
                "\x1b[32m✓\x1b[0m  Added task \x1b[1m{task_name}\x1b[0m to {}",
                target_path.file_name().unwrap().to_string_lossy()
            );
        }
        None => {
            println!("\x1b[2m✕  Cancelled\x1b[0m");
        }
    }

    Ok(())
}

fn copy_to_clipboard(text: &str) -> bool {
    let cmd = if cfg!(target_os = "macos") {
        "pbcopy"
    } else if cfg!(target_os = "linux") {
        "xclip"
    } else {
        return false;
    };

    let mut args = vec![];
    if cmd == "xclip" {
        args.extend(["-selection", "clipboard"]);
    }

    let Ok(mut child) = std::process::Command::new(cmd)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };

    if let Some(ref mut stdin) = child.stdin
        && stdin.write_all(text.as_bytes()).is_err()
    {
        return false;
    }
    child.wait().is_ok_and(|s| s.success())
}

pub fn setup() -> Result<()> {
    let plz_dir =
        config_dir().ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    let defaults_dir = plz_dir.join("defaults");

    cliclack::intro("🙏 plz plz 🙏")?;

    if defaults_dir.exists() {
        cliclack::log::info(format!("Defaults directory: {}", defaults_dir.display()))?;
        let mut entries: Vec<_> = std::fs::read_dir(&defaults_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in &entries {
            cliclack::log::info(format!("  {}", entry.file_name().to_string_lossy()))?;
        }
        cliclack::outro("Edit the files above to customize the tasks offered by `plz init`.")?;
    } else {
        let should_create: bool = cliclack::confirm(format!(
            "Create defaults directory at {}?",
            defaults_dir.display()
        ))
        .interact()?;

        if !should_create {
            cliclack::outro("Skipped.")?;
        } else {
            std::fs::create_dir_all(&defaults_dir)?;

            for (name, _, content) in DEFAULTS {
                let path = defaults_dir.join(format!("{name}.plz.toml"));
                let scaffold = generate_scaffold(content);
                std::fs::write(&path, scaffold)?;
                cliclack::log::success(format!("Created {name}.plz.toml"))?;
            }

            cliclack::outro(format!(
                "Edit files in {} to customize the tasks offered by `plz init`.",
                defaults_dir.display()
            ))?;
        }
    }

    hooks::interactive_install_for_cwd()?;

    Ok(())
}
