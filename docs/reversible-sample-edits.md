# Reversible Sample Edits

The managed Patch editor uses persisted recipes, independently of the legacy
folder editor's destructive **Save to sample** workflow. Originals retain their
imported bytes, encoding, channels and metadata. A Variant is a new Sample ID
with its own metadata and active Revision; it shares its parent's Original.

`sample_edits.rs` persists immutable `SampleRevision { id, settings }` records and
one current Revision per Sample. Initial unedited Revisions receive a stable ID
when first prepared. Overwrite appends a Revision and changes the current pointer.
Save as new atomically creates the Variant, its Revision, and the selected Layer
retarget in one SQLite transaction. Existing Revisions never change or disappear
when the active sound changes. Tags, Trash, Patch settings and musical session
remain independent. A retired source may produce an active new Variant.

`ManagedLibrary::prepare` provides the complete Original playback source,
including channel-preserving decoding for non-WAV imports. `prepare_rample`
resolves the current saved Revision. `prepare_revision(sample_id, revision_id)`
resolves a specific retained Revision, rejecting IDs belonging to another Sample.
The application equivalents are `prepare_library_sample`, `sample_revisions` and
`prepare_sample_revision`. Downstream export recognition must retain the actual
published output hash, Sample ID and Revision ID, rather than hash a recipe.

Processing order for edited Rample audio is complete Original playback source →
`editing::render` at 44.1 kHz (trim/varispeed, channel average, gain/EQ/echo/reverb)
→ dithered mono PCM16. The shared engine resets effects per trigger and writes
the full tail. Unedited Revisions retain the existing band-limited Rample
preparation policy. Preparation and complete validation happen in bounded chunks
on the command/control thread. Immutable derived files are disposable caches;
Originals and Revision records are the masters.

Drafts use this same final Rample rendering path, then replace only the selected
occurrence's prepared bank entry. Other occurrences and Voices keep saved assets.
Consequently draft, saved preview and revision export play the same prepared PCM
at the sample clock, including tails. Driver-rate conversion and Kit mixer gain
still apply during playback. Longer drafts show an Updating status while prepared;
no filesystem access or decoding occurs on the audio callback. Draft cache names
include a unique editor session and sequence and are discarded on restart.

Editor IDs, monotonically increasing preview sequences, selected occurrence and
expected saved Revision guard publication. The application lock serializes
preparation, save, bank refresh and editor reopen, preventing an older operation
from replacing a newer editor. Original integrity is rechecked during rendering.
Changed sources disable the editor until reopened. Cancel/bypass restores saved
banks; saving clears the draft and rebuilds the ready-bank cache so shared edits
are heard once. Failures after durable save explicitly report that edits were
saved even if preview refresh fails.

Owned-fixture tests use the agreed application-command boundary, the real offline
AudioHost/public Renderer PCM boundary, and visible controls with native IPC
mocked. They cover complete-range reopen, trim expansion/effect reduction,
retained revisions/restart, stereo preservation, local repeated occurrences,
cancel, stale operations, shared changes across Patches, independent Variants,
source changes, saved/draft PCM agreement and effect tails.
