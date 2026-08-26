#!/usr/bin/env bash
#
# Bump the semantic version of the crate, commit and tag it.
#
# Usage:
#   ./scripts/bump-version.sh [patch|minor|major] [--push]
#
#   patch (default): 0.2.1 -> 0.2.2
#   minor:           0.2.1 -> 0.3.0
#   major:           0.2.1 -> 1.0.0
#
#   --push: also push the commit and tag to origin.
#
# The CI release pipeline (`.github/workflows/release.yml`) does this
# automatically on every code push to master (patch by default, or
# major/minor when the commit subject starts with `[major]` / `[minor]`).
# This script is for local, manual releases or pre-push version control.
set -euo pipefail

cd "$(dirname "$0")/.."

BUMP="${1:-patch}"
PUSH=false
for arg in "$@"; do
  [ "$arg" = "--push" ] && PUSH=true
done

CURRENT="$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
MAJOR="$(echo "$CURRENT" | cut -d. -f1)"
MINOR="$(echo "$CURRENT" | cut -d. -f2)"
PATCH="$(echo "$CURRENT" | cut -d. -f3)"

case "$BUMP" in
  major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
  minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
  patch) PATCH=$((PATCH + 1)) ;;
  *) echo "usage: $0 [patch|minor|major] [--push]" >&2; exit 1 ;;
esac

NEW="$MAJOR.$MINOR.$PATCH"

if [ -n "$(git tag -l "v$NEW")" ]; then
  echo "error: tag v$NEW already exists" >&2
  exit 1
fi

if ! git diff --quiet; then
  echo "error: working tree is dirty; commit or stash your changes first" >&2
  exit 1
fi

python3 - <<EOF
import re

# Cargo.toml: bump the package version
path = "Cargo.toml"
s = open(path).read()
s = re.sub(r'^version = \"[^\"]+\"', 'version = "$NEW"', s, count=1, flags=re.M)
open(path, "w").write(s)

# Cargo.lock: keep the root package version in sync
path = "Cargo.lock"
s = open(path).read()
s = re.sub(r'(name = "basilk"\nversion = ")[^"]+', r'\g<1>$NEW', s, count=1)
open(path, "w").write(s)
EOF

git add Cargo.toml Cargo.lock
git commit -m "chore: release v$NEW"
git tag "v$NEW"

echo "bumped $CURRENT -> $NEW (tag v$NEW)"

if $PUSH; then
  git push origin "HEAD:master" --tags
  echo "pushed"
else
  echo "next: git push origin master --tags"
fi
echo "pushing the v$NEW tag triggers the GitHub Release pipeline"
