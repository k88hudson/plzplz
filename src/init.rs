use crate::config;
use crate::hooks;
use crate::settings;
use crate::templates::{self, Snippet, TemplateMeta};
use anyhow::{Result, bail};
use std::env;
use std::fmt::Write as _;
use std::io::IsTerminal;
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

#[allow(dead_code)]
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

pub fn convert_to_taskgroup(
    content: &str,
    group_name: &str,
    task_names: &[(String, Option<String>)],
    template_env: &str,
) -> String {
    let mut result = content.to_string();

    // Replace [tasks.X] with [taskgroup.GROUP.X]
    for (name, _) in task_names {
        result = result.replace(
            &format!("[tasks.{name}]"),
            &format!("[taskgroup.{group_name}.{name}]"),
        );
    }

    // Update plz references: plz:task → plz:group:task, plz task → plz group task
    for (name, _) in task_names {
        result = result.replace(
            &format!("\"plz:{name}\""),
            &format!("\"plz:{group_name}:{name}\""),
        );
        result = result.replace(
            &format!("\"plz {name}\""),
            &format!("\"plz {group_name} {name}\""),
        );
    }

    // Extract common env into [taskgroup.GROUP.extends] if tasks use a shared env
    let env_line = format!("env = \"{template_env}\"");
    if result.lines().any(|l| l.trim() == env_line) {
        // Remove per-task env lines that match the template env
        let lines: Vec<&str> = result.lines().collect();
        let filtered: Vec<&str> = lines.into_iter().filter(|l| l.trim() != env_line).collect();
        result = filtered.join("\n");

        // Prepend extends section
        result = format!(
            "[taskgroup.{group_name}.extends]\n{env_line}\n\n{}",
            result.trim_start()
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

    // Sort templates: detected envs first (user templates before embedded), then alternatives, then others
    let mut sorted_templates: Vec<&TemplateMeta> = Vec::new();
    let env_detected = |t: &TemplateMeta| t.env.as_ref().is_some_and(|e| detected.contains(e));
    let env_alternative =
        |t: &TemplateMeta| t.env.as_ref().is_some_and(|e| alternative_envs.contains(e));
    // Detected first — user templates before embedded for the same env
    for t in &all_templates {
        if env_detected(t) && t.is_user {
            sorted_templates.push(t);
        }
    }
    for t in &all_templates {
        if env_detected(t) && !t.is_user {
            sorted_templates.push(t);
        }
    }
    // Alternatives (detected but superseded)
    for t in &all_templates {
        if env_alternative(t) && !env_detected(t) {
            sorted_templates.push(t);
        }
    }
    // Everything else
    for t in &all_templates {
        if !env_detected(t) && !env_alternative(t) {
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

    // Pre-select: prefer detected env template, else fall back to env-agnostic templates
    let initial: Vec<&str> = sorted_templates
        .iter()
        .find(|t| env_detected(t) && !env_alternative(t))
        .or_else(|| sorted_templates.iter().find(|t| t.env.is_none()))
        .map(|t| vec![t.name.as_str()])
        .unwrap_or_default();

    let mut ms_items: Vec<crate::utils::MultiSelectItem> = sorted_templates
        .iter()
        .map(|t| {
            let padding = " ".repeat(max_name_len - t.name.len() + 2);
            crate::utils::MultiSelectItem {
                label: format!("{}{padding}{}", t.name, t.description),
                hint: String::new(),
                selected: initial.contains(&t.name.as_str()),
            }
        })
        .collect();

    let selected: Vec<&str> =
        match crate::utils::multiselect("Which templates?", &mut ms_items, false)? {
            Some(indices) => indices
                .iter()
                .map(|&i| sorted_templates[i].name.as_str())
                .collect(),
            None => {
                print_templates_hint(&cfg_dir);
                return Ok(());
            }
        };

    let selected = if selected.is_empty() {
        vec!["general"]
    } else {
        selected
    };

    // Build output from selected templates
    let mut output = String::new();
    let use_taskgroups = selected.len() > 1;

    for template_name in &selected {
        let template = sorted_templates
            .iter()
            .find(|t| t.name.as_str() == *template_name)
            .unwrap();

        let content = templates::strip_template_section(&template.content);

        if use_taskgroups {
            if let Some((_, tasks)) = parse_default(&content) {
                let grouped = convert_to_taskgroup(
                    &content,
                    template_name,
                    &tasks,
                    template.env.as_deref().unwrap_or(""),
                );
                write!(output, "{}", grouped.trim())?;
            } else {
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
    print_templates_hint(&cfg_dir);

    Ok(())
}

fn print_templates_hint(cfg_dir: &Option<PathBuf>) {
    if !settings::config_dir_exists() {
        eprintln!("\x1b[2mRun `plz plz` to set up custom settings and templates.\x1b[0m");
    } else if let Some(dir) = cfg_dir {
        eprintln!(
            "\x1b[2mAdd templates to {} to customize\x1b[0m",
            dir.join("templates").display()
        );
    }
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

pub fn print_cheatsheet() -> Result<()> {
    let bold = "\x1b[1m";
    let dim = "\x1b[2m";
    let cyan = "\x1b[36m";
    let reset = "\x1b[0m";

    let mut out = String::new();

    out.push_str(&format!("{bold}plz.toml cheatsheet{reset}\n\n"));

    out.push_str(&format!("{cyan}Basic task{reset}\n"));
    out.push_str("[tasks.build]\n");
    out.push_str("run = \"cargo build\"\n\n");

    out.push_str(&format!("{cyan}Description (comment){reset}\n"));
    out.push_str(&format!("{dim}# Build the project{reset}\n"));
    out.push_str("[tasks.build]\n");
    out.push_str("run = \"cargo build\"\n\n");

    out.push_str(&format!("{cyan}Description (explicit){reset}\n"));
    out.push_str("[tasks.build]\n");
    out.push_str("run = \"cargo build\"\n");
    out.push_str("description = \"Build the project\"\n\n");

    out.push_str(&format!("{cyan}Serial execution{reset}\n"));
    out.push_str("[tasks.fix]\n");
    out.push_str("run_serial = [\"cargo fmt\", \"cargo clippy --fix --allow-dirty\"]\n\n");

    out.push_str(&format!("{cyan}Parallel execution{reset}\n"));
    out.push_str("[tasks.check]\n");
    out.push_str("run_parallel = [\"plz lint\", \"plz format\"]\n\n");

    out.push_str(&format!("{cyan}Task references{reset}\n"));
    out.push_str("[tasks.check]\n");
    out.push_str("run_parallel = [\"plz:lint\", \"plz:format\"]\n\n");

    out.push_str(&format!("{cyan}Working directory{reset}\n"));
    out.push_str("[tasks.frontend]\n");
    out.push_str("dir = \"packages/web\"\n");
    out.push_str("run = \"pnpm dev\"\n\n");

    out.push_str(&format!(
        "{cyan}Environment wrappers{reset}  {dim}pnpm | npm | uv | uvx{reset}\n"
    ));
    out.push_str("[tasks.vitest]\n");
    out.push_str("run = \"vitest\"\n");
    out.push_str("tool_env = \"pnpm\"\n\n");

    out.push_str(&format!("{cyan}Failure hooks{reset}\n"));
    out.push_str(&format!("{dim}# suggest a fix command{reset}\n"));
    out.push_str("fail_hook = { suggest_command = \"cargo fmt\" }\n");
    out.push_str(&format!("{dim}# show a message{reset}\n"));
    out.push_str("fail_hook = { message = \"Check the logs\" }\n");
    out.push_str(&format!("{dim}# run a command{reset}\n"));
    out.push_str("fail_hook = \"notify-send 'Tests failed'\"\n\n");

    out.push_str(&format!(
        "{cyan}Git hooks{reset}  {dim}pre-commit | pre-push | commit-msg | post-commit | post-merge | post-checkout{reset}\n"
    ));
    out.push_str("[tasks.check]\n");
    out.push_str("run_parallel = [\"plz:lint\", \"plz:format\"]\n");
    out.push_str("git_hook = \"pre-commit\"\n\n");

    out.push_str(&format!("{cyan}Extends (global defaults){reset}\n"));
    out.push_str("[extends]\n");
    out.push_str("env = { NODE_ENV = \"production\" }\n");
    out.push_str("dir = \"packages/app\"\n\n");

    out.push_str(&format!("{cyan}Task groups{reset}\n"));
    out.push_str("[taskgroup.docs.build]\n");
    out.push_str("run = \"pnpm docs:build\"\n\n");
    out.push_str("[taskgroup.docs.dev]\n");
    out.push_str("run = \"pnpm docs:dev\"\n");

    print!("{out}");
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
    let user_template_path = templates_dir.join("user.plz.toml");
    let settings_path = plz_dir.join("settings.toml");

    let perm_hint = format!(
        "Try: chmod u+w {} or set PLZ_CONFIG_DIR to a writable path",
        plz_dir.display()
    );

    // Check write permissions early
    if plz_dir.exists() && !check_dir_writable(&plz_dir) {
        cliclack::intro("plz setup")?;
        cliclack::outro(format!(
            "{} is not writable. {perm_hint}",
            plz_dir.display()
        ))?;
        return Ok(());
    }

    // If .plz already exists, show settings editor
    if plz_dir.exists() {
        return setup_settings_editor(&settings_path);
    }

    if !check_dir_writable(&plz_dir) {
        cliclack::log::error(format!(
            "Cannot create {} (parent directory is not writable). {perm_hint}",
            plz_dir.display(),
        ))?;
        return Ok(());
    }

    if let Err(e) = std::fs::create_dir_all(&templates_dir) {
        cliclack::log::error(format!("Could not create {}: {e}", templates_dir.display()))?;
        return Ok(());
    }

    if !user_template_path.exists() {
        let content = r#"# Uncomment and edit to create a custom template for plz init
# [template]
# description = "Custom tasks"
# env = "uv"
#
# [tasks.example]
# run = "echo hello"
"#;
        let _ = std::fs::write(&user_template_path, content);
    }

    if !settings_path.exists() {
        let content = "# show_hints = true\n";
        let _ = std::fs::write(&settings_path, content);
    }

    eprintln!("🙏 Created {}", plz_dir.display());
    eprintln!(
        "\x1b[2m   Add templates to {} to customize plz init\x1b[0m",
        templates_dir.display()
    );

    Ok(())
}

fn setup_settings_editor(settings_path: &std::path::Path) -> Result<()> {
    let raw = settings::load_raw(settings_path);

    cliclack::intro("plz settings")?;

    // Display current settings with color coding
    for (key, value, is_user_set) in &raw {
        let entry = settings::ALL_SETTINGS
            .iter()
            .find(|e| e.key == *key)
            .unwrap();
        if *is_user_set {
            // Green for user-set values
            cliclack::log::step(format!(
                "\x1b[32m{key} = {value}\x1b[0m  {}",
                entry.description
            ))?;
        } else {
            // Grey for default values
            cliclack::log::step(format!(
                "\x1b[2m{key} = {value}\x1b[0m  {}",
                entry.description
            ))?;
        }
    }

    // Build multiselect with current values
    let currently_enabled: Vec<&str> = raw
        .iter()
        .filter(|(_, value, _)| *value)
        .map(|(key, _, _)| *key)
        .collect();

    let mut ms_items: Vec<crate::utils::MultiSelectItem> = settings::ALL_SETTINGS
        .iter()
        .map(|entry| crate::utils::MultiSelectItem {
            label: format!("{} — {}", entry.key, entry.description),
            hint: String::new(),
            selected: currently_enabled.contains(&entry.key),
        })
        .collect();

    let selected: Vec<&str> =
        match crate::utils::multiselect("Toggle settings", &mut ms_items, false)? {
            Some(indices) => indices
                .iter()
                .map(|&i| settings::ALL_SETTINGS[i].key)
                .collect(),
            None => return Ok(()),
        };

    let values: Vec<(&str, bool)> = settings::ALL_SETTINGS
        .iter()
        .map(|entry| (entry.key, selected.contains(&entry.key)))
        .collect();

    // Only save if something changed
    let changed = raw
        .iter()
        .zip(values.iter())
        .any(|((_, old_val, _), (_, new_val))| old_val != new_val);

    if changed {
        settings::save(settings_path, &values)?;
        cliclack::outro("Settings saved")?;
    } else {
        cliclack::outro("No changes")?;
    }

    Ok(())
}

pub fn self_update() -> Result<()> {
    use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

    let current = env!("CARGO_PKG_VERSION");
    cliclack::intro(format!("plz update (current: v{current})"))?;

    let spinner = cliclack::spinner();
    spinner.start("Checking for updates...");

    let current_exe = std::env::current_exe()?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Can't determine install directory"))?;

    let mut updater = AxoUpdater::new_for("plzplz");
    updater.set_release_source(ReleaseSource {
        release_type: ReleaseSourceType::GitHub,
        owner: "k88hudson".to_string(),
        name: "plzplz".to_string(),
        app_name: "plzplz".to_string(),
    });
    updater.set_install_dir(install_dir.to_string_lossy().as_ref());
    updater
        .set_current_version(current.parse()?)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let update_needed = updater
        .is_update_needed_sync()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if !update_needed {
        spinner.stop(format!("v{current} is already the latest version"));
        cliclack::outro("Nothing to do")?;
        return Ok(());
    }

    spinner.stop("New version available");
    let update_spinner = cliclack::spinner();
    update_spinner.start("Downloading and installing...");

    let result = updater.run_sync().map_err(|e| anyhow::anyhow!("{e}"))?;

    match result {
        Some(update_result) => {
            update_spinner.stop(format!("Updated to v{}", update_result.new_version));
            cliclack::outro("Update complete")?;
        }
        None => {
            update_spinner.stop("No update performed");
            cliclack::outro("Nothing to do")?;
        }
    }

    Ok(())
}
