---
status: accepted
---

# Deduplicate Samples and share overwrite across Layers

SHA-256 is an Import recognition tool, not a Sample's identity. Deduplicate exact
Original file contents regardless of filename or path, storing identical Original
bytes only once. Multiple independently editable Samples may share an Original;
this refines the earlier rule that a file SHA could identify only one Sample.

Recognize app-exported edited audio as well as imported Originals: remember the
exact file hashes of exported audio and their originating Samples and processing
states, so importing those files again does not ingest the sound as a new Original
or lose its editable origin. Recognition must survive subsequent edits, restarts
and Library backup/restore. Different file metadata can change a file hash; this
decision does not require similarity-based matching of unrelated encodings.

A Sample's stable identity is separate from these hashes. Overwrite updates every
Layer referring to it across all Kits and Patches, with the scope made visible
before saving. Save as new creates an independently editable Sample sharing its
Original and changes only the currently edited occurrence.

Layers represent occurrences rather than owned copies of audio. Repeating a Sample
within a Voice is intentional and must preserve its distinct ordered positions.

Reimporting an older export preserves its exported sound even if its originating
Sample has since changed. Retain the corresponding Sample Revision and reuse an
existing matching Variant, or recover that Revision as an independent Variant
when necessary, without duplicating its Original or reverting the current Sample.
Such recovery can add a Library entry; it is not a fresh ingestion of known audio.
