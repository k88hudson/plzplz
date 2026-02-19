use std::fs;
use tempfile::TempDir;

mod config_tests {
    use super::*;
    use plzplz::config::{self, FailHook};

    fn write_config(dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = dir.path().join("plz.toml");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn parse_minimal_config() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.hello]
run = "echo hello"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert!(cfg.tasks.contains_key("hello"));
    }

    #[test]
    fn parse_full_config() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.build]
run = "echo build"
description = "Build the project"

[tasks.test]
run_serial = ["echo one", "echo two"]

[tasks.lint]
run_parallel = ["echo a", "echo b"]
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks.len(), 3);
    }

    #[test]
    fn parse_task_with_run() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.hello]
run = "echo hello"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["hello"].run.as_deref(), Some("echo hello"));
    }

    #[test]
    fn parse_task_with_run_serial() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.multi]
run_serial = ["echo one", "echo two"]
"#,
        );
        let cfg = config::load(&path).unwrap();
        let serial = cfg.tasks["multi"].run_serial.as_ref().unwrap();
        assert_eq!(serial, &["echo one", "echo two"]);
    }

    #[test]
    fn parse_task_with_run_parallel() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.multi]
run_parallel = ["echo a", "echo b"]
"#,
        );
        let cfg = config::load(&path).unwrap();
        let parallel = cfg.tasks["multi"].run_parallel.as_ref().unwrap();
        assert_eq!(parallel, &["echo a", "echo b"]);
    }

    #[test]
    fn parse_task_with_tool_env() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.a]
run = "test"
env = "uv"

[tasks.b]
run = "test"
env = "pnpm"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["a"].tool_env.as_deref(), Some("uv"));
        assert_eq!(cfg.tasks["b"].tool_env.as_deref(), Some("pnpm"));
    }

    #[test]
    fn parse_task_with_dir() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.sub]
run = "echo hi"
dir = "subdir"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["sub"].dir.as_deref(), Some("subdir"));
    }

    #[test]
    fn parse_fail_hook_command() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
fail_hook = "cargo fmt"
"#,
        );
        let cfg = config::load(&path).unwrap();
        match &cfg.tasks["lint"].fail_hook {
            Some(FailHook::Command(cmd)) => assert_eq!(cmd, "cargo fmt"),
            other => panic!("Expected Command, got {:?}", other),
        }
    }

    #[test]
    fn parse_fail_hook_suggest() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
fail_hook = { suggest_command = "plz fix" }
"#,
        );
        let cfg = config::load(&path).unwrap();
        match &cfg.tasks["lint"].fail_hook {
            Some(FailHook::Suggest { suggest_command }) => {
                assert_eq!(suggest_command, "plz fix")
            }
            other => panic!("Expected Suggest, got {:?}", other),
        }
    }

    #[test]
    fn parse_fail_hook_message() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
fail_hook = { message = "Run plz fix first" }
"#,
        );
        let cfg = config::load(&path).unwrap();
        match &cfg.tasks["lint"].fail_hook {
            Some(FailHook::Message(msg)) => assert_eq!(msg, "Run plz fix first"),
            other => panic!("Expected Message, got {:?}", other),
        }
    }

    #[test]
    fn parse_explicit_description() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.build]
run = "cargo build"
description = "Build the project"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(
            cfg.tasks["build"].description.as_deref(),
            Some("Build the project")
        );
    }

    #[test]
    fn parse_comment_description() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
# Build the project
[tasks.build]
run = "cargo build"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(
            cfg.tasks["build"].description.as_deref(),
            Some("Build the project")
        );
    }

    #[test]
    fn extract_comment_single_line() {
        assert_eq!(
            config::extract_comment("# Hello world\n"),
            Some("Hello world".to_string())
        );
    }

    #[test]
    fn extract_comment_multi_line() {
        assert_eq!(
            config::extract_comment("# Line one\n# Line two\n"),
            Some("Line one Line two".to_string())
        );
    }

    #[test]
    fn extract_comment_empty() {
        assert_eq!(config::extract_comment(""), None);
        assert_eq!(config::extract_comment("\n\n"), None);
    }

    #[test]
    fn parse_invalid_toml_errors() {
        let dir = TempDir::new().unwrap();
        let path = write_config(&dir, "this is not valid toml [[[");
        assert!(config::load(&path).is_err());
    }

    #[test]
    fn parse_missing_file_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        assert!(config::load(&path).is_err());
    }
}

