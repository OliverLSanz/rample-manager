# Rample Manager

Cross-platform desktop app: Tauri 2 shell, React/TypeScript UI and Rust audio/file
core. The macOS installation is `/Applications/Rample Manager.app` (called Rample
Preview before 3.0). Windows NSIS and universal macOS installers are built by
`.github/workflows/desktop.yml`; a `v*` tag publishes them as a public GitHub
Release of `OliverLSanz/rample-manager` with `docs/releases/<tag>.md` as notes.
The website and visual manual live in `site/` and deploy with
`./scripts/deploy-site.sh` to https://oliverlsanz.com/RampleManager/. Keep the
manual in step with user-facing changes.

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
or leave the user to copy bundles. Preserve the support folder and its Library.
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
cards (`VoiceCard`), the Add Samples side panel (`SamplePanel`) and the Patch
sequencer. The **Samples** view (`SamplesScreen`) replaces it in the main area
for the Library; Library audition goes through `useSampleAudition`. The window sets `dragDropEnabled: false` so HTML5 drag and drop
(Samples onto voices, Layer reordering) works in WebView2; the app takes no
file drops. Keep one Kit workspace: Patch,
Kit and Layer management belong on this screen, never in a second dialog that
repeats it. The switcher only chooses, creates, renames or imports Patches and
returns to the main window. Pure view logic lives in `frontend/patchView.ts`.
Kits, Patches and Layers delete at once with an in-memory Undo
(`patches::Deleted`), not a confirmation; Undo must restore the same IDs.
Deleting a Patch unlinks its `sd_intakes` rows in the same transaction.

## Architecture and audio

- `frontend/`: UI, native IPC adapter and UI tests. It never schedules audio.
- `crates/rample-core/`: WAV and compressed-audio decoding, Rample preparation,
  Sample Edit DSP, native `_save` reading, layer selection and the sample-clock
  renderer. `native-audio` enables CPAL.
- `src-tauri/`: the managed Library (`managed.rs`), Patches, SD Import, Build
  Patch, Library snapshots, native dialogs/commands and audio devices.

Sequence hits against the output sample clock. Keep filesystem reads, decoding,
resampler preparation and device reconnection off the audio callback. Long WAVs
have bounded preloaded heads and background disk readers, not whole-file decoding.
Keep headless PCM/timing tests separate from hardware-dependent smoke tests:
`cargo run -p rample-core --example audio_smoke --features native-audio`.

Changes to selected or automatic layers must reload in real time.
The app has no folder library (ADR 0009): every sound is a Sample imported into
the Library, whose Originals are never modified. Imports, SD Import and Build
Patch only read their sources. The one exception is **Build Patch** (ADR 0005,
below): after the user applies its reviewed difference, it may replace, remove
and rename owned WAVs in canonical Kit folders of the destination, only once
every affected file is backed up and verified as a Destination backup.

## Rample preparation

Use the bundled Rust decoder/converter; no FFmpeg installation is required.
Kit preview, Rample audition and Build Patch use the same preparation: standard
mono PCM16 WAV at 44.1 kHz, averaging all input channels (stereo: (L + R) / 2),
padding audio shorter than 50 ms with silence. Preserve duration and use
band-limited resampling and dither where needed. Decode in bounded chunks off
the audio thread. Originals keep their channels, rate and bytes.

## Sample editor

`editing.rs` owns the shared pitch/trim interpolation and gain/EQ/echo/reverb DSP.
Live preview uses this same engine inside each playback; rendering streams
through it at 44.1 kHz into mono PCM16. FX tails are part of the sound and reset
on each trigger, matching Rample retrigger behavior. Pitch is varispeed, not time
stretch. Prepare sinc tables, coefficients, delay buffers and trim-start preloads
on the control thread. Preserve bounded streaming and never read files in the
callback. Sample Edits are reversible settings over the complete Original
(ADR 0004): a draft affects only its Layer occurrence until **Overwrite shared
Sample** or **Save as new** (a Variant). Bypass/close restore the saved sound.
Keep musical session/modes unchanged. Stale IDs and older preview requests must
not affect a newer editor. Changed Originals disable preview/save until the
Sample is reopened.

## Support folder

macOS uses `~/Library/Application Support/Rample Manager/`, Windows
`%APPDATA%/Rample Manager/`; an existing `Rample Preview/Library` keeps being used
in place until a `Rample Manager` folder exists (`support_folder`). The app reads only its `Library/` (plus
`Patch build backups/`). Files from the retired folder workflow (`session.json`,
Swift session backups, `Demo SD`, `sample-reviews.json`, `Conversion backups`,
`Sample edit backups`) are no longer read; never delete or rewrite them.
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
