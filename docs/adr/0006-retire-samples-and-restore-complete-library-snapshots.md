---
status: accepted
---

# Retire Samples and restore complete Library snapshots

Retiring a Sample moves it to recoverable Trash and hides it from the add-Sample
picker; existing Layers continue to resolve and play it, and shared Originals
remain available. Portable backup includes Originals, Sample Edits and Revisions,
Variants, names, tags, Patches, musical session, Trash and Import recognition
records. Restore replaces the complete active Library snapshot rather than
merging it, and first creates a recoverable backup of the current Library.

Importing an Original from the Samples view when every Sample that uses it is in
Trash revives it as a fresh import (filename name, no tags, no Sample Edits, no
Layers): an unused trashed Sample leaves Trash with its ID and keeps its
Revisions as history; if Layers still play them all, a new Sample shares the
Original. SD Import and exported edited audio still recognize the Trash entry.
