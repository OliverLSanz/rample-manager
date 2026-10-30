//! Migration publishes a Patch only after verified private copies and selected playback are ready.
use crate::*;
use patches::{Layer, Patch, PatchKit, PatchVoice};
use rusqlite::OptionalExtension;
use std::io::Write;
use uuid::Uuid;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatus {
    pub status: String,
    pub processed: usize,
    pub total: usize,
    pub current: Option<String>,
    pub errors: Vec<String>,
    pub backup: Option<String>,
}
pub fn initialize(store: &managed::ManagedLibrary) -> Result<bool, String> {
    store.db.execute_batch("CREATE TABLE IF NOT EXISTS legacy_migration (id INTEGER PRIMARY KEY CHECK(id=1), patch_id TEXT NOT NULL);").map_err(|e| e.to_string())?;
    Ok(store
        .db
        .query_row(
            "SELECT patch_id FROM legacy_migration WHERE id=1",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .is_some())
}
pub fn backup(support: &Path) -> Result<Option<String>, String> {
    let source = support.join("session.json");
    if !source.exists() {
        return Ok(None);
    }
    let destination = support.join("session-before-managed.backup.json");
    if !destination.exists() {
        let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
        // A synced temporary file is published atomically; interruption never leaves
        // a partial backup that a retry could mistake for the original session.
        let mut file = tempfile::NamedTempFile::new_in(support).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist_noclobber(&destination)
            .map_err(|e| format!("Could not back up the folder session: {e}"))?;
    }
    if !destination.is_file() {
        return Err(
            "The folder-session backup path is not a file. Migration was not activated.".into(),
        );
    }
    Ok(Some(destination.to_string_lossy().into_owned()))
}
impl AppState {
    pub fn migration_status(&self) -> MigrationStatus {
        self.migration.lock().unwrap().clone()
    }
    pub fn migrate_legacy(&self) -> Result<MigrationStatus, String> {
        {
            let mut status = self
                .migration
                .lock()
                .map_err(|_| "Migration status unavailable")?;
            if status.status == "complete" || status.status == "working" {
                return Ok(status.clone());
            }
            *status = MigrationStatus {
                status: "working".into(),
                ..MigrationStatus::default()
            };
        }
        let result = self.migrate_legacy_inner();
        let mut status = self
            .migration
            .lock()
            .map_err(|_| "Migration status unavailable")?;
        status.current = None;
        status.processed = status.total;
        match result {
            Ok(()) => status.status = "complete".into(),
            Err(error) => {
                status.status = "failed".into();
                status.errors.push(error);
            }
        }
        let result = status.clone();
        drop(status);
        if result.status == "failed" {
            self.watch();
        }
        Ok(result)
    }
    fn migrate_legacy_inner(&self) -> Result<(), String> {
        let mut legacy = self.library.lock().map_err(|_| "Library is unavailable")?;
        let mut store = self.managed.lock().map_err(|_| "Library is unavailable")?;
        let backup = backup(&legacy.support)?;
        self.migration.lock().unwrap().backup = backup;
        // Scan on the control thread; copying and full validation stay bounded.
        legacy.scan(&HashSet::new(), true);
        if let Some(error) = &legacy.catalog.error {
            return Err(error.clone());
        }
        let old = legacy.session.clone();
        let review =
            store.prepare_sd_progress(Path::new(&old.root_path), |done, total, source| {
                let mut status = self.migration.lock().unwrap();
                status.processed = done;
                status.total = total;
                status.current = source.file_name().map(|s| s.to_string_lossy().into_owned());
            })?;
        let audio = review.audio;
        let imported_sd = review.draft.imported_sd;
        if imported_sd.as_ref().is_some_and(|sd| !sd.copy_complete) {
            let mut errors = imported_sd
                .as_ref()
                .map(|sd| sd.issues.clone())
                .unwrap_or_default();
            errors.extend(
                audio
                    .results
                    .iter()
                    .filter_map(|r| r.error.as_ref().map(|e| format!("{}: {e}", r.source))),
            );
            return Err(errors.join("\n"));
        }
        if !audio.complete {
            return Err(audio
                .results
                .iter()
                .filter_map(|r| r.error.as_ref().map(|e| format!("{}: {e}", r.source)))
                .collect::<Vec<_>>()
                .join("\n"));
        }
        let copied: HashMap<_, _> = audio
            .results
            .iter()
            .filter_map(|r| r.sample.as_ref().map(|s| (r.source.as_str(), s.id.clone())))
            .collect();
        let mut patch = Patch {
            id: Uuid::new_v4().to_string(),
            name: Path::new(&old.root_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            imported_sd,
            selected_kit: String::new(),
            kits: vec![],
            bpm: old.bpm,
            voices: old.voices.clone(),
            voice_kits: vec![],
        };
        let mut mapped = HashMap::new();
        for kit in &legacy.catalog.kits {
            let id = Uuid::new_v4().to_string();
            mapped.insert(kit.id.clone(), id.clone());
            let mut voices = vec![PatchVoice::default(); 4];
            for (channel, voice) in voices.iter_mut().enumerate() {
                let selected = old.voices.get(channel).filter(|v| v.kit_id == kit.id);
                let remembered = old.kit_configurations.get(&old.root_path, &kit.id, channel);
                let filename = selected
                    .map(|v| {
                        Path::new(&v.sample_path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .or_else(|| remembered.map(|v| v.sample_name.clone()));
                voice.mode = selected
                    .map(|v| v.layer_mode)
                    .or_else(|| remembered.map(|v| v.layer_mode))
                    .unwrap_or_default();
                for sample in kit.samples.iter().filter(|s| s.voice == channel + 1) {
                    let sample_id = copied.get(sample.id.as_str()).ok_or_else(|| {
                        format!(
                            "{} was not copied. The folder session remains active.",
                            sample.name
                        )
                    })?;
                    let layer = Layer {
                        id: Uuid::new_v4().to_string(),
                        sample_id: sample_id.clone(),
                    };
                    if filename.as_deref().is_some_and(|name| {
                        Path::new(&sample.id).file_name().is_some_and(|n| n == name)
                    }) {
                        voice.selected_layer = layer.id.clone();
                    }
                    voice.layers.push(layer);
                }
                if let Some(name) = filename {
                    if !name.is_empty() && voice.selected_layer.is_empty() {
                        return Err(format!("{} · Voice {}: selected file {name} is missing. Restore it and retry migration; the saved filename is retained.",kit.id,channel+1));
                    }
                } else {
                    voice.selected_layer = voice
                        .layers
                        .first()
                        .map(|l| l.id.clone())
                        .unwrap_or_default();
                }
            }
            if kit.samples.iter().any(|s| s.voice == 0) {
                return Err(format!("{} contains audio without a Voice prefix. Fix its placement before retrying migration. Copied audio is retained in the Library.",kit.id));
            }
            patch.kits.push(PatchKit {
                id,
                bank: kit.bank.clone(),
                number: kit.id[1..].parse().map_err(|_| "Invalid Kit number")?,
                voices,
            });
        }
        if !legacy.catalog.unassigned.is_empty() {
            return Err("Some audio has no valid Kit placement. Fix its placement before retrying migration. Copied audio is retained in the Library.".into());
        }
        patch.selected_kit = mapped
            .get(&old.selected_kit)
            .cloned()
            .ok_or("The selected Kit is missing. Restore the source folder and retry migration.")?;
        patch.voice_kits = old
            .voices
            .iter()
            .map(|v| {
                mapped.get(&v.kit_id).cloned().ok_or_else(|| {
                    format!(
                        "Voice source Kit {} is missing. Restore it and retry migration.",
                        v.kit_id
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        // Authoritative managed musical memory must contain no external source paths.
        for voice in &mut patch.voices {
            voice.sample_path.clear();
            voice.kit_id.clear();
        }
        for channel in 0..4 {
            if let Some(kit) = patch.voice_kit(channel) {
                for layer in &kit.voices[channel].layers {
                    store.prepare_rample(&layer.sample_id)?;
                }
            }
        }
        let prior = store.load_patches()?;
        let mut graph = prior.clone();
        graph.selected_patch = patch.id.clone();
        graph.patches.push(patch);
        let tx = store
            .db
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO patch_state(id,payload) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[serde_json::to_string(&graph).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        tx.execute(
            "INSERT INTO legacy_migration(id,patch_id) VALUES (1,?1)",
            [&graph.selected_patch],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE sd_intakes SET patch_id=?2 WHERE id=?1",
            [&review.id, &graph.selected_patch],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        if let Err(error) =
            self.patch_action_locked(patches::PatchAction::Open, &mut legacy, &store)
        {
            // Activation errors restore both durable and running folder state.
            let tx = store
                .db
                .unchecked_transaction()
                .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE patch_state SET payload=?1 WHERE id=1",
                [serde_json::to_string(&prior).map_err(|e| e.to_string())?],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE sd_intakes SET patch_id=NULL WHERE id=?1",
                [&review.id],
            )
            .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM legacy_migration", [])
                .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            self.patch_active.store(false, Ordering::Relaxed);
            legacy.patch_preview = None;
            legacy.restore_legacy_audio = true;
            legacy.synchronize(&self.audio)?;
            return Err(format!(
                "Migration could not activate: {error}. The folder session remains active."
            ));
        }
        legacy.restore_legacy_audio = false;
        legacy.editor = None;
        legacy.catalog = Catalog::default();
        legacy.prepared.clear();
        legacy.banks = std::array::from_fn(|_| vec![]);
        Ok(())
    }
}
#[tauri::command]
pub fn migration_status(state: tauri::State<'_, Arc<AppState>>) -> MigrationStatus {
    state.migration_status()
}
#[tauri::command]
pub async fn migrate_legacy(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<MigrationStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.migrate_legacy())
        .await
        .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use crate::{patches::PatchAction, tests::fixture, *};

    #[test]
    fn managed_restart_ignores_obsolete_session_and_reviews_and_does_not_recreate_the_source() {
        let (temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        assert_eq!(state.migrate_legacy().unwrap().status, "complete");
        drop(state);
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::write(
            temp.path().join("support/session.json"),
            b"obsolete invalid session",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("support/sample-reviews.json"),
            b"obsolete invalid review",
        )
        .unwrap();
        let (state, _) = crate::tests::fixture_state(temp.path().join("support"));
        assert!(state.snapshot().managed);
        assert!(!root.exists());
        let (audio, mut renderer) = AudioHost::offline(44100);
        let mut state = Arc::try_unwrap(state).ok().unwrap();
        state.audio = audio;
        state.patch_action(PatchAction::Open).unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::Audition { channel: 0 },
            })
            .unwrap();
        let mut pcm = vec![0.; 256];
        renderer.render(&mut pcm, 2);
        assert!(pcm.iter().any(|v| v.abs() > 0.01));
    }

    #[test]
    fn missing_selected_filename_is_retained_until_restored_and_never_replaced_by_another_layer() {
        let (_temp, state, _) = fixture();
        let selected = PathBuf::from(&state.snapshot().session.voices[0].sample_path);
        let original = std::fs::read(&selected).unwrap();
        std::fs::remove_file(&selected).unwrap();
        let failed = state.migrate_legacy().unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed
            .errors
            .iter()
            .any(|e| e.contains("selected file 1_Sub kick.wav is missing")));
        assert_eq!(
            state.snapshot().session.voices[0].sample_path,
            selected.to_string_lossy()
        );
        std::fs::write(&selected, original).unwrap();
        assert_eq!(state.migrate_legacy().unwrap().status, "complete");
        let view = state.patch_action(PatchAction::Open).unwrap();
        let voice = &view.graph.current().unwrap().kit().unwrap().voices[0];
        let sample = voice
            .layers
            .iter()
            .find(|l| l.id == voice.selected_layer)
            .unwrap();
        assert_eq!(
            view.samples
                .iter()
                .find(|s| s.id == sample.sample_id)
                .unwrap()
                .name,
            "1_Sub kick"
        );
    }
    #[test]
    fn late_activation_failure_restores_folder_audio_before_the_first_recovered_audition() {
        let (_temp, original, _) = fixture();
        let (audio, mut renderer) = AudioHost::offline(44100);
        let mut state = Arc::try_unwrap(original).ok().unwrap();
        state.audio = audio;
        {
            let mut legacy = state.library.lock().unwrap();
            legacy.banks = std::array::from_fn(|_| vec![]);
            legacy.synchronize(&state.audio).unwrap();
        }
        renderer.render(&mut [0.; 256], 2);
        let mut capacity = 0;
        while state.audio.send(Command::Stop).is_ok() {
            capacity += 1;
        }
        renderer.render(&mut [0.; 256], 2);
        // Permit Stop, Edit, Settings and the first managed bank, then fail.
        for _ in 0..capacity - 4 {
            state.audio.send(Command::Stop).unwrap();
        }
        assert_eq!(state.migrate_legacy().unwrap().status, "failed");
        assert!(!state.snapshot().managed);
        renderer.render(&mut [0.; 256], 2);
        state.act(Action::Audition { channel: 0 }).unwrap();
        let mut pcm = vec![0.; 1024];
        renderer.render(&mut pcm, 2);
        assert!(
            pcm.iter().any(|v| v.abs() > 0.01),
            "Folder audio was not restored before audition"
        );
        assert!(state.snapshot().watching);
    }

    #[test]
    fn audio_activation_failure_keeps_the_previous_session_and_existing_patches_retryable() {
        let (temp, original, _) = fixture();
        original
            .patch_action(PatchAction::CreatePatch {
                name: "Existing work".into(),
            })
            .unwrap();
        original.close_patch().unwrap();
        let (audio, mut renderer) = AudioHost::offline(44100);
        let mut state = Arc::try_unwrap(original).ok().unwrap();
        state.audio = audio;
        let before = state.snapshot().session;
        while state.audio.send(Command::Stop).is_ok() {}
        let failed = state.migrate_legacy().unwrap();
        assert_eq!(failed.status, "failed");
        assert!(!state.snapshot().managed);
        assert_eq!(state.snapshot().session, before);
        renderer.render(&mut [0.; 256], 2);
        drop(state);
        let (restarted, _) = crate::tests::fixture_state(temp.path().join("support"));
        assert!(!restarted.patch_active.load(Ordering::Relaxed));
        assert_eq!(restarted.migrate_legacy().unwrap().status, "complete");
        let view = restarted.patch_action(PatchAction::Open).unwrap();
        assert_eq!(view.graph.patches.len(), 2);
        assert!(view.graph.patches.iter().any(|p| p.name == "Existing work"));
    }

    #[test]
    fn partial_migration_keeps_folder_playback_and_retries_without_duplicate_samples_or_patches() {
        let (temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        std::fs::write(root.join("A0/2_Broken.wav"), b"RIFFbroken").unwrap();
        let before = state.snapshot().session;
        let failed = state.migrate_legacy().unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.errors.iter().any(|e| e.contains("Broken.wav")));
        assert!(!state.snapshot().managed);
        assert!(!state.patch_active.load(Ordering::Relaxed));
        assert_eq!(state.snapshot().session, before);
        state.act(Action::Audition { channel: 0 }).unwrap();
        drop(state);
        let (restarted, _) = crate::tests::fixture_state(temp.path().join("support"));
        assert!(!restarted.snapshot().managed);
        wav::write_pcm16(&root.join("A0/2_Broken.wav"), &[1234; 4410], 44100).unwrap();
        assert_eq!(restarted.migrate_legacy().unwrap().status, "complete");
        assert_eq!(restarted.list_samples().unwrap().len(), 5);
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .patches
                .len(),
            1
        );
        assert_eq!(restarted.migrate_legacy().unwrap().status, "complete");
        assert_eq!(
            restarted
                .patch_action(PatchAction::Open)
                .unwrap()
                .graph
                .patches
                .len(),
            1
        );
    }

    #[test]
    fn migration_keeps_musical_memory_and_plays_without_source_after_restart() {
        let (temp, original, _) = fixture();
        let (audio, mut renderer) = AudioHost::offline(44100);
        let mut state = Arc::try_unwrap(original).ok().unwrap();
        state.audio = audio;
        let root = temp.path().join("owned SD");
        for (kit, pcm) in [("A0", 8192), ("B12", 16384)] {
            std::fs::create_dir_all(root.join(kit)).unwrap();
            for voice in 1..=4 {
                wav::write_pcm16(
                    &root.join(format!("{kit}/{voice}_Sound.wav")),
                    &[pcm; 4410],
                    44100,
                )
                .unwrap();
            }
        }
        state
            .act(Action::Root {
                path: root.to_string_lossy().into_owned(),
            })
            .unwrap();
        state
            .act(Action::SetMode {
                channel: 0,
                mode: LayerMode::Random,
            })
            .unwrap();
        state.act(Action::SelectKit { kit: "B12".into() }).unwrap();
        for (channel, mode) in [
            LayerMode::Sequential,
            LayerMode::Reverse,
            LayerMode::Velocity,
            LayerMode::Single,
        ]
        .into_iter()
        .enumerate()
        {
            state.act(Action::SetMode { channel, mode }).unwrap();
        }
        state
            .act(Action::SelectSource {
                channel: 0,
                kit: "A0".into(),
            })
            .unwrap();
        state.act(Action::SetBpm { value: 137. }).unwrap();
        state
            .act(Action::ToggleStep {
                channel: 0,
                step: 3,
            })
            .unwrap();
        state
            .act(Action::SetVelocity {
                channel: 0,
                step: 3,
                value: 42,
            })
            .unwrap();
        state
            .act(Action::SetVolume {
                channel: 0,
                value: 0.5,
            })
            .unwrap();
        state.act(Action::ToggleMute { channel: 2 }).unwrap();
        state.act(Action::ToggleSolo { channel: 0 }).unwrap();
        std::fs::create_dir_all(root.join("_save")).unwrap();
        std::fs::write(root.join("_save/A0.rpl"), [0, 255, 13, 10]).unwrap();
        let audio_bytes = std::fs::read(root.join("A0/1_Sound.wav")).unwrap();
        let before = state.snapshot().session;
        let saved_bytes = std::fs::read(temp.path().join("support/session.json")).unwrap();
        let migrated = state.migrate_legacy().unwrap();
        assert_eq!(migrated.status, "complete");
        assert_eq!(
            std::fs::read(root.join("A0/1_Sound.wav")).unwrap(),
            audio_bytes
        );
        assert_eq!(
            std::fs::read(root.join("_save/A0.rpl")).unwrap(),
            [0, 255, 13, 10]
        );
        assert_eq!(
            std::fs::read(migrated.backup.unwrap()).unwrap(),
            saved_bytes
        );
        assert_eq!(
            std::fs::read(temp.path().join("support/session.json")).unwrap(),
            saved_bytes
        );
        let view = state.patch_action(PatchAction::Open).unwrap();
        let patch_id = view.graph.selected_patch.clone();
        let intake = view
            .graph
            .current()
            .unwrap()
            .imported_sd
            .as_ref()
            .unwrap()
            .intake_id
            .clone();
        assert_eq!(
            state.sd_native_blob(&intake, "_save/A0.rpl").unwrap(),
            [0, 255, 13, 10]
        );
        let session = view.session.unwrap();
        assert_eq!(session.bpm, 137.);
        for (actual, expected) in session.voices.iter().zip(&before.voices) {
            assert_eq!(actual.steps, expected.steps);
            assert_eq!(actual.velocities, expected.velocities);
            assert_eq!(actual.volume, expected.volume);
            assert_eq!(actual.muted, expected.muted);
            assert_eq!(actual.solo, expected.solo);
            assert_eq!(actual.layer_mode, expected.layer_mode);
        }
        assert!(!state.snapshot().watching);
        std::fs::remove_dir_all(&root).unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::Audition { channel: 0 },
            })
            .unwrap();
        let mut pcm = vec![0.; 256];
        renderer.render(&mut pcm, 2);
        assert!((pcm[100] - 0.05).abs() < 0.002, "{}", pcm[100]);
        drop(state);
        let (restarted, _) = crate::tests::fixture_state(temp.path().join("support"));
        assert!(restarted.patch_active.load(Ordering::Relaxed));
        let again = restarted.migrate_legacy().unwrap();
        assert_eq!(again.status, "complete");
        assert_eq!(
            restarted.sd_native_blob(&intake, "_save/A0.rpl").unwrap(),
            [0, 255, 13, 10]
        );
        let view = restarted.patch_action(PatchAction::Open).unwrap();
        assert_eq!(view.graph.patches.len(), 1);
        assert_eq!(view.graph.selected_patch, patch_id);
        let patch = view.graph.current().unwrap();
        let a = patch.kits.iter().find(|k| k.bank == "A").unwrap();
        let selected = restarted
            .patch_action(PatchAction::SelectKit { kit: a.id.clone() })
            .unwrap();
        assert_eq!(
            selected.session.unwrap().voices[0].layer_mode,
            LayerMode::Random
        );
        let opened = restarted.open_managed_editor(0).unwrap();
        assert!(serde_json::to_value(opened).unwrap()["managed"].is_object());
    }
}
