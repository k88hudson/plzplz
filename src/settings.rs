use std::env;
use std::path::{Path, PathBuf};

pub struct SettingEntry {
    pub key: &'static str,
    pub description: &'static str,
    pub default: bool,
}

pub const ALL_SETTINGS: &[SettingEntry] = &[SettingEntry {
    key: "show_hints",
    description: "Show helpful tips and suggestions",
    default: true,
}];

#[derive(Debug)]
pub struct Settings {
    pub show_hints: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { show_hints: true }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    if let Ok(dir) = env::var("PLZ_CONFIG_DIR") {
        Some(PathBuf::from(dir))
    } else {
        dirs::home_dir().map(|h| h.join(".plz"))
    }
}

pub fn settings_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.toml"))
}

pub fn load() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    load_from(&path)
}

/// Returns (value, is_user_set) for each setting key
pub fn load_raw(path: &Path) -> Vec<(&'static str, bool, bool)> {
    let doc = std::fs::read_to_string(path)
        .ok()
        .and_then(|c| c.parse::<toml_edit::DocumentMut>().ok());

    ALL_SETTINGS
        .iter()
        .map(|entry| {
            let (value, is_user_set) = doc
                .as_ref()
                .and_then(|d| d.get(entry.key))
                .and_then(|v| v.as_bool())
                .map(|v| (v, true))
                .unwrap_or((entry.default, false));
            (entry.key, value, is_user_set)
        })
        .collect()
}

pub fn save(path: &Path, values: &[(&str, bool)]) -> anyhow::Result<()> {
    let mut lines = Vec::new();
    for (key, value) in values {
        lines.push(format!("{key} = {value}"));
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

pub fn load_from(path: &Path) -> Settings {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Settings::default();
    };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return Settings::default();
    };

    let show_hints = doc
        .get("show_hints")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Settings { show_hints }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_returns_defaults() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = load_from(&dir.path().join("nonexistent/settings.toml"));
        assert!(s.show_hints);
    }

    #[test]
    fn load_show_hints_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "show_hints = false\n").unwrap();
        let s = load_from(&path);
        assert!(!s.show_hints);
    }

    #[test]
    fn load_show_hints_default_true() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "").unwrap();
        let s = load_from(&path);
        assert!(s.show_hints);
    }
}
