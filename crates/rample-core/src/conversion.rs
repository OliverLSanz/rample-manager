//! Offline, bounded-memory conversion. Originals are backed up before publication.
use crate::{catalog, wav};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Seek, SeekFrom, Write},
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
fn hash(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Converted {
    pub source: String,
    pub target: String,
    pub backup: String,
}
fn unchanged(path: &Path, expected: &str) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        && catalog::fingerprint(path) == expected
}
/// Uses a reviewed fingerprint, rechecks the original bytes before replacement,
/// and never overwrites a different file at the desired .wav destination.
pub fn convert(source: &Path, expected: &str, backups: &Path) -> Result<Converted, String> {
    replace_with(source, expected, backups, transcode)
}
pub(crate) fn replace_with(
    source: &Path,
    expected: &str,
    backups: &Path,
    render: impl FnOnce(&Path, &mut File) -> Result<(), String>,
) -> Result<Converted, String> {
    if !unchanged(source, expected) {
        return Err("Sample changed since review. Check it again before converting.".into());
    }
    let same = source
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("wav"));
    let target = if same {
        source.to_owned()
    } else {
        source.with_extension("wav")
    };
    if !same && target.try_exists().map_err(|e| e.to_string())? {
        return Err("A WAV with this name already exists. Rename the new sample first.".into());
    }
    if fs::metadata(source)
        .map_err(|e| e.to_string())?
        .permissions()
        .readonly()
    {
        return Err("The sample is read-only. Make it writable before converting.".into());
    }
    let parent = source.parent().ok_or("Missing sample folder")?;
    let backup_dir = backups.join(format!("{:016x}", rand::random::<u64>()));
    fs::create_dir_all(backups).map_err(|e| e.to_string())?;
    fs::create_dir(&backup_dir).map_err(|e| e.to_string())?;
    let backup = backup_dir.join(source.file_name().ok_or("Missing sample filename")?);
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut original = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|e| format!("Could not back up the original: {e}"))?;
    std::io::copy(&mut input, &mut original).map_err(|e| e.to_string())?;
    original.sync_all().map_err(|e| e.to_string())?;
    drop((input, original));
    let digest = hash(&backup)?;
    let converted = Converted {
        source: source.to_string_lossy().into(),
        target: target.to_string_lossy().into(),
        backup: backup.to_string_lossy().into(),
    };
    let manifest = serde_json::json!({"source": converted.source, "target": converted.target, "backup": converted.backup, "sha256": digest});
    fs::write(
        backup_dir.join("restore.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut temp = tempfile::Builder::new()
        .prefix(".rample-convert-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|e| e.to_string())?;
    render(&backup, temp.as_file_mut())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let spec = wav::inspect(temp.path())?;
    if spec.rate != 44100
        || spec.bits != 16
        || spec.channels != 1
        || spec.format_tag != 1
        || spec.frames < 2205
    {
        return Err("Converted WAV failed Rample validation; the original is preserved.".into());
    }
    if !unchanged(source, expected) || hash(source)? != digest {
        return Err("Sample changed during conversion; the original is preserved.".into());
    }
    let permissions = fs::metadata(source)
        .map_err(|e| e.to_string())?
        .permissions();
    temp.as_file()
        .set_permissions(permissions.clone())
        .map_err(|e| e.to_string())?;
    if same {
        temp.persist(&target).map_err(|e| e.error.to_string())?;
    } else {
        // Atomic no-clobber publication where supported. FAT/exFAT fallback
        // exclusively creates the new name before copying the validated output.
        match temp.persist_noclobber(&target) {
            Ok(_) => {}
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(
                    "A WAV with this name appeared during conversion. It was preserved.".into(),
                )
            }
            Err(mut e) => {
                let mut dest = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(|err| err.to_string())?;
                let copied = (|| {
                    e.file
                        .as_file_mut()
                        .seek(SeekFrom::Start(0))
                        .map_err(|err| err.to_string())?;
                    std::io::copy(e.file.as_file_mut(), &mut dest)
                        .map_err(|err| err.to_string())?;
                    dest.sync_all().map_err(|err| err.to_string())?;
                    dest.set_permissions(permissions)
                        .map_err(|err| err.to_string())
                })();
                drop(dest);
                if let Err(err) = copied {
                    let _ = fs::remove_file(&target);
                    return Err(err);
                }
            }
        }
        if !unchanged(source, expected) || !hash(source).is_ok_and(|value| value == digest) {
            let _ = fs::remove_file(&target);
            return Err("Sample changed during conversion; the original is preserved.".into());
        }
        if let Err(e) = fs::remove_file(source) {
            let _ = fs::remove_file(&target);
            return Err(format!(
                "Could not replace the source; the original is preserved: {e}"
            ));
        }
    }
    Ok(converted)
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
