//! Application-owned Library commands. Paths stay private; callers use stable IDs.
use crate::*;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySample {
    pub id: String,
    pub original_id: String,
    pub name: String,
    pub sha256: String,
    pub duration: f64,
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub readable: bool,
    pub error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub source: String,
    pub status: String,
    pub sample: Option<LibrarySample>,
    pub error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReply {
    pub imported: usize,
    pub recognized: usize,
    pub failed: usize,
    pub complete: bool,
    pub results: Vec<ImportResult>,
    pub samples: Vec<LibrarySample>,
}

#[derive(Default)]
pub(super) struct AuditionSession {
    owner: String,
    generation: u64,
}

pub struct ManagedLibrary {
    root: PathBuf,
    db: Connection,
}
impl ManagedLibrary {
    pub fn open(support: &Path) -> Result<Self, String> {
        let root = support.join("Library");
        fs::create_dir_all(root.join("Originals")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("Incoming")).map_err(|e| e.to_string())?;
        let db = Connection::open(root.join("library.sqlite3")).map_err(|e| e.to_string())?;
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if version > 1 {
            return Err("This Library was created by a newer application. Update Rample Preview before opening it.".into());
        }
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .map_err(|e| e.to_string())?;
        if version == 0 {
            db.execute_batch(
                "BEGIN IMMEDIATE;
                CREATE TABLE originals (
                    id TEXT PRIMARY KEY, sha256 TEXT NOT NULL UNIQUE,
                    rate INTEGER NOT NULL, channels INTEGER NOT NULL, bits INTEGER NOT NULL,
                    frames INTEGER NOT NULL);
                CREATE TABLE samples (
                    id TEXT PRIMARY KEY, original_id TEXT NOT NULL REFERENCES originals(id),
                    name TEXT NOT NULL);
                CREATE INDEX samples_original ON samples(original_id);
                PRAGMA user_version=1;
                COMMIT;",
            )
            .map_err(|e| e.to_string())?;
        }
        // A single desktop instance owns this store. Staged copies never have published
        // references, so abandoned tempfile artifacts can be discarded on restart.
        for entry in fs::read_dir(root.join("Incoming")).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name().to_string_lossy().starts_with(".tmp")
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(Self { root, db })
    }
    fn original_path(&self, id: &str) -> PathBuf {
        self.root.join("Originals").join(format!("{id}.wav"))
    }
    pub fn list(&self) -> Result<Vec<LibrarySample>, String> {
        let mut query = self
            .db
            .prepare(
                "SELECT s.id, s.original_id, s.name, o.sha256,
            o.rate, o.channels, o.bits, o.frames FROM samples s JOIN originals o
            ON s.original_id=o.id ORDER BY s.rowid",
            )
            .map_err(|e| e.to_string())?;
        let rows = query.query_map([], sample_row).map_err(|e| e.to_string())?;
        let mut samples = vec![];
        for row in rows {
            let mut sample = row.map_err(|e| e.to_string())?;
            if let Err(error) = wav::inspect(&self.original_path(&sample.original_id)) {
                sample.readable = false;
                sample.error = Some(format!("Original is unavailable: {error}"));
            }
            samples.push(sample);
        }
        Ok(samples)
    }
    fn find_sample(&self, predicate: &str, value: &str) -> Result<Option<LibrarySample>, String> {
        // Predicate is one of the two fixed indexed lookups below, never external input.
        self.db
            .query_row(
                &format!(
                    "SELECT s.id, s.original_id, s.name, o.sha256,
            o.rate, o.channels, o.bits, o.frames FROM originals o JOIN samples s
            ON s.original_id=o.id WHERE {predicate} ORDER BY s.rowid LIMIT 1"
                ),
                [value],
                sample_row,
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn prepare(&self, id: &str) -> Result<Arc<PreparedSample>, String> {
        let sample = self
            .find_sample("s.id=?1", id)?
            .ok_or("Sample no longer exists")?;
        let path = self.original_path(&sample.original_id);
        if fingerprint(&path)? != sample.sha256 {
            return Err(
                "Original changed in managed storage. Restore the Library from a backup.".into(),
            );
        }
        PreparedSample::load(&path, &sample.sha256)
    }
    fn import_file(&mut self, source: &Path) -> Result<(LibrarySample, bool), String> {
        if !fs::metadata(source).map_err(|e| e.to_string())?.is_file() {
            return Err("Choose a regular mono PCM WAV file".into());
        }
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        if !input.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Choose a regular mono PCM WAV file".into());
        }
        // Stage and hash in bounded chunks. Never decode a whole file or publish a partial copy.
        let mut staged = tempfile::NamedTempFile::new_in(self.root.join("Incoming"))
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut chunk = [0; 65536];
        loop {
            let read = input.read(&mut chunk).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            staged
                .write_all(&chunk[..read])
                .map_err(|e| e.to_string())?;
            hash.update(&chunk[..read]);
        }
        staged.as_file().sync_all().map_err(|e| e.to_string())?;
        let sha256 = format!("{:x}", hash.finalize());
        let spec = validate_wav(staged.path())?;
        if spec.channels != 1 || spec.code != 1 || spec.frames == 0 {
            return Err("Import samples currently supports nonempty mono PCM WAV files.".into());
        }
        if fingerprint(staged.path())? != sha256 {
            return Err("Could not verify the copied Original".into());
        }
        if fingerprint(source)? != sha256 {
            return Err("Source changed during import. Import the file again.".into());
        }
        if let Some(sample) = self.find_sample("o.sha256=?1", &sha256)? {
            self.prepare(&sample.id)?;
            return Ok((sample, false));
        }
        let original_id = Uuid::new_v4().to_string();
        let sample_id = Uuid::new_v4().to_string();
        let name = source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let target = self.original_path(&original_id);
        let original = staged
            .persist_noclobber(&target)
            .map_err(|e| e.to_string())?;
        let mut permissions = original
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions();
        permissions.set_readonly(true);
        original
            .set_permissions(permissions)
            .map_err(|e| e.to_string())?;
        original.sync_all().map_err(|e| e.to_string())?;
        #[cfg(unix)]
        File::open(self.root.join("Originals"))
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        let transaction = self.db.transaction().map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO originals VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    original_id,
                    sha256,
                    spec.rate,
                    spec.channels,
                    spec.bits,
                    i64::try_from(spec.frames).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO samples VALUES (?1,?2,?3)",
                params![sample_id, original_id, name],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok((
            LibrarySample {
                id: sample_id,
                original_id,
                name,
                sha256,
                duration: spec.frames as f64 / f64::from(spec.rate),
                rate: spec.rate,
                channels: spec.channels,
                bits: spec.bits,
                readable: true,
                error: None,
            },
            true,
        ))
    }
    pub fn import(&mut self, paths: Vec<PathBuf>) -> Result<ImportReply, String> {
        let mut inputs = vec![];
        for path in paths {
            if !path.is_dir() {
                inputs.push((path, None));
                continue;
            }
            let before = inputs.len();
            for entry in walkdir::WalkDir::new(&path).sort_by_file_name() {
                match entry {
                    Ok(entry)
                        if entry
                            .path()
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
                            && !entry.file_type().is_dir() =>
                    {
                        inputs.push((entry.into_path(), None))
                    }
                    Ok(_) => {}
                    Err(error) => inputs.push((
                        error.path().unwrap_or(&path).to_path_buf(),
                        Some(error.to_string()),
                    )),
                }
            }
            if inputs.len() == before {
                inputs.push((path, Some("No WAV files found in this folder.".into())));
            }
        }
        let mut results = vec![];
        for (source, prior_error) in inputs {
            let outcome = match prior_error {
                Some(error) => Err(error),
                None => self.import_file(&source),
            };
            let (sample, status, error) = match outcome {
                Ok((sample, added)) => (
                    Some(sample),
                    if added { "imported" } else { "recognized" },
                    None,
                ),
                Err(error) => (None, "failed", Some(error)),
            };
            results.push(ImportResult {
                source: source.to_string_lossy().into_owned(),
                status: status.into(),
                sample,
                error,
            });
        }
        let imported = results.iter().filter(|r| r.status == "imported").count();
        let recognized = results.iter().filter(|r| r.status == "recognized").count();
        let failed = results.iter().filter(|r| r.status == "failed").count();
        Ok(ImportReply {
            imported,
            recognized,
            failed,
            complete: failed == 0,
            results,
            samples: self.list()?,
        })
    }
}
fn validate_wav(path: &Path) -> Result<wav::WavSpec, String> {
    let spec = wav::inspect(path)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0; 12];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    let end = u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) + 8;
    if end > file.metadata().map_err(|e| e.to_string())?.len() || end < 12 {
        return Err("Truncated or invalid WAV container".into());
    }
    // inspect() locates the audio. Also validate all remaining chunks so truncated
    // metadata after the data chunk is not accepted as a complete Original.
    let mut position = 12;
    while position < end {
        if position + 8 > end {
            return Err("Truncated WAV chunk".into());
        }
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk).map_err(|e| e.to_string())?;
        let size = u64::from(u32::from_le_bytes(chunk[4..].try_into().unwrap()));
        position += 8 + size + size % 2;
        if position > end {
            return Err("Truncated WAV chunk".into());
        }
        file.seek(SeekFrom::Start(position))
            .map_err(|e| e.to_string())?;
    }
    Ok(spec)
}
fn sample_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibrarySample> {
    let rate: u32 = row.get(4)?;
    Ok(LibrarySample {
        id: row.get(0)?,
        original_id: row.get(1)?,
        name: row.get(2)?,
        sha256: row.get(3)?,
        rate,
        channels: row.get(5)?,
        bits: row.get(6)?,
        duration: row.get::<_, i64>(7)? as f64 / f64::from(rate),
        readable: true,
        error: None,
    })
}
fn fingerprint(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut chunk = [0; 65536];
    loop {
        let read = file.read(&mut chunk).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
// Resolve symlinks before allowing either legacy destructive entry point.
pub(super) fn guard_legacy_path(support: &Path, path: &Path, folder: bool) -> Result<(), String> {
    let managed = dunce::canonicalize(support.join("Library")).map_err(|e| e.to_string())?;
    let source = dunce::canonicalize(path).map_err(|e| e.to_string())?;
    if source.starts_with(&managed) || (folder && managed.starts_with(&source)) {
        return Err(
            "This folder workflow cannot modify managed Originals. Use Library samples instead."
                .into(),
        );
    }
    Ok(())
}
impl AppState {
    pub fn list_samples(&self) -> Result<Vec<LibrarySample>, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .list()
    }
    pub fn import_samples(&self, paths: Vec<PathBuf>) -> Result<ImportReply, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .import(paths)
    }
    pub fn open_sample_audition(&self) -> Result<String, String> {
        let mut current = self
            .original_audition
            .lock()
            .map_err(|_| "Audition is unavailable")?;
        current.owner = Uuid::new_v4().to_string();
        current.generation += 1;
        self.audio.send(Command::OriginalAudition(None))?;
        Ok(current.owner.clone())
    }
    pub fn stop_sample_audition(&self, owner: String, closing: bool) -> Result<bool, String> {
        let mut current = self
            .original_audition
            .lock()
            .map_err(|_| "Audition is unavailable")?;
        if current.owner != owner {
            return Ok(false);
        }
        current.generation += 1;
        if closing {
            current.owner.clear();
        }
        self.audio.send(Command::OriginalAudition(None))?;
        Ok(true)
    }
    pub fn audition_sample(&self, owner: String, id: String) -> Result<(), String> {
        let generation = {
            let mut current = self
                .original_audition
                .lock()
                .map_err(|_| "Audition is unavailable")?;
            if current.owner != owner {
                return Err("This Library audition has closed.".into());
            }
            current.generation += 1;
            current.generation
        };
        let prepared = self
            .managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .prepare(&id)?;
        let current = self
            .original_audition
            .lock()
            .map_err(|_| "Audition is unavailable")?;
        if current.owner == owner && current.generation == generation {
            self.audio.send(Command::OriginalAudition(Some(prepared)))?;
        }
        Ok(())
    }
}

