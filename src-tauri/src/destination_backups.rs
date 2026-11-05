//! Destination backups: the original bytes of every destination file one Build
//! Patch replaced or removed, with a `restore.json` record of what it wrote, so
//! that destination can be restored to its state before that Build (ADR 0005).
//! Only owned Kit audio and the metadata of Kit folders a Build removes are
//! backed up or restored; `_save` and other content are never touched (ADR 0007).
use crate::{
    build_patch::{BuildReview, DestinationFile},
    *,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
};
use uuid::Uuid;

/// Folder in the application support directory holding destination backups.
pub(crate) const BACKUPS: &str = "Patch build backups";
const MANIFEST: &str = "restore.json";

pub(crate) fn read_error(e: std::io::Error) -> String {
    format!("Could not read the destination: {e}")
}
/// Best effort: every file is already synced, and some removable-media
/// filesystems do not support syncing a directory handle. Published files are
/// validated by reading them back instead.
pub(crate) fn sync_directory(path: &Path) {
    #[cfg(unix)]
    let _ = fs::File::open(path).and_then(|f| f.sync_all());
    #[cfg(not(unix))]
    let _ = path;
}
/// Whether a folder holds nothing but removable-media metadata.
pub(crate) fn only_metadata(folder: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(folder).map_err(read_error)? {
        let name = entry.map_err(read_error)?.file_name();
        if !sd_import::is_system_entry(&name.to_string_lossy()) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// The current SHA-256 of a destination file, or `None` when it is absent.
pub(crate) fn current_hash(destination: &Path, path: &str) -> Result<Option<String>, String> {
    let target = destination.join(path);
    match fs::symlink_metadata(&target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(read_error(e)),
        Ok(meta) if meta.is_file() => managed::fingerprint(&target)
            .map(Some)
            .map_err(|e| format!("Could not read {path}: {e}")),
        Ok(_) => Err(format!("{path} is not a file.")),
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
/// Copies `source` to a new file at `target` and returns the SHA-256 of the bytes
/// read. The copy is synced before returning.
fn copy_hashed(source: &Path, target: &Path) -> Result<String, String> {
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut chunk = vec![0; 65536];
    loop {
        let read = input.read(&mut chunk).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
        output
            .write_all(&chunk[..read])
            .map_err(|e| e.to_string())?;
    }
    output.sync_all().map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hash.finalize()))
}
fn restore_file(backup: &Path, destination: &Path, path: &str) -> Result<(), String> {
    let target = destination.join(path);
    let parent = target.parent().ok_or("Invalid destination path.")?;
    fs::create_dir_all(parent).map_err(|e| format!("Could not restore {path}: {e}"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".rample-restore-")
        .tempfile_in(parent)
        .map_err(|e| format!("Could not restore {path}: {e}"))?;
    let mut input = File::open(backup).map_err(|e| format!("Could not restore {path}: {e}"))?;
    std::io::copy(&mut input, temporary.as_file_mut())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|e| format!("Could not restore {path}: {e}"))?;
    temporary
        .persist(&target)
        .map_err(|e| format!("Could not restore {path}: {}", e.error))?;
    Ok(())
}

/// Where one Build stands. Serialized as `applying`, `complete` or
/// `incomplete` in `restore.json` and replies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildStatus {
    /// Publication started and has not ended; if it stays so, the Build was
    /// interrupted and the destination may be partly updated.
    Applying,
    Complete,
    /// Publication stopped part way; the record says why.
    Incomplete,
}
/// What a Destination backup records about its Build, shared by `restore.json`
/// and the summaries shown to the user.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRecord {
    pub id: String,
    /// Milliseconds since the Unix epoch.
    pub created: u64,
    pub patch_name: String,
    pub destination: String,
    pub status: BuildStatus,
    pub error: Option<String>,
    /// When the destination was last restored from this backup.
    pub restored: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackedUpFile {
    path: String,
    sha256: String,
    /// Backup copy, relative to the backup folder.
    file: String,
}
/// `restore.json` in each destination backup folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    version: u32,
    #[serde(flatten)]
    pub(crate) record: BackupRecord,
    /// The Patch built; `patch` in manifests written before it was renamed.
    #[serde(alias = "patch")]
    patch_id: String,
    /// Files Build Patch writes, with the bytes it writes.
    written: Vec<DestinationFile>,
    /// Original bytes of every replaced or removed file.
    backed_up: Vec<BackedUpFile>,
    /// Removable-media metadata of obsolete Kit folders the build removes.
    #[serde(default)]
    metadata: Vec<BackedUpFile>,
    /// Kit folders that did not exist before the build.
    new_folders: Vec<String>,
    /// Kit folders not in the Patch when it was built.
    obsolete_folders: Vec<String>,
}
/// One change that restores a destination.
enum Step<'a> {
    Restore(&'a BackedUpFile),
    Remove(&'a str),
}
impl Manifest {
    /// A new record, `applying`, for a Build that writes `written`.
    pub(crate) fn new(review: &BuildReview, written: Vec<DestinationFile>) -> Self {
        Manifest {
            version: 1,
            record: BackupRecord {
                id: Uuid::new_v4().to_string(),
                created: now(),
                patch_name: review.patch_name.clone(),
                destination: review.output.destination.clone(),
                status: BuildStatus::Applying,
                error: None,
                restored: None,
            },
            patch_id: review.patch_id.clone(),
            written,
            backed_up: vec![],
            metadata: vec![],
            new_folders: review
                .output
                .kits
                .iter()
                .filter(|kit| !review.reviewed_kit_folders.contains(kit))
                .cloned()
                .collect(),
            obsolete_folders: review.removed_kits.clone(),
        }
    }
    pub(crate) fn save(&self, folder: &Path) -> Result<(), String> {
        let temporary = folder.join(format!("{MANIFEST}.tmp"));
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temporary, folder.join(MANIFEST)).map_err(|e| e.to_string())?;
        sync_directory(folder);
        Ok(())
    }
    /// Copies every file the review replaces or removes into `folder`, with
    /// the metadata of obsolete Kit folders the Build will remove, verifies
    /// every copy and records the backup before publication starts.
    pub(crate) fn back_up(
        &mut self,
        review: &BuildReview,
        destination: &Path,
        folder: &Path,
    ) -> Result<(), String> {
        let reviewed: HashMap<&str, &str> = review
            .reviewed_audio
            .iter()
            .map(|f| (f.path.as_str(), f.sha256.as_str()))
            .collect();
        let create = |path: &Path| {
            fs::create_dir_all(path)
                .map_err(|e| format!("Could not create the destination backup: {e}."))
        };
        create(folder.parent().unwrap_or(folder))?;
        fs::create_dir(folder)
            .map_err(|e| format!("Could not create the destination backup: {e}."))?;
        create(&folder.join("files"))?;
        let affected = review.replacements.iter().chain(&review.removals);
        for (n, path) in affected.enumerate() {
            let file = format!("files/{:04}.wav", n + 1);
            let sha256 = copy_hashed(&destination.join(path), &folder.join(&file))
                .map_err(|e| format!("Could not back up {path}: {e}."))?;
            if reviewed.get(path.as_str()) != Some(&sha256.as_str()) {
                return Err(format!(
                    "{path} changed after this review. Review Build Patch again."
                ));
            }
            self.backed_up.push(BackedUpFile {
                path: path.clone(),
                sha256,
                file,
            });
        }
        // Obsolete Kit folders left with only metadata are removed, so their
        // metadata files are affected content too.
        create(&folder.join("metadata"))?;
        let removals: HashSet<&str> = review.removals.iter().map(String::as_str).collect();
        for kit in &review.removed_kits {
            let mut metadata = vec![];
            let mut removable = true;
            for entry in fs::read_dir(destination.join(kit)).map_err(read_error)? {
                let entry = entry.map_err(read_error)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = format!("{kit}/{name}");
                if !sd_import::is_system_entry(&name) {
                    removable &= removals.contains(path.as_str());
                } else if entry.file_type().map_err(read_error)?.is_file() {
                    metadata.push(path);
                } else {
                    removable = false;
                }
            }
            if !removable {
                continue;
            }
            for path in metadata {
                let file = format!("metadata/{:04}", self.metadata.len() + 1);
                let sha256 = copy_hashed(&destination.join(&path), &folder.join(&file))
                    .map_err(|e| format!("Could not back up {path}: {e}."))?;
                self.metadata.push(BackedUpFile { path, sha256, file });
            }
        }
        sync_directory(&folder.join("files"));
        sync_directory(&folder.join("metadata"));
        // Verify the complete backup by reading every copy back.
        if let Some(path) = self.damaged_copies(folder).first() {
            return Err(format!("The backup of {path} could not be verified."));
        }
        self.save(folder)
            .map_err(|e| format!("Could not record the destination backup: {e}."))
    }
    /// Backed-up metadata files inside one Kit folder.
    pub(crate) fn metadata_in<'a>(&'a self, kit: &str) -> impl Iterator<Item = &'a str> {
        let prefix = format!("{kit}/");
        self.metadata
            .iter()
            .map(|m| m.path.as_str())
            .filter(move |path| path.starts_with(&prefix))
    }
    /// Destination paths whose backup copy in `folder` is missing or no
    /// longer holds the recorded bytes.
    fn damaged_copies(&self, folder: &Path) -> Vec<&str> {
        self.backed_up
            .iter()
            .chain(&self.metadata)
            .filter(|b| {
                managed::fingerprint(&folder.join(&b.file)).ok().as_ref() != Some(&b.sha256)
            })
            .map(|b| b.path.as_str())
            .collect()
    }
    /// The changes that return `destination` to its state before this Build.
    /// Files already holding their original bytes are skipped; any file that
    /// holds neither this Build's output nor its original bytes, nor is absent,
    /// refuses the whole restore.
    fn restore_plan(&self, destination: &Path) -> Result<Vec<Step<'_>>, String> {
        let originals: HashMap<&str, &BackedUpFile> = self
            .backed_up
            .iter()
            .map(|b| (b.path.as_str(), b))
            .collect();
        let written: HashMap<&str, &str> = self
            .written
            .iter()
            .map(|f| (f.path.as_str(), f.sha256.as_str()))
            .collect();
        let mut paths: Vec<&str> = originals.keys().chain(written.keys()).copied().collect();
        paths.sort_by(|a, b| natord::compare(a, b));
        paths.dedup();
        let mut steps = vec![];
        let mut conflicts = vec![];
        for path in paths {
            let current = match current_hash(destination, path) {
                Ok(current) => current,
                Err(error) => {
                    conflicts.push(error);
                    continue;
                }
            };
            let original = originals.get(path);
            if current.is_some() && current.as_deref() == original.map(|b| b.sha256.as_str()) {
                continue;
            }
            if current.is_some() && current.as_deref() != written.get(path).copied() {
                conflicts.push(format!("{path} changed after this Build."));
                continue;
            }
            match (original, current) {
                (Some(backed), _) => steps.push(Step::Restore(backed)),
                (None, Some(_)) => steps.push(Step::Remove(path)),
                (None, None) => {}
            }
        }
        if !conflicts.is_empty() {
            return Err(format!(
                "Restore would overwrite files that changed after this Build. Nothing was changed. Move them away, or restore the newer Build that changed them first.\n{}",
                conflicts.join("\n")
            ));
        }
        Ok(steps)
    }
    pub(crate) fn summary(&self, folder: &Path) -> BuildBackup {
        let written: HashSet<_> = self.written.iter().map(|f| f.path.as_str()).collect();
        let backed: HashSet<_> = self.backed_up.iter().map(|f| f.path.as_str()).collect();
        BuildBackup {
            record: self.record.clone(),
            folder: folder.to_string_lossy().into_owned(),
            added: self
                .written
                .iter()
                .filter(|f| !backed.contains(f.path.as_str()))
                .map(|f| f.path.clone())
                .collect(),
            replaced: self
                .backed_up
                .iter()
                .filter(|f| written.contains(f.path.as_str()))
                .map(|f| f.path.clone())
                .collect(),
            removed: self
                .backed_up
                .iter()
                .filter(|f| !written.contains(f.path.as_str()))
                .map(|f| f.path.clone())
                .collect(),
        }
    }
}
/// A recoverable record of one applied Build Patch.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildBackup {
    #[serde(flatten)]
    pub record: BackupRecord,
    pub folder: String,
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub removed: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReply {
    pub destination: String,
    /// Files that got their backed-up bytes back.
    pub restored: usize,
    /// Files the Build had added that were removed.
    pub removed: usize,
    pub backup: BuildBackup,
    pub notices: Vec<String>,
}

