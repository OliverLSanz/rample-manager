use rample_core::{
    catalog::{fingerprint, Scanner},
    conversion,
    review::{ReviewedFile, Reviews},
    wav,
};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
fn pcm(root: &Path, name: &str, rate: u32, frames: usize) -> PathBuf {
    let kit = root.join("A0");
    fs::create_dir_all(&kit).unwrap();
    let path = kit.join(name);
    wav::write_pcm16(&path, &vec![8000; frames], rate).unwrap();
    path
}
fn encoded(root: &Path, name: &str, hex: &str) -> PathBuf {
    let hex: String = hex.split_whitespace().collect();
    let data: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let path = root.join("A0").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, data).unwrap();
    path
}
#[test]
fn new_formats_are_detected_without_rewriting_or_repeated_prompts() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("SD");
    pcm(&root, "1_valid.wav", 44100, 4410);
    let path = pcm(&root, "2_rate.wav", 48000, 4800);
    let original = fs::read(&path).unwrap();
    let mut scanner = Scanner::default();
    let mut reviews = Reviews::default();
    let catalog = scanner.scan(&root, &HashSet::new(), false);
    let key = root.to_str().unwrap();
    reviews.update(key, &catalog);
    let pending = reviews.pending(key, &catalog);
    assert_eq!(pending.len(), 1);
    assert!(pending[0].convertible);
    assert_eq!(fs::read(&path).unwrap(), original);
    let support = temp.path().join("support");
    reviews.save(&support).unwrap();
    let mut restored = Reviews::load(&support).unwrap();
    assert_eq!(restored.pending(key, &catalog).len(), 1);
    restored.dismiss(
        key,
        &[ReviewedFile {
            id: pending[0].id.clone(),
            fingerprint: pending[0].fingerprint.clone(),
        }],
    );
    restored.save(&support).unwrap();
    let forced = scanner.scan(&root, &HashSet::new(), true);
    restored.update(key, &forced);
    assert!(restored.pending(key, &forced).is_empty());
    assert!(Reviews::load(&support)
        .unwrap()
        .pending(key, &forced)
        .is_empty());
    pcm(&root, "3_new.wav", 96000, 9600);
    let next = scanner.scan(&root, &HashSet::new(), false);
    restored.update(key, &next);
    assert_eq!(restored.pending(key, &next).len(), 1);
    restored.request_all(key, &next);
    assert_eq!(restored.pending(key, &next).len(), 2);
    fs::remove_file(&path).unwrap();
    let gone = scanner.scan(&root, &HashSet::new(), false);
    restored.update(key, &gone);
    assert!(restored
        .pending(key, &gone)
        .iter()
        .all(|c| c.id != path.to_string_lossy()));
}
#[test]
fn short_pcm_is_padded_without_changing_existing_samples() {
    let temp = TempDir::new().unwrap();
    let source = pcm(temp.path(), "1_short.wav", 44100, 100);
    let original = fs::read(&source).unwrap();
    let done =
        conversion::convert(&source, &fingerprint(&source), &temp.path().join("backups")).unwrap();
    let spec = wav::inspect(&source).unwrap();
    assert_eq!(spec.frames, 2205);
    assert_eq!(spec.bits, 16);
    assert_eq!(spec.format_tag, 1);
    let audio = wav::read_frames(&mut fs::File::open(&source).unwrap(), &spec, 0, 2205).unwrap();
    assert!(audio[..100].iter().all(|v| v[0] == 8000. / 32768.));
    assert!(audio[100..].iter().all(|v| v[0] == 0.));
    assert_eq!(fs::read(&done.backup).unwrap(), original);
    assert!(Path::new(&done.backup)
        .parent()
        .unwrap()
        .join("restore.json")
        .exists());
}
#[test]
fn sample_rate_conversion_preserves_duration_and_dc_level() {
    let temp = TempDir::new().unwrap();
    let source = pcm(temp.path(), "1_rate.wav", 48000, 4800);
    conversion::convert(&source, &fingerprint(&source), &temp.path().join("backups")).unwrap();
    let spec = wav::inspect(&source).unwrap();
    assert_eq!((spec.rate, spec.frames), (44100, 4410));
    let audio = wav::read_frames(&mut fs::File::open(&source).unwrap(), &spec, 0, 4410).unwrap();
    assert!(audio[100..4300]
        .iter()
        .all(|v| (v[0] - 8000. / 32768.).abs() < 0.0001));
}
#[test]
fn aiff_stereo_is_mixed_to_mono_renamed_and_backed_up() {
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
    let done =
        conversion::convert(&path, &fingerprint(&path), &temp.path().join("backups")).unwrap();
    assert!(!path.exists());
    assert_eq!(fs::read(&done.backup).unwrap(), bytes);
    let spec = wav::inspect(Path::new(&done.target)).unwrap();
    assert_eq!((spec.rate, spec.channels, spec.frames), (44100, 1, 4410));
    let audio =
        wav::read_frames(&mut fs::File::open(done.target).unwrap(), &spec, 100, 50).unwrap();
    assert!(audio.iter().all(|f| (f[0] - 2500. / 32768.).abs() < 0.0001));
}
#[test]
fn flac_and_mp3_decode_to_compatible_wavs() {
    let temp = TempDir::new().unwrap();
    for (name, hex, channels) in [
        ("1_stereo.flac", include_str!("fixtures/stereo-flac.hex"), 1),
        ("2_mono.mp3", include_str!("fixtures/mono-mp3.hex"), 1),
    ] {
        let path = encoded(temp.path(), name, hex);
        let original = fs::read(&path).unwrap();
        let done =
            conversion::convert(&path, &fingerprint(&path), &temp.path().join("backups")).unwrap();
        assert_eq!(fs::read(done.backup).unwrap(), original);
        assert!(!path.exists());
        let spec = wav::inspect(Path::new(&done.target)).unwrap();
        assert_eq!(
            (spec.format_tag, spec.bits, spec.rate, spec.channels),
            (1, 16, 44100, channels)
        );
        assert_eq!(spec.frames, 5292);
    }
}
#[test]
fn collisions_stale_reviews_and_corrupt_audio_preserve_files() {
    let temp = TempDir::new().unwrap();
    let source = encoded(
        temp.path(),
        "1_conflict.mp3",
        include_str!("fixtures/mono-mp3.hex"),
    );
    let target = source.with_extension("wav");
    fs::write(&target, b"existing user file").unwrap();
    let original = fs::read(&source).unwrap();
    assert!(
        conversion::convert(&source, &fingerprint(&source), &temp.path().join("backups")).is_err()
    );
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::read(&target).unwrap(), b"existing user file");
    let wave = pcm(temp.path(), "2_changed.wav", 48000, 4800);
    let stamp = fingerprint(&wave);
    fs::OpenOptions::new()
        .append(true)
        .open(&wave)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    let changed = fs::read(&wave).unwrap();
    assert!(conversion::convert(&wave, &stamp, &temp.path().join("backups")).is_err());
    assert_eq!(fs::read(&wave).unwrap(), changed);
    let broken = pcm(temp.path(), "3_broken.wav", 48000, 4800);
    let mut bytes = fs::read(&broken).unwrap();
    bytes.truncate(bytes.len() - 4000);
    fs::write(&broken, &bytes).unwrap();
    assert!(
        conversion::convert(&broken, &fingerprint(&broken), &temp.path().join("backups")).is_err()
    );
    assert_eq!(fs::read(&broken).unwrap(), bytes);
}
#[test]
fn structural_issues_do_not_offer_pointless_audio_conversion() {
    let temp = TempDir::new().unwrap();
    for i in 0..13 {
        pcm(temp.path(), &format!("1_{i}.wav"), 44100, 4410);
    }
    pcm(temp.path(), "wrong_name.wav", 44100, 4410);
    let loose = temp.path().join("1_loose.wav");
    wav::write_pcm16(&loose, &[0; 4410], 48000).unwrap();
    let catalog = Scanner::default().scan(temp.path(), &HashSet::new(), false);
    let mut reviews = Reviews::default();
    let root = temp.path().to_str().unwrap();
    reviews.update(root, &catalog);
    let pending = reviews.pending(root, &catalog);
    assert_eq!(pending.len(), 15);
    assert!(pending.iter().all(|c| !c.convertible));
    assert_eq!(catalog.unassigned.len(), 1);
    assert!(pending
        .iter()
        .any(|c| c.issues.iter().any(|i| i.contains("12"))));
}
#[test]
fn editing_the_original_during_conversion_prevents_replacement() {
    let temp = TempDir::new().unwrap();
    let source = pcm(temp.path(), "1_in_flight.wav", 96000, 192000);
    let path = source.clone();
    let backups = temp.path().join("backups");
    let observed = backups.clone();
    let thread = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if observed.exists()
                && walkdir::WalkDir::new(&observed)
                    .into_iter()
                    .filter_map(Result::ok)
                    .any(|e| e.file_name() == "restore.json")
            {
                fs::write(&path, b"edited while converting").unwrap();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("conversion did not create a backup");
    });
    let result = conversion::convert(&source, &fingerprint(&source), &backups);
    thread.join().unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&source).unwrap(), b"edited while converting");
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
fn float_high_bit_depth_and_multichannel_inputs_are_converted() {
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
        conversion::convert(&path, &fingerprint(&path), &temp.path().join("backups")).unwrap();
        let spec = wav::inspect(&path).unwrap();
        assert_eq!((spec.rate, spec.bits, spec.frames), (44100, 16, 4410));
        assert_eq!(spec.channels, 1);
        let audio = wav::read_frames(&mut fs::File::open(path).unwrap(), &spec, 100, 100).unwrap();
        let expected = if channels > 1 { 0.0625 } else { 0.25 };
        assert!(audio.iter().all(|f| (f[0] - expected).abs() < 0.0001));
    }
}
#[test]
fn stereo_layers_are_fixable_without_rewriting_compatible_mono_layers() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("SD");
    pcm(&root, "1_valid.wav", 44100, 4410);
    let mono = pcm(&root, "4_mono.wav", 44100, 4410);
    let original_mono = fs::read(&mono).unwrap();
    let stereo = raw_wave(temp.path(), 1, 16, 2, 44100, &vec![0.5; 8820]);
    let source = root.join("A0").join("4_stereo.wav");
    fs::rename(stereo, &source).unwrap();
    let mut scanner = Scanner::default();
    let mut reviews = Reviews::default();
    let key = root.to_str().unwrap();
    let catalog = scanner.scan(&root, &HashSet::new(), false);
    reviews.update(key, &catalog);
    let pending = reviews.pending(key, &catalog);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, source.to_string_lossy());
    assert!(pending[0].convertible);
    let converted = conversion::convert(
        &source,
        &pending[0].fingerprint,
        &temp.path().join("backups"),
    )
    .unwrap();
    assert_eq!(
        wav::inspect(Path::new(&converted.target)).unwrap().channels,
        1
    );
    assert_eq!(fs::read(mono).unwrap(), original_mono);
    let next = scanner.scan(&root, &HashSet::new(), true);
    assert!(next.kits[0].warnings.is_empty());
    reviews.update(key, &next);
    assert!(reviews.pending(key, &next).is_empty());
}
#[test]
fn mono_policy_reopens_old_stereo_reviews_once_and_respects_new_declines() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("SD");
    fs::create_dir_all(root.join("A0")).unwrap();
    let stereo = raw_wave(temp.path(), 1, 16, 2, 44100, &vec![0.5; 8820]);
    let source = root.join("A0").join("1_stereo.wav");
    fs::rename(stereo, &source).unwrap();
    let other = pcm(&root, "2_rate.wav", 48000, 4800);
    let key = root.to_str().unwrap();
    let support = temp.path().join("support");
    fs::create_dir(&support).unwrap();
    let legacy = serde_json::json!({"libraries": {key: {
        "fingerprints": {source.to_str().unwrap(): fingerprint(&source), other.to_str().unwrap(): fingerprint(&other)},
        "pending": []
    }}});
    fs::write(
        support.join("sample-reviews.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let mut reviews = Reviews::load(&support).unwrap();
    let catalog = Scanner::default().scan(&root, &HashSet::new(), false);
    // A stereo voice 1 can open on hardware even though this app now targets mono.
    assert!(!catalog.kits[0]
        .warnings
        .iter()
        .any(|w| w.contains("cannot open")));
    reviews.update(key, &catalog);
    let pending = reviews.pending(key, &catalog);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, source.to_string_lossy());
    reviews.dismiss(
        key,
        &[ReviewedFile {
            id: pending[0].id.clone(),
            fingerprint: pending[0].fingerprint.clone(),
        }],
    );
    reviews.save(&support).unwrap();
    let mut restored = Reviews::load(&support).unwrap();
    restored.update(key, &catalog);
    assert!(restored.pending(key, &catalog).is_empty());
    restored.request_all(key, &catalog);
    assert_eq!(restored.pending(key, &catalog).len(), 2);
}
#[test]
fn mono_mix_averages_both_channels_without_clipping_and_preserves_duration() {
    let temp = TempDir::new().unwrap();
    for (left, right, expected) in [(0.75, 0.75, 0.75), (0.5, 0., 0.25), (0.5, -0.5, 0.)] {
        let samples: Vec<_> = (0..4410).flat_map(|_| [left, right]).collect();
        let path = raw_wave(temp.path(), 1, 16, 2, 44100, &samples);
        conversion::convert(&path, &fingerprint(&path), &temp.path().join("backups")).unwrap();
        let spec = wav::inspect(&path).unwrap();
        assert_eq!((spec.channels, spec.frames), (1, 4410));
        let audio = wav::read_frames(&mut fs::File::open(path).unwrap(), &spec, 0, 4410).unwrap();
        assert!(audio
            .iter()
            .all(|frame| (frame[0] - expected).abs() <= 2. / 32768.));
    }
}
#[test]
fn conversion_resampling_rejects_aliasing_above_the_output_nyquist() {
    let temp = TempDir::new().unwrap();
    let rms = |frequency: f64| {
        let samples: Vec<_> = (0..4800)
            .map(|i| 0.5 * (std::f64::consts::TAU * frequency * i as f64 / 48000.).sin())
            .collect();
        let path = raw_wave(temp.path(), 3, 32, 1, 48000, &samples);
        conversion::convert(&path, &fingerprint(&path), &temp.path().join("backups")).unwrap();
        let spec = wav::inspect(&path).unwrap();
        let audio = wav::read_frames(&mut fs::File::open(path).unwrap(), &spec, 128, 4000).unwrap();
        (audio.iter().map(|f| f[0] as f64 * f[0] as f64).sum::<f64>() / audio.len() as f64).sqrt()
    };
    assert!((rms(1000.) - 0.35355).abs() < 0.002);
    assert!(rms(23000.) < 0.003);
}

#[test]
fn export_can_resolve_source_kit_structure_without_converting_valid_audio() {
    let source = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    for i in 0..13 {
        pcm(source.path(), &format!("1_{i}.wav"), 44100, 4410);
    }
    let catalog = Scanner::default().scan(source.path(), &HashSet::new(), false);
    let sample = catalog.kits[0].samples[0].clone();
    assert!(!sample.warnings.is_empty());
    assert!(!sample.needs_conversion);
    let target =
        rample_core::catalog::export(&[Some(sample), None, None, None], destination.path(), "B0")
            .unwrap();
    assert!(target.is_dir());
    let exported = Scanner::default().scan(destination.path(), &HashSet::new(), false);
    assert!(exported.kits[0].warnings.is_empty());
    assert_eq!(exported.kits[0].samples.len(), 1);
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
