mod config;
mod init;
mod runner;
mod templates;
mod utils;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "plz",
    about = "A simple task runner. Define tasks in plz.toml and run them with `plz <task>`.",
    after_help = "\x1b[34mRun \x1b[1mplz\x1b[22m to choose a task\nRun \x1b[1mplz init\x1b[22m to create a new config\n\x1b[0m"
)]
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
    /// Manage global defaults for plz
    Plz {
        #[command(subcommand)]
        plz_command: Option<PlzCommand>,
    },
    /// Create a plz.toml
    Init,
    /// Add a new task to plz.toml
    Add {
        /// Name for the new task (prompted if omitted)
        name: Option<String>,
    },
    /// Browse and copy example task snippets
    Example,
}

#[derive(Subcommand)]
enum PlzCommand {
    /// Print JSON Schema for plz.toml
    Schema,
}

fn is_interactive(cli: &Cli) -> bool {
    !cli.no_interactive && !is_ci::cached()
}

const CONFIG_NAMES: &[&str] = &["plz.toml", ".plz.toml"];

fn find_config() -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    for name in CONFIG_NAMES {
        let path = cwd.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    bail!("No plz.toml found in current directory");
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Init) => return init::run(),
        Some(Command::Example) => return init::help_templates(),
        Some(Command::Add { name }) => return init::add_task(name),
        Some(Command::Plz { ref plz_command }) => match plz_command {
            Some(PlzCommand::Schema) => {
                let schema = schemars::schema_for!(config::PlzConfig);
                println!("{}", serde_json::to_string_pretty(&schema)?);
                return Ok(());
            }
            None => return init::setup(),
        },
        None => {}
    }

    let config_path = find_config()?;
    let config = config::load(&config_path)?;

    let interactive = is_interactive(&cli);
    let base_dir = config_path.parent().unwrap().to_path_buf();

    if cli.task.is_empty() {
        if !interactive {
            bail!("No task specified (running in non-interactive mode)");
        }
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
            Some(idx) => {
                return runner::run_task(&config, &names[idx], &base_dir, interactive);
            }
            None => {
                println!("\x1b[2m✕  Cancelled\x1b[0m");
                return Ok(());
            }
        }
    }

    let input = &cli.task[0];

    if input == "add" && !config.tasks.contains_key("add") {
        let name = cli.task.get(1).cloned();
        return init::add_task(name);
    }

    let task_name = resolve_task(&config, input, interactive)?;
    runner::run_task(&config, &task_name, &base_dir, interactive)?;

    Ok(())
}

fn resolve_task(config: &config::PlzConfig, input: &str, interactive: bool) -> Result<String> {
    if config.tasks.contains_key(input) {
        return Ok(input.to_string());
    }

    if !interactive {
        bail!("Unknown task: {input}");
    }

    let mut matches: Vec<&String> = config
        .tasks
        .keys()
        .filter(|k| utils::fuzzy_match(input, k))
        .collect();
    matches.sort();

    match matches.len() {
        0 => bail!("Unknown task: {input}"),
        1 => {
            let confirmed: bool = cliclack::confirm(format!("Did you mean \"{}\"?", matches[0]))
                .initial_value(true)
                .interact()?;
            if confirmed {
                Ok(matches[0].clone())
            } else {
                bail!("Cancelled");
            }
        }
        _ => {
            let selected: &&String = cliclack::select(format!("Did you mean..."))
                .items(
                    &matches
                        .iter()
                        .map(|n| (n, n.as_str(), ""))
                        .collect::<Vec<_>>(),
                )
                .interact()?;
            Ok(selected.to_string())
        }
    }
}
