use crate::config::PlzConfig;
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MANAGED_MARKER: &str = "# plz:managed - do not edit";

pub fn find_git_hooks_dir(base_dir: &Path) -> Result<PathBuf> {
    let mut dir = base_dir;
    loop {
        let git_dir = dir.join(".git");
        if git_dir.is_dir() {
            return Ok(git_dir.join("hooks"));
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => bail!("Not a git repository (no .git directory found)"),
        }
    }
}

/// Group tasks by their git_hook stage. Returns sorted map for deterministic output.
pub fn tasks_by_stage(config: &PlzConfig) -> BTreeMap<String, Vec<String>> {
    let mut stages: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut task_names: Vec<&String> = config.tasks.keys().collect();
    task_names.sort();
    for name in task_names {
        let task = &config.tasks[name];
        if let Some(ref hook) = task.git_hook {
            stages.entry(hook.clone()).or_default().push(name.clone());
        }
    }
    stages
}

fn generate_hook_script(stage: &str) -> String {
    format!(
        "#!/bin/sh\n\
         {MANAGED_MARKER}\n\
         [ \"${{PLZ_SKIP_HOOKS}}\" = \"1\" ] && exit 0\n\
         command -v plz >/dev/null 2>&1 || {{ echo \"plz not found in PATH, skipping {stage} hook\" >&2; exit 0; }}\n\
         plz --no-interactive hooks run {stage} \"$@\"\n"
    )
}

fn is_plz_managed(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|content| content.contains(MANAGED_MARKER))
        .unwrap_or(false)
}

pub fn install(config: &PlzConfig, base_dir: &Path) -> Result<()> {
    let stages = tasks_by_stage(config);
    if stages.is_empty() {
        eprintln!("No tasks have git_hook configured in plz.toml");
        return Ok(());
    }

    let hooks_dir = find_git_hooks_dir(base_dir)?;
    fs::create_dir_all(&hooks_dir)?;

    for (stage, task_names) in &stages {
        let hook_path = hooks_dir.join(stage);

        if hook_path.exists() && !is_plz_managed(&hook_path) {
            eprintln!(
                "\x1b[33mWarning:\x1b[0m Skipping {stage} — existing hook is not plz-managed"
            );
            continue;
        }

        let script = generate_hook_script(stage);
        fs::write(&hook_path, &script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))?;
        }

        let names = task_names.join(", ");
        eprintln!("\x1b[32m✓\x1b[0m Installed {stage} hook (tasks: {names})");
    }

    Ok(())
}

pub fn uninstall(config: &PlzConfig, base_dir: &Path) -> Result<()> {
    let stages = tasks_by_stage(config);
    if stages.is_empty() {
        eprintln!("No tasks have git_hook configured in plz.toml");
        return Ok(());
    }

    let hooks_dir = find_git_hooks_dir(base_dir)?;

    for stage in stages.keys() {
        let hook_path = hooks_dir.join(stage);
        if !hook_path.exists() {
            continue;
        }
        if !is_plz_managed(&hook_path) {
            eprintln!("\x1b[33mWarning:\x1b[0m Skipping {stage} — not plz-managed");
            continue;
        }
        fs::remove_file(&hook_path)?;
        eprintln!("\x1b[32m✓\x1b[0m Removed {stage} hook");
    }

    Ok(())
}

/// Run all tasks for a given git hook stage (called by the hook script itself).
/// `extra_args` are forwarded to each task (e.g. commit message file path for commit-msg hooks).
pub fn run_stage(
    config: &PlzConfig,
    stage: &str,
    base_dir: &Path,
    interactive: bool,
    extra_args: &[String],
) -> Result<()> {
    let stages = tasks_by_stage(config);
    let task_names = match stages.get(stage) {
        Some(names) => names,
        None => return Ok(()),
    };

    for name in task_names {
        crate::runner::run_task_with_args(config, name, base_dir, interactive, extra_args)?;
    }
    Ok(())
}

pub fn status(config: &PlzConfig, base_dir: &Path) -> Result<()> {
    let stages = tasks_by_stage(config);
    if stages.is_empty() {
        eprintln!("No tasks have git_hook configured in plz.toml");
        return Ok(());
    }

    let hooks_dir = find_git_hooks_dir(base_dir).ok();

    for (stage, task_names) in &stages {
        let names = task_names.join(", ");
        let installed = hooks_dir
            .as_ref()
            .map(|d| {
                let p = d.join(stage);
                p.exists() && is_plz_managed(&p)
            })
            .unwrap_or(false);

        let status_icon = if installed {
            "\x1b[32m✓\x1b[0m"
        } else {
            "\x1b[2m·\x1b[0m"
        };
        eprintln!("{status_icon} {stage}: {names}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_hook_script() {
        let script = generate_hook_script("pre-commit");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains(MANAGED_MARKER));
        assert!(script.contains("plz --no-interactive hooks run pre-commit \"$@\""));
        assert!(script.contains("PLZ_SKIP_HOOKS"));
        assert!(script.contains("command -v plz"));
    }

    #[test]
    fn test_generate_hook_script_commit_msg() {
        let script = generate_hook_script("commit-msg");
        assert!(script.contains("plz --no-interactive hooks run commit-msg \"$@\""));
    }

    #[test]
    fn test_is_plz_managed_true() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pre-commit");
        fs::write(
            &path,
            format!("#!/bin/sh\n{MANAGED_MARKER}\nplz hooks run pre-commit\n"),
        )
        .unwrap();
        assert!(is_plz_managed(&path));
    }

    #[test]
    fn test_is_plz_managed_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pre-commit");
        fs::write(&path, "#!/bin/sh\necho custom hook\n").unwrap();
        assert!(!is_plz_managed(&path));
    }

    #[test]
    fn test_is_plz_managed_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent");
        assert!(!is_plz_managed(&path));
    }
}
