use crate::config::{FailHook, PlzConfig};
use anyhow::{Result, bail};
use std::path::Path;
use std::process::Command;

pub fn run_task(
    config: &PlzConfig,
    task_name: &str,
    base_dir: &Path,
    interactive: bool,
) -> Result<()> {
    let task = config
        .tasks
        .get(task_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown task: {task_name}"))?;

    let work_dir = match &task.dir {
        Some(d) => base_dir.join(d),
        None => base_dir.to_path_buf(),
    };

    let wrap = |cmd: &str| -> String {
        match task.tool_env.as_deref() {
            Some("uv") => format!("uv run {cmd}"),
            Some("pnpm") => format!("pnpm exec {cmd}"),
            _ => cmd.to_string(),
        }
    };

    let result: Result<()> = (|| {
        if let Some(ref cmd) = task.run {
            run_command_or_ref(config, &wrap(cmd), &work_dir, base_dir, interactive)?;
        }

        if let Some(ref cmds) = task.run_serial {
            for cmd in cmds {
                run_command_or_ref(config, &wrap(cmd), &work_dir, base_dir, interactive)?;
            }
        }

        if let Some(ref cmds) = task.run_parallel {
            run_parallel_commands(config, cmds, &wrap, &work_dir, base_dir, interactive)?;
        }

        Ok(())
    })();

    if let Err(ref e) = result
        && let Some(ref hook) = task.fail_hook
    {
        handle_fail_hook(hook, e, &work_dir, interactive)?;
    }

    result
}

fn run_command_or_ref(
    config: &PlzConfig,
    cmd: &str,
    work_dir: &Path,
    base_dir: &Path,
    interactive: bool,
) -> Result<()> {
    if let Some(ref_name) = cmd.strip_prefix("plz:") {
        return run_task(config, ref_name, base_dir, interactive);
    }
    exec_shell(cmd, work_dir)
}

fn exec_shell(cmd: &str, work_dir: &Path) -> Result<()> {
    eprintln!("→ {cmd}");
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(work_dir)
        .status()?;

    if !status.success() {
        bail!(
            "Command failed with exit code {}: {cmd}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

fn run_parallel_commands(
    config: &PlzConfig,
    cmds: &[String],
    wrap: &dyn Fn(&str) -> String,
    work_dir: &Path,
    base_dir: &Path,
    interactive: bool,
) -> Result<()> {
    let mut children = Vec::new();
    let mut plz_refs = Vec::new();

    for cmd in cmds {
        let wrapped = wrap(cmd);
        if let Some(ref_name) = wrapped.strip_prefix("plz:") {
            plz_refs.push(ref_name.to_string());
        } else {
            eprintln!("→ {wrapped} &");
            let child = Command::new("/bin/sh")
                .arg("-c")
                .arg(&wrapped)
                .current_dir(work_dir)
                .spawn()?;
            children.push((wrapped, child));
        }
    }

    for ref_name in &plz_refs {
        run_task(config, ref_name, base_dir, interactive)?;
    }

    for (cmd, mut child) in children {
        let status = child.wait()?;
        if !status.success() {
            bail!(
                "Command failed with exit code {}: {cmd}",
                status.code().unwrap_or(-1)
            );
        }
    }

    Ok(())
}

fn handle_fail_hook(
    hook: &FailHook,
    error: &anyhow::Error,
    work_dir: &Path,
    interactive: bool,
) -> Result<()> {
    match hook {
        FailHook::Command(cmd) => {
            eprintln!("Task failed: {error}");
            eprintln!("Running fail hook: {cmd}");
            let _ = exec_shell(cmd, work_dir);
        }
        FailHook::Message(msg) => {
            eprintln!("\n\x1b[31mTask failed:\x1b[0m {error}");
            eprintln!("⚠️  {msg}");
        }
        FailHook::Suggest { suggest_command } => {
            eprintln!("\n\x1b[31mTask failed:\x1b[0m {error}");
            if !interactive {
                eprintln!("\x1b[33mSuggestion:\x1b[0m try running \x1b[1m{suggest_command}\x1b[0m");
            } else {
                let run_it: bool = cliclack::confirm(format!("Run `{suggest_command}`?"))
                    .initial_value(true)
                    .interact()
                    .unwrap_or(false);
                if run_it {
                    let _ = exec_shell(suggest_command, work_dir);
                }
            }
        }
    }
    Ok(())
}