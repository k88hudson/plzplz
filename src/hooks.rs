use crate::config::PlzConfig;
use crate::settings;
use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MANAGED_MARKER: &str = "# plz:managed - do not edit";
const HOOKS_VERSION: u32 = 2;

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
/// Group tasks are stored as "group:task" format.
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
    if let Some(ref groups) = config.taskgroup {
        let mut group_names: Vec<&String> = groups.keys().collect();
        group_names.sort();
        for gname in group_names {
            let group = &groups[gname];
            let mut gtask_names: Vec<&String> = group.tasks.keys().collect();
            gtask_names.sort();
            for tname in gtask_names {
                if let Some(ref hook) = group.tasks[tname].git_hook {
                    stages
                        .entry(hook.clone())
                        .or_default()
                        .push(format!("{gname}:{tname}"));
                }
            }
        }
    }
    stages
}

fn generate_hook_script(stage: &str) -> String {
    format!(
        "#!/bin/sh\n\
         {MANAGED_MARKER}\n\
         # plz:hooks_version={HOOKS_VERSION}\n\
         [ \"${{PLZ_SKIP_HOOKS}}\" = \"1\" ] && exit 0\n\
         command -v plz >/dev/null 2>&1 || {{ echo \"plz not found in PATH, skipping {stage} hook\" >&2; exit 0; }}\n\
         plz --no-interactive hooks run {stage}\n"
    )
}

fn installed_hook_version(path: &Path) -> Option<u32> {
    let content = fs::read_to_string(path).ok()?;
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("# plz:hooks_version=") {
            return v.trim().parse().ok();
        }
    }
    // Managed hook without a version tag is v1
    if content.contains(MANAGED_MARKER) {
        return Some(1);
    }
    None
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
pub fn run_stage(
    config: &PlzConfig,
    stage: &str,
    base_dir: &Path,
    interactive: bool,
) -> Result<()> {
    let stages = tasks_by_stage(config);
    let task_names = match stages.get(stage) {
        Some(names) => names,
        None => return Ok(()),
    };

    let names = task_names.join(", ");
    eprintln!("\x1b[36m🙏 Running {stage} hook ({names})\x1b[0m");

    for name in task_names {
        if let Some((group, task)) = name.split_once(':') {
            crate::runner::run_group_task(config, group, task, base_dir, interactive)?;
        } else {
            crate::runner::run_task(config, name, base_dir, interactive)?;
        }
    }
    eprintln!("\x1b[32m✓ {stage} hook passed\x1b[0m");
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
        let (status_icon, suffix) = match hooks_dir.as_ref() {
            Some(d) => {
                let p = d.join(stage);
                if !p.exists() || !is_plz_managed(&p) {
                    ("\x1b[2m·\x1b[0m", "")
                } else if installed_hook_version(&p).unwrap_or(0) < HOOKS_VERSION {
                    ("\x1b[33m↑\x1b[0m", " \x1b[33m(outdated)\x1b[0m")
                } else {
                    ("\x1b[32m✓\x1b[0m", "")
                }
            }
            None => ("\x1b[2m·\x1b[0m", ""),
        };
        eprintln!("{status_icon} {stage}: {names}{suffix}");
    }

    Ok(())
}

fn hook_needs_install(path: &Path) -> bool {
    if !path.exists() || !is_plz_managed(path) {
        return true;
    }
    installed_hook_version(path).unwrap_or(0) < HOOKS_VERSION
}

fn has_uninstalled_hooks(config: &PlzConfig, base_dir: &Path) -> bool {
    let stages = tasks_by_stage(config);
    if stages.is_empty() {
        return false;
    }
    let Ok(hooks_dir) = find_git_hooks_dir(base_dir) else {
        return false;
    };
    stages
        .keys()
        .any(|stage| hook_needs_install(&hooks_dir.join(stage)))
}

/// Show a grey tip if hooks are configured but not installed.
/// If ~/.plz doesn't exist yet, suggest running `plz plz` first.
pub fn hint_uninstalled_hooks(config: &PlzConfig, base_dir: &Path) {
    if std::env::var_os("PLZ_COMMAND").is_some() {
        return;
    }
    if !settings::config_dir_exists() {
        eprintln!("\x1b[2mRun `plz plz` to set up custom settings and templates.\x1b[0m");
        return;
    }
    if !settings::load().show_hints {
        return;
    }
    if has_uninstalled_hooks(config, base_dir) {
        eprintln!(
            "\x1b[2mYour plz.toml has git hooks that need to be installed or updated. Run `plz hooks` to install them.\x1b[0m"
        );
    }
}

/// Interactive hook install prompt (for `plz hooks` with no subcommand).
/// Shows status, then offers yes/no install.
pub fn interactive_install(config: &PlzConfig, base_dir: &Path, interactive: bool) -> Result<()> {
    status(config, base_dir)?;

    if !has_uninstalled_hooks(config, base_dir) {
        return Ok(());
    }

    if !interactive {
        return Ok(());
    }

    let should_install: bool = cliclack::confirm("Install hooks?")
        .initial_value(true)
        .interact()?;

    if should_install {
        install(config, base_dir)?;
    }

    Ok(())
}

/// Variant that auto-discovers config from cwd (for `plz plz`).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_hook_script() {
        let script = generate_hook_script("pre-commit");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains(MANAGED_MARKER));
        assert!(script.contains(&format!("# plz:hooks_version={HOOKS_VERSION}")));
        assert!(script.contains("plz --no-interactive hooks run pre-commit"));
        assert!(script.contains("PLZ_SKIP_HOOKS"));
        assert!(script.contains("command -v plz"));
    }

    #[test]
    fn test_generate_hook_script_commit_msg() {
        let script = generate_hook_script("commit-msg");
        assert!(script.contains("plz --no-interactive hooks run commit-msg"));
    }

    #[test]
    fn test_installed_hook_version_current() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pre-commit");
        fs::write(&path, generate_hook_script("pre-commit")).unwrap();
        assert_eq!(installed_hook_version(&path), Some(HOOKS_VERSION));
    }

    #[test]
    fn test_installed_hook_version_v1() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pre-commit");
        fs::write(
            &path,
            format!("#!/bin/sh\n{MANAGED_MARKER}\nplz hooks run pre-commit \"$@\"\n"),
        )
        .unwrap();
        assert_eq!(installed_hook_version(&path), Some(1));
    }

    #[test]
    fn test_installed_hook_version_not_managed() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("pre-commit");
        fs::write(&path, "#!/bin/sh\necho custom\n").unwrap();
        assert_eq!(installed_hook_version(&path), None);
    }

    #[test]
    fn test_installed_hook_version_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent");
        assert_eq!(installed_hook_version(&path), None);
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
