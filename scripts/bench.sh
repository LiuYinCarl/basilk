#!/usr/bin/env bash
#
# Run the opt-in performance benchmarks (release build, timing printed).
#
# Usage: ./scripts/bench.sh
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test --release --features bench --bin basilk perf_tests -- --ignored --nocapture
