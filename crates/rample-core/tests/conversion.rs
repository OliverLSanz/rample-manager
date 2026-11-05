//! Rample preparation: mono PCM16 WAV at 44.1 kHz from any supported Original.
use rample_core::{conversion, wav};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
fn pcm(root: &Path, name: &str, rate: u32, frames: usize) -> PathBuf {
    let path = root.join(name);
    wav::write_pcm16(&path, &vec![8000; frames], rate).unwrap();
    path
}
fn encoded(root: &Path, name: &str, hex: &str) -> PathBuf {
    let hex: String = hex.split_whitespace().collect();
    let data: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, data).unwrap();
    path
}
/// Prepares `source` beside it and checks the Original was left untouched.
fn prepare(source: &Path) -> PathBuf {
    let original = fs::read(source).unwrap();
    let target = source.with_extension("prepared.wav");
    conversion::prepare_rample(source, &mut fs::File::create(&target).unwrap()).unwrap();
    assert_eq!(fs::read(source).unwrap(), original);
    target
}
#[test]
fn short_pcm_is_padded_without_changing_existing_samples() {
    let temp = TempDir::new().unwrap();
    let source = prepare(&pcm(temp.path(), "short.wav", 44100, 100));
    let spec = wav::inspect(&source).unwrap();
    assert_eq!(spec.frames, 2205);
    assert_eq!(spec.bits, 16);
    assert_eq!(spec.format_tag, 1);
    let audio = wav::read_frames(&mut fs::File::open(&source).unwrap(), &spec, 0, 2205).unwrap();
    assert!(audio[..100].iter().all(|v| v[0] == 8000. / 32768.));
    assert!(audio[100..].iter().all(|v| v[0] == 0.));
}
#[test]
fn sample_rate_conversion_preserves_duration_and_dc_level() {
    let temp = TempDir::new().unwrap();
    let source = prepare(&pcm(temp.path(), "rate.wav", 48000, 4800));
    let spec = wav::inspect(&source).unwrap();
    assert_eq!((spec.rate, spec.frames), (44100, 4410));
    let audio = wav::read_frames(&mut fs::File::open(&source).unwrap(), &spec, 0, 4410).unwrap();
    assert!(audio[100..4300]
        .iter()
        .all(|v| (v[0] - 8000. / 32768.).abs() < 0.0001));
}
#[test]
fn aiff_stereo_is_mixed_to_mono() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("1_stereo.aiff");
    let frames = 4410u32;
    let mut bytes = b"FORM".to_vec();
    bytes.extend((46 + frames * 4).to_be_bytes());
    bytes.extend(b"AIFFCOMM");
    bytes.extend(18u32.to_be_bytes());
    bytes.extend(2u16.to_be_bytes());
    bytes.extend(frames.to_be_bytes());
    bytes.extend(16u16.to_be_bytes());
    bytes.extend([0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
    bytes.extend(b"SSND");
    bytes.extend((frames * 4 + 8).to_be_bytes());
    bytes.extend([0; 8]);
    for _ in 0..frames {
        bytes.extend(10000i16.to_be_bytes());
        bytes.extend((-5000i16).to_be_bytes());
    }
    fs::write(&path, &bytes).unwrap();
    let target = prepare(&path);
    let spec = wav::inspect(&target).unwrap();
    assert_eq!((spec.rate, spec.channels, spec.frames), (44100, 1, 4410));
    let audio = wav::read_frames(&mut fs::File::open(target).unwrap(), &spec, 100, 50).unwrap();
    assert!(audio.iter().all(|f| (f[0] - 2500. / 32768.).abs() < 0.0001));
}
#[test]
fn flac_and_mp3_decode_to_rample_wavs() {
    let temp = TempDir::new().unwrap();
    for (name, hex, channels) in [
        ("1_stereo.flac", include_str!("fixtures/stereo-flac.hex"), 1),
        ("2_mono.mp3", include_str!("fixtures/mono-mp3.hex"), 1),
    ] {
        let path = encoded(temp.path(), name, hex);
        let target = prepare(&path);
        let spec = wav::inspect(&target).unwrap();
        assert_eq!(
            (spec.format_tag, spec.bits, spec.rate, spec.channels),
            (1, 16, 44100, channels)
        );
        assert_eq!(spec.frames, 5292);
    }
}
fn raw_wave(
    root: &Path,
    code: u16,
    bits: u16,
    channels: u16,
    rate: u32,
    samples: &[f64],
) -> PathBuf {
    let path = root.join(format!("1_{code}_{bits}_{channels}_{rate}.wav"));
    let mut data = vec![];
    for value in samples {
        match (code, bits) {
            (3, 32) => data.extend((*value as f32).to_le_bytes()),
            (3, 64) => data.extend(value.to_le_bytes()),
            (1, 16) => data.extend(((*value * 32768.) as i16).to_le_bytes()),
            (1, 24) => data.extend(&((*value * 8388608.) as i32).to_le_bytes()[..3]),
            (1, 32) => data.extend(((*value * 2147483648.) as i32).to_le_bytes()),
            _ => unreachable!(),
        }
    }
    let align = channels * bits / 8;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data.len() as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(code.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * align as u32).to_le_bytes());
    bytes.extend(align.to_le_bytes());
    bytes.extend(bits.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((data.len() as u32).to_le_bytes());
    bytes.extend(data);
    fs::write(&path, bytes).unwrap();
    path
}
#[test]
fn float_high_bit_depth_and_multichannel_inputs_are_prepared() {
    let temp = TempDir::new().unwrap();
    for (code, bits, channels, rate) in [
        (3, 32, 2, 48000),
        (3, 64, 1, 44100),
        (1, 24, 1, 96000),
        (1, 32, 1, 44100),
        (1, 16, 4, 44100),
    ] {
        let samples: Vec<_> = (0..rate / 10)
            .flat_map(|_| {
                [0.25, -0.125, 0.5, -0.375]
                    .into_iter()
                    .take(channels as usize)
            })
            .collect();
        let path = raw_wave(temp.path(), code, bits, channels, rate, &samples);
        let path = prepare(&path);
        let spec = wav::inspect(&path).unwrap();
        assert_eq!((spec.rate, spec.bits, spec.frames), (44100, 16, 4410));
        assert_eq!(spec.channels, 1);
        let audio = wav::read_frames(&mut fs::File::open(path).unwrap(), &spec, 100, 100).unwrap();
        let expected = if channels > 1 { 0.0625 } else { 0.25 };
        assert!(audio.iter().all(|f| (f[0] - expected).abs() < 0.0001));
    }
}
#[test]
fn mono_mix_averages_both_channels_without_clipping_and_preserves_duration() {
    let temp = TempDir::new().unwrap();
    for (left, right, expected) in [(0.75, 0.75, 0.75), (0.5, 0., 0.25), (0.5, -0.5, 0.)] {
        let samples: Vec<_> = (0..4410).flat_map(|_| [left, right]).collect();
        let path = raw_wave(temp.path(), 1, 16, 2, 44100, &samples);
        let path = prepare(&path);
        let spec = wav::inspect(&path).unwrap();
        assert_eq!((spec.channels, spec.frames), (1, 4410));
        let audio = wav::read_frames(&mut fs::File::open(path).unwrap(), &spec, 0, 4410).unwrap();
        assert!(audio
            .iter()
            .all(|frame| (frame[0] - expected).abs() <= 2. / 32768.));
    }
}
#[test]
fn resampling_rejects_aliasing_above_the_output_nyquist() {
    let temp = TempDir::new().unwrap();
    let rms = |frequency: f64| {
        let samples: Vec<_> = (0..4800)
            .map(|i| 0.5 * (std::f64::consts::TAU * frequency * i as f64 / 48000.).sin())
            .collect();
        let path = raw_wave(temp.path(), 3, 32, 1, 48000, &samples);
        let path = prepare(&path);
        let spec = wav::inspect(&path).unwrap();
        let audio = wav::read_frames(&mut fs::File::open(path).unwrap(), &spec, 128, 4000).unwrap();
        (audio.iter().map(|f| f[0] as f64 * f[0] as f64).sum::<f64>() / audio.len() as f64).sqrt()
    };
    assert!((rms(1000.) - 0.35355).abs() < 0.002);
    assert!(rms(23000.) < 0.003);
}

#[test]
fn preparing_rample_audio_preserves_the_original_and_averages_channels() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("Stereo.aiff");
    let mut bytes = Vec::from(&b"FORM"[..]);
    bytes.extend(58u32.to_be_bytes());
    bytes.extend(b"AIFFCOMM");
    bytes.extend(18u32.to_be_bytes());
    bytes.extend(2u16.to_be_bytes());
    bytes.extend(2u32.to_be_bytes());
    bytes.extend(16u16.to_be_bytes());
    bytes.extend([0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
    bytes.extend(b"SSND");
    bytes.extend(16u32.to_be_bytes());
    bytes.extend([0; 8]);
    for sample in [16384i16, 0, -16384, 0] {
        bytes.extend(sample.to_be_bytes());
    }
    fs::write(&source, &bytes).unwrap();
    let target = temp.path().join("prepared.wav");
    conversion::prepare_rample(&source, &mut fs::File::create(&target).unwrap()).unwrap();
    let spec = wav::inspect(&target).unwrap();
    assert_eq!(
        (spec.rate, spec.channels, spec.bits, spec.frames),
        (44100, 1, 16, 2205)
    );
    let audio = wav::read_frames(&mut fs::File::open(target).unwrap(), &spec, 0, 2205).unwrap();
    assert!((audio[0][0] - 0.25).abs() < 0.0001);
    assert!((audio[1][0] + 0.25).abs() < 0.0001);
    assert!(audio[2..].iter().all(|f| f[0] == 0.));
    assert_eq!(fs::read(source).unwrap(), bytes);
}
