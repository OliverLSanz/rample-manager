# Rample Preview

A desktop sample auditioning and kit-building app for Squarp Rample / Rample Turbo.
Runs on **macOS and Windows** with a shared React interface and native Rust audio
engine. Everything works locally and offline; no account or server is required.

## Install

- **macOS:** open the DMG and copy **Rample Preview** to Applications. The build
  currently targets Apple Silicon and macOS 14+. Codex installs local changes with
  `./scripts/install.sh`; the previous Swift app is kept as a local backup.
- **Windows:** run the x64 `.exe` installer. It installs for the current user. Windows 10/11 x64 and the WebView2 runtime are required;
  the installer can install WebView2 if needed. A publisher signing certificate
  is not configured, so Windows may display an unknown-publisher warning.

Installers are attached to [private GitHub releases](https://github.com/OliverLSanz/rample-preview/releases). The **Desktop builds** workflow
also keeps macOS DMG and Windows EXE artifacts for each successful code build.
Updates on Oliver's Mac remain fully automated through the local install script.
Hosted automatic updates are not enabled; the installers can upgrade an existing
installation while keeping preferences and sample libraries.

## Use

Open **Library samples…** to import mono or stereo WAV, AIFF, FLAC and MP3 files
into the app's private Library. Folder import includes nested folders and does
not require Rample kit names or voice prefixes. Each Sample gets a stable identity
and an initial name from its source filename. Identical file bytes are recognized
across filenames, locations and restarts, retaining the existing Sample name.
The results show each imported, recognized or failed file; an incomplete import
is reported explicitly. **Original** auditions the unchanged source channels
independently of the kit mixer and sample editor. **Rample mono** auditions a
separate target preparation: channel-average mono PCM16 WAV at 44.1 kHz, with
band-limited resampling and dither where needed. Only audio shorter than 50 ms
is padded. **Stop audition** stops either audition.

Select a Sample in the Library to edit its descriptive name and tags (one tag per
line). Tags can contain spaces, punctuation and Unicode text. Search matches a
literal, case-insensitive part of a name or tag. Renaming and tagging do not rename
Original files or change Sample identity. Reimporting known audio retains this
metadata, including after restarting the app. Samples with the same name remain
separate entries; selecting or auditioning a row always uses its Sample ID.

**Move to Trash** retires a Sample from ordinary Library searches and the Add
Samples panel. Open **Trash** to audition or **Restore** it. Existing Layers keep their
Sample identity and saved sound, including after restarting. Restore makes the
same Sample available again without copying audio or changing its Kit references.
Reimporting known retired audio reports the existing Trash entry and offers
**Restore Sample**. Retirement retains Originals and saved history; permanent
purge is not available.

Completed imports preserve all Original bytes privately and work after removing
the source. WAV playback reads its Original directly. Other supported formats
use a disposable source-rate, channel-preserving float64 WAV playback artifact,
without making that artifact or the mono Rample preparation the Library master.
Decode and preparation use the bundled Rust codecs, work in bounded chunks off
the audio callback, and need no FFmpeg installation. Playback artifacts currently
support mono/stereo audio up to 384 kHz and standard RIFF size limits; files that
cannot be completely decoded are reported individually.

The main window always shows the open Patch
([ADR 0008](docs/adr/0008-make-the-open-patch-the-main-screen.md)). Its name
at the top opens the Patch switcher: choose another Patch, create, rename,
duplicate or delete one, or **Import Rample SD…**. Each choice returns to the
main window with that Patch loaded. All Patches share the Library's Samples.

The sidebar lists the open Patch's Kits in Rample order (A0, A1 … Z99), grouped
by Bank; click a Bank to fold it (the selected Kit's Bank stays open). Search
matches a Kit address or the name of any Sample in it, and a warning marks a Kit
that Build would refuse. **+** creates a Kit at the next free address; change the
Bank letter or Kit number before confirming. A Kit's **⋯** menu changes its
address without copying any audio, duplicates it to another address (new Layer
occurrences of the same Samples, same modes and selections) or deletes it. Used
addresses are refused as you type. With a Kit focused, ↑/↓ select the previous or
next Kit and Delete removes it.

Deleting a Kit or Patch happens at once and shows **Undo** (also Cmd/Ctrl+Z) for
ten seconds. Undo restores it exactly, with the same identities, even after
other edits, until the next deletion, a Library restore or quitting the app.
A deleted imported Patch stops being recognized when its card is imported
again; undo restores that recognition. A duplicated Patch starts without the
original's SD Import report.

Each voice card lists its Voice's ordered Layers. Click a Layer to select it;
hover it to move it up or down, **repeat** it (intentionally adding another
occurrence of the same Sample) or remove just that occurrence, or drag it to
another position in its Voice. Selection follows the occurrence when its
position changes, even for duplicates. All five Layer modes use this saved order;
new Voices use the Library's Default Layer mode. The waveform shows the Layer you
hear: the selected one in Single sample mode, otherwise the one last triggered.
Kit preview uses the Library's mono Rample preparation.

**Add Sample** on a voice card opens the **Add Samples** panel beside the Kit,
targeting that Voice; the 1–4 buttons change the target. Search matches part of
a name or tag. ↑/↓ browse and, with **Hear while browsing**, play each Sample as
the Rample will (mono preparation, independently of the mixer); Enter,
double-click or **+** adds it as the Voice's last Layer. The panel stays open,
so you can keep auditioning and adding, also while switching Kits. Drag any row
onto a voice card to add it there. Long Libraries show 150 matches at a time.

Select a Layer and click the sliders button beside **Audition** to edit its complete Original. Trim,
gain, varispeed pitch, EQ, echo and reverb are saved as reversible Sample Edits;
reopening retains the controls and full waveform, so trims can expand and effects
can be reduced. Drafts affect only that Layer occurrence, including repeated uses
of the same Sample. Bypass compares against its saved sound; closing discards the
draft. **Overwrite shared Sample** lists every affected Layer across Patches and
updates their shared sound. **Save as new** names an independent Variant, retains
tags and the shared Original, and retargets only the selected occurrence. Saved
Revisions remain available for reproducible future exports. See
[reversible Sample Edits](docs/reversible-sample-edits.md) for the rendering contract.

Below the voice cards are the four-channel, 16-step sequencer, tempo, velocities,
volume, mute and solo controls. Modes and selected Layers are remembered per Kit;
patterns, mixer and tempo persist within each Patch when changing Kits. Musical
controls reuse prepared audio, so changing a step or stopping playback does not
reread/decode long Originals; opening the app, the Library or the editor
revalidates assets. Incomplete drafts remain editable: missing Voice 1 and more
than 12 Layers are listed under **issues to fix before Build**, retaining every
Layer. Patches save locally in the private Library database. These controls
never write native Rample settings (`_save`). To use a mode on the device, set
it on the Rample and STORE.

| Shortcut                               | Action            |
| -------------------------------------- | ----------------- |
| Space                                  | Play / stop       |
| 1–4                                    | Audition a voice  |
| Option/Alt-click or right-click a step | Edit its velocity |

Shortcuts leave text fields and focused buttons alone. The **Pattern** menu loads
an example beat or clears the sequence.

**Import Rample SD…** copies an SD root or local copy into managed storage and
reconstructs its Bank/Kit addresses, Voice ownership and numerical/alphabetical
Layer order. Exact Originals reuse existing Samples, preserving their names,
tags and Trash identity. An unchanged arrangement and native state opens its
previous Patch, including when imported from a different mount location. A
changed intake requires **Create new Patch** or **Update existing Patch**; update
retains the Patch name, tempo, sequencer/mixer memory and matching Layer selection.

Usable audio survives malformed layouts and individual failures. Invalid source
assignments stay unassigned rather than being guessed; use **Add Sample** in
the intended Voice to place them explicitly. Excess Layers are retained. The
**SD Import report** keeps historical source issues separately from current
draft validation. A partial copy reports the failures and asks you to retain the
source for retry. Completed copies, pending reviews, and committed Patches survive
removing the source and restarting.

Native files in `_save` and other non-audio files are retained as exact bytes in
the Library database. Known operating-system filesystem metadata is ignored.
Import reads each Kit's STOREd Layer modes and selected Layers. Kits never saved
with STORE use your **Default Layer mode**, which you set in the Patch switcher
and which also applies to new Kits. The Rample's global LAYER setting
is not read. Unverified or unreadable values use that default and are listed
in the review. A source with Kits but no `_save` folder at all (for example a
copy of only the Kit folders) is reported too: no native settings were found,
so every Kit uses your Default Layer mode.

**Build to SD…** (Build Patch) writes the open Patch to a folder or directly onto an SD
card, empty or populated. Every Kit and every ordered Layer, including repeated
occurrences, becomes a Bank/Kit folder and a mono PCM16 44.1 kHz WAV of at
least 50 ms rendered from its Original and saved Sample Edits, exactly as Kit
preview plays it. Files are named `<Voice>_<position>_<name>.wav`, for example
`A0/1_01_Warm_kick.wav` and `A0/1_02_Snare.wav`, so the Rample's
numerical/alphabetical sort keeps the intended Layer order; Library names are
unchanged. Build first checks Voice 1, the 12-Layer limit and all audio,
listing every problem.

Before anything is written, Build shows a review of the destination: files to
add, files to replace, files to remove and files already up to date. Build
owns only WAV files directly inside `A0`–`Z99` Kit folders: the card's Kit
audio is made to match the Patch, so Kits that are not in the Patch lose their
audio and their folder is removed when nothing else is left in it. Native
settings (`_save`), firmware, other files and folders, and non-canonical names
such as `A05` are never changed or backed up. Names that would collide on a
card that ignores letter case (for example an `a0` folder) block the build
until you move them, and so does a non-canonical folder that **Import Rample
SD…** would read as a built or kept Kit (for example `A05` when the Patch
builds `A5`), because reimporting the card would keep only one of them. **Apply Build** first rechecks that the Patch and the
reviewed Kit audio are unchanged; if anything changed, nothing is written and
you review again. It then renders every Layer, copies every file it will
replace or remove into `Patch build backups` in the application support
folder (with a `restore.json` manifest listing paths and SHA-256), verifies
those copies and only then updates the card, rechecking each file right before
it is replaced or removed so audio saved in the meantime is never overwritten. Files that already hold the
Patch's current audio are left alone, so rebuilding an unchanged Patch writes
nothing. A failure after publication starts is reported as an incomplete
Build with the number of changes applied, never as a complete one.
Originals are never modified. Built files are recognized by **Import Rample
SD…** and **Import samples**, so reimporting a build keeps Samples and their
edits instead of adding new Originals. Build never writes `_save`, Layer modes,
selected Layers, BPM or patterns: set modes on the Rample and STORE. Settings
stored on the device that no longer match its files are yours to update there.

The clock button beside **Build to SD…** opens **Destination backups**, which lists every Build that changed a folder or card, with
its Patch, destination, time and status (complete, incomplete or interrupted);
a Build's result links straight to its backup. **Restore…** asks for
confirmation, then returns that destination to its state before the Build:
files the Build added are removed, files it replaced or removed get their
backed-up bytes back (verified by SHA-256), and Kit folders it created or
removed are undone. Use **Restore another folder…** when the card mounts
elsewhere. Restore only changes files that still hold that Build's output or
are missing; if anything changed since, nothing is changed and the files are
listed. To step back several Builds, restore the newest first. Restoring again
is harmless. `_save` and all other content stay untouched.

A folder session from an earlier version is offered on the main window as
**Move current folder into Library** (hide it with ×; it returns on the next launch). Migration copies
and validates audio in bounded chunks, backs up the saved session byte for byte as
`session-before-managed.backup.json`, and preserves Kits, selected filenames,
all five modes, each Kit's remembered choices, mixed source Kits across Voices,
BPM, patterns, velocities and mixer state. External audio and native configuration
stay untouched. Previously baked audio becomes an unedited Original with its current
sound; transient old editor controls cannot be reconstructed.

Progress shows the current file and file count. A failed copy, missing selected
filename or failed activation leaves the folder session usable, reports the problem,
and retains verified copies for retry. Restore a missing selected file before retrying;
migration never silently chooses another Layer. After success, its Kits are a
Patch like any other. Restart uses private audio without scanning or watching
the source, which can be disconnected.

Use **Library backup…** in the sidebar to create a complete
portable snapshot, or replace the Library with a verified snapshot from another
machine. Restore first creates a recoverable snapshot of the current Library and
shows its location, including on later failures. **Restore recovery snapshot…**
returns to that saved state. Complete folder migration first so its musical state
is included. See [portable Library snapshots](docs/library-snapshots.md) for the
format, validation and interruption/recovery behavior.

## Layer playback

| App mode                | Rample mode    | Behavior                                     |
| ----------------------- | -------------- | -------------------------------------------- |
| Single sample (default) | MANUAL         | Trigger only the selected sample             |
| Random                  | RANDOM         | Choose a layer per trigger, allowing repeats |
| Sequential              | CYCLIC         | Advance in natural filename order, then wrap |
| Reverse sequential      | REVERSE CYCLIC | Advance backwards, then wrap                 |
| Velocity                | VELOCITY       | Select a layer using the step's velocity     |

Automatic modes use up to 12 readable layers. Cyclic modes start at the selected
layer when transport starts, advancing on triggers rather than empty steps.
Audition uses the same cursor and mode, with step 1's velocity. The waveform and
layer counter show the triggered layer. Velocity selects a layer in even zones
without changing amplitude. Exact hardware velocity boundaries and cyclic
initialization still require a device comparison.

Modes and selected layers are remembered independently per voice and Kit.
Patterns, velocities, BPM and mixer belong to the Patch; browsing Kits keeps the
same beat. Mono and stereo audition work; stereo pairing on the module, its DAC
and its effects are not emulated.

### Native Rample SD settings are read-only

The app manages audio files and never creates, modifies or deletes `_save/*.rpl`
([ADR 0007](docs/adr/0007-read-native-settings-and-never-write-save.md)).
Import only reads verified fields to fill initial modes. Device settings that
no longer match the files are adjusted on the Rample. The verified mappings and
the current limits of reading are in
[the mappings note](docs/research/rample-native-settings-mappings.md).
[The format study](docs/research/rample-save-format-study.es.md) is historical
reference only: no further device captures are planned.

## Session preservation

The private Library lives in `Library/` under the same app support folder as the
legacy session. After restore, its active store is selected from protected
`Library/Generations/` by immutable activation records. `library.sqlite3` stores stable Sample/Original identities, names,
audio metadata and SHA-256 recognition; Original locators are relative to this
store, with no dependency on external source paths. SQLite is bundled on both
platforms. Complete verified Originals are published before their database
references are committed. Interrupted temporary copies are discarded on restart;
a complete file interrupted before its database commit stays unreferenced and
is never presented as a completed import. Missing/damaged assets are reported,
and audition checks their fingerprint rather than substituting another file.
`Derived/` holds regenerable Rample preparations and decoded Original playback
files. Removing these cache files does not remove Samples or Originals; the next
preparation rebuilds them from the managed Original. The cache version and target
are part of its key; it is never used for Import identity.

The legacy session remains separate:

- macOS: `~/Library/Application Support/Rample Preview/session.json`
- Windows: `%APPDATA%/Rample Preview/session.json`

The same folder contains `Demo SD`. Swift 1.x sessions migrate automatically,
keeping all selections, per-kit modes, patterns, velocity, tempo and mixer. The
original JSON is backed up as `session-swift-1.1.0.backup.json` before migration.
Installing/updating the app never deletes this folder. `RAMPLE_DATA_DIR` provides
an isolated support directory for development and tests.

## Develop and build

Requires Node 22+, stable Rust and platform build tools: Xcode command-line tools
on Mac, Microsoft C++ Build Tools and WebView2 on Windows.

```sh
npm ci
npm run tauri dev
```

Validation:

```sh
npm run build
npm test
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Package on the target operating system:

```sh
# macOS
npm run tauri -- build --bundles app,dmg
./scripts/install.sh
# Windows
npm run tauri -- build --bundles nsis
```

The installer builds before closing the current app, signs locally, replaces the
bundle and reopens it. `--no-launch` skips reopening; `RAMPLE_INSTALL_DIR` changes
the installation directory. Mac builds are signed ad hoc, not notarized for other
computers. Windows installers are currently unsigned.

## Architecture and validation

`frontend/` holds the React UI and IPC: `App.tsx` is the Patch main screen,
with `KitList`, `PatchMenu` and `VoiceCard`, and `patchView.ts` its pure view
model. `crates/rample-core/` holds SD scanning, WAV validation, export,
backwards-compatible session data, layer selection and the sample-clock
renderer. `src-tauri/` integrates the Library, Patches, native dialogs and
devices. CPAL outputs through CoreAudio on Mac and WASAPI on Windows.

Triggers are rendered against the device sample clock, independently of browser
or UI timers. Each track retriggers monophonically at the correct frame. WAVs
stream with bounded heads and background readers; decoding and disk I/O stay off
the callback. Matched sample rates preserve PCM exactly; other rates use a
32-tap windowed-sinc resampler. Device reconnection is retried automatically.

Headless tests check actual PCM, trigger boundaries, modes, mute/solo, resampling,
long-file streaming, migration, export, watcher events and UI behavior on both
operating systems. Actual-device smoke testing is separate:

```sh
cargo run -p rample-core --example audio_smoke --features native-audio
```

The Windows CI build verifies compilation, tests, packaging, native app startup,
graceful close saving the first session, and a reinstall that preserves preferences
(`scripts/smoke-windows.ps1`). Startup readiness uses the native window and Library
store because opening the app does not rewrite an existing session. This
cannot replace a listening test on physical Windows audio hardware. The original Swift sources
are archived under `legacy/swift` for reference and rollback.

Keep UI text in English and commit/push each atomic functional change with its
relevant tests and docs; see [AGENTS.md](AGENTS.md).
