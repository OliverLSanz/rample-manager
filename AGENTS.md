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
