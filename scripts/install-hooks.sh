#!/usr/bin/env bash
#
# Point git at the repository-managed hooks in .githooks/ (one-time
# setup per clone):
#
#   pre-commit   block code commits that do not bump the crate version
#   post-commit  auto-create the vX.Y.Z tag after a version-bump commit
set -euo pipefail

cd "$(dirname "$0")/.."

git config core.hooksPath .githooks
echo "git hooks enabled (core.hooksPath=.githooks)"
