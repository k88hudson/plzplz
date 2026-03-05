use super::{CheckResult, FileEntry, Finding};
use anyhow::Result;
use std::path::Path;

pub const NAME: &str = "large-files";

const MAX_SIZE: u64 = 500 * 1024;

pub fn run(base_dir: &Path, files: &[FileEntry]) -> Result<CheckResult> {
    let mut findings = Vec::new();

    for file in files {
        let path = base_dir.join(&file.path);
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > MAX_SIZE {
                findings.push(Finding {
                    file: file.path.clone(),
                    detail: format!("{}KB", meta.len() / 1024),
                });
            }
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "Check large files (>500KB)",
        passed: findings.is_empty(),
        findings,
    })
}
