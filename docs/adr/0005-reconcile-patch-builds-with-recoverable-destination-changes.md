---
status: accepted
---

# Reconcile Patch builds with recoverable destination changes

Build Patch may target an empty directory or an existing SD root, with the musical
Kit layout matching the selected Patch rather than accumulating stale Kits.
Before replacing files or removing destination Kits absent from the Patch, show
the proposed changes and create a recoverable backup of all affected content;
preserve unrelated files, including firmware. This permits repeated SD updates
without requiring users to empty their cards manually, while keeping destructive
destination changes explicit and recoverable.
