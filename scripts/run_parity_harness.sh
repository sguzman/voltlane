#!/usr/bin/env bash
set -euo pipefail

cargo test -p voltlane-core parity_report_matches_golden_baseline
cargo run -p voltlane-core --bin voltlane-cli -- parity-report --output data/parity/report.json

echo "Parity harness complete: data/parity/report.json"
