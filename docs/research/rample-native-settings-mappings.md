# Verified Rample native settings mappings

Status: verified for **reading** on Rample Turbo firmware **2.00**. The app never
writes `_save`
([ADR 0007](../adr/0007-read-native-settings-and-never-write-save.md)); these
mappings only fill initial modes when importing an SD.
For the full-format gap analysis and the next capture batches, see
[the format study](rample-save-format-study.es.md).

## Provenance

Oliver followed [the capture protocol](../rample-sd-capture.es.md) on
2026-11-04: Rample Turbo, firmware 2.00 (Settings → INFO), Kit `Z0` with twelve
Voice 1 Layers. Each capture copied `_save` after exactly one intentional
STORE or SAVE SETTINGS change. The captures stay private (ignored by git and
never committed). One deviation: the sixth capture selected Layer **4** rather
than Layer 2 (`06-manual-capa-4`). Oliver was unsure whether Layer 1 was
selected for the first capture. The stored value there is `0`, and capture 06
alone establishes the zero-based mapping.

Every file decoded as exactly one definite-length CBOR item and re-encoded byte
for byte with shortest-form arguments and original map order. That covers the
Kit file, `settings.rpl` and `global_assign.rpl`. The repository's synthetic
fixtures in `rample_core::native::fixtures` are byte-identical to the device's
default Kit and settings files for the same values. Diffs of the decoded values
between consecutive captures:

| Capture | Change on the device | Only decoded difference |
| --- | --- | --- |
| 01-manual | Baseline: STORE + SAVE SETTINGS | — |
| 02-random | Voice 1 → RANDOM, STORE | `Z0.layer_modes[0]` 0 → 1 |
| 03-cyclic | Voice 1 → CYCLIC, STORE | `layer_modes[0]` 1 → 2 |
| 04-reverse | Voice 1 → REVERSE CYCLIC, STORE | `layer_modes[0]` 2 → 3 |
| 05-velocity | Voice 1 → VELOCITY, STORE | `layer_modes[0]` 3 → 4 |
| 06-manual-capa-4 | Voice 1 → MANUAL, Layer 4 selected, STORE | `layer_modes[0]` 4 → 0, `selected_layer[0]` 0 → 3 |
| 07-global-manual | Settings → LAYER → MANUAL, SAVE SETTINGS | `settings.layerMode` 1 → 0 |
| 08-global-random | Settings → LAYER → RANDOM, SAVE SETTINGS | `settings.layerMode` 0 → 1 |

`Z1.rpl`, `global_assign.rpl` and the empty `autosave_Z0.rpl` never changed.

## Mappings used by SD Import

| File / key | Values | Status |
| --- | --- | --- |
| `_save/<Kit>.rpl` `layer_modes[voice]` | 0 MANUAL, 1 RANDOM, 2 CYCLIC, 3 REVERSE CYCLIC, 4 VELOCITY | Verified |
| `_save/<Kit>.rpl` `selected_layer[voice]` | Zero-based position among the Voice's sorted WAV files | Verified |

The global `settings.rpl` `layerMode` is **not read** (Oliver's decision,
2026-11-05). Captures 07/08 showed 0 = MANUAL and 1 = RANDOM, but the app
replaces the device's global LAYER with its own **Default Layer mode**,
configured in the Patches workspace and stored with the Library. That default
applies to new Kits, to imported Kits without their own `.rpl` (never saved
with STORE) and to unverified or unreadable stored values. The source of those
Kits is recorded as `appDefault`.

Layer files are assumed to follow standard natural filename order, as the
manual describes ("numerically and alphabetically sorted"). No further
sort-order captures are planned.

## Import behavior

- `_save` bytes are always preserved exactly, even when parsing fails. The
  reader is bounded (64 KiB, depth 16, item limit). It rejects truncation,
  trailing bytes, indefinite lengths, invalid UTF-8 and duplicate keys, and it
  never guesses a value.
- Unknown keys and value types are kept in the decoded tree but not interpreted.
- Unverified values and unreadable files use the user's Default Layer mode.
  Out-of-range or failed-import Layer positions select the first Layer. They are listed as
  native issues in the Import review and in Patch validation.
- Positions are counted over the Voice's `.wav` files in natural filename order,
  including files that failed to import. A failure therefore never silently
  shifts the selection onto a different file.
- Explicitly updating an existing Patch takes verified device values. App
  memory is kept only where nothing verified was read.
- Native interpretation is not part of the SD recognition fingerprint; the
  preserved native bytes already are.

## Known limits (no further captures planned)

- Sort order is assumed, not compared on the device, for case, punctuation,
  accents and leading zeros. An unusual name could make `selected_layer` point
  to a neighboring file.
- Firmware 3.00 and other versions are not verified. The files carry no explicit
  firmware version, so the reader cannot detect a different firmware.
- All other Kit and global fields (levels, filter, assignments, mute groups,
  MIDI settings and others) are preserved but not interpreted.
- Writing is out of scope (ADR 0007). Device settings that no longer match the
  files are adjusted on the Rample by the user.
