//! SD intake snapshots the source once. Pending reviews and opaque native content
//! live in the managed database; neither committing nor previewing revisits the SD.
use crate::{
    managed::{ImportReply, ManagedLibrary},
    patches::{Layer, Patch, PatchKit, PatchSnapshot, PatchVoice},
    *,
};
use rample_core::{model::LayerMode, native};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::fs;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdFile {
    pub path: String,
    pub sha256: String,
    pub kind: String,
    pub sample_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSd {
    pub intake_id: String,
    pub fingerprint: String,
    pub copy_complete: bool,
    pub files: Vec<SdFile>,
    pub issues: Vec<String>,
    /// Usable Samples whose source does not specify a valid Kit and Voice.
    /// Keep the path as evidence; never assign a guessed Voice.
    pub unassigned: Vec<SdFile>,
    /// Interpretation of verified native fields. Absent for Imports made
    /// before native reading existed. Not part of the recognition fingerprint:
    /// the preserved native bytes already are.
    #[serde(default)]
    pub native: Option<NativeReading>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeReading {
    pub verified_firmware: String,
    /// The user's default Layer mode applied to Kits without stored modes.
    pub default_layer_mode: LayerMode,
    pub kits: Vec<NativeKitReading>,
    /// Problems with the source as a whole, such as a missing `_save` folder.
    #[serde(default)]
    pub issues: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeKitReading {
    pub bank: String,
    pub number: u8,
    pub source: NativeSource,
    pub voices: Vec<NativeVoiceReading>,
    pub issues: Vec<String>,
}
/// Where a Kit's initial Layer modes come from. Serialized as `store`,
/// `appDefault` or `unreadable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeSource {
    /// The Kit's own STOREd file.
    Store,
    /// No stored Kit file, so the user's Default Layer mode applies.
    AppDefault,
    /// Its stored file has an unsupported shape, so the default applies.
    Unreadable,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeVoiceReading {
    pub mode_restored: bool,
    pub selection_restored: bool,
}
impl NativeReading {
    pub fn complete(&self) -> bool {
        self.issues.is_empty() && self.kits.iter().all(|kit| kit.issues.is_empty())
    }
    /// Every problem, source-wide first, in the order the review shows them.
    pub fn all_issues(&self) -> impl Iterator<Item = &String> {
        self.issues
            .iter()
            .chain(self.kits.iter().flat_map(|kit| &kit.issues))
    }
    fn kit(&self, bank: &str, number: u8) -> Option<&NativeKitReading> {
        self.kits
            .iter()
            .find(|kit| kit.bank == bank && kit.number == number)
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdImportReview {
    pub id: String,
    pub status: String,
    pub recognized_patch: Option<String>,
    pub candidates: Vec<SdPatchCandidate>,
    pub audio: ImportReply,
    pub draft: Patch,
    pub native_verified: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SdPatchCandidate {
    pub id: String,
    pub name: String,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SdImportChoice {
    New { name: String },
    Update { patch: String },
    Reuse,
}

pub(super) fn migrate(db: &Connection, version: u32) -> Result<(), String> {
    if version < 7 {
        db.execute_batch(
            "BEGIN IMMEDIATE;
            CREATE TABLE sd_intakes (id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL,
                patch_id TEXT, draft TEXT NOT NULL);
            CREATE INDEX sd_intakes_fingerprint ON sd_intakes(fingerprint);
            CREATE TABLE sd_native_blobs (sha256 TEXT PRIMARY KEY, content BLOB NOT NULL);
            PRAGMA user_version=7; COMMIT;",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
/// Operating-system metadata on removable media; never Rample content.
pub(super) fn is_system_entry(name: &str) -> bool {
    matches!(
        name,
        ".DS_Store"
            | ".Spotlight-V100"
            | ".Trashes"
            | ".fseventsd"
            | "System Volume Information"
            | "$RECYCLE.BIN"
    ) || name.starts_with("._")
}
/// Whether a file name or path has a `.wav` extension, in any letter case.
/// The Rample plays and numbers only these files.
pub(crate) fn is_wav(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn relative(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map_err(|e| e.to_string())?
        .components()
        .map(|part| {
            part.as_os_str().to_str().map(str::to_owned).ok_or_else(|| {
                "The path is not valid Unicode; its assignment could not be preserved.".into()
            })
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|parts| parts.join("/"))
}
/// The Bank and Kit number SD Import reads from a root folder name. Leading
/// zeros are accepted, so `A05` is Kit A5.
pub(crate) fn address(name: &str) -> Option<(String, u8)> {
    if !catalog::is_kit_name(name) {
        return None;
    }
    Some((name[..1].into(), name[1..].parse().ok()?))
}
fn draft(name: String) -> Patch {
    Patch {
        id: Uuid::new_v4().to_string(),
        name,
        selected_kit: String::new(),
        kits: vec![],
        bpm: 120.,
        voices: vec![rample_core::model::Voice::default(); 4],
        imported_sd: None,
        voice_kits: vec![],
    }
}
/// Sorted device WAV files per (Kit ID, Voice), with the Layer each became
/// (`None` when its audio could not be imported).
type VoiceFiles = HashMap<(String, usize), Vec<(String, Option<String>)>>;

/// Restores verified native Layer modes and selections into the draft Kits.
/// Kits never saved with STORE, unverified values and unreadable files use the
/// user's default Layer mode; problems are reported, never guessed. A source
/// without `_save` (for example a copy of only the Kit folders) is reported
/// once, because its Kits cannot be told apart from Kits never saved.
fn read_native(
    patch: &mut Patch,
    default_mode: LayerMode,
    has_save: bool,
    kit_paths: &HashMap<String, String>,
    files: &HashMap<String, Vec<u8>>,
    voice_files: &VoiceFiles,
) -> NativeReading {
    let mut reading = NativeReading {
        verified_firmware: native::VERIFIED_FIRMWARE.into(),
        default_layer_mode: default_mode,
        ..NativeReading::default()
    };
    if !has_save && !kit_paths.is_empty() {
        reading.issues.push("No _save folder was found, so no native settings were read. Layer modes use your Default Layer mode.".into());
    }
    let mut folders: Vec<_> = kit_paths.iter().collect();
    folders.sort_by(|a, b| a.0.cmp(b.0));
    for (folder, kit_id) in folders {
        let kit = patch.kits.iter_mut().find(|k| &k.id == kit_id).unwrap();
        for voice in &mut kit.voices {
            voice.mode = default_mode;
        }
        let mut entry = NativeKitReading {
            bank: kit.bank.clone(),
            number: kit.number,
            source: NativeSource::AppDefault,
            voices: vec![NativeVoiceReading::default(); 4],
            issues: vec![],
        };
        let file = format!("_save/{folder}.rpl");
        match files.get(&file).map(|bytes| native::read_kit(bytes)) {
            Some(Ok(settings)) => {
                entry.source = NativeSource::Store;
                for (channel, stored) in settings.voices.iter().enumerate() {
                    let voice = &mut kit.voices[channel];
                    let restored = &mut entry.voices[channel];
                    match stored.mode {
                        Some(mode) => {
                            voice.mode = mode;
                            restored.mode_restored = true;
                        }
                        None => entry.issues.push(format!("{folder} Voice {}: Stored Layer mode value {} is not a verified mode. Your default Layer mode is used.", channel + 1, stored.raw_mode)),
                    }
                    let layers = voice_files
                        .get(&(kit_id.clone(), channel))
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    if layers.is_empty() {
                        continue;
                    }
                    let position = stored.selected_layer;
                    match usize::try_from(position).ok().and_then(|i| layers.get(i)) {
                        Some((_, Some(layer))) => {
                            voice.selected_layer = layer.clone();
                            restored.selection_restored = true;
                        }
                        Some((path, None)) => entry.issues.push(format!("{folder} Voice {}: Stored selected Layer {} ({path}) could not be imported. The first Layer is selected.", channel + 1, position + 1)),
                        None => entry.issues.push(format!("{folder} Voice {}: Stored selected Layer {} does not exist among its {} Layer file(s). The first Layer is selected.", channel + 1, position.saturating_add(1), layers.len())),
                    }
                }
            }
            Some(Err(error)) => {
                entry.source = NativeSource::Unreadable;
                entry.issues.push(format!("{file} could not be interpreted ({error}). The file is preserved; your default Layer mode is used."));
            }
            // Never saved with STORE: the user's default Layer mode applies.
            None => {}
        }
        reading.kits.push(entry);
    }
    reading
}

impl ManagedLibrary {
    /// SD intakes that currently recognize `patch`.
    pub(crate) fn intakes_of(&self, patch: &str) -> Result<Vec<String>, String> {
        let mut query = self
            .db
            .prepare("SELECT id FROM sd_intakes WHERE patch_id=?1 ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let ids = query
            .query_map([patch], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string());
        ids
    }
    /// Saves Patches together with the intakes they gain or lose, so a deleted
    /// Patch never leaves an SD intake pointing at it.
    pub(crate) fn save_patches_and_intakes(
        &self,
        graph: &crate::patches::PatchGraph,
        links: &[(String, Option<String>)],
    ) -> Result<(), String> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        for (intake, patch) in links {
            tx.execute(
                "UPDATE sd_intakes SET patch_id=?2 WHERE id=?1",
                rusqlite::params![intake, patch],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT INTO patch_state(id,payload) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
            [serde_json::to_string(graph).map_err(|e| e.to_string())?],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    fn recognize_sd(&self, fingerprint: &str) -> Result<Option<String>, String> {
        self.db.query_row("SELECT patch_id FROM sd_intakes WHERE fingerprint=?1 AND patch_id IS NOT NULL ORDER BY rowid DESC LIMIT 1", [fingerprint], |row| row.get(0))
            .optional().map_err(|e| e.to_string())
    }
    fn matching_sd(
        &self,
        fingerprint: &str,
        draft: &Patch,
        graph: &crate::patches::PatchGraph,
    ) -> Result<Option<String>, String> {
        let Some(id) = self.recognize_sd(fingerprint)? else {
            return Ok(None);
        };
        let Some(existing) = graph.patches.iter().find(|p| p.id == id) else {
            return Ok(None);
        };
        let (Some(before), Some(now)) = (&existing.imported_sd, &draft.imported_sd) else {
            return Ok(None);
        };
        // A Patch imported before native reading existed never received the
        // SD's stored Layer modes. Offer an explicit update instead of
        // silently reopening it with app defaults.
        if before.native.is_none() && now.native.is_some() {
            return Ok(None);
        }
        let same = now.files.iter().all(|file| {
            before
                .files
                .iter()
                .find(|old| old.path == file.path && old.kind == file.kind)
                .map(|old| old.sample_id == file.sample_id)
                .unwrap_or(false)
        });
        Ok(same.then_some(id))
    }
    fn prepare_sd(&mut self, source: &Path) -> Result<SdImportReview, String> {
        self.prepare_sd_progress(source, |_, _, _| {})
    }
    pub(super) fn prepare_sd_progress(
        &mut self,
        source: &Path,
        progress: impl FnMut(usize, usize, &Path),
    ) -> Result<SdImportReview, String> {
        if !source.is_dir() {
            return Err("Choose the root folder of a Rample SD or its copy.".into());
        }
        let mut patch = draft(
            source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );
        let id = Uuid::new_v4().to_string();
        let mut state = ImportedSd {
            intake_id: id.clone(),
            fingerprint: String::new(),
            copy_complete: true,
            files: vec![],
            issues: vec![],
            unassigned: vec![],
            native: None,
        };
        let mut audio_paths = vec![];
        let mut native_files = HashMap::new();
        let mut kit_paths = HashMap::new();
        let mut has_save = false;
        for entry in walkdir::WalkDir::new(source)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| !is_system_entry(&entry.file_name().to_string_lossy()))
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    state.copy_complete = false;
                    state.issues.push(format!("Could not read source: {error}"));
                    continue;
                }
            };
            let path = match relative(source, entry.path()) {
                Ok(path) => path,
                Err(error) => {
                    state.copy_complete = false;
                    state.issues.push(error);
                    continue;
                }
            };
            if entry.file_type().is_symlink() {
                state.copy_complete = false;
                state.issues.push(format!(
                    "{path}: Symbolic links are not imported. Import the actual file or folder."
                ));
                state.files.push(SdFile {
                    path,
                    sha256: String::new(),
                    kind: "unsupportedLink".into(),
                    sample_id: None,
                });
                continue;
            }
            if entry.file_type().is_dir() {
                has_save |= path == "_save";
                if !path.contains('/') {
                    if let Some((bank, number)) = address(&path) {
                        if patch
                            .kits
                            .iter()
                            .any(|kit| kit.bank == bank && kit.number == number)
                        {
                            state.issues.push(format!(
                                "{path}: Duplicate Bank/Kit address; its audio is unassigned."
                            ));
                        } else {
                            let id = Uuid::new_v4().to_string();
                            kit_paths.insert(path.clone(), id.clone());
                            patch.kits.push(PatchKit {
                                id,
                                bank,
                                number,
                                voices: vec![PatchVoice::default(); 4],
                            });
                        }
                    }
                }
                // Empty Kits are musical layout; native directories are opaque state.
                // Other directories contribute only through audio or layout errors below.
                if (!path.contains('/') && address(&path).is_some())
                    || path.split('/').next() == Some("_save")
                {
                    state.files.push(SdFile {
                        path,
                        sha256: String::new(),
                        kind: "directory".into(),
                        sample_id: None,
                    });
                }
            } else if entry.file_type().is_file() {
                // _save is always opaque, even when a future native file happens
                // to use a supported audio extension.
                if conversion::is_audio(entry.path())
                    && !path.split('/').any(|part| part == "_save")
                {
                    audio_paths.push((path, entry.into_path()));
                } else if path.split('/').next() == Some("_save") {
                    if entry
                        .metadata()
                        .map(|m| m.len() > 16 * 1024 * 1024)
                        .unwrap_or(true)
                    {
                        state.copy_complete = false;
                        state.issues.push(format!("{path}: Native file is too large or cannot be inspected; it was not preserved."));
                        continue;
                    }
                    let path_key = path.clone();
                    match fs::read(entry.path()) {
                        Ok(bytes) => {
                            let sha256 = hash(&bytes);
                            if managed::fingerprint(entry.path()).ok().as_deref() != Some(&sha256) {
                                state.copy_complete = false;
                                state.issues.push(format!("{path}: Native file changed during intake. The snapshot is kept; import again to capture its current content."));
                            }
                            state.files.push(SdFile {
                                path,
                                sha256: sha256.clone(),
                                kind: "native".into(),
                                sample_id: None,
                            });
                            self.db.execute("INSERT OR IGNORE INTO sd_native_blobs(sha256,content) VALUES (?1,?2)", params![sha256, &bytes]).map_err(|e| e.to_string())?;
                            native_files.insert(path_key, bytes);
                        }
                        Err(error) => {
                            state.copy_complete = false;
                            state.issues.push(format!(
                                "{path}: Native content could not be preserved: {error}"
                            ));
                        }
                    }
                }
            } else {
                state.copy_complete = false;
                state
                    .issues
                    .push(format!("{path}: Unsupported source entry."));
            }
        }
        audio_paths.sort_by(|a, b| natord::compare(&a.0, &b.0));
        let audio = self.import_progress(
            audio_paths.iter().map(|(_, path)| path.clone()).collect(),
            managed::TrashedOriginal::Recognize,
            progress,
        )?;
        state.copy_complete &= audio.complete;
        let mut voice_files = VoiceFiles::new();
        for ((path, source), result) in audio_paths.iter().zip(&audio.results) {
            let file = SdFile {
                path: path.clone(),
                sha256: result
                    .copied_hash
                    .clone()
                    .unwrap_or_else(|| managed::fingerprint(source).unwrap_or_default()),
                kind: "audio".into(),
                sample_id: result.sample.as_ref().map(|s| s.id.clone()),
            };
            let parts: Vec<_> = path.split('/').collect();
            let assignment = (parts.len() == 2)
                .then(|| {
                    let kit = kit_paths.get(parts[0])?;
                    let voice = parts[1].as_bytes().first().copied()?;
                    (b'1'..=b'4')
                        .contains(&voice)
                        .then(|| (kit.clone(), usize::from(voice - b'1')))
                })
                .flatten();
            // The device numbers only WAV files; keep failed ones so a stored
            // Layer position never silently shifts onto a different file.
            let device_layer = assignment.clone().filter(|_| is_wav(path));
            if let Some(sample_id) = &file.sample_id {
                if managed::fingerprint(source).ok().as_deref() != Some(&file.sha256) {
                    state.copy_complete = false;
                    state.issues.push(format!("{path}: Source changed during intake. The copied Sample is kept; import again to capture the changed source."));
                }
                if let Some((kit, channel)) = assignment {
                    let voice =
                        &mut patch.kits.iter_mut().find(|k| k.id == kit).unwrap().voices[channel];
                    let id = Uuid::new_v4().to_string();
                    voice.layers.push(Layer {
                        id: id.clone(),
                        sample_id: sample_id.clone(),
                    });
                    if voice.selected_layer.is_empty() {
                        voice.selected_layer = id.clone();
                    }
                    if let Some(key) = device_layer {
                        voice_files
                            .entry(key)
                            .or_default()
                            .push((path.clone(), Some(id)));
                    }
                } else {
                    state.issues.push(format!("{path}: Invalid Kit folder or Voice prefix. Sample is kept in the Library without a guessed assignment."));
                    state.unassigned.push(file.clone());
                }
            } else {
                if let Some(key) = device_layer {
                    voice_files
                        .entry(key)
                        .or_default()
                        .push((path.clone(), None));
                }
                state.issues.push(format!(
                    "{path}: {}",
                    result.error.as_deref().unwrap_or("Audio import failed.")
                ));
            }
            state.files.push(file);
        }
        // Source-relative paths and exact bytes determine recognition. App-only
        // names, metadata, generated IDs and the mount location do not.
        state.files.sort_by(|a, b| a.path.cmp(&b.path));
        let content: Vec<_> = state
            .files
            .iter()
            .map(|f| (&f.path, &f.sha256, &f.kind))
            .collect();
        state.fingerprint =
            hash(&serde_json::to_vec(&(content, &state.issues)).map_err(|e| e.to_string())?);
        let graph = self.load_patches()?;
        state.native = Some(read_native(
            &mut patch,
            graph.default_layer_mode,
            has_save,
            &kit_paths,
            &native_files,
            &voice_files,
        ));
        if patch.kits.is_empty() {
            state.issues.push("No valid Bank/Kit folders were found. Create a Kit and explicitly assign the imported Samples.".into());
        }
        patch.selected_kit = patch
            .kits
            .first()
            .map(|kit| kit.id.clone())
            .unwrap_or_default();
        let fingerprint = state.fingerprint.clone();
        patch.imported_sd = Some(state);
        let recognized_patch = self.matching_sd(&fingerprint, &patch, &graph)?;
        let status = if recognized_patch.is_some() {
            "recognized"
        } else if graph.patches.iter().any(|p| p.imported_sd.is_some()) {
            "changed"
        } else {
            "new"
        };
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sd_intakes(id,fingerprint,draft) VALUES (?1,?2,?3)",
            params![
                id,
                fingerprint,
                serde_json::to_string(&patch).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(SdImportReview {
            id,
            status: status.into(),
            recognized_patch,
            candidates: graph
                .patches
                .iter()
                .map(|p| SdPatchCandidate {
                    id: p.id.clone(),
                    name: p.name.clone(),
                })
                .collect(),
            native_verified: patch
                .imported_sd
                .as_ref()
                .and_then(|sd| sd.native.as_ref())
                .is_some_and(NativeReading::complete),
            audio,
            draft: patch,
        })
    }
    fn finish_sd(&self, id: &str, choice: SdImportChoice) -> Result<(), String> {
        let (fingerprint, payload): (String, String) = self
            .db
            .query_row(
                "SELECT fingerprint,draft FROM sd_intakes WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "Import review no longer exists. Import the SD again.".to_string())?;
        let mut patch: Patch = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        if let Some(imported) = &patch.imported_sd {
            for file in &imported.files {
                if let Some(id) = &file.sample_id {
                    if !self.export_matches_sample(&file.sha256, id)? {
                        return Err(
                            "A Sample changed since this SD review. Import the SD again.".into(),
                        );
                    }
                }
            }
        }
        let mut graph = self.load_patches()?;
        if let Some(existing) = self.matching_sd(&fingerprint, &patch, &graph)? {
            // Exact recognition always reuses its existing Patch, preserving any
            // subsequent work performed there by the user.
            graph.selected_patch = existing;
        } else {
            match choice {
                SdImportChoice::Reuse => return Err("This arrangement changed. Choose Create new Patch or Update existing Patch explicitly.".into()),
                SdImportChoice::New { name } => {
                    if name.trim().is_empty() { return Err("Patch name cannot be empty.".into()); }
                    patch.name = name;
                    graph.selected_patch = patch.id.clone();
                    graph.patches.push(patch);
                }
                SdImportChoice::Update { patch: existing } => {
                    let old = graph.patches.iter_mut().find(|p| p.id == existing).ok_or("Patch no longer exists.")?;
                    patch.id = old.id.clone(); patch.name = old.name.clone();
                    patch.bpm = old.bpm; patch.voices = old.voices.clone();
                    // Keep musical memory and occurrence identity for unchanged
                    // positions at matching Kit addresses during explicit update.
                    // Verified native modes/selections come from the SD instead.
                    let native = patch.imported_sd.as_ref().and_then(|sd| sd.native.clone());
                    for kit in &mut patch.kits {
                        if let Some(previous) = old.kits.iter().find(|k| k.bank == kit.bank && k.number == kit.number) {
                            let restored = native.as_ref().and_then(|n| n.kit(&kit.bank, kit.number)).map(|k| k.voices.clone()).unwrap_or_default();
                            kit.id = previous.id.clone();
                            for (channel, (voice, previous)) in kit.voices.iter_mut().zip(&previous.voices).enumerate() {
                                let flags = restored.get(channel).cloned().unwrap_or_default();
                                if !flags.mode_restored {
                                    voice.mode = previous.mode;
                                }
                                let selected = voice.layers.iter().position(|l| l.id == voice.selected_layer);
                                let mut used = HashSet::new();
                                for layer in &mut voice.layers {
                                    if let Some(prior) = previous.layers.iter().find(|p| p.sample_id == layer.sample_id && !used.contains(&p.id)) {
                                        layer.id = prior.id.clone(); used.insert(prior.id.clone());
                                    }
                                }
                                voice.selected_layer = match selected.filter(|_| flags.selection_restored) {
                                    Some(index) => voice.layers[index].id.clone(),
                                    None if voice.layers.iter().any(|l| l.id == previous.selected_layer) => previous.selected_layer.clone(),
                                    None => voice.layers.first().map(|l| l.id.clone()).unwrap_or_default(),
                                };
                            }
                        }
                    }
                    patch.selected_kit = if patch.kits.iter().any(|k| k.id == old.selected_kit) {old.selected_kit.clone()} else {patch.kits.first().map(|k|k.id.clone()).unwrap_or_default()};
                    graph.selected_patch = patch.id.clone();
                    *old = patch;
                }
            }
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO patch_state(id,payload) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [serde_json::to_string(&graph).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE sd_intakes SET patch_id=NULL WHERE patch_id=?1 AND fingerprint<>?2",
            params![graph.selected_patch, fingerprint],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE sd_intakes SET patch_id=?2 WHERE id=?1",
            params![id, graph.selected_patch],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn native_blob(&self, intake: &str, path: &str) -> Result<Vec<u8>, String> {
        let payload: String = self
            .db
            .query_row(
                "SELECT draft FROM sd_intakes WHERE id=?1",
                [intake],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let patch: Patch = serde_json::from_str(&payload).map_err(|e| e.to_string())?;
        let imported = patch.imported_sd.ok_or("Import has no native state.")?;
        let file = imported
            .files
            .iter()
            .find(|f| f.path == path && f.kind == "native")
            .ok_or("Native file not found.")?;
        self.db
            .query_row(
                "SELECT content FROM sd_native_blobs WHERE sha256=?1",
                [&file.sha256],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
}
impl AppState {
    pub fn prepare_sd_import(&self, source: PathBuf) -> Result<SdImportReview, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .prepare_sd(&source)
    }
    pub fn finish_sd_import(
        &self,
        id: &str,
        choice: SdImportChoice,
    ) -> Result<PatchSnapshot, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .finish_sd(id, choice)?;
        self.patch_action(patches::PatchAction::Open)
    }
    pub fn sd_native_blob(&self, intake: &str, path: &str) -> Result<Vec<u8>, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .native_blob(intake, path)
    }
}
#[tauri::command]
pub async fn sd_native_blob(
    intake: String,
    path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<u8>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.sd_native_blob(&intake, &path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn prepare_sd_import(
    source: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SdImportReview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.prepare_sd_import(source))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn finish_sd_import(
    id: String,
    choice: SdImportChoice,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<PatchSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.finish_sd_import(&id, choice))
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use crate::tests::fixture;
    use crate::*;
    #[test]
    fn partial_copy_is_reported_and_failed_audio_changes_are_not_recognized_as_unchanged() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("A0")).unwrap();
        wav::write_pcm16(&source.join("A0/1_Valid.wav"), &[1000; 4410], 44100).unwrap();
        std::fs::write(source.join("A0/2_Broken.wav"), b"RIFFbroken").unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        assert!(!review.draft.imported_sd.as_ref().unwrap().copy_complete);
        state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New {
                    name: "Partial".into(),
                },
            )
            .unwrap();
        std::fs::write(source.join("A0/2_Broken.wav"), b"Different broken audio").unwrap();
        let changed = state.prepare_sd_import(source).unwrap();
        assert_eq!(changed.status, "changed");
    }
    #[test]
    fn invalid_assignments_keep_usable_audio_and_can_be_assigned_explicitly_after_restart() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        for folder in ["A0", "wrong", "_save"] {
            std::fs::create_dir_all(source.join(folder)).unwrap();
        }
        wav::write_pcm16(&source.join("A0/1_Valid.wav"), &[1000; 4410], 44100).unwrap();
        wav::write_pcm16(&source.join("A0/0_Unknown.wav"), &[2000; 4410], 44100).unwrap();
        wav::write_pcm16(&source.join("wrong/2_Sound.wav"), &[3000; 4410], 44100).unwrap();
        std::fs::write(source.join("A0/4_Broken.wav"), b"RIFFbroken").unwrap();
        std::fs::write(
            source.join("_save/unknown.future"),
            [0, 255, 13, 10, 0, 128],
        )
        .unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        assert_eq!(review.audio.imported, 3);
        assert_eq!(review.audio.failed, 1);
        assert_eq!(
            review.draft.imported_sd.as_ref().unwrap().unassigned.len(),
            2
        );
        let pending = review.id.clone();
        let unresolved: Vec<_> = review
            .draft
            .imported_sd
            .as_ref()
            .unwrap()
            .unassigned
            .iter()
            .map(|file| file.sample_id.clone().unwrap())
            .collect();
        drop(state);
        std::fs::remove_dir_all(&source).unwrap();
        let (state, _) = crate::tests::fixture_state(temp.path().join("support"));
        let saved = state
            .finish_sd_import(
                &pending,
                super::SdImportChoice::New {
                    name: "Draft".into(),
                },
            )
            .unwrap();
        assert_eq!(state.list_samples().unwrap().len(), 3);
        assert!(saved
            .validation
            .iter()
            .any(|issue| issue.contains("2 unassigned")));
        assert_eq!(
            state
                .sd_native_blob(&pending, "_save/unknown.future")
                .unwrap(),
            vec![0, 255, 13, 10, 0, 128]
        );
        for sample in unresolved {
            state
                .patch_action(patches::PatchAction::AddLayer { channel: 1, sample })
                .unwrap();
        }
        let repaired = state.patch_action(patches::PatchAction::Open).unwrap();
        assert!(!repaired
            .validation
            .iter()
            .any(|issue| issue.contains("unassigned")));
        assert_eq!(
            repaired.graph.current().unwrap().kits[0].voices[1]
                .layers
                .len(),
            2
        );
        assert!(repaired
            .graph
            .current()
            .unwrap()
            .imported_sd
            .as_ref()
            .unwrap()
            .issues
            .iter()
            .any(|issue| issue.contains("Unknown.wav")));
    }
    #[test]
    fn deleting_an_imported_patch_forgets_its_card_until_undo() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("A0")).unwrap();
        wav::write_pcm16(&source.join("A0/1_Kick.wav"), &[8192; 4410], 44100).unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        let imported = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New {
                    name: "Card".into(),
                },
            )
            .unwrap();
        let card = imported.graph.selected_patch.clone();
        let other = state
            .patch_action(patches::PatchAction::CreatePatch {
                name: "Other".into(),
            })
            .unwrap()
            .graph
            .selected_patch;
        let deleted = state
            .patch_action(patches::PatchAction::DeletePatch {
                patch: card.clone(),
            })
            .unwrap();
        assert!(deleted.graph.patches.iter().all(|p| p.id != card));
        // Deleting a Patch that is not open keeps the open one.
        assert_eq!(deleted.graph.selected_patch, other);
        assert_eq!(deleted.undo.as_deref(), Some("Patch Card"));
        // The card is no longer recognized.
        assert_eq!(
            state
                .prepare_sd_import(source.clone())
                .unwrap()
                .recognized_patch,
            None
        );
        let restored = state
            .patch_action(patches::PatchAction::UndoDelete)
            .unwrap();
        let patch = restored
            .graph
            .patches
            .iter()
            .find(|p| p.id == card)
            .unwrap();
        assert!(patch.imported_sd.is_some());
        assert_eq!(
            state.prepare_sd_import(source).unwrap().recognized_patch,
            Some(card)
        );
    }
    #[test]
    fn unrelated_directories_do_not_change_sd_recognition_but_empty_kits_do() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("A0")).unwrap();
        wav::write_pcm16(&source.join("A0/1_Kick.wav"), &[8192; 4410], 44100).unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        let imported = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New { name: "Set".into() },
            )
            .unwrap();
        std::fs::create_dir_all(source.join("firmware-notes/archive")).unwrap();
        std::fs::create_dir_all(source.join("A0/notes/empty")).unwrap();
        std::fs::write(source.join("firmware-notes/readme.txt"), b"unrelated").unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        assert_eq!(review.status, "recognized");
        assert_eq!(review.recognized_patch, Some(imported.graph.selected_patch));
        std::fs::create_dir_all(source.join("B12")).unwrap();
        let changed = state.prepare_sd_import(source).unwrap();
        assert_eq!(changed.status, "changed");
        assert!(changed
            .draft
            .kits
            .iter()
            .any(|kit| kit.bank == "B" && kit.number == 12));
    }
    #[test]
    fn changed_sd_requires_explicit_choice_and_old_state_is_not_mistaken_for_updated_patch() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("A0")).unwrap();
        std::fs::create_dir_all(source.join("_save")).unwrap();
        wav::write_pcm16(&source.join("A0/1_Kick.wav"), &[8192; 4410], 44100).unwrap();
        std::fs::write(source.join("_save/A0.rpl"), [0, 255]).unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        let first = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New { name: "Set".into() },
            )
            .unwrap();
        let patch_id = first.graph.selected_patch.clone();
        state
            .patch_action(patches::PatchAction::Musical {
                action: Action::SetBpm { value: 137. },
            })
            .unwrap();
        state
            .patch_action(patches::PatchAction::Musical {
                action: Action::ToggleStep {
                    channel: 0,
                    step: 3,
                },
            })
            .unwrap();
        let layer_id = first.graph.current().unwrap().kits[0].voices[0].layers[0]
            .id
            .clone();
        let relocated = temp.path().join("Different mount");
        std::fs::rename(&source, &relocated).unwrap();
        let same = state.prepare_sd_import(relocated.clone()).unwrap();
        assert_eq!(same.status, "recognized");
        assert_eq!(same.audio.recognized, 1);
        assert_eq!(same.recognized_patch.as_deref(), Some(patch_id.as_str()));
        std::fs::write(relocated.join(".DS_Store"), b"Finder metadata").unwrap();
        assert_eq!(
            state.prepare_sd_import(relocated.clone()).unwrap().status,
            "recognized"
        );
        std::fs::write(relocated.join("_save/A0.rpl"), [0, 254]).unwrap();
        let changed = state.prepare_sd_import(relocated.clone()).unwrap();
        assert_eq!(changed.status, "changed");
        assert!(state
            .finish_sd_import(&changed.id, super::SdImportChoice::Reuse)
            .is_err());
        assert_eq!(
            state
                .patch_action(patches::PatchAction::Open)
                .unwrap()
                .graph
                .patches
                .len(),
            1
        );
        let updated = state
            .finish_sd_import(
                &changed.id,
                super::SdImportChoice::Update {
                    patch: patch_id.clone(),
                },
            )
            .unwrap();
        assert_eq!(updated.graph.selected_patch, patch_id);
        assert_eq!(updated.session.as_ref().unwrap().bpm, 137.);
        assert!(updated.session.as_ref().unwrap().voices[0].steps[3]);
        assert_eq!(
            updated.graph.current().unwrap().kits[0].voices[0].layers[0].id,
            layer_id
        );
        std::fs::write(relocated.join("_save/A0.rpl"), [0, 255]).unwrap();
        let older = state.prepare_sd_import(relocated).unwrap();
        assert_eq!(older.status, "changed");
        assert_eq!(older.recognized_patch, None);
        let new = state
            .finish_sd_import(
                &older.id,
                super::SdImportChoice::New {
                    name: "Older".into(),
                },
            )
            .unwrap();
        assert_eq!(new.graph.patches.len(), 2);
    }
    #[test]
    fn sd_layers_follow_numeric_filename_order_without_truncation() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("B12")).unwrap();
        for n in 1..=13 {
            wav::write_pcm16(
                &source.join(format!("B12/1_{n}.wav")),
                &[n * 100; 4410],
                44100,
            )
            .unwrap();
        }
        let review = state.prepare_sd_import(source).unwrap();
        let layers = &review.draft.kits[0].voices[0].layers;
        let names: Vec<_> = layers
            .iter()
            .map(|l| {
                review
                    .audio
                    .samples
                    .iter()
                    .find(|s| s.id == l.sample_id)
                    .unwrap()
                    .name
                    .as_str()
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "1_1", "1_2", "1_3", "1_4", "1_5", "1_6", "1_7", "1_8", "1_9", "1_10", "1_11",
                "1_12", "1_13"
            ]
        );
        let saved = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New { name: "Set".into() },
            )
            .unwrap();
        assert!(saved
            .validation
            .iter()
            .any(|issue| issue.contains("13 Layers")));
        assert_eq!(saved.graph.current().unwrap().kits[0].bank, "B");
        assert_eq!(saved.graph.current().unwrap().kits[0].number, 12);
    }
    #[test]
    fn sd_intake_reuses_samples_and_reopens_without_source() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("A0")).unwrap();
        std::fs::create_dir_all(source.join("_save")).unwrap();
        wav::write_pcm16(&source.join("A0/1_Kick.wav"), &[8192; 4410], 44100).unwrap();
        std::fs::write(source.join("_save/A0.rpl"), [0, 255, 6, 9]).unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        assert_eq!(review.status, "new");
        assert_eq!(review.audio.imported, 1);
        std::fs::remove_dir_all(&source).unwrap();
        let patch = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New { name: "Set".into() },
            )
            .unwrap();
        assert_eq!(
            patch.graph.current().unwrap().kits[0].voices[0]
                .layers
                .len(),
            1
        );
        assert!(patch
            .validation
            .iter()
            .any(|issue| issue.contains("_save/A0.rpl could not be interpreted")));
        assert_eq!(
            state.sd_native_blob(&review.id, "_save/A0.rpl").unwrap(),
            vec![0, 255, 6, 9]
        );
    }
    #[test]
    fn unrelated_firmware_is_not_copied_into_native_state() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        std::fs::create_dir_all(source.join("_save")).unwrap();
        std::fs::write(source.join("firmware.bin"), b"firmware").unwrap();
        std::fs::write(source.join("_save/future.rpl"), b"native").unwrap();
        let review = state.prepare_sd_import(source).unwrap();
        let files = &review.draft.imported_sd.as_ref().unwrap().files;
        assert!(!files.iter().any(|file| file.path == "firmware.bin"));
        assert_eq!(
            state
                .sd_native_blob(&review.id, "_save/future.rpl")
                .unwrap(),
            b"native"
        );
    }
    fn wavs(source: &std::path::Path, kit: &str, names: &[&str]) {
        std::fs::create_dir_all(source.join(kit)).unwrap();
        for (i, name) in names.iter().enumerate() {
            let level = 1000 + 500 * i as i16 + 97 * kit.len() as i16;
            wav::write_pcm16(&source.join(kit).join(name), &[level; 4410], 44100).unwrap();
        }
    }
    #[test]
    fn verified_native_modes_and_selected_layers_restore_into_imported_kits() {
        use rample_core::{model::LayerMode, native::fixtures};
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        wavs(
            &source,
            "A0",
            &[
                "1_a.wav", "1_b.wav", "1_c.wav", "1_d.wav", "1_e.wav", "2_a.wav", "2_b.wav",
                "3_a.wav",
            ],
        );
        wavs(&source, "A1", &["1_kick.wav", "2_snare.wav"]);
        std::fs::create_dir_all(source.join("_save")).unwrap();
        let kit_file = fixtures::kit([0, 2, 4, 1], [3, 1, 0, 0]);
        std::fs::write(source.join("_save/A0.rpl"), &kit_file).unwrap();
        std::fs::write(source.join("_save/settings.rpl"), fixtures::settings(1)).unwrap();
        std::fs::write(source.join("_save/autosave_A0.rpl"), []).unwrap();
        // The device's global LAYER (RANDOM in settings.rpl) is ignored: Kits
        // never saved with STORE use the user's own default.
        state
            .patch_action(patches::PatchAction::SetDefaultMode {
                mode: LayerMode::Velocity,
            })
            .unwrap();
        let review = state.prepare_sd_import(source.clone()).unwrap();
        assert!(review.native_verified);
        let native = review
            .draft
            .imported_sd
            .as_ref()
            .unwrap()
            .native
            .clone()
            .unwrap();
        assert_eq!(native.default_layer_mode, LayerMode::Velocity);
        let sources: Vec<_> = native.kits.iter().map(|k| k.source).collect();
        assert_eq!(
            sources,
            [super::NativeSource::Store, super::NativeSource::AppDefault]
        );
        let saved = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New {
                    name: "Device".into(),
                },
            )
            .unwrap();
        let patch = saved.graph.current().unwrap();
        let (a0, a1) = (&patch.kits[0], &patch.kits[1]);
        let modes: Vec<_> = a0.voices.iter().map(|v| v.mode).collect();
        assert_eq!(
            modes,
            [
                LayerMode::Single,
                LayerMode::Sequential,
                LayerMode::Velocity,
                LayerMode::Random
            ]
        );
        // Zero-based positions over the Voice's sorted WAV files.
        assert_eq!(a0.voices[0].selected_layer, a0.voices[0].layers[3].id);
        assert_eq!(a0.voices[1].selected_layer, a0.voices[1].layers[1].id);
        assert!(a1.voices.iter().all(|v| v.mode == LayerMode::Velocity));
        // Read-only native settings leave nothing pending in validation.
        assert!(!saved.validation.iter().any(|v| v.starts_with("Device:")));
        assert!(saved
            .session
            .as_ref()
            .is_some_and(|s| s.voices[1].layer_mode == LayerMode::Sequential));
        assert_eq!(
            state.sd_native_blob(&review.id, "_save/A0.rpl").unwrap(),
            kit_file
        );

        // An explicit update takes verified device values over app memory and
        // keeps occurrence identity for unchanged positions.
        let layer_ids: Vec<_> = a0.voices[0].layers.iter().map(|l| l.id.clone()).collect();
        let patch_id = patch.id.clone();
        std::fs::write(
            source.join("_save/A0.rpl"),
            fixtures::kit([3, 0, 0, 0], [1, 0, 0, 0]),
        )
        .unwrap();
        let changed = state.prepare_sd_import(source).unwrap();
        assert_eq!(changed.status, "changed");
        let updated = state
            .finish_sd_import(
                &changed.id,
                super::SdImportChoice::Update { patch: patch_id },
            )
            .unwrap();
        let a0 = &updated.graph.current().unwrap().kits[0];
        assert_eq!(a0.voices[0].mode, LayerMode::Reverse);
        assert_eq!(a0.voices[1].mode, LayerMode::Single);
        assert_eq!(a0.voices[0].selected_layer, layer_ids[1]);
        let ids: Vec<_> = a0.voices[0].layers.iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids, layer_ids);
        // New Kits start in the user's default Layer mode too.
        let created = state
            .patch_action(patches::PatchAction::CreateKit {
                bank: "C".into(),
                number: 5,
            })
            .unwrap();
        let kit = created.graph.current().unwrap().kit().unwrap();
        assert!(kit.voices.iter().all(|v| v.mode == LayerMode::Velocity));
    }
    #[test]
    fn unverified_or_unreadable_native_settings_are_reported_and_preserved() {
        use rample_core::{model::LayerMode, native::fixtures};
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        wavs(&source, "A0", &["1_a.wav", "1_c.wav", "2_a.wav", "2_c.wav"]);
        std::fs::write(source.join("A0/2_b.wav"), b"RIFFbroken").unwrap();
        wavs(&source, "A1", &["1_kick.wav"]);
        wavs(&source, "A2", &["1_kick.wav"]);
        std::fs::create_dir_all(source.join("_save")).unwrap();
        // Voice 1: unknown mode and a position past its two files.
        // Voice 2: position 2 is the WAV that failed to import.
        std::fs::write(
            source.join("_save/A0.rpl"),
            fixtures::kit([7, 2, 0, 0], [5, 1, 0, 0]),
        )
        .unwrap();
        let mut truncated = fixtures::kit([1; 4], [0; 4]);
        truncated.truncate(100);
        std::fs::write(source.join("_save/A1.rpl"), &truncated).unwrap();
        std::fs::write(source.join("_save/settings.rpl"), fixtures::settings(3)).unwrap();
        let review = state.prepare_sd_import(source).unwrap();
        assert!(!review.native_verified);
        let imported = review.draft.imported_sd.as_ref().unwrap();
        let native = imported.native.as_ref().unwrap();
        let issues: Vec<_> = native.kits.iter().flat_map(|k| k.issues.clone()).collect();
        assert!(issues
            .iter()
            .any(|i| i.contains("A0 Voice 1: Stored Layer mode value 7")));
        assert!(issues
            .iter()
            .any(|i| i.contains("A0 Voice 1: Stored selected Layer 6 does not exist among its 2")));
        assert!(issues.iter().any(|i| i
            .contains("A0 Voice 2: Stored selected Layer 2 (A0/2_b.wav) could not be imported")));
        assert!(issues
            .iter()
            .any(|i| i.contains("_save/A1.rpl could not be interpreted (file ends")));
        assert!(!issues.iter().any(|i| i.contains("A2")));
        let a0 = &review.draft.kits[0];
        // Unknown stored value: the user's default (MANUAL unless changed).
        assert_eq!(a0.voices[0].mode, LayerMode::Single);
        assert_eq!(a0.voices[0].selected_layer, a0.voices[0].layers[0].id);
        assert_eq!(a0.voices[1].mode, LayerMode::Sequential);
        assert_eq!(a0.voices[1].selected_layer, a0.voices[1].layers[0].id);
        assert_eq!(native.kits[1].source, super::NativeSource::Unreadable);
        assert_eq!(
            state.sd_native_blob(&review.id, "_save/A1.rpl").unwrap(),
            truncated
        );
        let saved = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New {
                    name: "Partial".into(),
                },
            )
            .unwrap();
        assert!(saved
            .validation
            .iter()
            .any(|v| v.contains("Partial: A0 Voice 1: Stored Layer mode value 7")));
    }
    #[test]
    fn kits_without_a_stored_file_in_save_use_the_default_layer_mode_silently() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        wavs(&source, "B3", &["1_kick.wav"]);
        std::fs::create_dir_all(source.join("_save")).unwrap();
        let review = state.prepare_sd_import(source).unwrap();
        let native = review.draft.imported_sd.unwrap().native.unwrap();
        assert!(review.native_verified);
        assert_eq!(native.kits[0].source, super::NativeSource::AppDefault);
        assert!(native.kits[0].issues.is_empty());
        assert!(native.issues.is_empty());
    }
    #[test]
    fn a_source_without_save_reports_that_no_native_settings_were_found() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        wavs(&source, "B3", &["1_kick.wav"]);
        let review = state.prepare_sd_import(source).unwrap();
        assert!(!review.native_verified);
        let native = review
            .draft
            .imported_sd
            .as_ref()
            .unwrap()
            .native
            .clone()
            .unwrap();
        assert_eq!(native.kits[0].source, super::NativeSource::AppDefault);
        assert_eq!(native.issues.len(), 1, "{:?}", native.issues);
        assert!(native.issues[0].starts_with("No _save folder was found"));
        let saved = state
            .finish_sd_import(
                &review.id,
                super::SdImportChoice::New {
                    name: "Copy".into(),
                },
            )
            .unwrap();
        assert!(saved
            .validation
            .iter()
            .any(|v| v.starts_with("Copy: No _save folder was found")));
    }
    #[test]
    fn native_sources_keep_their_saved_names() {
        use super::NativeSource::*;
        for (source, name) in [
            (Store, "store"),
            (AppDefault, "appDefault"),
            (Unreadable, "unreadable"),
        ] {
            assert_eq!(serde_json::to_value(source).unwrap(), name);
            assert_eq!(
                serde_json::from_value::<super::NativeSource>(name.into()).unwrap(),
                source
            );
        }
    }
    #[test]
    fn sd_imported_before_native_reading_offers_update_with_stored_modes() {
        use rample_core::{model::LayerMode, native::fixtures};
        let (temp, state, _) = fixture();
        let source = temp.path().join("SD");
        wavs(&source, "A0", &["1_a.wav", "1_b.wav", "1_c.wav"]);
        std::fs::create_dir_all(source.join("_save")).unwrap();
        std::fs::write(
            source.join("_save/A0.rpl"),
            fixtures::kit([2, 0, 0, 0], [2, 0, 0, 0]),
        )
        .unwrap();
        let first = state.prepare_sd_import(source.clone()).unwrap();
        let saved = state
            .finish_sd_import(&first.id, super::SdImportChoice::New { name: "Old".into() })
            .unwrap();
        let patch_id = saved.graph.selected_patch.clone();
        // Simulate a Patch saved by a build without native reading, with the
        // user's own changes to keep.
        {
            let store = state.managed.lock().unwrap();
            let mut graph = store.load_patches().unwrap();
            let patch = graph.patches.iter_mut().find(|p| p.id == patch_id).unwrap();
            patch.imported_sd.as_mut().unwrap().native = None;
            patch.bpm = 97.;
            let voice = &mut patch.kits[0].voices[0];
            voice.mode = LayerMode::Single;
            voice.selected_layer = voice.layers[0].id.clone();
            store.save_patches(&graph).unwrap();
        }
        let reopened = state.patch_action(patches::PatchAction::Open).unwrap();
        assert!(reopened
            .validation
            .iter()
            .any(|v| v.contains("choose Update existing Patch")));
        let again = state.prepare_sd_import(source).unwrap();
        assert_eq!(again.status, "changed");
        assert_eq!(again.recognized_patch, None);
        let updated = state
            .finish_sd_import(&again.id, super::SdImportChoice::Update { patch: patch_id })
            .unwrap();
        let patch = updated.graph.current().unwrap();
        let voice = &patch.kits[0].voices[0];
        assert_eq!(voice.mode, LayerMode::Sequential);
        assert_eq!(voice.selected_layer, voice.layers[2].id);
        assert_eq!(patch.bpm, 97.);
        assert!(!updated
            .validation
            .iter()
            .any(|v| v.contains("were not read")));
        // Once updated, the same SD is recognized again.
        let source = temp.path().join("SD");
        assert_eq!(
            state.prepare_sd_import(source).unwrap().status,
            "recognized"
        );
    }
}
