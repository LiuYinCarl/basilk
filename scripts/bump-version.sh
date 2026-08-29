#!/usr/bin/env bash
#
# Bump the semantic version of the crate in Cargo.toml + Cargo.lock.
# The files are edited in the working tree only — nothing is committed
# or tagged here.
#
# Usage:
#   ./scripts/bump-version.sh [patch|minor|major]
#
#   patch (default): 0.2.1 -> 0.2.2
#   minor:           0.2.1 -> 0.3.0
#   major:           0.2.1 -> 1.0.0
#
# Flow: the pre-commit hook (see .githooks/) requires every code commit
# to include a version bump, so run this while preparing your commit,
# stage everything together, and commit. The post-commit hook then
# creates the vX.Y.Z tag automatically; pushing it triggers the CI
# release pipeline (.github/workflows/release.yml).
set -euo pipefail

cd "$(dirname "$0")/.."

BUMP="${1:-patch}"
case "$BUMP" in
  patch | minor | major) ;;
  *) echo "usage: $0 [patch|minor|major]" >&2; exit 1 ;;
esac

CURRENT="$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
MAJOR="$(echo "$CURRENT" | cut -d. -f1)"
MINOR="$(echo "$CURRENT" | cut -d. -f2)"
PATCH="$(echo "$CURRENT" | cut -d. -f3)"

case "$BUMP" in
  major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
  minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
  patch) PATCH=$((PATCH + 1)) ;;
esac

NEW="$MAJOR.$MINOR.$PATCH"

if [ -n "$(git tag -l "v$NEW")" ]; then
  echo "error: tag v$NEW already exists" >&2
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

echo "bumped $CURRENT -> $NEW"
echo "next: stage Cargo.toml + Cargo.lock together with your code changes and commit"
echo "(the post-commit hook tags v$NEW; pushing the tag triggers the release pipeline)"
