use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::settings;

const CHECK_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60; // 7 days
const CHECK_TIMEOUT: Duration = Duration::from_millis(500);

struct CachedCheck {
    last_check: u64,
    latest_version: Option<String>,
}

fn cache_path() -> Option<std::path::PathBuf> {
    settings::config_dir().map(|d| d.join("update-check"))
}

fn read_cache(path: &std::path::Path) -> CachedCheck {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => {
            return CachedCheck {
                last_check: 0,
                latest_version: None,
            };
        }
    };
    let doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(d) => d,
        Err(_) => {
            return CachedCheck {
                last_check: 0,
                latest_version: None,
            };
        }
    };
    CachedCheck {
        last_check: doc
            .get("last_check")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64,
        latest_version: doc
            .get("latest_version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

fn write_cache(path: &std::path::Path, latest_version: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let content = format!("last_check = {now}\nlatest_version = \"{latest_version}\"\n");
    let _ = std::fs::write(path, content);
}

fn fetch_latest_version() -> Option<String> {
    use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

    let mut updater = AxoUpdater::new_for("plzplz");
    updater.set_release_source(ReleaseSource {
        release_type: ReleaseSourceType::GitHub,
        owner: "k88hudson".to_string(),
        name: "plzplz".to_string(),
        app_name: "plzplz".to_string(),
    });
    let current = env!("CARGO_PKG_VERSION");
    updater.set_current_version(current.parse().ok()?).ok()?;

    // query_new_version is async; build a small runtime like axoupdater does internally
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let version = rt.block_on(updater.query_new_version()).ok()??;
    Some(version.to_string())
}

fn print_hint(latest: &str) {
    let current = env!("CARGO_PKG_VERSION");
    if let (Ok(curr), Ok(lat)) = (
        semver::Version::parse(current),
        semver::Version::parse(latest),
    ) {
        if lat > curr {
            eprintln!(
                "\x1b[2mA new version of plz is available (v{latest}). Run `plz update` to upgrade.\x1b[0m"
            );
        }
    }
}

pub fn maybe_print_update_hint() {
    if std::env::var_os("PLZ_COMMAND").is_some() {
        return;
    }
    if is_ci::cached() {
        return;
    }

    let s = settings::load();
    if !s.check_for_updates {
        return;
    }

    let Some(path) = cache_path() else { return };
    let cache = read_cache(&path);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let needs_fetch = now.saturating_sub(cache.last_check) >= CHECK_INTERVAL_SECS;

    if needs_fetch {
        let cache_path = path.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(fetch_latest_version());
        });

        match rx.recv_timeout(CHECK_TIMEOUT) {
            Ok(Some(version)) => {
                write_cache(&cache_path, &version);
                print_hint(&version);
            }
            Ok(None) => {
                // No version found (e.g. no releases) — update timestamp to avoid retrying immediately
                let current = env!("CARGO_PKG_VERSION");
                write_cache(&cache_path, current);
            }
            Err(_) => {
                // Timeout — print from stale cache if available
                if let Some(ref v) = cache.latest_version {
                    print_hint(v);
                }
            }
        }
    } else if let Some(ref v) = cache.latest_version {
        print_hint(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_cache_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let c = read_cache(&dir.path().join("nonexistent"));
        assert_eq!(c.last_check, 0);
        assert!(c.latest_version.is_none());
    }

    #[test]
    fn read_write_cache_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("update-check");
        write_cache(&path, "1.2.3");
        let c = read_cache(&path);
        assert!(c.last_check > 0);
        assert_eq!(c.latest_version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn print_hint_no_panic_on_equal_version() {
        let current = env!("CARGO_PKG_VERSION");
        print_hint(current);
    }
}
