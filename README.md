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

**Move to Trash** retires a Sample from ordinary Library searches and the add-Sample
picker. Open **Trash** to audition or **Restore** it. Existing Layers keep their
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
cannot be completely decoded are reported individually. The folder workflow below remains available until its session has migrated. Legacy conversion and **Save to sample** cannot modify managed
Originals, and managed storage cannot be opened as a legacy folder.

Open **Patches…** to create multiple named Patches that share Library Samples.
Create Kits at addresses A0–Z99, select a Kit, or move its address without copying
any audio. Each Kit has four Voice columns. **Add Sample…** opens name/tag search
and audition; **Repeat** intentionally adds another occurrence of a Sample.
Select a Layer, reorder it with the arrows or remove just that occurrence.
Selection follows the occurrence when its position changes, even for duplicates.
All five Layer modes use this saved order; new Voices default to Single sample
(MANUAL). Kit preview uses the Library's mono Rample preparation.

Select a Layer and choose **Edit Sample…** to edit its complete Original. Trim,
gain, varispeed pitch, EQ, echo and reverb are saved as reversible Sample Edits;
reopening retains the controls and full waveform, so trims can expand and effects
can be reduced. Drafts affect only that Layer occurrence, including repeated uses
of the same Sample. Bypass compares against its saved sound; closing discards the
draft. **Overwrite shared Sample** lists every affected Layer across Patches and
updates their shared sound. **Save as new** names an independent Variant, retains
tags and the shared Original, and retargets only the selected occurrence. Saved
Revisions remain available for reproducible future exports. See
[reversible Sample Edits](docs/reversible-sample-edits.md) for the rendering contract.

The Patch workspace reuses the four-channel, 16-step sequencer, tempo, velocities,
volume, mute and solo controls. Modes and selected Layers are remembered per Kit;
patterns, mixer and tempo persist within each Patch when changing Kits. Musical
controls reuse prepared audio, so changing a step or stopping playback does not
reread/decode long Originals. **Refresh preview** explicitly revalidates assets.
Incomplete drafts remain editable: missing Voice 1 and more than 12 Layers are
reported visibly, retaining every Layer. Patches save locally in the private
Library database, independently of the folder session. Before migration, **Back to folders** restores
that workflow. These controls never write native Rample settings (`_save`).
To use a mode on the device, set it on the Rample and STORE.

**Import Rample SD…** copies an SD root or local copy into managed storage and
reconstructs its Bank/Kit addresses, Voice ownership and numerical/alphabetical
Layer order. Exact Originals reuse existing Samples, preserving their names,
tags and Trash identity. An unchanged arrangement and native state opens its
previous Patch, including when imported from a different mount location. A
changed intake requires **Create new Patch** or **Update existing Patch**; update
retains the Patch name, tempo, sequencer/mixer memory and matching Layer selection.

Usable audio survives malformed layouts and individual failures. Invalid source
assignments stay unassigned rather than being guessed; use **Add Sample…** in
the intended Voice to place them explicitly. Excess Layers are retained. The
**SD Import report** keeps historical source issues separately from current
draft validation. A partial copy reports the failures and asks you to retain the
source for retry. Completed copies, pending reviews, and committed Patches survive
removing the source and restarting. Managed playback suspends the legacy folder
watcher; before migration, returning to folders resumes that separate workflow.

Native files in `_save` and other non-audio files are retained as exact bytes in
the Library database. Known operating-system filesystem metadata is ignored.
Import reads each Kit's STOREd Layer modes and selected Layers. Kits never saved
with STORE use your **Default Layer mode**, which you set in the Patches
workspace and which also applies to new Kits. The Rample's global LAYER setting
is not read. Unverified or unreadable values use that default and are listed
in the review.

**Build Patch…** writes the selected Patch to a folder or directly onto an SD
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
those copies and only then updates the card. Files that already hold the
Patch's current audio are left alone, so rebuilding an unchanged Patch writes
nothing. A failure after publication starts is reported as an incomplete
Build with the number of changes applied, never as a complete one.
Originals are never modified. Built files are recognized by **Import Rample
SD…** and **Import samples**, so reimporting a build keeps Samples and their
edits instead of adding new Originals. Build never writes `_save`, Layer modes,
selected Layers, BPM or patterns: set modes on the Rample and STORE. Settings
stored on the device that no longer match its files are yours to update there.