mod runner_tests {
    use super::*;
    use plzplz::config;
    use plzplz::runner;

    fn load_config(dir: &TempDir, content: &str) -> config::PlzConfig {
        let path = dir.path().join("plz.toml");
        fs::write(&path, content).unwrap();
        config::load(&path).unwrap()
    }

    #[test]
    fn run_simple_command() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.hello]
run = "echo hello"
"#,
        );
        runner::run_task(&cfg, "hello", dir.path(), false).unwrap();
    }

    #[test]
    fn run_failing_command() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.fail]
run = "false"
"#,
        );
        assert!(runner::run_task(&cfg, "fail", dir.path(), false).is_err());
    }

    #[test]
    fn run_serial_commands() {
        let dir = TempDir::new().unwrap();
        let marker = dir.path().join("marker.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.serial]
run_serial = ["echo first", "touch {}"]
"#,
                marker.display()
            ),
        );
        runner::run_task(&cfg, "serial", dir.path(), false).unwrap();
        assert!(marker.exists());
    }

    #[test]
    fn run_serial_stops_on_failure() {
        let dir = TempDir::new().unwrap();
        let marker = dir.path().join("should_not_exist.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.serial]
run_serial = ["false", "touch {}"]
"#,
                marker.display()
            ),
        );
        assert!(runner::run_task(&cfg, "serial", dir.path(), false).is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn run_parallel_commands() {
        let dir = TempDir::new().unwrap();
        let m1 = dir.path().join("p1.txt");
        let m2 = dir.path().join("p2.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.par]
run_parallel = ["touch {}", "touch {}"]
"#,
                m1.display(),
                m2.display()
            ),
        );
        runner::run_task(&cfg, "par", dir.path(), false).unwrap();
        assert!(m1.exists());
        assert!(m2.exists());
    }

    #[test]
    fn run_parallel_fails_on_error() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.par]
run_parallel = ["true", "false"]
"#,
        );
        assert!(runner::run_task(&cfg, "par", dir.path(), false).is_err());
    }

    #[test]
    fn run_with_working_dir() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("mysubdir");
        fs::create_dir(&subdir).unwrap();
        let out = dir.path().join("pwd_out.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.wd]
run = "pwd > {}"
dir = "mysubdir"
"#,
                out.display()
            ),
        );
        runner::run_task(&cfg, "wd", dir.path(), false).unwrap();
        let content = fs::read_to_string(&out).unwrap();
        assert!(content.trim().ends_with("mysubdir"));
    }

    #[test]
    fn run_with_tool_env_uv() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.uv_test]
run = "echo hello"
env = "uv"
"#,
        );
        assert_eq!(cfg.tasks["uv_test"].tool_env.as_deref(), Some("uv"));
        // Don't assert run success/failure since it depends on uv installation
        let _ = runner::run_task(&cfg, "uv_test", dir.path(), false);
    }

    #[test]
    fn run_with_tool_env_pnpm() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.pnpm_test]
run = "echo hello"
env = "pnpm"
"#,
        );
        assert_eq!(cfg.tasks["pnpm_test"].tool_env.as_deref(), Some("pnpm"));
        let _ = runner::run_task(&cfg, "pnpm_test", dir.path(), false);
    }

    #[test]
    fn run_task_reference() {
        let dir = TempDir::new().unwrap();
        let marker = dir.path().join("ref_marker.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.caller]
run = "plz:target"

[tasks.target]
run = "touch {}"
"#,
                marker.display()
            ),
        );
        runner::run_task(&cfg, "caller", dir.path(), false).unwrap();
        assert!(marker.exists());
    }

    #[test]
    fn run_unknown_task_errors() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.hello]
run = "echo hello"
"#,
        );
        let err = runner::run_task(&cfg, "nonexistent", dir.path(), false);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Unknown task"));
    }

    #[test]
    fn run_serial_with_task_ref() {
        let dir = TempDir::new().unwrap();
        let m1 = dir.path().join("s1.txt");
        let m2 = dir.path().join("s2.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.main]
run_serial = ["touch {}", "plz:helper"]

[tasks.helper]
run = "touch {}"
"#,
                m1.display(),
                m2.display()
            ),
        );
        runner::run_task(&cfg, "main", dir.path(), false).unwrap();
        assert!(m1.exists());
        assert!(m2.exists());
    }

    #[test]
    fn run_parallel_with_task_ref() {
        let dir = TempDir::new().unwrap();
        let m1 = dir.path().join("p1.txt");
        let m2 = dir.path().join("p2.txt");
        let cfg = load_config(
            &dir,
            &format!(
                r#"
[tasks.main]
run_parallel = ["touch {}", "plz:helper"]

[tasks.helper]
run = "touch {}"
"#,
                m1.display(),
                m2.display()
            ),
        );
        runner::run_task(&cfg, "main", dir.path(), false).unwrap();
        assert!(m1.exists());
        assert!(m2.exists());
    }
}

