mod config;
mod hooks;
mod init;
mod runner;
mod settings;
mod templates;
mod utils;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "plz")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Task name, followed by any extra arguments to pass through
    #[arg(trailing_var_arg = true)]
    task: Vec<String>,

    /// Disable interactive prompts (auto-detected in CI)
    #[arg(long)]
    no_interactive: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Manage plz itself: init, add, hooks, schema, and more
    Plz {
        #[command(subcommand)]
        plz_command: Option<PlzCommand>,
    },
}

#[derive(Subcommand)]
enum PlzCommand {
    /// Create a plz.toml
    Init,
    /// Add a new task to plz.toml
    Add {
        /// Name for the new task (prompted if omitted)
        name: Option<String>,
    },
    /// Install or manage git hooks
    Hooks {
        #[command(subcommand)]
        hook_command: Option<HookCommand>,
    },
    /// Print JSON Schema for plz.toml
    Schema,
    /// Print a cheatsheet of plz.toml features
    Cheatsheet,
    /// Update plz to the latest version
    Update,
}

#[derive(Subcommand)]
enum HookCommand {
    /// Install git hooks from plz.toml
    Install,
    /// Uninstall plz-managed git hooks
    Uninstall,
    /// Add a git hook stage to existing tasks
    Add,
    /// Run all tasks for a git hook stage (called by hook scripts)
    Run {
        /// The git hook stage to run (e.g. pre-commit)
        stage: String,
        /// Extra arguments forwarded to tasks (e.g. commit message file for commit-msg)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

fn is_nested() -> bool {
    env::var_os("PLZ_COMMAND").is_some()
}

fn is_interactive(cli: &Cli) -> bool {
    if cli.no_interactive {
        return false;
    }
    if is_nested() {
        return false;
    }
    if is_ci::cached() {
        eprintln!("Skipping interactive prompts: CI environment detected");
        return false;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!("Skipping interactive prompts: stdin is not a terminal");
        return false;
    }
    true
}

const CONFIG_NAMES: &[&str] = &["plz.toml", ".plz.toml"];

fn find_config() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    for name in CONFIG_NAMES {
        let path = cwd.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

struct HelpEntry {
    usage: &'static str,
    description: &'static str,
}

const HELP_COMMANDS: &[HelpEntry] = &[
    HelpEntry {
        usage: "init",
        description: "Create a plz.toml",
    },
    HelpEntry {
        usage: "add [name]",
        description: "Add a new task to plz.toml",
    },
    HelpEntry {
        usage: "hooks",
        description: "Install or manage git hooks",
    },
    HelpEntry {
        usage: "hooks add",
        description: "Add a git hook to existing tasks",
    },
    HelpEntry {
        usage: "schema",
        description: "Print JSON Schema for plz.toml",
    },
    HelpEntry {
        usage: "cheatsheet",
        description: "Print a cheatsheet of plz.toml features",
    },
    HelpEntry {
        usage: "update",
        description: "Update plz to the latest version",
    },
    HelpEntry {
        usage: "plz",
        description: "Manage global defaults",
    },
];

const HELP_OPTIONS: &[HelpEntry] = &[
    HelpEntry {
        usage: "--no-interactive",
        description: "Disable interactive prompts (auto-detected in CI)",
    },
    HelpEntry {
        usage: "-h, --help",
        description: "Print help",
    },
];

enum ResolvedTask {
    Task(String),
    GroupTask(String, String),
}

pub fn format_help() -> String {
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    let mut out = String::new();
    out.push_str(&format!(
        "{bold}plz{reset} [task] [args...]          Run a task from plz.toml\n"
    ));
    out.push_str(&format!(
        "{bold}plz{reset} [group] [task] [args...]  Run a task from a task group\n"
    ));
    out.push_str(&format!(
        "{bold}plz{reset}                           Choose a task interactively\n"
    ));
    out.push('\n');

    let max_usage = HELP_COMMANDS
        .iter()
        .map(|e| e.usage.len())
        .max()
        .unwrap_or(0);
    out.push_str(&format!("{dim}Commands:{reset}\n"));
    for entry in HELP_COMMANDS {
        let padding = " ".repeat(max_usage - entry.usage.len() + 2);
        out.push_str(&format!(
            "  {dim}plz{reset} {}{padding}{}\n",
            entry.usage, entry.description
        ));
    }

    out.push('\n');
    let max_opt = HELP_OPTIONS
        .iter()
        .map(|e| e.usage.len())
        .max()
        .unwrap_or(0);
    out.push_str(&format!("{dim}Options:{reset}\n"));
    for entry in HELP_OPTIONS {
        let padding = " ".repeat(max_opt - entry.usage.len() + 2);
        out.push_str(&format!(
            "  {}{padding}{}\n",
            entry.usage, entry.description
        ));
    }

    out
}

fn main() -> Result<()> {
    // Intercept --help/-h at top level before clap parses
    // (clap's help is disabled so subcommands keep their own help)
    {
        let args: Vec<String> = env::args().collect();
        if args.len() == 2 && (args[1] == "--help" || args[1] == "-h" || args[1] == "help") {
            print!("{}", format_help());
            return Ok(());
        }
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Plz { ref plz_command }) => match plz_command {
            Some(PlzCommand::Init) => return init::run(),
            Some(PlzCommand::Add { name }) => return init::add_task(name.clone()),
            Some(PlzCommand::Schema) => {
                let schema = schemars::schema_for!(config::PlzConfig);
                println!("{}", serde_json::to_string_pretty(&schema)?);
                return Ok(());
            }
            Some(PlzCommand::Cheatsheet) => return init::print_cheatsheet(),
            Some(PlzCommand::Update) => return init::self_update(),
            Some(PlzCommand::Hooks { hook_command }) => {
                let config_path =
                    find_config().ok_or_else(|| anyhow::anyhow!("No plz.toml found"))?;
                let config = config::load(&config_path)?;
                let base_dir = config_path.parent().unwrap().to_path_buf();
                let interactive = is_interactive(&cli);
                match hook_command {
                    Some(HookCommand::Install) => return hooks::install(&config, &base_dir),
                    Some(HookCommand::Uninstall) => return hooks::uninstall(&config, &base_dir),
                    Some(HookCommand::Add) => return hooks::add_hook(&config, &config_path),
                    Some(HookCommand::Run { stage, .. }) => {
                        return hooks::run_stage(&config, stage, &base_dir, interactive);
                    }
                    None => {
                        return hooks::interactive_install(&config, &base_dir, interactive);
                    }
                }
            }
            None => return init::setup(),
        },
        None => {}
    }

    let interactive = is_interactive(&cli);

    let config_path = match find_config() {
        Some(path) => path,
        None => {
            if cli.task.is_empty() {
                if interactive {
                    return init::run();
                }
                print!("{}", format_help());
                return Ok(());
            }
            if let Some(result) = try_plz_subcommand(&cli.task) {
                return result;
            }
            bail!("No plz.toml found. Run `plz init` to create one.");
        }
    };
    let config = config::load(&config_path)?;
    let base_dir = config_path.parent().unwrap().to_path_buf();

    if cli.task.is_empty() {
        if !interactive {
            bail!("No task specified (running in non-interactive mode)");
        }

        // Build pick list: top-level tasks + group:task entries
        let mut pick_entries: Vec<(String, ResolvedTask)> = Vec::new();
        let mut names: Vec<&String> = config.tasks.keys().collect();
        names.sort();
        for name in &names {
            pick_entries.push((name.to_string(), ResolvedTask::Task(name.to_string())));
        }
        if let Some(ref groups) = config.taskgroup {
            let mut group_names: Vec<&String> = groups.keys().collect();
            group_names.sort();
            for gname in group_names {
                let group = &groups[gname];
                let mut task_names: Vec<&String> = group.tasks.keys().collect();
                task_names.sort();
                for tname in task_names {
                    pick_entries.push((
                        format!("{gname}:{tname}"),
                        ResolvedTask::GroupTask(gname.clone(), tname.clone()),
                    ));
                }
            }
        }

        if pick_entries.is_empty() {
            bail!("No tasks defined in plz.toml");
        }

        let items: Vec<utils::PickItem> = pick_entries
            .iter()
            .map(|(label, resolved)| {
                let desc = match resolved {
                    ResolvedTask::Task(n) => {
                        config.tasks[n].description.clone().unwrap_or_default()
                    }
                    ResolvedTask::GroupTask(g, t) => config
                        .get_group_task(g, t)
                        .and_then(|task| task.description.clone())
                        .unwrap_or_default(),
                };
                utils::PickItem {
                    label: label.clone(),
                    description: desc,
                    preview: None,
                }
            })
            .collect();
        match utils::pick_from_list(&items, "Enter to run · Esc to cancel")? {
            Some(idx) => {
                match &pick_entries[idx].1 {
                    ResolvedTask::Task(name) => {
                        runner::run_task(&config, name, &base_dir, interactive)?;
                    }
                    ResolvedTask::GroupTask(g, t) => {
                        runner::run_group_task(&config, g, t, &base_dir, interactive)?;
                    }
                }
                hooks::hint_uninstalled_hooks(&config, &base_dir);
                return Ok(());
            }
            None => {
                println!("\x1b[2m✕  Cancelled\x1b[0m");
                return Ok(());
            }
        }
    }

    let input = &cli.task[0];

    // Fall through to built-in subcommands if no task matches
    if !config.tasks.contains_key(input)
        && let Some(result) = try_plz_subcommand(&cli.task)
    {
        return result;
    }

    let resolved = resolve_task(&config, input, &cli.task[1..], interactive)?;
    match resolved {
        ResolvedTask::Task(task_name) => {
            let extra_args = &cli.task[1..];
            runner::run_task_with_args(&config, &task_name, &base_dir, interactive, extra_args)?;
        }
        ResolvedTask::GroupTask(group, task) => {
            // For group tasks, args start at [2] (task[0]=group, task[1]=task_name)
            let extra_args = if cli.task.len() > 2 {
                &cli.task[2..]
            } else {
                &[]
            };
            runner::run_group_task_with_args(
                &config,
                &group,
                &task,
                &base_dir,
                interactive,
                extra_args,
            )?;
        }
    }
    hooks::hint_uninstalled_hooks(&config, &base_dir);

    Ok(())
}

fn try_plz_subcommand(task: &[String]) -> Option<Result<()>> {
    let input = task.first()?.as_str();
    match input {
        "init" => Some(init::run()),
        "add" => {
            let name = task.get(1).cloned();
            Some(init::add_task(name))
        }
        "schema" => {
            let schema = schemars::schema_for!(config::PlzConfig);
            Some(
                serde_json::to_string_pretty(&schema)
                    .map(|s| println!("{}", s))
                    .map_err(Into::into),
            )
        }
        "cheatsheet" => Some(init::print_cheatsheet()),
        "update" => Some(init::self_update()),
        "help" => {
            print!("{}", format_help());
            Some(Ok(()))
        }
        "hooks" => {
            let config_path = match find_config() {
                Some(p) => p,
                None => return Some(Err(anyhow::anyhow!("No plz.toml found"))),
            };
            let config = match config::load(&config_path) {
                Ok(c) => c,
                Err(e) => return Some(Err(e)),
            };
            let base_dir = config_path.parent().unwrap().to_path_buf();
            let sub = task.get(1).map(|s| s.as_str());
            match sub {
                Some("install") => Some(hooks::install(&config, &base_dir)),
                Some("uninstall") => Some(hooks::uninstall(&config, &base_dir)),
                Some("add") => Some(hooks::add_hook(&config, &config_path)),
                _ => {
                    let interactive = !is_ci::cached()
                        && std::io::stdin().is_terminal()
                        && env::var_os("PLZ_COMMAND").is_none();
                    Some(hooks::interactive_install(&config, &base_dir, interactive))
                }
            }
        }
        _ => None,
    }
}

fn resolve_task(
    config: &config::PlzConfig,
    input: &str,
    rest: &[String],
    interactive: bool,
) -> Result<ResolvedTask> {
    // 1. Exact match on top-level task (top-level wins)
    if config.tasks.contains_key(input) {
        return Ok(ResolvedTask::Task(input.to_string()));
    }

    // 2. Check if input matches a taskgroup name
    if let Some(group) = config.get_group(input) {
        if rest.is_empty() {
            // `plz <group>` with no task — interactive picker within group
            if !interactive {
                bail!(
                    "No task specified for group \"{input}\". Available tasks: {}",
                    {
                        let mut names: Vec<&String> = group.tasks.keys().collect();
                        names.sort();
                        names
                            .iter()
                            .map(|n| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                );
            }
            let mut names: Vec<&String> = group.tasks.keys().collect();
            names.sort();
            if names.is_empty() {
                bail!("No tasks defined in group \"{input}\"");
            }
            let items: Vec<utils::PickItem> = names
                .iter()
                .map(|name| utils::PickItem {
                    label: name.to_string(),
                    description: group.tasks[*name].description.clone().unwrap_or_default(),
                    preview: None,
                })
                .collect();
            match utils::pick_from_list(&items, "Enter to run · Esc to cancel")? {
                Some(idx) => {
                    return Ok(ResolvedTask::GroupTask(
                        input.to_string(),
                        names[idx].clone(),
                    ));
                }
                None => bail!("Cancelled"),
            }
        }

        let task_input = &rest[0];

        // Exact match within group
        if group.tasks.contains_key(task_input.as_str()) {
            return Ok(ResolvedTask::GroupTask(
                input.to_string(),
                task_input.clone(),
            ));
        }

        // Fuzzy match within group
        if !interactive {
            bail!("\"{input}:{task_input}\" isn't a task. Run `plz {input}` to see group tasks.");
        }

        let mut matches: Vec<&String> = group
            .tasks
            .keys()
            .filter(|k| utils::fuzzy_match(task_input, k))
            .collect();
        matches.sort();

        match matches.len() {
            0 => {
                bail!(
                    "\"{input}:{task_input}\" isn't a task. Run `plz {input}` to see group tasks."
                )
            }
            1 => {
                let confirmed: bool =
                    cliclack::confirm(format!("Did you mean \"{input}:{}\"?", matches[0]))
                        .initial_value(true)
                        .interact()?;
                if confirmed {
                    return Ok(ResolvedTask::GroupTask(
                        input.to_string(),
                        matches[0].clone(),
                    ));
                }
                bail!("Cancelled");
            }
            _ => {
                let selected: &&String = cliclack::select("Did you mean...".to_string())
                    .items(
                        &matches
                            .iter()
                            .map(|n| (n, n.as_str(), ""))
                            .collect::<Vec<_>>(),
                    )
                    .interact()?;
                return Ok(ResolvedTask::GroupTask(
                    input.to_string(),
                    selected.to_string(),
                ));
            }
        }
    }

    // 3. Fall through to fuzzy match on top-level tasks
    if !interactive {
        bail!("\"{input}\" isn't a task. Run `plz` to see all commands.");
    }

    let mut matches: Vec<&String> = config
        .tasks
        .keys()
        .filter(|k| utils::fuzzy_match(input, k))
        .collect();
    matches.sort();

    match matches.len() {
        0 => {
            eprintln!("\x1b[2m\"{input}\" isn't a task.\x1b[0m\n");
            let mut names: Vec<&String> = config.tasks.keys().collect();
            names.sort();
            if names.is_empty() {
                bail!("No tasks defined in plz.toml");
            }
            let items: Vec<utils::PickItem> = names
                .iter()
                .map(|name| utils::PickItem {
                    label: name.to_string(),
                    description: config.tasks[*name].description.clone().unwrap_or_default(),
                    preview: None,
                })
                .collect();
            match utils::pick_from_list(&items, "Enter to run · Esc to cancel")? {
                Some(idx) => Ok(ResolvedTask::Task(names[idx].clone())),
                None => bail!("Cancelled"),
            }
        }
        1 => {
            let confirmed: bool = cliclack::confirm(format!("Did you mean \"{}\"?", matches[0]))
                .initial_value(true)
                .interact()?;
            if confirmed {
                Ok(ResolvedTask::Task(matches[0].clone()))
            } else {
                bail!("Cancelled");
            }
        }
        _ => {
            let selected: &&String = cliclack::select("Did you mean...".to_string())
                .items(
                    &matches
                        .iter()
                        .map(|n| (n, n.as_str(), ""))
                        .collect::<Vec<_>>(),
                )
                .interact()?;
            Ok(ResolvedTask::Task(selected.to_string()))
        }
    }
}
