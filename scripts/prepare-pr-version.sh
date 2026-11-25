#!/usr/bin/env bash
# Run from the trusted automation checkout with a separate PR worktree.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd -- "$1"
base_sha=$2
# Cargo inputs come from the PR, so refuse symlinks before reading or writing.
for file in Cargo.toml Cargo.lock; do
  if [[ ! -f "$file" || -L "$file" ]]; then
    echo "Expected a regular file at $file." >&2
    exit 1
  fi
done
if python3 "$script_dir/bump-version.py" --check "$base_sha"; then
  exit 0
fi
# An earlier automatic bump may now be on both branches. Integrate main before
# changing the version again so the next squash does not conflict on that line.
if ! git merge-base --is-ancestor "$base_sha" HEAD; then
  git -c core.hooksPath=/dev/null merge --no-commit --no-ff "$base_sha"
fi
python3 "$script_dir/bump-version.py" "$base_sha"
python3 "$script_dir/bump-version.py" --check "$base_sha"
git add -- Cargo.toml Cargo.lock
git -c core.hooksPath=/dev/null commit -m "Prepare ClothCraft package version"
