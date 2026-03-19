# Config & CLI Reference

## CLI Commands

Tools that need to interoperate with plz should run `plz help` or `plz schema`
for the TOML schema.

| Command                 | Description                                      |
| ----------------------- | ------------------------------------------------ |
| `plz [task]`            | Run a task (interactive picker if no task given) |
| `plz [task] -- [args]`  | Run a task with extra arguments                  |
| `plz init`              | Initialize plz.toml with auto-detected defaults  |
| `plz add [task]`        | Add a task from built-in snippets                |
| `plz hooks install`     | Install git hooks defined in plz.toml            |
| `plz hooks uninstall`   | Remove plz-managed git hooks                     |
| `plz hooks run <stage>` | Run all tasks for a git hook stage               |
| `plz healthcheck`       | Run code health checks on your repo              |
| `plz schema`            | Print JSON schema for plz.toml                   |
| `plz cheatsheet`        | Print a cheatsheet of plz.toml features           |
| `plz update`            | Update plz to the latest version                 |
| `plz plz`               | Set up user defaults in ~/.plz/                  |

> Built-in commands like `init`, `add`, and `hooks` are actually subcommands of `plz plz`. If you have a task with the same name as a built-in command, use `plz plz <command>` to run the built-in (e.g. `plz plz add`).

### Flags

| Flag | Description |
|---|---|
| `--no-interactive` | Disable interactive prompts. This happens automatically in your terminal programs and CI. |
| `--help` | Show help |
| `--version` | Show version |

## TOML Configuration

Tasks are defined in `plz.toml` (or `.plz.toml`) at your project root.

### Basic task

```toml
[tasks.build]
run = "cargo build"
```

`run` also accepts an array, which runs commands serially (equivalent to `run_serial`):

```toml
[tasks.fix]
run = ["cargo fmt", "cargo clippy --fix --allow-dirty"]
```

Comments above a task table are used as the task description in `plz --help`:

```toml
# Build the project
[tasks.build]
run = "cargo build"
```

Or set it explicitly:

```toml
[tasks.build]
run = "cargo build"
description = "Build the project"
```

### Serial execution

Run commands in order, stopping on first failure. You can use `run_serial` or pass an array to `run`:

```toml
[tasks.fix]
run_serial = ["cargo fmt", "cargo clippy --fix --allow-dirty"]
```

### Parallel execution

Run commands concurrently:

```toml
[tasks.check]
run_parallel = ["plz lint", "plz format"]
```

### Task references

Reference other tasks with `plz:taskname` or `plz:group:task` syntax in serial/parallel lists:

```toml
[tasks.check]
run_parallel = ["plz:lint", "plz:format"]
```

Reference group tasks with `plz:group:task`:

```toml
[tasks.all]
run_parallel = ["plz:ui:build", "plz:api:build"]
```

### Dependencies

Declare prerequisite tasks with `depends`. Dependencies run before the task, in order. Shared dependencies across tasks only run once per invocation.

```toml
[tasks.build]
run = "cargo build"

[tasks.test]
depends = "build"
run = "cargo test"

[tasks.deploy]
depends = ["build", "lint"]
run = "deploy.sh"
```

Use dot notation for group task dependencies:

```toml
[tasks.serve]
depends = ["ui.build"]
run = "python -m http.server"

[taskgroup.ui.build]
run = "pnpm build"
```

Circular dependencies are detected at config load time.

### Working directory

```toml
[tasks.frontend]
dir = "packages/web"
run = "pnpm dev"
```

### Environment wrappers

Wrap commands with a package manager using `tool_env`:

```toml
[tasks.vitest]
run = "vitest"
tool_env = "pnpm"
```

Supported values: `pnpm` (uses `pnpm exec`), `npm` (uses `npx`), `uv` (uses `uv run`), `uvx` (uses `uvx`).

### Failure hooks

Suggest a command for the user to run:

```toml
[tasks.format]
run = "cargo fmt --check"
fail_hook = { suggest_command = "cargo fmt" }
```

Display a message:

```toml
[tasks.deploy]
run = "deploy.sh"
fail_hook = { message = "Check the deploy logs at /var/log/deploy.log" }
```

Run a shell command:

```toml
[tasks.test]
run = "cargo test"
fail_hook = "notify-send 'Tests failed'"
```

### Git hooks

Assign a task to a git hook stage:

```toml
[tasks.check]
run_parallel = ["plz format", "plz lint"]
git_hook = "pre-commit"
```

Supported stages: `pre-commit`, `pre-push`, `commit-msg`, `post-commit`, `post-merge`, `post-checkout`.

### Extends (global defaults)

Set default `env` and `dir` for all tasks:

```toml
[extends]
env = { NODE_ENV = "production" }
dir = "packages/app"

[tasks.build]
run = "pnpm build"
# inherits env and dir from [extends]

[tasks.dev]
run = "pnpm dev"
dir = "."
# per-task dir overrides [extends]
```

### Task groups

Namespace related tasks under `[taskgroup.X]`:

```toml
[taskgroup.docs.build]
run = "pnpm docs:build"

[taskgroup.docs.dev]
run = "pnpm docs:dev"
```

Run with:

```bash
plz docs:build
plz docs:dev
```

Groups can have their own extends that cascade (top-level extends → group extends → task):

```toml
[extends]
env = { CI = "true" }

[taskgroup.docs.extends]
dir = "docs"

[taskgroup.docs.build]
run = "pnpm build"
# inherits CI=true from top-level, dir=docs from group
```

## Healthcheck

`plz healthcheck` runs Rust-native code health checks on any git repo. No `plz.toml` required.

```bash
plz healthcheck
```

### Checks

| Check | What it detects |
|-------|----------------|
| Check merge conflict markers | `<<<<<<<`, `=======`, `>>>>>>>` at line start |
| Check large files (>500KB) | Files exceeding 500KB in the git index |
| Detect private keys | `BEGIN *PRIVATE KEY` headers |
| Check case conflicts | Filenames that collide case-insensitively |
| Trailing whitespace | Lines ending in spaces or tabs |
| End of file newline | Files missing a final newline |
| Mixed line endings | Files with both `\r\n` and `\n` |

Binary files are automatically skipped for content checks. Exit code is 1 if any check fails.

### Ignoring findings

Add `plz:ignore <rule>` to a line to suppress a specific check on that line. Use `plz:ignore` without a rule to suppress all checks.

```
<<<<<<< HEAD  # plz:ignore merge-conflict
some trailing whitespace   // plz:ignore trailing-whitespace
```

Add `plz:ignore-file <rule>` to the first line of a file to skip an entire file for that check:

```
# plz:ignore-file private-key
BEGIN RSA PRIVATE KEY  (this file will be skipped) <!-- plz:ignore private-key -->
```

### Using with git hooks

Wire healthcheck into a pre-commit hook via `plz.toml`:

```toml
[tasks.healthcheck]
run = "plz healthcheck"
git_hook = "pre-commit"
```
