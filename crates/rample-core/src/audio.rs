use crate::{
    editing::{Engine as EditEngine, Kernel},
    model::{LayerMode, Voice},
    wav::{self, PreparedSample},
};
use crossbeam_channel::{bounded, Receiver, SendTimeoutError, Sender};
use serde::Serialize;
use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

const CHUNK: usize = 2048;
#[derive(Clone)]
pub enum Command {
    Settings(Vec<Voice>, f64),
    Bank(usize, Vec<Arc<PreparedSample>>),
    Play,
    Stop,
    Audition(usize),
    Edit(Option<EditPreview>),
    AuditionSample(usize, Arc<PreparedSample>),
}
#[derive(Clone)]
pub struct EditPreview {
    pub channel: usize,
    pub sample: Arc<PreparedSample>,
    pub settings: crate::editing::Settings,
}
#[derive(Default)]
pub struct Meters {
    pub running: AtomicBool,
    pub step: AtomicI32,
    pub peak: AtomicU32,
    pub editor_peak: AtomicU32,
    pub active: [AtomicI32; 4],
    pub underruns: AtomicU64,
    pub frames: AtomicU64,
    pub rate: AtomicU32,
    pub failed: AtomicBool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub step: i32,
    pub peak: f32,
    pub editor_peak: f32,
    pub active_layers: [i32; 4],
    pub underruns: u64,
    pub rendered_frames: u64,
    pub sample_rate: u32,
}
impl Meters {
    pub fn new() -> Arc<Self> {
        let value = Arc::new(Self::default());
        value.step.store(-1, Ordering::Relaxed);
        for a in &value.active {
            a.store(-1, Ordering::Relaxed);
        }
        value
    }
    pub fn snapshot(&self) -> Status {
        Status {
            running: self.running.load(Ordering::Relaxed),
            step: self.step.load(Ordering::Relaxed),
            peak: f32::from_bits(self.peak.load(Ordering::Relaxed)),
            editor_peak: f32::from_bits(self.editor_peak.swap(0, Ordering::Relaxed)),
            active_layers: std::array::from_fn(|i| self.active[i].load(Ordering::Relaxed)),
            underruns: self.underruns.load(Ordering::Relaxed),
            rendered_frames: self.frames.load(Ordering::Relaxed),
            sample_rate: self.rate.load(Ordering::Relaxed),
        }
    }
}

