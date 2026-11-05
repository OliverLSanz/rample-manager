---
status: accepted
---

# Make the open Patch the main screen

ADR 0001 made the Library and its Patches the application's model, but the
main window kept showing the older folder session: it read WAVs from a chosen
folder and offered **Open folder…**, **Check formats** and **Export kit**. Patches
were reachable only through a **Patches…** dialog that duplicated the Kit list,
voice cards and sequencer with a plainer interface. Users met two workspaces
for the same job, and migrating made it worse, because the dialog then replaced
the main window. Decided by Oliver on 2026-11-05.

- The main window always shows the open Patch of the Library. Its sidebar lists
  that Patch's Kits; the voice cards show the selected Kit's Voices with their
  ordered Layers; the sequencer is the Patch's.
- The Patch switcher at the top only chooses, creates, renames or imports a
  Patch, then returns to the main window with that Patch loaded. It also holds
  the Library's **Default Layer mode**.
- Kits are created and re-addressed in the sidebar. Layers are selected,
  repeated, reordered, removed and added on the voice cards.
- **Build to SD…** and Destination backups sit at the top of the main window.
  **Export kit** is retired: Build Patch is the way to write a Kit to a card.
- The folder session is no longer shown. While it has not migrated (and is not
  the bundled demo), the main window offers **Move current folder into
  Library** once per launch; it can be hidden. The folder sample-review dialog,
  **Check formats**, **Open folder…**, file watching from the UI and the folder
  editor's **Save to sample** are no longer reachable. Their backend commands
  remain for migration and are removed separately.

Consequences:

- Library backup and restore still require a completed folder migration, so a
  Library that only ever held the demo cannot be backed up yet. This needs its
  own decision.
- Library samples stays a dialog until a full Samples screen replaces it.
- Deleting and duplicating Kits and Patches are not available yet; they need
  new Patch actions with undo.
