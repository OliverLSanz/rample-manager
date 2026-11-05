//! Build Patch writes Bank/Kit folders and ordered, prepared audio into an empty
//! destination. It never reads, creates or changes `_save` (ADR 0007).
use crate::{
    managed::{ManagedLibrary, StagedExport},
    *,
};
use std::fs;
use uuid::Uuid;

/// Hidden working folder inside the destination, removed after every build. If
/// the application is interrupted, it remains as visible evidence and keeps the
/// destination from being treated as empty or complete.
const STAGING_PREFIX: &str = ".rample-build-";
const SUFFIX_LENGTH: usize = 32;

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
fn existing_entries(destination: &Path, except: Option<&str>) -> Result<Vec<String>, String> {
    let mut found = vec![];
    for entry in
        fs::read_dir(destination).map_err(|e| format!("Could not read the destination: {e}"))?
    {
        let name = entry
            .map_err(|e| format!("Could not read the destination: {e}"))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if !sd_import::is_system_entry(&name) && Some(name.as_str()) != except {
            found.push(name);
        }
    }
    found.sort();
    Ok(found)
}
fn ensure_empty(destination: &Path, except: Option<&str>) -> Result<(), String> {
    let found = existing_entries(destination, except)?;
    if found.is_empty() {
        return Ok(());
    }
    let shown = found.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
    let more = match found.len() {
        0..=5 => String::new(),
        n => format!(" and {} more", n - 5),
    };
    Err(format!("Build Patch currently writes only to an empty folder or card. This destination already contains {shown}{more}. Nothing was written."))
}
#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    fs::File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
#[cfg(not(unix))]
fn sync_directory(_: &Path) -> Result<(), String> {
    Ok(())
}

