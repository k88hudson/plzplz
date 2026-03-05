use super::{CheckResult, FileEntry, Finding};
use anyhow::Result;
use std::collections::HashMap;

pub const NAME: &str = "case-conflict";

pub fn run(files: &[FileEntry]) -> Result<CheckResult> {
    let mut findings = Vec::new();

    let mut by_lower: HashMap<String, Vec<String>> = HashMap::new();
    for file in files {
        by_lower
            .entry(file.path.to_lowercase())
            .or_default()
            .push(file.path.clone());
    }

    let mut conflicts: Vec<_> = by_lower.into_iter().filter(|(_, v)| v.len() > 1).collect();
    conflicts.sort_by(|a, b| a.0.cmp(&b.0));

    for (_, group) in conflicts {
        for file in &group {
            let others: Vec<&str> = group
                .iter()
                .filter(|f| *f != file)
                .map(|f| f.as_str())
                .collect();
            findings.push(Finding {
                file: file.clone(),
                detail: format!("conflicts with {}", others.join(", ")),
            });
        }
    }

    Ok(CheckResult {
        name: NAME,
        description: "Check case conflicts",
        passed: findings.is_empty(),
        findings,
    })
}
