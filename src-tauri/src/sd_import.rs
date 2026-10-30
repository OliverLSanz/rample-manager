//! SD intake snapshots the source once. Pending reviews and opaque native content
//! live in the managed database; neither committing nor previewing revisits the SD.
use crate::{
    managed::{ImportReply, ManagedLibrary},
    patches::{Layer, Patch, PatchKit, PatchSnapshot, PatchVoice},
    *,
};
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
fn address(name: &str) -> Option<(String, u8)> {
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
impl ManagedLibrary {
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
        };
        let mut audio_paths = vec![];
        let mut kit_paths = HashMap::new();
        for entry in walkdir::WalkDir::new(source)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".DS_Store"
                        | ".Spotlight-V100"
                        | ".Trashes"
                        | ".fseventsd"
                        | "System Volume Information"
                        | "$RECYCLE.BIN"
                ) && !name.starts_with("._")
            })
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
                            self.db.execute("INSERT OR IGNORE INTO sd_native_blobs(sha256,content) VALUES (?1,?2)", params![sha256, bytes]).map_err(|e| e.to_string())?;
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
            progress,
        )?;
        state.copy_complete &= audio.complete;
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
            if let Some(sample_id) = &file.sample_id {
                if managed::fingerprint(source).ok().as_deref() != Some(&file.sha256) {
                    state.copy_complete = false;
                    state.issues.push(format!("{path}: Source changed during intake. The copied Sample is kept; import again to capture the changed source."));
                }
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
                if let Some((kit, channel)) = assignment {
                    let voice =
                        &mut patch.kits.iter_mut().find(|k| k.id == kit).unwrap().voices[channel];
                    let id = Uuid::new_v4().to_string();
                    voice.layers.push(Layer {
                        id: id.clone(),
                        sample_id: sample_id.clone(),
                    });
                    if voice.selected_layer.is_empty() {
                        voice.selected_layer = id;
                    }
                } else {
                    state.issues.push(format!("{path}: Invalid Kit folder or Voice prefix. Sample is kept in the Library without a guessed assignment."));
                    state.unassigned.push(file.clone());
                }
            } else {
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
        let graph = self.load_patches()?;
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
            audio,
            draft: patch,
            native_verified: false,
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
                    for kit in &mut patch.kits {
                        if let Some(previous) = old.kits.iter().find(|k| k.bank == kit.bank && k.number == kit.number) {
                            kit.id = previous.id.clone();
                            for (voice, previous) in kit.voices.iter_mut().zip(&previous.voices) {
                                voice.mode = previous.mode;
                                let mut used = HashSet::new();
                                for layer in &mut voice.layers {
                                    if let Some(prior) = previous.layers.iter().find(|p| p.sample_id == layer.sample_id && !used.contains(&p.id)) {
                                        layer.id = prior.id.clone(); used.insert(prior.id.clone());
                                    }
                                }
                                voice.selected_layer = if voice.layers.iter().any(|l| l.id == previous.selected_layer) { previous.selected_layer.clone() } else {voice.layers.first().map(|l| l.id.clone()).unwrap_or_default()};
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
        state
            .act(Action::Root {
                path: source.to_string_lossy().into_owned(),
            })
            .unwrap();
        assert!(state.snapshot().watching);
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
        assert!(!state.snapshot().watching);
        assert!(patch
            .validation
            .iter()
            .any(|issue| issue.contains("unverified")));
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
}
