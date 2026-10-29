# Rample Preview

This repository builds Oliver's installed macOS app at `/Applications/Rample Preview.app`.
It is a native SwiftUI / AVFoundation executable, without third-party dependencies.

When asked to change the app, implement the change here, run `swift test`, then run
`./scripts/install.sh` to rebuild, sign locally, replace and reopen the installed app.
Installing updates is part of the requested workflow; no manual copy or approval is needed.
Do not delete user preferences or sample libraries during updates.

## Git workflow

Keep all user-facing app text in English, including menus, tooltips, dialogs,
accessibility labels and errors. Preserve users' sample filenames.

Make a separate commit for every atomic functional change. Include its relevant
tests and documentation in the same commit. Run the appropriate checks before
committing, and push each completed commit to the private GitHub repository so
the remote stays up to date. Do not combine unrelated functional changes or
leave finished changes only in the local checkout.

User session: `~/Library/Application Support/Rample Preview/session.json`.
The demo is generated in that same support folder under `Demo SD/A0`.
Source sample folders must only be read. Export must copy into a new kit directory
and refuse to overwrite existing kits. Preserve selection, patterns, mixer and BPM.

Use the official Squarp Rample manual for SD compatibility rules:
https://squarp.net/rample/manual/

## Native SD configuration

The five layer modes are implemented, defaulting to Single sample (MANUAL).
Per-kit layer choices and modes currently persist locally in session.json.
The app does not yet import or write native `_save/*.rpl` files. This is a format
verification dependency, not a missing user authorization to implement SD sync.

Oliver will collect controlled STORE/SAVE SETTINGS examples; follow
`docs/rample-sd-capture.es.md`. Public Romper research suggests CBOR maps with
`layer_modes`, `selected_layer`, and global `layerMode`, but the numeric mapping
and firmware compatibility must be checked against those device saves before
native import or writing. Do not guess offsets, substitute MIDI enum values,
or claim local JSON is recognized by the Rample. Preserve all unknown native
fields and unrelated settings when implementing the eventual writer, and retain
recoverable originals. Sequencer steps and BPM remain app-only.
