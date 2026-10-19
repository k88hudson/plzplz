use std::path::{Path, PathBuf};

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    /// cargo-dist shell/powershell installer; can update itself
    Installer,
    UvTool,
    Pipx,
    /// Any other Python venv, e.g. a project dependency
    Venv,
    Cargo,
    Unknown,
}

impl InstallMethod {
    pub fn upgrade_command(self) -> Option<&'static str> {
        match self {
            InstallMethod::Installer => Some("plz update"),
            InstallMethod::UvTool => Some("uv tool upgrade plzplz"),
            InstallMethod::Pipx => Some("pipx upgrade plzplz"),
            InstallMethod::Cargo => Some("cargo install --locked plzplz"),
            InstallMethod::Venv | InstallMethod::Unknown => None,
        }
    }

    /// One-line advice on how to update, shared by `plz update` and the update hint
    pub fn update_advice(self) -> String {
        if let Some(cmd) = self.upgrade_command() {
            return format!("Run `{cmd}` to upgrade.");
        }
        match self {
            InstallMethod::Venv => "plz is in a Python virtual environment. Upgrade it with that \
                 project's package manager, e.g. `uv sync --upgrade-package plzplz`."
                .to_string(),
            _ => "Couldn't determine how plz was installed. Update it with the tool you \
                 installed it with, or reinstall from https://plzplz.org/getting-started.html"
                .to_string(),
        }
    }
}

pub fn new_updater() -> AxoUpdater {
    let mut updater = AxoUpdater::new_for("plzplz");
    updater.set_release_source(ReleaseSource {
        release_type: ReleaseSourceType::GitHub,
        owner: "k88hudson".to_string(),
        name: "plzplz".to_string(),
        app_name: "plzplz".to_string(),
    });
    updater
}

pub fn detect() -> InstallMethod {
    detect_with_updater().0
}

/// Detects how the running binary was installed. For `Installer`, also
/// returns the updater with its receipt loaded. Python environments are
/// checked before the cargo-dist receipt, since a stale receipt may point into
/// a uv/pipx venv after an older `plz update` ran the installer there.
pub fn detect_with_updater() -> (InstallMethod, Option<AxoUpdater>) {
    let Ok(raw_exe) = std::env::current_exe() else {
        return (InstallMethod::Unknown, None);
    };
    let exe = raw_exe.canonicalize().unwrap_or_else(|_| raw_exe.clone());
    if let Some(method) = python_env_method(&exe) {
        return (method, None);
    }

    let mut updater = new_updater();
    if updater.load_receipt().is_ok()
        && updater
            .check_receipt_is_for_this_executable()
            .unwrap_or(false)
    {
        return (InstallMethod::Installer, Some(updater));
    }

    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
    // Check both paths in case ~/.cargo itself is a symlink
    if is_cargo_bin(&exe, cargo_home.as_deref()) || is_cargo_bin(&raw_exe, cargo_home.as_deref()) {
        return (InstallMethod::Cargo, None);
    }
    (InstallMethod::Unknown, None)
}

pub fn python_env_method(exe: &Path) -> Option<InstallMethod> {
    let env_root = exe.parent()?.parent()?;
    if !env_root.join("pyvenv.cfg").is_file() {
        return None;
    }
    Some(if env_root.join("uv-receipt.toml").is_file() {
        InstallMethod::UvTool
    } else if env_root.join("pipx_metadata.json").is_file() {
        InstallMethod::Pipx
    } else {
        InstallMethod::Venv
    })
}

pub fn is_cargo_bin(exe: &Path, cargo_home: Option<&Path>) -> bool {
    let Some(bin_dir) = exe.parent() else {
        return false;
    };
    if bin_dir.file_name().is_none_or(|n| n != "bin") {
        return false;
    }
    let in_default = bin_dir
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == ".cargo");
    let in_cargo_home = cargo_home
        .and_then(|h| h.canonicalize().ok())
        .is_some_and(|h| h.join("bin") == bin_dir);
    in_default || in_cargo_home
}
