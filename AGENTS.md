# Rample Preview

Cross-platform desktop app: Tauri 2 shell, React/TypeScript UI and Rust audio/file
core. The macOS installation is `/Applications/Rample Preview.app`. Windows NSIS
installers are built by `.github/workflows/desktop.yml` and published privately.

## Change, validate and install

Use Node 22+, npm and stable Rust. Run `npm ci` after dependency changes.
Before committing functional changes run:

```sh
npm run build
npm test
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Use `./scripts/install.sh` to build, sign locally, replace and reopen the Mac app.
Installation is part of the authorized workflow; do not ask for another approval
or leave the user to copy bundles. Preserve the session and source libraries.
The installer keeps a local backup of an installed Swift 1.x app in `.build/legacy`.
`legacy/swift` contains the archived implementation for reference/rollback only.
Do not add features there or use its installer for normal updates.

## Git workflow

Keep every user-facing string in English, including menus, accessibility labels,
tooltips and errors. Preserve user sample filenames.

Commit each atomic functional change separately, with its relevant tests and docs.
Push each completed commit to the private GitHub repository; do not leave finished
work only locally. Keep the lockfiles. Never commit user WAVs, session files,
configuration captures, local build outputs or signing credentials.

## Architecture and audio

- `frontend/`: UI, native IPC adapter and UI tests. It never schedules audio.
- `crates/rample-core/`: WAV decoding, SD catalog, layer selection, session
  migration/export and sample-clock renderer. `native-audio` enables CPAL.
- `src-tauri/`: serialized app state, native dialogs/commands, file watching and
  audio device management. Native settings live separately from app preferences.

Sequence hits against the output sample clock. Keep filesystem reads, decoding,
resampler preparation and device reconnection off the audio callback. Long WAVs
have bounded preloaded heads and background disk readers, not whole-file decoding.
Keep headless PCM/timing tests separate from hardware-dependent smoke tests:
`cargo run -p rample-core --example audio_smoke --features native-audio`.

Changes to selected or automatic layers must reload in real time. Preserve the
selected filename when a file is removed, so restoring it restores playback.
Source WAVs are read-only. Export copies to a new kit and never overwrites one.

## Session migration

macOS preferences stay at `~/Library/Application Support/Rample Preview/session.json`.
Windows uses `%APPDATA%/Rample Preview/session.json`. The same folder contains
`Demo SD`. Swift sessions migrate automatically and receive a byte-for-byte
`session-swift-1.1.0.backup.json` backup before the first migrated save. Keep modes,
selected layers, patterns, velocity, BPM, mixer and per-kit memory compatible.
`RAMPLE_DATA_DIR` overrides the support folder for isolated testing only.

## Native SD configuration

Five modes are supported, defaulting to Single sample (MANUAL). Modes and selected
layers persist on this computer; `_save/*.rpl` files are not imported or written.
Oliver will collect controlled STORE/SAVE SETTINGS examples following
`docs/rample-sd-capture.es.md`. Public research suggests CBOR with `layer_modes`,
`selected_layer` and global `layerMode`; verify mappings against those device
saves before implementing native import/write. This is a format dependency, not
missing user authorization to implement sync. Do not guess offsets or substitute
MIDI enums. Preserve unknown fields and unrelated native settings, and retain
recoverable originals. BPM and sequencer steps remain app-only.

Official SD-format reference: https://squarp.net/rample/manual/