**Destination backups…** lists every Build that changed a folder or card, with
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

Existing folder sessions show **Move current folder into Library**. Migration copies
and validates audio in bounded chunks, backs up the saved session byte for byte as
`session-before-managed.backup.json`, and preserves Kits, selected filenames,
all five modes, each Kit's remembered choices, mixed source Kits across Voices,
BPM, patterns, velocities and mixer state. External audio and native configuration
stay untouched. Previously baked audio becomes an unedited Original with its current
sound; transient old editor controls cannot be reconstructed.

Progress shows the current file and file count. A failed copy, missing selected
filename or failed activation leaves the folder session usable, reports the problem,
and retains verified copies for retry. Restore a missing selected file before retrying;
migration never silently chooses another Layer. After success, the managed workspace
and reversible editor replace the folder conversion controls. Restart uses private
audio without scanning or watching the source, which can be disconnected.

Use **Library backup and restore…** in the managed workspace to create a complete
portable snapshot, or replace the Library with a verified snapshot from another
machine. Restore first creates a recoverable snapshot of the current Library and
shows its location, including on later failures. **Restore recovery snapshot…**
returns to that saved state. Complete folder migration first so its musical state
is included. See [portable Library snapshots](docs/library-snapshots.md) for the
format, validation and interruption/recovery behavior.

The following controls describe compatibility with folder sessions before migration.

Choose **Open folder…** and select the root containing your kits:

```text
My SD/
├── A0/
│   ├── 1_Kick.wav
│   ├── 1_Kick soft.wav
│   ├── 2_Snare.wav
│   ├── 3_Hat.wav
│   └── 4_Clap.wav
├── A1/
└── B12/
```

Choose a kit in the sidebar and a layer in each voice's sample menu. The source
kit menu lets you combine voices from different kits. The four tracks each have
16 sixteenth-note steps, volume, mute and solo. Set the tempo from 30–300 BPM.

| Shortcut                               | Action            |
| -------------------------------------- | ----------------- |
| Space                                  | Play / stop       |
| 1–4                                    | Audition a voice  |
| Cmd/Ctrl+O                             | Open library      |
| Cmd/Ctrl+R                             | Force refresh     |
| Shift+Cmd/Ctrl+E                       | Export kit        |
| Option/Alt-click or right-click a step | Edit its velocity |

Playback shortcuts leave text fields alone. The **Pattern** menu loads an example
beat or clears the sequence. A generated demo library is included on first launch.

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

Modes and selected layers are remembered independently per voice, kit and library.
Patterns, velocities, BPM and mixer are session controls; browsing kits keeps the
same beat. Mono and stereo audition work; stereo pairing on the module, its DAC
and its effects are not emulated.

## Live files and export

Native filesystem notifications watch the library recursively, with a 300 ms
quiet-period debounce and a four-second metadata fallback. Both in-place and
atomic WAV replacements reload audio/waveforms. New layers appear immediately.
A removed selection stays selected by filename but becomes silent; restoring the
file restores playback. Automatic modes continue using their remaining layers.

