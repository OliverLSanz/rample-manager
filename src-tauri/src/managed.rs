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
    pub tags: Vec<String>,
    pub retired: bool,
    pub sha256: String,
    pub duration: f64,
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub format: String,
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
    #[serde(skip)]
    pub(super) copied_hash: Option<String>,
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
    pub(super) owner: String,
    pub(super) generation: u64,
}

pub struct ManagedLibrary {
    pub(super) root: PathBuf,
    pub(super) db: Connection,
}
impl ManagedLibrary {
    pub fn open(support: &Path) -> Result<Self, String> {
        let root = crate::library_snapshots::active_root(support)?;
        Self::open_root(root)
    }
    pub(super) fn open_root(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(root.join("Originals")).map_err(|e| e.to_string())?;
        fs::create_dir_all(root.join("Incoming")).map_err(|e| e.to_string())?;
        let db = Connection::open(root.join("library.sqlite3")).map_err(|e| e.to_string())?;
        let version: u32 = db
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if version > 8 {
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
        if version < 2 {
            db.execute_batch("BEGIN IMMEDIATE; ALTER TABLE samples ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'; PRAGMA user_version=2; COMMIT;")
                .map_err(|e| e.to_string())?;
        }
        if version < 3 {
            db.execute_batch("BEGIN IMMEDIATE; ALTER TABLE originals ADD COLUMN format TEXT NOT NULL DEFAULT 'WAV'; PRAGMA user_version=3; COMMIT;")
                .map_err(|e| e.to_string())?;
        }
        if version < 4 {
            db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE patch_state (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL); PRAGMA user_version=4; COMMIT;")
                .map_err(|e| e.to_string())?;
        }
        if version < 5 {
            db.execute_batch("BEGIN IMMEDIATE; ALTER TABLE samples ADD COLUMN retired INTEGER NOT NULL DEFAULT 0; PRAGMA user_version=5; COMMIT;")
                .map_err(|e| e.to_string())?;
        }
        if version < 6 {
            db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE sample_revisions(id TEXT PRIMARY KEY, sample_id TEXT NOT NULL REFERENCES samples(id), settings TEXT NOT NULL); CREATE TABLE sample_current(sample_id TEXT PRIMARY KEY REFERENCES samples(id), revision_id TEXT NOT NULL REFERENCES sample_revisions(id)); PRAGMA user_version=6; COMMIT;")
                .map_err(|e| e.to_string())?;
        }
        crate::sd_import::migrate(&db, version)?;
        if version < 8 {
            db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE exported_audio (sha256 TEXT NOT NULL, sample_id TEXT NOT NULL REFERENCES samples(id), revision_id TEXT NOT NULL REFERENCES sample_revisions(id), render TEXT NOT NULL DEFAULT 'rample-v1', recovered_id TEXT REFERENCES samples(id), PRIMARY KEY (sha256, sample_id, revision_id)); PRAGMA user_version=8; COMMIT;").map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(root.join("Derived")).map_err(|e| e.to_string())?;
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
        for entry in fs::read_dir(root.join("Derived")).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("edit-v1-draft-")
                && entry.file_type().map_err(|e| e.to_string())?.is_file()
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        Ok(Self { root, db })
    }
    pub fn load_patches(&self) -> Result<patches::PatchGraph, String> {
        let payload: Option<String> = self
            .db
            .query_row("SELECT payload FROM patch_state WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        match payload {
            Some(payload) => serde_json::from_str(&payload)
                .map_err(|e| format!("Could not read saved Patches: {e}")),
            None => Ok(patches::PatchGraph::default()),
        }
    }
    pub fn save_patches(&self, graph: &patches::PatchGraph) -> Result<(), String> {
        let payload = serde_json::to_string(graph).map_err(|e| e.to_string())?;
        self.db.execute("INSERT INTO patch_state(id,payload) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [payload]).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn original_path(&self, id: &str) -> PathBuf {
        self.root.join("Originals").join(format!("{id}.wav"))
    }
    pub fn list(&self) -> Result<Vec<LibrarySample>, String> {
        let mut query = self
            .db
            .prepare(
                "SELECT s.id, s.original_id, s.name, o.sha256,
            o.rate, o.channels, o.bits, o.frames, s.tags, o.format, s.retired FROM samples s JOIN originals o
            ON s.original_id=o.id ORDER BY s.rowid",
            )
            .map_err(|e| e.to_string())?;
        let rows = query.query_map([], sample_row).map_err(|e| e.to_string())?;
        let mut samples = vec![];
        for row in rows {
            let mut sample = row.map_err(|e| e.to_string())?;
            if let Err(error) = conversion::inspect(&self.original_path(&sample.original_id)) {
                sample.readable = false;
                sample.error = Some(format!("Original is unavailable: {error}"));
            }
            samples.push(sample);
        }
        Ok(samples)
    }
    /// Literal Unicode-lowercase substring across the name or an individual tag.
    /// Empty/whitespace query returns active Samples in stable import order.
    pub fn query(&self, query: &str) -> Result<Vec<LibrarySample>, String> {
        let query = query.trim().to_lowercase();
        Ok(self
            .list()?
            .into_iter()
            .filter(|sample| {
                !sample.retired
                    && (sample.name.to_lowercase().contains(&query)
                        || sample
                            .tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query)))
            })
            .collect())
    }
    fn find_sample(&self, predicate: &str, value: &str) -> Result<Option<LibrarySample>, String> {
        // Predicate is one of the two fixed indexed lookups below, never external input.
        self.db
            .query_row(
                &format!(
                    "SELECT s.id, s.original_id, s.name, o.sha256,
            o.rate, o.channels, o.bits, o.frames, s.tags, o.format, s.retired FROM originals o JOIN samples s
            ON s.original_id=o.id WHERE {predicate} ORDER BY s.rowid LIMIT 1"
                ),
                [value],
                sample_row,
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn set_retired(&self, id: &str, retired: bool) -> Result<LibrarySample, String> {
        let changed = self
            .db
            .execute(
                "UPDATE samples SET retired=?2 WHERE id=?1",
                params![id, retired],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("Sample no longer exists.".into());
        }
        self.find_sample("s.id=?1", id)?
            .ok_or_else(|| "Sample no longer exists.".into())
    }
    pub fn update_metadata(
        &self,
        id: &str,
        name: String,
        tags: Vec<String>,
    ) -> Result<LibrarySample, String> {
        if name.trim().is_empty() {
            return Err("Sample name cannot be empty.".into());
        }
        let changed = self
            .db
            .execute(
                "UPDATE samples SET name=?2, tags=?3 WHERE id=?1",
                params![
                    id,
                    name,
                    serde_json::to_string(&tags).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err("Sample no longer exists".into());
        }
        self.find_sample("s.id=?1", id)?
            .ok_or_else(|| "Sample no longer exists".into())
    }
    pub fn prepare(&self, id: &str) -> Result<Arc<PreparedSample>, String> {
        self.prepare_target(id, false)
    }
    pub fn prepare_rample(&self, id: &str) -> Result<Arc<PreparedSample>, String> {
        let revision = self.sample_edits(id)?;
        if revision.id.starts_with("original-") {
            self.prepare_target(id, true)
        } else {
            self.render_edits(id, &revision.id, &revision.settings)
        }
    }
    pub(super) fn prepare_target(
        &self,
        id: &str,
        rample: bool,
    ) -> Result<Arc<PreparedSample>, String> {
        let sample = self
            .find_sample("s.id=?1", id)?
            .ok_or("Sample no longer exists")?;
        let original = self.original_path(&sample.original_id);
        if fingerprint(&original)? != sample.sha256 {
            return Err(
                "Original changed in managed storage. Restore the Library from a backup.".into(),
            );
        }
        if !rample && sample.format == "WAV" {
            validate_wav(&original)?;
            return PreparedSample::load(&original, &sample.sha256);
        }
        // Derivatives never participate in identity/recognition. Rebuild missing or
        // incomplete files from the immutable Original, with publication off callback.
        let kind = if rample {
            "rample-v1"
        } else {
            "original-playback-v1"
        };
        let path = self
            .root
            .join("Derived")
            .join(format!("{}-{kind}.wav", sample.sha256));
        if validate_wav(&path).is_err() {
            let mut staged = tempfile::NamedTempFile::new_in(self.root.join("Incoming"))
                .map_err(|e| e.to_string())?;
            if rample {
                conversion::prepare_rample(&original, staged.as_file_mut())?;
            } else {
                conversion::decode_original(&original, staged.as_file_mut())?;
            }
            validate_wav(staged.path())?;
            staged.as_file().sync_all().map_err(|e| e.to_string())?;
            if fingerprint(&original)? != sample.sha256 {
                return Err("Original changed during preparation.".into());
            }
            // Only this disposable, application-owned cache entry may be replaced.
            staged.persist(&path).map_err(|e| e.to_string())?;
        }
        PreparedSample::load(&path, &format!("{}-{kind}", sample.sha256))
    }
    fn recognize_export(&mut self, sha256: &str) -> Result<Option<LibrarySample>, String> {
        let record: Option<(String, String, String)> = self.db.query_row(
            "SELECT e.sample_id,e.revision_id,r.settings FROM exported_audio e JOIN sample_revisions r ON r.id=e.revision_id WHERE e.sha256=?1 ORDER BY CASE WHEN EXISTS (SELECT 1 FROM sample_current c WHERE c.sample_id=e.sample_id AND c.revision_id=e.revision_id) THEN 0 ELSE 1 END,e.rowid LIMIT 1",
            [sha256], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))) .optional().map_err(|e| e.to_string())?;
        let Some((source, revision, settings)) = record else {
            return Ok(None);
        };
        let current = self.sample_edits(&source)?;
        if current.id == revision {
            self.prepare(&source)?;
            return self.find_sample("s.id=?1", &source);
        }
        // Recovery belongs to this source Sample's saved recipe, not one emitted
        // container hash. Intentional Variants keep their own source identities.
        let recovered = {
            let mut query = self.db.prepare("SELECT DISTINCT e.recovered_id FROM exported_audio e JOIN sample_revisions r ON r.id=e.revision_id WHERE e.sample_id=?1 AND r.settings=?2 AND e.recovered_id IS NOT NULL ORDER BY e.rowid").map_err(|e| e.to_string())?;
            let ids = query
                .query_map(params![source, settings], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            ids
        };
        for id in recovered {
            let saved = self.sample_edits(&id)?;
            if serde_json::to_string(&saved.settings).map_err(|e| e.to_string())? == settings {
                self.prepare(&id)?;
                self.db.execute("UPDATE exported_audio SET recovered_id=?1 WHERE sha256=?2 AND sample_id=?3 AND revision_id=?4", params![id,sha256,source,revision]).map_err(|e| e.to_string())?;
                return self.find_sample("s.id=?1", &id);
            }
        }
        let sample = self
            .find_sample("s.id=?1", &source)?
            .ok_or("Sample no longer exists.")?;
        self.prepare(&source)?;
        let id = Uuid::new_v4().to_string();
        let new_revision = if revision.starts_with("original-") {
            format!("original-{id}")
        } else {
            Uuid::new_v4().to_string()
        };
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO samples(id,original_id,name,tags) VALUES (?1,?2,?3,?4)",
            params![
                id,
                sample.original_id,
                format!("{} (recovered)", sample.name),
                serde_json::to_string(&sample.tags).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sample_revisions(id,sample_id,settings) VALUES (?1,?2,?3)",
            params![new_revision, id, settings],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sample_current(sample_id,revision_id) VALUES (?1,?2)",
            params![id, new_revision],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("UPDATE exported_audio SET recovered_id=?1 WHERE sha256=?2 AND sample_id=?3 AND revision_id=?4", params![id,sha256,source,revision]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.find_sample("s.id=?1", &id)
    }
    pub(super) fn export_matches_sample(&self, sha256: &str, id: &str) -> Result<bool, String> {
        let mut records = self.db.prepare("SELECT e.sample_id,e.revision_id,e.recovered_id,r.settings FROM exported_audio e JOIN sample_revisions r ON r.id=e.revision_id WHERE e.sha256=?1").map_err(|e|e.to_string())?;
        let rows = records
            .query_map([sha256], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(true);
        }
        let current = self.sample_edits(id)?;
        let settings = serde_json::to_string(&current.settings).map_err(|e| e.to_string())?;
        Ok(rows.iter().any(|(source, revision, recovered, saved)| {
            (source == id && revision == &current.id)
                || (recovered.as_deref() == Some(id) && saved == &settings)
        }))
    }
    pub fn export_sample(&self, id: &str, destination: &Path) -> Result<(), String> {
        let parent = destination
            .parent()
            .ok_or("Choose an export destination.")?;
        let staged = self.stage_export(id, parent)?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        staged.register(&tx)?;
        staged
            .file
            .persist_noclobber(destination)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| {
            format!("Sample was exported, but its recognition record could not be saved: {e}")
        })?;
        Ok(())
    }
    /// Renders the Sample's current Revision for the Rample into a uniquely tagged,
    /// validated temporary file in `directory`. Every application path that emits
    /// processed audio registers it with [`StagedExport::register`].
    pub(super) fn stage_export(&self, id: &str, directory: &Path) -> Result<StagedExport, String> {
        let revision = self.sample_edits(id)?;
        let prepared = self.prepare_revision(id, &revision.id)?;
        let render = if Path::new(&prepared.path)
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("edit-v1-"))
        {
            "edit-v1"
        } else {
            "rample-v1"
        };
        let mut staged = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
        let mut input = File::open(&prepared.path).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, staged.as_file_mut()).map_err(|e| e.to_string())?;
        // Tag the emitted RIFF container without changing PCM. This distinguishes a
        // byte-identical Original from its historical prepared export on reimport.
        let file = staged.as_file_mut();
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        let riff_size = u32::try_from(length.checked_add(36).ok_or("Export is too large.")?)
            .map_err(|_| "Export is too large.")?;
        file.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;
        file.write_all(&riff_size.to_le_bytes())
            .map_err(|e| e.to_string())?;
        file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        file.write_all(b"JUNK\x24\x00\x00\x00")
            .and_then(|_| file.write_all(Uuid::new_v4().to_string().as_bytes()))
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        let spec = validate_wav(staged.path())?;
        // Rample target: standard mono PCM16 at 44.1 kHz and at least 50 ms.
        if (spec.code, spec.channels, spec.bits, spec.rate) != (1, 1, 16, 44100)
            || spec.frames < 2205
        {
            return Err("Prepared audio is not mono PCM16 at 44.1 kHz of at least 50 ms.".into());
        }
        let sha256 = fingerprint(staged.path())?;
        Ok(StagedExport {
            file: staged,
            sha256,
            sample_id: id.to_owned(),
            revision_id: revision.id,
            render,
        })
    }
    fn import_file(&mut self, source: &Path) -> Result<(LibrarySample, bool, String), String> {
        if !fs::metadata(source).map_err(|e| e.to_string())?.is_file() {
            return Err("Choose a regular supported audio file".into());
        }
        let mut input = File::open(source).map_err(|e| e.to_string())?;
        if !input.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Choose a regular supported audio file".into());
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
        let mut info = conversion::inspect(staged.path())?;
        if info.format == "WAVE" {
            let spec = validate_wav(staged.path())?;
            if spec.frames == 0 {
                return Err("The sample contains no audio.".into());
            }
            info.format = "WAV".into();
            info.frames = spec.frames;
            info.rate = spec.rate;
            info.channels = spec.channels;
            info.bits = spec.bits;
        } else {
            let mut decoded = tempfile::NamedTempFile::new_in(self.root.join("Incoming"))
                .map_err(|e| e.to_string())?;
            info = conversion::decode_original(staged.path(), decoded.as_file_mut())?;
            validate_wav(decoded.path())?;
        }
        if fingerprint(staged.path())? != sha256 {
            return Err("Could not verify the copied Original".into());
        }
        if fingerprint(source)? != sha256 {
            return Err("Source changed during import. Import the file again.".into());
        }
        if let Some(sample) = self.recognize_export(&sha256)? {
            return Ok((sample, false, sha256));
        }
        if let Some(sample) = self.find_sample("o.sha256=?1", &sha256)? {
            self.prepare(&sample.id)?;
            return Ok((sample, false, sha256));
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
                "INSERT INTO originals (id,sha256,rate,channels,bits,frames,format) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    original_id,
                    sha256,
                    info.rate,
                    info.channels,
                    info.bits,
                    i64::try_from(info.frames).map_err(|e| e.to_string())?,
                    info.format
                ],
            )
            .map_err(|e| e.to_string())?;
        transaction
            .execute(
                "INSERT INTO samples (id, original_id, name) VALUES (?1,?2,?3)",
                params![sample_id, original_id, name],
            )
            .map_err(|e| e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok((
            LibrarySample {
                id: sample_id,
                original_id,
                name,
                tags: vec![],
                retired: false,
                sha256: sha256.clone(),
                duration: info.frames as f64 / f64::from(info.rate),
                rate: info.rate,
                channels: info.channels,
                bits: info.bits,
                format: info.format,
                readable: true,
                error: None,
            },
            true,
            sha256,
        ))
    }
    pub fn import(&mut self, paths: Vec<PathBuf>) -> Result<ImportReply, String> {
        self.import_progress(paths, |_, _, _| {})
    }
    pub(super) fn import_progress(
        &mut self,
        paths: Vec<PathBuf>,
        mut progress: impl FnMut(usize, usize, &Path),
    ) -> Result<ImportReply, String> {
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
                        if conversion::is_audio(entry.path()) && !entry.file_type().is_dir() =>
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
                inputs.push((
                    path,
                    Some("No supported audio files found in this folder.".into()),
                ));
            }
        }
        let mut results = vec![];
        let total = inputs.len();
        for (index, (source, prior_error)) in inputs.into_iter().enumerate() {
            progress(index, total, &source);
            let outcome = match prior_error {
                Some(error) => Err(error),
                None => self.import_file(&source),
            };
            let (sample, status, error, copied_hash) = match outcome {
                Ok((sample, added, hash)) => (
                    Some(sample),
                    if added { "imported" } else { "recognized" },
                    None,
                    Some(hash),
                ),
                Err(error) => (None, "failed", Some(error), None),
            };
            results.push(ImportResult {
                source: source.to_string_lossy().into_owned(),
                status: status.into(),
                sample,
                error,
                copied_hash,
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
/// Validated prepared audio awaiting publication, with its recognition provenance.
pub(super) struct StagedExport {
    pub(super) file: tempfile::NamedTempFile,
    pub(super) sha256: String,
    pub(super) sample_id: String,
    pub(super) revision_id: String,
    pub(super) render: &'static str,
}
impl StagedExport {
    /// Records the emitted hash so reimport resolves it to its Sample and Revision.
    pub(super) fn register(&self, tx: &rusqlite::Transaction<'_>) -> Result<(), String> {
        tx.execute(
            "INSERT OR IGNORE INTO exported_audio(sha256,sample_id,revision_id,render) VALUES (?1,?2,?3,?4)",
            params![self.sha256, self.sample_id, self.revision_id, self.render],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub(super) fn validate_wav(path: &Path) -> Result<wav::WavSpec, String> {
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
        tags: serde_json::from_str(&row.get::<_, String>(8)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        sha256: row.get(3)?,
        rate,
        channels: row.get(5)?,
        bits: row.get(6)?,
        format: row.get(9)?,
        retired: row.get(10)?,
        duration: row.get::<_, i64>(7)? as f64 / f64::from(rate),
        readable: true,
        error: None,
    })
}
pub(super) fn fingerprint(path: &Path) -> Result<String, String> {
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
    pub fn prepare_library_sample(
        &self,
        id: &str,
        rample: bool,
    ) -> Result<Arc<PreparedSample>, String> {
        let library = self.managed.lock().map_err(|_| "Library is unavailable")?;
        if rample {
            let revision = library.sample_edits(id)?.id;
            drop(library);
            self.prepare_sample_revision(id.to_owned(), revision)
        } else {
            library.prepare(id)
        }
    }
    pub fn list_samples(&self) -> Result<Vec<LibrarySample>, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .list()
    }
    pub fn list_trashed_samples(&self) -> Result<Vec<LibrarySample>, String> {
        Ok(self
            .list_samples()?
            .into_iter()
            .filter(|sample| sample.retired)
            .collect())
    }
    pub fn retire_sample(&self, id: String) -> Result<LibrarySample, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .set_retired(&id, true)
    }
    pub fn restore_sample(&self, id: String) -> Result<LibrarySample, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .set_retired(&id, false)
    }
    pub fn export_sample(&self, id: String, destination: PathBuf) -> Result<(), String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .export_sample(&id, &destination)
    }
    pub fn import_samples(&self, paths: Vec<PathBuf>) -> Result<ImportReply, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .import(paths)
    }
    pub fn query_samples(&self, query: String) -> Result<Vec<LibrarySample>, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .query(&query)
    }
    pub fn update_sample_metadata(
        &self,
        id: String,
        name: String,
        tags: Vec<String>,
    ) -> Result<LibrarySample, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .update_metadata(&id, name, tags)
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
        self.audition_sample_target(owner, id, false)
    }
    pub fn audition_sample_target(
        &self,
        owner: String,
        id: String,
        rample: bool,
    ) -> Result<(), String> {
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
        let prepared = self.prepare_library_sample(&id, rample)?;
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
pub(super) async fn list_trashed_samples(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<LibrarySample>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.list_trashed_samples())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn retire_sample(
    id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LibrarySample, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.retire_sample(id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn restore_sample(
    id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LibrarySample, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.restore_sample(id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn export_sample(
    id: String,
    destination: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.export_sample(id, destination))
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
pub(super) async fn query_samples(
    query: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<LibrarySample>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.query_samples(query))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn update_sample_metadata(
    id: String,
    name: String,
    tags: Vec<String>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LibrarySample, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.update_sample_metadata(id, name, tags))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn audition_sample(
    id: String,
    session: String,
    rample: bool,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if rample {
            state.audition_sample_target(session, id, true)
        } else {
            state.audition_sample(session, id)
        }
    })
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
    fn exported_sample_is_recognized_after_restart_without_changing_original() {
        use std::fs;
        let (temp, state, _) = fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let original = fs::read(&source).unwrap();
        let sample = state.import_samples(vec![source.clone()]).unwrap().samples[0].clone();
        let exported = temp.path().join("export.wav");
        state
            .export_sample(sample.id.clone(), exported.clone())
            .unwrap();
        assert_ne!(fs::read(&exported).unwrap(), original);
        let prepared = state.prepare_library_sample(&sample.id, true).unwrap();
        let emitted = crate::PreparedSample::load(&exported, "export").unwrap();
        assert_eq!(prepared.head, emitted.head);
        assert_eq!(fs::read(&source).unwrap(), original);
        drop(state);
        let (state, _) = crate::tests::fixture_state(temp.path().join("support"));
        let reply = state.import_samples(vec![exported]).unwrap();
        assert_eq!((reply.imported, reply.recognized), (0, 1));
        assert_eq!(reply.results[0].sample.as_ref().unwrap().id, sample.id);
    }

    #[test]
    fn sample_export_never_overwrites_an_existing_destination() {
        use std::fs;
        let (temp, state, _) = fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source.clone()]).unwrap().samples[0].clone();
        let before = fs::read(&source).unwrap();
        assert!(state.export_sample(sample.id, source.clone()).is_err());
        assert_eq!(fs::read(source).unwrap(), before);
    }

    #[test]
    fn retired_samples_keep_layer_identity_pcm_and_recognition_across_restart() {
        use crate::patches::PatchAction;
        use rample_core::audio::{Command, Meters, Renderer};
        use std::{fs, sync::Arc};
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source.clone()]).unwrap().samples[0].clone();
        state
            .update_sample_metadata(sample.id.clone(), "低音 kick".into(), vec!["warm".into()])
            .unwrap();
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Live".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        let before = state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let layer = before.graph.current().unwrap().kit().unwrap().voices[0].layers[0]
            .id
            .clone();
        let original = state.prepare_library_sample(&sample.id, false).unwrap();
        let original_bytes = fs::read(&original.path).unwrap();
        assert!(state.retire_sample(sample.id.clone()).unwrap().retired);
        assert!(state.query_samples("warm".into()).unwrap().is_empty());
        assert_eq!(state.list_trashed_samples().unwrap()[0].id, sample.id);
        assert!(state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone()
            })
            .is_err());
        let reply = state.import_samples(vec![source.clone()]).unwrap();
        assert_eq!((reply.imported, reply.recognized), (0, 1));
        assert_eq!(reply.results[0].sample.as_ref().unwrap().id, sample.id);
        assert!(reply.results[0].sample.as_ref().unwrap().retired);
        assert_eq!(reply.samples.len(), 1);
        fs::remove_file(source).unwrap();
        drop(state);
        let (state, _) = crate::tests::fixture_state(temp.path().join("support"));
        let reopened = state.patch_action(PatchAction::Open).unwrap();
        let voice = &reopened.graph.current().unwrap().kit().unwrap().voices[0];
        assert_eq!(voice.selected_layer, layer);
        assert_eq!(voice.layers[0].sample_id, sample.id);
        assert!(reopened.validation.is_empty());
        assert_eq!(state.list_trashed_samples().unwrap()[0].tags, ["warm"]);
        let prepared = state.prepare_library_sample(&sample.id, true).unwrap();
        let (mut renderer, control) = Renderer::new(44100, Arc::new(Meters::default()));
        control
            .send(Command::Settings(reopened.session.unwrap().voices, 120.))
            .unwrap();
        control
            .send(Command::LayerBank(
                0,
                vec![rample_core::audio::LayerSample {
                    id: layer,
                    sample: prepared,
                }],
            ))
            .unwrap();
        control.send(Command::Audition(0)).unwrap();
        let mut pcm = vec![0.; 1024];
        renderer.render(&mut pcm, 2);
        assert!(pcm[200..400]
            .iter()
            .all(|value| (*value - 0.08).abs() < 0.001));
        let restored = state.restore_sample(sample.id.clone()).unwrap();
        assert!(!restored.retired);
        assert_eq!(restored.original_id, sample.original_id);
        assert_eq!(restored.name, "低音 kick");
        assert_eq!(state.query_samples("warm".into()).unwrap()[0].id, sample.id);
        assert!(state.list_trashed_samples().unwrap().is_empty());
        assert_eq!(
            state
                .prepare_library_sample(&sample.id, false)
                .unwrap()
                .path,
            original.path
        );
        assert_eq!(fs::read(&original.path).unwrap(), original_bytes);
    }

    #[test]
    fn stereo_original_and_rample_preparation_remain_independent_after_restart() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Stereo.wav");
        let pcm: Vec<i16> = (0..4800).flat_map(|_| [16384, -8192]).collect();
        wav::write_pcm16(&source, &pcm, 48000).unwrap();
        let mut bytes = std::fs::read(&source).unwrap();
        bytes[22..24].copy_from_slice(&2u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&192000u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&4u16.to_le_bytes());
        std::fs::write(&source, &bytes).unwrap();
        let reply = state.import_samples(vec![source.clone()]).unwrap();
        assert_eq!((reply.imported, reply.failed), (1, 0));
        let sample = reply.samples[0].clone();
        assert_eq!(
            (
                sample.channels,
                sample.rate,
                sample.bits,
                sample.format.as_str()
            ),
            (2, 48000, 16, "WAV")
        );
        let original = state.prepare_library_sample(&sample.id, false).unwrap();
        assert_eq!(std::fs::read(&original.path).unwrap(), bytes);
        assert_eq!(original.head[100], [0.5, -0.25]);
        let rample = state.prepare_library_sample(&sample.id, true).unwrap();
        assert_eq!(
            (
                rample.spec.rate,
                rample.spec.channels,
                rample.spec.bits,
                rample.spec.frames
            ),
            (44100, 1, 16, 4410)
        );
        assert!((rample.head[100][0] - 0.125).abs() < 0.0001);
        assert_ne!(original.path, rample.path);
        std::fs::remove_file(&rample.path).unwrap();
        std::fs::remove_file(source).unwrap();
        let support = temp.path().join("support");
        drop(state);
        let restarted = super::ManagedLibrary::open(&support).unwrap();
        assert_eq!(restarted.list().unwrap()[0].id, sample.id);
        let rebuilt = restarted.prepare_rample(&sample.id).unwrap();
        assert!((rebuilt.head[100][0] - 0.125).abs() < 0.0001);
        assert_eq!(
            std::fs::read(restarted.prepare(&sample.id).unwrap().path.clone()).unwrap(),
            bytes
        );
    }

    #[test]
    fn supported_compressed_originals_survive_source_removal_and_incomplete_imports() {
        let (temp, state, _) = fixture();
        let mut originals = vec![];
        let mut paths = vec![];
        for (name, hex) in [
            (
                "Stereo.flac",
                include_str!("../../crates/rample-core/tests/fixtures/stereo-flac.hex"),
            ),
            (
                "Mono.mp3",
                include_str!("../../crates/rample-core/tests/fixtures/mono-mp3.hex"),
            ),
        ] {
            let hex: String = hex.split_whitespace().collect();
            let bytes: Vec<_> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let path = temp.path().join(name);
            std::fs::write(&path, &bytes).unwrap();
            paths.push(path);
            originals.push(bytes);
        }
        let mut damaged_mp3 = originals[1].clone();
        damaged_mp3.pop();
        let broken_mp3 = temp.path().join("Truncated.mp3");
        std::fs::write(&broken_mp3, &damaged_mp3).unwrap();
        // Valid stereo AIFF with trailing metadata; its truncated sibling must fail.
        let mut aiff = b"FORM".to_vec();
        aiff.extend(66u32.to_be_bytes());
        aiff.extend(b"AIFFCOMM");
        aiff.extend(18u32.to_be_bytes());
        aiff.extend(2u16.to_be_bytes());
        aiff.extend(2u32.to_be_bytes());
        aiff.extend(16u16.to_be_bytes());
        aiff.extend([0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
        aiff.extend(b"SSND");
        aiff.extend(16u32.to_be_bytes());
        aiff.extend([0; 8]);
        for value in [16384i16, -8192, 16384, -8192] {
            aiff.extend(value.to_be_bytes());
        }
        aiff.extend(b"NAME");
        aiff.extend(0u32.to_be_bytes());
        let source = temp.path().join("Stereo.aiff");
        std::fs::write(&source, &aiff).unwrap();
        paths.push(source);
        originals.push(aiff.clone());
        let broken = temp.path().join("Truncated.aiff");
        aiff.pop();
        std::fs::write(&broken, &aiff).unwrap();
        paths.push(broken);
        paths.push(broken_mp3);
        let reply = state.import_samples(paths.clone()).unwrap();
        assert_eq!((reply.imported, reply.failed), (3, 2));
        assert!(reply.results[3].error.is_some());
        for path in paths {
            std::fs::remove_file(path).unwrap();
        }
        let support = temp.path().join("support");
        drop(state);
        let library = super::ManagedLibrary::open(&support).unwrap();
        let samples = library.list().unwrap();
        assert_eq!(
            samples
                .iter()
                .map(|s| (s.format.as_str(), s.channels, s.rate))
                .collect::<Vec<_>>(),
            [("FLAC", 2, 48000), ("MP3", 1, 44100), ("AIFF", 2, 44100)]
        );
        for (sample, bytes) in samples.iter().zip(originals) {
            assert_eq!(
                std::fs::read(
                    support
                        .join("Library/Originals")
                        .join(format!("{}.wav", sample.original_id))
                )
                .unwrap(),
                bytes
            );
            let playback = library.prepare(&sample.id).unwrap();
            assert_eq!(playback.spec.channels, sample.channels);
            assert_eq!(playback.spec.rate, sample.rate);
            assert!(playback.head.len() <= 262144);
            let prepared = library.prepare_rample(&sample.id).unwrap();
            assert_eq!(
                (
                    prepared.spec.rate,
                    prepared.spec.channels,
                    prepared.spec.bits
                ),
                (44100, 1, 16)
            );
        }
    }

    #[test]
    fn opening_v1_and_v2_libraries_preserves_metadata_and_adds_source_format() {
        for version in [1, 2] {
            let temp = tempfile::tempdir().unwrap();
            let support = temp.path().join("support");
            let root = support.join("Library");
            std::fs::create_dir_all(root.join("Originals")).unwrap();
            let original = root.join("Originals/existing-original.wav");
            wav::write_pcm16(&original, &[8192; 4410], 44100).unwrap();
            let bytes = std::fs::read(&original).unwrap();
            // A pre-feature store is an input fixture, never the behavioral oracle.
            let db = rusqlite::Connection::open(root.join("library.sqlite3")).unwrap();
            db.execute_batch("CREATE TABLE originals (id TEXT PRIMARY KEY, sha256 TEXT NOT NULL UNIQUE,
            rate INTEGER NOT NULL, channels INTEGER NOT NULL, bits INTEGER NOT NULL, frames INTEGER NOT NULL);
            CREATE TABLE samples (id TEXT PRIMARY KEY, original_id TEXT NOT NULL REFERENCES originals(id), name TEXT NOT NULL);
            CREATE INDEX samples_original ON samples(original_id);
            PRAGMA user_version=1;").unwrap();
            use sha2::Digest;
            db.execute(
                "INSERT INTO originals VALUES ('existing-original',?1,44100,1,16,4410)",
                [format!("{:x}", sha2::Sha256::digest(&bytes))],
            )
            .unwrap();
            db.execute(
            "INSERT INTO samples VALUES ('existing-sample','existing-original','Existing name')",
            [],
        )
        .unwrap();
            if version == 2 {
                db.execute_batch(
                    "ALTER TABLE samples ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
                UPDATE samples SET tags='[\"library v2\",\"低音\"]'; PRAGMA user_version=2;",
                )
                .unwrap();
            }
            drop(db);
            let library = super::ManagedLibrary::open(&support).unwrap();
            let sample = library.list().unwrap().remove(0);
            assert_eq!(sample.id, "existing-sample");
            assert_eq!(sample.original_id, "existing-original");
            assert_eq!(sample.name, "Existing name");
            assert_eq!(sample.format, "WAV");
            if version == 1 {
                assert!(sample.tags.is_empty());
            } else {
                assert_eq!(sample.tags, ["library v2", "低音"]);
            }
            library
                .update_metadata(&sample.id, "Renamed".into(), vec!["warm".into()])
                .unwrap();
            drop(library);
            let restarted = super::ManagedLibrary::open(&support).unwrap();
            let sample = restarted.query("WARM").unwrap().remove(0);
            assert_eq!(sample.id, "existing-sample");
            assert_eq!(
                std::fs::read(&restarted.prepare(&sample.id).unwrap().path).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn sample_queries_match_literal_name_and_arbitrary_tags_and_keep_duplicate_names_distinct() {
        let (temp, state, _) = fixture();
        let first = temp.path().join("First.wav");
        let second = temp.path().join("Second.wav");
        wav::write_pcm16(&first, &[8192; 4410], 44100).unwrap();
        wav::write_pcm16(&second, &[4096; 4410], 44100).unwrap();
        let samples = state.import_samples(vec![first, second]).unwrap().samples;
        state
            .update_sample_metadata(
                samples[0].id.clone(),
                "Same kick".into(),
                vec!["Café / 100%_warm".into(), "低音".into()],
            )
            .unwrap();
        state
            .update_sample_metadata(
                samples[1].id.clone(),
                "Same kick".into(),
                vec!["dry".into()],
            )
            .unwrap();
        assert_eq!(state.query_samples("SAME KICK".into()).unwrap().len(), 2);
        assert_eq!(
            state.query_samples("café / 100%_".into()).unwrap()[0].id,
            samples[0].id
        );
        assert_eq!(
            state.query_samples("低音".into()).unwrap()[0].id,
            samples[0].id
        );
        assert!(state.query_samples("%wildcard".into()).unwrap().is_empty());
        assert_eq!(state.query_samples("  ".into()).unwrap().len(), 2);
        let session = state.open_sample_audition().unwrap();
        for sample in &samples {
            state
                .audition_sample(session.clone(), sample.id.clone())
                .unwrap();
        }
        drop(state);
        let restarted = super::ManagedLibrary::open(&temp.path().join("support")).unwrap();
        assert_eq!(restarted.query("DRY").unwrap()[0].id, samples[1].id);
    }

    #[test]
    fn metadata_survives_restart_and_renamed_reimport_without_changing_identity_or_audio() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let original = state
            .import_samples(vec![source.clone()])
            .unwrap()
            .samples
            .remove(0);
        let tags = vec!["drums".into(), "café / warm, soft".into(), "低音".into()];
        let edited = state
            .update_sample_metadata(original.id.clone(), "My warm kick".into(), tags.clone())
            .unwrap();
        assert_eq!(
            (&edited.id, &edited.original_id, &edited.sha256),
            (&original.id, &original.original_id, &original.sha256)
        );
        assert_eq!(edited.name, "My warm kick");
        assert_eq!(edited.tags, tags);
        let renamed = temp.path().join("Different.wav");
        std::fs::rename(source, &renamed).unwrap();
        drop(state);
        let mut restarted = super::ManagedLibrary::open(&temp.path().join("support")).unwrap();
        let reply = restarted.import(vec![renamed]).unwrap();
        assert_eq!(
            (reply.imported, reply.recognized, reply.samples.len()),
            (0, 1, 1)
        );
        let sample = &reply.results[0].sample.as_ref().unwrap();
        assert_eq!(sample.id, original.id);
        assert_eq!(sample.name, "My warm kick");
        assert_eq!(sample.tags, tags);
        assert_eq!(restarted.prepare(&sample.id).unwrap().head[0], [0.25, 0.25]);
        assert_eq!(restarted.list().unwrap()[0].tags, tags);
    }

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
        assert_eq!(Library::new(support, false).unwrap().session, legacy);
    }
}
