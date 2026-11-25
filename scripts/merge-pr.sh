#!/usr/bin/env bash
# Prepare the current PR's version, wait for its checks, then squash merge.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Commit your work before running this command." >&2
  exit 1
fi
read -r pr base branch head cross draft state < <(
  gh pr view --json number,baseRefName,headRefName,headRefOid,isCrossRepository,isDraft,state \
    --jq '[.number,.baseRefName,.headRefName,.headRefOid,.isCrossRepository,.isDraft,.state] | join(" ")'
)
if [[ "$base" != main || "$branch" == main || "$cross" != false ||
      "$draft" != false || "$state" != OPEN || "$head" != "$(git rev-parse HEAD)" ]]; then
  echo "Check out the current head of an open, ready PR from this repository into main." >&2
  exit 1
fi
git fetch origin main
base_sha=$(git rev-parse origin/main)
python3 scripts/bump-version.py "$base_sha"
if ! git diff --quiet -- Cargo.toml Cargo.lock; then
  git add Cargo.toml Cargo.lock
  git commit -m "Prepare ClothCraft package version for PR #$pr"
  git push origin "HEAD:refs/heads/$branch"
fi
head=$(git rev-parse HEAD)
test "$(gh pr view "$pr" --json headRefOid --jq .headRefOid)" = "$head"
gh pr checks "$pr" --watch
git fetch origin main
if [[ "$(git rev-parse origin/main)" != "$base_sha" ]]; then
  echo "main advanced while checks ran. Update your branch and run this command again." >&2
  exit 1
fi
gh pr merge "$pr" --squash --match-head-commit "$head"
