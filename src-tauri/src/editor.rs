use super::*;
use rample_core::{
    audio::EditPreview,
    editing::{self, Settings},
};
use std::sync::atomic::AtomicU64;
static EDIT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct EditorState {
    pub id: String,
    pub channel: usize,
    pub source: Arc<PreparedSample>,
    pub cropped: Arc<PreparedSample>,
    pub fingerprint: String,
    pub settings: Settings,
    pub sequence: u64,
    pub enabled: bool,
    pub invalidated: bool,
    pub managed: Option<ManagedEdit>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EditorInfo {
    id: String,
    channel: usize,
    path: String,
    fingerprint: String,
    duration: f64,
    waveform: Vec<f32>,
    settings: Settings,
    output_duration: f64,
    managed: Option<ManagedEdit>,
}
impl EditorState {
    fn info(&self) -> EditorInfo {
        EditorInfo {
            id: self.id.clone(),
            channel: self.channel,
            path: self.source.path.clone(),
            fingerprint: self.fingerprint.clone(),
            duration: self.source.spec.frames as f64 / self.source.spec.rate as f64,
            waveform: self.source.waveform.clone(),
            settings: self.settings.clone(),
            output_duration: self.settings.output_seconds(),
            managed: self.managed.clone(),
        }
    }
    fn check(&self, id: &str) -> Result<(), String> {
        if self.id != id {
            return Err("This editor session has closed. Reopen the sample.".into());
        }
        if self.invalidated
            || catalog::fingerprint(Path::new(&self.source.path)) != self.fingerprint
        {
            return Err(
                "The source changed on disk. Close the editor and reopen the sample before saving."
                    .into(),
            );
        }
        Ok(())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PreviewReply {
    sequence: u64,
    duration: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SaveReply {
    snapshot: Snapshot,
    editor: EditorInfo,
    backup: String,
    rendered: editing::Rendered,
}
impl AppState {
    pub(super) fn open_editor(&self, channel: usize) -> Result<EditorInfo, String> {
        if matches!(
            self.migration_status().status.as_str(),
            "working" | "complete"
        ) {
            return Err("Use Edit Sample in the managed Patch workspace.".into());
        }
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        if channel >= 4 {
            return Err("Invalid voice".into());
        }
        let path = l.session.voices[channel].sample_path.clone();
        managed::guard_legacy_path(&l.support, Path::new(&path), false)?;
        let sample = l
            .samples(channel)
            .into_iter()
            .find(|s| s.id == path && s.readable)
            .ok_or("Select a readable WAV in this voice first.")?;
        let fingerprint = catalog::fingerprint(Path::new(&path));
        let source = PreparedSample::load(Path::new(&path), &sample.revision)?;
        let settings = Settings::original(source.spec.frames as f64 / source.spec.rate as f64);
        let editor = EditorState {
            id: format!("editor-{}", EDIT_ID.fetch_add(1, Ordering::Relaxed)),
            channel,
            source: source.clone(),
            cropped: source.clone(),
            fingerprint,
            settings: settings.clone(),
            sequence: 0,
            enabled: true,
            invalidated: false,
            managed: None,
        };
        self.audio.send(Command::Edit(Some(EditPreview {
            channel,
            sample: source,
            settings,
        })))?;
        let info = editor.info();
        l.editor = Some(editor);
        Ok(info)
    }
    pub(super) fn preview_editor(
        &self,
        id: String,
        sequence: u64,
        settings: Settings,
        enabled: bool,
    ) -> Result<PreviewReply, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        if l.editor.as_ref().is_some_and(|e| e.managed.is_some()) {
            let store = self.managed.lock().map_err(|_| "Library is unavailable")?;
            let context = self.checked_managed_editor(&mut l, &store, &id)?;
            let e = l.editor.as_ref().unwrap();
            if sequence <= e.sequence {
                return Ok(PreviewReply {
                    sequence: e.sequence,
                    duration: e.settings.output_seconds(),
                });
            }
            settings.validate(&e.source.spec)?;
            let channel = e.channel;
            let mut bank =
                l.patch_preview.as_ref().ok_or("Reopen the Patch.")?.ready[channel].clone();
            if enabled {
                let prepared = store.render_edits(
                    &context.sample,
                    &format!("draft-{}-{sequence}", e.id),
                    &settings,
                )?;
                let target = bank
                    .iter_mut()
                    .find(|l| l.id == context.layer)
                    .ok_or("Layer changed. Reopen the editor.")?;
                target.sample = prepared;
            }
            self.audio.send(Command::LayerBank(channel, bank))?;
            let e = l.editor.as_mut().unwrap();
            e.settings = settings;
            e.enabled = enabled;
            e.sequence = sequence;
            return Ok(PreviewReply {
                sequence,
                duration: e.settings.output_seconds(),
            });
        }
        let e = l.editor.as_mut().ok_or("Open a sample editor first.")?;
        e.check(&id)?;
        if sequence <= e.sequence {
            return Ok(PreviewReply {
                sequence: e.sequence,
                duration: e.settings.output_seconds(),
            });
        }
        settings.validate(&e.source.spec)?;
        let start = (settings.start * e.source.spec.rate as f64).round() as u64;
        if e.cropped.head_start != start {
            e.cropped = if start == 0 {
                e.source.clone()
            } else {
                e.source.preload_at(start)?
            };
        }
        self.audio.send(Command::Edit(enabled.then(|| EditPreview {
            channel: e.channel,
            sample: e.cropped.clone(),
            settings: settings.clone(),
        })))?;
        e.settings = settings;
        e.enabled = enabled;
        e.sequence = sequence;
        Ok(PreviewReply {
            sequence,
            duration: e.settings.output_seconds(),
        })
    }
    fn close_editor(&self, id: String) -> Result<(), String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        if l.editor.as_ref().is_some_and(|e| e.id == id) {
            self.audio.send(Command::Edit(None))?;
            if l.editor.as_ref().is_some_and(|e| e.managed.is_some()) {
                if let Some(preview) = &l.patch_preview {
                    for (i, bank) in preview.ready.iter().enumerate() {
                        self.audio.send(Command::LayerBank(i, bank.clone()))?;
                    }
                }
            }
            l.editor = None;
        }
        Ok(())
    }
    fn audition_editor(&self, id: String) -> Result<(), String> {
        let l = self.library.lock().map_err(|_| "Library is unavailable")?;
        let e = l.editor.as_ref().ok_or("Open a sample editor first.")?;
        e.check(&id)?;
        if let Some(context) = &e.managed {
            let store = self.managed.lock().map_err(|_| "Library is unavailable")?;
            context.check(&store, e.channel)?;
            self.audio.send(Command::Audition(e.channel))
        } else {
            self.audio
                .send(Command::AuditionSample(e.channel, e.source.clone()))
        }
    }
    fn save_editor(&self, id: String, settings: Settings) -> Result<SaveReply, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        let e = l.editor.as_ref().ok_or("Open a sample editor first.")?;
        e.check(&id)?;
        settings.validate(&e.source.spec)?;
        let (path, channel, stamp) = (e.source.path.clone(), e.channel, e.fingerprint.clone());
        managed::guard_legacy_path(&l.support, Path::new(&path), false)?;
        let backups = l.support.join("Sample edit backups");
        let (saved, rendered) = editing::save(Path::new(&path), &stamp, &settings, &backups)?;
        // Once the file is published, report that success even if a later preference write fails.
        l.editor = None;
        let mut warnings = vec![];
        if let Err(error) = self.audio.send(Command::Edit(None)) {
            warnings.push(error);
        }
        l.scan(&HashSet::new(), true);
        let root = l.session.root_path.clone();
        l.reviews.dismiss(
            &root,
            &[ReviewedFile {
                id: path.clone(),
                fingerprint: catalog::fingerprint(Path::new(&path)),
            }],
        );
        if let Err(error) = l.reviews.save(&l.support) {
            warnings.push(error);
        }
        if let Err(error) = l.synchronize(&self.audio) {
            warnings.push(error);
        }
        if let Err(error) = l.save() {
            warnings.push(format!("Could not save the session: {error}"));
        }
        let source =
            PreparedSample::load(Path::new(&path), &catalog::fingerprint(Path::new(&path)));
        // The validated output is still safely saved if the device becomes unavailable after publication.
        let duration = rendered.duration;
        let new_settings = Settings::original(duration);
        let next_id = format!("editor-{}", EDIT_ID.fetch_add(1, Ordering::Relaxed));
        let info = EditorInfo {
            id: next_id.clone(),
            channel,
            path: path.clone(),
            fingerprint: catalog::fingerprint(Path::new(&path)),
            duration,
            waveform: source
                .as_ref()
                .map(|s| s.waveform.clone())
                .unwrap_or_default(),
            settings: new_settings.clone(),
            output_duration: duration,
            managed: None,
        };
        match source {
            Ok(source) => {
                l.editor = Some(EditorState {
                    id: next_id,
                    channel,
                    source: source.clone(),
                    cropped: source,
                    fingerprint: info.fingerprint.clone(),
                    settings: new_settings,
                    sequence: 0,
                    enabled: false,
                    invalidated: false,
                    managed: None,
                })
            }
            Err(error) => warnings.push(format!(
                "Sample saved, but could not reopen the editor: {error}"
            )),
        }
        l.message = (!warnings.is_empty()).then(|| warnings.join(" "));
        l.revision += 1;
        Ok(SaveReply {
            snapshot: l.snapshot(self.watching.load(Ordering::Relaxed)),
            editor: info,
            backup: saved.backup,
            rendered,
        })
    }
}
#[tauri::command]
pub(super) async fn open_editor(
    channel: usize,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<EditorInfo, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.open_editor(channel))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn preview_editor(
    id: String,
    sequence: u64,
    settings: Settings,
    enabled: bool,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<PreviewReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.preview_editor(id, sequence, settings, enabled)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn save_editor(
    id: String,
    settings: Settings,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SaveReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.save_editor(id, settings))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn close_editor(
    id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.close_editor(id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn audition_editor(
    id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.audition_editor(id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) fn show_edit_backups(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    super::open_backup_folder(&state, "Sample edit backups")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManagedEdit {
    pub sample: String,
    pub revision: String,
    pub layer: String,
    pub patch: String,
    pub kit: String,
    pub name: String,
    pub references: Vec<String>,
}
fn sample_references(graph: &patches::PatchGraph, sample: &str) -> Vec<String> {
    let mut references = vec![];
    for p in &graph.patches {
        for k in &p.kits {
            for (ch, v) in k.voices.iter().enumerate() {
                for (i, l) in v.layers.iter().enumerate() {
                    if l.sample_id == sample {
                        references.push(format!(
                            "{} · {}{} · Voice {} · Layer {}",
                            p.name,
                            k.bank,
                            k.number,
                            ch + 1,
                            i + 1
                        ));
                    }
                }
            }
        }
    }
    references
}
impl ManagedEdit {
    fn check(&self, store: &managed::ManagedLibrary, channel: usize) -> Result<(), String> {
        let graph = store.load_patches()?;
        let valid = graph
            .current()
            .filter(|p| p.id == self.patch)
            .and_then(|p| p.voice_kit(channel))
            .filter(|k| k.id == self.kit)
            .and_then(|k| k.voices.get(channel))
            .is_some_and(|v| {
                v.selected_layer == self.layer
                    && v.layers
                        .iter()
                        .any(|l| l.id == self.layer && l.sample_id == self.sample)
            });
        if sample_references(&graph, &self.sample) != self.references {
            return Err("Shared Sample references changed. Reopen the editor to review the affected Layers.".into());
        }
        if !valid || store.sample_edits(&self.sample)?.id != self.revision {
            return Err("This Layer or Sample changed. Reopen the editor.".into());
        }
        Ok(())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManagedSaveReply {
    editor: EditorInfo,
    patch: patches::PatchSnapshot,
}
impl AppState {
    fn checked_managed_editor(
        &self,
        l: &mut Library,
        store: &managed::ManagedLibrary,
        id: &str,
    ) -> Result<ManagedEdit, String> {
        let e = l.editor.as_mut().ok_or("Open a sample editor first.")?;
        if e.id != id {
            return Err("This editor session has closed. Reopen the sample.".into());
        }
        let context = e.managed.clone().ok_or("Open a managed Sample first.")?;
        if let Err(error) = e.check(id).and_then(|_| context.check(store, e.channel)) {
            e.invalidated = true;
            e.enabled = false;
            if let Some(preview) = &l.patch_preview {
                for (i, bank) in preview.ready.iter().enumerate() {
                    let _ = self.audio.send(Command::LayerBank(i, bank.clone()));
                }
            }
            return Err(error);
        }
        Ok(context)
    }
    pub(super) fn open_managed_editor(&self, channel: usize) -> Result<EditorInfo, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        let store = self.managed.lock().map_err(|_| "Library is unavailable")?;
        self.open_managed_editor_locked(channel, &mut l, &store)
    }
    fn open_managed_editor_locked(
        &self,
        channel: usize,
        l: &mut Library,
        store: &managed::ManagedLibrary,
    ) -> Result<EditorInfo, String> {
        if !self.patch_active.load(Ordering::Relaxed) {
            return Err("Open a Patch first.".into());
        }
        let graph = store.load_patches()?;
        let patch = graph.current().ok_or("Select a Patch first.")?;
        let kit = patch.voice_kit(channel).ok_or("Select a Kit first.")?;
        let voice = kit.voices.get(channel).ok_or("Invalid Voice.")?;
        let layer = voice
            .layers
            .iter()
            .find(|l| l.id == voice.selected_layer)
            .ok_or("Select a Layer first.")?;
        let sample = store
            .list()?
            .into_iter()
            .find(|s| s.id == layer.sample_id)
            .ok_or("Sample no longer exists.")?;
        let saved = store.sample_edits(&sample.id)?;
        let source = store.prepare(&sample.id)?;
        let references = sample_references(&graph, &sample.id);

        self.audio.send(Command::Edit(None))?;
        // Reopening replaces any previous local draft with the saved ready bank.
        if let Some(preview) = &l.patch_preview {
            for (i, bank) in preview.ready.iter().enumerate() {
                self.audio.send(Command::LayerBank(i, bank.clone()))?;
            }
        }
        let editor = EditorState {
            id: format!("editor-{}", uuid::Uuid::new_v4()),
            channel,
            fingerprint: catalog::fingerprint(Path::new(&source.path)),
            source: source.clone(),
            cropped: source,
            settings: saved.settings,
            sequence: 0,
            enabled: true,
            invalidated: false,
            managed: Some(ManagedEdit {
                sample: sample.id,
                revision: saved.id,
                layer: layer.id.clone(),
                patch: patch.id.clone(),
                kit: kit.id.clone(),
                name: sample.name,
                references,
            }),
        };
        let info = editor.info();
        l.editor = Some(editor);
        Ok(info)
    }
    pub(super) fn save_managed_editor(
        &self,
        id: String,
        settings: Settings,
        variant_name: Option<String>,
    ) -> Result<ManagedSaveReply, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        let mut store = self.managed.lock().map_err(|_| "Library is unavailable")?;
        let context = self.checked_managed_editor(&mut l, &store, &id)?;
        let e = l.editor.as_ref().unwrap();
        settings.validate(&e.source.spec)?;
        let channel = e.channel;
        let graph = store.load_patches()?;
        store.publish_edits(
            &context.sample,
            &context.revision,
            &settings,
            variant_name.as_deref(),
            graph,
            &context.layer,
        )?;
        l.editor = None;
        l.patch_preview = None;
        let saved_warning = |error| {
            format!("Sample Edits saved and Original preserved, but preview could not be refreshed: {error}. Reopen the Patch.")
        };
        self.audio
            .send(Command::Edit(None))
            .map_err(saved_warning)?;
        let patch = self
            .patch_action_locked(patches::PatchAction::Open, &mut l, &store)
            .map_err(saved_warning)?;
        let editor = self
            .open_managed_editor_locked(channel, &mut l, &store)
            .map_err(saved_warning)?;
        Ok(ManagedSaveReply { editor, patch })
    }
}
#[tauri::command]
pub(super) async fn open_managed_editor(
    channel: usize,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<EditorInfo, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.open_managed_editor(channel))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(super) async fn save_managed_editor(
    id: String,
    settings: Settings,
    variant_name: Option<String>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ManagedSaveReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.save_managed_editor(id, settings, variant_name)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overwrite_rejects_a_shared_scope_changed_after_the_editor_opened() {
        use crate::patches::PatchAction;
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("Shared scope.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0);
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Scope".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 1,
                sample: sample.id,
            })
            .unwrap();
        let mut settings = editor.settings;
        settings.gain_db = -6.;
        assert!(state
            .save_managed_editor(editor.id, settings, None)
            .err()
            .unwrap()
            .contains("references changed"));
    }
    #[test]
    fn changed_managed_original_disables_the_editor_even_if_bytes_are_restored() {
        use crate::patches::PatchAction;
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("Owned.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0);
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Source guards".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let original = PathBuf::from(
            state
                .prepare_library_sample(&sample.id, false)
                .unwrap()
                .path
                .clone(),
        );
        let saved = original.with_extension("safe");
        std::fs::rename(&original, &saved).unwrap();
        wav::write_pcm16(&original, &[4096; 4410], 44100).unwrap();
        assert!(state
            .preview_editor(editor.id.clone(), 1, editor.settings.clone(), true)
            .is_err());
        std::fs::remove_file(&original).unwrap();
        std::fs::rename(saved, original).unwrap();
        assert!(state
            .save_managed_editor(editor.id.clone(), editor.settings.clone(), None)
            .is_err());
        assert!(state
            .preview_editor(editor.id, 2, editor.settings, true)
            .is_err());
        assert!(state.open_managed_editor(0).is_ok());
    }
    #[test]
    fn historical_export_recovers_a_variant_without_reverting_current_sample() {
        use crate::patches::PatchAction;
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0].clone();
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Patch".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -3.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let exported = temp.path().join("export.wav");
        state
            .export_sample(sample.id.clone(), exported.clone())
            .unwrap();
        let same_revision_export = temp.path().join("export-again.wav");
        state
            .export_sample(sample.id.clone(), same_revision_export.clone())
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let saved = state
            .save_managed_editor(editor.id, editor.settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let same_recipe_export = temp.path().join("same-recipe.wav");
        state
            .export_sample(sample.id.clone(), same_recipe_export.clone())
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let reply = state.import_samples(vec![exported.clone()]).unwrap();
        let variant = reply.results[0].sample.as_ref().unwrap();
        assert_eq!((reply.imported, reply.recognized), (0, 1));
        assert_ne!(variant.id, sample.id);
        assert_eq!(variant.original_id, sample.original_id);
        assert_eq!(
            state
                .import_samples(vec![exported.clone()])
                .unwrap()
                .results[0]
                .sample
                .as_ref()
                .unwrap()
                .id,
            variant.id
        );
        let recovered_id = variant.id.clone();
        let emitted = PreparedSample::load(&exported, "export").unwrap();
        let recovered = state.prepare_library_sample(&recovered_id, true).unwrap();
        assert_eq!(recovered.head.len(), emitted.head.len());
        assert!(recovered
            .head
            .iter()
            .zip(&emitted.head)
            .all(|(a, b)| (a[0] - b[0]).abs() <= 2.0 / 32768.0));
        drop(state);
        std::fs::remove_dir_all(temp.path().join("support/Library/Derived")).unwrap();
        let (state, _) = crate::tests::fixture_state(temp.path().join("support"));
        assert_eq!(
            state.import_samples(vec![exported]).unwrap().results[0]
                .sample
                .as_ref()
                .unwrap()
                .id,
            recovered_id
        );
        for exported in [same_revision_export, same_recipe_export] {
            let reply = state.import_samples(vec![exported]).unwrap();
            assert_eq!(reply.results[0].sample.as_ref().unwrap().id, recovered_id);
        }
        let recovered = state.prepare_library_sample(&recovered_id, true).unwrap();
        assert_eq!(recovered.head.len(), emitted.head.len());
        assert!(recovered
            .head
            .iter()
            .zip(&emitted.head)
            .all(|(a, b)| (a[0] - b[0]).abs() <= 2.0 / 32768.0));
        assert_eq!(
            state
                .sample_revisions(sample.id)
                .unwrap()
                .last()
                .unwrap()
                .settings
                .gain_db,
            -6.
        );
    }

    #[test]
    fn exports_of_independent_variants_keep_their_sample_identities() {
        use crate::patches::PatchAction;
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0].clone();
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Patch".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let saved = state
            .save_managed_editor(editor.id, editor.settings, Some("Variant".into()))
            .unwrap();
        let variant = saved.patch.graph.current().unwrap().kit().unwrap().voices[0].layers[0]
            .sample_id
            .clone();
        state.close_editor(saved.editor.id).unwrap();
        assert_ne!(variant, sample.id);
        let a = temp.path().join("a.wav");
        let b = temp.path().join("b.wav");
        state.export_sample(sample.id.clone(), a.clone()).unwrap();
        state.export_sample(variant.clone(), b.clone()).unwrap();
        let a_pcm = PreparedSample::load(&a, "a").unwrap();
        let b_pcm = PreparedSample::load(&b, "b").unwrap();
        assert_eq!(a_pcm.head, b_pcm.head);
        let reply = state.import_samples(vec![a.clone(), b.clone()]).unwrap();
        assert_eq!(reply.results[0].sample.as_ref().unwrap().id, sample.id);
        assert_eq!(reply.results[1].sample.as_ref().unwrap().id, variant);
        // Equal historical recipes from independent Samples also keep separate recoveries.
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let added = state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let layer = added.graph.current().unwrap().kit().unwrap().voices[0].layers[1]
            .id
            .clone();
        state
            .patch_action(PatchAction::SelectLayer { channel: 0, layer })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let recovered = state.import_samples(vec![a.clone(), b.clone()]).unwrap();
        let first = &recovered.results[0].sample.as_ref().unwrap().id;
        let second = &recovered.results[1].sample.as_ref().unwrap().id;
        assert_ne!(first, second);
        let repeated = state.import_samples(vec![a, b]).unwrap();
        assert_eq!(&repeated.results[0].sample.as_ref().unwrap().id, first);
        assert_eq!(&repeated.results[1].sample.as_ref().unwrap().id, second);
    }

    #[test]
    fn reimporting_original_after_export_and_edit_preserves_current_sample() {
        use crate::patches::PatchAction;
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source.clone()]).unwrap().samples[0].clone();
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Patch".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        state
            .export_sample(sample.id.clone(), temp.path().join("export.wav"))
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let reply = state.import_samples(vec![source]).unwrap();
        assert_eq!(reply.results[0].sample.as_ref().unwrap().id, sample.id);
        assert_eq!(reply.samples.len(), 1);
    }

    #[test]
    fn sd_review_cannot_finish_after_its_exported_sample_changes() {
        use crate::{patches::PatchAction, sd_import::SdImportChoice};
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0].clone();
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Patch".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let sd = temp.path().join("SD");
        std::fs::create_dir_all(sd.join("A0")).unwrap();
        state
            .export_sample(sample.id, sd.join("A0/1_Kick.wav"))
            .unwrap();
        let review = state.prepare_sd_import(sd).unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        assert!(state
            .finish_sd_import(&review.id, SdImportChoice::New { name: "SD".into() })
            .is_err());
    }

    #[test]
    fn sd_reimport_of_historical_export_does_not_reuse_patch_with_new_sound() {
        use crate::{patches::PatchAction, sd_import::SdImportChoice};
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("source.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0].clone();
        let sd = temp.path().join("SD");
        std::fs::create_dir_all(sd.join("A0")).unwrap();
        state
            .export_sample(sample.id.clone(), sd.join("A0/1_Kick.wav"))
            .unwrap();
        let review = state.prepare_sd_import(sd.clone()).unwrap();
        state
            .finish_sd_import(&review.id, SdImportChoice::New { name: "SD".into() })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.close_editor(saved.editor.id).unwrap();
        let review = state.prepare_sd_import(sd).unwrap();
        assert_eq!(review.status, "changed");
        let recovered = &review.draft.kits[0].voices[0].layers[0].sample_id;
        assert_ne!(recovered, &sample.id);
        assert!(state
            .finish_sd_import(&review.id, SdImportChoice::Reuse)
            .is_err());
        let _ = PatchAction::Open;
    }

    #[test]
    fn shared_overwrite_updates_other_patches_and_keeps_exportable_revision_pcm() {
        use crate::patches::PatchAction;
        let (temp, original_state, _) = crate::tests::fixture();
        let (audio, mut renderer) = AudioHost::offline(44100);
        let mut state = Arc::try_unwrap(original_state).ok().unwrap();
        state.audio = audio;
        let source = temp.path().join("Stereo48000.wav");
        wav::write_pcm16(&source, &[8192, -4096].repeat(4800), 48000).unwrap();
        let mut bytes = std::fs::read(&source).unwrap();
        bytes[22..24].copy_from_slice(&2u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&192000u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&4u16.to_le_bytes());
        std::fs::write(&source, &bytes).unwrap();
        let sample = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0);
        state
            .patch_action(PatchAction::CreatePatch {
                name: "First".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        let view = state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let first = view.graph.selected_patch;
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Second".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "B".into(),
                number: 2,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        assert_eq!(editor.managed.as_ref().unwrap().references.len(), 2);
        let original_revision = editor.managed.as_ref().unwrap().revision.clone();
        let mut settings = editor.settings.clone();
        settings.gain_db = -6.;
        settings.pitch = 3.;
        settings.echo_mix = 0.25;
        settings.echo_ms = 50.;
        settings.reverb_mix = 0.2;
        settings.reverb_seconds = 0.2;
        state
            .preview_editor(editor.id.clone(), 1, settings.clone(), true)
            .unwrap();
        state.audition_editor(editor.id.clone()).unwrap();
        let mut draft = vec![0.; 22050 * 2];
        renderer.render(&mut draft, 2);
        let saved = state
            .save_managed_editor(editor.id, settings, None)
            .unwrap();
        state.audition_editor(saved.editor.id.clone()).unwrap();
        let mut pcm = vec![0.; draft.len()];
        renderer.render(&mut pcm, 2);
        assert!(pcm.iter().zip(&draft).all(|(a, b)| (a - b).abs() < 0.00003));
        state.close_editor(saved.editor.id).unwrap();
        state
            .patch_action(PatchAction::SelectPatch { patch: first })
            .unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::Audition { channel: 0 },
            })
            .unwrap();
        let mut shared = vec![0.; pcm.len()];
        renderer.render(&mut shared, 2);
        assert_eq!(shared, pcm);
        let current = state.open_managed_editor(0).unwrap();
        let revision = current.managed.as_ref().unwrap().revision.clone();
        let prepared = state
            .prepare_sample_revision(sample.id.clone(), revision.clone())
            .unwrap();
        let old = state
            .prepare_sample_revision(sample.id.clone(), original_revision)
            .unwrap();
        assert_eq!(old.spec.frames, 4410);
        assert!(prepared.spec.frames > 4410);
        assert!(prepared.head[5000..].iter().any(|v| v[0].abs() > 0.00001));
        for (audio, encoded) in pcm.as_chunks::<2>().0.iter().zip(&prepared.head) {
            assert!((audio[0] - encoded[0] * 0.32).abs() < 0.00001);
        }
        assert_eq!(
            std::fs::read(
                state
                    .prepare_library_sample(&sample.id, false)
                    .unwrap()
                    .path
                    .clone()
            )
            .unwrap(),
            bytes
        );
        assert!(state
            .prepare_sample_revision(sample.id, "unrelated".into())
            .is_err());
    }
    #[test]
    fn managed_reset_and_unchanged_preview_preserve_unedited_rample_pcm() {
        use crate::patches::PatchAction;
        for rate in [48000, 96000] {
            let (temp, original_state, _) = crate::tests::fixture();
            let (audio, mut renderer) = AudioHost::offline(44100);
            let mut state = Arc::try_unwrap(original_state).ok().unwrap();
            state.audio = audio;
            let source = temp.path().join("High-frequency original.wav");
            let signal: Vec<i16> = (0..rate / 10)
                .map(|i| {
                    (12000. * (std::f64::consts::TAU * 18000. * i as f64 / rate as f64).sin())
                        as i16
                })
                .collect();
            wav::write_pcm16(&source, &signal, rate).unwrap();
            let sample = state
                .import_samples(vec![source])
                .unwrap()
                .samples
                .remove(0);
            state
                .patch_action(PatchAction::CreatePatch {
                    name: "Reset".into(),
                })
                .unwrap();
            state
                .patch_action(PatchAction::CreateKit {
                    bank: "A".into(),
                    number: 0,
                })
                .unwrap();
            state
                .patch_action(PatchAction::AddLayer {
                    channel: 0,
                    sample: sample.id,
                })
                .unwrap();
            let editor = state.open_managed_editor(0).unwrap();
            state.audition_editor(editor.id.clone()).unwrap();
            let mut saved = vec![0.; 8820];
            renderer.render(&mut saved, 2);
            assert!(saved.iter().any(|v| v.abs() > 0.01));
            let mut changed = editor.settings.clone();
            changed.gain_db = -12.;
            let mut inactive_effects = editor.settings.clone();
            inactive_effects.echo_ms = 750.;
            inactive_effects.echo_feedback = 0.7;
            inactive_effects.reverb_seconds = 4.5;
            for (sequence, settings, enabled) in [
                (1, editor.settings.clone(), true),
                (2, changed, true),
                (3, editor.settings.clone(), true),
                (4, editor.settings.clone(), false),
                (5, editor.settings.clone(), true),
                (6, inactive_effects.clone(), true),
            ] {
                state
                    .preview_editor(editor.id.clone(), sequence, settings, enabled)
                    .unwrap();
                state.audition_editor(editor.id.clone()).unwrap();
                let mut pcm = vec![0.; saved.len()];
                renderer.render(&mut pcm, 2);
                if sequence == 2 {
                    assert_ne!(pcm, saved);
                } else {
                    let error = pcm
                        .iter()
                        .zip(&saved)
                        .map(|(a, b)| (a - b).abs())
                        .fold(0., f32::max);
                    assert!(
                        error < 0.00003,
                        "{rate} Hz, preview {sequence}: peak difference {error}"
                    );
                }
            }
            let result = state
                .save_managed_editor(editor.id, inactive_effects.clone(), None)
                .unwrap();
            assert_eq!(result.editor.settings, inactive_effects);
            state.audition_editor(result.editor.id).unwrap();
            let mut pcm = vec![0.; saved.len()];
            renderer.render(&mut pcm, 2);
            assert!(pcm.iter().zip(&saved).all(|(a, b)| (a - b).abs() < 0.00003));
        }
    }
    #[test]
    fn managed_drafts_are_local_cancel_restores_saved_and_variant_retargets_one_occurrence() {
        use crate::patches::PatchAction;
        let (temp, original_state, _) = crate::tests::fixture();
        let (audio, mut renderer) = AudioHost::offline(44100);
        let mut state = Arc::try_unwrap(original_state).ok().unwrap();
        state.audio = audio;
        let source = temp.path().join("Shared.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0);
        state
            .update_sample_metadata(sample.id.clone(), "Shared".into(), vec!["warm".into()])
            .unwrap();
        state
            .patch_action(PatchAction::CreatePatch { name: "One".into() })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let view = state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let layers = &view.graph.current().unwrap().kit().unwrap().voices[0].layers;
        let first = layers[0].id.clone();
        let second = layers[1].id.clone();
        state
            .patch_action(PatchAction::SelectLayer {
                channel: 0,
                layer: second.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.gain_db = -12.;
        state
            .preview_editor(editor.id.clone(), 2, settings.clone(), true)
            .unwrap();
        state
            .preview_editor(editor.id.clone(), 1, editor.settings.clone(), false)
            .unwrap();
        state.audition_editor(editor.id.clone()).unwrap();
        let mut pcm = [0.; 2];
        renderer.render(&mut pcm, 2);
        let quiet = pcm[0];
        assert!((quiet - 0.0201).abs() < 0.001, "{quiet}");
        // Switching a selected occurrence invalidates the editor without losing its peer's saved sound.
        state
            .patch_action(PatchAction::SelectLayer {
                channel: 0,
                layer: first.clone(),
            })
            .unwrap();
        assert!(state
            .preview_editor(editor.id.clone(), 3, settings.clone(), true)
            .is_err());
        state
            .patch_action(PatchAction::Musical {
                action: Action::Audition { channel: 0 },
            })
            .unwrap();
        renderer.render(&mut pcm, 2);
        assert!((pcm[0] - 0.08).abs() < 0.001);
        state.close_editor(editor.id).unwrap();
        state
            .patch_action(PatchAction::SelectLayer {
                channel: 0,
                layer: second.clone(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::Musical {
                action: Action::Audition { channel: 0 },
            })
            .unwrap();
        renderer.render(&mut pcm, 2);
        assert!((pcm[0] - 0.08).abs() < 0.001);
        let editor = state.open_managed_editor(0).unwrap();
        let saved = state
            .save_managed_editor(editor.id.clone(), settings, Some("Quiet variant".into()))
            .unwrap();
        let voice = &saved.patch.graph.current().unwrap().kit().unwrap().voices[0];
        assert_eq!(voice.layers[0].sample_id, sample.id);
        assert_ne!(voice.layers[1].sample_id, sample.id);
        assert_eq!(voice.layers[1].id, second);
        let variants = state.list_samples().unwrap();
        assert_eq!(variants[0].original_id, variants[1].original_id);
        assert_eq!(variants[1].tags, ["warm"]);
        assert_eq!(variants[1].name, "Quiet variant");
        assert!(state
            .save_managed_editor(editor.id, editor.settings, None)
            .is_err());
        state.audition_editor(saved.editor.id).unwrap();
        renderer.render(&mut pcm, 2);
        assert!((pcm[0] - quiet).abs() < 0.001); // saved sound is processed once
    }
    #[test]
    fn managed_edits_reopen_complete_original_and_preserve_saved_revisions() {
        use crate::patches::PatchAction;
        let (temp, state, _) = crate::tests::fixture();
        let source = temp.path().join("Reversible.wav");
        wav::write_pcm16(&source, &[8192; 44100], 44100).unwrap();
        let bytes = std::fs::read(&source).unwrap();
        let sample = state
            .import_samples(vec![source])
            .unwrap()
            .samples
            .remove(0);
        state
            .patch_action(PatchAction::CreatePatch {
                name: "Edits".into(),
            })
            .unwrap();
        state
            .patch_action(PatchAction::CreateKit {
                bank: "A".into(),
                number: 0,
            })
            .unwrap();
        state
            .patch_action(PatchAction::AddLayer {
                channel: 0,
                sample: sample.id.clone(),
            })
            .unwrap();
        let editor = state.open_managed_editor(0).unwrap();
        let mut settings = editor.settings.clone();
        settings.end = 0.2;
        settings.gain_db = -12.;
        settings.reverb_mix = 0.5;
        state
            .save_managed_editor(editor.id, settings.clone(), None)
            .unwrap();
        let reopened = state.open_managed_editor(0).unwrap();
        assert_eq!(reopened.duration, 1.);
        assert_eq!(reopened.settings, settings);
        let first_revision = reopened.managed.as_ref().unwrap().revision.clone();
        settings.end = 0.8;
        settings.gain_db = -3.;
        settings.reverb_mix = 0.;
        state
            .save_managed_editor(reopened.id, settings.clone(), None)
            .unwrap();
        let prepared = state.prepare_library_sample(&sample.id, true).unwrap();
        assert_eq!(prepared.spec.frames, 35280);
        let revisions = state.sample_revisions(sample.id.clone()).unwrap();
        assert!(revisions.iter().any(|r| r.id == first_revision));
        drop(state);
        let (restarted, _) = crate::tests::fixture_state(temp.path().join("support"));
        restarted.patch_action(PatchAction::Open).unwrap();
        assert_eq!(restarted.open_managed_editor(0).unwrap().settings, settings);
        assert_eq!(
            std::fs::read(
                &restarted
                    .prepare_library_sample(&sample.id, false)
                    .unwrap()
                    .path
            )
            .unwrap(),
            bytes
        );
    }
    #[test]
    fn editor_preview_and_cancel_preserve_original_audio_and_session() {
        let (_temp, state, _) = crate::tests::fixture();
        let before = state.snapshot().session;
        let path = PathBuf::from(&before.voices[0].sample_path);
        let bytes = std::fs::read(&path).unwrap();
        let info = state.open_editor(0).unwrap();
        let mut settings = info.settings.clone();
        settings.gain_db = -6.;
        settings.pitch = 7.;
        state
            .preview_editor(info.id.clone(), 1, settings, true)
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            serde_json::to_value(&before).unwrap(),
            serde_json::to_value(state.snapshot().session).unwrap()
        );
        state.close_editor(info.id).unwrap();
        assert!(state.library.lock().unwrap().editor.is_none());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn saving_bakes_once_reloads_the_selected_voice_and_retains_original_backup() {
        let (_temp, state, _) = crate::tests::fixture();
        let info = state.open_editor(0).unwrap();
        let path = PathBuf::from(&info.path);
        let bytes = std::fs::read(&path).unwrap();
        let mut settings = info.settings.clone();
        settings.end = 0.1;
        settings.pitch = 12.;
        settings.gain_db = -6.;
        let result = state.save_editor(info.id.clone(), settings).unwrap();
        assert_eq!(std::fs::read(result.backup).unwrap(), bytes);
        assert_eq!(wav::inspect(&path).unwrap().frames, 2205);
        assert_eq!(state.library.lock().unwrap().banks[0][0].spec.frames, 2205);
        assert_eq!(result.editor.settings.pitch, 0.);
        assert_eq!(result.editor.settings.gain_db, 0.);
        assert_eq!(result.editor.settings.end, 0.05);
        state.close_editor(info.id).unwrap();
        assert!(state.library.lock().unwrap().editor.is_some()); // stale close cannot clear the new edit session
        assert_eq!(
            result.snapshot.session.voices[0].sample_path,
            path.to_string_lossy()
        );
    }
    #[test]
    fn changed_sources_invalidate_preview_and_reject_stale_saves() {
        let (_temp, state, _) = crate::tests::fixture();
        let info = state.open_editor(0).unwrap();
        let path = PathBuf::from(&info.path);
        wav::write_pcm16(&path, &[7000; 4410], 44100).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let snapshot = state.act(Action::Refresh).unwrap();
        assert!(snapshot.editor_invalidated);
        assert!(
            !state
                .library
                .lock()
                .unwrap()
                .editor
                .as_ref()
                .unwrap()
                .enabled
        );
        assert!(state
            .preview_editor(info.id.clone(), 1, info.settings.clone(), true)
            .is_err());
        assert!(state.save_editor(info.id, info.settings).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn older_preview_requests_cannot_replace_more_recent_settings() {
        let (_temp, state, _) = crate::tests::fixture();
        let info = state.open_editor(0).unwrap();
        let mut recent = info.settings.clone();
        recent.gain_db = -12.;
        state
            .preview_editor(info.id.clone(), 2, recent.clone(), true)
            .unwrap();
        let older = state
            .preview_editor(info.id, 1, info.settings, false)
            .unwrap();
        assert_eq!(older.sequence, 2);
        let l = state.library.lock().unwrap();
        let e = l.editor.as_ref().unwrap();
        assert_eq!(e.settings, recent);
        assert!(e.enabled);
    }
}
