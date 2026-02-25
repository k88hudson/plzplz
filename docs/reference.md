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

Run commands in order, stopping on first failure:

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
