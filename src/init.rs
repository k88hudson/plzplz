use crate::config;
use crate::hooks;
use crate::settings;
use crate::templates::{self, Snippet, TemplateMeta};
use anyhow::{Result, bail};
use std::env;
use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use toml_edit::DocumentMut;

fn config_dir() -> Option<PathBuf> {
    settings::config_dir()
}

pub fn extract_tasks(doc: &DocumentMut) -> Vec<(String, Option<String>)> {
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

pub fn run() -> Result<()> {
    let cwd = env::current_dir()?;
    let config_path = cwd.join("plz.toml");

    if config_path.exists() {
        // If plz.toml already exists, check for git hooks to install
        let in_git_repo = hooks::find_git_hooks_dir(&cwd).is_ok();
        let content = std::fs::read_to_string(&config_path)?;
        let has_git_hooks = content.contains("git_hook");

        if in_git_repo && has_git_hooks {
            cliclack::intro("plz init")?;
            let install_hooks: bool = cliclack::confirm("Install git hooks?")
                .initial_value(true)
                .interact()?;
            if install_hooks {
                if let Ok(ref cfg) = config::load(&config_path) {
                    hooks::install(cfg, &cwd)?;
                    cliclack::outro("Installed git hooks")?;
                }
            } else {
                cliclack::outro("Skipped git hook installation")?;
            }
        } else {
            cliclack::log::info(
                "plz.toml already exists. Run \x1b[1mplz\x1b[0m to see all commands.",
            )?;
        }
        return Ok(());
    }

    let cfg_dir = config_dir();
    let interactive = std::io::stdin().is_terminal() && !is_ci::cached();

    // Load environments and detect which ones match
    let environments = templates::load_environments();
    let detected = templates::detect_environments(&cwd, &environments);

    // Determine which envs are "alternatives" (detected but superseded)
    let alternative_envs: Vec<String> = detected
        .iter()
        .flat_map(|d| environments[d].alternative_to.clone())
        .collect();

    // Load all templates
    let all_templates = templates::load_templates(cfg_dir.as_deref());

    if !interactive {
        let output = "[tasks.hello]\nrun = \"echo 'hello world'\"";
        std::fs::write(&config_path, output)?;
        eprintln!("Created plz.toml with a starter task");
        return Ok(());
    }

    cliclack::intro("plz init")?;

    if !detected.is_empty() {
        cliclack::log::info(format!("Detected: {}", detected.join(", ")))?;
    }

    // Sort templates: detected envs first, then alternatives, then others
    let mut sorted_templates: Vec<&TemplateMeta> = Vec::new();
    // Detected first
    for t in &all_templates {
        if detected.contains(&t.env) {
            sorted_templates.push(t);
        }
    }
    // Alternatives (detected but superseded)
    for t in &all_templates {
        if alternative_envs.contains(&t.env) && !detected.contains(&t.env) {
            sorted_templates.push(t);
        }
    }
    // Everything else
    for t in &all_templates {
        if !detected.contains(&t.env) && !alternative_envs.contains(&t.env) {
            sorted_templates.push(t);
        }
    }

    if sorted_templates.is_empty() {
        let output = "[tasks.hello]\nrun = \"echo 'hello world'\"";
        std::fs::write(&config_path, output)?;
        cliclack::outro("Created plz.toml with a starter task")?;
        return Ok(());
    }

    // Build multiselect items
    let max_name_len = sorted_templates
        .iter()
        .map(|t| t.name.len())
        .max()
        .unwrap_or(0);

    let items: Vec<(&str, String, &str)> = sorted_templates
        .iter()
        .map(|t| {
            let padding = " ".repeat(max_name_len - t.name.len() + 2);
            (
                t.name.as_str(),
                format!("{}{padding}{}", t.name, t.description),
                "",
            )
        })
        .collect();

    // Pre-select templates whose env is detected AND not an alternative
    let initial: Vec<&str> = sorted_templates
        .iter()
        .filter(|t| detected.contains(&t.env) && !alternative_envs.contains(&t.env))
        .map(|t| t.name.as_str())
        .collect();

    let selected: Vec<&str> = cliclack::multiselect("Which templates?")
        .items(&items)
        .initial_values(initial)
        .required(false)
        .interact()?;

    if selected.is_empty() {
        cliclack::outro("No templates selected, skipping plz.toml creation")?;
        return Ok(());
    }

    // Build output from selected templates
    let mut output = String::new();
    let needs_suffix = selected.len() > 1;

    for template_name in &selected {
        let template = sorted_templates
            .iter()
            .find(|t| t.name.as_str() == *template_name)
            .unwrap();

        let content = templates::strip_template_section(&template.content);

        if needs_suffix {
            if let Some((_, tasks)) = parse_default(&content) {
                let suffixed = add_suffix_to_toml(&content, template_name, &tasks);
                writeln!(output, "# {}", template.name)?;
                write!(output, "{}", suffixed.trim())?;
            } else {
                writeln!(output, "# {}", template.name)?;
                write!(output, "{}", content.trim())?;
            }
        } else {
            write!(output, "{}", content.trim())?;
        }
        writeln!(output)?;
        writeln!(output)?;
    }

    if output.is_empty() {
        cliclack::outro("No tasks selected, skipping plz.toml creation")?;
        return Ok(());
    }

    // Check if any selected template has git_hook tasks
    let in_git_repo = hooks::find_git_hooks_dir(&cwd).is_ok();
    let has_git_hooks = output.contains("git_hook");

    if in_git_repo && has_git_hooks {
        let install_hooks: bool = cliclack::confirm("Install git hooks?")
            .initial_value(true)
            .interact()?;

        std::fs::write(&config_path, output.trim_end())?;

        if install_hooks && let Ok(ref cfg) = config::load(&config_path) {
            hooks::install(cfg, &cwd)?;
        }
    } else {
        std::fs::write(&config_path, output.trim_end())?;
    }

    cliclack::outro("Created plz.toml".to_string())?;

    Ok(())
}

fn pick_snippet(
    all_snippets: &[(String, Vec<Snippet>)],
    detected_envs: &[String],
    footer_hint: &str,
) -> Result<Option<Snippet>> {
    use crate::utils::{PickItem, pick_from_list};

    // Build flat list: detected env snippets first, then general, then others
    let mut items: Vec<(PickItem, Snippet)> = Vec::new();

    // Detected env snippets first
    for (env_name, snippets) in all_snippets {
        if detected_envs.contains(env_name) {
            for s in snippets {
                items.push((
                    PickItem {
                        label: s.name.clone(),
                        description: s.description.clone(),
                        preview: Some(s.content.trim().to_string()),
                    },
                    s.clone(),
                ));
            }
        }
    }

    // General snippets
    for (env_name, snippets) in all_snippets {
        if env_name == "general" && !detected_envs.contains(env_name) {
            for s in snippets {
                items.push((
                    PickItem {
                        label: s.name.clone(),
                        description: s.description.clone(),
                        preview: Some(s.content.trim().to_string()),
                    },
                    s.clone(),
                ));
            }
        }
    }

    // Other env snippets
    for (env_name, snippets) in all_snippets {
        if !detected_envs.contains(env_name) && env_name != "general" {
            for s in snippets {
                items.push((
                    PickItem {
                        label: format!("{} ({})", s.name, env_name),
                        description: s.description.clone(),
                        preview: Some(s.content.trim().to_string()),
                    },
                    s.clone(),
                ));
            }
        }
    }

    let pick_items: Vec<PickItem> = items.iter().map(|(pi, _)| pi.clone()).collect();
    match pick_from_list(&pick_items, footer_hint)? {
        Some(idx) => Ok(Some(items[idx].1.clone())),
        None => Ok(None),
    }
}

pub fn help_templates() -> Result<()> {
    let cwd = env::current_dir()?;
    let environments = templates::load_environments();
    let detected = templates::detect_environments(&cwd, &environments);
    let all_snippets = templates::load_snippets();

    match pick_snippet(&all_snippets, &detected, "Enter to copy · Esc to cancel")? {
        Some(snippet) => {
            if copy_to_clipboard(snippet.content.trim()) {
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

    let environments = templates::load_environments();
    let detected = templates::detect_environments(&cwd, &environments);
    let all_snippets = templates::load_snippets();

    match pick_snippet(&all_snippets, &detected, "Enter to add · Esc to cancel")? {
        Some(snippet) => {
            let content = rewrite_template(snippet.content.trim(), &task_name);

            let mut existing = std::fs::read_to_string(&target_path)?;
            if !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push('\n');
            existing.push_str(&content);
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

fn check_dir_writable(dir: &std::path::Path) -> bool {
    let target = if dir.is_dir() {
        dir.to_path_buf()
    } else if let Some(parent) = dir.parent() {
        parent.to_path_buf()
    } else {
        return false;
    };
    if !target.exists() {
        return true;
    }
    let probe = target.join(".plz_write_test");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

pub fn setup() -> Result<()> {
    let plz_dir =
        config_dir().ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    let templates_dir = plz_dir.join("templates");
    let user_template_path = plz_dir.join("user.plz.toml");

    cliclack::intro("plz setup")?;

    let perm_hint = format!(
        "Try: chmod u+w {} or set PLZ_CONFIG_DIR to a writable path",
        plz_dir.display()
    );

    // Check write permissions early
    if plz_dir.exists() && !check_dir_writable(&plz_dir) {
        cliclack::outro(format!(
            "{} is not writable. {perm_hint}",
            plz_dir.display()
        ))?;
        return Ok(());
    }
    if !plz_dir.exists() && !check_dir_writable(&plz_dir) {
        cliclack::outro(format!(
            "Cannot create {} (parent directory is not writable). {perm_hint}",
            plz_dir.display(),
        ))?;
        return Ok(());
    }

    if !plz_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&plz_dir) {
            cliclack::outro(format!("Could not create {}: {e}", plz_dir.display()))?;
            return Ok(());
        }
        cliclack::log::step(format!("Created {}", plz_dir.display()))?;
    } else {
        cliclack::log::step(format!("{} already exists", plz_dir.display()))?;
    }

    if !user_template_path.exists() {
        let content = r#"[template]
description = "Custom tasks"
env = "uv"

[tasks.hello]
run = "echo hello"
"#;
        match std::fs::write(&user_template_path, content) {
            Ok(()) => {
                cliclack::log::step(format!(
                    "Added user template: {}",
                    user_template_path.display()
                ))?;
            }
            Err(e) => {
                cliclack::log::warning(format!("Could not create user template: {e}"))?;
            }
        }
    } else if std::fs::OpenOptions::new()
        .write(true)
        .open(&user_template_path)
        .is_err()
    {
        cliclack::log::warning(format!(
            "User template is not writable: {}. {perm_hint}",
            user_template_path.display(),
        ))?;
    } else {
        cliclack::log::step(format!(
            "User template already exists: {}",
            user_template_path.display()
        ))?;
    }

    if !templates_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&templates_dir) {
            cliclack::log::warning(format!("Could not create {}: {e}", templates_dir.display()))?;
        }
    }

    cliclack::outro(format!(
        "Add files to {} to customize templates for plz init",
        templates_dir.display()
    ))?;

    Ok(())
}