mod init_tests {
    use super::*;
    use plzplz::init;

    #[test]
    fn detect_project_type_rust() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        // Set PLZ_CONFIG_DIR to a nonexistent path to avoid loading user defaults
        unsafe {
            std::env::set_var("PLZ_CONFIG_DIR", dir.path().join("no_config"));
        }
        let types = init::detect_project_types(dir.path());
        assert!(types.iter().any(|t| t.name == "rust"));
    }

    #[test]
    fn detect_project_type_pnpm() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        unsafe {
            std::env::set_var("PLZ_CONFIG_DIR", dir.path().join("no_config"));
        }
        let types = init::detect_project_types(dir.path());
        assert!(types.iter().any(|t| t.name == "pnpm"));
    }

    #[test]
    fn detect_project_type_uv() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("uv.lock"), "").unwrap();
        unsafe {
            std::env::set_var("PLZ_CONFIG_DIR", dir.path().join("no_config"));
        }
        let types = init::detect_project_types(dir.path());
        assert!(types.iter().any(|t| t.name == "uv"));
    }

    #[test]
    fn detect_project_type_multiple() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        unsafe {
            std::env::set_var("PLZ_CONFIG_DIR", dir.path().join("no_config"));
        }
        let types = init::detect_project_types(dir.path());
        assert!(types.len() >= 2);
        assert!(types.iter().any(|t| t.name == "rust"));
        assert!(types.iter().any(|t| t.name == "pnpm"));
    }

    #[test]
    fn add_suffix_renames_tasks() {
        let toml = r#"
[tasks.dev]
run = "cargo run"

[tasks.build]
run = "cargo build"
"#;
        let (_, tasks) = init::parse_default(toml).unwrap();
        let result = init::add_suffix_to_toml(toml, "rust", &tasks);
        assert!(result.contains("[tasks.dev-rust]"));
        assert!(result.contains("[tasks.build-rust]"));
        assert!(!result.contains("[tasks.dev]"));
        assert!(!result.contains("[tasks.build]"));
    }

    #[test]
    fn add_suffix_updates_plz_references() {
        let toml = r#"
[tasks.lint]
run_serial = ["cargo clippy", "cargo fmt --check"]
fail_hook = { suggest_command = "plz fix" }

[tasks.fix]
run_serial = ["cargo fmt", "cargo clippy --fix --allow-dirty"]
"#;
        let (_, tasks) = init::parse_default(toml).unwrap();
        let result = init::add_suffix_to_toml(toml, "rust", &tasks);
        assert!(result.contains("plz fix-rust"), "got:\n{result}");
    }

    #[test]
    fn add_suffix_not_applied_for_single_type() {
        let toml = r#"
[tasks.dev]
run = "cargo run"
"#;
        let (_, tasks) = init::parse_default(toml).unwrap();
        // When there's only one type, callers should not call add_suffix_to_toml.
        // Verify the original names are intact.
        assert!(tasks.iter().any(|(n, _)| n == "dev"));
    }

    #[test]
    fn detect_project_type_none() {
        let dir = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("PLZ_CONFIG_DIR", dir.path().join("no_config"));
        }
        let types = init::detect_project_types(dir.path());
        assert!(types.is_empty());
    }

    #[test]
    fn parse_default_rust() {
        let rust_toml = init::DEFAULTS
            .iter()
            .find(|(name, _, _)| *name == "rust")
            .unwrap()
            .2;
        let (_, tasks) = init::parse_default(rust_toml).unwrap();
        let names: Vec<&str> = tasks.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"test"));
        assert!(names.contains(&"lint"));
    }

    #[test]
    fn parse_default_pnpm() {
        let pnpm_toml = init::DEFAULTS
            .iter()
            .find(|(name, _, _)| *name == "pnpm")
            .unwrap()
            .2;
        let (_, tasks) = init::parse_default(pnpm_toml).unwrap();
        let names: Vec<&str> = tasks.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"dev"));
        assert!(names.contains(&"build"));
        assert!(names.contains(&"test"));
    }

    #[test]
    fn parse_default_uv() {
        let uv_toml = init::DEFAULTS
            .iter()
            .find(|(name, _, _)| *name == "uv")
            .unwrap()
            .2;
        let (_, tasks) = init::parse_default(uv_toml).unwrap();
        let names: Vec<&str> = tasks.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"test"));
        assert!(names.contains(&"build"));
        assert!(names.contains(&"lint"));
    }

    #[test]
    fn merge_defaults_override_task() {
        let embedded = r#"
[tasks.build]
run = "cargo build"

[tasks.test]
run = "cargo test"
"#;
        let user = r#"
[tasks.build]
run = "cargo build --release"
"#;
        let (doc, _) = init::merge_defaults(embedded, user).unwrap();
        let tasks = doc.get("tasks").unwrap().as_table().unwrap();
        let build = tasks.get("build").unwrap().as_table().unwrap();
        assert_eq!(
            build.get("run").unwrap().as_str().unwrap(),
            "cargo build --release"
        );
    }

    #[test]
    fn merge_defaults_blank_removes_task() {
        let embedded = r#"
[tasks.build]
run = "cargo build"

[tasks.test]
run = "cargo test"
"#;
        let user = r#"
[tasks.test]
"#;
        let (doc, tasks) = init::merge_defaults(embedded, user).unwrap();
        let table = doc.get("tasks").unwrap().as_table().unwrap();
        assert!(table.get("build").is_some());
        assert!(table.get("test").is_none());
        assert!(!tasks.iter().any(|(n, _)| n == "test"));
    }

    #[test]
    fn merge_defaults_preserves_unmodified() {
        let embedded = r#"
[tasks.build]
run = "cargo build"

[tasks.test]
run = "cargo test"

[tasks.lint]
run = "cargo clippy"
"#;
        let user = r#"
[tasks.build]
run = "cargo build --release"
"#;
        let (doc, _) = init::merge_defaults(embedded, user).unwrap();
        let table = doc.get("tasks").unwrap().as_table().unwrap();
        assert!(table.get("test").is_some());
        assert!(table.get("lint").is_some());
        let test = table.get("test").unwrap().as_table().unwrap();
        assert_eq!(test.get("run").unwrap().as_str().unwrap(), "cargo test");
    }

    #[test]
    fn detect_project_type_rust_with_commented_out_defaults() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        // Create a user defaults file with all entries commented out (scaffolded state)
        let defaults_dir = dir.path().join("config/defaults");
        fs::create_dir_all(&defaults_dir).unwrap();
        fs::write(
            defaults_dir.join("rust.plz.toml"),
            r#"# These defaults extend the built-ins.
# Uncomment to override. Leave blank to omit from the list.

# [tasks.build]
# run = "cargo build"

# [tasks.test]
# run = "cargo test"
"#,
        )
        .unwrap();
        unsafe {
            std::env::set_var("PLZ_CONFIG_DIR", dir.path().join("config"));
        }
        let types = init::detect_project_types(dir.path());
        assert!(
            types.iter().any(|t| t.name == "rust"),
            "Should detect Cargo.toml even when user defaults are all commented out"
        );
    }

    #[test]
    fn generate_scaffold_comments_out_content() {
        let input = r#"[tasks.build]
run = "cargo build"

# Run tests
[tasks.test]
run = "cargo test"
"#;
        let result = init::generate_scaffold(input);
        // Non-comment lines should be prefixed with "# "
        assert!(result.contains("# [tasks.build]"));
        assert!(result.contains("# run = \"cargo build\""));
        // Already-comment lines should NOT get double-prefixed
        assert!(result.contains("# Run tests"));
        assert!(!result.contains("# # Run tests"));
    }
}

