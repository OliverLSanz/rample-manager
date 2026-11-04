//! Portable, verified Library snapshots and replacement generations.
use crate::*;
use managed::ManagedLibrary;
use rusqlite::{Connection, OpenFlags};
use std::{
    fs,
    fs::File,
    io::{Read, Write},
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Manifest {
    version: u32,
    database_sha256: String,
    originals: Vec<Asset>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Asset {
    id: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Activation {
    version: u32,
    generation: String,
    recovery: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotReply {
    pub complete: bool,
    pub activated: bool,
    pub location: Option<String>,
    pub recovery: Option<String>,
    pub error: Option<String>,
    pub snapshot: Option<Snapshot>,
}
impl SnapshotReply {
    fn success(location: &Path) -> Self {
        Self {
            complete: true,
            activated: false,
            location: Some(location.to_string_lossy().into_owned()),
            recovery: None,
            error: None,
            snapshot: None,
        }
    }
}
fn io(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn plain_metadata(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if metadata.file_type().is_symlink() {
        return Err("Snapshot paths must not contain symbolic links.".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("Snapshot paths must not contain junctions or reparse points.".into());
        }
    }
    Ok(metadata)
}
fn plain_directory(path: &Path) -> Result<(), String> {
    if !plain_metadata(path)?.is_dir() {
        return Err("Choose a regular snapshot folder.".into());
    }
    Ok(())
}
fn ensure_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir(path).map_err(io)?;
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
    }
    plain_directory(path)
}
fn read_only(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path).map_err(io)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(io)
}
fn open_regular(path: &Path) -> Result<File, String> {
    if !plain_metadata(path)?.is_file() {
        return Err("Snapshot entries must be regular files.".into());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100); // O_NOFOLLOW
        #[cfg(not(target_os = "macos"))]
        options.custom_flags(0x20000); // O_NOFOLLOW (Linux)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path).map_err(io)?;
    if !file.metadata().map_err(io)?.is_file() {
        return Err("Snapshot entries must be regular files.".into());
    }
    plain_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if file.metadata().map_err(io)?.file_attributes() & 0x400 != 0 {
            return Err("Snapshot entries must not be reparse points.".into());
        }
    }
    Ok(file)
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let mut bytes = vec![];
    open_regular(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Snapshot manifest is too large.".into());
    }
    serde_json::from_slice(&bytes).map_err(io)
}
fn copy_regular(source: &Path, destination: &Path) -> Result<(), String> {
    let mut input = open_regular(source)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io)?;
    std::io::copy(&mut input, &mut output).map_err(io)?;
    output.sync_all().map_err(io)
}
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
/// Use a write-through, non-replacing namespace move for the Windows commit.
/// https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw
#[cfg(windows)]
fn move_new(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both nul-terminated buffers remain alive for the call. The paths are owned,
    // generated staging/commit paths. Omitting REPLACE_EXISTING preserves conflicts.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x8) } == 0 {
        return Err(io(std::io::Error::last_os_error()));
    }
    Ok(())
}
#[cfg(not(windows))]
fn move_new(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(io)
}
fn publish_activation(
    temporary: tempfile::NamedTempFile,
    destination: &Path,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        // keep clears FILE_ATTRIBUTE_TEMPORARY before the flushed namespace move.
        let (file, path) = temporary.keep().map_err(io)?;
        file.sync_all().map_err(io)?;
        let result = move_new(&path, destination);
        drop(file);
        if result.is_err() {
            let _ = fs::remove_file(path);
        }
        result
    }
    #[cfg(not(windows))]
    temporary
        .persist_noclobber(destination)
        .map(|_| ())
        .map_err(io)
}
fn safe_id(id: &str) -> Result<(), String> {
    let upper = id.to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'));
    if reserved
        || id.is_empty()
        || id.len() > 120
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("Snapshot contains an unsafe Library identity.".into());
    }
    Ok(())
}
/// Append-only activation records avoid replacement of any open Windows file.
/// A complete record is the commit point. Earlier generations remain recoverable.
pub(super) fn active_root(support: &Path) -> Result<PathBuf, String> {
    let boundary = support.join("Library");
    let records = boundary.join("Activations");
    if boundary.exists() {
        plain_directory(&boundary)?;
    }
    if !records.exists() {
        return Ok(boundary);
    }
    plain_directory(&records)?;
    let mut names = fs::read_dir(&records)
        .map_err(io)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    names.retain(|p| p.extension().is_some_and(|e| e == "json"));
    names.sort();
    let Some(last) = names.last() else {
        return Ok(boundary);
    };
    let record: Activation = read_json(last)?;
    if record.version != 1 {
        return Err("Unsupported Library activation record.".into());
    }
    safe_id(&record.generation)?;
    let root = boundary.join("Generations").join(record.generation);
    if !root.join("library.sqlite3").is_file() {
        return Err(format!(
            "The active Library is unavailable. Restore the recovery snapshot at {}.",
            record.recovery
        ));
    }
    plain_directory(&boundary.join("Generations"))?;
    plain_directory(&root)?;
    open_regular(&root.join("library.sqlite3"))?;
    plain_directory(&root.join("Originals"))?;
    Ok(root)
}
fn assets(store: &ManagedLibrary) -> Result<Vec<Asset>, String> {
    let mut query = store
        .db
        .prepare("SELECT id,sha256 FROM originals ORDER BY id")
        .map_err(io)?;
    let result = query
        .query_map([], |r| {
            Ok(Asset {
                id: r.get(0)?,
                sha256: r.get(1)?,
            })
        })
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    let mut identities = HashSet::new();
    for asset in &result {
        safe_id(&asset.id)?;
        if !identities.insert(asset.id.to_ascii_lowercase()) {
            return Err("Snapshot Original identities collide on Windows.".into());
        }
    }
    Ok(result)
}
fn validate_audio(path: &Path) -> Result<(), String> {
    let info = conversion::inspect(path)?;
    if info.format == "WAVE" {
        let spec = managed::validate_wav(path)?;
        if spec.frames == 0 {
            return Err("Snapshot Original contains no audio.".into());
        }
    } else {
        let mut decoded = tempfile::tempfile().map_err(io)?;
        conversion::decode_original(path, &mut decoded)?;
    }
    Ok(())
}
fn validate_graph(graph: &patches::PatchGraph, samples: &HashSet<String>) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut unique = |id: &str| -> Result<(), String> {
        safe_id(id)?;
        if !ids.insert(id.to_ascii_lowercase()) {
            return Err("Snapshot contains duplicate Patch, Kit or Layer identities.".into());
        }
        Ok(())
    };
    if !graph.selected_patch.is_empty() && graph.current().is_none() {
        return Err("Snapshot selected Patch is missing.".into());
    }
    for patch in &graph.patches {
        unique(&patch.id)?;
        if patch.name.trim().is_empty()
            || !patch.bpm.is_finite()
            || !(30. ..=300.).contains(&patch.bpm)
            || patch.voices.len() != 4
            || !(patch.voice_kits.is_empty() || patch.voice_kits.len() == 4)
        {
            return Err("Snapshot has invalid Patch musical state.".into());
        }
        for voice in &patch.voices {
            if voice.steps.len() != 16
                || voice.velocities.len() != 16
                || voice.velocities.iter().any(|v| !(1..=127).contains(v))
                || !voice.volume.is_finite()
                || !(0. ..=1.).contains(&voice.volume)
                || !voice.kit_id.is_empty()
                || !voice.sample_path.is_empty()
            {
                return Err("Snapshot contains invalid or nonportable Voice state.".into());
            }
        }
        if (!patch.selected_kit.is_empty() && patch.kit().is_none())
            || patch
                .voice_kits
                .iter()
                .any(|id| !patch.kits.iter().any(|k| &k.id == id))
        {
            return Err("Snapshot references a missing Kit.".into());
        }
        let mut addresses = HashSet::new();
        for kit in &patch.kits {
            unique(&kit.id)?;
            if kit.bank.len() != 1
                || !kit.bank.as_bytes()[0].is_ascii_uppercase()
                || kit.number > 99
                || !addresses.insert((&kit.bank, kit.number))
                || kit.voices.len() != 4
            {
                return Err(
                    "Snapshot contains an invalid Kit structure or duplicate address.".into(),
                );
            }
            for voice in &kit.voices {
                if !voice.selected_layer.is_empty()
                    && !voice.layers.iter().any(|l| l.id == voice.selected_layer)
                {
                    return Err("Snapshot references a missing selected Layer.".into());
                }
                for layer in &voice.layers {
                    unique(&layer.id)?;
                    if !samples.contains(&layer.sample_id) {
                        return Err("Snapshot Layer references a missing Sample.".into());
                    }
                }
            }
        }
    }
    Ok(())
}
fn capture(store: &ManagedLibrary, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err("The backup destination already exists. Choose a new folder name.".into());
    }
    let parent = destination.parent().ok_or("Choose a backup location.")?;
    let stage = tempfile::Builder::new()
        .prefix(".rample-snapshot-")
        .tempdir_in(parent)
        .map_err(io)?;
    plain_directory(&store.root.join("Originals"))?;
    fs::create_dir(stage.path().join("Originals")).map_err(io)?;
    let db_path = stage.path().join("library.sqlite3");
    // SQLite produces one consistent, self-contained image while the managed lock
    // excludes all application mutations; no WAL/SHM files are copied.
    store
        .db
        .execute("VACUUM INTO ?1", [db_path.to_string_lossy().as_ref()])
        .map_err(io)?;
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&db_path)
        .and_then(|f| f.sync_all())
        .map_err(io)?;
    let originals = assets(store)?;
    for asset in &originals {
        let name = format!("{}.wav", asset.id);
        let target = stage.path().join("Originals").join(&name);
        copy_regular(&store.root.join("Originals").join(&name), &target)?;
        validate_audio(&target)?;
        if managed::fingerprint(&target)? != asset.sha256 {
            return Err(
                "An Original changed or could not be verified. The backup was not published."
                    .into(),
            );
        }
    }
    let manifest = Manifest {
        version: 1,
        database_sha256: managed::fingerprint(&db_path)?,
        originals,
    };
    validate_schema(&db_path)?;
    {
        let verified = ManagedLibrary {
            root: stage.path().to_owned(),
            db: Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(io)?,
        };
        validate_store(&verified, &manifest)?;
    }
    for asset in &manifest.originals {
        read_only(
            &stage
                .path()
                .join("Originals")
                .join(format!("{}.wav", asset.id)),
        )?;
    }
    write_synced(
        &stage.path().join("manifest.json"),
        &serde_json::to_vec_pretty(&manifest).map_err(io)?,
    )?;
    sync_directory(&stage.path().join("Originals"))?;
    sync_directory(stage.path())?;
    // Reserve the destination without replacing an existing directory on either OS.
    // Publish the completion manifest last; interrupted publication is never a backup.
    fs::create_dir(destination).map_err(io)?;
    for name in ["Originals", "library.sqlite3", "manifest.json"] {
        move_new(&stage.path().join(name), &destination.join(name))?;
    }
    checkpoint(RestoreCheckpoint::AfterRecoveryPublication)?;
    sync_directory(destination)?;
    sync_directory(parent)
}
type SchemaObject = (String, String, String, Option<String>);
fn schema(db: &Connection) -> Result<Vec<SchemaObject>, String> {
    let mut query = db
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
        .map_err(io)?;
    let result = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io)?;
    Ok(result)
}
fn validate_schema(path: &Path) -> Result<(), String> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(io)?;
    db.execute_batch("PRAGMA trusted_schema=OFF;").map_err(io)?;
    let version: u32 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(io)?;
    if version != 8 {
        return Err("Unsupported Library snapshot database version. Update Rample Preview or choose a supported backup.".into());
    }
    let reference = tempfile::tempdir().map_err(io)?;
    let known = ManagedLibrary::open(reference.path())?;
    legacy_migration::initialize(&known)?;
    if schema(&db)? != schema(&known.db)? {
        return Err("Snapshot contains an unsupported database schema.".into());
    }
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(io)?;
    if integrity != "ok" {
        return Err("Snapshot database integrity check failed.".into());
    }
    let mut query = db.prepare("PRAGMA foreign_key_check").map_err(io)?;
    if query.query([]).map_err(io)?.next().map_err(io)?.is_some() {
        return Err("Snapshot database contains broken Library references.".into());
    }
    Ok(())
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate_store(store: &ManagedLibrary, manifest: &Manifest) -> Result<(), String> {
    let stored = assets(store)?;
    if stored.len() != manifest.originals.len()
        || stored
            .iter()
            .zip(&manifest.originals)
            .any(|(a, b)| a.id != b.id || a.sha256 != b.sha256 || !valid_hash(&a.sha256))
    {
        return Err("Snapshot manifest does not match its Library Originals.".into());
    }
    let samples = store.list()?;
    let mut sample_ids = HashSet::new();
    let mut portable_sample_ids = HashSet::new();
    let mut source_specs = HashMap::new();
    for sample in &samples {
        safe_id(&sample.id)?;
        if !portable_sample_ids.insert(sample.id.to_ascii_lowercase()) {
            return Err("Snapshot Sample identities collide on Windows.".into());
        }
        safe_id(&sample.original_id)?;
        if sample.name.trim().is_empty() || !sample.readable {
            return Err("Snapshot contains an invalid or unavailable Sample.".into());
        }
        sample_ids.insert(sample.id.clone());
        let path = store
            .root
            .join("Originals")
            .join(format!("{}.wav", sample.original_id));
        let info = conversion::inspect(&path)?;
        let spec = if info.format == "WAVE" {
            managed::validate_wav(&path)?
        } else {
            let mut decoded = tempfile::tempfile().map_err(io)?;
            let info = conversion::decode_original(&path, &mut decoded)?;
            wav::WavSpec {
                rate: info.rate,
                channels: info.channels,
                bits: info.bits,
                frames: info.frames,
                code: 1,
                format_tag: 1,
                data_offset: 44,
                block_align: info.channels * 2,
            }
        };
        if spec.rate != sample.rate
            || spec.channels != sample.channels
            || spec.bits != sample.bits
            || (spec.frames as f64 / spec.rate as f64 - sample.duration).abs() > 1e-8
            || (if info.format == "WAVE" {
                "WAV"
            } else {
                &info.format
            }) != sample.format
        {
            return Err("Snapshot Original metadata does not match its audio.".into());
        }
        source_specs.insert(sample.id.clone(), spec);
    }
    let invalid: i64 = store
        .db
        .query_row(
            "SELECT count(*) FROM samples WHERE retired NOT IN (0,1)",
            [],
            |r| r.get(0),
        )
        .map_err(io)?;
    if invalid != 0 {
        return Err("Snapshot contains invalid Trash state.".into());
    }
    let mut revisions = HashMap::new();
    let mut portable_revisions = HashSet::new();
    let mut query = store
        .db
        .prepare("SELECT id,sample_id,settings FROM sample_revisions")
        .map_err(io)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(io)?;
    for row in rows {
        let (id, sample, settings) = row.map_err(io)?;
        safe_id(&id)?;
        if !portable_revisions.insert(id.to_ascii_lowercase()) {
            return Err("Snapshot Revision identities collide on Windows.".into());
        }
        let spec = source_specs
            .get(&sample)
            .ok_or("Snapshot Revision references a missing Sample.")?;
        let settings: rample_core::editing::Settings =
            serde_json::from_str(&settings).map_err(io)?;
        settings.validate(spec)?;
        if id.starts_with("original-")
            && (id != format!("original-{sample}")
                || settings
                    != rample_core::editing::Settings::original(
                        spec.frames as f64 / spec.rate as f64,
                    ))
        {
            return Err("Snapshot Original Revision has an invalid recipe.".into());
        }
        revisions.insert(id, sample);
    }
    let mut query = store
        .db
        .prepare("SELECT sample_id,revision_id FROM sample_current")
        .map_err(io)?;
    for row in query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(io)?
    {
        let (sample, revision) = row.map_err(io)?;
        if revisions.get(&revision) != Some(&sample) {
            return Err("Snapshot current Revision belongs to another Sample.".into());
        }
    }
    let mut query = store
        .db
        .prepare("SELECT sha256,sample_id,revision_id,render,recovered_id FROM exported_audio")
        .map_err(io)?;
    for row in query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(io)?
    {
        let (hash, sample, revision, render, recovered) = row.map_err(io)?;
        if !valid_hash(&hash)
            || revisions.get(&revision) != Some(&sample)
            || !["rample-v1", "edit-v1"].contains(&render.as_str())
            || recovered.as_ref().is_some_and(|id| {
                samples.iter().find(|s| &s.id == id).map(|s| &s.original_id)
                    != samples
                        .iter()
                        .find(|s| s.id == sample)
                        .map(|s| &s.original_id)
            })
        {
            return Err("Snapshot contains broken export recognition references.".into());
        }
    }
    let graph = store.load_patches()?;
    validate_graph(&graph, &sample_ids)?;
    let patch_ids: HashSet<_> = graph.patches.iter().map(|p| p.id.clone()).collect();
    let mut native = HashSet::new();
    let mut query = store
        .db
        .prepare("SELECT sha256,content,length(content) FROM sd_native_blobs")
        .map_err(io)?;
    for row in query
        .query_map([], |r| {
            let len: i64 = r.get(2)?;
            if len > 16 * 1024 * 1024 {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })
        .map_err(io)?
    {
        let (hash, bytes) = row.map_err(io)?;
        use sha2::Digest;
        if !valid_hash(&hash) || format!("{:x}", sha2::Sha256::digest(&bytes)) != hash {
            return Err("Snapshot native settings integrity check failed.".into());
        }
        native.insert(hash);
    }
    let mut intakes = HashSet::new();
    let mut drafts = vec![];
    let mut query = store
        .db
        .prepare("SELECT id,fingerprint,patch_id,draft FROM sd_intakes")
        .map_err(io)?;
    for row in query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(io)?
    {
        let (id, fingerprint, patch, draft) = row.map_err(io)?;
        safe_id(&id)?;
        if !valid_hash(&fingerprint) || patch.as_ref().is_some_and(|p| !patch_ids.contains(p)) {
            return Err("Snapshot contains a broken SD intake reference.".into());
        }
        let draft: patches::Patch = serde_json::from_str(&draft).map_err(io)?;
        let imported = draft
            .imported_sd
            .as_ref()
            .ok_or("Snapshot intake is missing its source evidence.")?;
        if imported.intake_id != id || imported.fingerprint != fingerprint {
            return Err("Snapshot SD intake identity does not match its evidence.".into());
        }
        intakes.insert(id);
        drafts.push(draft);
    }
    for patch in graph.patches.iter().chain(drafts.iter()) {
        validate_graph(
            &patches::PatchGraph {
                selected_patch: patch.id.clone(),
                patches: vec![patch.clone()],
                default_layer_mode: graph.default_layer_mode,
            },
            &sample_ids,
        )?;
        if let Some(imported) = &patch.imported_sd {
            if !intakes.contains(&imported.intake_id) || !valid_hash(&imported.fingerprint) {
                return Err("Snapshot Patch references a missing SD intake.".into());
            }
            let mut paths = HashSet::new();
            for file in &imported.files {
                if !paths.insert(&file.path) {
                    return Err("Snapshot SD evidence contains duplicate paths.".into());
                }
                if file
                    .sample_id
                    .as_ref()
                    .is_some_and(|id| !sample_ids.contains(id))
                    || (!file.sha256.is_empty() && !valid_hash(&file.sha256))
                    || (file.kind == "native" && !native.contains(&file.sha256))
                    || !["audio", "native", "directory", "unsupportedLink"]
                        .contains(&file.kind.as_str())
                {
                    return Err("Snapshot SD evidence contains a broken reference.".into());
                }
            }
            for file in &imported.unassigned {
                if !imported.files.iter().any(|f| {
                    f.path == file.path
                        && f.sha256 == file.sha256
                        && f.sample_id == file.sample_id
                        && f.kind == file.kind
                }) {
                    return Err("Snapshot unassigned source evidence is inconsistent.".into());
                }
            }
        }
    }
    let marker: String = store
        .db
        .query_row(
            "SELECT patch_id FROM legacy_migration WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "Snapshot has no complete managed musical workspace.".to_owned())?;
    if !patch_ids.contains(&marker) {
        return Err("Snapshot migration marker references a missing Patch.".into());
    }
    Ok(())
}
fn stage_snapshot(source: &Path, generation: &Path) -> Result<ManagedLibrary, String> {
    plain_directory(source)?;
    plain_directory(&source.join("Originals"))?;
    let manifest: Manifest = read_json(&source.join("manifest.json"))?;
    if manifest.version != 1 {
        return Err("This Library snapshot requires a different application version.".into());
    }
    fs::create_dir(generation).map_err(io)?;
    fs::create_dir(generation.join("Originals")).map_err(io)?;
    let database = generation.join("library.sqlite3");
    copy_regular(&source.join("library.sqlite3"), &database)?;
    if managed::fingerprint(&database)? != manifest.database_sha256 {
        return Err("Snapshot database integrity check failed.".into());
    }
    validate_schema(&database)?;
    for asset in &manifest.originals {
        safe_id(&asset.id)?;
        let name = format!("{}.wav", asset.id);
        let destination = generation.join("Originals").join(&name);
        copy_regular(&source.join("Originals").join(&name), &destination)?;
        validate_audio(&destination)?;
        if managed::fingerprint(&destination)? != asset.sha256 {
            return Err("Snapshot Original integrity check failed.".into());
        }
    }
    let store = ManagedLibrary::open_root(generation.to_owned())?;
    validate_store(&store, &manifest)?;
    for asset in &manifest.originals {
        read_only(
            &generation
                .join("Originals")
                .join(format!("{}.wav", asset.id)),
        )?;
    }
    sync_directory(&generation.join("Originals"))?;
    sync_directory(generation)?;
    sync_directory(generation.parent().ok_or("Missing generation boundary")?)?;
    Ok(store)
}
#[derive(Clone, Copy, PartialEq)]
enum RestoreCheckpoint {
    BeforeActivation,
    AfterActivation,
    AfterRecoveryPublication,
}
#[cfg(test)]
thread_local! { static INTERRUPT: std::cell::Cell<Option<RestoreCheckpoint>> = const { std::cell::Cell::new(None) }; }
fn checkpoint(phase: RestoreCheckpoint) -> Result<(), String> {
    #[cfg(test)]
    if INTERRUPT.with(|value| value.get()) == Some(phase) {
        return Err("Simulated application interruption".into());
    }
    let _ = phase;
    Ok(())
}
impl AppState {
    #[cfg(test)]
    fn restore_library_interrupted(
        &self,
        source: PathBuf,
        phase: RestoreCheckpoint,
    ) -> Result<SnapshotReply, String> {
        INTERRUPT.with(|value| value.set(Some(phase)));
        let result = self.restore_library(source);
        INTERRUPT.with(|value| value.set(None));
        result
    }
    pub fn library_recovery_snapshots(&self) -> Result<Vec<String>, String> {
        let legacy = self.library.lock().map_err(io)?;
        let recovery = legacy.support.join("Library/Recovery");
        if !recovery.exists() {
            return Ok(vec![]);
        }
        plain_directory(&recovery)?;
        let mut snapshots = vec![];
        for entry in fs::read_dir(&recovery).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            if Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err()
                || plain_directory(&path).is_err()
            {
                continue;
            }
            let Ok(manifest) = read_json::<Manifest>(&path.join("manifest.json")) else {
                continue;
            };
            if manifest.version != 1 {
                continue;
            }
            let created = fs::metadata(&path).and_then(|m| m.modified()).map_err(io)?;
            snapshots.push((
                created,
                dunce::canonicalize(&path)
                    .map_err(io)?
                    .to_string_lossy()
                    .into_owned(),
            ));
        }
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.0));
        Ok(snapshots.into_iter().map(|(_, path)| path).collect())
    }
    pub fn backup_library(&self, destination: PathBuf) -> Result<SnapshotReply, String> {
        let legacy = self.library.lock().map_err(io)?;
        let boundary = dunce::canonicalize(legacy.support.join("Library")).map_err(io)?;
        let parent =
            dunce::canonicalize(destination.parent().ok_or("Choose a backup destination.")?)
                .map_err(io)?;
        if parent.starts_with(&boundary)
            || boundary.starts_with(
                parent.join(destination.file_name().ok_or("Choose a new folder name.")?),
            )
        {
            return Err("Choose a backup destination outside the managed Library.".into());
        }
        let store = self.managed.lock().map_err(io)?;
        if self.migration.lock().map_err(io)?.status != "complete" {
            return Err("Move the folder session into the managed Library before creating a portable backup. This preserves its musical state.".into());
        }
        capture(&store, &destination)?;
        Ok(SnapshotReply::success(&destination))
    }
    pub fn restore_library(&self, source: PathBuf) -> Result<SnapshotReply, String> {
        let mut legacy = self.library.lock().map_err(io)?;
        let mut current = self.managed.lock().map_err(io)?;
        if self.migration.lock().map_err(io)?.status != "complete" {
            return Err("Move the folder session into the managed Library before replacing it, so its recovery backup is portable.".into());
        }
        plain_directory(&source)?;
        let source = dunce::canonicalize(&source).map_err(io)?;
        let boundary = dunce::canonicalize(legacy.support.join("Library")).map_err(io)?;
        if boundary.starts_with(&source)
            || (source.starts_with(&boundary) && !source.starts_with(boundary.join("Recovery")))
        {
            return Err("Choose a portable snapshot outside the active Library, or a Library recovery snapshot.".into());
        }
        ensure_directory(&boundary.join("Generations"))?;
        let generation = Uuid::new_v4().to_string();
        let root = boundary.join("Generations").join(&generation);
        let staged = stage_snapshot(&source, &root)?;
        ensure_directory(&boundary.join("Recovery"))?;
        let recovery = boundary.join("Recovery").join(Uuid::new_v4().to_string());
        let captured = capture(&current, &recovery);
        let mut reply = SnapshotReply::success(&source);
        if let Err(error) = captured {
            if !recovery.join("manifest.json").is_file() {
                return Err(error);
            }
            reply.complete = false;
            reply.recovery = Some(recovery.to_string_lossy().into_owned());
            reply.error = Some(format!("Library was not replaced. The verified recovery snapshot was published, but its final durability confirmation failed: {error}"));
            return Ok(reply);
        }
        reply.recovery = Some(recovery.to_string_lossy().into_owned());
        let activate = (|| -> Result<(), String> {
            checkpoint(RestoreCheckpoint::BeforeActivation)?;
            let records = boundary.join("Activations");
            ensure_directory(&records)?;
            let sequence = fs::read_dir(&records)
                .map_err(io)?
                .filter_map(Result::ok)
                .filter_map(|e| e.path().file_stem()?.to_str()?.parse::<u64>().ok())
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Library activation sequence exhausted")?;
            let record = Activation {
                version: 1,
                generation,
                recovery: recovery.to_string_lossy().into_owned(),
            };
            let mut temporary = tempfile::NamedTempFile::new_in(&records).map_err(io)?;
            temporary
                .write_all(&serde_json::to_vec(&record).map_err(io)?)
                .map_err(io)?;
            temporary.as_file().sync_all().map_err(io)?;
            publish_activation(temporary, &records.join(format!("{sequence:020}.json")))?;
            // Once published, every outcome reports activated=true. A later sync or
            // audio/UI error must never be described as an unchanged Library.
            reply.activated = true;
            *current = staged;
            legacy.editor = None;
            legacy.patch_preview = None;
            legacy.prepared.clear();
            legacy.banks = std::array::from_fn(|_| vec![]);
            legacy.catalog = Catalog::default();
            legacy.session = Session::default();
            legacy.playing = false;
            legacy.revision += 1;
            self.patch_active.store(false, Ordering::Relaxed);
            self.migration.lock().map_err(io)?.status = "complete".into();
            {
                let mut audition = self.original_audition.lock().map_err(io)?;
                audition.owner.clear();
                audition.generation += 1;
            }
            sync_directory(&records)?;
            checkpoint(RestoreCheckpoint::AfterActivation)?;
            self.audio.send(Command::Stop)?;
            self.audio.send(Command::Edit(None))?;
            self.audio.send(Command::OriginalAudition(None))?;
            for i in 0..4 {
                self.audio.send(Command::LayerBank(i, vec![]))?;
            }
            self.patch_action_locked(patches::PatchAction::Open, &mut legacy, &current)?;
            Ok(())
        })();
        if let Err(error) = activate {
            reply.complete = false;
            reply.error = Some(if reply.activated {
                format!("Library was restored, but final activation or playback refresh could not be confirmed: {error}. Reopen the application to reload it.")
            } else {
                format!("Library was not replaced: {error}")
            });
        }
        drop(current);
        drop(legacy);
        reply.snapshot = Some(self.snapshot());
        Ok(reply)
    }
}

