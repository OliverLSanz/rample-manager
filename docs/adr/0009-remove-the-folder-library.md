---
status: accepted
supersedes: the folder migration in 0001 and 0008
---

# Remove the folder library and its migration

ADR 0001 kept the older folder workflow (a folder of WAVs, its session file,
format reviews, in-place conversion and **Save to sample**) until each user
migrated it into the managed Library, and ADR 0008 kept offering that migration
on the main window. Oliver decided on 2026-11-05 to remove the folder workflow
entirely and reimport his sounds by hand.

- The app only has the managed Library and its Patches. There is no folder
  session, file watching, format review, in-place conversion, **Save to
  sample**, **Export kit** or demo folder, and no migration or migration offer.
- A new Library starts empty, with **Start a Patch**. Sounds come in through
  **Import audio files…**, **Import folder…** and **Import Rample SD…**.
- Library backup and restore no longer require a completed migration. The
  retired `legacy_migration` table stays, empty, in every Library so snapshots
  from all versions keep one schema; nothing reads it.
- Files the folder workflow left in the support folder (`session.json`, Swift
  session backups, `Demo SD`, `sample-reviews.json`, `Conversion backups`,
  `Sample edit backups`) are no longer read and are never deleted.
- Patches migrated earlier keep working, including Voices that play from
  another Kit (`voice_kits`).
