use super::{CheckResult, FileEntry, Finding, file_is_ignored};
use anyhow::Result;
use std::path::Path;

pub const NAME: &str = "mixed-line-ending";

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
        if file_is_ignored(&bytes, NAME) {
            continue;
        }
        let has_crlf = bytes.windows(2).any(|w| w == b"\r\n");
        let has_lf = bytes
            .iter()
            .enumerate()
            .any(|(i, &b)| b == b'\n' && (i == 0 || bytes[i - 1] != b'\r'));
        if has_crlf && has_lf {
            findings.push(Finding {
                file: file.path.clone(),
                detail: "mixed \\r\\n and \\n".to_string(),
            });
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "Mixed line endings",
        passed: findings.is_empty(),
        findings,
    })
}
