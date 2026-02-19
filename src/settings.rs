use std::env;
use std::path::{Path, PathBuf};

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
