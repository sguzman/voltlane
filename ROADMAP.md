# Voltlane Roadmap

## Phase 0: Product Blueprint

- [x] Capture architecture, milestones, and subsystem boundaries from planning doc
- [x] Establish Rust-first ownership model (core authoritative, UI thin)
- [x] Define logging + diagnostics as first-class requirement

## Phase 1: Foundation / MVP Infrastructure

- [x] Rust workspace with `voltlane-core` + `src-tauri`
- [x] Tauri shell with typed command bridge
- [x] React/TypeScript/CSS frontend scaffold
- [x] Project create/load/save/autosave flows
- [x] Global transport state (play/stop + loop)
- [x] Extensive structured tracing in Rust core
- [ ] Crash recovery UX polish in desktop app

## Phase 2: Composition Core (MIDI + Playlist)

- [x] Colored FL-style track lanes
- [x] Add/reorder/hide/mute/enable tracks
- [x] Add and move MIDI/pattern clips
- [x] Basic clip visualization in playlist
- [x] Effect attachment primitives per track
- [ ] Full piano roll editor with note drag/resize
- [x] Quantize/transpose tools
- [x] Marker and loop-region editor UI

## Phase 3: Audio Clip Workflow

- [ ] Audio file import/decode path in core
- [ ] Waveform peak cache generation
- [ ] Clip trim/fade/reverse/stretch
- [ ] Browser preview and asset indexing

## Phase 4: Chiptune/Tracker Differentiator

- [x] Pattern clip payload shape and chip-source metadata
- [ ] Tracker grid editor (rows/effects)
- [ ] Chip macro editor (duty/env/arpeggio)
- [ ] Real chip backend emulation pipeline

## Phase 5: Mixer/FX/Routing

- [x] Track-level effect chain model
- [ ] Mixer buses/sends/routing graph
- [ ] Built-in FX suite (EQ/comp/reverb/delay/limiter)
- [ ] Automation lanes and parameter IDs

## Phase 6: Export/Render

- [x] MIDI export
- [x] WAV export
- [x] MP3 export via ffmpeg sidecar
- [ ] Stem export
- [ ] Offline/realtime render mode selection

## Phase 7: Quality, Testing, and Release

- [x] Deterministic parity harness (golden report)
- [x] Export smoke tests
- [x] Time conversion unit tests
- [ ] Property/fuzz tests for import/project corruption
- [ ] Performance regression suite
- [ ] CI pipeline (fmt/clippy/tests/dependency audit)
