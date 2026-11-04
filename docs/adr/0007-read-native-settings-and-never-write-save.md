---
status: accepted
supersedes: 0003
---

# Read native settings at Import and never write `_save`

The application manages audio files. Rample native settings in `_save` belong to
the device and its user. Decided by Oliver on 2026-11-05.

- **Import Rample SD** reads `_save` through the bounded reader. Where a Kit's
  own STOREd file has verified values, it fills that Kit's initial Layer modes
  and selected Layers. Everything else is preserved exactly with the Import
  record and reported honestly. Values are never guessed.
- The device's global LAYER setting is not read. The app has its own
  **Default Layer mode**, set by the user. It applies to new Kits, to imported
  Kits never saved with STORE and to unreadable or unverified stored values.
- Layer positions assume standard natural filename order. No further device
  captures are planned.
- **Build Patch** writes Bank/Kit folders and ordered, prepared audio files. It
  never creates, modifies, deletes, backs up or restores anything in `_save`.
  Complete output means complete audio files and folders, not native settings.
- App Layer modes and selections are preview settings. To use them on the
  Rample, set them on the device and STORE.
- Device settings that no longer match the files are the user's concern on the
  Rample. Examples are a `selected_layer` that now points at a different file,
  or a `.rpl` for a removed Kit. The app neither detects nor repairs them.

This supersedes [ADR 0003](0003-require-native-settings-for-complete-patch-builds.md).
Native format work is now only needed to improve what Import can read. Write
round trips on the device, templates for never-stored Kits and native backups
are out of scope. [ADR 0005](0005-reconcile-patch-builds-with-recoverable-destination-changes.md)
still governs audio and Kit folder changes, and `_save` counts as unrelated
content it must leave untouched.
