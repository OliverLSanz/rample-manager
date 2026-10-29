use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq)]
pub struct WavSpec {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub code: u16,
    pub data_offset: u64,
    pub frames: u64,
    pub block_align: u16,
}
fn error(message: &str) -> String {
    message.into()
}
fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

pub fn inspect(path: &Path) -> Result<WavSpec, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut head = [0u8; 12];
    f.read_exact(&mut head).map_err(|e| e.to_string())?;
    if &head[..4] != b"RIFF" || &head[8..] != b"WAVE" {
        return Err(error("Expected a standard RIFF/WAVE file"));
    }
    let (mut fmt, mut data) = (None, None);
    for _ in 0..1024 {
        if f.stream_position().map_err(|e| e.to_string())? + 8 > len {
            break;
        }
        let mut h = [0; 8];
        f.read_exact(&mut h).map_err(|e| e.to_string())?;
        let size = u32le(&h[4..]) as u64;
        let start = f.stream_position().map_err(|e| e.to_string())?;
        if start + size > len {
            return Err(error("Truncated WAV chunk"));
        }
        if &h[..4] == b"fmt " {
            if size < 16 {
                return Err(error("Invalid WAV format chunk"));
            }
            let mut b = vec![0; size.min(64) as usize];
            f.read_exact(&mut b).map_err(|e| e.to_string())?;
            let mut code = u16le(&b);
            if code == 0xfffe && b.len() >= 40 {
                code = u16le(&b[24..]);
            }
            fmt = Some((
                code,
                u16le(&b[2..]),
                u32le(&b[4..]),
                u16le(&b[12..]),
                u16le(&b[14..]),
            ));
        } else if &h[..4] == b"data" {
            data = Some((start, size));
        }
        if fmt.is_some() && data.is_some() {
            break;
        }
        f.seek(SeekFrom::Start(start + size + (size % 2)))
            .map_err(|e| e.to_string())?;
    }
    let (code, channels, rate, block_align, bits) = fmt.ok_or("Missing WAV format")?;
    let (data_offset, bytes) = data.ok_or("Missing WAV audio data")?;
    if !(1..=2).contains(&channels) || rate == 0 || rate > 384000 {
        return Err(error(
            "Only mono/stereo WAV with a valid sample rate can be auditioned",
        ));
    }
    if !((code == 1 && [8, 16, 24, 32].contains(&bits)) || (code == 3 && [32, 64].contains(&bits)))
    {
        return Err(error("Unsupported WAV encoding"));
    }
    if block_align != channels * (bits / 8)
        || block_align == 0
        || bytes % u64::from(block_align) != 0
    {
        return Err(error("Invalid WAV frame alignment"));
    }
    Ok(WavSpec {
        rate,
        channels,
        bits,
        code,
        data_offset,
        frames: bytes / u64::from(block_align),
        block_align,
    })
}

fn decode_sample(bytes: &[u8], spec: &WavSpec) -> f32 {
    let value = match (spec.code, spec.bits) {
        (1, 8) => (f32::from(bytes[0]) - 128.) / 128.,
        (1, 16) => f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.,
        (1, 24) => {
            let n = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8;
            n as f32 / 8388608.
        }
        (1, 32) => i32::from_le_bytes(bytes[..4].try_into().unwrap()) as f32 / 2147483648.,
        (3, 32) => f32::from_le_bytes(bytes[..4].try_into().unwrap()),
        (3, 64) => f64::from_le_bytes(bytes[..8].try_into().unwrap()) as f32,
        _ => 0.,
    };
    if value.is_finite() {
        value.clamp(-4., 4.)
    } else {
        0.
    }
}

pub fn read_frames(
    f: &mut File,
    spec: &WavSpec,
    start: u64,
    count: usize,
) -> Result<Vec<[f32; 2]>, String> {
    let count = count.min(spec.frames.saturating_sub(start) as usize);
    f.seek(SeekFrom::Start(
        spec.data_offset + start * u64::from(spec.block_align),
    ))
    .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; count * usize::from(spec.block_align)];
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    let size = usize::from(spec.bits / 8);
    Ok(bytes
        .chunks_exact(usize::from(spec.block_align))
        .map(|frame| {
            let l = decode_sample(frame, spec);
            let r = if spec.channels == 2 {
                decode_sample(&frame[size..], spec)
            } else {
                l
            };
            [l, r]
        })
        .collect())
}

