//! The same bounded-memory DSP and pitch/trim engine powers live preview and WAV baking.
use crate::{
    conversion,
    wav::{self, WavSpec},
};
use serde::{Deserialize, Serialize};
use std::{fs::File, path::Path, sync::Arc};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub start: f64,
    pub end: f64,
    pub gain_db: f64,
    pub pitch: f64,
    pub low_db: f64,
    pub mid_db: f64,
    pub high_db: f64,
    pub echo_mix: f64,
    pub echo_ms: f64,
    pub echo_feedback: f64,
    pub reverb_mix: f64,
    pub reverb_seconds: f64,
}
impl Settings {
    pub fn original(duration: f64) -> Self {
        Self {
            start: 0.,
            end: duration,
            gain_db: 0.,
            pitch: 0.,
            low_db: 0.,
            mid_db: 0.,
            high_db: 0.,
            echo_mix: 0.,
            echo_ms: 250.,
            echo_feedback: 0.35,
            reverb_mix: 0.,
            reverb_seconds: 1.5,
        }
    }
    pub fn validate(&self, spec: &WavSpec) -> Result<(), String> {
        let duration = spec.frames as f64 / spec.rate as f64;
        if ![
            self.start,
            self.end,
            self.gain_db,
            self.pitch,
            self.low_db,
            self.mid_db,
            self.high_db,
            self.echo_mix,
            self.echo_ms,
            self.echo_feedback,
            self.reverb_mix,
            self.reverb_seconds,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.start < 0.
            || self.end > duration + 1e-8
            || self.end <= self.start
            || ((self.end - self.start) * spec.rate as f64).round() < 1.
            || !(-36. ..=12.).contains(&self.gain_db)
            || !(-24. ..=24.).contains(&self.pitch)
            || [self.low_db, self.mid_db, self.high_db]
                .iter()
                .any(|v| !(-18. ..=18.).contains(v))
            || !(0. ..=1.).contains(&self.echo_mix)
            || !(30. ..=1000.).contains(&self.echo_ms)
            || !(0. ..=0.7).contains(&self.echo_feedback)
            || !(0. ..=1.).contains(&self.reverb_mix)
            || !(0.2..=5.).contains(&self.reverb_seconds)
        {
            return Err("Invalid editor settings or trim range.".into());
        }
        Ok(())
    }
    pub fn tail_seconds(&self) -> f64 {
        let echo = if self.echo_mix == 0. {
            0.
        } else {
            let repeats = if self.echo_feedback == 0. {
                1.
            } else {
                (1e-4f64.ln() / self.echo_feedback.ln()).ceil().max(1.)
            };
            self.echo_ms / 1000. * repeats
        };
        let reverb = if self.reverb_mix == 0. {
            0.
        } else {
            self.reverb_seconds * 1.5 + 0.1
        };
        // EQ ringing is allowed to decay beyond a trimmed endpoint.
        let eq = if [self.low_db, self.mid_db, self.high_db]
            .iter()
            .any(|v| *v != 0.)
        {
            0.15
        } else {
            0.
        };
        echo + reverb + eq
    }
    pub fn output_seconds(&self) -> f64 {
        ((self.end - self.start) / 2f64.powf(self.pitch / 12.) + self.tail_seconds()).max(0.05)
    }
}

/// Windowed-sinc interpolation, prepared outside the device callback.
pub struct Kernel {
    pub(crate) phases: Vec<[f32; 32]>,
}
impl Kernel {
    pub fn new(source: u32, output: u32) -> Arc<Self> {
        Self::ratio(source as f64 / output as f64)
    }
    fn ratio(ratio: f64) -> Arc<Self> {
        let cutoff = (1. / ratio).min(1.) * 0.95;
        let phases = (0..1024)
            .map(|phase| {
                let mut coefficients = [0.; 32];
                let mut sum = 0.;
                for (i, c) in coefficients.iter_mut().enumerate() {
                    let x = i as f64 - 15. - phase as f64 / 1024.;
                    let window = if x.abs() < 16. {
                        0.5 + 0.5 * (std::f64::consts::PI * x / 16.).cos()
                    } else {
                        0.
                    };
                    *c = (if x.abs() < 1e-9 {
                        cutoff
                    } else {
                        (std::f64::consts::PI * x * cutoff).sin() / (std::f64::consts::PI * x)
                    } * window) as f32;
                    sum += *c;
                }
                coefficients.iter_mut().for_each(|c| *c /= sum);
                coefficients
            })
            .collect();
        Arc::new(Self { phases })
    }
}
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}
impl Biquad {
    // RBJ peaking EQ (W3C Audio EQ Cookbook), fixed octave-wide bands.
    fn peak(rate: u32, frequency: f64, db: f64) -> Self {
        let a = 10f64.powf(db / 40.);
        let w = std::f64::consts::TAU * frequency.min(rate as f64 * 0.4) / rate as f64;
        let alpha = w.sin() / (2. * 0.7);
        let a0 = 1. + alpha / a;
        Self {
            b: [
                (1. + alpha * a) / a0,
                -2. * w.cos() / a0,
                (1. - alpha * a) / a0,
            ],
            a: [-2. * w.cos() / a0, (1. - alpha / a) / a0],
            z: [0.; 2],
        }
    }
    fn next(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}
struct Delay {
    buffer: Vec<f64>,
    cursor: usize,
    damp: f64,
}
impl Delay {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.; size.max(1)],
            cursor: 0,
            damp: 0.,
        }
    }
    fn read(&self) -> f64 {
        self.buffer[self.cursor]
    }
    fn write(&mut self, value: f64) {
        self.buffer[self.cursor] = value;
        self.cursor = (self.cursor + 1) % self.buffer.len();
    }
    fn reset(&mut self) {
        self.buffer.fill(0.);
        self.cursor = 0;
        self.damp = 0.;
    }
}
struct Processor {
    eq: [Biquad; 3],
    echo: Delay,
    combs: [Delay; 4],
    allpass: [Delay; 2],
    gain_target: f64,
    smoothing: f64,
    delay_frames: usize,
    reverb_feedback: [f64; 4],
    gain: f64,
    echo_mix: f64,
    reverb_mix: f64,
    settings: Settings,
}
impl Processor {
    fn new(rate: u32, settings: &Settings) -> Self {
        Self {
            eq: [
                Biquad::peak(rate, 120., settings.low_db),
                Biquad::peak(rate, 1000., settings.mid_db),
                Biquad::peak(rate, 8000., settings.high_db),
            ],
            echo: Delay::new(rate as usize + 1),
            combs: [0.0297, 0.0371, 0.0411, 0.0437].map(|s| Delay::new((s * rate as f64) as usize)),
            allpass: [0.005, 0.0017].map(|s| Delay::new((s * rate as f64) as usize)),
            gain_target: 10f64.powf(settings.gain_db / 20.),
            smoothing: 1. - (-1. / (rate as f64 * 0.005)).exp(),
            delay_frames: (settings.echo_ms * rate as f64 / 1000.).round() as usize,
            reverb_feedback: [0.0297, 0.0371, 0.0411, 0.0437].map(|s| {
                10f64.powf(
                    -3. * ((s * rate as f64) as usize) as f64
                        / rate as f64
                        / settings.reverb_seconds,
                )
            }),
            gain: 10f64.powf(settings.gain_db / 20.),
            echo_mix: settings.echo_mix,
            reverb_mix: settings.reverb_mix,
            settings: settings.clone(),
        }
    }
    fn update(&mut self, other: &Self) {
        for (eq, new) in self.eq.iter_mut().zip(other.eq) {
            eq.b = new.b;
            eq.a = new.a;
        }
        self.settings = other.settings.clone();
        self.gain_target = other.gain_target;
        self.delay_frames = other.delay_frames;
        self.reverb_feedback = other.reverb_feedback;
    }
    fn reset(&mut self) {
        for eq in &mut self.eq {
            eq.z = [0.; 2];
        }
        self.echo.reset();
        self.combs.iter_mut().for_each(Delay::reset);
        self.allpass.iter_mut().for_each(Delay::reset);
        self.gain = self.gain_target;
        self.echo_mix = self.settings.echo_mix;
        self.reverb_mix = self.settings.reverb_mix;
    }
    fn next(&mut self, input: f32) -> f32 {
        // Smooth gain and wet amounts for continuous knob movement (about 5 ms).
        self.gain += (self.gain_target - self.gain) * self.smoothing;
        self.echo_mix += (self.settings.echo_mix - self.echo_mix) * self.smoothing;
        self.reverb_mix += (self.settings.reverb_mix - self.reverb_mix) * self.smoothing;
        let mut dry = input as f64 * self.gain;
        for eq in &mut self.eq {
            dry = eq.next(dry);
        }
        let delay = self.delay_frames;
        let index = (self.echo.cursor + self.echo.buffer.len() - delay) % self.echo.buffer.len();
        let delayed = self.echo.buffer[index];
        self.echo.write(dry + delayed * self.settings.echo_feedback);
        let echoed = dry + delayed * self.echo_mix;
        // Parallel damped Schroeder combs followed by two diffusion allpasses.
        let mut reverbed = 0.;
        for (comb, feedback) in self.combs.iter_mut().zip(self.reverb_feedback) {
            let value = comb.read();
            comb.damp = value * 0.65 + comb.damp * 0.35;
            comb.write(echoed + comb.damp * feedback);
            reverbed += value * 0.25;
        }
        for allpass in &mut self.allpass {
            let delayed = allpass.read();
            let value = delayed - reverbed;
            allpass.write(reverbed + delayed * 0.5);
            reverbed = value;
        }
        let out = echoed + reverbed * self.reverb_mix;
        if out.is_finite() {
            out as f32
        } else {
            0.
        }
    }
}

