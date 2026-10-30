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

Completed imports preserve all Original bytes privately and work after removing
the source. WAV playback reads its Original directly. Other supported formats
use a disposable source-rate, channel-preserving float64 WAV playback artifact,
without making that artifact or the mono Rample preparation the Library master.
Decode and preparation use the bundled Rust codecs, work in bounded chunks off
the audio callback, and need no FFmpeg installation. Playback artifacts currently
support mono/stereo audio up to 384 kHz and standard RIFF size limits; files that
cannot be completely decoded are reported individually. This coexists with the folder workflow
below; reversible managed Sample editing follows in later work. Legacy conversion and **Save to sample** cannot modify managed
Originals, and managed storage cannot be opened as a legacy folder.

Open **Patches…** to create multiple named Patches that share Library Samples.
Create Kits at addresses A0–Z99, select a Kit, or move its address without copying
any audio. Each Kit has four Voice columns. **Add Sample…** opens name/tag search
and audition; **Repeat** intentionally adds another occurrence of a Sample.
Select a Layer, reorder it with the arrows or remove just that occurrence.
Selection follows the occurrence when its position changes, even for duplicates.
All five Layer modes use this saved order; new Voices default to Single sample
(MANUAL). Kit preview uses the Library's mono Rample preparation.

The Patch workspace reuses the four-channel, 16-step sequencer, tempo, velocities,
volume, mute and solo controls. Modes and selected Layers are remembered per Kit;
patterns, mixer and tempo persist within each Patch when changing Kits. Musical
controls reuse prepared audio, so changing a step or stopping playback does not
reread/decode long Originals. **Refresh preview** explicitly revalidates assets.
Incomplete drafts remain editable: missing Voice 1 and more than 12 Layers are
reported visibly, retaining every Layer. Patches save locally in the private
Library database, independently of the folder session. **Back to folders** restores
that workflow. Native settings and complete Build Patch remain pending verified
device evidence; no `.rpl` files are read or written by these controls.

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

| Shortcut | Action |
| --- | --- |
| Space | Play / stop |
| 1–4 | Audition a voice |
| Cmd/Ctrl+O | Open library |
| Cmd/Ctrl+R | Force refresh |
| Shift+Cmd/Ctrl+E | Export kit |
| Option/Alt-click or right-click a step | Edit its velocity |

Playback shortcuts leave text fields alone. The **Pattern** menu loads an example
beat or clears the sequence. A generated demo library is included on first launch.

## Layer playback

| App mode | Rample mode | Behavior |
| --- | --- | --- |
| Single sample (default) | MANUAL | Trigger only the selected sample |
| Random | RANDOM | Choose a layer per trigger, allowing repeats |
| Sequential | CYCLIC | Advance in natural filename order, then wrap |
| Reverse sequential | REVERSE CYCLIC | Advance backwards, then wrap |
| Velocity | VELOCITY | Select a layer using the step's velocity |

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

### Native Rample SD settings remain pending

The app never modifies `_save/*.rpl`. Its JSON preferences are not native Rample
saves. Controlled hardware examples are still needed before implementing native
configuration import/write. See the [capture guide in Spanish](docs/rample-sd-capture.es.md).

## Session preservation

The private Library lives in `Library/` under the same app support folder as the
legacy session. `library.sqlite3` stores stable Sample/Original identities, names,
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

The Windows CI build verifies compilation, tests, packaging, native app startup
and a reinstall that preserves preferences (`scripts/smoke-windows.ps1`). This
cannot replace a listening test on physical Windows audio hardware. The original Swift sources
are archived under `legacy/swift` for reference and rollback.

Keep UI text in English and commit/push each atomic functional change with its
relevant tests and docs; see [AGENTS.md](AGENTS.md).
