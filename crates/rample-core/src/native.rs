//! Bounded, read-only interpretation of Rample `_save` native settings.
//!
//! The device stores each `.rpl` file as one definite-length CBOR item. The
//! decoder keeps every key and value (including unknown ones) and rejects
//! anything outside its bounds instead of guessing. Callers must keep the
//! original bytes: interpretation never replaces the preserved file.
//!
//! Verified mappings come from controlled single-change STORE captures on a
//! Rample Turbo running firmware 2.00 (2026-11-04). Only per-Kit Layer modes and
//! selections are interpreted; global settings are ignored and the app uses
//! the user's own default Layer mode (ADR 0007). See
//! `docs/research/rample-native-settings-mappings.md`.
use crate::model::LayerMode;
use thiserror::Error;

/// Firmware on which the interpreted mappings were confirmed by device saves.
pub const VERIFIED_FIRMWARE: &str = "2.00";
/// Native files are a few hundred bytes; anything larger is not a known shape.
pub const MAX_NATIVE_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 16;
const MAX_ITEMS: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unsigned(u64),
    /// CBOR negative integer `-1 - n`, stored as `n` to avoid overflow.
    Negative(u64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    /// Insertion order is the file order.
    Map(Vec<(Value, Value)>),
    Tag(u64, Box<Value>),
    Bool(bool),
    Null,
    Undefined,
    Simple(u8),
    /// Raw IEEE bits and their encoded width in bytes (2, 4 or 8).
    Float {
        bits: u64,
        width: u8,
    },
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find(|(k, _)| matches!(k, Value::Text(t) if t == key))
                .map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_unsigned(&self) -> Option<u64> {
        match self {
            Value::Unsigned(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeError {
    #[error("file is empty")]
    Empty,
    #[error("file is larger than {MAX_NATIVE_BYTES} bytes")]
    TooLarge,
    #[error("file ends before its content is complete")]
    Truncated,
    #[error("{0} unexpected byte(s) follow the content")]
    Trailing(usize),
    #[error("nesting is deeper than supported")]
    TooDeep,
    #[error("content has more items than supported")]
    TooManyItems,
    #[error("indefinite-length or reserved encoding is not supported")]
    Unsupported,
    #[error("text is not valid UTF-8")]
    InvalidText,
    #[error("a map repeats the key {0}")]
    DuplicateKey(String),
}

/// Decodes exactly one CBOR item occupying the whole input.
pub fn decode(bytes: &[u8]) -> Result<Value, NativeError> {
    if bytes.is_empty() {
        return Err(NativeError::Empty);
    }
    if bytes.len() > MAX_NATIVE_BYTES {
        return Err(NativeError::TooLarge);
    }
    let mut reader = Reader {
        bytes,
        at: 0,
        items: 0,
    };
    let value = reader.item(0)?;
    match bytes.len() - reader.at {
        0 => Ok(value),
        rest => Err(NativeError::Trailing(rest)),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    items: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], NativeError> {
        let end = self.at.checked_add(n).ok_or(NativeError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(NativeError::Truncated)?;
        self.at = end;
        Ok(slice)
    }
    fn argument(&mut self, info: u8) -> Result<u64, NativeError> {
        Ok(match info {
            0..=23 => u64::from(info),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap())),
            26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().unwrap())),
            27 => u64::from_be_bytes(self.take(8)?.try_into().unwrap()),
            _ => return Err(NativeError::Unsupported),
        })
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }
    fn count(&mut self, n: u64, min_bytes_each: usize) -> Result<usize, NativeError> {
        // Every element needs at least one byte, so a declared count larger
        // than the remaining input is truncation, never an allocation.
        let n = usize::try_from(n).map_err(|_| NativeError::Truncated)?;
        if n.saturating_mul(min_bytes_each) > self.remaining() {
            return Err(NativeError::Truncated);
        }
        self.items = self.items.saturating_add(n);
        if self.items > MAX_ITEMS {
            return Err(NativeError::TooManyItems);
        }
        Ok(n)
    }
    fn item(&mut self, depth: usize) -> Result<Value, NativeError> {
        if depth > MAX_DEPTH {
            return Err(NativeError::TooDeep);
        }
        let initial = self.take(1)?[0];
        let (major, info) = (initial >> 5, initial & 0x1f);
        if info == 31 {
            return Err(NativeError::Unsupported);
        }
        if major == 7 {
            return Ok(match info {
                20 => Value::Bool(false),
                21 => Value::Bool(true),
                22 => Value::Null,
                23 => Value::Undefined,
                0..=19 => Value::Simple(info),
                24 => Value::Simple(self.take(1)?[0]),
                25..=27 => {
                    let width = 1u8 << (info - 24);
                    Value::Float {
                        bits: self.argument(info)?,
                        width,
                    }
                }
                _ => return Err(NativeError::Unsupported),
            });
        }
        let argument = self.argument(info)?;
        Ok(match major {
            0 => Value::Unsigned(argument),
            1 => Value::Negative(argument),
            2 | 3 => {
                let n = self.count(argument, 1)?;
                let raw = self.take(n)?.to_vec();
                if major == 2 {
                    Value::Bytes(raw)
                } else {
                    Value::Text(String::from_utf8(raw).map_err(|_| NativeError::InvalidText)?)
                }
            }
            4 => {
                let n = self.count(argument, 1)?;
                let mut out = Vec::with_capacity(n);
                for _ in 0..n {
                    out.push(self.item(depth + 1)?);
                }
                Value::Array(out)
            }
            5 => {
                let n = self.count(argument, 2)?;
                let mut out: Vec<(Value, Value)> = Vec::with_capacity(n);
                for _ in 0..n {
                    let key = self.item(depth + 1)?;
                    if out.iter().any(|(k, _)| *k == key) {
                        return Err(NativeError::DuplicateKey(match key {
                            Value::Text(t) => t,
                            other => format!("{other:?}"),
                        }));
                    }
                    let value = self.item(depth + 1)?;
                    out.push((key, value));
                }
                Value::Map(out)
            }
            6 => Value::Tag(argument, Box::new(self.item(depth + 1)?)),
            _ => unreachable!("three-bit major type"),
        })
    }
}

/// Encodes with shortest-form arguments, preserving map order and float widths.
/// Device files observed so far re-encode byte for byte; callers that need the
/// original bytes must keep them rather than rely on this. Only tests and
/// generated fixtures encode: the app never writes native settings (ADR 0007).
#[cfg(any(test, feature = "test-support"))]
pub fn encode(value: &Value) -> Vec<u8> {
    let mut out = vec![];
    write(&mut out, value);
    out
}
#[cfg(any(test, feature = "test-support"))]
fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let m = major << 5;
    match n {
        0..=23 => out.push(m | n as u8),
        24..=0xff => out.extend([m | 24, n as u8]),
        0x100..=0xffff => {
            out.push(m | 25);
            out.extend((n as u16).to_be_bytes())
        }
        0x1_0000..=0xffff_ffff => {
            out.push(m | 26);
            out.extend((n as u32).to_be_bytes())
        }
        _ => {
            out.push(m | 27);
            out.extend(n.to_be_bytes())
        }
    }
}
#[cfg(any(test, feature = "test-support"))]
fn write(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Unsigned(n) => head(out, 0, *n),
        Value::Negative(n) => head(out, 1, *n),
        Value::Bytes(b) => {
            head(out, 2, b.len() as u64);
            out.extend(b)
        }
        Value::Text(t) => {
            head(out, 3, t.len() as u64);
            out.extend(t.as_bytes())
        }
        Value::Array(items) => {
            head(out, 4, items.len() as u64);
            items.iter().for_each(|v| write(out, v))
        }
        Value::Map(entries) => {
            head(out, 5, entries.len() as u64);
            for (k, v) in entries {
                write(out, k);
                write(out, v)
            }
        }
        Value::Tag(tag, inner) => {
            head(out, 6, *tag);
            write(out, inner)
        }
        Value::Bool(b) => out.push(0xf4 | u8::from(*b)),
        Value::Null => out.push(0xf6),
        Value::Undefined => out.push(0xf7),
        Value::Simple(n) if *n < 24 => out.push(0xe0 | n),
        Value::Simple(n) => out.extend([0xf8, *n]),
        Value::Float { bits, width } => match width {
            2 => {
                out.push(0xf9);
                out.extend((*bits as u16).to_be_bytes())
            }
            4 => {
                out.push(0xfa);
                out.extend((*bits as u32).to_be_bytes())
            }
            _ => {
                out.push(0xfb);
                out.extend(bits.to_be_bytes())
            }
        },
    }
}

