pub mod check_case_conflict;
pub mod check_large_files;
pub mod check_merge_conflict;
pub mod detect_private_key;
pub mod end_of_file;
pub mod mixed_line_ending;
pub mod trailing_whitespace;

use anyhow::{Context, Result, bail};
use glob::Pattern;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use crate::config::{self, HealthcheckSection};

pub const ALL_CHECKS: &[&str] = &[
    check_merge_conflict::NAME,
    check_large_files::NAME,
    detect_private_key::NAME,
    check_case_conflict::NAME,
    trailing_whitespace::NAME,
    end_of_file::NAME,
    mixed_line_ending::NAME,
];

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
    collect_files_inner(base_dir, false)
}

pub fn collect_staged_files(base_dir: &Path) -> Result<Vec<FileEntry>> {
    collect_files_inner(base_dir, true)
}

fn collect_files_inner(base_dir: &Path, staged_only: bool) -> Result<Vec<FileEntry>> {
    let args: &[&str] = if staged_only {
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
        ]
    } else {
        &["ls-files", "-z"]
    };
    let output = Command::new("git")
        .args(args)
        .current_dir(base_dir)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args[0],
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

pub fn load_section(base_dir: &Path) -> Result<Option<HealthcheckSection>> {
    let Some(config_path) = ["plz.toml", ".plz.toml"]
        .iter()
        .map(|n| base_dir.join(n))
        .find(|p| p.exists())
    else {
        return Ok(None);
    };
    Ok(config::load(&config_path)?.healthcheck)
}

fn exclude_patterns(section: Option<&HealthcheckSection>) -> Result<Vec<Pattern>> {
    let Some(hc) = section else {
        return Ok(Vec::new());
    };
    hc.exclude
        .iter()
        .map(|s| {
            Pattern::new(s).with_context(|| format!("Invalid healthcheck exclude pattern \"{s}\""))
        })
        .collect()
}

fn validate_check_names(names: &[String], flag: &str) -> Result<Vec<&'static str>> {
    let mut out = Vec::with_capacity(names.len());
    for n in names {
        let trimmed = n.trim();
        if trimmed.is_empty() {
            continue;
        }
        match ALL_CHECKS.iter().find(|c| **c == trimmed) {
            Some(c) => out.push(*c),
            None => bail!(
                "{flag}: unknown check \"{trimmed}\". Valid checks: {}",
                ALL_CHECKS.join(", ")
            ),
        }
    }
    Ok(out)
}

/// Resolve which checks should run. CLI flags override config.
/// `--only` wins over `enable`; `--skip` wins over `disable`.
/// Returns an error if both `--only` and `--skip` are set.
pub fn resolve_active_checks(
    section: Option<&HealthcheckSection>,
    cli_only: &[String],
    cli_skip: &[String],
) -> Result<HashSet<&'static str>> {
    if !cli_only.is_empty() && !cli_skip.is_empty() {
        bail!("--only and --skip cannot be used together");
    }

    if !cli_only.is_empty() {
        let only = validate_check_names(cli_only, "--only")?;
        if only.is_empty() {
            bail!("--only: no check names provided");
        }
        return Ok(only.into_iter().collect());
    }
    if !cli_skip.is_empty() {
        let skip: HashSet<&'static str> = validate_check_names(cli_skip, "--skip")?
            .into_iter()
            .collect();
        return Ok(ALL_CHECKS
            .iter()
            .copied()
            .filter(|c| !skip.contains(c))
            .collect());
    }

    if let Some(hc) = section {
        if let Some(ref enable) = hc.enable {
            // Config is validated at load time; names must be known.
            return Ok(enable
                .iter()
                .filter_map(|n| ALL_CHECKS.iter().find(|c| **c == n.as_str()).copied())
                .collect());
        }
        if let Some(ref disable) = hc.disable {
            let skip: HashSet<&str> = disable.iter().map(|s| s.as_str()).collect();
            return Ok(ALL_CHECKS
                .iter()
                .copied()
                .filter(|c| !skip.contains(*c))
                .collect());
        }
    }
    Ok(ALL_CHECKS.iter().copied().collect())
}

pub fn run_all_checks(
    base_dir: &Path,
    staged_only: bool,
    section: Option<&HealthcheckSection>,
    active: &HashSet<&'static str>,
) -> Result<Vec<CheckResult>> {
    let exclude_patterns = exclude_patterns(section)?;
    let collected = if staged_only {
        collect_staged_files(base_dir)?
    } else {
        collect_files(base_dir)?
    };
    let files: Vec<FileEntry> = collected
        .into_iter()
        .filter(|f| !is_excluded(&f.path, &exclude_patterns))
        .collect();

    let mut results = Vec::new();
    if active.contains(check_merge_conflict::NAME) {
        results.push(check_merge_conflict::run(base_dir, &files)?);
    }
    if active.contains(check_large_files::NAME) {
        results.push(check_large_files::run(base_dir, &files)?);
    }
    if active.contains(detect_private_key::NAME) {
        results.push(detect_private_key::run(base_dir, &files)?);
    }
    if active.contains(check_case_conflict::NAME) {
        results.push(check_case_conflict::run(&files)?);
    }
    if active.contains(trailing_whitespace::NAME) {
        results.push(trailing_whitespace::run(base_dir, &files)?);
    }
    if active.contains(end_of_file::NAME) {
        results.push(end_of_file::run(base_dir, &files)?);
    }
    if active.contains(mixed_line_ending::NAME) {
        results.push(mixed_line_ending::run(base_dir, &files)?);
    }
    Ok(results)
}

fn is_excluded(path: &str, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|p| p.matches(path))
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

pub fn run_healthcheck(
    base_dir: &Path,
    staged_only: bool,
    only: &[String],
    skip: &[String],
) -> Result<()> {
    let section = load_section(base_dir)?;
    let active = resolve_active_checks(section.as_ref(), only, skip)?;
    let results = run_all_checks(base_dir, staged_only, section.as_ref(), &active)?;
    print_results(&results);
    let any_failed = results.iter().any(|r| !r.passed);
    if any_failed {
        std::process::exit(1);
    }
    Ok(())
}
