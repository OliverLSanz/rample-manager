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
tooltips and errors. Preserve user sample filenames (Build Patch, below, is the
reviewed exception for its own Layer files).

Commit each atomic functional change separately, with its relevant tests and docs.
Push each completed commit to the private GitHub repository; do not leave finished
work only locally. Keep the lockfiles. Never commit user WAVs, session files,
configuration captures, local build outputs or signing credentials.

## Main screen

The main window is the open Patch (ADR 0008,
`docs/adr/0008-make-the-open-patch-the-main-screen.md`): Kits in the sidebar
(`KitList`), the Patch switcher at the top (`PatchMenu`), Layers on the voice
cards (`VoiceCard`) and the Patch sequencer. Keep one Kit workspace: Patch,
Kit and Layer management belong on this screen, never in a second dialog that
repeats it. The switcher only chooses, creates, renames or imports Patches and
returns to the main window. Pure view logic lives in `frontend/patchView.ts`.
The folder session is not shown; it is only offered as **Move current folder
into Library**. Kits and Patches delete at once with an in-memory Undo
(`patches::Deleted`), not a confirmation; Undo must restore the same IDs.
Deleting a Patch unlinks its `sd_intakes` rows in the same transaction.

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

Changes to selected or automatic layers must reload in real time. In the legacy
folder backend kept for migration, preserve the selected filename when a file
is removed, so restoring it restores playback.
Source audio is read-only during browsing/playback. The explicit exception is
conversion approved in the sample-review modal or explicit **Save to sample** in
the editor: validate the reviewed fingerprint,
back up the original under the app support folder, validate the complete output,
recheck the source bytes and only then replace it. Never overwrite a different
existing destination WAV. Export copies to a new kit and never overwrites one.
The explicit exception is **Build Patch** (ADR 0005, below): after the user
applies its reviewed difference, it may replace, remove and rename owned WAVs
in canonical Kit folders of the destination, only once every affected file is
backed up and verified as a Destination backup.

## Sample validation and conversion

The folder review and conversion below belong to the legacy folder backend,
which the interface no longer opens (ADR 0008); keep its behavior intact until
it is removed. New or revised audio is reviewed against the official manual, including initial
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

Five modes are supported. New Kits use the user's Default Layer mode (initially
Single sample / MANUAL). Modes and selected
layers persist on this computer. The app manages audio files only and never
creates, modifies, deletes or restores anything in `_save`
(`docs/adr/0007-read-native-settings-and-never-write-save.md`). Device settings
that no longer match the files are the user's concern on the Rample. SD Import
reads each Kit's `_save/<Kit>.rpl` through the bounded reader in
`rample_core::native` to fill initial modes. It restores only mappings verified
by Oliver's controlled firmware 2.00 captures
(`docs/research/rample-native-settings-mappings.md`): per-Voice `layer_modes`
0–4 and zero-based `selected_layer`, assuming standard natural filename order.
The device's global `layerMode` is ignored. The user's Library-wide Default
Layer mode (`PatchGraph.default_layer_mode`) applies to new Kits, to Kits
never saved with STORE and to unverified or unreadable values, which are
reported, never guessed. A source with Kits but no `_save` folder gets a
source-wide `NativeReading.issues` entry instead of silent defaults. No further captures are planned. Do not guess offsets or substitute MIDI enums. Preserve the imported
bytes exactly. Device captures stay in the git-ignored `Rample SD Captures/`;
tests use generated fixtures. BPM and sequencer steps remain app-only.

## Build Patch

`src-tauri/src/build_patch.rs` reconciles a folder or SD root with a Patch
(ADR 0005) in two commands: `review_patch_build` and `apply_patch_build(review)`.
`src-tauri/src/destination_backups.rs` owns Destination backups: the
`restore.json` `Manifest` (backup, verification, restore planning), listing and
`restore_patch_build`.
Build owns only WAV files directly inside canonical Kit folders (`A0`–`Z99`, no
leading zeros). Everything else, including `_save`, firmware, other files in Kit
folders and names such as `A05`, is never written, removed or backed up; OS
metadata is ignored. Case-insensitive name collisions block the review, as does
a non-canonical folder that SD Import's `address()` reads as a Patch Kit or a
kept Kit (`A05` next to `A5`); unrelated non-canonical folders get a notice. A file
is unchanged only when its hash is a recognized export of the same Sample and
current Revision. Apply recomputes the review and refuses any difference in the
Patch output or the owned destination state. Then it renders every changed Layer
through `stage_export`, copies and verifies every replaced or removed file under
`Patch build backups/<id>/` with a `restore.json` manifest (`applying` before
publication, then `complete` or `incomplete`), registers the exports and only
then removes, writes and validates, rechecking each target's current bytes
against the review immediately before it is removed or overwritten (a mismatch
stops publication as incomplete and is never overwritten). Errors before publication must say nothing
was written; failures after it return an incomplete reply with its backup and
progress, never success. Metadata files (`.DS_Store`) of obsolete Kit folders
that Build removes are backed up too. `restore_patch_build(id, destination)`
undoes one Build: it verifies the backup copies, refuses (changing nothing)
when any affected file holds neither that Build's output, its original bytes nor
is absent, then removes added files, restores originals via temp file + rename,
restores metadata and removes Kit folders the Build created. It is idempotent.
Layer files are `<Voice>_<two-digit position>_<safe
suffix>.wav`, so natural and byte order agree; user-facing Sample names are
never changed. Never write `_save`, Layer modes, selections, BPM or sequencer
state.

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
