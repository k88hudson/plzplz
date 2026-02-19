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
    run_task_impl(config, task_name, base_dir, interactive, true, &[])
}

pub fn run_task_with_args(
    config: &PlzConfig,
    task_name: &str,
    base_dir: &Path,
    interactive: bool,
    extra_args: &[String],
) -> Result<()> {
    run_task_impl(config, task_name, base_dir, interactive, true, extra_args)
}

fn run_task_impl(
    config: &PlzConfig,
    task_name: &str,
    base_dir: &Path,
    interactive: bool,
    run_hooks: bool,
    extra_args: &[String],
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
            let wrapped = if extra_args.is_empty() {
                wrap(cmd)
            } else {
                let args_str = shlex::try_join(extra_args.iter().map(|s| s.as_str()))
                    .map_err(|e| anyhow::anyhow!("Failed to escape arguments: {e}"))?;
                format!("{} {args_str}", wrap(cmd))
            };
            run_command_or_ref(config, &wrapped, &work_dir, base_dir, interactive)?;
        }

        if let Some(ref cmds) = task.run_serial {
            run_serial_commands(config, cmds, &wrap, &work_dir, base_dir, interactive)?;
        }

        if let Some(ref cmds) = task.run_parallel {
            run_parallel_commands(config, cmds, &wrap, &work_dir, base_dir, interactive)?;
        }

        Ok(())
    })();

    if run_hooks {
        if let Err(ref e) = result
            && let Some(ref hook) = task.fail_hook
        {
            if handle_fail_hook(hook, e, &work_dir, task.tool_env.as_deref(), interactive)? {
                return Ok(());
            }
        }
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
        .env("PLZ_COMMAND", "1")
        .status()?;

    if !status.success() {
        bail!(
            "Command failed with exit code {}: {cmd}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

struct DeferredFailure {
    name: String,
    error: anyhow::Error,
}

fn print_summary(results: &[(String, bool)]) {
    let total = results.len();
    let parts: Vec<String> = results
        .iter()
        .map(|(name, ok)| {
            if *ok {
                format!("\x1b[32m✓ {name}\x1b[0m")
            } else {
                format!("\x1b[31m✗ {name}\x1b[0m")
            }
        })
        .collect();
    eprintln!("\nRan {total} tasks: {}", parts.join("  "));
}

/// Process deferred failures: run each task's fail_hook in succession,
/// asking "continue?" between unresolved ones.
fn handle_deferred_failures(
    config: &PlzConfig,
    failures: Vec<DeferredFailure>,
    base_dir: &Path,
    interactive: bool,
) -> Result<()> {
    for (i, failure) in failures.iter().enumerate() {
        let task = config.tasks.get(&failure.name);
        let hook = task.and_then(|t| t.fail_hook.as_ref());

        if let Some(hook) = hook {
            let task_work_dir = task
                .and_then(|t| t.dir.as_ref())
                .map(|d| base_dir.join(d))
                .unwrap_or_else(|| base_dir.to_path_buf());
            let tool_env = task.and_then(|t| t.tool_env.as_deref());

            if handle_fail_hook(hook, &failure.error, &task_work_dir, tool_env, interactive)? {
                continue;
            }
        } else {
            eprintln!(
                "\n\x1b[31mTask failed:\x1b[0m {}: {}",
                failure.name, failure.error
            );
        }

        let has_more = i + 1 < failures.len();
        if interactive && has_more {
            let cont = cliclack::confirm("Continue to next task?")
                .initial_value(true)
                .interact()
                .unwrap_or(false);
            if !cont {
                bail!("Aborted");
            }
        }
    }

    bail!("One or more tasks failed");
}

fn run_serial_commands(
    config: &PlzConfig,
    cmds: &[String],
    wrap: &dyn Fn(&str) -> String,
    work_dir: &Path,
    base_dir: &Path,
    interactive: bool,
) -> Result<()> {
    let mut task_results: Vec<(String, bool)> = Vec::new();
    let mut failures: Vec<DeferredFailure> = Vec::new();

    for cmd in cmds {
        let wrapped = wrap(cmd);
        if let Some(ref_name) = wrapped.strip_prefix("plz:") {
            match run_task_impl(config, ref_name, base_dir, interactive, false, &[]) {
                Ok(()) => task_results.push((ref_name.to_string(), true)),
                Err(e) => {
                    task_results.push((ref_name.to_string(), false));
                    failures.push(DeferredFailure {
                        name: ref_name.to_string(),
                        error: e,
                    });
                }
            }
        } else {
            exec_shell(&wrapped, work_dir)?;
        }
    }

    if !failures.is_empty() {
        if task_results.len() > 1 {
            print_summary(&task_results);
        }
        return handle_deferred_failures(config, failures, base_dir, interactive);
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
                .env("PLZ_COMMAND", "1")
                .spawn()?;
            children.push((wrapped, child));
        }
    }

    let mut task_results: Vec<(String, bool)> = Vec::new();
    let mut failures: Vec<DeferredFailure> = Vec::new();

    for ref_name in &plz_refs {
        match run_task_impl(config, ref_name, base_dir, interactive, false, &[]) {
            Ok(()) => task_results.push((ref_name.clone(), true)),
            Err(e) => {
                task_results.push((ref_name.clone(), false));
                failures.push(DeferredFailure {
                    name: ref_name.clone(),
                    error: e,
                });
            }
        }
    }

    for (cmd, mut child) in children {
        let status = child.wait()?;
        if !status.success() {
            task_results.push((cmd.clone(), false));
            failures.push(DeferredFailure {
                name: cmd.clone(),
                error: anyhow::anyhow!(
                    "Command failed with exit code {}: {cmd}",
                    status.code().unwrap_or(-1)
                ),
            });
        } else {
            task_results.push((cmd, true));
        }
    }

    if !failures.is_empty() {
        if task_results.len() > 1 {
            print_summary(&task_results);
        }
        return handle_deferred_failures(config, failures, base_dir, interactive);
    }

    Ok(())
}

/// Returns true if the fail hook resolved the failure (e.g. suggestion was taken and succeeded).
fn handle_fail_hook(
    hook: &FailHook,
    error: &anyhow::Error,
    work_dir: &Path,
    tool_env: Option<&str>,
    interactive: bool,
) -> Result<bool> {
    let wrap = |cmd: &str| -> String {
        match tool_env {
            Some("uv") => format!("uv run {cmd}"),
            Some("pnpm") => format!("pnpm exec {cmd}"),
            _ => cmd.to_string(),
        }
    };

    match hook {
        FailHook::Command(cmd) => {
            let wrapped = wrap(cmd);
            eprintln!("\n\x1b[31mTask failed:\x1b[0m {error}");
            eprintln!("Running fail hook: {wrapped}");
            let _ = exec_shell(&wrapped, work_dir);
        }
        FailHook::Message(msg) => {
            eprintln!("\n\x1b[31mTask failed:\x1b[0m {error}");
            eprintln!("⚠️  {msg}");
        }
        FailHook::Suggest { suggest_command } => {
            let wrapped = wrap(suggest_command);
            eprintln!("\n\x1b[31mTask failed:\x1b[0m {error}");
            if !interactive {
                eprintln!("\x1b[33mSuggestion:\x1b[0m try running \x1b[1m{wrapped}\x1b[0m");
            } else {
                let run_it: bool = cliclack::confirm(format!("Run `{wrapped}`?"))
                    .initial_value(true)
                    .interact()
                    .unwrap_or(false);
                if run_it {
                    if exec_shell(&wrapped, work_dir).is_ok() {
                        return Ok(true);
                    }
                    eprintln!("\x1b[31mFix command failed.\x1b[0m");
                }
            }
        }
    }
    Ok(false)
}
