# Rample kit tools and native SD settings

Researched 2026-10-29. Scope: original tool repositories, deployed Ramplaid assets,
Squarp documentation and first-party community posts. Repository snapshots below
are pinned so later changes do not silently change this assessment. No third-party
application, package installation, build script or test suite was executed.

## Findings

Several kit tools publish source, but their audio-folder exporters do not establish
the native settings format. **Romper provides the strongest public evidence found:**
actual binary fixtures, a CBOR reader/encoder, raw-field views and preservation
tests. Its published fixtures establish the encoding of those files; they do not
establish all layer-mode values or prove that changed files load on hardware.
Its own implementation protects the card's `_save` folder from writes.
[Romper fixtures](https://github.com/peteb4ker/romper/tree/3539120c5f3c120233f34308b30a799ceeb0f9f7/tests/fixtures/rample-save),
[codec](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/cbor.ts),
[write protection](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/services/sdCardSafety.ts).

This narrows the unknowns substantially: we need not rediscover whether this public
fixture corpus is CBOR. Controlled mode captures and a hardware round trip remain
necessary under [ADR 0003](../adr/0003-require-native-settings-for-complete-patch-builds.md).
An audio export or preservation of an old `.rpl` alone cannot establish a complete
Build Patch when the intended native settings differ.

## Which tools are open source?

| Tool and reviewed snapshot | Source/license evidence | Native settings evidence and useful lessons |
| --- | --- | --- |
| [Ramplaid](https://ramplaid.com/), deployed footer v1.1.0 | No public original repository or application license identified in this search. Squarp credits Bálint Zalkai and links the tool. The deployed bundle links the creator's Instagram; its GitHub links concern dependencies. Dependency MIT banners do not license the application. | Kit arrangement, conversion and ZIP export. The inspected bundle has no `_save`, `.rpl`, `cbor` or `layerMode` strings; its export path adds prepared audio blobs to a ZIP. This is evidence about the inspected deployment, not proof about private source or future versions. [Official link](https://squarp.net/rample/manual/), [inspected bundle](https://ramplaid.com/assets/index-Cgds6icn.js), [creator's announcement](https://squarp.community/t/new-sample-editor/15752). |
| [Rample Kit Studio](https://github.com/canufeel/rample-kit-studio/tree/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581) | [MIT, © 2026 Petr Kosikhin](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/LICENSE). Browser React/TypeScript app. | Import explicitly skips native `.rpl` state, with a test for `_save`. Export plans WAV entries; layer modes are preview-only. README explicitly says exports have not been verified on hardware. Useful for arrangement UX, repeated Layers, deterministic export names and export planning. [Import/test](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/src/domain/cardImport.test.ts), [export](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/src/export/buildKit.ts), [status](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/README.md). |
| [rample-kitman](https://github.com/gnee-code/rample-kitman/tree/fc0358f9113d848b42db66d59062085e1cc70a2e) | [GPL version 3](https://github.com/gnee-code/rample-kitman/blob/fc0358f9113d848b42db66d59062085e1cc70a2e/LICENSE). Python/Streamlit web UI. | `SaveKit` creates a Kit folder and writes voice-prefixed PCM WAVs; the source contains no native CBOR/settings implementation. Study the workflow; do not treat its conversion comments as verified audio correctness. [Actual implementation](https://github.com/gnee-code/rample-kitman/blob/fc0358f9113d848b42db66d59062085e1cc70a2e/utilities.py). |
| [Romper](https://github.com/peteb4ker/romper/tree/3539120c5f3c120233f34308b30a799ceeb0f9f7) | [MIT, © 2025 Romper Development Team](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/LICENSE). Electron/React desktop app, rather than a browser-only tool. | Read-only native settings inspection, opaque backups, codec and real-card fixtures. No completed native settings editor/writer at this snapshot. [Reader](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/rampleSaveReader.ts), [backup integration test](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/tests/integration/sync-rample-save-backup.integration.test.ts). |
| [Rample-kit-generator](https://github.com/tog-touseg/Rample-kit-generator/tree/afbc068fed0aa33668102c7091f5d08867443ad0) and [salmple2rample](https://github.com/abluenautilus/salmple2rample/tree/dea9f79e80b0be46f7da7c30db1f742a14bef292) | Public Python source; no repository license found at these snapshots. Do not equate public visibility with permission to copy implementation. | Audio file organization/copying only; neither supplies a native settings codec or device-verified `.rpl` mappings. [Generator implementation](https://github.com/tog-touseg/Rample-kit-generator/blob/afbc068fed0aa33668102c7091f5d08867443ad0/rample_kit_generator.py), [converter implementation](https://github.com/abluenautilus/salmple2rample/blob/dea9f79e80b0be46f7da7c30db1f742a14bef292/salmple2rample.py). |

For implementation reuse, the two MIT licenses require retaining their copyright
and permission notices in copies or substantial portions. GPLv3 carries conditions
on conveying covered code and corresponding source; it is not the same reuse
option as MIT. Prefer independent implementation of observed format facts, or
explicitly attributed MIT reuse after checking the particular files and assets.
No third-party code or fixtures were copied into this repository during this
research. [Romper license](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/LICENSE),
[Studio license](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/LICENSE),
[kitman license](https://github.com/gnee-code/rample-kitman/blob/fc0358f9113d848b42db66d59062085e1cc70a2e/LICENSE).

## What the primary evidence establishes

Squarp distinguishes STORE (Kit parameters/assignments) from SAVE SETTINGS
(device settings). The global LAYER default applies to Kits never saved with
STORE. The manual documents five modes, numerical/alphabetical Layer filename
sorting, up to twelve Layers per Voice and homogeneous mono/stereo Layers within
a Voice; stereo occupies two outputs. It does not publish a `.rpl` schema.
[Manual](https://squarp.net/rample/manual/).

A Squarp staff reply independently confirms per-Kit `.rpl` files in `_save` and
that deleting the corresponding file resets a Kit. That reply acknowledges a
binary encoding without specifying CBOR or enum mappings.
[Thibault_Squarp, 2024-11-12](https://squarp.community/t/folder-settings-mishap/12026).

Romper's fixture README claims an unchanged copy from a real SD card on
2026-10-04. Its introducing commit identifies the maintainer's six files and
explicitly leaves the device firmware unconfirmed. This is traceable provenance
from the contributor, not independently witnessed device capture. Only
`fw-2.00-inferred` exists in the reviewed fixture tree; no controlled five-mode or
confirmed firmware-3.00 corpus was found there.
[Provenance](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/tests/fixtures/rample-save/README.md),
[introducing commit a6e99aaf](https://github.com/peteb4ker/romper/commit/a6e99aaf5c0b7ce1530dd81cab748e48face8cbe),
[fixture tree](https://github.com/peteb4ker/romper/tree/3539120c5f3c120233f34308b30a799ceeb0f9f7/tests/fixtures/rample-save).

For this investigation, an independent, temporary Python decoder/encoder read
the fixture bytes without importing Romper code. Each of the five nonempty files
consumed exactly one definite-length CBOR item and re-encoded byte for byte using
shortest integer/length forms and the original map order. These observations
also agree with Romper's fixture tests. This checks encoding, not device behavior.
[Bytes and upstream assertions](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/__tests__/rampleSaveFixtures.test.ts).

| File | Bytes | Values independently observed |
| --- | ---: | --- |
| `F7.rpl` | 277 | `layer_modes=[1,1,1,1]`, `selected_layer=[1,0,3,2]`, `filter=[189,127,127,127]` |
| `L1.rpl` | 276 | `layer_modes=[1,1,1,1]`, `selected_layer=[0,0,0,0]`, `level=[42,127,127,127]`, `loop=[0,127,127,127]` |
| `L4.rpl` | 277 | `layer_modes=[1,1,1,1]`, `selected_layer=[0,0,0,0]`, `level` and `loop` each `[127,127,127,127]` |
| `settings.rpl` | 240 | Map, including `layerMode=1`, `midi_channel_in=8`, `slicer_quantize_postv200=2` |
| `global_assign.rpl` | 61 | Four maps: `{param:9,voice:0}`, then voices 1, 2 and 3 |
| `autosave_C1.rpl` | 0 | Empty file; last-Kit marker meaning remains inferred |

All three Kit maps contain these exact thirteen keys:

```text
assignments, bitcrush, env, filter, freeze, layer_modes, length,
level, loop, mute_group, pitch, selected_layer, start
```

Most values are arrays of four unsigned integers. `assignments` is four
`{param,voice}` maps; `mute_group` is a 4×4 boolean matrix (all false in this
corpus). Keys are in plain bytewise string order, including nested maps. A
changed integer can change byte width, so fixed offsets are inappropriate.
There are no filename, Layer count, stereo, tempo or sequencer fields in these
observed Kit maps. Their absence here is not proof that every firmware uses the
same schema. [Fixture data](https://github.com/peteb4ker/romper/tree/3539120c5f3c120233f34308b30a799ceeb0f9f7/tests/fixtures/rample-save/fw-2.00-inferred),
[raw key model](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/shared/rampleSave.ts).

The observed settings map has these seventeen keys:

```text
anti_clic, assign, autosave, cv_in_range, flip, layerMode,
midi_channel_in, midi_velocity, note_sp1, note_sp2, note_sp3, note_sp4,
pitch_tracking_chromatic, receive_pitchbend, receive_progchange,
slicer_quantize_postv200, vu_meters
```

Their existence/spelling is established by bytes. Most value-to-control mappings
remain hypotheses. In particular, `layer_modes=1` probably denotes RANDOM,
`selected_layer` probably uses zero-based positions, and `layerMode` probably
stores the global default; none of the public fixtures is a labeled
MANUAL/RANDOM/CYCLIC/REVERSE CYCLIC/VELOCITY comparison. Do not replace those
unknowns with a presumed 0–4 enum or the MIDI chart's values. Romper's model
deliberately exposes raw numbers rather than verified mode names.
[Model](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/shared/rampleSave.ts),
[fixture tests](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/__tests__/rampleSaveFixtures.test.ts).

## Preservation and firmware limitations

Romper's codec retains map insertion order and opaque bytes for types outside
its interpreted subset. Its reader retains unknown keys, missing keys and raw
values, and reports whether re-encoding reproduces original bytes. It rejects
duplicates, truncation, trailing data, excessive size/depth and indefinite-length
items. This offers useful reader architecture, but not universal byte preservation:
ordinary integer widths are normalized to shortest form, and some well-formed
CBOR forms are rejected. Keep the untouched whole file even when parsing fails;
never reconstruct unknown settings from only a typed projection.
[Codec](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/cbor.ts),
[codec tests](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/__tests__/cbor.test.ts),
[reader tests including nonminimal integer encoding](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/__tests__/rampleSaveReader.test.ts).

The firmware detector is explicitly heuristic: it treats
`slicer_quantize_postv200` as a lower-bound marker and the absence of unknown
master-FX keys as evidence for pre-3.00. Absence cannot certify firmware; an old
file can survive a device update, and these files contain no explicit firmware
version. Do not use that guess as a writer compatibility guarantee.
[Detector](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/electron/main/rample/rampleSaveFirmware.ts).

Squarp's current firmware page lists 3.00 (2026-02-27). Version 2.00 changed
levels, Freeze, Bitcrusher and SLICER; 3.00 adds Tape/Compressor and changes effects
again. Older captures cannot establish the new storage fields or parameter
semantics. This is a concrete reason to record the device's INFO version and
capture freshly saved settings on Oliver's Turbo.
[Official changelog](https://squarp.net/rample/firmware/).

A useful caution for Layer ordering: Kit Studio implements locale-sensitive
numeric, case-insensitive comparison, and generates zero-padded positional
filenames. This is a design choice plus unit tests, not a hardware comparison for
punctuation, accents or case ties. Our ordered Layers should produce deterministic
device filenames and be tested on the device rather than assuming a browser
locale comparator is Squarp's implementation.
[Comparator](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/src/domain/cardImport.ts),
[export naming](https://github.com/canufeel/rample-kit-studio/blob/2efbb177ccbd2b11280b1e7ab7d2d825b80bc581/src/domain/filename.ts).

## Practical next steps for Rample Preview

1. Continue the managed Library/Patch implementation under the accepted ADRs.
   Preserve stereo Originals byte for byte; derive current mono device audio.
   Native decoding is a separate concern from the Library's Sample identity,
   edits and ordered Layer occurrences. This follows
   [ADR 0001](../adr/0001-own-the-library-and-model-sd-layouts-as-patches.md),
   [ADR 0002](../adr/0002-deduplicate-samples-and-share-overwrite-across-layers.md)
   and [ADR 0004](../adr/0004-preserve-originals-and-save-reversible-sample-edits.md).
2. A bounded, read-only native reader can now be designed against known public
   bytes. Keep complete original `_save` contents and per-file fingerprints in
   Patch Import state, expose uncertain values honestly, and preserve unrelated
   and unknown settings. Establish dependency/license attribution before vendoring
   any upstream fixture or code. The public corpus can bootstrap decoder tests;
   its hardware provenance must remain labeled as contributor-reported.
3. Retain the existing eight-capture protocol in
   [the capture guide](../rample-sd-capture.es.md): five Voice modes, a distinct
   manual Layer selection and the two global-default saves, with firmware INFO
   recorded. The public corpus does not eliminate any of these semantic checks.
   Use before/after decoded diffs, not MIDI enums or fixed byte offsets.
4. Verify the selected Layer against the actual ordered filenames and test a
   generated, minimally changed file on the target firmware, followed by STORE
   and re-read. Confirm other known settings and original unknown content survive
   the app operation. Check how missing/new fields behave before synthesizing
   settings for a never-saved Kit. Global settings and assignments need their own
   verified schemas; copying another user's defaults is not sufficient.
5. Gate native writing and the claim of complete Build Patch on those results,
   preserving recoverable destination originals under
   [ADR 0003](../adr/0003-require-native-settings-for-complete-patch-builds.md)
   and [ADR 0005](../adr/0005-reconcile-patch-builds-with-recoverable-destination-changes.md).
   Romper's remaining hardware checklist independently identifies the same gaps,
   but is a proposed validation plan, not evidence that its checks passed.
   [Upstream roadmap/checklist](https://github.com/peteb4ker/romper/blob/3539120c5f3c120233f34308b30a799ceeb0f9f7/docs/developer/rample-save-integration.md).

The actionable result is a credible starting point for native reading and
preservation, with a much smaller remaining hardware investigation. No inspected
tool supplies a verified, complete native writer that can simply be adopted.
