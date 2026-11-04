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
Source audio is read-only during browsing/playback. The explicit exception is
conversion approved in the sample-review modal or explicit **Save to sample** in
the editor: validate the reviewed fingerprint,
back up the original under the app support folder, validate the complete output,
recheck the source bytes and only then replace it. Never overwrite a different
existing destination WAV. Export copies to a new kit and never overwrites one.

## Sample validation and conversion

New or revised audio is reviewed against the official manual, including initial
folder loading. `sample-reviews.json` remembers per-library reviews; declining
must survive force refreshes/restarts and pending reviews must survive restart.
`Check formats` explicitly reopens all current issues. Keep review state separate
from the musical session. Files outside an A0–Z99 kit, bad voice prefixes, missing
voice 1 and excess layers require manual fixes. Mixed mono/stereo layers are
resolved by converting stereo layers to mono; leave compatible mono layers alone.

Use the bundled Rust decoder/converter; no FFmpeg installation is required.
Target standard mono PCM16 WAV at 44.1 kHz. Average all input channels (stereo:
(L + R) / 2) with disclosed consent, and pad short files with silence to 50 ms.
Mono is the user-requested preparation policy, even for otherwise valid stereo
WAVs. Reopen old stereo reviews once when introducing this policy; subsequent
declines must persist. Do not report already compatible mono layers as needing
manual fixes solely because another layer is stereo.
Preserve duration and use band-limited resampling and dither where needed.
Decode/convert in bounded chunks off the audio thread. Corrupt/truncated files,
read-only files, stale reviews and conflicting .wav names must preserve originals.
Backups contain the original bytes and a restore.json manifest with paths/SHA-256.
Never guess a voice/kit, silently rename collisions or write native .rpl settings.

## Sample editor

`editing.rs` owns the shared pitch/trim interpolation and gain/EQ/echo/reverb DSP.
Live preview uses this same engine inside each playback; baking streams through
it at 44.1 kHz into mono PCM16. FX tails are part of the sample and reset on each
trigger, matching Rample retrigger behavior. Pitch is varispeed, not time stretch.
Prepare sinc tables, coefficients, delay buffers and trim-start preloads on the
control thread. Preserve bounded streaming and never read files in the callback.
The editor is transient and applies only to its source path, not all voice layers.
Bypass/close restore original playback. Save backs up original bytes under
`Sample edit backups`, validates the full WAV, rechecks the source before atomic
publication, clears preview to prevent double effects, and reloads the voice.
Keep musical session/modes unchanged. Stale IDs and older preview requests must
not affect a newer editor. External file changes disable preview/save until the
sample is reopened. Failed post-save preference writes must still report a saved
sample and backup accurately. Use owned fixtures for destructive editor QA.

## Session migration

macOS preferences stay at `~/Library/Application Support/Rample Preview/session.json`.
Windows uses `%APPDATA%/Rample Preview/session.json`. The same folder contains
`Demo SD`. Swift sessions migrate automatically and receive a byte-for-byte
`session-swift-1.1.0.backup.json` backup before the first migrated save. Keep modes,
selected layers, patterns, velocity, BPM, mixer and per-kit memory compatible.
`RAMPLE_DATA_DIR` overrides the support folder for isolated testing only.

## Native SD configuration

Five modes are supported, defaulting to Single sample (MANUAL). Modes and selected
layers persist on this computer. SD Import reads `_save/*.rpl` through the
bounded reader in `rample_core::native` and restores only mappings verified by
Oliver's controlled firmware 2.00 captures
(`docs/research/rample-native-settings-mappings.md`): per-Voice `layer_modes`
0–4, zero-based `selected_layer` and global `layerMode` 0/1. Unverified values
keep app defaults and are reported, never guessed. Device captures stay in the
git-ignored `Rample SD Captures/`; tests use generated fixtures. Native files
are not written yet: writing needs the device round trip in ADR 0003. Do not guess
offsets or substitute MIDI enums. Preserve unknown fields and unrelated native
settings, and retain recoverable originals. BPM and sequencer steps remain
app-only. Remaining capture batches are listed in
`docs/research/rample-save-format-study.es.md`.

Official SD-format reference: https://squarp.net/rample/manual/

## Agent skills

### Issue tracker

Track issues and specs in GitHub Issues. Before reading or publishing tickets,
read `docs/agents/issue-tracker.md`.

### Triage labels

Use the five canonical triage labels. Before triaging issues, read
`docs/agents/triage-labels.md`.

### Domain docs

Use a single-context layout. Before exploring the codebase, read
`docs/agents/domain.md`.
