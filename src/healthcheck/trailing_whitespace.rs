use super::{CheckResult, FileEntry, Finding, file_str_is_ignored, line_is_ignored};
use anyhow::Result;
use std::path::Path;

pub const NAME: &str = "trailing-whitespace";

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
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if file_str_is_ignored(&content, NAME) {
            continue;
        }
        for (i, line) in content.lines().enumerate() {
            if line_is_ignored(line, NAME) {
                continue;
            }
            if line.ends_with(' ') || line.ends_with('\t') {
                findings.push(Finding {
                    file: file.path.clone(),
                    detail: format!("line {}", i + 1),
                });
            }
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "Trailing whitespace",
        passed: findings.is_empty(),
        findings,
    })
}