impl ManagedLibrary {
    fn build_patch(&self, patch: &str, destination: &Path) -> Result<BuildReply, String> {
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
            return Err("Choose an existing empty folder or card for Build Patch.".into());
        }
        ensure_empty(destination, None)?;
        let names: HashMap<String, String> =
            self.list()?.into_iter().map(|s| (s.id, s.name)).collect();
        let mut kits: Vec<_> = patch.kits.iter().collect();
        kits.sort_by(|a, b| (&a.bank, a.number).cmp(&(&b.bank, b.number)));
        let mut files = vec![];
        for kit in kits {
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
                    });
                }
            }
        }
        let staging_name = format!("{STAGING_PREFIX}{}", Uuid::new_v4());
        let staging = destination.join(&staging_name);
        fs::create_dir(&staging).map_err(|e| format!("Could not write to the destination: {e}"))?;
        let result = self.stage_and_publish(&files, destination, &staging, &staging_name);
        let cleanup = fs::remove_dir_all(&staging);
        result?;
        cleanup.map_err(|e| {
            format!("The Patch was built, but its temporary folder {staging_name} could not be removed: {e}. Delete it before using the card.")
        })?;
        sync_directory(destination)?;
        let mut kits: Vec<String> = files.iter().map(|f| f.kit.clone()).collect();
        kits.dedup();
        let mut notices = vec![];
        if let Some(imported) = &patch.imported_sd {
            if !imported.unassigned.is_empty() {
                notices.push(format!("{} unassigned SD Import occurrence(s) are not part of any Kit and were not built. Their audio stays in the Library.", imported.unassigned.len()));
            }
        }
        Ok(BuildReply {
            destination: destination.to_string_lossy().into_owned(),
            kits,
            files,
            notices,
        })
    }
    /// Renders every Layer before anything is published, then publishes all Kit
    /// folders and recognition records together. On failure, published folders
    /// are removed and nothing is reported as built.
    fn stage_and_publish(
        &self,
        files: &[BuiltFile],
        destination: &Path,
        staging: &Path,
        staging_name: &str,
    ) -> Result<(), String> {
        let mut staged = vec![];
        let mut failures = vec![];
        for file in files {
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
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        for export in &staged {
            export.register(&tx)?;
        }
        ensure_empty(destination, Some(staging_name))?;
        let mut claimed = vec![];
        let published = publish(files, staged, destination, &mut claimed)
            .and_then(|_| tx.commit().map_err(|e| e.to_string()));
        if let Err(error) = published {
            let leftovers: Vec<_> = claimed
                .iter()
                .filter(|folder| fs::remove_dir_all(folder).is_err())
                .map(|folder| folder.to_string_lossy().into_owned())
                .collect();
            return Err(if leftovers.is_empty() {
                format!("Build Patch failed and its partial output was removed: {error}")
            } else {
                format!("Build Patch failed: {error}. Partial output could not be removed; delete these folders before using the card: {}", leftovers.join(", "))
            });
        }
        Ok(())
    }
}
fn publish(
    files: &[BuiltFile],
    staged: Vec<StagedExport>,
    destination: &Path,
    claimed: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let hashes: Vec<_> = staged.iter().map(|s| s.sha256.clone()).collect();
    for (file, export) in files.iter().zip(staged) {
        let folder = destination.join(&file.kit);
        if claimed.last() != Some(&folder) {
            // create_dir is the exclusive claim: an existing Kit is never reused.
            fs::create_dir(&folder)
                .map_err(|e| format!("Could not create Kit folder {}: {e}", file.kit))?;
            claimed.push(folder.clone());
        }
        export
            .file
            .persist_noclobber(destination.join(&file.path))
            .map_err(|e| format!("Could not write {}: {e}", file.path))?;
    }
    for folder in claimed.iter() {
        sync_directory(folder)?;
    }
    // Validate the complete published output, not only the staged copies.
    for (file, sha256) in files.iter().zip(hashes) {
        let path = destination.join(&file.path);
        managed::validate_wav(&path)?;
        if managed::fingerprint(&path)? != sha256 {
            return Err(format!("{} changed while it was written.", file.path));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltFile {
    /// Destination-relative path, always with `/` separators.
    pub path: String,
    pub kit: String,
    pub voice: usize,
    pub layer: usize,
    pub sample_id: String,
    pub sample_name: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReply {
    pub destination: String,
    pub kits: Vec<String>,
    pub files: Vec<BuiltFile>,
    pub notices: Vec<String>,
}

impl AppState {
    pub fn build_patch(&self, patch: String, destination: PathBuf) -> Result<BuildReply, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .build_patch(&patch, &destination)
    }
}
#[tauri::command]
pub async fn build_patch(
    patch: String,
    destination: PathBuf,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<BuildReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.build_patch(patch, destination))
        .await
        .map_err(|e| e.to_string())?
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

    #[test]
    fn only_empty_destinations_are_accepted_and_save_is_never_touched() {
        let (temp, state, _) = fixture();
        let kick = import(&state, temp.path(), "Kick", &[8192; 4410]);
        let patch = run(&state, PatchAction::CreatePatch { name: "Set".into() })
            .graph
            .selected_patch;
        kit(&state, "A", 0);
        add(&state, 0, &kick);
        let card = temp.path().join("Card");
        fs::create_dir_all(card.join("_save")).unwrap();
        fs::write(card.join("_save/A0.rpl"), b"device settings").unwrap();
        let error = state.build_patch(patch.clone(), card.clone()).unwrap_err();
        assert!(error.contains("_save"), "{error}");
        assert_eq!(tree(&card), vec!["_save/", "_save/A0.rpl"]);
        assert_eq!(
            fs::read(card.join("_save/A0.rpl")).unwrap(),
            b"device settings"
        );
        assert!(state
            .build_patch(patch.clone(), temp.path().join("Missing"))
            .is_err());
        assert!(!temp.path().join("Missing").exists());

        // Removable-media metadata does not make a fresh card non-empty.
        let fresh = temp.path().join("Fresh");
        fs::create_dir_all(fresh.join(".Spotlight-V100")).unwrap();
        fs::write(fresh.join(".DS_Store"), b"finder").unwrap();
        state.build_patch(patch, fresh.clone()).unwrap();
        assert_eq!(
            tree(&fresh),
            vec![".DS_Store", ".Spotlight-V100/", "A0/", "A0/1_01_Kick.wav"]
        );
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
