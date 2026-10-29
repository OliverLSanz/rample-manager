# Rample Preview

A native macOS sample auditioning app for Squarp Rample / Rample Turbo libraries.
Built with SwiftUI and AVFoundation, without third-party dependencies, accounts
or a server. Requires Apple Silicon and macOS 14 or later.

## Use

Open `/Applications/Rample Preview.app`, click **Open folder…**, and choose the
root containing the kit folders, rather than an individual kit:

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

Select a kit in the sidebar. Each voice's sample menu selects a layer; its kit
menu lets you mix voices from different kits. Click sequencer steps to toggle
hits. Each step is a sixteenth note; 16 steps make one bar in 4/4.

| Control | Action |
| --- | --- |
| Space | Play / stop |
| 1–4 | Audition a voice |
| ⌘O | Open sample folder |
| ⌘R | Force refresh |
| ⇧⌘E | Export kit |
| M / S | Mute / solo a voice |

The **Pattern** menu loads an example beat or clears all steps. Each voice has
its own volume control. Playback shortcuts are disabled while editing text.

## Live refresh and persistence

FSEvents watches the library recursively, with a 300 ms debounce. Both in-place
edits and atomic replacements invalidate audio and waveforms. New files appear
in the sample menu. Removing a selected sample silences the voice and displays
**Sample deleted**; restoring its filename reloads it. Patterns stay intact.

A metadata scan every four seconds covers filesystems that miss events and
disconnected SD cards. **⌘R** bypasses the cache. Scanning, audio scheduling and
UI drawing run independently. WAV files stream from disk; waveform previews
sample 100 small windows distributed across the file.

## Layer playback modes

Each voice has a mode menu below its sample name:

| App mode | Rample mode | Behavior |
| --- | --- | --- |
| Single sample (default) | MANUAL | Trigger only the selected sample |
| Random | RANDOM | Choose a layer per trigger, with repeats allowed |
| Sequential | CYCLIC | Advance through layers, then wrap |
| Reverse sequential | REVERSE CYCLIC | Advance backwards, then wrap |
| Velocity | VELOCITY | Select a layer using the step's velocity |

Automatic modes use up to 12 readable layers in natural filename order. Cyclic
modes start with the selected layer, restart from it when transport starts,
and advance on triggers rather than on empty steps. Audition also uses the
voice's mode and advances the same cursor; in Velocity mode it uses step 1's
velocity. The layer counter and waveform show the most recently triggered layer.

**Option-click a step**, or right-click it and choose **Edit velocity…**, to set
its velocity from 1–127. Velocity mode displays these numbers on enabled steps.
It selects layers in even velocity zones, without changing amplitude. Exact
zone boundaries and cyclic initialization on hardware still need comparison.

The chosen layer and mode are remembered independently for each voice, kit and
library on this Mac. Patterns, step velocities, BPM and mixer remain session
controls, so browsing kits keeps the same beat. Old sessions migrate to Single
sample without losing the selected files or patterns.

### Native SD settings: pending hardware verification

App persistence is currently local JSON. The app does **not** read, create or
modify native Rample `_save/*.rpl` files. Public investigation identifies CBOR
kit maps with `layer_modes` and `selected_layer`, but their meanings and firmware
compatibility need controlled saves from Oliver's module before import/write
support can be enabled. Export still copies one selected sample per voice; it
does not encode layer modes or the sequencer for the module.

See the [step-by-step capture guide (Spanish)](docs/rample-sd-capture.es.md) to
generate the controlled STORE/SAVE SETTINGS examples needed for this next step.

Evidence: [Squarp manual](https://squarp.net/rample/manual/) and
[Romper's hardware-save investigation](https://github.com/peteb4ker/romper/blob/main/docs/developer/rample-save-integration.md).

The library, selected samples, pattern, tempo and mixer are saved to:

```text
~/Library/Application Support/Rample Preview/session.json
```

The generated demo library lives in `Demo SD` in that same support directory.
App updates preserve this folder. Original WAVs are never modified.

## Hardware compatibility

The app checks the [official Squarp format](https://squarp.net/rample/manual/):
kit folders A0–Z99 in the SD root; sample prefixes 1–4; uncompressed integer PCM
WAV at 44.1 kHz, 8 or 16 bits, at least 50 ms long, and up to 12 layers per voice.
Warnings identify files the Mac can play but the module may reject. Symlinks
are skipped to keep scanning within the chosen library.

Mono and stereo WAVs can be auditioned. The app does not emulate Rample effects,
DAC or stereo voice pairing. Each sequencer track is monophonic; a trigger
restarts its sample. Manual audition can play over the sequencer.

**Export kit** copies selected mono WAVs into a new A0–Z99 folder and validates
the copies before publishing the folder. Voice 1 is required. Existing kits are
never overwritten. Stereo samples can be auditioned, but export does not
rearrange the two hardware voices they consume. Mixer and sequencer settings
are listening controls and are not baked into exported WAVs.

## Build, install and update

Requires Swift/Xcode tools installed on this Mac. No package downloads needed.

```sh
cd /Users/oliver/oli/rample-preview
swift test
./scripts/install.sh
```

The installer builds release, generates the icon and `.app` bundle, signs it
locally, quits an existing instance, replaces `/Applications/Rample Preview.app`
and reopens it. Session data and sample libraries are preserved. Codex uses
this same command to apply future changes without manual installation steps.

`--no-launch` installs without opening. `RAMPLE_INSTALL_DIR="$HOME/Applications"`
changes the destination. `swift run` launches a development build. The app is
signed locally for this Mac, not notarized for distribution to other Macs.

## Development

Keep the app UI in English. Commit and push every atomic functional change,
including its relevant validation and documentation; see [AGENTS.md](AGENTS.md).

`swift test` covers SD layout, natural layer ordering, WAV metadata, compatibility
warnings, same-size and atomic edits, additions/deletions, symlinks, recursive
watching, layer limits and non-destructive export. Audio tests check real PCM
at the mixer, host-clock sequencer progression, and reloading/muting four voices
during playback.
