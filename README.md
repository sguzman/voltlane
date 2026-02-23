# Voltlane

Voltlane is a Rust-first FL-style/chiptune composition prototype built with:

- `voltlane-core` (Rust): project model, timeline engine, export pipeline, parity tooling, and tracing.
- `src-tauri` (Rust/Tauri): desktop shell and typed command bridge to the core.
- `ui` (React + TypeScript + CSS): lightweight visual playlist/mixer control surface.

The current implementation targets Milestone A/B from your planning document: project lifecycle, transport controls, colored tracks, clip creation, effect attachment, exports, parity harness, logging, and docs.

## Features Implemented

- Rust domain model for projects, tracks, clips, effects, transport, and notes.
- Command-style engine API for:
  - project create/load/save/autosave
  - track add/reorder/hide/mute/enable
  - clip add/move
  - effect attachment
  - playback + loop control
- Export pipeline:
  - MIDI (`.mid`) via `midly`
  - WAV (`.wav`) via deterministic renderer + `hound`
  - MP3 (`.mp3`) via `ffmpeg` sidecar invocation
- Extensive structured logging with `tracing` in core and bridge.
- Tauri commands for all major operations (`project/track/clip/export/parity`).
- React UI for playlist lanes, track controls, transport, and parity panel.
- Deterministic parity harness:
  - golden baseline test
  - CLI parity report generation

## Repository Layout

- `Cargo.toml`: Rust workspace root
- `crates/voltlane-core`: core engine/domain/export/parity modules
- `crates/voltlane-core/tests`: export/parity integration tests + golden baseline
- `src-tauri`: Tauri host and command bridge
- `ui`: React/TypeScript frontend
- `scripts/run_parity_harness.sh`: parity harness runner
- `ROADMAP.md`: progress tracker with checkboxes

## Build and Run

## Runtime Config

The app reads runtime settings from:

- `voltlane.config.toml`

This file now contains the app parameters (mode, defaults, diagnostics, paths, Wayland behavior, export binary path).

Current mode is set to:

- `mode = "dev"`

In `dev` mode, each app launch writes timestamped Rust tracing logs into:

- `logs/`

### Install dependencies

```bash
pnpm install
```

### Rust checks/tests

```bash
cargo check -p voltlane-core
cargo check -p voltlane-tauri
cargo test -p voltlane-core
```

### Frontend build

```bash
pnpm --dir ui run build
```

### Run desktop app (dev)

```bash
pnpm run tauri:dev
```

### Wayland note (Linux)

Voltlane reads Wayland compatibility toggles from `voltlane.config.toml`.
With the current config it auto-applies:

- `WEBKIT_DISABLE_DMABUF_RENDERER=1`

when `WAYLAND_DISPLAY` is present.

## Logging

- Core tracing is initialized at runtime with session UUID and per-run timestamped JSON log files.
- Tauri plugin log forwards logs to stdout, log dir, and webview stream.
- Log filtering and sink behavior are configured in `voltlane.config.toml`.

## Parity Harness

### Golden parity test

```bash
cargo test -p voltlane-core parity_report_matches_golden_baseline
```

### Generate parity report artifact

```bash
cargo run -p voltlane-core --bin voltlane-cli -- parity-report --output tmp/parity/report.json
```

### One-shot runner

```bash
./scripts/run_parity_harness.sh
```

Golden baseline lives at:

- `crates/voltlane-core/tests/fixtures/parity_baseline.json`

To refresh it intentionally:

```bash
UPDATE_PARITY_BASELINE=1 cargo test -p voltlane-core parity_report_matches_golden_baseline
```

## Export Notes

- MP3 export requires `ffmpeg` in `PATH` unless you pass a specific binary path through Tauri command input.
- WAV and MIDI exports are fully Rust-native in this implementation.

## Status

This is a production-style foundation, not a finished DAW. See `ROADMAP.md` for checked progress and next milestones.
