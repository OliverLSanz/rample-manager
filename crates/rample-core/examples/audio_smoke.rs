#[cfg(feature = "native-audio")]
fn main() {
    use rample_core::{
        audio::{native::AudioHost, Command},
        model::Voice,
        wav::{create_demo, PreparedSample},
    };
    let temp = tempfile::tempdir().unwrap();
    let root = create_demo(temp.path()).unwrap();
    let audio = AudioHost::new();
    let mut voices = vec![Voice::default(); 4];
    for (i, name) in [
        "1_Sub kick.wav",
        "2_Dust snare.wav",
        "3_Closed hat.wav",
        "4_Wood click.wav",
    ]
    .iter()
    .enumerate()
    {
        let sample = PreparedSample::load(&root.join("A0").join(name), "smoke").unwrap();
        voices[i].sample_path = sample.path.clone();
        voices[i].steps = (0..16).map(|step| step % 4 == i).collect();
        audio.send(Command::Bank(i, vec![sample])).unwrap();
    }
    audio.send(Command::Settings(voices, 120.)).unwrap();
    audio.send(Command::Play).unwrap();
    let mut peak = 0f32;
    for _ in 0..160 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        peak = peak.max(audio.meters.snapshot().peak);
        if audio.meters.snapshot().rendered_frames > 88200 && peak > 0.01 {
            break;
        }
    }
    audio.send(Command::Stop).unwrap();
    let status = audio.meters.snapshot();
    println!(
        "{}",
        serde_json::json!({"sampleRate":status.sample_rate,"renderedFrames":status.rendered_frames,"peak":peak,"underruns":status.underruns,"error":*audio.error.lock().unwrap()})
    );
    assert!(status.rendered_frames > 44100, "No native output callbacks");
    assert!(peak > 0.01, "Audio callback produced no audible PCM");
    assert_eq!(status.underruns, 0);
}
#[cfg(not(feature = "native-audio"))]
fn main() {
    panic!("Run with --features native-audio")
}
