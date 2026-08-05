# /// script
# requires-python = ">=3.11"
# ///
"""Cut a release: bump the version, update CHANGELOG.md, commit, and tag.

Usage: uv run scripts/release.py [patch|minor|major|X.Y.Z]
With no argument, the next version comes from `git-cliff --bumped-version`.
Pushing the tag triggers the release workflows (GitHub, crates.io, PyPI).
"""

import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def run(*args: str, capture: bool = False) -> str:
    result = subprocess.run(
        args, cwd=ROOT, check=True, capture_output=capture, text=True
    )
    return result.stdout.strip() if capture else ""


def die(msg: str) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def next_version(current: str, arg: str | None) -> str:
    if arg is None:
        return run("git-cliff", "--bumped-version", capture=True).removeprefix("v")
    if arg in ("patch", "minor", "major"):
        major, minor, patch = (int(p) for p in current.split("."))
        if arg == "patch":
            return f"{major}.{minor}.{patch + 1}"
        if arg == "minor":
            return f"{major}.{minor + 1}.0"
        return f"{major + 1}.0.0"
    version = arg.removeprefix("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        die(f"invalid version: {arg}")
    return version


def main() -> None:
    if run("git", "status", "--porcelain", capture=True):
        die("working tree is not clean")
    branch = run("git", "branch", "--show-current", capture=True)
    if branch != "main":
        die(f"not on main (on {branch})")

    cargo_toml = ROOT / "Cargo.toml"
    current = tomllib.loads(cargo_toml.read_text())["package"]["version"]
    version = next_version(current, sys.argv[1] if len(sys.argv) > 1 else None)
    tag = f"v{version}"

    tag_exists = subprocess.run(
        ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"],
        cwd=ROOT,
        capture_output=True,
    )
    if tag_exists.returncode == 0:
        die(f"tag {tag} already exists")

    print(f"Releasing {current} -> {version}")

    content = cargo_toml.read_text()
    new_content, n = re.subn(
        rf'^version = "{re.escape(current)}"$',
        f'version = "{version}"',
        content,
        count=1,
        flags=re.M,
    )
    if n != 1:
        die("couldn't find the version line in Cargo.toml")
    cargo_toml.write_text(new_content)
    run("cargo", "update", "-p", "plzplz", "--offline")

    section = run("git-cliff", "--unreleased", "--tag", tag, capture=True)
    changelog = ROOT / "CHANGELOG.md"
    marker = "## [Unreleased]\n"
    text = changelog.read_text()
    if marker not in text:
        die("no '## [Unreleased]' marker in CHANGELOG.md")
    changelog.write_text(text.replace(marker, f"{marker}\n{section}\n", 1))

    run("git", "add", "Cargo.toml", "Cargo.lock", "CHANGELOG.md")
    run("git", "commit", "-m", f"chore: release {tag}")
    run("git", "tag", "-a", tag, "-m", tag)

    print(f"\nTagged {tag}. Review with:  git show {tag}")
    print(f"Publish with:  git push origin main {tag}")
    print(f"Undo with:     git tag -d {tag} && git reset --hard HEAD~1")


if __name__ == "__main__":
    main()
