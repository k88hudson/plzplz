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
    fn parse_task_with_npm_env() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[tasks.a]
run = "test"
env = "npm"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["a"].tool_env.as_deref(), Some("npm"));
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
    fn extends_env_applies_to_all_tasks() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[extends]
env = "pnpm"

[tasks.build]
run = "vite build"

[tasks.dev]
run = "vite dev"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["build"].tool_env.as_deref(), Some("pnpm"));
        assert_eq!(cfg.tasks["dev"].tool_env.as_deref(), Some("pnpm"));
    }

    #[test]
    fn extends_dir_applies_to_all_tasks() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[extends]
dir = "packages/web"

[tasks.build]
run = "echo build"

[tasks.dev]
run = "echo dev"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["build"].dir.as_deref(), Some("packages/web"));
        assert_eq!(cfg.tasks["dev"].dir.as_deref(), Some("packages/web"));
    }

    #[test]
    fn per_task_overrides_extends() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[extends]
env = "pnpm"
dir = "packages/web"

[tasks.build]
run = "vite build"

[tasks.api]
run = "node server.js"
env = "npm"
dir = "packages/api"
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["build"].tool_env.as_deref(), Some("pnpm"));
        assert_eq!(cfg.tasks["build"].dir.as_deref(), Some("packages/web"));
        assert_eq!(cfg.tasks["api"].tool_env.as_deref(), Some("npm"));
        assert_eq!(cfg.tasks["api"].dir.as_deref(), Some("packages/api"));
    }

    #[test]
    fn empty_string_opts_out_of_extends() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
[extends]
env = "pnpm"
dir = "packages/web"

[tasks.build]
run = "vite build"

[tasks.plain]
run = "echo hi"
env = ""
dir = ""
"#,
        );
        let cfg = config::load(&path).unwrap();
        assert_eq!(cfg.tasks["build"].tool_env.as_deref(), Some("pnpm"));
        assert_eq!(cfg.tasks["build"].dir.as_deref(), Some("packages/web"));
        assert_eq!(cfg.tasks["plain"].tool_env, None);
        assert_eq!(cfg.tasks["plain"].dir, None);
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
    fn run_with_tool_env_npm() {
        let dir = TempDir::new().unwrap();
        let cfg = load_config(
            &dir,
            r#"
[tasks.npm_test]
run = "echo hello"
env = "npm"
"#,
        );
        assert_eq!(cfg.tasks["npm_test"].tool_env.as_deref(), Some("npm"));
        let _ = runner::run_task(&cfg, "npm_test", dir.path(), false);
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
        assert!(err.unwrap_err().to_string().contains("isn't a task"));
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
    use plzplz::templates;

    #[test]
    fn detect_environment_rust() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.contains(&"rust".to_string()));
    }

    #[test]
    fn detect_environment_pnpm() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.contains(&"pnpm".to_string()));
    }

    #[test]
    fn detect_environment_npm() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package-lock.json"), "{}").unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.contains(&"npm".to_string()));
    }

    #[test]
    fn detect_environment_npm_from_package_json() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.contains(&"npm".to_string()));
    }

    #[test]
    fn detect_environment_uv() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("uv.lock"), "").unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.contains(&"uv".to_string()));
    }

    #[test]
    fn detect_environment_multiple() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.len() >= 2);
        assert!(detected.contains(&"rust".to_string()));
        assert!(detected.contains(&"pnpm".to_string()));
    }

    #[test]
    fn detect_environment_none() {
        let dir = TempDir::new().unwrap();
        let envs = templates::load_environments();
        let detected = templates::detect_environments(dir.path(), &envs);
        assert!(detected.is_empty());
    }

    #[test]
    fn pnpm_is_alternative_to_npm() {
        let envs = templates::load_environments();
        let pnpm = &envs["pnpm"];
        assert!(pnpm.alternative_to.contains(&"npm".to_string()));
    }

    #[test]
    fn load_embedded_templates() {
        let all = templates::load_templates(None);
        let names: Vec<&str> = all.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"pnpm"));
        assert!(names.contains(&"npm"));
        assert!(names.contains(&"uv"));
    }

    #[test]
    fn template_has_metadata() {
        let all = templates::load_templates(None);
        let rust = all.iter().find(|t| t.name == "rust").unwrap();
        assert_eq!(rust.env, "rust");
        assert!(!rust.description.is_empty());
    }

    #[test]
    fn strip_template_section() {
        let input = r#"[template]
description = "Test"
env = "rust"

# Build
[tasks.build]
run = "cargo build"
"#;
        let result = templates::strip_template_section(input);
        assert!(!result.contains("[template]"));
        assert!(!result.contains("description = \"Test\""));
        assert!(result.contains("[tasks.build]"));
        assert!(result.contains("cargo build"));
    }

    #[test]
    fn load_embedded_snippets() {
        let all = templates::load_snippets();
        let env_names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
        assert!(env_names.contains(&"general"));
        assert!(env_names.contains(&"rust"));
        assert!(env_names.contains(&"pnpm"));
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
    fn embedded_templates_parse_correctly() {
        let all = templates::load_templates(None);
        for t in &all {
            let stripped = templates::strip_template_section(&t.content);
            let parsed = init::parse_default(&stripped);
            assert!(parsed.is_some(), "Template {} failed to parse", t.name);
            let (_, tasks) = parsed.unwrap();
            assert!(!tasks.is_empty(), "Template {} has no tasks", t.name);
        }
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

        let hook_path = dir.path().join(".git/hooks/pre-commit");
        fs::create_dir_all(hook_path.parent().unwrap()).unwrap();
        fs::write(
            &hook_path,
            "#!/bin/sh\n# plz:managed - do not edit\nplz hooks run pre-commit \"$@\"\n",
        )
        .unwrap();

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
            .stdout(predicate::str::contains("Run a task from plz.toml"));
    }

    #[test]
    fn cli_help_lists_all_commands() {
        let dir = TempDir::new().unwrap();
        let output = plz()
            .arg("--help")
            .current_dir(dir.path())
            .output()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "init",
            "add",
            "hooks",
            "plz schema",
            "plz example",
            "--no-interactive",
        ] {
            assert!(
                stdout.contains(expected),
                "Help output missing '{expected}'"
            );
        }
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
            .stderr(predicate::str::contains("isn't a task"));
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

        plz()
            .current_dir(dir.path())
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "No task specified (running in non-interactive mode)",
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
            .env_remove("PLZ_COMMAND")
            .env_remove("CI")
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "No task specified (running in non-interactive mode)",
            ));
    }

    #[test]
    fn cli_init_already_exists() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("plz.toml"), "[tasks]").unwrap();

        plz()
            .arg("init")
            .current_dir(dir.path())
            .assert()
            .success()
            .stderr(predicate::str::contains("already exists"))
            .stderr(predicate::str::contains("plz"));
    }

    #[test]
    fn cli_init_no_project_creates_hello_task() {
        let dir = TempDir::new().unwrap();

        plz()
            .arg("init")
            .current_dir(dir.path())
            .assert()
            .success()
            .stderr(predicate::str::contains("Created plz.toml"));

        let content = fs::read_to_string(dir.path().join("plz.toml")).unwrap();
        assert!(content.contains("[tasks.hello]"));
        assert!(content.contains("echo 'hello world'"));
    }
}