struct Request {
    sample: Option<Arc<PreparedSample>>,
    token: u64,
}
struct Chunk {
    token: u64,
    start: u64,
    frames: Vec<[f32; 2]>,
}
struct DiskReader {
    requests: Sender<Request>,
    chunks: Receiver<Chunk>,
    token: u64,
    previous: Option<Chunk>,
    current: Option<Chunk>,
}
impl DiskReader {
    fn new() -> Self {
        let (tx, rx) = bounded::<Request>(8);
        let (chunks_tx, chunks_rx) = bounded::<Chunk>(8);
        thread::Builder::new()
            .name("sample-reader".into())
            .spawn(move || {
                let mut request: Option<Request> = None;
                'worker: loop {
                    let r = match request.take() {
                        Some(r) => r,
                        None => match rx.recv() {
                            Ok(r) => r,
                            Err(_) => break,
                        },
                    };
                    let Some(sample) = r.sample else { continue };
                    let Ok(mut file) = File::open(&sample.path) else {
                        continue;
                    };
                    let mut frame = sample.head_start + sample.head.len() as u64;
                    while frame < sample.spec.frames {
                        match rx.try_recv() {
                            Ok(next) => {
                                request = Some(next);
                                while let Ok(next) = rx.try_recv() {
                                    request = Some(next);
                                }
                                continue 'worker;
                            }
                            Err(crossbeam_channel::TryRecvError::Disconnected) => break 'worker,
                            Err(_) => {}
                        }
                        let Ok(frames) = wav::read_frames(&mut file, &sample.spec, frame, CHUNK)
                        else {
                            continue 'worker;
                        };
                        if frames.is_empty() {
                            continue 'worker;
                        }
                        let count = frames.len() as u64;
                        let mut chunk = Chunk {
                            token: r.token,
                            start: frame,
                            frames,
                        };
                        loop {
                            match chunks_tx.send_timeout(chunk, Duration::from_millis(5)) {
                                Ok(()) => break,
                                Err(SendTimeoutError::Disconnected(_)) => break 'worker,
                                Err(SendTimeoutError::Timeout(returned)) => {
                                    chunk = returned;
                                    match rx.try_recv() {
                                        Ok(next) => {
                                            request = Some(next);
                                            while let Ok(next) = rx.try_recv() {
                                                request = Some(next);
                                            }
                                            continue 'worker;
                                        }
                                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                                            break 'worker
                                        }
                                        Err(_) => {}
                                    }
                                }
                            }
                        }
                        frame += count;
                    }
                }
            })
            .expect("create disk reader");
        Self {
            requests: tx,
            chunks: chunks_rx,
            token: 0,
            previous: None,
            current: None,
        }
    }
    fn reset(&mut self, sample: Option<Arc<PreparedSample>>) {
        self.token = self.token.wrapping_add(1);
        self.previous = None;
        self.current = None;
        while self.chunks.try_recv().is_ok() {}
        let _ = self.requests.try_send(Request {
            sample: sample.filter(|s| s.spec.frames > s.head_start + s.head.len() as u64),
            token: self.token,
        });
    }
    fn frame(&mut self, sample: &PreparedSample, index: i64) -> Option<[f32; 2]> {
        if index < 0 || index as u64 >= sample.spec.frames {
            return Some([0.; 2]);
        }
        if index as u64 >= sample.head_start
            && (index as u64 - sample.head_start) < sample.head.len() as u64
        {
            return Some(sample.head[(index as u64 - sample.head_start) as usize]);
        }
        let index = index as u64;
        for _ in 0..10 {
            for c in [&self.current, &self.previous].into_iter().flatten() {
                if index >= c.start && index < c.start + c.frames.len() as u64 {
                    return Some(c.frames[(index - c.start) as usize]);
                }
            }
            match self.chunks.try_recv() {
                Ok(c) if c.token == self.token => {
                    self.previous = self.current.take();
                    self.current = Some(c);
                }
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
        None
    }
}

#[derive(Clone)]
struct Sound {
    sample: Arc<PreparedSample>,
    kernel: Arc<Kernel>,
}
struct PlaybackEdit {
    sample: Arc<PreparedSample>,
    engine: EditEngine,
}
struct Playback {
    sound: Option<Sound>,
    cursor: f64,
    reader: DiskReader,
    editor: Option<Box<PlaybackEdit>>,
}
impl Playback {
    fn new() -> Self {
        Self {
            sound: None,
            cursor: 0.,
            reader: DiskReader::new(),
            editor: None,
        }
    }
    fn set_editor(&mut self, value: Option<Box<PlaybackEdit>>) {
        let active = self.sound.as_ref().is_some_and(|s| {
            self.editor
                .as_ref()
                .is_some_and(|e| e.sample.path == s.sample.path)
        });
        match (self.editor.as_mut(), value) {
            (Some(old), Some(new)) if old.sample.path == new.sample.path => {
                let moved = old.sample.head_start != new.sample.head_start;
                old.engine.update(new.engine);
                old.sample = new.sample;
                if moved && active {
                    old.engine.reset();
                    self.reader.reset(Some(old.sample.clone()));
                }
            }
            (_, mut value) => {
                if active {
                    if let Some(old) = &self.editor {
                        self.cursor = old.engine.source_position();
                    }
                }
                if let Some(new) = &mut value {
                    if self
                        .sound
                        .as_ref()
                        .is_some_and(|s| s.sample.path == new.sample.path)
                    {
                        new.engine.follow_source(self.cursor);
                    }
                }
                self.editor = value;
            }
        }
    }
    fn start(&mut self, mut sound: Option<Sound>) {
        if let (Some(sound), Some(editor)) = (&mut sound, &mut self.editor) {
            if sound.sample.path == editor.sample.path {
                sound.sample = editor.sample.clone();
                editor.engine.reset();
            }
        }
        self.reader.reset(sound.as_ref().map(|s| s.sample.clone()));
        self.sound = sound;
        self.cursor = 0.;
    }
    fn next(&mut self, rate: u32, misses: &mut u64, editor_peak: &mut f32) -> [f32; 2] {
        let Some(sound) = &self.sound else {
            return [0.; 2];
        };
        if let Some(editor) = &mut self.editor {
            if editor.sample.path == sound.sample.path {
                return match editor
                    .engine
                    .next(|index| self.reader.frame(&editor.sample, index))
                {
                    Some((value, complete)) => {
                        if !complete {
                            *misses += 1;
                        }
                        *editor_peak = editor_peak.max(value.abs());
                        [value.clamp(-1., 1.); 2]
                    }
                    None => {
                        self.sound = None;
                        [0.; 2]
                    }
                };
            }
        }
        let sample = &sound.sample;
        if self.cursor >= sample.spec.frames as f64 {
            self.sound = None;
            return [0.; 2];
        }
        let base = self.cursor.floor() as i64;
        let value = if sample.spec.rate == rate {
            self.reader.frame(sample, base)
        } else {
            let phase = ((self.cursor - base as f64) * 1024.) as usize;
            let mut result = [0.; 2];
            let mut complete = true;
            for (i, weight) in sound.kernel.phases[phase.min(1023)].iter().enumerate() {
                if let Some(frame) = self.reader.frame(sample, base + i as i64 - 15) {
                    result[0] += frame[0] * weight;
                    result[1] += frame[1] * weight;
                } else {
                    complete = false;
                }
            }
            complete.then_some(result)
        };
        self.cursor += f64::from(sample.spec.rate) / f64::from(rate);
        value.unwrap_or_else(|| {
            *misses += 1;
            [0.; 2]
        })
    }
}
struct Track {
    bank: Vec<Sound>,
    paths: Vec<String>,
    selected: Option<usize>,
    previous: Option<usize>,
    rng: u64,
    sequence: Playback,
    preview: Playback,
}
impl Track {
    fn new() -> Self {
        Self {
            bank: vec![],
            paths: vec![],
            selected: None,
            previous: None,
            rng: rand::random::<u64>().max(1),
            sequence: Playback::new(),
            preview: Playback::new(),
        }
    }
    fn choose(&mut self, voice: &Voice, velocity: u8) -> Option<usize> {
        if voice.sample_path.is_empty() || self.bank.is_empty() {
            return None;
        }
        let anchor = self.selected.unwrap_or(0);
        let n = self.bank.len();
        let index = match voice.layer_mode {
            LayerMode::Single => self.selected?,
            LayerMode::Sequential => self.previous.map(|i| (i + 1) % n).unwrap_or(anchor),
            LayerMode::Reverse => self.previous.map(|i| (i + n - 1) % n).unwrap_or(anchor),
            LayerMode::Velocity => (usize::from(velocity.clamp(1, 127)) * n / 128).min(n - 1),
            LayerMode::Random => {
                self.rng ^= self.rng << 13;
                self.rng ^= self.rng >> 7;
                self.rng ^= self.rng << 17;
                (self.rng as usize) % n
            }
        };
        self.previous = Some(index);
        Some(index)
    }
}

pub struct Renderer {
    rate: u32,
    voices: Vec<Voice>,
    tracks: [Track; 4],
    bpm: f64,
    playing: bool,
    next_step: usize,
    remaining: f64,
    meters: Arc<Meters>,
    commands: Receiver<RenderCommand>,
}
struct RenderBank {
    channel: usize,
    sounds: Vec<Sound>,
}
struct RenderEdit {
    channel: usize,
    sequence: Box<PlaybackEdit>,
    preview: Box<PlaybackEdit>,
}
enum RenderCommand {
    Settings(Vec<Voice>, f64),
    Bank(RenderBank),
    Play,
    Stop,
    Audition(usize),
    Edit(Option<RenderEdit>),
    AuditionSample(usize, Sound),
}
/// Own this on a non-audio control thread. Preparation can read disk and build
/// conversion tables. The renderer consumes only ready-to-play commands.
pub struct Control {
    sender: Sender<RenderCommand>,
    rate: u32,
}
impl Control {
    pub fn send(&self, command: Command) -> Result<(), String> {
        let prepared = match command {
            Command::Bank(channel, samples) => RenderCommand::Bank(RenderBank {
                channel,
                sounds: samples
                    .into_iter()
                    .map(|sample| Sound {
                        kernel: Kernel::new(sample.spec.rate, self.rate),
                        sample,
                    })
                    .collect(),
            }),
            Command::Settings(mut v, bpm) => {
                v.resize(4, Voice::default());
                v.truncate(4);
                v.iter_mut().for_each(Voice::normalize);
                RenderCommand::Settings(v, bpm.clamp(30., 300.))
            }
            Command::Edit(edit) => RenderCommand::Edit(match edit {
                Some(edit) => Some(RenderEdit {
                    channel: edit.channel,
                    sequence: Box::new(PlaybackEdit {
                        engine: EditEngine::new(&edit.sample.spec, self.rate, &edit.settings)?,
                        sample: edit.sample.clone(),
                    }),
                    preview: Box::new(PlaybackEdit {
                        engine: EditEngine::new(&edit.sample.spec, self.rate, &edit.settings)?,
                        sample: edit.sample,
                    }),
                }),
                None => None,
            }),
            Command::AuditionSample(i, sample) => RenderCommand::AuditionSample(
                i,
                Sound {
                    kernel: Kernel::new(sample.spec.rate, self.rate),
                    sample,
                },
            ),
            Command::Play => RenderCommand::Play,
            Command::Stop => RenderCommand::Stop,
            Command::Audition(i) => RenderCommand::Audition(i),
        };
        self.sender
            .try_send(prepared)
            .map_err(|_| "Audio command queue is busy".into())
    }
}
impl Renderer {
    pub fn new(rate: u32, meters: Arc<Meters>) -> (Self, Control) {
        let (tx, rx) = bounded(128);
        meters.rate.store(rate, Ordering::Relaxed);
        (
            Self {
                rate,
                voices: vec![Voice::default(); 4],
                tracks: std::array::from_fn(|_| Track::new()),
                bpm: 120.,
                playing: false,
                next_step: 0,
                remaining: 0.,
                meters,
                commands: rx,
            },
            Control { sender: tx, rate },
        )
    }
    fn apply_commands(&mut self) {
        while let Ok(c) = self.commands.try_recv() {
            match c {
                RenderCommand::Settings(voices, bpm) => {
                    self.remaining *= self.bpm / bpm;
                    self.bpm = bpm;
                    for (i, v) in voices.iter().enumerate() {
                        if self.voices[i].layer_mode != v.layer_mode
                            || self.voices[i].sample_path != v.sample_path
                        {
                            self.tracks[i].previous = None;
                        }
                        self.tracks[i].selected = self.tracks[i]
                            .paths
                            .iter()
                            .position(|p| p == &v.sample_path);
                    }
                    self.voices = voices;
                }
                RenderCommand::Bank(RenderBank { channel, sounds }) => {
                    if channel >= 4 {
                        continue;
                    }
                    let track = &mut self.tracks[channel];
                    let old_path = track.previous.and_then(|i| track.paths.get(i).cloned());
                    let new_paths: Vec<_> = sounds.iter().map(|s| s.sample.path.clone()).collect();
                    // A sounding stream is kept only if its exact revision still exists.
                    for playback in [&mut track.sequence, &mut track.preview] {
                        if playback.sound.as_ref().is_some_and(|old| {
                            !sounds.iter().any(|s| {
                                s.sample.path == old.sample.path
                                    && s.sample.revision == old.sample.revision
                            })
                        }) {
                            playback.start(None);
                        }
                    }
                    track.previous = old_path.and_then(|p| new_paths.iter().position(|n| n == &p));
                    track.paths = new_paths;
                    track.bank = sounds;
                    track.selected = track
                        .paths
                        .iter()
                        .position(|p| p == &self.voices[channel].sample_path);
                    self.meters.active[channel].store(-1, Ordering::Relaxed);
                }
                RenderCommand::Edit(edit) => {
                    for (i, t) in self.tracks.iter_mut().enumerate() {
                        if edit.as_ref().is_none_or(|e| e.channel != i) {
                            t.sequence.set_editor(None);
                            t.preview.set_editor(None);
                        }
                    }
                    if let Some(edit) = edit {
                        if let Some(track) = self.tracks.get_mut(edit.channel) {
                            track.sequence.set_editor(Some(edit.sequence));
                            track.preview.set_editor(Some(edit.preview));
                        }
                    }
                }
                RenderCommand::AuditionSample(i, sound) => {
                    if let Some(t) = self.tracks.get_mut(i) {
                        t.preview.start(Some(sound));
                    }
                }
                RenderCommand::Play => {
                    self.playing = true;
                    self.next_step = 0;
                    self.remaining = 0.;
                    for t in &mut self.tracks {
                        t.previous = None;
                        t.sequence.start(None);
                    }
                    self.meters.running.store(true, Ordering::Relaxed);
                }
                RenderCommand::Stop => {
                    self.playing = false;
                    for t in &mut self.tracks {
                        t.sequence.start(None);
                        t.preview.start(None);
                    }
                    self.meters.running.store(false, Ordering::Relaxed);
                    self.meters.step.store(-1, Ordering::Relaxed);
                }
                RenderCommand::Audition(i) => {
                    if i < 4 {
                        self.trigger(i, self.voices[i].velocities[0], true);
                    }
                }
            }
        }
    }
    fn trigger(&mut self, i: usize, velocity: u8, preview: bool) {
        let t = &mut self.tracks[i];
        if let Some(index) = t.choose(&self.voices[i], velocity) {
            let sound = Some(t.bank[index].clone());
            if preview {
                t.preview.start(sound)
            } else {
                t.sequence.start(sound)
            }
            self.meters.active[i].store(index as i32, Ordering::Relaxed);
        }
    }
    pub fn render(&mut self, output: &mut [f32], channels: usize) {
        self.apply_commands();
        let mut peak: f32 = 0.;
        let mut misses = 0;
        let mut editor_peak = 0f32;
        let any_solo = self.voices.iter().any(|v| v.solo);
        let gains: [f32; 4] = std::array::from_fn(|i| {
            if self.voices[i].muted || (any_solo && !self.voices[i].solo) {
                0.
            } else {
                self.voices[i].volume * 0.4
            }
        });
        for frame in output.chunks_exact_mut(channels) {
            if self.playing {
                if self.remaining <= 0. {
                    let step = self.next_step;
                    for i in 0..4 {
                        if self.voices[i].steps[step] {
                            self.trigger(i, self.voices[i].velocities[step], false);
                        }
                    }
                    self.meters.step.store(step as i32, Ordering::Relaxed);
                    self.next_step = (step + 1) % 16;
                    self.remaining += f64::from(self.rate) * 60. / self.bpm / 4.;
                }
                self.remaining -= 1.;
            }
            let mut mixed = [0.; 2];
            for (i, t) in self.tracks.iter_mut().enumerate() {
                let a = t.sequence.next(self.rate, &mut misses, &mut editor_peak);
                let b = t.preview.next(self.rate, &mut misses, &mut editor_peak);
                mixed[0] += (a[0] + b[0]) * gains[i];
                mixed[1] += (a[1] + b[1]) * gains[i];
            }
            for (i, sample) in frame.iter_mut().enumerate() {
                let value = match channels {
                    1 => (mixed[0] + mixed[1]) * 0.5,
                    _ => {
                        if i < 2 {
                            mixed[i]
                        } else {
                            0.
                        }
                    }
                };
                peak = peak.max(value.abs());
                *sample = value.clamp(-1., 1.);
            }
        }
        self.meters.peak.store(peak.to_bits(), Ordering::Relaxed);
        self.meters
            .editor_peak
            .fetch_max(editor_peak.to_bits(), Ordering::Relaxed);
        self.meters.underruns.fetch_add(misses, Ordering::Relaxed);
        self.meters
            .frames
            .fetch_add((output.len() / channels) as u64, Ordering::Relaxed);
    }
}

#[cfg(feature = "native-audio")]
pub mod native {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::time::Instant;

    pub struct AudioHost {
        sender: Sender<Command>,
        pub meters: Arc<Meters>,
        pub error: Arc<std::sync::Mutex<Option<String>>>,
    }
    impl Default for AudioHost {
        fn default() -> Self {
            Self::new()
        }
    }
    impl AudioHost {
        pub fn new() -> Self {
            Self::spawn(true)
        }
        /// Headless command integration tests must not enumerate or open OS
        /// audio devices. Actual PCM and devices have separate test coverage.
        #[cfg(feature = "test-support")]
        pub fn headless() -> Self {
            Self::spawn(false)
        }
        fn spawn(enable_device: bool) -> Self {
            let (sender, rx) = bounded::<Command>(128);
            let meters = Meters::new();
            let error = Arc::new(std::sync::Mutex::new(None));
            let status = meters.clone();
            let errors = error.clone();
            thread::Builder::new()
                .name("audio-control".into())
                .spawn(move || {
                    let mut shadow_voices = vec![Voice::default(); 4];
                    let mut shadow_bpm = 120.;
                    let mut banks: [Vec<Arc<PreparedSample>>; 4] = std::array::from_fn(|_| vec![]);
                    let mut running = false;
                    let mut editor = None;
                    let mut connection: Option<(cpal::Stream, Control, String)> = None;
                    let mut checked = Instant::now() - Duration::from_secs(3);
                    loop {
                        if enable_device && checked.elapsed() > Duration::from_secs(2) {
                            checked = Instant::now();
                            let default_name = cpal::default_host()
                                .default_output_device()
                                .and_then(|d| d.name().ok())
                                .unwrap_or_default();
                            if connection
                                .as_ref()
                                .is_some_and(|(_, _, name)| *name != default_name)
                                || status.failed.swap(false, Ordering::Relaxed)
                            {
                                connection = None;
                            }
                            if connection.is_none() {
                                match connect(status.clone()) {
                                    Ok((stream, control, name)) => {
                                        let _ = control.send(Command::Settings(
                                            shadow_voices.clone(),
                                            shadow_bpm,
                                        ));
                                        for (i, bank) in banks.iter().enumerate() {
                                            let _ = control.send(Command::Bank(i, bank.clone()));
                                        }
                                        let _ = control.send(Command::Edit(editor.clone()));
                                        if running {
                                            let _ = control.send(Command::Play);
                                        }
                                        *errors.lock().unwrap() = None;
                                        connection = Some((stream, control, name));
                                    }
                                    Err(e) => {
                                        *errors.lock().unwrap() = Some(e);
                                        status.running.store(false, Ordering::Relaxed);
                                    }
                                }
                            }
                        }
                        match rx.recv_timeout(Duration::from_millis(50)) {
                            Ok(command) => {
                                match &command {
                                    Command::Settings(v, b) => {
                                        shadow_voices = v.clone();
                                        shadow_bpm = *b
                                    }
                                    Command::Bank(i, b) if *i < 4 => banks[*i] = b.clone(),
                                    Command::Edit(edit) => editor = edit.clone(),
                                    Command::Play => running = true,
                                    Command::Stop => running = false,
                                    _ => {}
                                }
                                if let Some((_, control, _)) = &connection {
                                    if let Err(e) = control.send(command) {
                                        *errors.lock().unwrap() = Some(e);
                                    }
                                }
                            }
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                            Err(_) => {}
                        }
                    }
                })
                .expect("create audio control");
            Self {
                sender,
                meters,
                error,
            }
        }
        pub fn send(&self, c: Command) -> Result<(), String> {
            self.sender
                .try_send(c)
                .map_err(|_| "Audio engine is busy. Please try again.".into())
        }
    }
    fn connect(meters: Arc<Meters>) -> Result<(cpal::Stream, Control, String), String> {
        let host = cpal::default_host();
        let device=host.default_output_device().ok_or("No audio output device. Connect speakers or headphones; the app will retry automatically.")?;
        let name = device.name().unwrap_or_else(|_| "Default output".into());
        let default = device.default_output_config().map_err(|e| e.to_string())?;
        let config = device
            .supported_output_configs()
            .ok()
            .and_then(|mut configs| {
                configs
                    .find(|c| {
                        c.channels() == 2
                            && c.sample_format() == cpal::SampleFormat::F32
                            && c.min_sample_rate().0 <= 44100
                            && c.max_sample_rate().0 >= 44100
                    })
                    .map(|c| c.with_sample_rate(cpal::SampleRate(44100)))
            })
            .unwrap_or(default);
        let (renderer, control) = Renderer::new(config.sample_rate().0, meters.clone());
        let cfg: cpal::StreamConfig = config.clone().into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &cfg, renderer, meters),
            cpal::SampleFormat::I16 => build::<i16>(&device, &cfg, renderer, meters),
            cpal::SampleFormat::U16 => build::<u16>(&device, &cfg, renderer, meters),
            cpal::SampleFormat::I32 => build::<i32>(&device, &cfg, renderer, meters),
            _ => return Err("Unsupported output sample format".into()),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, control, name))
    }
    fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
        device: &cpal::Device,
        cfg: &cpal::StreamConfig,
        mut renderer: Renderer,
        meters: Arc<Meters>,
    ) -> Result<cpal::Stream, String> {
        let channels = cfg.channels as usize;
        // Fixed scratch storage: arbitrary device callback sizes are processed
        // in chunks without allocation, decoding, locks or filesystem I/O.
        let mut scratch = vec![0f32; 4096 * channels];
        device
            .build_output_stream(
                cfg,
                move |data: &mut [T], _| {
                    for part in data.chunks_mut(scratch.len()) {
                        renderer.render(&mut scratch[..part.len()], channels);
                        for (out, value) in part.iter_mut().zip(&scratch) {
                            *out = T::from_sample(*value);
                        }
                    }
                },
                move |_| {
                    meters.failed.store(true, Ordering::Relaxed);
                },
                None,
            )
            .map_err(|e| e.to_string())
    }
}