mod hooks_tests {
    use super::*;
    use plzplz::config;
    use plzplz::hooks;

    fn write_config(dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = dir.path().join("plz.toml");
        fs::write(&path, content).unwrap();
        path
    }

    fn init_git_repo(dir: &TempDir) {
        fs::create_dir_all(dir.path().join(".git/hooks")).unwrap();
    }

    #[test]
    fn parse_git_hook_field() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["lint"].git_hook.as_deref(), Some("pre-commit"));
    }

    #[test]
    fn reject_invalid_git_hook() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "not-a-real-hook"
"#,
        );
        let err = config::load(&path).unwrap_err();
        assert!(err.to_string().contains("invalid git_hook"), "got: {err}");
    }

    #[test]
    fn tasks_by_stage_groups_correctly() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"

[tasks.fmt]
run = "cargo fmt --check"
git_hook = "pre-commit"

[tasks.test]
run = "cargo test"
git_hook = "pre-push"

[tasks.build]
run = "cargo build"
"#,
        );
        let cfg = config::load(&path).unwrap();
        let stages = hooks::tasks_by_stage(&cfg);
        assert_eq!(stages.len(), 2);
        assert_eq!(stages["pre-commit"].len(), 2);
        assert!(stages["pre-commit"].contains(&"lint".to_string()));
        assert!(stages["pre-commit"].contains(&"fmt".to_string()));
        assert_eq!(stages["pre-push"], vec!["test"]);
    }

    #[test]
    fn install_creates_hook_scripts() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"