#[tauri::command]
pub async fn library_recovery_snapshots(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<String>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.library_recovery_snapshots())
        .await
        .map_err(io)?
}
#[tauri::command]
pub async fn backup_library(
    destination: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SnapshotReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.backup_library(destination))
        .await
        .map_err(io)?
}
#[tauri::command]
pub async fn restore_library(
    source: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SnapshotReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.restore_library(source))
        .await
        .map_err(io)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{fixture, fixture_state};
    use patches::PatchAction;

    fn alter_database(backup: &Path, change: impl FnOnce(&rusqlite::Connection)) {
        let db = rusqlite::Connection::open(backup.join("library.sqlite3")).unwrap();
        db.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
        change(&db);
        drop(db);
        let path = backup.join("manifest.json");
        let mut manifest: Manifest = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest.database_sha256 = managed::fingerprint(&backup.join("library.sqlite3")).unwrap();
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }
    fn editor_values(info: editor::EditorInfo) -> (String, rample_core::editing::Settings) {
        let value = serde_json::to_value(info).unwrap();
        (
            value["id"].as_str().unwrap().into(),
            serde_json::from_value(value["settings"].clone()).unwrap(),
        )
    }
    #[test]
    fn portable_history_keeps_variants_native_intakes_repeated_layers_and_long_pcm_without_sources()
    {
        use sd_import::SdImportChoice;
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let stereo_source = temp.path().join("Stereo.flac");
        let hex: String = include_str!("../../crates/rample-core/tests/fixtures/stereo-flac.hex")
            .split_whitespace()
            .collect();
        let encoded: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        fs::write(&stereo_source, &encoded).unwrap();
        let stereo = state
            .import_samples(vec![stereo_source.clone()])
            .unwrap()
            .results[0]
            .sample
            .clone()
            .unwrap();
        let source = temp.path().join("Long Original.wav");
        wav::write_pcm16(&source, &vec![8192; 44100 * 4], 44100).unwrap();
        let sample = state.import_samples(vec![source.clone()]).unwrap().results[0]
            .sample
            .clone()
            .unwrap();
        state
            .update_sample_metadata(
                sample.id.clone(),
                "Long Café".into(),
                vec!["低音".into(), "drums".into()],
            )
            .unwrap();
        let view = state
            .patch_action(PatchAction::CreatePatch {
                name: "Portable set".into(),
            })
            .unwrap();
        let patch_id = view.graph.selected_patch;
        state
            .patch_action(PatchAction::CreateKit {
                bank: "Z".into(),
                number: 99,
            })
            .unwrap();
        for _ in 0..2 {
            state
                .patch_action(PatchAction::AddLayer {
                    channel: 0,
                    sample: sample.id.clone(),
                })
                .unwrap();
        }
        for (channel, mode) in [
            LayerMode::Random,
            LayerMode::Sequential,
            LayerMode::Reverse,
            LayerMode::Velocity,
        ]
        .into_iter()
        .enumerate()
        {
            state
                .patch_action(PatchAction::Musical {
                    action: Action::SetMode { channel, mode },
                })
                .unwrap();
        }
        state
            .patch_action(PatchAction::Musical {
                action: Action::SetBpm { value: 173. },
            })
            .unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::SetVolume {
                    channel: 0,
                    value: 0.5,
                },
            })
            .unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::SetVelocity {
                    channel: 0,
                    step: 3,
                    value: 53,
                },
            })
            .unwrap();
        let (id, mut settings) = editor_values(state.open_managed_editor(0).unwrap());
        settings.gain_db = -6.;
        state
            .save_managed_editor(id, settings.clone(), None)
            .unwrap();
        let export = temp.path().join("historical A.wav");
        state
            .export_sample(sample.id.clone(), export.clone())
            .unwrap();
        let (id, _) = editor_values(state.open_managed_editor(0).unwrap());
        settings.gain_db = -12.;
        state
            .save_managed_editor(id, settings.clone(), None)
            .unwrap();
        let (id, _) = editor_values(state.open_managed_editor(0).unwrap());
        settings.gain_db = -3.;
        state
            .save_managed_editor(id, settings.clone(), Some("Independent Variant".into()))
            .unwrap();
        state.retire_sample(sample.id.clone()).unwrap();
        let native_source = temp.path().join("opaque SD");
        fs::create_dir_all(native_source.join("A0")).unwrap();
        fs::create_dir(native_source.join("_save")).unwrap();
        fs::copy(&source, native_source.join("A0/1_Long.wav")).unwrap();
        fs::write(native_source.join("_save/A0.rpl"), [0, 255, 13, 10]).unwrap();
        let intake = state.prepare_sd_import(native_source.clone()).unwrap();
        let native_intake = intake.id.clone();
        state
            .finish_sd_import(
                &intake.id,
                SdImportChoice::New {
                    name: "Imported opaque Patch".into(),
                },
            )
            .unwrap();
        let pending_source = temp.path().join("pending SD");
        fs::create_dir_all(pending_source.join("_save")).unwrap();
        fs::write(pending_source.join("_save/global.rpl"), [9, 8, 7]).unwrap();
        let pending = state.prepare_sd_import(pending_source.clone()).unwrap().id;
        state
            .patch_action(PatchAction::SelectPatch {
                patch: patch_id.clone(),
            })
            .unwrap();
        let before = state.patch_action(PatchAction::Open).unwrap();
        let before_json = serde_json::to_value(&before.graph).unwrap();
        let (draft_id, mut unsaved) = editor_values(state.open_managed_editor(0).unwrap());
        unsaved.gain_db = -20.;
        state.preview_editor(draft_id, 1, unsaved, true).unwrap();
        let backup = temp.path().join("complete-portable");
        state.backup_library(backup.clone()).unwrap();
        assert!(!backup.join("Derived").exists() && !backup.join("Incoming").exists());
        assert_eq!(
            fs::read(
                backup
                    .join("Originals")
                    .join(format!("{}.wav", stereo.original_id))
            )
            .unwrap(),
            encoded
        );
        assert_eq!(fs::read(&stereo_source).unwrap(), encoded);
        fs::remove_file(stereo_source).unwrap();
        fs::remove_file(source).unwrap();
        fs::remove_dir_all(native_source).unwrap();
        fs::remove_dir_all(pending_source).unwrap();
        let support = temp.path().join("new-host");
        let (other, _) = fixture_state(support.clone());
        let mut other = Arc::try_unwrap(other).ok().unwrap();
        let (audio, mut renderer) = AudioHost::offline(44100);
        other.audio = audio;
        other.migrate_legacy().unwrap();
        let view = other.patch_action(PatchAction::Open).unwrap();
        let previous = view.graph.current().unwrap().kit().unwrap().voices[0].layers[0]
            .sample_id
            .clone();
        let owner = other.open_sample_audition().unwrap();
        other.audition_sample(owner.clone(), previous).unwrap();
        let (old_editor, mut old_settings) = editor_values(other.open_managed_editor(0).unwrap());
        old_settings.gain_db = -20.;
        other
            .preview_editor(old_editor.clone(), 1, old_settings.clone(), true)
            .unwrap();
        other
            .patch_action(PatchAction::Musical {
                action: Action::TogglePlayback,
            })
            .unwrap();
        renderer.render(&mut [0.; 256], 2);
        let old_revision = other.snapshot().revision;
        let restored = other.restore_library(backup).unwrap();
        assert!(restored.complete && restored.activated);
        assert!(restored.snapshot.unwrap().revision > old_revision);
        assert!(!other.snapshot().playing);
        assert!(other
            .preview_editor(old_editor.clone(), 2, old_settings.clone(), true)
            .is_err());
        assert!(other
            .save_managed_editor(old_editor, old_settings, None)
            .is_err());
        assert!(other.audition_sample(owner, sample.id.clone()).is_err());
        let view = other.patch_action(PatchAction::Open).unwrap();
        assert_eq!(serde_json::to_value(&view.graph).unwrap(), before_json);
        other
            .patch_action(PatchAction::Musical {
                action: Action::SetMode {
                    channel: 0,
                    mode: LayerMode::Single,
                },
            })
            .unwrap();
        other
            .patch_action(PatchAction::Musical {
                action: Action::Audition { channel: 0 },
            })
            .unwrap();
        let mut kit_pcm = vec![0.; 1024];
        renderer.render(&mut kit_pcm, 2);
        assert!(
            (kit_pcm[100] - 0.035397).abs() < 0.00003,
            "restored Kit PCM {}",
            kit_pcm[100]
        );
        let musical = view.session.unwrap();
        assert_eq!(
            (
                musical.bpm,
                musical.voices[0].volume,
                musical.voices[0].velocities[3]
            ),
            (173., 0.5, 53)
        );
        assert!(other
            .list_trashed_samples()
            .unwrap()
            .iter()
            .any(|s| s.id == sample.id && s.tags == ["低音", "drums"]));
        assert_eq!(
            other
                .sd_native_blob(&native_intake, "_save/A0.rpl")
                .unwrap(),
            [0, 255, 13, 10]
        );
        assert_eq!(
            other.sd_native_blob(&pending, "_save/global.rpl").unwrap(),
            [9, 8, 7]
        );
        other
            .finish_sd_import(
                &pending,
                SdImportChoice::New {
                    name: "Recovered pending draft".into(),
                },
            )
            .unwrap();
        other
            .patch_action(PatchAction::SelectPatch { patch: patch_id })
            .unwrap();
        let restored_stereo = other.prepare_library_sample(&stereo.id, false).unwrap();
        assert_eq!(
            (restored_stereo.spec.channels, restored_stereo.spec.rate),
            (2, 48000)
        );
        let original = other.prepare_library_sample(&sample.id, false).unwrap();
        let reimport = temp.path().join("renamed Original.wav");
        fs::copy(&original.path, &reimport).unwrap();
        let known = other.import_samples(vec![reimport]).unwrap();
        assert_eq!((known.imported, known.recognized), (0, 1));
        let a = other.import_samples(vec![export.clone()]).unwrap();
        let recovered = a.results[0].sample.as_ref().unwrap();
        assert_ne!(recovered.id, sample.id);
        assert_eq!(recovered.original_id, sample.original_id);
        let b = other.prepare_library_sample(&sample.id, true).unwrap();
        let historical = other.prepare_library_sample(&recovered.id, true).unwrap();
        assert!((b.head[100][0] - 0.062797).abs() < 0.0001);
        assert!((historical.head[100][0] - 0.125297).abs() < 0.0001);
        let recovered_id = recovered.id.clone();
        other
            .patch_action(PatchAction::Musical {
                action: Action::Stop,
            })
            .unwrap();
        let owner = other.open_sample_audition().unwrap();
        other.audition_sample(owner, sample.id.clone()).unwrap();
        let mut out = vec![0.; 882];
        for _ in 0..310 {
            renderer.render(&mut out, 2);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(out.iter().all(|v| (*v - 0.1).abs() < 1e-5));
        let caches = [b.path.clone(), historical.path.clone()];
        drop(other);
        drop(renderer);
        drop(b);
        drop(historical);
        // Readers release asynchronously. Use owned cache files and wait only for
        // their actual Windows handle release; the app never depends on this wait.
        for cache in caches {
            let mut removed = false;
            for _ in 0..1000 {
                if fs::remove_file(&cache).is_ok() {
                    removed = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(removed, "owned cache remained locked: {cache}");
        }
        let (restarted, _) = fixture_state(support);
        for _ in 0..2 {
            let again = restarted
                .import_samples(vec![
                    export.clone(),
                    temp.path().join("renamed Original.wav"),
                ])
                .unwrap();
            assert_eq!((again.imported, again.recognized), (0, 2));
            assert!(again
                .results
                .iter()
                .any(|r| r.sample.as_ref().is_some_and(|s| s.id == recovered_id)));
            assert!(
                (restarted
                    .prepare_library_sample(&sample.id, true)
                    .unwrap()
                    .head[100][0]
                    - 0.062797)
                    .abs()
                    < 0.0001
            );
        }
        let reopened = serde_json::to_value(restarted.open_managed_editor(0).unwrap()).unwrap();
        assert_eq!(reopened["settings"]["gainDb"], -3.);
    }

    #[test]
    fn restored_originals_remain_read_only_and_held_windows_files_need_no_rename() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let sample = state.list_samples().unwrap()[0].id.clone();
        let previous = state.prepare_library_sample(&sample, false).unwrap();
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let held = options.open(&previous.path).unwrap();
        let backup = temp.path().join("held-windows-file");
        state.backup_library(backup.clone()).unwrap();
        assert!(state.restore_library(backup).unwrap().complete);
        let restored = state.prepare_library_sample(&sample, false).unwrap();
        assert_ne!(restored.path, previous.path);
        assert!(Path::new(&previous.path).exists());
        assert!(fs::metadata(&restored.path)
            .unwrap()
            .permissions()
            .readonly());
        let boundary = dunce::canonicalize(temp.path().join("support/Library")).unwrap();
        assert!(dunce::canonicalize(&restored.path)
            .unwrap()
            .starts_with(boundary));
        drop(held);
        assert!(state
            .act(Action::Root {
                path: temp
                    .path()
                    .join("support/Library/Generations")
                    .to_string_lossy()
                    .into_owned()
            })
            .is_err());
    }

    #[test]
    fn restore_rejects_revision_identities_that_would_share_a_windows_cache_path() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let samples = state.list_samples().unwrap();
        let first = state.sample_revisions(samples[0].id.clone()).unwrap()[0]
            .id
            .clone();
        let second = state.sample_revisions(samples[1].id.clone()).unwrap()[0]
            .id
            .clone();
        let backup = temp.path().join("case-collision");
        state.backup_library(backup.clone()).unwrap();
        alter_database(&backup, |db| {
            for (old, new, gain) in [(&first, "Same", -6.), (&second, "same", -12.)] {
                db.execute("UPDATE sample_revisions SET id=?2,settings=json_set(settings,'$.gainDb',?3) WHERE id=?1",rusqlite::params![old,new,gain]).unwrap();
                db.execute(
                    "UPDATE sample_current SET revision_id=?2 WHERE revision_id=?1",
                    [old, new],
                )
                .unwrap();
            }
        });
        assert!(state.restore_library(backup).is_err());
    }

    #[test]
    fn restore_accepts_recovery_snapshots_but_rejects_other_active_boundary_nesting() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("portable-source");
        state.backup_library(backup.clone()).unwrap();
        let generations = temp.path().join("support/Library/Generations");
        fs::create_dir(&generations).unwrap();
        let nested = generations.join("nested-snapshot");
        fs::rename(backup, &nested).unwrap();
        assert!(state.restore_library(nested).is_err());
        assert!(!state.list_samples().unwrap().is_empty());
    }

    #[test]
    fn backup_and_pre_restore_recovery_refuse_unusable_current_history() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("good");
        state.backup_library(backup.clone()).unwrap();
        let sample = state.list_samples().unwrap()[0].id.clone();
        state.sample_revisions(sample).unwrap();
        // Simulate external damage to an owned fixture, never a production database.
        let db = rusqlite::Connection::open(temp.path().join("support/Library/library.sqlite3"))
            .unwrap();
        db.execute_batch("UPDATE sample_revisions SET settings=json_set(settings,'$.gainDb',999)")
            .unwrap();
        drop(db);
        let invalid = temp.path().join("must-not-publish");
        assert!(state.backup_library(invalid.clone()).is_err());
        assert!(!invalid.join("manifest.json").exists());
        assert!(state.restore_library(backup).is_err());
        assert!(!state.list_samples().unwrap().is_empty());
    }

    #[test]
    fn restore_returns_a_published_recovery_path_even_if_its_final_sync_fails() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("restore-source");
        state.backup_library(backup.clone()).unwrap();
        state
            .patch_action(PatchAction::RenamePatch {
                name: "Previous Library".into(),
            })
            .unwrap();
        let reply = state
            .restore_library_interrupted(backup, RestoreCheckpoint::AfterRecoveryPublication)
            .unwrap();
        assert!(!reply.complete && !reply.activated);
        let recovery = reply.recovery.unwrap();
        assert_eq!(
            state
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Previous Library"
        );
        assert!(
            state
                .restore_library(PathBuf::from(recovery))
                .unwrap()
                .complete
        );
    }

    #[test]
    fn restore_reports_recovery_before_and_after_the_durable_activation_commit() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        state
            .patch_action(PatchAction::RenamePatch {
                name: "Incoming Library".into(),
            })
            .unwrap();
        let backup = temp.path().join("activation");
        state.backup_library(backup.clone()).unwrap();
        state
            .patch_action(PatchAction::RenamePatch {
                name: "Current Library".into(),
            })
            .unwrap();
        let failed = state
            .restore_library_interrupted(backup.clone(), RestoreCheckpoint::BeforeActivation)
            .unwrap();
        assert!(!failed.complete && !failed.activated);
        let recovery = failed.recovery.unwrap();
        assert_eq!(
            state
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Current Library"
        );
        assert!(
            state
                .restore_library(PathBuf::from(recovery))
                .unwrap()
                .complete
        );
        let refreshed_failed = state
            .restore_library_interrupted(backup, RestoreCheckpoint::AfterActivation)
            .unwrap();
        assert!(!refreshed_failed.complete && refreshed_failed.activated);
        assert!(refreshed_failed.error.unwrap().contains("restored"));
        let recovery = refreshed_failed.recovery.unwrap();
        drop(state);
        let (restarted, _) = fixture_state(temp.path().join("support"));
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Incoming Library"
        );
        assert!(
            restarted
                .restore_library(PathBuf::from(recovery))
                .unwrap()
                .complete
        );
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Current Library"
        );
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_rejects_windows_reserved_asset_names_even_on_macos() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("reserved-name");
        state.backup_library(backup.clone()).unwrap();
        let path = backup.join("manifest.json");
        let mut manifest: Manifest = read_json(&path).unwrap();
        let old = manifest.originals[0].id.clone();
        fs::rename(
            backup.join("Originals").join(format!("{old}.wav")),
            backup.join("Originals/CON.wav"),
        )
        .unwrap();
        alter_database(&backup, |db| {
            db.execute("UPDATE originals SET id='CON' WHERE id=?1", [&old])
                .unwrap();
            db.execute(
                "UPDATE samples SET original_id='CON' WHERE original_id=?1",
                [&old],
            )
            .unwrap();
        });
        manifest = read_json(&path).unwrap();
        manifest
            .originals
            .iter_mut()
            .find(|a| a.id == old)
            .unwrap()
            .id = "CON".into();
        manifest.originals.sort_by(|a, b| a.id.cmp(&b.id));
        fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(state.restore_library(backup).is_err());
    }

    #[test]
    fn restore_checks_unsafe_manifest_identities_and_database_asset_agreement() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("hostile-manifest");
        state.backup_library(backup.clone()).unwrap();
        let path = backup.join("manifest.json");
        let bytes = fs::read(&path).unwrap();
        for id in [
            "../escape",
            "C:\\escape",
            "\\server\\share",
            "CON",
            "nul",
            "original:stream",
            "safe/../../escape",
        ] {
            let mut manifest: Manifest = serde_json::from_slice(&bytes).unwrap();
            manifest.originals[0].id = id.into();
            fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
            assert!(
                state.restore_library(backup.clone()).is_err(),
                "accepted {id}"
            );
        }
        fs::write(&path, &bytes).unwrap();
        alter_database(&backup, |db| {
            db.execute_batch("UPDATE originals SET id='different-safe-id' WHERE id=(SELECT id FROM originals ORDER BY id LIMIT 1)").unwrap()
        });
        assert!(state.restore_library(backup).is_err());
        assert!(!state.list_samples().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn restore_refuses_symlinked_snapshot_files_and_backup_refuses_linked_originals() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("linked-backup");
        state.backup_library(backup.clone()).unwrap();
        let manifest: Manifest =
            serde_json::from_slice(&fs::read(backup.join("manifest.json")).unwrap()).unwrap();
        let asset = &manifest.originals[0];
        let path = backup.join("Originals").join(format!("{}.wav", asset.id));
        let actual = temp.path().join("actual.wav");
        fs::rename(&path, &actual).unwrap();
        std::os::unix::fs::symlink(&actual, &path).unwrap();
        assert!(state.restore_library(backup).is_err());
        let original = temp
            .path()
            .join("support/Library/Originals")
            .join(format!("{}.wav", asset.id));
        fs::remove_file(&original).unwrap();
        std::os::unix::fs::symlink(&actual, &original).unwrap();
        assert!(state
            .backup_library(temp.path().join("reject-linked-original"))
            .is_err());
    }

    #[test]
    fn backup_rejects_nested_or_existing_destinations_without_clobbering() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let nested = temp.path().join("support/Library/nested-backup");
        assert!(state.backup_library(nested).is_err());
        let existing = temp.path().join("existing");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep.txt"), b"keep me").unwrap();
        assert!(state.backup_library(existing.clone()).is_err());
        assert_eq!(fs::read(existing.join("keep.txt")).unwrap(), b"keep me");
    }

    #[test]
    fn restore_rejects_broken_revision_ownership_recipes_and_native_recognition_links() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        for sample in state.list_samples().unwrap() {
            state.sample_revisions(sample.id).unwrap();
        }
        for (index,sql) in [
            "UPDATE sample_current SET revision_id=(SELECT revision_id FROM sample_current ORDER BY sample_id LIMIT 1)",
            "UPDATE sample_revisions SET settings=json_set(settings,'$.gainDb',999)",
            "UPDATE sd_native_blobs SET content=x'01'",
            "UPDATE sd_intakes SET patch_id='missing-patch'",
        ].iter().enumerate() {
            let backup = temp.path().join(format!("bad-reference-{index}"));
            state.backup_library(backup.clone()).unwrap();
            alter_database(&backup,|db| {
                if index==2 { db.execute("INSERT INTO sd_native_blobs VALUES (?1,x'00')",["0".repeat(64)]).unwrap(); }
                db.execute_batch(sql).unwrap();
            });
            assert!(state.restore_library(backup).is_err(),"accepted {sql}");
            assert!(!state.list_samples().unwrap().is_empty());
        }
    }

    #[test]
    fn restore_rejects_unknown_sql_schema_and_newer_database_versions() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        for (index, sql) in [
            "CREATE TABLE unexpected (value TEXT)",
            "CREATE TRIGGER surprise AFTER INSERT ON samples BEGIN DELETE FROM samples; END",
            "PRAGMA user_version=999",
        ]
        .iter()
        .enumerate()
        {
            let backup = temp.path().join(format!("unsafe-{index}"));
            state.backup_library(backup.clone()).unwrap();
            alter_database(&backup, |db| db.execute_batch(sql).unwrap());
            assert!(state.restore_library(backup).is_err(), "accepted {sql}");
            assert!(!state.list_samples().unwrap().is_empty());
        }
    }

    #[test]
    fn restore_rejects_corrupt_graph_but_preserves_legal_incomplete_patch_drafts() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Empty draft".into(),
            })
            .unwrap();
        let backup = temp.path().join("drafts");
        state.backup_library(backup.clone()).unwrap();
        assert!(state.restore_library(backup.clone()).unwrap().complete);
        alter_database(&backup, |db| {
            let payload: String = db
                .query_row("SELECT payload FROM patch_state", [], |r| r.get(0))
                .unwrap();
            let mut graph: serde_json::Value = serde_json::from_str(&payload).unwrap();
            graph["patches"][0]["kits"][0]["voices"] = serde_json::json!([]);
            db.execute("UPDATE patch_state SET payload=?1", [graph.to_string()])
                .unwrap();
        });
        assert!(state.restore_library(backup).is_err());
        assert_eq!(
            state
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Empty draft"
        );
    }

    #[test]
    fn snapshot_rejects_self_consistent_but_truncated_audio_before_replacement() {
        let (temp, state, _) = fixture();
        state.migrate_legacy().unwrap();
        let backup = temp.path().join("damaged");
        state.backup_library(backup.clone()).unwrap();
        let manifest_path = backup.join("manifest.json");
        let mut manifest: Manifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        let original = backup
            .join("Originals")
            .join(format!("{}.wav", manifest.originals[0].id));
        let mut permissions = fs::metadata(&original).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() | 0o200);
        }
        #[cfg(windows)]
        {
            // This owned corruption fixture must clear the Windows read-only file
            // attribute. Unix above adds only owner-write, never world-write.
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
        }
        fs::set_permissions(&original, permissions).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&original)
            .unwrap()
            .set_len(100)
            .unwrap();
        let hash = managed::fingerprint(&original).unwrap();
        let db = rusqlite::Connection::open(backup.join("library.sqlite3")).unwrap();
        db.execute(
            "UPDATE originals SET sha256=?1 WHERE id=?2",
            [&hash, &manifest.originals[0].id],
        )
        .unwrap();
        drop(db);
        manifest.originals[0].sha256 = hash;
        manifest.database_sha256 = managed::fingerprint(&backup.join("library.sqlite3")).unwrap();
        fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        state
            .patch_action(PatchAction::RenamePatch {
                name: "Keep this music".into(),
            })
            .unwrap();
        assert!(state.restore_library(backup).is_err());
        assert_eq!(
            state
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Keep this music"
        );
        drop(state);
        let (restarted, _) = fixture_state(temp.path().join("support"));
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Keep this music"
        );
    }

    #[test]
    fn portable_snapshot_replaces_a_library_and_its_recovery_snapshot_is_usable() {
        let (temp, state, _) = fixture();
        assert_eq!(state.migrate_legacy().unwrap().status, "complete");
        state
            .patch_action(PatchAction::RenamePatch {
                name: "Portable music".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::SetBpm { value: 173. },
            })
            .unwrap();
        let sample = state.list_samples().unwrap()[0].clone();
        state
            .update_sample_metadata(sample.id.clone(), "Café kick".into(), vec!["低音".into()])
            .unwrap();
        state.retire_sample(sample.id.clone()).unwrap();
        let destination = temp.path().join("Portable backup");
        let saved = state.backup_library(destination.clone()).unwrap();
        assert!(saved.complete);
        let other_support = temp.path().join("other-machine");
        let (other, _) = fixture_state(other_support.clone());
        other.migrate_legacy().unwrap();
        other
            .patch_action(PatchAction::RenamePatch {
                name: "Replaced music".into(),
            })
            .unwrap();
        let restored = other.restore_library(destination).unwrap();
        assert!(restored.complete && restored.activated);
        let recovery = restored.recovery.unwrap();
        assert_eq!(other.list_trashed_samples().unwrap()[0].name, "Café kick");
        let patch = other.patch_action(PatchAction::Open).unwrap();
        assert_eq!(patch.graph.current().unwrap().name, "Portable music");
        assert_eq!(patch.session.unwrap().bpm, 173.);
        drop(other);
        let (restarted, _) = fixture_state(other_support);
        assert!(restarted.snapshot().managed);
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Portable music"
        );
        assert!(restarted
            .library_recovery_snapshots()
            .unwrap()
            .contains(&recovery));
        let recovered = restarted.restore_library(PathBuf::from(recovery)).unwrap();
        assert!(recovered.complete && recovered.activated);
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .current()
                .unwrap()
                .name,
            "Replaced music"
        );
    }
}
