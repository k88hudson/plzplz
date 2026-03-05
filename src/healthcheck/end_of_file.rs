use super::{CheckResult, FileEntry, Finding, file_is_ignored};
use anyhow::Result;
use std::path::Path;

pub const NAME: &str = "end-of-file";

pub fn run(base_dir: &Path, files: &[FileEntry]) -> Result<CheckResult> {
    let mut findings = Vec::new();

    for file in files {
        if file.is_binary {
            continue;
        }
        let path = base_dir.join(&file.path);
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        if file_is_ignored(&bytes, NAME) {
            continue;
        }
        if *bytes.last().unwrap() != b'\n' {
            findings.push(Finding {
                file: file.path.clone(),
                detail: "no final newline".to_string(),
            });
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "End of file newline",
        passed: findings.is_empty(),
        findings,
    })
}