pub struct Engine {
    pub settings: Settings,
    start: u64,
    end: u64,
    ratio: f64,
    cursor: f64,
    kernel: Arc<Kernel>,
    processor: Processor,
    tail_frames: u64,
    tail_played: u64,
}
impl Engine {
    pub fn new(spec: &WavSpec, rate: u32, settings: &Settings) -> Result<Self, String> {
        settings.validate(spec)?;
        let ratio = spec.rate as f64 / rate as f64 * 2f64.powf(settings.pitch / 12.);
        let start = (settings.start * spec.rate as f64).round() as u64;
        let end = ((settings.end * spec.rate as f64).round() as u64).min(spec.frames);
        Ok(Self {
            settings: settings.clone(),
            start,
            end,
            ratio,
            cursor: start as f64,
            kernel: Kernel::ratio(ratio),
            processor: Processor::new(rate, settings),
            tail_frames: (settings.tail_seconds() * rate as f64).ceil() as u64,
            tail_played: 0,
        })
    }
    pub fn source_position(&self) -> f64 {
        self.cursor
    }
    pub fn follow_source(&mut self, position: f64) {
        self.cursor = position.max(self.start as f64);
    }
    pub fn reset(&mut self) {
        self.cursor = self.start as f64;
        self.tail_played = 0;
        self.processor.reset();
    }
    pub fn update(&mut self, other: Self) {
        self.start = other.start;
        self.end = other.end;
        self.ratio = other.ratio;
        self.kernel = other.kernel;
        self.settings = other.settings;
        self.tail_frames = other.tail_frames;
        self.cursor = self.cursor.max(self.start as f64);
        self.processor.update(&other.processor);
    }
    pub fn next(&mut self, mut read: impl FnMut(i64) -> Option<[f32; 2]>) -> Option<(f32, bool)> {
        let mut complete = true;
        let input = if self.cursor < self.end as f64 {
            let base = self.cursor.floor() as i64;
            let mut get = |index: i64| {
                if index < self.start as i64 || index >= self.end as i64 {
                    return 0.;
                }
                match read(index) {
                    Some(f) => (f[0] + f[1]) * 0.5,
                    None => {
                        complete = false;
                        0.
                    }
                }
            };
            let value = if self.ratio == 1. {
                get(base)
            } else {
                let phase = ((self.cursor - base as f64) * 1024.) as usize;
                self.kernel.phases[phase.min(1023)]
                    .iter()
                    .enumerate()
                    .map(|(i, w)| get(base + i as i64 - 15) * w)
                    .sum()
            };
            self.cursor += self.ratio;
            value
        } else {
            if self.tail_played >= self.tail_frames {
                return None;
            }
            self.tail_played += 1;
            0.
        };
        Some((self.processor.next(input), complete))
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub frames: u64,
    pub duration: f64,
    pub peak: f32,
    pub clipped_frames: u64,
}
struct Reader {
    file: File,
    spec: WavSpec,
    start: u64,
    frames: Vec<[f32; 2]>,
    error: Option<String>,
}
impl Reader {
    fn frame(&mut self, index: i64) -> Option<[f32; 2]> {
        if index < 0 || index as u64 >= self.spec.frames {
            return Some([0.; 2]);
        }
        let n = index as u64;
        if n < self.start || n >= self.start + self.frames.len() as u64 {
            self.start = n.saturating_sub(32);
            match wav::read_frames(&mut self.file, &self.spec, self.start, 16384) {
                Ok(frames) => self.frames = frames,
                Err(e) => {
                    self.error = Some(e);
                    return None;
                }
            }
        }
        self.frames.get((n - self.start) as usize).copied()
    }
}
pub fn render(source: &Path, settings: &Settings, target: &mut File) -> Result<Rendered, String> {
    let spec = wav::inspect(source)?;
    let mut engine = Engine::new(&spec, 44100, settings)?;
    let mut reader = Reader {
        file: File::open(source).map_err(|e| e.to_string())?,
        spec,
        start: 0,
        frames: vec![],
        error: None,
    };
    let mut writer = conversion::PcmWriter::new(target, 1, true)?;
    let (mut frames, mut peak, mut clipped) = (0u64, 0f32, 0u64);
    while let Some((value, complete)) = engine.next(|i| reader.frame(i)) {
        if !complete {
            return Err(reader
                .error
                .unwrap_or_else(|| "Could not read the entire sample.".into()));
        }
        peak = peak.max(value.abs());
        if value.abs() > 1. {
            clipped += 1;
        }
        writer.frame([value.clamp(-1., 1.) as f64; 2], false)?;
        frames += 1;
    }
    writer.finish()?;
    frames = frames.max(2205);
    Ok(Rendered {
        frames,
        duration: frames as f64 / 44100.,
        peak,
        clipped_frames: clipped,
    })
}