#[derive(Debug)]
pub struct PreparedSample {
    pub path: String,
    pub revision: String,
    pub spec: WavSpec,
    pub head: Vec<[f32; 2]>,
    pub waveform: Vec<f32>,
}
impl PreparedSample {
    pub fn load(path: &Path, revision: &str) -> Result<Arc<Self>, String> {
        let spec = inspect(path)?;
        let mut f = File::open(path).map_err(|e| e.to_string())?;
        let head = read_frames(&mut f, &spec, 0, (spec.rate as usize * 2).min(262144))?;
        let mut waveform = Vec::with_capacity(100);
        for i in 0..100 {
            let start = spec.frames * i / 100;
            let frames = read_frames(&mut f, &spec, start, 512)?;
            let peak = frames
                .iter()
                .flat_map(|f| f.iter())
                .copied()
                .map(f32::abs)
                .fold(0., f32::max);
            waveform.push(peak);
        }
        let max = waveform.iter().copied().fold(0.001, f32::max);
        waveform.iter_mut().for_each(|v| *v /= max);
        Ok(Arc::new(Self {
            path: path.to_string_lossy().into_owned(),
            revision: revision.into(),
            spec,
            head,
            waveform,
        }))
    }
}

pub fn write_pcm16(path: &Path, frames: &[i16], rate: u32) -> Result<(), String> {
    let mut f = File::create(path).map_err(|e| e.to_string())?;
    let len = (frames.len() * 2) as u32;
    let mut h = Vec::new();
    h.extend(b"RIFF");
    h.extend((len + 36).to_le_bytes());
    h.extend(b"WAVEfmt ");
    h.extend(16u32.to_le_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend(1u16.to_le_bytes());
    h.extend(rate.to_le_bytes());
    h.extend((rate * 2).to_le_bytes());
    h.extend(2u16.to_le_bytes());
    h.extend(16u16.to_le_bytes());
    h.extend(b"data");
    h.extend(len.to_le_bytes());
    f.write_all(&h).map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = frames.iter().flat_map(|n| n.to_le_bytes()).collect();
    f.write_all(&bytes).map_err(|e| e.to_string())
}

pub fn create_demo(support: &Path) -> Result<PathBuf, String> {
    let root = support.join("Demo SD");
    let kit = root.join("A0");
    std::fs::create_dir_all(&kit).map_err(|e| e.to_string())?;
    for (i, name) in [
        "1_Sub kick.wav",
        "2_Dust snare.wav",
        "3_Closed hat.wav",
        "4_Wood click.wav",
    ]
    .iter()
    .enumerate()
    {
        let path = kit.join(name);
        if path.exists() {
            continue;
        }
        let duration = [0.55, 0.30, 0.12, 0.19][i];
        let mut rng = 401u64 + i as u64;
        let frames: Vec<i16> = (0..(duration * 44100.) as usize)
            .map(|n| {
                let t = n as f64 / 44100.;
                rng = rng
                    .wrapping_mul(2862933555777941757)
                    .wrapping_add(3037000493);
                let noise = (rng >> 33) as f64 / (u32::MAX >> 1) as f64 * 2. - 1.;
                let value = match i {
                    0 => {
                        (std::f64::consts::TAU * (48. * t + 7. * (1. - (-t * 28.).exp()))).sin()
                            * (-t * 9.).exp()
                            * (t * 1200.).min(1.)
                    }
                    1 => {
                        (noise * 0.65 + (std::f64::consts::TAU * 185. * t).sin() * 0.25)
                            * (-t * 18.).exp()
                            * (t * 2000.).min(1.)
                    }
                    2 => {
                        (noise * 0.7 + (std::f64::consts::TAU * 7800. * t).sin() * 0.1)
                            * (-t * 48.).exp()
                    }
                    _ => {
                        ((std::f64::consts::TAU * 760. * t).sin() * 0.6 + noise * 0.15)
                            * (-t * 38.).exp()
                    }
                };
                (value * 0.8 * 32767.).clamp(-32767., 32767.) as i16
            })
            .collect();
        write_pcm16(&path, &frames, 44100)?;
    }
    Ok(root)
}
