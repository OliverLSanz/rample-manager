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
    fn preview_editor(
        &self,
        id: String,
        sequence: u64,
        settings: Settings,
        enabled: bool,
    ) -> Result<PreviewReply, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
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
            l.editor = None;
        }
        Ok(())
    }
    fn audition_editor(&self, id: String) -> Result<(), String> {
        let l = self.library.lock().map_err(|_| "Library is unavailable")?;
        let e = l.editor.as_ref().ok_or("Open a sample editor first.")?;
        e.check(&id)?;
        self.audio
            .send(Command::AuditionSample(e.channel, e.source.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;
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
