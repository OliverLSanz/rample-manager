use rample_core::{
    audio::{Command, Meters, Renderer},
    catalog::{export, is_kit_name, Scanner},
    model::{LayerMode, LayerSelector, Voice},
    session,
    wav::{self, PreparedSample},
};
use std::{collections::HashSet, fs, path::Path, sync::Arc};
use tempfile::TempDir;
fn write(
    root: &Path,
    kit: &str,
    name: &str,
    frames: usize,
    rate: u32,
    value: i16,
) -> std::path::PathBuf {
    let dir = root.join(kit);
    fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    wav::write_pcm16(&p, &vec![value; frames], rate).unwrap();
    p
}
#[test]
fn kit_names_and_natural_order() {
    for s in ["A0", "A00", "Z99"] {
        assert!(is_kit_name(s))
    }
    for s in ["a0", "A100", "A1\n", "../A0", ""] {
        assert!(!is_kit_name(s))
    }
    let t = TempDir::new().unwrap();
    for (kit, name) in [
        ("A0", "1_Kick10.wav"),
        ("A0", "1_Kick2.WAV"),
        ("A2", "2_snare.wav"),
        ("A10", "1.wav"),
        ("other", "1.wav"),
        ("A0", "5_ignored.wav"),
    ] {
        write(t.path(), kit, name, 4410, 44100, 1200);
    }
    let c = Scanner::default().scan(t.path(), &HashSet::new(), false);
    assert_eq!(
        c.kits.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(),
        ["A0", "A2", "A10"]
    );
    assert_eq!(c.kits[0].samples[0].name, "1_Kick2");
    assert!(c.kits[0].warnings.iter().any(|w| w.contains("filenames")));
    assert!(!c.kits[1].warnings.is_empty());
}
#[test]
fn wav_metadata_and_compatibility() {
    let t = TempDir::new().unwrap();
    write(t.path(), "A0", "1_valid.wav", 4410, 44100, 1200);
    write(t.path(), "A0", "2_rate.wav", 4410, 48000, 1200);
    write(t.path(), "A0", "3_short.wav", 100, 44100, 1200);
    fs::write(t.path().join("A0/4_broken.wav"), b"bad").unwrap();
    let c = Scanner::default().scan(t.path(), &HashSet::new(), false);
    let s = &c.kits[0].samples;
    assert_eq!(s.len(), 4);
    assert_eq!(s[0].bits, 16);
    assert!((s[0].duration - 0.1).abs() < 1e-6);
    assert!(s[0].warnings.is_empty());
    assert!(s[1].warnings[0].contains("44.1"));
    assert!(s[2].warnings[0].contains("50 ms"));
    assert!(!s[3].readable);
}
#[test]
fn cache_invalidation_add_delete_and_atomic_save() {
    let t = TempDir::new().unwrap();
    let p = write(t.path(), "A0", "1.wav", 4410, 44100, 1200);
    let mut scanner = Scanner::default();
    let before = scanner.scan(t.path(), &HashSet::new(), false);
    let same = scanner.scan(t.path(), &HashSet::new(), false);
    assert_eq!(before, same);
    let forced = scanner.scan(t.path(), &HashSet::from([p.clone()]), false);
    assert_ne!(
        before.kits[0].samples[0].revision,
        forced.kits[0].samples[0].revision
    );
    let tmp = write(t.path(), "A0", "temp.wav", 8820, 44100, 2400);
    fs::rename(tmp, &p).unwrap();
    write(t.path(), "A0", "2.wav", 4410, 44100, 1200);
    let changed = scanner.scan(t.path(), &HashSet::new(), false);
    assert_eq!(changed.kits[0].samples.len(), 2);
    assert_eq!(changed.kits[0].samples[0].duration, 0.2);
    fs::remove_file(p).unwrap();
    assert_eq!(
        scanner.scan(t.path(), &HashSet::new(), false).kits[0]
            .samples
            .len(),
        1
    );
}
#[test]
fn layer_limit_and_missing_volume() {
    let t = TempDir::new().unwrap();
    for i in 0..13 {
        write(t.path(), "A0", &format!("1_{i}.wav"), 4410, 44100, 1200);
    }
    let c = Scanner::default().scan(t.path(), &HashSet::new(), false);
    assert!(c.kits[0].warnings.iter().any(|w| w.contains("supports 12")));
    assert!(Scanner::default()
        .scan(&t.path().join("missing"), &HashSet::new(), false)
        .error
        .is_some());
}
#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let t = TempDir::new().unwrap();
    let p = write(t.path(), "A0", "1.wav", 4410, 44100, 1000);
    std::os::unix::fs::symlink(&p, t.path().join("A0/2.wav")).unwrap();
    std::os::unix::fs::symlink(t.path().join("A0"), t.path().join("B0")).unwrap();
    let c = Scanner::default().scan(t.path(), &HashSet::new(), false);
    assert_eq!(c.kits.len(), 1);
    assert_eq!(c.kits[0].samples.len(), 1);
}
#[test]
fn export_copies_without_overwrite_or_native_settings() {
    let t = TempDir::new().unwrap();
    let p = write(t.path(), "A0", "1_kick.wav", 4410, 44100, 1000);
    write(t.path(), "A0", "2_snare.wav", 4410, 44100, 1200);
    let original = fs::read(&p).unwrap();
    let c = Scanner::default().scan(t.path(), &HashSet::new(), false);
    let selected = vec![
        Some(c.kits[0].samples[0].clone()),
        Some(c.kits[0].samples[1].clone()),
        None,
        None,
    ];
    let target = export(&selected, t.path(), "B12").unwrap();
    assert_eq!(fs::read(target.join("1_kick.wav")).unwrap(), original);
    assert!(export(&selected, t.path(), "B12").is_err());
    assert!(export(&selected, t.path(), "../oops").is_err());
    assert_eq!(fs::read(p).unwrap(), original);
    assert!(!target.join("_save").exists());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 2);
}
#[test]
fn swift_session_migration_and_atomic_roundtrip() {
    let t = TempDir::new().unwrap();
    let old=br#"{"rootPath":"/tmp/library","selectedKit":"A0","bpm":137,"voices":[{"kitID":"A0","samplePath":"/tmp/library/A0/1.wav","steps":[true,false,true],"volume":0.6,"muted":true,"solo":false}],"kitConfigurations":{"libraries":{}}}"#;
    fs::write(t.path().join("session.json"), old).unwrap();
    let mut s = session::load(t.path()).unwrap();
    assert_eq!(s.bpm, 137.);
    assert_eq!(s.voices.len(), 4);
    assert_eq!(s.voices[0].steps.len(), 16);
    assert!(s.voices[0].steps[2]);
    assert!(s.voices[0].muted);
    assert_eq!(s.voices[0].layer_mode, LayerMode::Single);
    assert_eq!(
        fs::read(t.path().join("session-swift-1.1.0.backup.json")).unwrap(),
        old
    );
    s.voices[0].layer_mode = LayerMode::Reverse;
    session::save(t.path(), &s).unwrap();
    let restored = session::load(t.path()).unwrap();
    assert_eq!(restored.voices[0].layer_mode, LayerMode::Reverse);
    assert_eq!(
        restored
            .kit_configurations
            .get("/tmp/library", "A0", 0)
            .unwrap()
            .layer_mode,
        LayerMode::Reverse
    );
    assert_eq!(
        fs::read(t.path().join("session-swift-1.1.0.backup.json")).unwrap(),
        old
    );
}
#[test]
fn malformed_session_is_preserved() {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("session.json"), b"corrupt").unwrap();
    assert!(session::load(t.path()).is_err());
    assert_eq!(fs::read(t.path().join("session.json")).unwrap(), b"corrupt");
}
#[test]
fn layer_modes_wrap_and_default_to_single() {
    let paths = vec!["1_A".into(), "1_B".into(), "1_C".into()];
    let mut s = LayerSelector::default();
    assert_eq!(Voice::default().layer_mode, LayerMode::Single);
    for _ in 0..10 {
        assert_eq!(s.choose(&paths, "1_B", LayerMode::Single, 100), Some(1))
    }
    s.reset();
    let seq: Vec<_> = (0..5)
        .map(|_| s.choose(&paths, "1_B", LayerMode::Sequential, 100))
        .collect();
    assert_eq!(seq, vec![Some(1), Some(2), Some(0), Some(1), Some(2)]);
    s.reset();
    let rev: Vec<_> = (0..4)
        .map(|_| s.choose(&paths, "1_B", LayerMode::Reverse, 100))
        .collect();
    assert_eq!(rev, vec![Some(1), Some(0), Some(2), Some(1)]);
    assert_eq!(s.choose(&paths, "1_B", LayerMode::Velocity, 1), Some(0));
    assert_eq!(s.choose(&paths, "1_B", LayerMode::Velocity, 64), Some(1));
    assert_eq!(s.choose(&paths, "1_B", LayerMode::Velocity, 127), Some(2));
    let indexes: HashSet<_> = (0..100)
        .map(|_| s.choose(&paths, "1_B", LayerMode::Random, 100).unwrap())
        .collect();
    assert_eq!(indexes.len(), 3);
    assert!(s.choose(&paths, "", LayerMode::Random, 100).is_none());
}
fn prepared(root: &Path, name: &str, value: i16, frames: usize) -> Arc<PreparedSample> {
    let p = write(root, "A0", name, frames, 44100, value);
    PreparedSample::load(&p, "test").unwrap()
}
#[test]
fn renderer_is_sample_accurate_and_monophonic() {
    let t = TempDir::new().unwrap();
    let sample = prepared(t.path(), "1.wav", 8192, 44100);
    let meters = Meters::new();
    let (mut renderer, control) = Renderer::new(44100, meters.clone());
    let mut voices = vec![Voice::default(); 4];
    voices[0].sample_path = sample.path.clone();
    voices[0].steps[0] = true;
    voices[0].steps[4] = true;
    control.send(Command::Settings(voices, 120.)).unwrap();
    control.send(Command::Bank(0, vec![sample])).unwrap();
    control.send(Command::Play).unwrap();
    let mut data = vec![0.; 44100 * 2];
    renderer.render(&mut data, 2);
    assert!(data.iter().all(|v| (*v - 0.08).abs() < 0.00001));
    assert_eq!(meters.snapshot().step, 7);
    assert_eq!(meters.snapshot().underruns, 0);
    control.send(Command::Stop).unwrap();
    renderer.render(&mut data, 2);
    assert!(data.iter().all(|v| *v == 0.));
}
#[test]
fn rendered_layers_follow_velocity_and_cycles() {
    let t = TempDir::new().unwrap();
    let bank = vec![
        prepared(t.path(), "1_A.wav", 4096, 4410),
        prepared(t.path(), "1_B.wav", 8192, 4410),
        prepared(t.path(), "1_C.wav", 12288, 4410),
    ];
    for (mode, expected) in [
        (LayerMode::Single, vec![1, 1, 1]),
        (LayerMode::Sequential, vec![1, 2, 0]),
        (LayerMode::Reverse, vec![1, 0, 2]),
        (LayerMode::Velocity, vec![0, 1, 2]),
    ] {
        let meters = Meters::new();
        let (mut renderer, control) = Renderer::new(44100, meters.clone());
        let mut voices = vec![Voice::default(); 4];
        voices[0].sample_path = bank[1].path.clone();
        voices[0].layer_mode = mode;
        voices[0].steps = vec![true; 16];
        voices[0].velocities[..3].copy_from_slice(&[1, 64, 127]);
        control.send(Command::Settings(voices, 120.)).unwrap();
        control.send(Command::Bank(0, bank.clone())).unwrap();
        control.send(Command::Play).unwrap();
        let mut out = vec![0.; 44100];
        renderer.render(&mut out, 2);
        for (i, index) in expected.iter().enumerate() {
            let frame = (i as f64 * 5512.5).ceil() as usize;
            assert!(
                (out[frame * 2] - 0.04 * (*index + 1) as f32).abs() < 1e-5,
                "{mode:?} at {frame}"
            );
        }
    }
}
#[test]
fn original_audition_ignores_voice_mixer_and_editor_and_streams_long_audio() {
    let temp = TempDir::new().unwrap();
    let sample = prepared(temp.path(), "Original.wav", 8192, 44100 * 4);
    assert_eq!(sample.head.len(), 88200);
    let (mut renderer, control) = Renderer::new(44100, Meters::new());
    let mut voices = vec![Voice::default(); 4];
    voices[0].muted = true;
    voices[0].volume = 0.;
    voices[1].solo = true;
    control.send(Command::Settings(voices, 120.)).unwrap();
    let mut edits = rample_core::editing::Settings::original(4.);
    edits.gain_db = -20.;
    control
        .send(Command::Edit(Some(rample_core::audio::EditPreview {
            channel: 0,
            sample: sample.clone(),
            settings: edits,
        })))
        .unwrap();
    control
        .send(Command::OriginalAudition(Some(sample)))
        .unwrap();
    let mut out = vec![0.; 882];
    renderer.render(&mut out, 2);
    assert!(out.iter().all(|v| (*v - 0.1).abs() < 1e-5));
    for _ in 0..310 {
        renderer.render(&mut out, 2);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(out.iter().all(|v| (*v - 0.1).abs() < 1e-5));
    control.send(Command::OriginalAudition(None)).unwrap();
    renderer.render(&mut out, 2);
    assert!(out.iter().all(|v| *v == 0.));
}
#[test]
fn mute_solo_volume_and_tempo_changes_reach_pcm() {
    let t = TempDir::new().unwrap();
    let sample = prepared(t.path(), "1.wav", 8192, 44100);
    let meters = Meters::new();
    let (mut r, c) = Renderer::new(44100, meters.clone());
    let mut v = vec![Voice::default(); 4];
    for voice in &mut v {
        voice.sample_path = sample.path.clone();
        voice.steps = vec![true; 16];
    }
    c.send(Command::Settings(v.clone(), 120.)).unwrap();
    for i in 0..4 {
        c.send(Command::Bank(i, vec![sample.clone()])).unwrap();
    }
    c.send(Command::Play).unwrap();
    let mut out = vec![0.; 8820];
    r.render(&mut out, 2);
    assert!((out[0] - 0.32).abs() < 1e-5);
    v[0].solo = true;
    c.send(Command::Settings(v.clone(), 240.)).unwrap();
    r.render(&mut out, 2);
    assert!((out[0] - 0.08).abs() < 1e-5);
    v[0].muted = true;
    c.send(Command::Settings(v, 240.)).unwrap();
    r.render(&mut out, 2);
    assert!(out.iter().all(|v| *v == 0.));
    assert_eq!(meters.snapshot().underruns, 0);
}
#[test]
fn long_wavs_use_bounded_heads_and_stream_the_tail() {
    let t = TempDir::new().unwrap();
    let sample = prepared(t.path(), "1_long.wav", 4096, 44100 * 4);
    assert_eq!(sample.head.len(), 44100 * 2);
    let meters = Meters::new();
    let (mut r, c) = Renderer::new(44100, meters.clone());
    let mut v = vec![Voice::default(); 4];
    v[0].sample_path = sample.path.clone();
    c.send(Command::Settings(v, 120.)).unwrap();
    c.send(Command::Bank(0, vec![sample])).unwrap();
    c.send(Command::Audition(0)).unwrap();
    let mut out = vec![0.; 882];
    for _ in 0..310 {
        r.render(&mut out, 2);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(out.iter().all(|v| (*v - 0.04).abs() < 1e-5));
    assert_eq!(meters.snapshot().underruns, 0);
}
#[test]
fn resampling_preserves_dc_and_stays_finite() {
    let t = TempDir::new().unwrap();
    let sample = prepared(t.path(), "1.wav", 8192, 44100);
    let (mut r, c) = Renderer::new(48000, Meters::new());
    let mut v = vec![Voice::default(); 4];
    v[0].sample_path = sample.path.clone();
    c.send(Command::Settings(v, 120.)).unwrap();
    c.send(Command::Bank(0, vec![sample])).unwrap();
    c.send(Command::Audition(0)).unwrap();
    let mut out = vec![0.; 9600];
    r.render(&mut out, 2);
    assert!(out.iter().all(|v| v.is_finite()));
    assert!(out[128..].iter().all(|v| (*v - 0.08).abs() < 0.001));
}

#[test]
fn repeated_layer_occurrences_keep_selection_when_reordered() {
    use rample_core::audio::LayerSample;
    let temp = TempDir::new().unwrap();
    let path = write(temp.path(), "A0", "1.wav", 4410, 44100, 8192);
    let sample = PreparedSample::load(&path, "original").unwrap();
    let meters = Meters::new();
    let (mut renderer, control) = Renderer::new(44100, meters.clone());
    let mut voice = Voice {
        sample_path: "second".into(),
        volume: 1.,
        ..Voice::default()
    };
    let entry = |id: &str| LayerSample {
        id: id.into(),
        sample: sample.clone(),
    };
    control
        .send(Command::Settings(vec![voice.clone()], 120.))
        .unwrap();
    control
        .send(Command::LayerBank(0, vec![entry("first"), entry("second")]))
        .unwrap();
    control.send(Command::Audition(0)).unwrap();
    let mut pcm = [0.; 2];
    renderer.render(&mut pcm, 2);
    assert_eq!(meters.snapshot().active_layers[0], 1);
    assert!((pcm[0] - 0.1).abs() < 0.00001);
    control
        .send(Command::LayerBank(0, vec![entry("second"), entry("first")]))
        .unwrap();
    control.send(Command::Audition(0)).unwrap();
    renderer.render(&mut pcm, 2);
    assert_eq!(meters.snapshot().active_layers[0], 0);
    for (mode, expected) in [
        (LayerMode::Sequential, vec![0, 1, 0]),
        (LayerMode::Reverse, vec![0, 1, 0]),
        (LayerMode::Velocity, vec![1, 1, 1]),
    ] {
        voice.layer_mode = mode;
        control
            .send(Command::Settings(vec![voice.clone()], 120.))
            .unwrap();
        let actual: Vec<_> = (0..3)
            .map(|_| {
                control.send(Command::Audition(0)).unwrap();
                renderer.render(&mut pcm, 2);
                meters.snapshot().active_layers[0]
            })
            .collect();
        assert_eq!(actual, expected);
    }
    voice.layer_mode = LayerMode::Random;
    control.send(Command::Settings(vec![voice], 120.)).unwrap();
    for _ in 0..20 {
        control.send(Command::Audition(0)).unwrap();
        renderer.render(&mut pcm, 2);
        assert!((0..2).contains(&meters.snapshot().active_layers[0]));
    }
}

#[test]
fn decoded_original_audition_keeps_stereo_channels_and_source_rate() {
    let temp = TempDir::new().unwrap();
    let mut bytes = b"FORM".to_vec();
    bytes.extend((46 + 192000u32 * 4).to_be_bytes());
    bytes.extend(b"AIFFCOMM");
    bytes.extend(18u32.to_be_bytes());
    bytes.extend(2u16.to_be_bytes());
    bytes.extend(192000u32.to_be_bytes());
    bytes.extend(16u16.to_be_bytes());
    bytes.extend([0x40, 0x0e, 0xbb, 0x80, 0, 0, 0, 0, 0, 0]);
    bytes.extend(b"SSND");
    bytes.extend((192000u32 * 4 + 8).to_be_bytes());
    bytes.extend([0; 8]);
    for _ in 0..192000 {
        bytes.extend(16384i16.to_be_bytes());
        bytes.extend((-8192i16).to_be_bytes());
    }
    let source = temp.path().join("Original.aiff");
    fs::write(&source, &bytes).unwrap();
    let playback = temp.path().join("Playback.wav");
    let info = rample_core::conversion::decode_original(
        &source,
        &mut fs::File::create(&playback).unwrap(),
    )
    .unwrap();
    assert_eq!((info.rate, info.channels, info.bits), (48000, 2, 16));
    let sample = PreparedSample::load(&playback, "original").unwrap();
    assert_eq!(
        (
            sample.spec.rate,
            sample.spec.channels,
            sample.spec.bits,
            sample.spec.frames
        ),
        (48000, 2, 64, 192000)
    );
    assert_eq!(sample.head.len(), 96000);
    let (mut renderer, control) = Renderer::new(48000, Meters::new());
    control
        .send(Command::OriginalAudition(Some(sample)))
        .unwrap();
    let mut out = vec![0.; 1000];
    renderer.render(&mut out, 2);
    assert!(out
        .as_chunks::<2>()
        .0
        .iter()
        .all(|f| (f[0] - 0.2).abs() < 1e-6 && (f[1] + 0.1).abs() < 1e-6));
    for _ in 0..240 {
        renderer.render(&mut out, 2);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(out
        .as_chunks::<2>()
        .0
        .iter()
        .all(|f| (f[0] - 0.2).abs() < 1e-6 && (f[1] + 0.1).abs() < 1e-6));
    assert_eq!(fs::read(source).unwrap(), bytes);
}