/// Per-Voice `layer_modes` value written by STORE (firmware 2.00, verified).
pub fn kit_layer_mode(raw: u64) -> Option<LayerMode> {
    Some(match raw {
        0 => LayerMode::Single,
        1 => LayerMode::Random,
        2 => LayerMode::Sequential,
        3 => LayerMode::Reverse,
        4 => LayerMode::Velocity,
        _ => return None,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KitVoiceSettings {
    pub raw_mode: u64,
    pub mode: Option<LayerMode>,
    /// Zero-based position in the Voice's sorted Layer files.
    pub selected_layer: u64,
}
/// The verified subset of a STOREd Kit file. Everything else stays opaque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KitSettings {
    pub voices: [KitVoiceSettings; 4],
}

fn four_unsigned(root: &Value, key: &str) -> Result<[u64; 4], String> {
    let Some(Value::Array(items)) = root.get(key) else {
        return Err(format!("`{key}` is missing or is not a list"));
    };
    let values: Vec<u64> = items.iter().filter_map(Value::as_unsigned).collect();
    if items.len() != 4 || values.len() != 4 {
        return Err(format!("`{key}` does not hold four non-negative numbers"));
    }
    Ok([values[0], values[1], values[2], values[3]])
}

/// Reads a `_save/<Kit>.rpl` file. Errors describe an unsupported shape;
/// the caller keeps the original bytes in every case.
pub fn read_kit(bytes: &[u8]) -> Result<KitSettings, String> {
    let root = decode(bytes).map_err(|e| e.to_string())?;
    if !matches!(root, Value::Map(_)) {
        return Err("content is not a settings map".into());
    }
    let modes = four_unsigned(&root, "layer_modes")?;
    let selected = four_unsigned(&root, "selected_layer")?;
    Ok(KitSettings {
        voices: std::array::from_fn(|i| KitVoiceSettings {
            raw_mode: modes[i],
            mode: kit_layer_mode(modes[i]),
            selected_layer: selected[i],
        }),
    })
}

#[cfg(any(test, feature = "test-support"))]
pub mod fixtures {
    //! Synthetic files shaped like firmware 2.00 saves. They are generated, not
    //! device captures, so they can be committed and changed freely.
    use super::*;
    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }
    fn four(values: [u64; 4]) -> Value {
        Value::Array(values.into_iter().map(Value::Unsigned).collect())
    }
    pub fn kit(layer_modes: [u64; 4], selected_layer: [u64; 4]) -> Vec<u8> {
        let assignment = || {
            Value::Map(vec![
                (text("param"), Value::Unsigned(9)),
                (text("voice"), Value::Unsigned(0)),
            ])
        };
        let row = || Value::Array(vec![Value::Bool(false); 4]);
        encode(&Value::Map(vec![
            (text("assignments"), Value::Array(vec![assignment(); 4])),
            (text("bitcrush"), four([127; 4])),
            (text("env"), four([127; 4])),
            (text("filter"), four([127; 4])),
            (text("freeze"), four([127; 4])),
            (text("layer_modes"), four(layer_modes)),
            (text("length"), four([254; 4])),
            (text("level"), four([127; 4])),
            (text("loop"), four([127; 4])),
            (text("mute_group"), Value::Array(vec![row(); 4])),
            (text("pitch"), four([127; 4])),
            (text("selected_layer"), four(selected_layer)),
            (text("start"), four([0; 4])),
        ]))
    }
    /// `settings.rpl` shape. The app preserves it but never interprets it.
    pub fn settings(layer_mode: u64) -> Vec<u8> {
        let entries: Vec<(&str, u64)> = vec![
            ("anti_clic", 1),
            ("assign", 0),
            ("autosave", 1),
            ("cv_in_range", 1),
            ("flip", 0),
            ("layerMode", layer_mode),
            ("midi_channel_in", 1),
            ("midi_velocity", 0),
            ("note_sp1", 128),
            ("note_sp2", 130),
            ("note_sp3", 132),
            ("note_sp4", 134),
            ("pitch_tracking_chromatic", 0),
            ("receive_pitchbend", 0),
            ("receive_progchange", 1),
            ("slicer_quantize_postv200", 2),
            ("vu_meters", 1),
        ];
        encode(&Value::Map(
            entries
                .into_iter()
                .map(|(k, v)| (text(k), Value::Unsigned(v)))
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_firmware_200_shapes_match_device_sizes_and_round_trip() {
        let kit = fixtures::kit([0; 4], [0; 4]);
        let settings = fixtures::settings(1);
        // Same sizes as firmware 2.00 saves of a default Kit and settings.
        assert_eq!((kit.len(), settings.len()), (277, 240));
        for bytes in [&kit, &settings] {
            assert_eq!(&encode(&decode(bytes).unwrap()), bytes);
        }
    }

    #[test]
    fn verified_kit_modes_and_zero_based_selection_are_interpreted() {
        let read = read_kit(&fixtures::kit([0, 1, 2, 3], [3, 0, 11, 1])).unwrap();
        let modes: Vec<_> = read.voices.iter().map(|v| v.mode).collect();
        assert_eq!(
            modes,
            [
                Some(LayerMode::Single),
                Some(LayerMode::Random),
                Some(LayerMode::Sequential),
                Some(LayerMode::Reverse)
            ]
        );
        assert_eq!(read.voices[0].selected_layer, 3);
        assert_eq!(
            read_kit(&fixtures::kit([4; 4], [0; 4])).unwrap().voices[3].mode,
            Some(LayerMode::Velocity)
        );
        let unknown = read_kit(&fixtures::kit([7, 0, 0, 0], [0; 4])).unwrap();
        assert_eq!(
            (unknown.voices[0].raw_mode, unknown.voices[0].mode),
            (7, None)
        );
    }

    #[test]
    fn unknown_keys_and_types_are_kept_and_do_not_block_known_fields() {
        let Value::Map(mut entries) = decode(&fixtures::kit([2; 4], [1; 4])).unwrap() else {
            panic!()
        };
        entries.push((
            Value::Text("zz_future".into()),
            Value::Tag(
                42,
                Box::new(Value::Array(vec![
                    Value::Negative(4),
                    Value::Float {
                        bits: 0x3c00,
                        width: 2,
                    },
                    Value::Bytes(vec![0, 255]),
                    Value::Null,
                ])),
            ),
        ));
        let bytes = encode(&Value::Map(entries));
        let decoded = decode(&bytes).unwrap();
        assert!(decoded.get("zz_future").is_some());
        assert_eq!(encode(&decoded), bytes);
        assert_eq!(
            read_kit(&bytes).unwrap().voices[1].mode,
            Some(LayerMode::Sequential)
        );
    }

    #[test]
    fn malformed_and_unsupported_files_are_rejected_without_guessing() {
        let kit = fixtures::kit([0; 4], [0; 4]);
        assert_eq!(decode(&[]), Err(NativeError::Empty));
        assert_eq!(decode(&kit[..kit.len() - 1]), Err(NativeError::Truncated));
        let mut trailing = kit.clone();
        trailing.push(0);
        assert_eq!(decode(&trailing), Err(NativeError::Trailing(1)));
        assert_eq!(decode(&[0xbf, 0xff]), Err(NativeError::Unsupported));
        assert_eq!(
            decode(&vec![0; MAX_NATIVE_BYTES + 1]),
            Err(NativeError::TooLarge)
        );
        // Huge declared lengths fail as truncation instead of allocating.
        assert_eq!(
            decode(&[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            Err(NativeError::Truncated)
        );
        assert_eq!(decode(&[0x81; 40]), Err(NativeError::TooDeep));
        assert_eq!(decode(&[0x62, 0xff, 0xfe]), Err(NativeError::InvalidText));
        assert_eq!(
            decode(&[0xa2, 0x61, b'a', 0, 0x61, b'a', 1]),
            Err(NativeError::DuplicateKey("a".into()))
        );
        // Recognized container, unsupported shape.
        let three = encode(&Value::Map(vec![
            (
                Value::Text("layer_modes".into()),
                Value::Array(vec![Value::Unsigned(0); 3]),
            ),
            (
                Value::Text("selected_layer".into()),
                Value::Array(vec![Value::Unsigned(0); 4]),
            ),
        ]));
        assert!(read_kit(&three).unwrap_err().contains("layer_modes"));
        assert!(read_kit(&encode(&Value::Array(vec![]))).is_err());
    }
}
