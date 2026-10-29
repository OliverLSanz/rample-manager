use rample_core::{
    audio::{Command, EditPreview, Meters, Renderer},
    catalog,
    editing::{self, Engine, Settings},
    model::Voice,
    wav::{self, PreparedSample},
};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
use tempfile::TempDir;
fn sample(root: &Path, values: &[f32]) -> PathBuf {
    let path = root.join("1_Edit.wav");
    wav::write_pcm16(
        &path,
        &values
            .iter()
            .map(|v| (v * 32767.) as i16)
            .collect::<Vec<_>>(),
        44100,
    )
    .unwrap();
    path
}
fn read(path: &Path) -> Vec<[f32; 2]> {
    let spec = wav::inspect(path).unwrap();
    wav::read_frames(
        &mut File::open(path).unwrap(),
        &spec,
        0,
        spec.frames as usize,
    )
    .unwrap()
}
fn rendered(root: &Path, path: &Path, settings: &Settings) -> (editing::Rendered, Vec<[f32; 2]>) {
    let output = root.join("render.wav");
    let stats = editing::render(path, settings, &mut File::create(&output).unwrap()).unwrap();
    (stats, read(&output))
}
#[test]
fn trim_gain_and_pitch_are_baked_with_backups_and_compatible_mono_output() {
    let temp = TempDir::new().unwrap();
    let input: Vec<_> = (0..44100)
        .map(|i| (std::f32::consts::TAU * 440. * i as f32 / 44100.).sin() * 0.25)
        .collect();
    let path = sample(temp.path(), &input);
    let original = fs::read(&path).unwrap();
    let mut settings = Settings::original(1.);
    settings.start = 0.2;
    settings.end = 0.6;
    settings.pitch = 12.;
    settings.gain_db = -6.;
    let (saved, stats) = editing::save(
        &path,
        &catalog::fingerprint(&path),
        &settings,
        &temp.path().join("backups"),
    )
    .unwrap();
    assert_eq!(fs::read(saved.backup).unwrap(), original);
    let spec = wav::inspect(&path).unwrap();
    assert_eq!(
        (spec.frames, spec.channels, spec.bits, spec.rate),
        (8820, 1, 16, 44100)
    );
    assert!((stats.peak - 0.1253).abs() < 0.002);
    let audio = read(&path);
    let crosses = audio
        .windows(2)
        .filter(|v| v[0][0] <= 0. && v[1][0] > 0.)
        .count();
    assert!((174..=178).contains(&crosses), "{crosses}");
}
#[test]
fn equalizer_boosts_the_selected_band_and_stays_finite_at_extremes() {
    let temp = TempDir::new().unwrap();
    let input: Vec<_> = (0..44100)
        .map(|i| (std::f32::consts::TAU * 120. * i as f32 / 44100.).sin() * 0.1)
        .collect();
    let path = sample(temp.path(), &input);
    let mut settings = Settings::original(1.);
    settings.low_db = 6.;
    let (_, audio) = rendered(temp.path(), &path, &settings);
    let rms = (audio[10000..40000].iter().map(|f| f[0] * f[0]).sum::<f32>() / 30000.).sqrt();
    assert!((0.139..0.144).contains(&rms), "{rms}");
    settings.low_db = 18.;
    settings.mid_db = -18.;
    settings.high_db = 18.;
    settings.echo_mix = 1.;
    settings.echo_feedback = 0.7;
    settings.reverb_mix = 1.;
    settings.reverb_seconds = 0.2;
    let (_, audio) = rendered(temp.path(), &path, &settings);
    assert!(audio.iter().flatten().all(|v| v.is_finite()));
}
#[test]
fn echo_and_reverb_tails_are_written_beyond_the_trimmed_endpoint() {
    let temp = TempDir::new().unwrap();
    let mut input = vec![0.; 4410];
    input[100] = 0.5;
    let path = sample(temp.path(), &input);
    let mut settings = Settings::original(0.1);
    settings.echo_mix = 0.5;
    settings.echo_ms = 50.;
    settings.echo_feedback = 0.5;
    let (_, echo) = rendered(temp.path(), &path, &settings);
    assert!((echo[2305][0] - 0.25).abs() < 0.0001);
    assert!((echo[4510][0] - 0.125).abs() < 0.0001);
    settings.echo_mix = 0.;
    settings.reverb_mix = 0.5;
    settings.reverb_seconds = 0.5;
    let (stats, reverb) = rendered(temp.path(), &path, &settings);
    assert!(stats.frames > 4410);
    assert!(reverb[4410..].iter().any(|f| f[0].abs() > 0.001));
    assert!(reverb[reverb.len() - 4410..]
        .iter()
        .all(|f| f[0].abs() < 0.0002));
}
#[test]
fn live_sequence_matches_the_baked_sample_including_combined_effects() {
    let temp = TempDir::new().unwrap();
    let input: Vec<_> = (0..8820)
        .map(|i| (std::f32::consts::TAU * 350. * i as f32 / 44100.).sin() * 0.15)
        .collect();
    let path = sample(temp.path(), &input);
    let base = PreparedSample::load(&path, "base").unwrap();
    let mut settings = Settings::original(0.2);
    settings.start = 0.03;
    settings.end = 0.17;
    settings.pitch = 3.5;
    settings.gain_db = -2.;
    settings.mid_db = 4.;
    settings.echo_mix = 0.25;
    settings.echo_ms = 40.;
    settings.echo_feedback = 0.1;
    settings.reverb_mix = 0.2;
    settings.reverb_seconds = 0.2;
    let cropped =
        PreparedSample::load_at(&path, "base", (settings.start * 44100.).round() as u64).unwrap();
    let (stats, baked) = rendered(temp.path(), &path, &settings);
    let meters = Meters::new();
    let (mut renderer, control) = Renderer::new(44100, meters.clone());
    let mut voice = Voice {
        sample_path: path.to_string_lossy().into(),
        volume: 1.,
        steps: vec![false; 16],
        ..Voice::default()
    };
    voice.steps[0] = true;
    control
        .send(Command::Settings(
            vec![voice, Voice::default(), Voice::default(), Voice::default()],
            120.,
        ))
        .unwrap();
    control.send(Command::Bank(0, vec![base])).unwrap();
    control
        .send(Command::Edit(Some(EditPreview {
            channel: 0,
            sample: cropped,
            settings,
        })))
        .unwrap();
    control.send(Command::Play).unwrap();
    let mut live = vec![0.; stats.frames as usize];
    renderer.render(&mut live, 1);
    let max_error = live
        .iter()
        .zip(&baked)
        .map(|(a, b)| (a - b[0] * 0.4).abs())
        .fold(0f32, f32::max);
    assert!(max_error < 0.00003, "{max_error}");
    assert_eq!(meters.snapshot().underruns, 0);
}
#[test]
fn late_trim_starts_from_preloaded_audio_without_callback_disk_reads() {
    let temp = TempDir::new().unwrap();
    let path = sample(temp.path(), &vec![0.25; 44100 * 5]);
    let base = PreparedSample::load(&path, "base").unwrap();
    let cropped = PreparedSample::load_at(&path, "base", 44100 * 4).unwrap();
    assert_eq!(cropped.head_start, 44100 * 4);
    assert!(cropped.head.len() <= 44100 * 2);
    let mut settings = Settings::original(5.);
    settings.start = 4.;
    settings.end = 4.1;
    let meters = Meters::new();
    let (mut renderer, control) = Renderer::new(44100, meters.clone());
    control.send(Command::Bank(0, vec![base.clone()])).unwrap();
    control
        .send(Command::Edit(Some(EditPreview {
            channel: 0,
            sample: cropped,
            settings,
        })))
        .unwrap();
    control.send(Command::AuditionSample(0, base)).unwrap();
    let mut live = vec![0.; 1000];
    renderer.render(&mut live, 1);
    assert!(live.iter().all(|v| (*v - 0.25 * 0.8 * 0.4).abs() < 0.0001));
    assert_eq!(meters.snapshot().underruns, 0);
}
#[test]
fn live_parameter_changes_do_not_restart_sequence_timing() {
    let temp = TempDir::new().unwrap();
    let path = sample(temp.path(), &vec![0.25; 44100]);
    let source = PreparedSample::load(&path, "base").unwrap();
    let mut settings = Settings::original(1.);
    let meters = Meters::new();
    let (mut renderer, control) = Renderer::new(44100, meters.clone());
    let mut voice = Voice {
        sample_path: source.path.clone(),
        volume: 1.,
        ..Voice::default()
    };
    voice.steps[0] = true;
    control.send(Command::Settings(vec![voice], 120.)).unwrap();
    control
        .send(Command::Bank(0, vec![source.clone()]))
        .unwrap();
    control
        .send(Command::Edit(Some(EditPreview {
            channel: 0,
            sample: source.clone(),
            settings: settings.clone(),
        })))
        .unwrap();
    control.send(Command::Play).unwrap();
    renderer.render(&mut vec![0.; 1000], 1);
    settings.gain_db = -12.;
    control
        .send(Command::Edit(Some(EditPreview {
            channel: 0,
            sample: source,
            settings,
        })))
        .unwrap();
    let mut output = vec![0.; 4000];
    renderer.render(&mut output, 1);
    assert!((output[3999] - 0.25 * 0.4 * 10f32.powf(-12. / 20.)).abs() < 0.0001);
    assert_eq!(meters.snapshot().rendered_frames, 5000);
    assert_eq!(meters.snapshot().step, 0);
}
#[test]
fn invalid_and_stale_edits_never_replace_the_source() {
    let temp = TempDir::new().unwrap();
    let path = sample(temp.path(), &vec![0.25; 4410]);
    let original = fs::read(&path).unwrap();
    let stamp = catalog::fingerprint(&path);
    let mut settings = Settings::original(0.1);
    settings.end = 0.;
    assert!(editing::save(&path, &stamp, &settings, &temp.path().join("backups")).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    settings.end = 0.1;
    settings.pitch = f64::NAN;
    assert!(Engine::new(&wav::inspect(&path).unwrap(), 44100, &settings).is_err());
    fs::write(&path, b"a later user edit").unwrap();
    settings.pitch = 0.;
    assert!(editing::save(&path, &stamp, &settings, &temp.path().join("backups")).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"a later user edit");
}
#[test]
fn opening_and_bypassing_an_editor_keep_the_current_source_position() {
    let temp = TempDir::new().unwrap();
    let mut values = vec![0.1; 1000];
    values.extend(vec![0.25; 44000]);
    let path = sample(temp.path(), &values);
    let source = PreparedSample::load(&path, "base").unwrap();
    let meters = Meters::new();
    let (mut renderer, control) = Renderer::new(44100, meters);
    control
        .send(Command::AuditionSample(0, source.clone()))
        .unwrap();
    renderer.render(&mut vec![0.; 2000], 1);
    let settings = Settings::original(45000. / 44100.);
    control
        .send(Command::Edit(Some(EditPreview {
            channel: 0,
            sample: source,
            settings,
        })))
        .unwrap();
    let mut output = vec![0.; 1000];
    renderer.render(&mut output, 1);
    assert!(output
        .iter()
        .all(|v| (*v - 0.25 * 0.8 * 0.4).abs() < 0.0001));
    control.send(Command::Edit(None)).unwrap();
    renderer.render(&mut output, 1);
    assert!(output
        .iter()
        .all(|v| (*v - 0.25 * 0.8 * 0.4).abs() < 0.0001));
}
