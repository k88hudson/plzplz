use super::{CheckResult, FileEntry, Finding, file_is_ignored, line_bytes_ignored};
use anyhow::Result;
use std::path::Path;

pub const NAME: &str = "merge-conflict";

const CONFLICT_PATTERNS: &[&[u8]] = &[
    b"<<<<<<< ",
    b"======= ",
    b"=======\r\n",
    b"=======\n",
    b">>>>>>> ",
];

// Scans all files (including binary) for merge conflict markers
pub fn run(base_dir: &Path, files: &[FileEntry]) -> Result<CheckResult> {
    let mut findings = Vec::new();

    for file in files {
        let path = base_dir.join(&file.path);
        if !path.is_file() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if file_is_ignored(&bytes, NAME) {
            continue;
        }
        let mut line_number = 1usize;
        let mut line_start = 0;
        for i in 0..bytes.len() {
            if i == line_start {
                for pattern in CONFLICT_PATTERNS {
                    if bytes[i..].starts_with(pattern) {
                        let line_end = bytes[i..]
                            .iter()
                            .position(|&b| b == b'\n')
                            .map(|p| i + p)
                            .unwrap_or(bytes.len());
                        if !line_bytes_ignored(&bytes[i..line_end], NAME) {
                            findings.push(Finding {
                                file: file.path.clone(),
                                detail: format!("line {}", line_number),
                            });
                        }
                        break;
                    }
                }
            }
            if bytes[i] == b'\n' {
                line_number += 1;
                line_start = i + 1;
            }
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "Check merge conflict markers",
        passed: findings.is_empty(),
        findings,
    })
}
