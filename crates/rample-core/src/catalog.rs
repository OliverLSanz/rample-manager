use crate::{conversion, wav};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub id: String,
    pub name: String,
    pub voice: usize,
    pub revision: String,
    pub duration: f64,
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub warnings: Vec<String>,
    pub readable: bool,
    pub needs_conversion: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Kit {
    pub id: String,
    pub bank: String,
    pub path: String,
    pub samples: Vec<Sample>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Catalog {
    pub kits: Vec<Kit>,
    pub error: Option<String>,
    pub unassigned: Vec<Sample>,
}
#[derive(Default)]
pub struct Scanner {
    cache: HashMap<PathBuf, (String, Sample)>,
    epoch: u64,
}
pub fn is_kit_name(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() == 2 || b.len() == 3)
        && b[0].is_ascii_uppercase()
        && b[1..].iter().all(u8::is_ascii_digit)
}
pub fn fingerprint(path: &Path) -> String {
    let Ok(m) = fs::symlink_metadata(path) else {
        return String::new();
    };
    let time = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|t| t.as_nanos())
        .unwrap_or(0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}-{time}-{}-{}-{}",
            m.len(),
            m.ino(),
            m.ctime(),
            m.ctime_nsec()
        )
    }
    #[cfg(not(unix))]
    {
        format!("{}-{time}-{}", m.len(), m.permissions().readonly())
    }
}
impl Scanner {
    pub fn scan(&mut self, root: &Path, invalidated: &HashSet<PathBuf>, force: bool) -> Catalog {
        let dirs=match fs::read_dir(root){Ok(d)=>d,Err(e)=>return Catalog{kits:vec![],error:Some(format!("Could not read the library. Check that the folder or SD card is connected. {e}")),unassigned:vec![]}};
        self.epoch += 1;
        let mut next = HashMap::new();
        let mut kits = Vec::new();
        for dir in dirs.flatten() {
            let path = dir.path();
            let name = dir.file_name().to_string_lossy().into_owned();
            if !is_kit_name(&name) || !dir.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let mut samples = Vec::new();
            let mut warnings = Vec::new();
            match fs::read_dir(&path) {
                Ok(files) => {
                    for file in files.flatten() {
                        let p = file.path();
                        let filename = file.file_name().to_string_lossy().into_owned();
                        if !file.file_type().is_ok_and(|t| t.is_file()) || !conversion::is_audio(&p)
                        {
                            continue;
                        }
                        let voice = filename
                            .as_bytes()
                            .first()
                            .filter(|c| b"1234".contains(c))
                            .map(|n| (n - b'0') as usize)
                            .unwrap_or(0);
                        let stamp = fingerprint(&p);
                        let invalid = force
                            || invalidated.contains(&p)
                            || invalidated.contains(&path)
                            || invalidated.contains(root);
                        let sample = if let Some((old, s)) = self
                            .cache
                            .get(&p)
                            .filter(|(old, _)| old == &stamp && !invalid)
                        {
                            let _ = old;
                            s.clone()
                        } else {
                            inspect_sample(&p, voice, format!("{stamp}-{}", self.epoch))
                        };
                        next.insert(p, (stamp, sample.clone()));
                        samples.push(sample);
                    }
                }
                Err(e) => warnings.push(format!("Could not read this kit: {e}")),
            }
            samples.sort_by(|a, b| natord::compare(&a.name, &b.name));
            if !samples
                .iter()
                .any(|s| s.voice == 1 && s.readable && s.warnings.is_empty())
            {
                warnings
                    .push("Voice 1 needs a compatible WAV; Rample cannot open this kit.".into());
            }
            for voice in 1..=4 {
                let layers: Vec<_> = samples.iter().filter(|s| s.voice == voice).collect();
                if layers.len() > 12 {
                    warnings.push(format!(
                        "Voice {voice}: {} layers; Rample supports 12.",
                        layers.len()
                    ));
                }
                if layers.iter().any(|s| s.channels == 1) && layers.iter().any(|s| s.channels == 2)
                {
                    warnings.push(format!("Voice {voice}: mixed mono and stereo layers."));
                }
            }
            if samples.iter().any(|s| s.channels == 2) {
                warnings.push(
                    "A stereo sample uses two hardware voices on Rample. It plays in stereo here."
                        .into(),
                );
            }
            for sample in &mut samples {
                for warning in &warnings {
                    if warning.contains("needs a compatible WAV")
                        || warning.starts_with(&format!("Voice {}:", sample.voice))
                    {
                        sample.warnings.push(warning.clone());
                    }
                }
                if sample.voice == 0 && !warnings.iter().any(|w| w.contains("filename")) {
                    warnings
                        .push("Some sample filenames must start with voice 1, 2, 3 or 4.".into());
                }
            }
            kits.push(Kit {
                id: name.clone(),
                bank: name[..1].into(),
                path: path.to_string_lossy().into_owned(),
                samples,
                warnings,
            });
        }
        let mut unassigned = vec![];
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || (!e.file_name().to_string_lossy().starts_with('.')
                        && e.file_name() != "Conversion backups"
                        && e.file_name() != "_save")
            })
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file() && conversion::is_audio(e.path()))
        {
            let p = entry.path();
            let relative = p.strip_prefix(root).unwrap_or(p);
            let parts: Vec<_> = relative.components().collect();
            if parts.len() == 2 && is_kit_name(&parts[0].as_os_str().to_string_lossy()) {
                continue;
            }
            let stamp = fingerprint(p);
            let mut sample = if let Some((_, cached)) = self
                .cache
                .get(p)
                .filter(|(old, _)| old == &stamp && !force && !invalidated.contains(p))
            {
                cached.clone()
            } else {
                let voice = p
                    .file_name()
                    .and_then(|x| x.to_str())
                    .and_then(|s| s.as_bytes().first().copied())
                    .filter(|n| b"1234".contains(n))
                    .map_or(0, |n| (n - b'0') as usize);
                let mut sample = inspect_sample(p, voice, format!("{stamp}-{}", self.epoch));
                sample.warnings.push("Move this file directly into an A0–Z99 kit folder at the library root before converting.".into());
                sample
            };
            sample.voice = 0;
            next.insert(p.to_owned(), (stamp, sample.clone()));
            unassigned.push(sample);
        }
        unassigned.sort_by(|a, b| natord::compare(&a.id, &b.id));
        kits.sort_by(|a, b| natord::compare(&a.id, &b.id));
        self.cache = next;
        Catalog {
            kits,
            error: None,
            unassigned,
        }
    }
}
fn inspect_sample(path: &Path, voice: usize, revision: String) -> Sample {
    let id = path.to_string_lossy().into_owned();
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let name_issue = if voice == 0 {
        vec![
            "Filename must start with voice 1, 2, 3 or 4. Rename the file before converting."
                .into(),
        ]
    } else {
        vec![]
    };
    if !path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
    {
        let info = conversion::inspect(path);
        let mut warnings = name_issue;
        warnings.push("Rample requires a .wav file; this audio format needs conversion.".into());
        if let Err(e) = &info {
            warnings.push(format!("Cannot decode audio: {e}"));
        }
        let info = info.unwrap_or_default();
        return Sample {
            id,
            name,
            voice,
            revision,
            duration: info.duration,
            rate: info.rate,
            channels: info.channels,
            bits: info.bits,
            warnings,
            readable: false,
            needs_conversion: true,
        };
    }
    match wav::inspect(path) {
        Ok(s) => {
            let duration = s.frames as f64 / f64::from(s.rate);
            let mut warnings = name_issue.clone();
            if s.code != 1 || s.format_tag != 1 {
                warnings.push("Rample requires uncompressed integer PCM WAV.".into());
            }
            if ![8, 16].contains(&s.bits) {
                warnings.push("Rample requires 8 or 16 bits.".into());
            }
            if s.rate != 44100 {
                warnings.push("Rample requires 44.1 kHz.".into());
            }
            if duration < 0.05 {
                warnings.push("Rample requires at least 50 ms.".into());
            }
            Sample {
                id,
                name,
                voice,
                revision,
                duration,
                rate: s.rate,
                channels: s.channels,
                bits: s.bits,
                needs_conversion: warnings.len() > name_issue.len() && s.frames > 0,
                warnings,
                readable: s.frames > 0,
            }
        }
        Err(e) => Sample {
            id,
            name,
            voice,
            revision,
            duration: 0.,
            rate: 0,
            channels: 0,
            bits: 0,
            warnings: name_issue
                .into_iter()
                .chain([format!("Unreadable WAV: {e}")])
                .collect(),
            readable: false,
            needs_conversion: true,
        },
    }
}

