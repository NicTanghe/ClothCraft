#!/usr/bin/env python3
"""Advance ClothCraft's patch version for commits on main's first-parent history."""

import re
import subprocess
import tomllib
from pathlib import Path

SCRIPT_PATH = "scripts/bump-version.py"


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def manifest_version(revision):
    return tomllib.loads(git("show", f"{revision}:Cargo.toml"))["package"]["version"]


def replace_version(block, previous, version):
    pattern = r'^version = "' + re.escape(previous) + r'"$'
    block, count = re.subn(pattern, f'version = "{version}"', block, flags=re.M)
    if count != 1:
        raise SystemExit("Could not locate exactly one ClothCraft version line")
    return block


def main():
    manifest = Path("Cargo.toml")
    lock = Path("Cargo.lock")
    manifest_text = manifest.read_text()
    lock_text = lock.read_text()
    previous = manifest_version("HEAD")
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", previous):
        raise SystemExit(f"Expected a major.minor.patch version, got {previous!r}")

    # Start at adoption so older repository history does not generate bumps.
    # Stop at the latest actual version change, including a manual version edit.
    adoption = git("log", "-1", "--first-parent", "--diff-merges=first-parent", "--no-patch",
                   "--diff-filter=A", "--format=%H", "--", SCRIPT_PATH)
    if not adoption:
        raise SystemExit("Could not find the version automation's first-parent history")
    base = git("rev-parse", f"{adoption}^")
    for commit in git("log", "--first-parent", "--diff-merges=first-parent", "--no-patch",
                      "--format=%H", f"{base}..HEAD", "--", "Cargo.toml").splitlines():
        if manifest_version(commit) != manifest_version(f"{commit}^"):
            base = commit
            break

    count = int(git("rev-list", "--first-parent", "--count", f"{base}..HEAD"))
    if count == 0:
        return

    major, minor, patch = previous.split(".")
    version = f"{major}.{minor}.{int(patch) + count}"
    sections = re.split(r"(?=^\[)", manifest_text, flags=re.M)
    packages = [i for i, section in enumerate(sections) if section.startswith("[package]\n")]
    lock_sections = re.split(r"(?=^\[\[package\]\])", lock_text, flags=re.M)
    entries = []
    for i, section in enumerate(lock_sections[1:], 1):
        entry, = tomllib.loads(section)["package"]
        if entry["name"] == "clothcraft" and "source" not in entry:
            entries.append(i)
    if len(packages) != 1 or len(entries) != 1:
        raise SystemExit("Expected one ClothCraft package in the manifest and lockfile")
    sections[packages[0]] = replace_version(sections[packages[0]], previous, version)
    lock_sections[entries[0]] = replace_version(lock_sections[entries[0]], previous, version)

    manifest.write_text("".join(sections))
    lock.write_text("".join(lock_sections))
    print(version)


if __name__ == "__main__":
    main()
