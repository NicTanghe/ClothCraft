#!/usr/bin/env python3
"""Prepare or verify ClothCraft's package version before merging a PR."""

import argparse
import re
import subprocess
import tomllib
from pathlib import Path


def version_tuple(version):
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version):
        raise SystemExit(f"Expected a major.minor.patch version, got {version!r}")
    return tuple(map(int, version.split(".")))


def replace_version(block, previous, version):
    pattern = r'^version = "' + re.escape(previous) + r'"$'
    block, count = re.subn(pattern, f'version = "{version}"', block, flags=re.M)
    if count != 1:
        raise SystemExit("Could not locate exactly one ClothCraft version line")
    return block


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", nargs="?", default="origin/main", help="Git revision being merged into")
    parser.add_argument("--check", action="store_true", help="Verify the version without editing files")
    args = parser.parse_args()
    virtual_base = subprocess.check_output(
        ["git", "show", f"{args.base}:Cargo.toml"], text=True
    )
    base_version = tomllib.loads(virtual_base)["package"]["version"]
    base = version_tuple(base_version)
    manifest = Path("Cargo.toml")
    lock = Path("Cargo.lock")
    manifest_text = manifest.read_text()
    lock_text = lock.read_text()
    previous = tomllib.loads(manifest_text)["package"]["version"]
    current = version_tuple(previous)
    lock_entries = [
        entry for entry in tomllib.loads(lock_text)["package"]
        if entry["name"] == "clothcraft" and "source" not in entry
    ]
    if len(lock_entries) != 1 or lock_entries[0]["version"] != previous:
        raise SystemExit("ClothCraft versions must match in Cargo.toml and Cargo.lock")

    if current > base:
        print(f"ClothCraft {previous} is newer than {base_version}")
        return
    if args.check:
        raise SystemExit(
            f"ClothCraft {previous} must be newer than {base_version}. "
            "Run python3 scripts/bump-version.py and commit both Cargo files."
        )

    branch = subprocess.check_output(["git", "branch", "--show-current"], text=True).strip()
    if branch == "main" or not branch:
        raise SystemExit("Prepare the version on a PR branch")
    major, minor, patch = base
    version = f"{major}.{minor}.{patch + 1}"
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
    print(f"Prepared ClothCraft {version}")


if __name__ == "__main__":
    main()