pub fn export(samples: &[Option<Sample>], parent: &Path, name: &str) -> Result<PathBuf, String> {
    if !is_kit_name(name) {
        return Err("Use a kit name from A0 to Z99.".into());
    }
    if samples.len() != 4 || !samples[0].as_ref().is_some_and(|s| s.readable) {
        return Err("The kit needs a sample in voice 1.".into());
    }
    let chosen: Vec<_> = samples.iter().flatten().collect();
    if chosen
        .iter()
        .any(|s| !s.readable || s.needs_conversion || s.voice == 0 || s.channels != 1)
    {
        return Err("Export requires compatible mono WAV files. Review the voice warnings.".into());
    }
    let target = parent.join(name);
    if target.exists() {
        return Err(format!(
            "{name} already exists. Existing kits are never overwritten."
        ));
    }
    let stage = parent.join(format!(".rample-export-{:016x}", rand::random::<u64>()));
    let kit = stage.join(name);
    fs::create_dir_all(&kit).map_err(|e| e.to_string())?;
    let mut claimed = false;
    let result = (|| {
        for (i, s) in samples.iter().enumerate() {
            if let Some(s) = s {
                let filename = Path::new(&s.id)
                    .file_name()
                    .ok_or("Invalid sample filename")?
                    .to_string_lossy();
                let new = format!("{}{}", i + 1, filename.chars().skip(1).collect::<String>());
                fs::copy(&s.id, kit.join(new)).map_err(|e| e.to_string())?;
            }
        }
        let check = Scanner::default().scan(&stage, &HashSet::new(), true);
        let copied = check
            .kits
            .first()
            .ok_or("Could not validate the exported kit")?;
        if copied.samples.len() != chosen.len()
            || copied
                .samples
                .iter()
                .any(|s| !s.readable || s.needs_conversion || s.voice == 0 || s.channels != 1)
        {
            return Err("A sample changed during export. Please try again.".into());
        }
        // create_dir is the exclusive claim: another export cannot overwrite it.
        fs::create_dir(&target).map_err(|e| format!("Could not create new kit: {e}"))?;
        claimed = true;
        // Move files into our exclusively claimed directory; roll back on failure.
        for entry in fs::read_dir(&kit).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            fs::rename(entry.path(), target.join(entry.file_name())).map_err(|e| e.to_string())?;
        }
        Ok(target.clone())
    })();
    let _ = fs::remove_dir_all(&stage);
    if result.is_err() && claimed {
        let _ = fs::remove_dir_all(&target);
    }
    result
}
