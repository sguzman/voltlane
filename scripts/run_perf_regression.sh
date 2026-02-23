#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

: "${VOLTLANE_PERF_MAX_MIDI_MS:=1500}"
: "${VOLTLANE_PERF_MAX_RENDER_MS:=4500}"

echo "Running Voltlane performance regression suite"
echo "  VOLTLANE_PERF_MAX_MIDI_MS=${VOLTLANE_PERF_MAX_MIDI_MS}"
echo "  VOLTLANE_PERF_MAX_RENDER_MS=${VOLTLANE_PERF_MAX_RENDER_MS}"

VOLTLANE_PERF_MAX_MIDI_MS="$VOLTLANE_PERF_MAX_MIDI_MS" \
VOLTLANE_PERF_MAX_RENDER_MS="$VOLTLANE_PERF_MAX_RENDER_MS" \
cargo test -p voltlane-core --release --test performance_regression -- --nocapture
