---
status: accepted
---

# Preserve Originals and save reversible Sample Edits

Keep the Original byte for byte and save Sample Edits as reversible settings:
reopening a saved Sample must allow widening a previous trim or reducing an
effect by processing the Original again, rather than processing already-baked
audio. Overwrite updates the Sample's saved edits and therefore every referencing
Layer; Save as new preserves the existing Sample and changes only the selected
Layer to a new independently editable sound. Device-compatible preview and build
audio is generated from the Original and saved edits without imposing mono or
Rample's encoding on the Library source; current Rample use remains mono.

An unsaved editor preview affects only the Layer from which the editor was opened.
Cancel restores its saved sound. Shared changes take effect only after Overwrite;
Save as new keeps the preview local to that Layer through its new Sample reference.
