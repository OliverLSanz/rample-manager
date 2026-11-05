//! Offline, bounded-memory decoding into Rample preparations and playback files.
use crate::wav;
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufWriter, Seek, SeekFrom, Write},
    path::Path,
};
use symphonia::core::{
    codecs::audio::AudioDecoderOptions,
    formats::{probe::Hint, FormatOptions, FormatReader, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

pub fn is_audio(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        matches!(
            ext.to_string_lossy().to_ascii_lowercase().as_str(),
            "wav"
                | "wave"
                | "aif"
                | "aiff"
                | "aifc"
                | "flac"
                | "mp3"
                | "ogg"
                | "oga"
                | "m4a"
                | "aac"
        )
    })
}
fn open(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
        hint.with_extension(ext);
    }
    symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(file), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())
}
#[derive(Default)]
pub struct AudioInfo {
    pub format: String,
    pub frames: u64,
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub duration: f64,
}
pub fn inspect(path: &Path) -> Result<AudioInfo, String> {
    let format = open(path)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("No audio track")?;
    let p = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("Missing audio format")?;
    symphonia::default::get_codecs()
        .make_audio_decoder(p, &AudioDecoderOptions::default())
        .map_err(|e| e.to_string())?;
    if track.num_frames == Some(0) {
        return Err("The sample contains no audio.".into());
    }
    let rate = p.sample_rate.unwrap_or(0);
    Ok(AudioInfo {
        format: format.format_info().short_name.to_ascii_uppercase(),
        frames: track.num_frames.unwrap_or(0),
        rate,
        channels: p.channels.as_ref().map_or(0, |c| c.count() as u16),
        bits: p.bits_per_sample.unwrap_or(0) as u16,
        duration: if rate > 0 {
            track.num_frames.unwrap_or(0) as f64 / rate as f64
        } else {
            0.
        },
    })
}
pub(crate) struct PcmWriter<'a> {
    file: BufWriter<&'a mut File>,
    frames: u64,
    channels: u16,
    rng: u64,
    dither: bool,
}
impl<'a> PcmWriter<'a> {
    pub(crate) fn new(file: &'a mut File, channels: u16, dither: bool) -> Result<Self, String> {
        let mut file = BufWriter::new(file);
        file.write_all(&[0; 44]).map_err(|e| e.to_string())?;
        Ok(Self {
            file,
            frames: 0,
            channels,
            rng: rand::random::<u64>().max(1),
            dither,
        })
    }
    fn noise(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / ((1u64 << 53) as f64)
    }
    pub(crate) fn frame(&mut self, values: [f64; 2], silence: bool) -> Result<(), String> {
        if self.frames >= (u32::MAX as u64 - 36) / (u64::from(self.channels) * 2) {
            return Err(
                "Converted audio is too large for a standard RIFF WAV. The original is preserved."
                    .into(),
            );
        }
        for value in values.iter().take(self.channels as usize) {
            let value = if value.is_finite() {
                value.clamp(-1., 1.)
            } else {
                0.
            };
            let noise = if self.dither && !silence {
                self.noise() - self.noise()
            } else {
                0.
            };
            let sample = (value * 32768. + noise).round().clamp(-32768., 32767.) as i16;
            self.file
                .write_all(&sample.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        self.frames += 1;
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<(), String> {
        if self.frames == 0 {
            return Err("The sample contains no audio. The original is preserved.".into());
        }
        while self.frames < 2205 {
            self.frame([0.; 2], true)?;
        }
        let bytes = (self.frames * self.channels as u64 * 2) as u32;
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| e.to_string())?;
        for data in [
            b"RIFF".to_vec(),
            (bytes + 36).to_le_bytes().to_vec(),
            b"WAVEfmt ".to_vec(),
            16u32.to_le_bytes().to_vec(),
            1u16.to_le_bytes().to_vec(),
            self.channels.to_le_bytes().to_vec(),
            44100u32.to_le_bytes().to_vec(),
            (44100u32 * self.channels as u32 * 2).to_le_bytes().to_vec(),
            (self.channels * 2).to_le_bytes().to_vec(),
            16u16.to_le_bytes().to_vec(),
            b"data".to_vec(),
            bytes.to_le_bytes().to_vec(),
        ] {
            self.file.write_all(&data).map_err(|e| e.to_string())?;
        }
        self.file.flush().map_err(|e| e.to_string())
    }
}
struct Resampler {
    rate: u32,
    table: Vec<[f64; 128]>,
    queue: VecDeque<[f64; 2]>,
    start: u64,
    received: u64,
    output: u64,
}
impl Resampler {
    fn new(rate: u32) -> Self {
        let cutoff = (44100. / rate as f64).min(1.) * 0.94;
        let table = if rate == 44100 {
            vec![]
        } else {
            (0..2048)
                .map(|phase| {
                    let mut weights = [0.; 128];
                    for (i, weight) in weights.iter_mut().enumerate() {
                        let x = i as f64 - 63. - phase as f64 / 2048.;
                        let window = if x.abs() < 64. {
                            0.42 + 0.5 * (std::f64::consts::PI * x / 64.).cos()
                                + 0.08 * (std::f64::consts::TAU * x / 64.).cos()
                        } else {
                            0.
                        };
                        *weight = window
                            * if x.abs() < 1e-9 {
                                cutoff
                            } else {
                                (std::f64::consts::PI * x * cutoff).sin()
                                    / (std::f64::consts::PI * x)
                            };
                    }
                    let sum: f64 = weights.iter().sum();
                    weights.iter_mut().for_each(|x| *x /= sum);
                    weights
                })
                .collect()
        };
        Self {
            rate,
            table,
            queue: VecDeque::new(),
            start: 0,
            received: 0,
            output: 0,
        }
    }
    fn push(&mut self, frame: [f64; 2], writer: &mut PcmWriter<'_>) -> Result<(), String> {
        if self.rate == 44100 {
            return writer.frame(frame, false);
        }
        self.queue.push_back(frame);
        self.received += 1;
        self.drain(false, writer)
    }
    fn drain(&mut self, eof: bool, writer: &mut PcmWriter<'_>) -> Result<(), String> {
        let total = (self.received * 44100).div_ceil(self.rate as u64);
        while self.output < total {
            let position = self.output * self.rate as u64;
            let base = position / 44100;
            if !eof && base + 64 >= self.received {
                break;
            }
            let phase = ((position % 44100) * 2048 / 44100) as usize;
            let mut frame = [0.; 2];
            for (i, weight) in self.table[phase].iter().enumerate() {
                let index = base as i64 + i as i64 - 63;
                if index >= self.start as i64 && index < self.received as i64 {
                    if let Some(value) = self.queue.get(index as usize - self.start as usize) {
                        for c in 0..2 {
                            frame[c] += value[c] * weight;
                        }
                    }
                }
            }
            writer.frame(frame, false)?;
            self.output += 1;
            let keep = (self.output * self.rate as u64 / 44100).saturating_sub(63);
            while self.start < keep && !self.queue.is_empty() {
                self.queue.pop_front();
                self.start += 1;
            }
        }
        Ok(())
    }
}
/// Render a separate Rample derivative using the established conversion policy.
/// The caller owns a distinct staging file and validates/publishes it atomically.
pub fn prepare_rample(source: &Path, target: &mut File) -> Result<(), String> {
    transcode(source, target)
}

fn transcode(source: &Path, target: &mut File) -> Result<(), String> {
    let mut format = open(source)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("No audio track")?;
    let track_id = track.id;
    let expected_frames = track.num_frames;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(
            track
                .codec_params
                .as_ref()
                .and_then(|p| p.audio())
                .ok_or("Missing audio format")?,
            &AudioDecoderOptions::default().verify(true),
        )
        .map_err(|e| e.to_string())?;
    let exact = wav::inspect(source)
        .is_ok_and(|s| s.rate == 44100 && s.code == 1 && s.bits <= 16 && s.channels == 1);
    let mut target = Some(target);
    let mut output = None;
    let mut resampler = None;
    let mut spec = None;
    let mut decoded_frames = 0;
    while let Some(packet) = format
        .next_packet()
        .map_err(|e| format!("Could not decode the sample: {e}"))?
    {
        if packet.track_id != track_id {
            continue;
        }
        let audio = decoder
            .decode(&packet)
            .map_err(|e| format!("Could not decode the sample: {e}"))?;
        let current = audio.spec().clone();
        if spec.as_ref().is_some_and(|old| old != &current) {
            return Err("Audio format changes inside the file; the original is preserved.".into());
        }
        if current.rate() == 0 || current.rate() > 384000 || current.channels().count() == 0 {
            return Err("Unsupported audio sample rate or channel layout.".into());
        }
        if spec.is_none() {
            output = Some(PcmWriter::new(target.take().unwrap(), 1, !exact)?);
            resampler = Some(Resampler::new(current.rate()));
            spec = Some(current.clone());
        }
        let mut buffer = vec![0f64; audio.samples_interleaved()];
        audio.copy_to_slice_interleaved(&mut buffer);
        let channels = current.channels().count();
        for frame in buffer.chunks_exact(channels) {
            let clean = |v: f64| if v.is_finite() { v } else { 0. };
            // Average rather than sum to retain headroom when channels correlate.
            let mono = frame.iter().map(|v| clean(*v)).sum::<f64>() / channels as f64;
            let values = [mono; 2];
            resampler
                .as_mut()
                .unwrap()
                .push(values, output.as_mut().unwrap())?;
            decoded_frames += 1;
        }
    }
    if decoded_frames == 0 || expected_frames.is_some_and(|n| decoded_frames < n) {
        return Err("The audio is empty or incomplete; the original is preserved.".into());
    }
    if decoder.finalize().verify_ok == Some(false) {
        return Err("The audio checksum failed; the original is preserved.".into());
    }
    let mut output = output.ok_or("No decoded audio")?;
    if let Some(r) = &mut resampler {
        if r.rate != 44100 {
            r.drain(true, &mut output)?;
        }
    }
    output.finish()
}

/// Decode a complete Original to a separate, channel-preserving playback artifact.
/// No resampling, mono mixing, truncation, padding or PCM16 quantization is applied.
/// Decoding is packet bounded; the artifact is disposable and is never a master.
pub fn decode_original(source: &Path, target: &mut File) -> Result<AudioInfo, String> {
    let mut format = open(source)?;
    let container = format.format_info().short_name.to_ascii_uppercase();
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("No audio track")?;
    let track_id = track.id;
    let expected_frames = track.num_frames;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("Missing audio format")?;
    let bits = params.bits_per_sample.unwrap_or(0) as u16;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().verify(true))
        .map_err(|e| e.to_string())?;
    let mut output = BufWriter::new(target);
    output.write_all(&[0; 44]).map_err(|e| e.to_string())?;
    let mut spec = None;
    let mut frames = 0u64;
    while let Some(packet) = format
        .next_packet()
        .map_err(|e| format!("Could not decode the Original: {e}"))?
    {
        if packet.track_id != track_id {
            continue;
        }
        let audio = decoder
            .decode(&packet)
            .map_err(|e| format!("Could not decode the Original: {e}"))?;
        let current = audio.spec().clone();
        if spec.as_ref().is_some_and(|old| old != &current) {
            return Err("Audio format changes inside the file.".into());
        }
        let channels = current.channels().count();
        if current.rate() == 0 || current.rate() > 384000 || !(1..=2).contains(&channels) {
            return Err(
                "Original audition supports mono or stereo audio at up to 384000 Hz.".into(),
            );
        }
        spec = Some(current);
        let mut buffer = vec![0f64; audio.samples_interleaved()];
        audio.copy_to_slice_interleaved(&mut buffer);
        frames += (buffer.len() / channels) as u64;
        if frames > (u32::MAX as u64 - 36) / (channels as u64 * 8) {
            return Err(
                "Decoded Original exceeds the standard WAV playback artifact size limit.".into(),
            );
        }
        for value in buffer {
            let value = if value.is_finite() { value } else { 0. };
            output
                .write_all(&value.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    if frames == 0 || expected_frames.is_some_and(|n| frames < n) {
        return Err("The audio is empty or incomplete; the Original is preserved.".into());
    }
    if decoder.finalize().verify_ok == Some(false) {
        return Err("The audio checksum failed; the Original is preserved.".into());
    }
    let spec = spec.ok_or("No decoded audio")?;
    let channels = spec.channels().count() as u16;
    let rate = spec.rate();
    let bytes = (frames * u64::from(channels) * 8) as u32;
    output.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    for data in [
        b"RIFF".to_vec(),
        (bytes + 36).to_le_bytes().to_vec(),
        b"WAVEfmt ".to_vec(),
        16u32.to_le_bytes().to_vec(),
        3u16.to_le_bytes().to_vec(),
        channels.to_le_bytes().to_vec(),
        rate.to_le_bytes().to_vec(),
        (rate * u32::from(channels) * 8).to_le_bytes().to_vec(),
        (channels * 8).to_le_bytes().to_vec(),
        64u16.to_le_bytes().to_vec(),
        b"data".to_vec(),
        bytes.to_le_bytes().to_vec(),
    ] {
        output.write_all(&data).map_err(|e| e.to_string())?;
    }
    output.flush().map_err(|e| e.to_string())?;
    Ok(AudioInfo {
        format: container,
        frames,
        rate,
        channels,
        bits,
        duration: frames as f64 / f64::from(rate),
    })
}