The [Squarp SD format](https://squarp.net/rample/manual/) is checked: A0–Z99 folders
in the root; filenames starting with 1–4; 44.1 kHz integer PCM WAV, 8/16 bits,
at least 50 ms, up to 12 layers per voice. Other readable PCM/float WAVs can be
auditioned with compatibility warnings. Symlinks inside the library are skipped.

### Review and convert new samples

New or changed audio is checked automatically, including when opening a library.
The **New samples detected** dialog lists each incompatibility and lets you choose
**Keep originals** or **Convert and replace** for the selected files. Compatible
files are left alone. Declining is remembered across refreshes and restarts;
**Check formats** opens the review again when you want it.

The built-in converter supports WAV, AIFF/AIFC, FLAC, MP3, Ogg Vorbis and supported
AAC/ALAC M4A files. It writes standard uncompressed **mono 16-bit PCM WAV at 44.1 kHz**.
Stereo is averaged as (L + R) / 2; additional channels are averaged too. Samples shorter
than 50 ms receive trailing silence. Resampling is band-limited and bit-depth
reduction uses dither. Non-WAV originals become a `.wav` with the same stem;
an existing destination WAV is never overwritten. The converter is bundled and
requires no FFmpeg or other external installation.

Before replacement, the original bytes are saved under **Conversion backups**
in the app support folder. **Show backups** opens that folder after conversion.
Each backup has a `restore.json` describing the original and converted paths plus
the original SHA-256. To restore, close playback and copy the backup to the
original path in that manifest; remove the converted `.wav` if its name differs.
The library detects the restored audio automatically.

Changes during conversion, unreadable/incomplete files and read-only sources are
reported without replacing the original. Invalid kit locations, wrong voice
prefixes, missing voice 1 and more than 12 layers are shown as issues requiring
manual changes. Mixed mono/stereo layers are fixed by converting the stereo
files to mono; compatible mono files are left alone. Stereo WAVs are offered for
conversion even when their other format settings already meet Rample requirements.
On upgrading to 2.2, previously reviewed stereo files are offered once again for
the new mono policy. Later declines are remembered as usual.
Review history lives in `sample-reviews.json` beside the musical session.

**Export kit** copies one selected compatible mono WAV per voice into a new
A0–Z99 directory. Voice 1 is required. Original WAVs and existing kits are never
overwritten. Export does not encode the sequencer, mixer or layer modes.

### Edit a sample in context

Click the sliders button beside **Audition** on a voice card to edit its selected
sample. The editor previews just this file, including when automatic layer modes
choose it; other layers and voices keep their sound. **Audition sample** always
plays this exact file. **Play sequence** lets you hear the edits in context.

- Drag either trim handle on the waveform, or enter start/end times in seconds.
- Adjust Gain, Pitch (±24 semitones), and the three EQ bands (120 Hz, 1 kHz, 8 kHz).
- Add Echo with amount, delay time and feedback, and/or Reverb with amount and decay.
- Toggle **Preview edits** to compare with the original. Changes reach native audio
  while playing; **Reset edits** returns controls to the current original file.
- Pitch changes playback speed and duration. Effect tails are included in the saved
  sample; repeated sequencer hits retrigger/cut those tails as on the hardware.
- **Save to sample** replaces the WAV with mono PCM16/44.1 kHz audio containing all
  edits. The original bytes and restore manifest are kept in **Sample edit backups**
  beside app preferences; **Show edit backups** opens that folder. Samples shorter
  than 50 ms are padded with silence. Lower Gain if the editor reports clipping.
- Closing the editor discards unsaved preview edits. Settings reset after saving,
  so the baked file is not processed twice. Mixer volume, mute/solo, sequencer
  steps, velocity and BPM are not baked into the source.

Live playback and saved output use the same bounded-memory DSP. The source stays
untouched until Save. If an external editor changes the file, reopen it before
saving so those changes are preserved. To undo a saved edit, close playback and
restore the backup to the original path described in its `restore.json`.

### Native Rample SD settings are read-only

The app manages audio files and never creates, modifies or deletes `_save/*.rpl`
([ADR 0007](docs/adr/0007-read-native-settings-and-never-write-save.md)).
Import only reads verified fields to fill initial modes. Device settings that
no longer match the files are adjusted on the Rample. Mappings and remaining
gaps are in [the mappings note](docs/research/rample-native-settings-mappings.md)
and [the format study](docs/research/rample-save-format-study.es.md).

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

`frontend/` holds React UI and IPC. `crates/rample-core/` holds SD scanning, WAV
validation, export, backwards-compatible session data, layer selection and the
sample-clock renderer. `src-tauri/` integrates native dialogs, filesystem watching
and devices. CPAL outputs through CoreAudio on Mac and WASAPI on Windows.

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
