pub mod check_case_conflict;
pub mod check_large_files;
pub mod check_merge_conflict;
pub mod detect_private_key;
pub mod end_of_file;
pub mod mixed_line_ending;
pub mod trailing_whitespace;

use anyhow::Result;
use std::path::Path;
use std::process::Command;

pub struct Finding {
    pub file: String,
    pub detail: String,
}

pub struct CheckResult {
    pub name: &'static str,
    pub description: &'static str,
    pub passed: bool,
    pub findings: Vec<Finding>,
}

pub struct FileEntry {
    pub path: String,
    pub is_binary: bool,
}

pub const IGNORE_MARKER: &str = "plz:ignore";
pub const IGNORE_FILE_MARKER: &str = "plz:ignore-file";

fn has_ignore(text: &str, marker: &str, rule: &str) -> bool {
    for (i, _) in text.match_indices(marker) {
        let after = &text[i + marker.len()..];
        if after.starts_with('-') {
            continue;
        }
        let rest = after.trim_start();
        if rest.is_empty() || rest.starts_with(rule) {
            return true;
        }
    }
    false
}

pub fn line_is_ignored(line: &str, rule: &str) -> bool {
    has_ignore(line, IGNORE_MARKER, rule)
}

pub fn line_bytes_ignored(line: &[u8], rule: &str) -> bool {
    let line_str = String::from_utf8_lossy(line);
    has_ignore(&line_str, IGNORE_MARKER, rule)
}

pub fn file_is_ignored(content: &[u8], rule: &str) -> bool {
    let first_line = content.split(|&b| b == b'\n').next().unwrap_or(content);
    let line_str = String::from_utf8_lossy(first_line);
    has_ignore(&line_str, IGNORE_FILE_MARKER, rule)
}

pub fn file_str_is_ignored(content: &str, rule: &str) -> bool {
    content
        .lines()
        .next()
        .is_some_and(|l| has_ignore(l, IGNORE_FILE_MARKER, rule))
}

pub fn collect_files(base_dir: &Path) -> Result<Vec<FileEntry>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(base_dir)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let paths: Vec<String> = output
        .stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();

    let entries = paths
        .into_iter()
        .map(|p| {
            let full = base_dir.join(&p);
            let is_binary = is_binary(&full);
            FileEntry { path: p, is_binary }
        })
        .collect();
    Ok(entries)
}

fn is_binary(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let mut buf = [0u8; 1024];
    let Ok(n) = file.take(1024).read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

pub fn run_all_checks(base_dir: &Path) -> Result<Vec<CheckResult>> {
    let files = collect_files(base_dir)?;
    let results = vec![
        check_merge_conflict::run(base_dir, &files)?,
        check_large_files::run(base_dir, &files)?,
        detect_private_key::run(base_dir, &files)?,
        check_case_conflict::run(&files)?,
        trailing_whitespace::run(base_dir, &files)?,
        end_of_file::run(base_dir, &files)?,
        mixed_line_ending::run(base_dir, &files)?,
    ];
    Ok(results)
}

pub fn print_results(results: &[CheckResult]) {
    let green = "\x1b[32m";
    let red = "\x1b[31m";
    let dim = "\x1b[2m";
    let reset = "\x1b[0m";

    for result in results {
        if result.passed {
            eprintln!("{green}✓{reset} {dim}{}{reset}", result.description);
        } else {
            eprintln!(
                "{red}✗{reset} {} {dim}({}){reset}",
                result.description, result.name
            );
            for finding in &result.findings {
                eprintln!("  {dim}{}: {}{reset}", finding.file, finding.detail);
            }
        }
    }

    let any_failed = results.iter().any(|r| !r.passed);
    if any_failed {
        eprintln!(
            "\n{dim}Make sure you really want to do this first, but to suppress a finding,add\nplz:ignore [rule] before a line or plz:ignore-file [rule] to the first line of a file.{reset}"
        );
    }
}

pub fn run_healthcheck(base_dir: &Path) -> Result<()> {
    let results = run_all_checks(base_dir)?;
    print_results(&results);
    let any_failed = results.iter().any(|r| !r.passed);
    if any_failed {
        std::process::exit(1);
    }
    Ok(())
}
