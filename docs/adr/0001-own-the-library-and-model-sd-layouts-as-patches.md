---
status: accepted
---

# Own the Library and model SD layouts as Patches

The accepted target architecture copies imported audio into an application-managed
Library and stores Patch arrangements and metadata independently of the source
filesystem. A completed Import leaves the source untouched and no longer reads,
watches or depends on it; the source SD can be removed. This replaces the current
dependency on external paths so that Kits can be edited independently of Rample's
physical folder and filename conventions. Library backup and restore must include
audio, descriptive metadata and Patches and support moving between macOS and Windows.

The Library preserves imported files byte for byte, including stereo information
and file metadata, independently of any device's requirements. Rample-compatible
audio is derived according to its intended use, rather than replacing the Original.
Current scope remains mono Rample Kits; retaining stereo originals allows future
stereo use and other device targets without importing the source again. Kit preview
automatically uses the same device-compatible conversion as Build Patch, while
Library audition can play the Original. The external import source remains
irrelevant after its audio has been copied into the Library.

Import samples adds only sounds not already recognized by the Library. Import
Rample SD also reconstructs the SD arrangement as a Patch: reuse an unchanged,
previously imported Patch; when the arrangement differs, offer a new Patch or an
explicit update of an existing Patch rather than silently replacing it.

Import usable audio even when its source layout is incompatible with Rample, and
report individual failures explicitly. Allow incomplete or invalid Patch drafts,
but block Build Patch until the intended Kit layouts and audio are valid; never
silently discard or truncate Layers to make a Patch pass validation.

Migrate the current folder library and musical session into the managed model
while leaving the source untouched. Preserve Kits, selected Layers, playback
modes, patterns, mixer and BPM; previously baked edits retain their current sound
as imported audio because their old editor settings were not persisted.
