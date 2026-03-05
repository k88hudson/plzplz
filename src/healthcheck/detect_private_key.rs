use super::{CheckResult, FileEntry, Finding, file_is_ignored, line_bytes_ignored};
use anyhow::Result;
use std::path::Path;

pub const NAME: &str = "private-key";

const PRIVATE_KEY_MARKERS: &[&[u8]] = &[
    b"BEGIN RSA PRIVATE KEY",            // plz:ignore private-key
    b"BEGIN DSA PRIVATE KEY",            // plz:ignore private-key
    b"BEGIN EC PRIVATE KEY",             // plz:ignore private-key
    b"BEGIN OPENSSH PRIVATE KEY",        // plz:ignore private-key
    b"BEGIN PRIVATE KEY",                // plz:ignore private-key
    b"BEGIN ENCRYPTED PRIVATE KEY",      // plz:ignore private-key
    b"BEGIN SSH2 ENCRYPTED PRIVATE KEY", // plz:ignore private-key
    b"BEGIN PGP PRIVATE KEY BLOCK",      // plz:ignore private-key
    b"BEGIN OpenVPN Static key V1",      // plz:ignore private-key
    b"PuTTY-User-Key-File-2",            // plz:ignore private-key
];

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
        let Ok(content) = std::fs::read(&path) else {
            continue;
        };
        if file_is_ignored(&content, NAME) {
            continue;
        }
        for (line_num, line) in content.split(|&b| b == b'\n').enumerate() {
            if line_bytes_ignored(line, NAME) {
                continue;
            }
            if PRIVATE_KEY_MARKERS
                .iter()
                .any(|m| memchr_find(line, m).is_some())
            {
                findings.push(Finding {
                    file: file.path.clone(),
                    detail: format!("line {}", line_num + 1),
                });
                break;
            }
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "Detect private keys",
        passed: findings.is_empty(),
        findings,
    })
}

fn memchr_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
