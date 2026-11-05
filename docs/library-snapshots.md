# Portable Library snapshots

Use **Library backup and restore…** in the managed Patch workspace. First complete
**Move current folder into Library** if a folder session is still active. This
makes the folder's music and audio portable before either backup or replacement;
a backup never silently omits an external folder session.

**Back up Library…** chooses a new snapshot folder name. Existing destinations and
locations inside the managed Library are refused. Copy the resulting entire folder
to another machine, then choose it with **Restore Library…**. The confirmation
explains that restoration replaces the whole Library. It does not merge collections.

A snapshot contains `manifest.json`, one consistent `library.sqlite3` image and
`Originals/<stable-id>.wav`. Original filenames here are internal locators; their
bytes retain the imported format, including stereo and compressed audio. User names,
tags and source-relative SD evidence remain in the database. The complete saved
state includes Samples, Trash, shared Originals, saved and historical Revisions,
Variants, every Patch and occurrence, musical controls and selection, opaque native
blobs, pending SD intake drafts, recognition history and the managed migration
marker. Musical session state is authoritative in the managed Patch graph. The
marker records that the folder session migrated; the Patch it created may have
been deleted since, so it is not required to exist.

Playing transport, unsaved editor previews, Derived caches, abandoned Incoming
files, audio-device settings, the old folder session and its external paths are not
part of a portable snapshot. Their exclusion does not remove saved music or edits.
A snapshot has no dependency on the source or support-folder location.

## Verification and recovery

Backup uses SQLite's consistent `VACUUM INTO` image while application mutations are
serialized. It stages and verifies the complete database and all Original copies
before publishing a completion manifest last. Interrupted copies without that
manifest are incomplete snapshots. Hashes detect changed contents; they are not
proof of who created a snapshot.

Restore reads only the fixed format files and synthesized, validated stable-ID
asset names. It rejects symbolic links, Windows junctions/reparse files, special
files, unsafe or colliding identities, unsupported snapshot/database versions or
SQL schema, broken database/domain references, invalid recipes and incomplete or
changed audio. Validation covers every saved Revision and SD/native/export
recognition reference, not only the current sound. Empty or musically incomplete
Patch drafts remain valid saved state; build warnings such as missing Voice 1 or
excess Layers are distinct from a corrupt graph.

Before activation, restore creates and verifies a portable recovery snapshot of the
current Library under `Library/Recovery/`. Its actual path is shown on success and
on any later failure. **Restore recovery snapshot…** uses the same confirmation and
replacement flow. Saved recovery snapshots are listed again after reopening the app,
and are fully reverified when selected. A validation or copy failure leaves the active Library unchanged.
A published recovery snapshot whose final sync failed is also reported with its
path, and replacement does not proceed.

## Activation and audio lifetime

Restoration prepares a fresh `Library/Generations/<id>/` without renaming, deleting
or overwriting any active audio or SQLite file. After staging and recovery, one
synced, immutable `Library/Activations/<sequence>.json` record commits the new
active generation. On Windows the namespace move uses non-replacing
[`MoveFileExW` with `MOVEFILE_WRITE_THROUGH`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw).
Startup resolves this record before opening SQLite, reading a legacy session or
starting source watchers. An interruption before the record keeps the previous
generation; one after it loads the new generation. Temporary records and abandoned
generations do not become active.

Generations and activation records remain inside the protected `Library/` boundary,
so legacy folder conversion/editor commands cannot reach their Originals. Old
generations are retained because audio readers can still hold Windows handles after
asynchronous Stop. There is no automatic permanent cleanup in this feature; recovery
snapshots and retained generations consume additional disk space.

Activation invalidates the old editor and Original-audition owners, clears prepared
banks and stops playback. Results distinguish failure before activation from an
activated Library whose final confirmation or playback refresh failed. For the
latter, reopen the application to reload the committed generation, or restore the
reported recovery snapshot. React refreshes the Patch and accepts the newer app
revision so delayed old UI results cannot reopen the displaced workspace.

The current portable format is version 1 with supported database schema 8. Unknown
schemas, triggers, views and newer versions are refused without changing the chosen
snapshot. Native device fields remain opaque; snapshot preservation does not claim
verified Rample configuration writing or hardware acceptance.