fn load_manifest(backups: &Path, id: &str) -> Result<(Manifest, PathBuf), String> {
    Uuid::parse_str(id).map_err(|_| "This destination backup does not exist.")?;
    let folder = backups.join(id);
    let bytes = fs::read(folder.join(MANIFEST))
        .map_err(|e| format!("Could not read the destination backup: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read the destination backup: {e}"))?;
    if manifest.version != 1 || manifest.record.id != id {
        return Err("This destination backup has an unsupported format.".into());
    }
    Ok((manifest, folder))
}
/// Every readable destination backup, newest first.
fn list_backups(backups: &Path) -> Result<Vec<BuildBackup>, String> {
    let entries = match fs::read_dir(backups) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("Could not read the destination backups: {e}")),
    };
    let mut found = vec![];
    for entry in entries {
        let entry = entry.map_err(|e| format!("Could not read the destination backups: {e}"))?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if let Ok((manifest, folder)) = load_manifest(backups, &id) {
            found.push(manifest.summary(&folder));
        }
    }
    found.sort_by(|a, b| {
        b.record
            .created
            .cmp(&a.record.created)
            .then(b.record.id.cmp(&a.record.id))
    });
    Ok(found)
}
/// Returns a destination to its state before one Build: files it added are
/// removed and files it replaced or removed get their backed-up bytes back.
/// Only files that still hold that Build's output, or are absent, are changed.
fn restore_build(backups: &Path, id: &str, destination: &Path) -> Result<RestoreReply, String> {
    let (mut manifest, folder) = load_manifest(backups, id)?;
    if !destination.is_dir() {
        return Err("Choose the existing folder or card to restore.".into());
    }
    let damaged = manifest.damaged_copies(&folder);
    if !damaged.is_empty() {
        return Err(format!(
            "The destination backup is damaged and cannot restore {}. Nothing was changed.",
            damaged.join(", ")
        ));
    }
    let steps = manifest.restore_plan(destination)?;
    let total = steps.len();
    let (mut restored, mut removed) = (0, 0);
    let applied = (|| {
        for step in &steps {
            match step {
                Step::Restore(backed) => {
                    restore_file(&folder.join(&backed.file), destination, &backed.path)?;
                    restored += 1;
                }
                Step::Remove(path) => {
                    fs::remove_file(destination.join(path))
                        .map_err(|e| format!("Could not remove {path}: {e}"))?;
                    removed += 1;
                }
            }
        }
        for backed in &manifest.metadata {
            if current_hash(destination, &backed.path)?.is_none() {
                restore_file(&folder.join(&backed.file), destination, &backed.path)?;
            }
        }
        for kit in &manifest.new_folders {
            let path = destination.join(kit);
            if path.is_dir() && only_metadata(&path)? {
                fs::remove_dir_all(&path)
                    .map_err(|e| format!("Could not remove the Kit folder {kit}: {e}"))?;
            }
        }
        sync_directory(destination);
        for backed in &manifest.backed_up {
            if current_hash(destination, &backed.path)?.as_ref() != Some(&backed.sha256) {
                return Err(format!("{} could not be verified", backed.path));
            }
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = applied {
        return Err(format!(
            "Restore did not complete: {error}. {} of {total} file changes were made. Restore again to finish; files already restored are skipped.",
            restored + removed
        ));
    }
    let mut notices = vec![];
    manifest.record.restored = Some(now());
    if let Err(error) = manifest.save(&folder) {
        notices.push(format!(
            "The destination was restored, but its backup record could not be updated: {error}."
        ));
    }
    Ok(RestoreReply {
        destination: destination.to_string_lossy().into_owned(),
        restored,
        removed,
        backup: manifest.summary(&folder),
        notices,
    })
}

impl AppState {
    /// The folder holding every Destination backup.
    pub(crate) fn destination_backups(&self) -> Result<PathBuf, String> {
        Ok(self
            .workspace
            .lock()
            .map_err(|_| "Library is unavailable")?
            .support
            .join(BACKUPS))
    }
    pub fn patch_build_backups(&self) -> Result<Vec<BuildBackup>, String> {
        list_backups(&self.destination_backups()?)
    }
    pub fn restore_patch_build(
        &self,
        id: String,
        destination: PathBuf,
    ) -> Result<RestoreReply, String> {
        let backups = self.destination_backups()?;
        // Serialized with builds, which hold the same lock.
        let _library = self.managed.lock().map_err(|_| "Library is unavailable")?;
        restore_build(&backups, &id, &destination)
    }
}
#[tauri::command]
pub async fn patch_build_backups(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<BuildBackup>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.patch_build_backups())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn restore_patch_build(
    id: String,
    destination: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<RestoreReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.restore_patch_build(id, destination))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn show_patch_build_backups(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    super::open_backup_folder(&state, BACKUPS)
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_statuses_keep_their_restore_json_names() {
        use super::BuildStatus::*;
        for (status, name) in [
            (Applying, "applying"),
            (Complete, "complete"),
            (Incomplete, "incomplete"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), name);
            assert_eq!(
                serde_json::from_value::<super::BuildStatus>(name.into()).unwrap(),
                status
            );
        }
    }
}