[tasks.test]
run = "cargo test"
git_hook = "pre-push"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::install(&cfg, dir.path()).unwrap();

        let pre_commit = dir.path().join(".git/hooks/pre-commit");
        let pre_push = dir.path().join(".git/hooks/pre-push");
        assert!(pre_commit.exists());
        assert!(pre_push.exists());

        let content = fs::read_to_string(&pre_commit).unwrap();
        assert!(content.contains("plz:managed"));
        assert!(content.contains("hooks run pre-commit"));

        let content = fs::read_to_string(&pre_push).unwrap();
        assert!(content.contains("hooks run pre-push"));
    }

    #[test]
    fn install_skips_non_managed_hooks() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);

        let existing_hook = dir.path().join(".git/hooks/pre-commit");
        fs::write(&existing_hook, "#!/bin/sh\necho my custom hook\n").unwrap();

        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::install(&cfg, dir.path()).unwrap();

        let content = fs::read_to_string(&existing_hook).unwrap();
        assert!(
            content.contains("my custom hook"),
            "Should not overwrite user hook"
        );
        assert!(!content.contains("plz:managed"));
    }

    #[test]
    fn install_overwrites_managed_hooks() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);

        let hook_path = dir.path().join(".git/hooks/pre-commit");
        fs::write(
            &hook_path,
            "#!/bin/sh\n# plz:managed - do not edit\nplz hooks run pre-commit\n",
        )
        .unwrap();

        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::install(&cfg, dir.path()).unwrap();

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("plz:managed"));
    }

    #[test]
    fn uninstall_removes_managed_hooks() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);

        let hook_path = dir.path().join(".git/hooks/pre-commit");
        fs::write(
            &hook_path,
            "#!/bin/sh\n# plz:managed - do not edit\nplz hooks run pre-commit\n",
        )
        .unwrap();

        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::uninstall(&cfg, dir.path()).unwrap();

        assert!(!hook_path.exists());
    }

    #[test]
    fn uninstall_skips_non_managed_hooks() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);

        let hook_path = dir.path().join(".git/hooks/pre-commit");
        fs::write(&hook_path, "#!/bin/sh\necho custom\n").unwrap();

        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::uninstall(&cfg, dir.path()).unwrap();

        assert!(hook_path.exists(), "Should not remove non-managed hook");
    }

    #[test]
    fn find_git_hooks_dir_walks_up() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".git/hooks")).unwrap();
        let subdir = dir.path().join("a/b/c");
        fs::create_dir_all(&subdir).unwrap();

        let result = hooks::find_git_hooks_dir(&subdir).unwrap();
        assert_eq!(result, dir.path().join(".git/hooks"));
    }

    #[test]
    fn find_git_hooks_dir_fails_without_git() {
        let dir = TempDir::new().unwrap();
        assert!(hooks::find_git_hooks_dir(dir.path()).is_err());
    }

    #[test]
    fn run_stage_executes_tasks() {
        let dir = TempDir::new().unwrap();
        let marker = dir.path().join("hook_ran.txt");
        let path = write_config(
            &dir,
            &format!(
                r#"
[tasks.check]
run = "touch {}"
git_hook = "pre-commit"
"#,
                marker.display()
            ),
        );
        let cfg = config::load(&path).unwrap();
        hooks::run_stage(&cfg, "pre-commit", dir.path(), false).unwrap();
        assert!(marker.exists());
    }

    #[test]
    fn hook_script_contains_skip_and_fallback() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::install(&cfg, dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(".git/hooks/pre-commit")).unwrap();
        assert!(content.contains("PLZ_SKIP_HOOKS"), "missing skip env var");
        assert!(
            content.contains("command -v plz"),
            "missing not-in-PATH fallback"
        );
        assert!(
            !content.contains("\"$@\""),
            "should not pass git args to tasks"
        );
        assert!(
            content.contains("plz:hooks_version="),
            "missing hooks version marker"
        );
    }

    #[test]
    fn install_upgrades_outdated_hooks() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();

        // Write a v1 hook (no version marker)
        let hook_path = dir.path().join(".git/hooks/pre-commit");
        fs::create_dir_all(hook_path.parent().unwrap()).unwrap();
        fs::write(
            &hook_path,
            "#!/bin/sh\n# plz:managed - do not edit\nplz hooks run pre-commit \"$@\"\n",
        )
        .unwrap();

        // Install should overwrite it since it's managed and outdated
        hooks::install(&cfg, dir.path()).unwrap();
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(
            content.contains("plz:hooks_version="),
            "should have been upgraded"
        );
        assert!(!content.contains("\"$@\""), "should not have $@");
    }

    #[test]
    fn run_stage_unconfigured_stage_succeeds() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.check]
