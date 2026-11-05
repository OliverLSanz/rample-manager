//! Build Patch reconciles a destination folder or SD root with a Patch (ADR 0005).
//! It reviews additions, replacements and removals of owned Kit audio, then backs
//! up every affected file before publishing. Content outside canonical Kit
//! folders, including firmware and `_save`, is never written, removed or backed
//! up (ADR 0007).
use crate::{
    managed::{ManagedLibrary, StagedExport},
    *,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
};
use uuid::Uuid;

/// Hidden working folder inside the destination, removed after every build. If
/// the application is interrupted, it remains as visible, unrelated content.
const STAGING_PREFIX: &str = ".rample-build-";
const SUFFIX_LENGTH: usize = 32;
/// Folder in the application support directory holding destination backups.
pub(crate) const BACKUPS: &str = "Patch build backups";
const MANIFEST: &str = "restore.json";

/// Descriptive filename part: ASCII letters, digits and `-`, with every other run
/// of characters as one `_`. Layer order never depends on it.
fn safe_suffix(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '-' {
            c
        } else {
            '_'
        };
        if !(c == '_' && out.ends_with('_')) {
            out.push(c);
        }
    }
    let trimmed: String = out
        .trim_matches(['_', '-'])
        .chars()
        .take(SUFFIX_LENGTH)
        .collect();
    match trimmed.trim_end_matches(['_', '-']) {
        "" => "Sample".into(),
        suffix => suffix.into(),
    }
}
/// `<Voice>_<two-digit Layer position>_<suffix>.wav`. The Voice digit comes first,
/// as the Rample requires, and the fixed-width position decides the order under
/// both natural and plain byte comparison before the suffix is reached.
fn layer_filename(voice: usize, position: usize, name: &str) -> String {
    format!(
        "{}_{:02}_{}.wav",
        voice + 1,
        position + 1,
        safe_suffix(name)
    )
}
/// A Kit folder exactly as Build Patch names it: `A0` to `Z99`, no leading zero.
/// Other names, such as `A05` or `a0`, are unrelated content and stay untouched.
fn canonical_kit(name: &str) -> bool {
    catalog::is_kit_name(name)
        && name[1..]
            .parse::<u8>()
            .is_ok_and(|n| format!("{}{n}", &name[..1]) == name)
}
fn is_wav(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}
fn read_error(e: std::io::Error) -> String {
    format!("Could not read the destination: {e}")
}
/// Best effort: every file is already synced, and some removable-media
/// filesystems do not support syncing a directory handle. Published files are
/// validated by reading them back instead.
fn sync_directory(path: &Path) {
    #[cfg(unix)]
    let _ = fs::File::open(path).and_then(|f| f.sync_all());
    #[cfg(not(unix))]
    let _ = path;
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
/// Whether a folder holds nothing but removable-media metadata.
fn only_metadata(folder: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(folder).map_err(read_error)? {
        let name = entry.map_err(read_error)?.file_name();
        if !sd_import::is_system_entry(&name.to_string_lossy()) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Owned audio on a destination: a WAV file directly inside a canonical Kit folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationFile {
    /// Destination-relative path with `/` separators.
    pub path: String,
    pub sha256: String,
}
#[derive(Default)]
struct Scan {
    /// Canonical Kit folders at the destination root.
    folders: Vec<String>,
    files: Vec<DestinationFile>,
    /// Other non-metadata entries inside each Kit folder.
    others: HashMap<String, Vec<String>>,
    /// Every non-metadata name at the destination root.
    entries: Vec<String>,
    /// Non-canonical folders that SD Import reads as a Kit, such as `A05`,
    /// with the canonical Kit folder name it reads them as.
    aliases: Vec<(String, String)>,
    conflicts: Vec<String>,
    notices: Vec<String>,
}
fn scan(destination: &Path) -> Result<Scan, String> {
    let mut scan = Scan::default();
    for entry in fs::read_dir(destination).map_err(read_error)? {
        let entry = entry.map_err(read_error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if sd_import::is_system_entry(&name) {
            continue;
        }
        scan.entries.push(name.clone());
        if !canonical_kit(&name) {
            let folder = entry.file_type().map_err(read_error)?.is_dir();
            if let Some((bank, number)) = sd_import::address(&name).filter(|_| folder) {
                scan.aliases.push((name, format!("{bank}{number}")));
            } else if catalog::is_kit_name(&name.to_ascii_uppercase()) {
                scan.notices.push(format!(
                    "{name} is not a Kit folder name Build Patch uses (A0 to Z99), so it is left untouched."
                ));
            }
            continue;
        }
        if !entry.file_type().map_err(read_error)?.is_dir() {
            scan.conflicts.push(format!(
                "{name} is not a plain folder. Move or rename it before building onto this destination."
            ));
            continue;
        }
        scan.folders.push(name.clone());
        let mut others = vec![];
        for item in fs::read_dir(entry.path()).map_err(read_error)? {
            let item = item.map_err(read_error)?;
            let file = item.file_name().to_string_lossy().into_owned();
            if sd_import::is_system_entry(&file) {
                continue;
            }
            if is_wav(&file) && item.file_type().map_err(read_error)?.is_file() {
                scan.files.push(DestinationFile {
                    path: format!("{name}/{file}"),
                    sha256: managed::fingerprint(&item.path())
                        .map_err(|e| format!("Could not read {name}/{file}: {e}"))?,
                });
            } else {
                others.push(file);
            }
        }
        others.sort_by(|a, b| natord::compare(a, b));
        scan.others.insert(name, others);
    }
    scan.folders.sort_by(|a, b| natord::compare(a, b));
    scan.files.sort_by(|a, b| natord::compare(&a.path, &b.path));
    scan.entries.sort();
    scan.aliases.sort();
    scan.notices.sort();
    Ok(scan)
}

/// The reviewed difference between a Patch's complete output and a destination.
/// Applying it requires the same Patch output and the same destination state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReview {
    pub patch: String,
    pub patch_name: String,
    pub destination: String,
    pub kits: Vec<String>,
    /// The complete output in Layer order.
    pub files: Vec<BuiltFile>,
    pub additions: Vec<String>,
    pub replacements: Vec<String>,
    pub removals: Vec<String>,
    /// Destination files that already hold this Patch's current audio.
    pub unchanged: Vec<String>,
    /// Kit folders on the destination that are not part of the Patch.
    pub removed_kits: Vec<String>,
    /// Problems that block applying this review.
    pub conflicts: Vec<String>,
    pub notices: Vec<String>,
    /// Owned destination audio and Kit folders when reviewed.
    pub state: Vec<DestinationFile>,
    pub folders: Vec<String>,
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
struct Manifest {
    version: u32,
    id: String,
    created: u64,
    patch: String,
    patch_name: String,
    destination: String,
    /// `applying` until publication ends, then `complete` or `incomplete`.
    status: String,
    error: Option<String>,
    restored: Option<u64>,
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
impl Manifest {
    fn save(&self, folder: &Path) -> Result<(), String> {
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
    fn summary(&self, folder: &Path) -> BuildBackup {
        let written: HashSet<_> = self.written.iter().map(|f| f.path.as_str()).collect();
        let backed: HashSet<_> = self.backed_up.iter().map(|f| f.path.as_str()).collect();
        BuildBackup {
            id: self.id.clone(),
            created: self.created,
            patch_name: self.patch_name.clone(),
            destination: self.destination.clone(),
            status: self.status.clone(),
            error: self.error.clone(),
            restored: self.restored,
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
    pub id: String,
    /// Milliseconds since the Unix epoch.
    pub created: u64,
    pub patch_name: String,
    pub destination: String,
    /// `applying` (interrupted), `complete` or `incomplete`.
    pub status: String,
    pub error: Option<String>,
    /// When the destination was last restored from this backup.
    pub restored: Option<u64>,
    pub folder: String,
    pub added: Vec<String>,
    pub replaced: Vec<String>,
    pub removed: Vec<String>,
}

impl ManagedLibrary {
    fn review_build(&self, patch: &str, destination: &Path) -> Result<BuildReview, String> {
        let graph = self.load_patches()?;
        let patch = graph
            .patches
            .iter()
            .find(|p| p.id == patch)
            .ok_or("Patch no longer exists.")?;
        let mut issues = patches::kit_issues(patch);
        if patch.kits.is_empty() {
            issues.push("The Patch has no Kits. Create a Kit with a Sample in Voice 1.".into());
        }
        if !issues.is_empty() {
            return Err(format!(
                "Build Patch is blocked until the Patch is valid. Nothing was written.\n{}",
                issues.join("\n")
            ));
        }
        if !destination.is_dir() {
            return Err("Choose an existing folder or card for Build Patch.".into());
        }
        let names: HashMap<String, String> =
            self.list()?.into_iter().map(|s| (s.id, s.name)).collect();
        let mut kits: Vec<_> = patch.kits.iter().collect();
        kits.sort_by(|a, b| (&a.bank, a.number).cmp(&(&b.bank, b.number)));
        let mut files = vec![];
        for kit in &kits {
            let folder = format!("{}{}", kit.bank, kit.number);
            for (voice, arrangement) in kit.voices.iter().enumerate() {
                for (layer, occurrence) in arrangement.layers.iter().enumerate() {
                    let name = names
                        .get(&occurrence.sample_id)
                        .cloned()
                        .unwrap_or_default();
                    files.push(BuiltFile {
                        path: format!("{folder}/{}", layer_filename(voice, layer, &name)),
                        kit: folder.clone(),
                        voice: voice + 1,
                        layer: layer + 1,
                        sample_id: occurrence.sample_id.clone(),
                        sample_name: name,
                        // An unavailable Sample is reported by name when it is prepared.
                        revision_id: self
                            .sample_edits(&occurrence.sample_id)
                            .map(|r| r.id)
                            .unwrap_or_default(),
                    });
                }
            }
        }
        let kits: Vec<String> = kits
            .iter()
            .map(|k| format!("{}{}", k.bank, k.number))
            .collect();
        let found = scan(destination)?;
        let mut conflicts = found.conflicts.clone();
        for kit in &kits {
            for entry in &found.entries {
                if entry != kit && entry.eq_ignore_ascii_case(kit) {
                    conflicts.push(format!("{entry} has the same name as Kit folder {kit} on cards that ignore letter case. Move or rename it before building onto this destination."));
                }
            }
        }
        for file in &files {
            let name = &file.path[file.kit.len() + 1..];
            for other in found.others.get(&file.kit).into_iter().flatten() {
                if other.eq_ignore_ascii_case(name) {
                    conflicts.push(format!("{}/{other} is not an audio file Build Patch can replace. Move or rename it before building onto this destination.", file.kit));
                }
            }
        }
        let existing: HashMap<&str, &str> = found
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.sha256.as_str()))
            .collect();
        let planned: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
        let (mut additions, mut replacements, mut unchanged) = (vec![], vec![], vec![]);
        for file in &files {
            match existing.get(file.path.as_str()) {
                None => additions.push(file.path.clone()),
                Some(sha256) if self.is_current_export(sha256, file)? => {
                    unchanged.push(file.path.clone())
                }
                Some(_) => replacements.push(file.path.clone()),
            }
        }
        let removals: Vec<String> = found
            .files
            .iter()
            .filter(|f| !planned.contains(f.path.as_str()))
            .map(|f| f.path.clone())
            .collect();
        let removed_kits: Vec<String> = found
            .folders
            .iter()
            .filter(|f| !kits.contains(f))
            .cloned()
            .collect();
        let mut notices = found.notices.clone();
        let mut kept = vec![];
        for kit in &removed_kits {
            let others = &found.others[kit];
            if !others.is_empty() {
                kept.push(kit);
                notices.push(format!(
                    "{kit} is not in the Patch. Its audio is removed, but the folder is kept because it also contains {}.",
                    others.join(", ")
                ));
            }
        }
        // SD Import reads `A05` as Kit A5 and keeps only one folder per Kit,
        // so an alias next to a built or kept Kit folder would replace or hide
        // it when this destination is imported again.
        for (alias, kit) in &found.aliases {
            if kits.contains(kit) || kept.contains(&kit) {
                conflicts.push(format!("{alias} is read as Kit {kit} by SD Import, so it would collide with the Kit folder {kit} when this destination is imported. Move or rename {alias} before building onto this destination."));
            } else {
                notices.push(format!(
                    "{alias} is not a Kit folder name Build Patch uses (A0 to Z99), so it is left untouched."
                ));
            }
        }
        if let Some(imported) = &patch.imported_sd {
            if !imported.unassigned.is_empty() {
                notices.push(format!("{} unassigned SD Import occurrence(s) are not part of any Kit and were not built. Their audio stays in the Library.", imported.unassigned.len()));
            }
        }
        Ok(BuildReview {
            patch: patch.id.clone(),
            patch_name: patch.name.clone(),
            destination: destination.to_string_lossy().into_owned(),
            kits,
            files,
            additions,
            replacements,
            removals,
            unchanged,
            removed_kits,
            conflicts,
            notices,
            state: found.files,
            folders: found.folders,
        })
    }
    /// Whether `sha256` is a recognized export of this Layer's current Revision.
    fn is_current_export(&self, sha256: &str, file: &BuiltFile) -> Result<bool, String> {
        self.db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM exported_audio WHERE sha256=?1 AND sample_id=?2 AND revision_id=?3)",
                rusqlite::params![sha256, file.sample_id, file.revision_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())
    }
    /// Applies a review only if the Patch output and destination are unchanged.
    /// Errors returned here mean nothing on the destination was changed; a reply
    /// with `error` means publication started and the destination is incomplete.
    fn apply_build(&self, reviewed: &BuildReview, backups: &Path) -> Result<BuildReply, String> {
        let destination = PathBuf::from(&reviewed.destination);
        let current = self.review_build(&reviewed.patch, &destination)?;
        if current.files != reviewed.files || current.kits != reviewed.kits {
            return Err(
                "The Patch changed after this review. Review Build Patch again. Nothing was written."
                    .into(),
            );
        }
        if current != *reviewed {
            return Err("The destination changed after this review. Review Build Patch again. Nothing was written.".into());
        }
        if !current.conflicts.is_empty() {
            return Err(format!(
                "Build Patch cannot apply this review. Nothing was written.\n{}",
                current.conflicts.join("\n")
            ));
        }
        let mut reply = BuildReply {
            destination: current.destination.clone(),
            kits: current.kits.clone(),
            files: current.files.clone(),
            notices: current.notices.clone(),
            added: current.additions.len(),
            replaced: current.replacements.len(),
            removed: current.removals.len(),
            unchanged: current.unchanged.len(),
            backup: None,
            error: None,
        };
        let empty_kits = current
            .removed_kits
            .iter()
            .any(|kit| only_metadata(&destination.join(kit)).unwrap_or(false));
        if current.additions.is_empty()
            && current.replacements.is_empty()
            && current.removals.is_empty()
            && !empty_kits
        {
            return Ok(reply);
        }
        let staging_name = format!("{STAGING_PREFIX}{}", Uuid::new_v4());
        let staging = destination.join(&staging_name);
        fs::create_dir(&staging).map_err(|e| {
            format!("Could not write to the destination: {e}. Nothing was written.")
        })?;
        let result = self.reconcile(&current, &destination, &staging, backups);
        let cleanup = fs::remove_dir_all(&staging);
        let (manifest, folder) = result?;
        reply.error = manifest.error.clone();
        reply.backup = Some(manifest.summary(&folder));
        if let Err(e) = cleanup {
            reply.notices.push(format!(
                "The temporary folder {staging_name} could not be removed: {e}. Delete it before using the card."
            ));
        }
        sync_directory(&destination);
        Ok(reply)
    }
    /// Renders every changed Layer and backs up every affected file before the
    /// destination is touched. Returns the final backup record.
    fn reconcile(
        &self,
        review: &BuildReview,
        destination: &Path,
        staging: &Path,
        backups: &Path,
    ) -> Result<(Manifest, PathBuf), String> {
        let writes: Vec<&BuiltFile> = review
            .files
            .iter()
            .filter(|f| !review.unchanged.contains(&f.path))
            .collect();
        let mut staged = vec![];
        let mut failures = vec![];
        for file in &writes {
            match self.stage_export(&file.sample_id, staging) {
                Ok(export) => staged.push(export),
                Err(error) => failures.push(format!(
                    "{} Voice {} Layer {} ({}): {error}",
                    file.kit,
                    file.voice,
                    file.layer,
                    if file.sample_name.is_empty() {
                        "Unavailable Sample"
                    } else {
                        &file.sample_name
                    }
                )),
            }
        }
        if !failures.is_empty() {
            return Err(format!(
                "Build Patch could not prepare every Layer. Nothing was written.\n{}",
                failures.join("\n")
            ));
        }
        let id = Uuid::new_v4().to_string();
        let folder = backups.join(&id);
        let mut manifest = Manifest {
            version: 1,
            id,
            created: now(),
            patch: review.patch.clone(),
            patch_name: review.patch_name.clone(),
            destination: review.destination.clone(),
            status: "applying".into(),
            error: None,
            restored: None,
            written: writes
                .iter()
                .zip(&staged)
                .map(|(file, export)| DestinationFile {
                    path: file.path.clone(),
                    sha256: export.sha256.clone(),
                })
                .collect(),
            backed_up: vec![],
            metadata: vec![],
            new_folders: review
                .kits
                .iter()
                .filter(|kit| !review.folders.contains(kit))
                .cloned()
                .collect(),
            obsolete_folders: review.removed_kits.clone(),
        };
        if let Err(error) = back_up(review, destination, &folder, &mut manifest) {
            let _ = fs::remove_dir_all(&folder);
            return Err(format!("{error} Nothing was written."));
        }
        let registered = (|| {
            let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
            for export in &staged {
                export.register(&tx)?;
            }
            tx.commit().map_err(|e| e.to_string())
        })();
        if let Err(error) = registered {
            let _ = fs::remove_dir_all(&folder);
            return Err(format!(
                "Could not record the Build's audio for recognition: {error}. Nothing was written."
            ));
        }
        let mut done = 0;
        let total = writes.len() + review.removals.len();
        let published = publish(review, &manifest, &writes, staged, destination, &mut done);
        manifest.status = if published.is_ok() {
            "complete"
        } else {
            "incomplete"
        }
        .into();
        manifest.error = published.err().map(|error| {
            format!("Build Patch did not complete: {error}. {done} of {total} file changes were applied, so the destination is only partly updated. Every replaced or removed file is backed up: restore the destination from this backup, or fix the problem and review Build Patch again.")
        });
        if let Err(error) = manifest.save(&folder) {
            let note = format!(
                "The backup record at {} could not be updated: {error}.",
                folder.display()
            );
            manifest.error = Some(match manifest.error.take() {
                Some(e) => format!("{e} {note}"),
                None => format!("The Patch was built. {note} Its backup is listed as interrupted, but it can still restore the destination."),
            });
        }
        Ok((manifest, folder))
    }
}
/// Copies and verifies every file the review replaces or removes, then records
/// the backup before publication starts.
fn back_up(
    review: &BuildReview,
    destination: &Path,
    folder: &Path,
    manifest: &mut Manifest,
) -> Result<(), String> {
    let reviewed: HashMap<&str, &str> = review
        .state
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();
    let create = |path: &Path| {
        fs::create_dir_all(path)
            .map_err(|e| format!("Could not create the destination backup: {e}."))
    };
    create(folder.parent().unwrap_or(folder))?;
    fs::create_dir(folder).map_err(|e| format!("Could not create the destination backup: {e}."))?;
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
        manifest.backed_up.push(BackedUpFile {
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
            let file = format!("metadata/{:04}", manifest.metadata.len() + 1);
            let sha256 = copy_hashed(&destination.join(&path), &folder.join(&file))
                .map_err(|e| format!("Could not back up {path}: {e}."))?;
            manifest.metadata.push(BackedUpFile { path, sha256, file });
        }
    }
    sync_directory(&folder.join("files"));
    sync_directory(&folder.join("metadata"));
    // Verify the complete backup by reading every copy back.
    for backed in manifest.backed_up.iter().chain(&manifest.metadata) {
        if managed::fingerprint(&folder.join(&backed.file))
            .ok()
            .as_ref()
            != Some(&backed.sha256)
        {
            return Err(format!(
                "The backup of {} could not be verified.",
                backed.path
            ));
        }
    }
    manifest
        .save(folder)
        .map_err(|e| format!("Could not record the destination backup: {e}."))
}
/// Removes obsolete audio, writes new and replacement Layers, removes Kit folders
/// left with only metadata and validates every published file.
fn publish(
    review: &BuildReview,
    manifest: &Manifest,
    writes: &[&BuiltFile],
    staged: Vec<StagedExport>,
    destination: &Path,
    done: &mut usize,
) -> Result<(), String> {
    for path in &review.removals {
        fs::remove_file(destination.join(path))
            .map_err(|e| format!("Could not remove {path}: {e}"))?;
        *done += 1;
    }
    let hashes: Vec<_> = staged.iter().map(|s| s.sha256.clone()).collect();
    for (file, export) in writes.iter().zip(staged) {
        let folder = destination.join(&file.kit);
        if !folder.is_dir() {
            fs::create_dir(&folder)
                .map_err(|e| format!("Could not create Kit folder {}: {e}", file.kit))?;
        }
        let target = destination.join(&file.path);
        let written = if review.replacements.contains(&file.path) {
            export.file.persist(&target)
        } else {
            export.file.persist_noclobber(&target)
        };
        written.map_err(|e| format!("Could not write {}: {}", file.path, e.error))?;
        *done += 1;
    }
    for kit in &review.removed_kits {
        let folder = destination.join(kit);
        if !only_metadata(&folder)? {
            continue;
        }
        let prefix = format!("{kit}/");
        for backed in manifest
            .metadata
            .iter()
            .filter(|m| m.path.starts_with(&prefix))
        {
            fs::remove_file(destination.join(&backed.path))
                .map_err(|e| format!("Could not remove {}: {e}", backed.path))?;
        }
        // Not recursive: anything that was not backed up keeps the folder.
        fs::remove_dir(&folder)
            .map_err(|e| format!("Could not remove the empty Kit folder {kit}: {e}"))?;
    }
    for kit in &review.kits {
        sync_directory(&destination.join(kit));
    }
    // Validate the complete published output, not only the staged copies.
    for (file, sha256) in writes.iter().zip(hashes) {
        let path = destination.join(&file.path);
        managed::validate_wav(&path).map_err(|e| format!("{} is not valid: {e}", file.path))?;
        if managed::fingerprint(&path)? != sha256 {
            return Err(format!("{} changed while it was written", file.path));
        }
    }
    Ok(())
}

fn load_manifest(backups: &Path, id: &str) -> Result<(Manifest, PathBuf), String> {
    Uuid::parse_str(id).map_err(|_| "This destination backup does not exist.")?;
    let folder = backups.join(id);
    let bytes = fs::read(folder.join(MANIFEST))
        .map_err(|e| format!("Could not read the destination backup: {e}"))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Could not read the destination backup: {e}"))?;
    if manifest.version != 1 || manifest.id != id {
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
    found.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
    Ok(found)
}
/// The current SHA-256 of a destination file, or `None` when it is absent.
fn current_hash(destination: &Path, path: &str) -> Result<Option<String>, String> {
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
enum Step<'a> {
    Restore(&'a BackedUpFile),
    Remove(&'a str),
}
/// Returns a destination to its state before one Build: files it added are
/// removed and files it replaced or removed get their backed-up bytes back.
/// Only files that still hold that Build's output, or are absent, are changed.
fn restore_build(backups: &Path, id: &str, destination: &Path) -> Result<RestoreReply, String> {
    let (mut manifest, folder) = load_manifest(backups, id)?;
    if !destination.is_dir() {
        return Err("Choose the existing folder or card to restore.".into());
    }
    let damaged: Vec<&str> = manifest
        .backed_up
        .iter()
        .chain(&manifest.metadata)
        .filter(|b| managed::fingerprint(&folder.join(&b.file)).ok().as_ref() != Some(&b.sha256))
        .map(|b| b.path.as_str())
        .collect();
    if !damaged.is_empty() {
        return Err(format!(
            "The destination backup is damaged and cannot restore {}. Nothing was changed.",
            damaged.join(", ")
        ));
    }
    let originals: HashMap<&str, &BackedUpFile> = manifest
        .backed_up
        .iter()
        .map(|b| (b.path.as_str(), b))
        .collect();
    let written: HashMap<&str, &str> = manifest
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
    manifest.restored = Some(now());
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltFile {
    /// Destination-relative path, always with `/` separators.
    pub path: String,
    pub kit: String,
    pub voice: usize,
    pub layer: usize,
    pub sample_id: String,
    pub sample_name: String,
    /// The Sample Revision this Layer renders; part of the reviewed output.
    pub revision_id: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReply {
    pub destination: String,
    pub kits: Vec<String>,
    pub files: Vec<BuiltFile>,
    pub notices: Vec<String>,
    pub added: usize,
    pub replaced: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// The recoverable record of affected destination content, when anything changed.
    pub backup: Option<BuildBackup>,
    /// Set when publication started but did not complete.
    pub error: Option<String>,
}

impl AppState {
    fn support(&self) -> Result<PathBuf, String> {
        Ok(self
            .library
            .lock()
            .map_err(|_| "Library is unavailable")?
            .support
            .clone())
    }
    pub fn review_patch_build(
        &self,
        patch: String,
        destination: PathBuf,
    ) -> Result<BuildReview, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .review_build(&patch, &destination)
    }
    pub fn apply_patch_build(&self, review: BuildReview) -> Result<BuildReply, String> {
        let backups = self.support()?.join(BACKUPS);
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .apply_build(&review, &backups)
    }
    pub fn patch_build_backups(&self) -> Result<Vec<BuildBackup>, String> {
        list_backups(&self.support()?.join(BACKUPS))
    }
    pub fn restore_patch_build(
        &self,
        id: String,
        destination: PathBuf,
    ) -> Result<RestoreReply, String> {
        let backups = self.support()?.join(BACKUPS);
        // Serialized with builds, which hold the same lock.
        let _library = self.managed.lock().map_err(|_| "Library is unavailable")?;
        restore_build(&backups, &id, &destination)
    }
    /// Reviews and applies in one step, failing on an incomplete Build.
    #[cfg(test)]
    pub fn build_patch(&self, patch: String, destination: PathBuf) -> Result<BuildReply, String> {
        let review = self.review_patch_build(patch, destination)?;
        let reply = self.apply_patch_build(review)?;
        match &reply.error {
            Some(error) => Err(error.clone()),
            None => Ok(reply),
        }
    }
}
#[tauri::command]
pub async fn review_patch_build(
    patch: String,
    destination: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<BuildReview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.review_patch_build(patch, destination))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_patch_build(
    review: BuildReview,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<BuildReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.apply_patch_build(review))
        .await
        .map_err(|e| e.to_string())?
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
    use crate::patches::PatchAction;
    use crate::tests::fixture;
    use crate::*;
    use std::fs;

    fn import(state: &AppState, dir: &Path, name: &str, frames: &[i16]) -> String {
        let source = dir.join(format!("{name}.wav"));
        wav::write_pcm16(&source, frames, 44100).unwrap();
        state.import_samples(vec![source]).unwrap().results[0]
            .sample
            .as_ref()
            .unwrap()
            .id
            .clone()
    }
    fn run(state: &AppState, action: PatchAction) -> patches::PatchSnapshot {
        state.patch_action(action).unwrap()
    }
    fn kit(state: &AppState, bank: &str, number: u8) {
        run(
            state,
            PatchAction::CreateKit {
                bank: bank.into(),
                number,
            },
        );
    }
    fn add(state: &AppState, channel: usize, sample: &str) -> patches::PatchSnapshot {
        run(
            state,
            PatchAction::AddLayer {
                channel,
                sample: sample.into(),
            },
        )
    }
    /// Every file below `root`, destination-relative with `/` separators, sorted.
    fn tree(root: &Path) -> Vec<String> {
        let mut files: Vec<_> = walkdir::WalkDir::new(root)
            .min_depth(1)
            .into_iter()
            .map(|e| {
                let e = e.unwrap();
                let rel = e.path().strip_prefix(root).unwrap();
                let mut path = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                if e.file_type().is_dir() {
                    path.push('/');
                }
                path
            })
            .collect();
        files.sort();
        files
    }

    /// Every entry below `root` with its bytes (`None` for folders).
    fn contents(root: &Path) -> std::collections::BTreeMap<String, Option<Vec<u8>>> {
        tree(root)
            .into_iter()
            .map(|path| {
                let bytes = (!path.ends_with('/')).then(|| fs::read(root.join(&path)).unwrap());
                (path, bytes)
            })
            .collect()
    }
    fn write(root: &Path, path: &str, bytes: &[u8]) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn foreign_wav(root: &Path, path: &str, level: i16) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        wav::write_pcm16(&path, &[level; 4410], 44100).unwrap();
    }
    /// A hand-made card: Kits A0, C3 and C4, native settings, firmware and notes.
    fn populated_card(root: &Path) {
        foreign_wav(root, "A0/1_01_Kick.wav", 100);
        foreign_wav(root, "A0/1_Old.wav", 200);
        write(root, "A0/notes.txt", b"my notes");
        foreign_wav(root, "C3/1_Old.wav", 300);
        write(root, "C3/.DS_Store", b"finder");
        foreign_wav(root, "C4/2_Old.wav", 400);
        write(root, "C4/info.txt", b"keep me");
        foreign_wav(root, "A05/1_Odd.wav", 500);
        write(root, "_save/A0.rpl", b"device settings A0");
        write(root, "_save/C3.rpl", b"device settings C3");
        write(root, "rample.bin", b"firmware");
    }
    /// Patch "Live": A0 Voice 1 Kick, Voice 2 Snare; B1 Voice 1 Snare.
    fn live_patch(state: &AppState, dir: &Path) -> (String, String, String) {
        let kick = import(state, dir, "Kick", &[8192; 4410]);
        let snare = import(state, dir, "Snare", &[4096; 4410]);
        let patch = run(
            state,
            PatchAction::CreatePatch {
                name: "Live".into(),
            },
        )
        .graph
        .selected_patch;
        kit(state, "A", 0);
        add(state, 0, &kick);
        add(state, 1, &snare);
        kit(state, "B", 1);
        add(state, 0, &snare);
        (patch, kick, snare)
    }

    #[test]
    fn reviewing_a_populated_card_lists_additions_replacements_and_removals_without_writing() {
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);
        let before = contents(&card);

        let review = state.review_patch_build(patch, card.clone()).unwrap();

        assert_eq!(
            review.additions,
            vec!["A0/2_01_Snare.wav", "B1/1_01_Snare.wav"]
        );
        assert_eq!(review.replacements, vec!["A0/1_01_Kick.wav"]);
        assert_eq!(
            review.removals,
            vec!["A0/1_Old.wav", "C3/1_Old.wav", "C4/2_Old.wav"]
        );
        assert_eq!(review.removed_kits, vec!["C3", "C4"]);
        assert!(review.unchanged.is_empty());
        assert!(review.conflicts.is_empty(), "{:?}", review.conflicts);
        assert!(
            review.notices.iter().any(|n| n.contains("A05")),
            "{:?}",
            review.notices
        );
        assert!(
            review
                .notices
                .iter()
                .any(|n| n.contains("C4") && n.contains("info.txt")),
            "{:?}",
            review.notices
        );
        assert_eq!(contents(&card), before);
    }

    #[test]
    fn builds_every_kit_and_ordered_repeated_layer_with_deterministic_names() {
        let (temp, state, _) = fixture();
        let kick = import(&state, temp.path(), "Warm kick", &[8192; 4410]);
        let snare = import(&state, temp.path(), "Snare", &[4096; 4410]);
        let patch = run(
            &state,
            PatchAction::CreatePatch {
                name: "Live".into(),
            },
        )
        .graph
        .selected_patch;
        kit(&state, "B", 12);
        add(&state, 0, &snare);
        kit(&state, "A", 0);
        add(&state, 0, &kick);
        add(&state, 0, &snare);
        add(&state, 0, &kick);
        for _ in 0..11 {
            add(&state, 1, &snare);
        }
        add(&state, 1, &kick);
        add(&state, 3, &kick);
        let destination = temp.path().join("Card");
        fs::create_dir(&destination).unwrap();

        let reply = state.build_patch(patch, destination.clone()).unwrap();

        let mut expected = vec![
            "A0/".to_string(),
            "A0/1_01_Warm_kick.wav".into(),
            "A0/1_02_Snare.wav".into(),
            "A0/1_03_Warm_kick.wav".into(),
        ];
        expected.extend((1..=11).map(|i| format!("A0/2_{i:02}_Snare.wav")));
        expected.push("A0/2_12_Warm_kick.wav".into());
        expected.push("A0/4_01_Warm_kick.wav".into());
        expected.push("B12/".into());
        expected.push("B12/1_01_Snare.wav".into());
        assert_eq!(tree(&destination), expected);
        assert_eq!(reply.kits, vec!["A0", "B12"]);
        assert_eq!(reply.files.len(), 17);
        // The Rample orders Layers by filename. Natural and plain byte order both
        // reproduce the intended Layer order, including positions 10 to 12.
        let voice2: Vec<_> = expected
            .iter()
            .filter(|p| p.starts_with("A0/2_"))
            .cloned()
            .collect();
        let mut natural = voice2.clone();
        natural.sort_by(|a, b| natord::compare(a, b));
        let mut bytes = voice2.clone();
        bytes.sort();
        assert_eq!(natural, voice2);
        assert_eq!(bytes, voice2);
        for file in &reply.files {
            let spec = wav::inspect(&destination.join(&file.path)).unwrap();
            assert_eq!(
                (spec.channels, spec.bits, spec.rate, spec.code),
                (1, 16, 44100, 1)
            );
        }
    }

    #[test]
    fn invalid_drafts_are_reported_completely_and_nothing_is_written() {
        let (temp, state, _) = fixture();
        let kick = import(&state, temp.path(), "Kick", &[8192; 4410]);
        let patch = run(
            &state,
            PatchAction::CreatePatch {
                name: "Draft".into(),
            },
        )
        .graph
        .selected_patch;
        kit(&state, "A", 0);
        for _ in 0..13 {
            add(&state, 0, &kick);
        }
        kit(&state, "C", 7);
        add(&state, 2, &kick);
        let destination = temp.path().join("Card");
        fs::create_dir(&destination).unwrap();

        let error = state
            .build_patch(patch.clone(), destination.clone())
            .unwrap_err();

        assert!(error.contains("A0: Voice 1 has 13 Layers"), "{error}");
        assert!(error.contains("C7: Voice 1 needs a Sample."), "{error}");
        assert!(error.contains("Nothing was written"), "{error}");
        assert!(tree(&destination).is_empty());
        // Drafts keep every Layer; Build never truncates them.
        let graph = run(&state, PatchAction::Open).graph;
        let kits = &graph.current().unwrap().kits;
        assert_eq!(kits[0].voices[0].layers.len(), 13);
        let empty = run(
            &state,
            PatchAction::CreatePatch {
                name: "Empty".into(),
            },
        )
        .graph
        .selected_patch;
        assert!(state
            .build_patch(empty, destination.clone())
            .unwrap_err()
            .contains("no Kits"));
        assert!(tree(&destination).is_empty());
    }

    fn backups(temp: &Path) -> Vec<String> {
        match fs::read_dir(temp.join("support").join(super::BACKUPS)) {
            Ok(entries) => entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => vec![],
        }
    }

    #[test]
    fn applying_a_review_reconciles_the_card_and_backs_up_every_affected_file() {
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);
        let before = contents(&card);
        let review = state
            .review_patch_build(patch.clone(), card.clone())
            .unwrap();

        let reply = state.apply_patch_build(review).unwrap();

        assert_eq!(reply.error, None);
        assert_eq!(
            (reply.added, reply.replaced, reply.removed, reply.unchanged),
            (2, 1, 3, 0)
        );
        let after = contents(&card);
        assert_eq!(
            after.keys().cloned().collect::<Vec<_>>(),
            vec![
                "A0/",
                "A0/1_01_Kick.wav",
                "A0/2_01_Snare.wav",
                "A0/notes.txt",
                "A05/",
                "A05/1_Odd.wav",
                "B1/",
                "B1/1_01_Snare.wav",
                "C4/",
                "C4/info.txt",
                "_save/",
                "_save/A0.rpl",
                "_save/C3.rpl",
                "rample.bin",
            ]
        );
        for unrelated in [
            "A0/notes.txt",
            "A05/1_Odd.wav",
            "C4/info.txt",
            "_save/A0.rpl",
            "_save/C3.rpl",
            "rample.bin",
        ] {
            assert_eq!(after[unrelated], before[unrelated], "{unrelated}");
        }
        assert_ne!(after["A0/1_01_Kick.wav"], before["A0/1_01_Kick.wav"]);
        // Every replaced or removed file is backed up byte for byte, and nothing else.
        let backup = reply.backup.unwrap();
        assert_eq!(backup.status, "complete");
        assert_eq!(backup.replaced, vec!["A0/1_01_Kick.wav"]);
        assert_eq!(
            backup.removed,
            vec!["A0/1_Old.wav", "C3/1_Old.wav", "C4/2_Old.wav"]
        );
        assert_eq!(backup.added, vec!["A0/2_01_Snare.wav", "B1/1_01_Snare.wav"]);
        let mut saved: Vec<Vec<u8>> = contents(&Path::new(&backup.folder).join("files"))
            .into_values()
            .flatten()
            .collect();
        saved.sort();
        let mut expected: Vec<Vec<u8>> = [
            "A0/1_01_Kick.wav",
            "A0/1_Old.wav",
            "C3/1_Old.wav",
            "C4/2_Old.wav",
        ]
        .iter()
        .map(|p| before[*p].clone().unwrap())
        .collect();
        expected.sort();
        assert_eq!(saved, expected);
        assert!(Path::new(&backup.folder).join("restore.json").is_file());

        // The rebuilt card already matches the Patch: nothing changes or is backed up.
        let again = state
            .review_patch_build(patch.clone(), card.clone())
            .unwrap();
        assert!(again.additions.is_empty() && again.replacements.is_empty());
        assert!(again.removals.is_empty() && again.removed_kits == vec!["C4"]);
        assert_eq!(again.unchanged.len(), 3);
        let unchanged = state.apply_patch_build(again).unwrap();
        assert!(unchanged.backup.is_none());
        assert_eq!(contents(&card), after);
        assert_eq!(backups(temp.path()).len(), 1);

        assert!(state
            .review_patch_build(patch, temp.path().join("Missing"))
            .is_err());
        assert!(!temp.path().join("Missing").exists());
    }

    #[test]
    fn changes_after_review_are_refused_and_nothing_is_written() {
        let (temp, state, _) = fixture();
        let (patch, kick, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);

        // Native settings and other unrelated content may change: they are not reviewed.
        let review = state
            .review_patch_build(patch.clone(), card.clone())
            .unwrap();
        write(&card, "_save/A0.rpl", b"stored again on the Rample");
        let before = contents(&card);
        let review_again = state
            .review_patch_build(patch.clone(), card.clone())
            .unwrap();
        assert_eq!(review, review_again);

        foreign_wav(&card, "C3/1_Old.wav", 301);
        let changed = contents(&card);
        let error = state.apply_patch_build(review.clone()).unwrap_err();
        assert!(error.contains("destination changed"), "{error}");
        assert!(error.contains("Nothing was written"), "{error}");
        assert_eq!(contents(&card), changed);

        fs::write(
            card.join("C3/1_Old.wav"),
            before["C3/1_Old.wav"].as_ref().unwrap(),
        )
        .unwrap();
        foreign_wav(&card, "D9/1_New.wav", 600);
        let error = state.apply_patch_build(review.clone()).unwrap_err();
        assert!(error.contains("destination changed"), "{error}");
        fs::remove_dir_all(card.join("D9")).unwrap();

        add(&state, 2, &kick);
        let error = state.apply_patch_build(review).unwrap_err();
        assert!(error.contains("Patch changed"), "{error}");
        assert_eq!(contents(&card), before);
        assert!(backups(temp.path()).is_empty());
    }

    #[test]
    fn a_backup_that_cannot_be_created_leaves_the_card_unchanged() {
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);
        let before = contents(&card);
        write(
            &temp.path().join("support"),
            super::BACKUPS,
            b"not a folder",
        );
        let review = state.review_patch_build(patch, card.clone()).unwrap();

        let error = state.apply_patch_build(review).unwrap_err();

        assert!(error.contains("destination backup"), "{error}");
        assert!(error.contains("Nothing was written"), "{error}");
        assert_eq!(contents(&card), before);
    }

    #[test]
    fn names_that_collide_by_letter_case_block_the_build() {
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        foreign_wav(&card, "a0/1_Kick.wav", 100);
        fs::create_dir_all(card.join("B1/1_01_Snare.wav")).unwrap();
        let before = contents(&card);

        let review = state.review_patch_build(patch, card.clone()).unwrap();

        assert_eq!(review.conflicts.len(), 2, "{:?}", review.conflicts);
        assert!(review.conflicts[0].starts_with("a0 has the same name as Kit folder A0"));
        assert!(review.conflicts[1].starts_with("B1/1_01_Snare.wav is not an audio file"));
        let error = state.apply_patch_build(review).unwrap_err();
        assert!(error.contains("Nothing was written"), "{error}");
        assert_eq!(contents(&card), before);
    }

    #[test]
    fn leading_zero_kit_folders_that_import_as_a_built_or_kept_kit_block_the_build() {
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let destination = temp.path().join("Card");
        populated_card(&destination);
        // SD Import reads B01 as Kit B1 and C04 as Kit C4 (kept for info.txt).
        foreign_wav(&destination, "B01/1_Stale.wav", 800);
        foreign_wav(&destination, "C04/1_Stale.wav", 900);
        let before = contents(&destination);

        let review = state
            .review_patch_build(patch.clone(), destination.clone())
            .unwrap();

        assert_eq!(review.conflicts.len(), 2, "{:?}", review.conflicts);
        assert!(review.conflicts[0].starts_with("B01 is read as Kit B1"));
        assert!(review.conflicts[1].starts_with("C04 is read as Kit C4"));
        // A05 maps to A5, which is neither built nor kept: only a notice.
        assert!(review.notices.iter().any(|n| n.starts_with("A05 ")));
        let error = state.apply_patch_build(review).unwrap_err();
        assert!(error.contains("Nothing was written"), "{error}");
        assert_eq!(contents(&destination), before);

        // Once the stale folders are moved away, reimporting the Build finds
        // every built Kit, with no duplicate Bank/Kit address.
        fs::remove_dir_all(destination.join("B01")).unwrap();
        fs::remove_dir_all(destination.join("C04")).unwrap();
        state.build_patch(patch, destination.clone()).unwrap();
        let reimport = state.prepare_sd_import(destination).unwrap();
        let issues = &reimport.draft.imported_sd.as_ref().unwrap().issues;
        assert!(
            !issues.iter().any(|i| i.contains("Duplicate")),
            "{issues:?}"
        );
        let b1 = reimport
            .draft
            .kits
            .iter()
            .find(|k| k.bank == "B" && k.number == 1)
            .unwrap();
        assert_eq!(b1.voices[0].layers.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_write_reports_an_incomplete_build_with_its_backup() {
        use std::os::unix::fs::PermissionsExt;
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);
        foreign_wav(&card, "B1/1_01_Snare.wav", 700);
        let before = contents(&card);
        let review = state.review_patch_build(patch, card.clone()).unwrap();
        fs::set_permissions(card.join("B1"), fs::Permissions::from_mode(0o555)).unwrap();

        let reply = state.apply_patch_build(review).unwrap();

        fs::set_permissions(card.join("B1"), fs::Permissions::from_mode(0o755)).unwrap();
        let error = reply.error.unwrap();
        assert!(
            error.starts_with("Build Patch did not complete: Could not write B1/1_01_Snare.wav"),
            "{error}"
        );
        assert!(
            error.contains("5 of 6 file changes were applied"),
            "{error}"
        );
        let backup = reply.backup.unwrap();
        assert_eq!(backup.status, "incomplete");
        assert_eq!(backup.error.as_deref(), Some(error.as_str()));
        assert_eq!(
            backup.replaced,
            vec!["A0/1_01_Kick.wav", "B1/1_01_Snare.wav"]
        );
        assert_eq!(state.patch_build_backups().unwrap()[0].status, "incomplete");

        // The partly updated card is restored to its reviewed state.
        let restored = state.restore_patch_build(backup.id, card.clone()).unwrap();
        assert_eq!((restored.restored, restored.removed), (4, 1));
        assert_eq!(contents(&card), before);
    }

    #[test]
    fn restoring_a_destination_backup_returns_the_card_to_its_reviewed_state() {
        let (temp, state, _) = fixture();
        let (patch, kick, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);
        let original = contents(&card);
        let first = state.build_patch(patch.clone(), card.clone()).unwrap();
        let first_card = contents(&card);
        // A second Build adds D2 and replaces the Kick its first Build wrote.
        kit(&state, "D", 2);
        add(&state, 0, &kick);
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let second = state.build_patch(patch.clone(), card.clone()).unwrap();
        assert_eq!((second.added, second.replaced), (1, 1));
        let listed = state.patch_build_backups().unwrap();
        assert_eq!(
            listed.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
            vec![
                second.backup.clone().unwrap().id,
                first.backup.clone().unwrap().id
            ]
        );
        assert_eq!(listed[1].patch_name, "Live");
        assert_eq!(listed[1].status, "complete");

        // The older Build's files were changed by the newer one: never overwritten.
        let blocked = state
            .restore_patch_build(first.backup.clone().unwrap().id, card.clone())
            .unwrap_err();
        assert!(blocked.contains("changed after this Build"), "{blocked}");
        assert!(blocked.contains("Nothing was changed"), "{blocked}");

        let newer = state
            .restore_patch_build(second.backup.unwrap().id, card.clone())
            .unwrap();
        assert_eq!((newer.restored, newer.removed), (1, 1));
        assert_eq!(contents(&card), first_card);
        let older = state
            .restore_patch_build(first.backup.clone().unwrap().id, card.clone())
            .unwrap();
        assert_eq!((older.restored, older.removed), (4, 2));
        assert_eq!(contents(&card), original);
        assert!(older.backup.restored.is_some());
        // Restoring again is harmless.
        state
            .restore_patch_build(first.backup.unwrap().id, card.clone())
            .unwrap();
        assert_eq!(contents(&card), original);
    }

    #[test]
    fn restore_refuses_files_changed_since_the_build_and_damaged_backups() {
        let (temp, state, _) = fixture();
        let (patch, _, _) = live_patch(&state, temp.path());
        let card = temp.path().join("Card");
        populated_card(&card);
        let built = state.build_patch(patch, card.clone()).unwrap();
        let backup = built.backup.unwrap();
        foreign_wav(&card, "B1/1_01_Snare.wav", 900);
        let edited = contents(&card);

        let error = state
            .restore_patch_build(backup.id.clone(), card.clone())
            .unwrap_err();

        assert!(
            error.contains("B1/1_01_Snare.wav changed after this Build"),
            "{error}"
        );
        assert_eq!(contents(&card), edited);
        fs::remove_file(card.join("B1/1_01_Snare.wav")).unwrap();
        let copy = fs::read_dir(Path::new(&backup.folder).join("files"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(copy, b"damaged").unwrap();
        let damaged = contents(&card);
        let error = state
            .restore_patch_build(backup.id, card.clone())
            .unwrap_err();
        assert!(error.contains("damaged"), "{error}");
        assert_eq!(contents(&card), damaged);
        assert!(state
            .restore_patch_build("../escape".into(), card.clone())
            .is_err());
    }

    #[test]
    fn built_audio_matches_kit_preview_with_saved_edits_applied_once_and_minimum_length() {
        let (temp, state, _) = fixture();
        let kick = import(&state, temp.path(), "Kick", &[8192; 4410]);
        // 10 ms: shorter than the Rample's 50 ms minimum.
        let blip = import(&state, temp.path(), "Blip", &[8192; 441]);
        let patch = run(&state, PatchAction::CreatePatch { name: "Set".into() })
            .graph
            .selected_patch;
        kit(&state, "A", 0);
        add(&state, 0, &kick);
        add(&state, 1, &blip);
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let destination = temp.path().join("Card");
        fs::create_dir(&destination).unwrap();

        state.build_patch(patch, destination.clone()).unwrap();

        let built =
            wav::PreparedSample::load(&destination.join("A0/1_01_Kick.wav"), "built").unwrap();
        let preview = state.prepare_library_sample(&kick, true).unwrap();
        assert_eq!(built.spec.frames, preview.spec.frames);
        assert_eq!(built.head, preview.head);
        // -6 dB of 8192 is about 4106 (PCM16 with dither), not 2058 (applied twice).
        let level = built.head[1000][0] * 32768.;
        assert!((level - 4106.).abs() <= 3., "{level}");
        let short =
            wav::PreparedSample::load(&destination.join("A0/2_01_Blip.wav"), "built").unwrap();
        assert_eq!(short.spec.frames, 2205);
        assert!((short.head[100][0] * 32768. - 8192.).abs() <= 1.);
        assert_eq!(short.head[2204][0], 0.);
    }

    #[test]
    fn reimporting_a_build_recognizes_every_layer_and_its_saved_edits() {
        let (temp, state, _) = fixture();
        let kick = import(&state, temp.path(), "Kick", &[8192; 4410]);
        let snare = import(&state, temp.path(), "Snare", &[4096; 4410]);
        let patch = run(&state, PatchAction::CreatePatch { name: "Set".into() })
            .graph
            .selected_patch;
        kit(&state, "A", 0);
        add(&state, 0, &snare);
        let first = add(&state, 0, &kick);
        add(&state, 0, &snare);
        add(&state, 2, &kick);
        let layer = first.graph.current().unwrap().kit().unwrap().voices[0].layers[1]
            .id
            .clone();
        run(&state, PatchAction::SelectLayer { channel: 0, layer });
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        kit(&state, "Z", 99);
        add(&state, 0, &kick);
        let originals = || {
            let mut files = fs::read_dir(temp.path().join("support/Library/Originals"))
                .unwrap()
                .map(|e| fs::read(e.unwrap().path()).unwrap())
                .collect::<Vec<_>>();
            files.sort();
            files
        };
        let before = originals();
        let destination = temp.path().join("Card");
        fs::create_dir(&destination).unwrap();
        state
            .build_patch(patch.clone(), destination.clone())
            .unwrap();
        let samples_before = state.list_samples().unwrap().len();

        let review = state.prepare_sd_import(destination.clone()).unwrap();

        assert_eq!((review.audio.imported, review.audio.recognized), (0, 5));
        assert_eq!(state.list_samples().unwrap().len(), samples_before);
        let arrangement = |kits: &[patches::PatchKit]| {
            let mut kits: Vec<_> = kits
                .iter()
                .map(|k| {
                    (
                        format!("{}{}", k.bank, k.number),
                        k.voices
                            .iter()
                            .map(|v| v.layers.iter().map(|l| l.sample_id.clone()).collect())
                            .collect::<Vec<Vec<_>>>(),
                    )
                })
                .collect();
            kits.sort();
            kits
        };
        let graph = run(&state, PatchAction::Open).graph;
        let built = graph.patches.iter().find(|p| p.id == patch).unwrap();
        assert_eq!(arrangement(&review.draft.kits), arrangement(&built.kits));
        // The edited Sample is recognized as itself, with its saved edits.
        let a0 = review.draft.kits.iter().find(|k| k.bank == "A").unwrap();
        assert_eq!(a0.voices[0].layers[1].sample_id, kick);
        let revisions = state.sample_revisions(kick).unwrap();
        assert_eq!(revisions.last().unwrap().settings.gain_db, -6.);
        assert_eq!(originals(), before);
    }

    #[test]
    fn a_layer_that_cannot_be_prepared_leaves_no_output_and_names_the_layer() {
        let (temp, state, _) = fixture();
        let kick = import(&state, temp.path(), "Kick", &[8192; 4410]);
        let snare = import(&state, temp.path(), "Snare", &[4096; 4410]);
        let patch = run(&state, PatchAction::CreatePatch { name: "Set".into() })
            .graph
            .selected_patch;
        kit(&state, "A", 0);
        add(&state, 0, &kick);
        add(&state, 1, &snare);
        let original = state
            .list_samples()
            .unwrap()
            .into_iter()
            .find(|s| s.id == snare)
            .unwrap()
            .original_id;
        let stored = temp
            .path()
            .join(format!("support/Library/Originals/{original}.wav"));
        let bytes = fs::read(&stored).unwrap();
        let moved = temp.path().join("kept.wav");
        fs::rename(&stored, &moved).unwrap();
        let destination = temp.path().join("Card");
        fs::create_dir(&destination).unwrap();

        let error = state
            .build_patch(patch.clone(), destination.clone())
            .unwrap_err();

        assert!(error.contains("A0 Voice 2 Layer 1 (Snare)"), "{error}");
        assert!(error.contains("Nothing was written"), "{error}");
        assert!(tree(&destination).is_empty(), "{:?}", tree(&destination));
        fs::rename(&moved, &stored).unwrap();
        assert_eq!(fs::read(&stored).unwrap(), bytes);
        state.build_patch(patch, destination.clone()).unwrap();
        assert_eq!(
            tree(&destination),
            vec!["A0/", "A0/1_01_Kick.wav", "A0/2_01_Snare.wav"]
        );
    }

    #[test]
    fn suffixes_are_safe_and_user_names_stay_unchanged() {
        let (temp, state, _) = fixture();
        let clap = import(&state, temp.path(), "Clap", &[8192; 4410]);
        let name = "  Clap / Rim: \u{e9}?  ";
        state
            .update_sample_metadata(clap.clone(), name.into(), vec![])
            .unwrap();
        let odd = import(&state, temp.path(), "Odd", &[2048; 4410]);
        state
            .update_sample_metadata(odd.clone(), "\u{2605}\u{2605}".into(), vec![])
            .unwrap();
        let patch = run(&state, PatchAction::CreatePatch { name: "Set".into() })
            .graph
            .selected_patch;
        kit(&state, "A", 0);
        add(&state, 0, &clap);
        add(&state, 0, &odd);
        let destination = temp.path().join("Card");
        fs::create_dir(&destination).unwrap();
        let reply = state.build_patch(patch, destination.clone()).unwrap();
        assert_eq!(
            tree(&destination),
            vec!["A0/", "A0/1_01_Clap_Rim.wav", "A0/1_02_Sample.wav"]
        );
        assert_eq!(reply.files[0].sample_name, name);
        let stored = state.list_samples().unwrap();
        assert_eq!(stored.iter().find(|s| s.id == clap).unwrap().name, name);
    }
}