#[tauri::command]
pub(super) async fn list_samples(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<LibrarySample>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.list_samples())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn import_samples(
    paths: Vec<PathBuf>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ImportReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.import_samples(paths))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn audition_sample(
    id: String,
    session: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.audition_sample(session, id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) fn stop_sample_audition(
    session: String,
    closing: bool,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    state.stop_sample_audition(session, closing)
}
#[tauri::command]
pub(super) fn open_sample_audition(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    state.open_sample_audition()
}

#[cfg(test)]
mod tests {
    use crate::{tests::fixture, Action, Library};
    use rample_core::wav;
    use std::path::Path;

    #[test]
    fn import_rejects_truncated_metadata_after_audio_and_preserves_valid_metadata_exactly() {
        let (temp, state, _) = fixture();
        let valid = temp.path().join("With metadata.wav");
        wav::write_pcm16(&valid, &[8192; 4410], 44100).unwrap();
        let mut bytes = std::fs::read(&valid).unwrap();
        bytes.extend_from_slice(b"LIST\x04\x00\x00\x00INFO");
        let size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&size.to_le_bytes());
        std::fs::write(&valid, &bytes).unwrap();
        let broken = temp.path().join("Truncated metadata.wav");
        let mut damaged = bytes.clone();
        damaged.pop();
        std::fs::write(&broken, &damaged).unwrap();
        let reply = state
            .import_samples(vec![valid.clone(), broken.clone()])
            .unwrap();
        assert_eq!((reply.imported, reply.failed), (1, 1));
        assert_eq!(std::fs::read(broken).unwrap(), damaged);
        let sample = &reply.samples[0];
        let prepared = state.managed.lock().unwrap().prepare(&sample.id).unwrap();
        assert_eq!(std::fs::read(&prepared.path).unwrap(), bytes);
        assert_eq!(std::fs::read(valid).unwrap(), bytes);
    }

    #[test]
    fn a_closed_library_audition_cannot_replace_or_stop_a_newer_session() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let id = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0)
            .id;
        let previous = state.open_sample_audition().unwrap();
        let current = state.open_sample_audition().unwrap();
        assert_ne!(previous, current);
        assert!(state
            .audition_sample(previous.clone(), id.clone())
            .unwrap_err()
            .contains("closed"));
        assert!(!state.stop_sample_audition(previous, true).unwrap());
        state.audition_sample(current.clone(), id).unwrap();
        assert!(state.stop_sample_audition(current.clone(), true).unwrap());
        assert!(state
            .audition_sample(current, "ignored".into())
            .unwrap_err()
            .contains("closed"));
    }

    #[test]
    fn restart_discards_an_interrupted_copy_and_keeps_completed_imports() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Complete.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state
            .import_samples(vec![source.clone()])
            .unwrap()
            .samples
            .remove(0);
        let support = temp.path().join("support");
        // Crash artifact: a copy stopped before verification/publication. No DB inspection.
        let unfinished = support.join("Library/Incoming/.tmp-interrupted");
        std::fs::write(&unfinished, b"RIFF partial copy").unwrap();
        drop(state);
        let mut restarted = super::ManagedLibrary::open(&support).unwrap();
        assert!(!unfinished.exists());
        assert_eq!(restarted.list().unwrap().len(), 1);
        assert_eq!(restarted.list().unwrap()[0].id, sample.id);
        let retry = restarted.import(vec![source]).unwrap();
        assert_eq!((retry.imported, retry.recognized, retry.failed), (0, 1, 0));
    }

    #[test]
    fn legacy_folder_commands_cannot_open_managed_storage_for_destructive_workflows() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Short.wav");
        wav::write_pcm16(&source, &[8192; 100], 48000).unwrap();
        let original_bytes = std::fs::read(&source).unwrap();
        let sample = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0);
        let prepared = state.managed.lock().unwrap().prepare(&sample.id).unwrap();
        let old_session = state.snapshot().session;
        let error = state
            .act(crate::Action::Root {
                path: temp.path().join("support").to_string_lossy().into_owned(),
            })
            .err()
            .expect("managed Library must never be a legacy source");
        assert!(error.contains("managed Originals"));
        assert_eq!(state.snapshot().session.root_path, old_session.root_path);
        #[cfg(unix)]
        {
            let root = temp.path().join("external");
            std::fs::create_dir_all(root.join("A0")).unwrap();
            let alias = root.join("A0/1_Alias.wav");
            std::os::unix::fs::symlink(&prepared.path, &alias).unwrap();
            state
                .act(crate::Action::Root {
                    path: root.to_string_lossy().into_owned(),
                })
                .unwrap();
            assert!(state.open_editor(0).is_err());
            let result = state
                .convert_imports(vec![crate::ReviewedFile {
                    id: alias.to_string_lossy().into_owned(),
                    fingerprint: rample_core::catalog::fingerprint(&alias),
                }])
                .unwrap();
            assert!(result.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("managed Originals"));
        }
        assert_eq!(std::fs::read(&prepared.path).unwrap(), original_bytes);
    }

    #[test]
    fn import_reports_partial_failures_and_never_publishes_incomplete_audio() {
        let (temp, state, _) = fixture();
        let valid = temp.path().join("Valid.wav");
        let corrupt = temp.path().join("Corrupt.wav");
        let missing = temp.path().join("Unreadable.wav");
        wav::write_pcm16(&valid, &vec![8192; 4410], 44100).unwrap();
        std::fs::copy(&valid, &corrupt).unwrap();
        let truncated = std::fs::OpenOptions::new()
            .write(true)
            .open(&corrupt)
            .unwrap();
        truncated.set_len(100).unwrap();
        let reply = state
            .import_samples(vec![valid.clone(), corrupt.clone(), missing])
            .unwrap();
        assert_eq!(
            (reply.imported, reply.failed, reply.complete),
            (1, 2, false)
        );
        assert!(reply.results[1..]
            .iter()
            .all(|r| r.error.is_some() && r.sample.is_none()));
        assert_eq!(std::fs::metadata(corrupt).unwrap().len(), 100);
        let support = temp.path().join("support");
        let restarted = super::ManagedLibrary::open(&support).unwrap();
        assert_eq!(restarted.list().unwrap().len(), 1);
        // Simulate failed publication after staging, with an existing complete Library intact.
        let originals = support.join("Library/Originals");
        std::fs::rename(&originals, support.join("Library/Originals-safe")).unwrap();
        std::fs::write(&originals, b"unavailable storage").unwrap();
        wav::write_pcm16(&valid, &vec![4096; 4410], 44100).unwrap();
        let failed = state.import_samples(vec![valid.clone()]).unwrap();
        assert_eq!(
            (failed.imported, failed.failed, failed.complete),
            (0, 1, false)
        );
        assert_eq!(failed.samples.len(), 1);
        assert!(!failed.samples[0].readable);
        std::fs::remove_file(&originals).unwrap();
        std::fs::rename(support.join("Library/Originals-safe"), originals).unwrap();
        assert_eq!(
            super::ManagedLibrary::open(&support)
                .unwrap()
                .list()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(wav::inspect(&valid).unwrap().frames, 4410);
    }

    #[test]
    fn renamed_repeated_and_partially_known_folder_imports_reuse_the_sample() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("First name.wav");
        wav::write_pcm16(&source, &vec![8192; 4410], 44100).unwrap();
        let first = state
            .import_samples(vec![source.clone()])
            .unwrap()
            .samples
            .remove(0);
        let folder = temp.path().join("not-a-Rample-folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::rename(source, folder.join("Different name.WAV")).unwrap();
        wav::write_pcm16(&folder.join("New sound.wav"), &vec![4096; 4410], 44100).unwrap();
        let reply = state.import_samples(vec![folder.clone()]).unwrap();
        assert_eq!(
            reply
                .results
                .iter()
                .map(|r| r.status.as_str())
                .collect::<Vec<_>>(),
            ["recognized", "imported"]
        );
        assert_eq!(reply.samples.len(), 2);
        assert_eq!(reply.results[0].sample.as_ref().unwrap().id, first.id);
        assert_eq!(
            reply.results[0].sample.as_ref().unwrap().original_id,
            first.original_id
        );
        assert_eq!(reply.samples[0].name, "First name");
        let repeated = state.import_samples(vec![folder]).unwrap();
        assert!(repeated.results.iter().all(|r| r.status == "recognized"));
        assert_eq!(repeated.samples.len(), 2);
    }

    #[test]
    fn imported_original_survives_source_removal_and_application_restart() {
        let (temp, state, _) = fixture();
        let legacy = state.act(Action::SetBpm { value: 137. }).unwrap().session;
        let source = temp.path().join("My kick.wav");
        wav::write_pcm16(&source, &vec![8192; 4410], 44100).unwrap();
        let bytes = std::fs::read(&source).unwrap();
        let reply = state.import_samples(vec![source.clone()]).unwrap();
        assert_eq!(reply.results[0].status, "imported");
        let samples = state.list_samples().unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "My kick");
        assert!(samples[0].readable);
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        std::fs::remove_file(source).unwrap();
        let support = temp.path().join("support");
        let restarted = super::ManagedLibrary::open(&support).unwrap();
        assert_eq!(restarted.list().unwrap()[0].id, samples[0].id);
        let prepared = restarted.prepare(&samples[0].id).unwrap();
        assert_eq!(std::fs::read(&prepared.path).unwrap(), bytes);
        assert!(!Path::new(&samples[0].id).is_absolute());
        assert_eq!(prepared.head[0], [0.25, 0.25]);
        let owner = state.open_sample_audition().unwrap();
        state.audition_sample(owner, samples[0].id.clone()).unwrap();
        assert_eq!(Library::new(support).unwrap().session, legacy);
    }
}