run = "echo hi"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        // No tasks configured for pre-push — should succeed silently
        hooks::run_stage(&cfg, "pre-push", dir.path(), false).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn install_sets_executable_permission() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir);
        let path = write_config(
            &dir,
            r#"
[tasks.lint]
run = "cargo clippy"
git_hook = "pre-commit"
"#,
        );
        let cfg = config::load(&path).unwrap();
        hooks::install(&cfg, dir.path()).unwrap();

        let hook_path = dir.path().join(".git/hooks/pre-commit");
        let perms = fs::metadata(&hook_path).unwrap().permissions();
        assert!(perms.mode() & 0o111 != 0, "Hook should be executable");
    }
}

mod cli_tests {
    use assert_cmd::Command;
    use predicates::prelude::*;
    use std::fs;
    use tempfile::TempDir;

    #[allow(deprecated)]
    fn plz() -> Command {
        Command::cargo_bin("plz").unwrap()
    }

    #[test]
    fn cli_no_args_no_config_shows_help() {
        let dir = TempDir::new().unwrap();
        plz()
            .current_dir(dir.path())
            .assert()
            .success()
            .stdout(predicate::str::contains("Usage:"));
    }

    #[test]
    fn cli_run_task() {
        let dir = TempDir::new().unwrap();
        let marker = dir.path().join("ran.txt");
        fs::write(
            dir.path().join("plz.toml"),
            format!(
                r#"
[tasks.hello]
run = "touch {}"
"#,
                marker.display()
            ),
        )
        .unwrap();

        plz()
            .arg("hello")
            .current_dir(dir.path())
            .assert()
            .success();
        assert!(marker.exists());
    }

    #[test]
    fn cli_run_unknown_task_errors() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("plz.toml"),
            r#"
[tasks.hello]
run = "echo hello"
"#,
        )
        .unwrap();

        plz()
            .arg("nonexistent")
            .current_dir(dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("Unknown task"));
    }

    #[test]
    fn cli_non_interactive_when_no_tty() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("plz.toml"),
            r#"
[tasks.hello]
run = "echo hello"
"#,
        )
        .unwrap();

        // Without a TTY, plz should detect non-interactive mode
        plz()
            .current_dir(dir.path())
            .assert()
            .stderr(predicate::str::contains(
                "Skipping interactive prompts: stdin is not a terminal",
            ));
    }

    #[test]
    fn cli_non_interactive_flag() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("plz.toml"),
            r#"
[tasks.hello]
run = "echo hello"
"#,
        )
        .unwrap();

        plz()
            .arg("--no-interactive")
            .current_dir(dir.path())
            .assert()
            .stderr(predicate::str::contains("Skipping interactive prompts"));
    }

    #[test]
    fn cli_init_already_exists() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("plz.toml"), "[tasks]").unwrap();

        plz()
            .arg("init")
            .current_dir(dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains("already exists"));
    }
}
