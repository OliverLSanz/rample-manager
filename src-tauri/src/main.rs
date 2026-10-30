#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod editor;
mod managed;
mod patches;
mod sample_edits;
use notify::{RecursiveMode, Watcher};
use rample_core::{
    audio::{native::AudioHost, Command, Status},
    catalog::{self, Catalog, Sample, Scanner},
    conversion,
    model::{LayerMode, Session},
    review::{Candidate, ReviewedFile, Reviews},
    session,
    wav::{self, PreparedSample},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    revision: u64,
    session: Session,
    catalog: Catalog,
    waveforms: BTreeMap<String, Vec<f32>>,
    banks: Vec<Vec<String>>,
    is_demo: bool,
    watching: bool,
    support_path: String,
    message: Option<String>,
    playing: bool,
    imports: Vec<Candidate>,
    editor_invalidated: bool,
}
struct Library {
    revision: u64,
    session: Session,
    catalog: Catalog,
    scanner: Scanner,
    prepared: HashMap<String, Arc<PreparedSample>>,
    banks: [Vec<Arc<PreparedSample>>; 4],
    support: PathBuf,
    reviews: Reviews,
    message: Option<String>,
    playing: bool,
    editor: Option<editor::EditorState>,
    patch_preview: Option<patches::PatchPreview>,
}
impl Library {
    fn new(support: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&support).map_err(|e| e.to_string())?;
        let mut session = session::load(&support)?;
        if session.root_path.is_empty() {
            session.root_path = wav::create_demo(&support)?.to_string_lossy().into_owned();
            let hits = [
                vec![0, 4, 8, 12],
                vec![4, 12],
                vec![0, 2, 4, 6, 8, 10, 12, 14],
                vec![6, 14],
            ];
            for (i, voice) in session.voices.iter_mut().enumerate() {
                voice.steps = (0..16).map(|n| hits[i].contains(&n)).collect();
            }
        }
        Ok(Self {
            revision: 0,
            session,
            catalog: Catalog::default(),
            scanner: Scanner::default(),
            prepared: HashMap::new(),
            banks: std::array::from_fn(|_| vec![]),
            reviews: Reviews::load(&support)?,
            support,
            message: None,
            playing: false,
            editor: None,
            patch_preview: None,
        })
    }
    fn snapshot(&self, watching: bool) -> Snapshot {
        Snapshot {
            revision: self.revision,
            session: self.session.clone(),
            catalog: self.catalog.clone(),
            waveforms: self
                .banks
                .iter()
                .flatten()
                .map(|s| (s.path.clone(), s.waveform.clone()))
                .collect(),
            banks: self
                .banks
                .iter()
                .map(|b| b.iter().map(|s| s.path.clone()).collect())
                .collect(),
            is_demo: Path::new(&self.session.root_path) == self.support.join("Demo SD"),
            watching,
            support_path: self.support.to_string_lossy().into_owned(),
            message: self.message.clone(),
            playing: self.playing,
            imports: self.reviews.pending(&self.session.root_path, &self.catalog),
            editor_invalidated: self.editor.as_ref().is_some_and(|e| e.invalidated),
        }
    }
    fn samples(&self, i: usize) -> Vec<Sample> {
        self.catalog
            .kits
            .iter()
            .find(|k| k.id == self.session.voices[i].kit_id)
            .map(|k| {
                k.samples
                    .iter()
                    .filter(|s| s.voice == i + 1)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
    fn playable(&self, i: usize) -> Vec<Sample> {
        let voice = &self.session.voices[i];
        if voice.sample_path.is_empty() {
            return vec![];
        }
        let samples = self.samples(i);
        if voice.layer_mode == LayerMode::Single {
            samples
                .into_iter()
                .filter(|s| s.readable && s.id == voice.sample_path)
                .collect()
        } else {
            samples
                .into_iter()
                .filter(|s| s.readable)
                .take(12)
                .collect()
        }
    }
    fn restore(&mut self, i: usize) {
        let voice = &self.session.voices[i];
        if let Some(config) = self
            .session
            .kit_configurations
            .get(&self.session.root_path, &voice.kit_id, i)
            .cloned()
        {
            let path = if config.sample_name.is_empty() {
                String::new()
            } else {
                Path::new(&self.session.root_path)
                    .join(&voice.kit_id)
                    .join(config.sample_name)
                    .to_string_lossy()
                    .into_owned()
            };
            self.session.voices[i].sample_path = path;
            self.session.voices[i].layer_mode = config.layer_mode;
        } else {
            let first = self
                .samples(i)
                .iter()
                .find(|s| s.readable)
                .map(|s| s.id.clone())
                .unwrap_or_default();
            self.session.voices[i].sample_path = first;
            self.session.voices[i].layer_mode = LayerMode::Single;
        }
    }
    fn select_kit(&mut self, kit: String) -> Result<(), String> {
        if !self.catalog.kits.iter().any(|k| k.id == kit) {
            return Err("Kit no longer exists. Refresh the library.".into());
        }
        self.session.remember();
        self.session.selected_kit = kit.clone();
        for i in 0..4 {
            self.session.voices[i].kit_id = kit.clone();
            self.restore(i);
        }
        Ok(())
    }
    fn scan(&mut self, paths: &HashSet<PathBuf>, force: bool) -> bool {
        let next = self
            .scanner
            .scan(Path::new(&self.session.root_path), paths, force);
        let reviews_changed = self.reviews.update(&self.session.root_path, &next);
        if reviews_changed {
            if let Err(e) = self.reviews.save(&self.support) {
                self.message = Some(e);
            }
        }
        let mut changed = next != self.catalog || reviews_changed;
        if let Some(e) = &mut self.editor {
            if !e.invalidated && catalog::fingerprint(Path::new(&e.source.path)) != e.fingerprint {
                e.invalidated = true;
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        self.catalog = next;
        if self.session.selected_kit.is_empty() {
            if let Some(kit) = self.catalog.kits.first() {
                let _ = self.select_kit(kit.id.clone());
            }
        }
        changed
    }
    fn synchronize(&mut self, audio: &AudioHost) -> Result<(), String> {
        if let Some(e) = &mut self.editor {
            if e.invalidated && e.enabled {
                audio.send(Command::Edit(None))?;
                e.enabled = false;
            }
        }
        audio.send(Command::Settings(
            self.session.voices.clone(),
            self.session.bpm,
        ))?;
        for i in 0..4 {
            let candidates = self.playable(i);
            let unchanged = candidates.len() == self.banks[i].len()
                && candidates
                    .iter()
                    .zip(&self.banks[i])
                    .all(|(a, b)| a.id == b.path && a.revision == b.revision);
            if unchanged {
                continue;
            }
            let mut bank = vec![];
            for sample in candidates {
                let key = format!("{}\0{}", sample.id, sample.revision);
                if let Some(prepared) = self.prepared.get(&key) {
                    bank.push(prepared.clone());
                } else {
                    match PreparedSample::load(Path::new(&sample.id), &sample.revision) {
                        Ok(prepared) => {
                            self.prepared.insert(key, prepared.clone());
                            bank.push(prepared)
                        }
                        Err(e) => {
                            self.message = Some(format!("Could not load {}: {e}", sample.name))
                        }
                    }
                }
            }
            audio.send(Command::Bank(i, bank.clone()))?;
            self.banks[i] = bank;
        }
        if self.prepared.len() > 64 {
            let active: HashSet<_> = self
                .banks
                .iter()
                .flatten()
                .map(|s| format!("{}\0{}", s.path, s.revision))
                .collect();
            self.prepared.retain(|k, _| active.contains(k));
        }
        Ok(())
    }
    fn save(&mut self) -> Result<(), String> {
        self.session.remember();
        session::save(&self.support, &self.session)
    }
}
struct AppState {
    managed: Mutex<managed::ManagedLibrary>,
    patch_active: AtomicBool,
    original_audition: Mutex<managed::AuditionSession>,
    library: Mutex<Library>,
    audio: AudioHost,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    watching: AtomicBool,
    events: std::sync::mpsc::Sender<Vec<PathBuf>>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Action {
    Root {
        path: String,
    },
    Demo,
    SelectKit {
        kit: String,
    },
    SelectSource {
        channel: usize,
        kit: String,
    },
    SelectSample {
        channel: usize,
        path: String,
    },
    SetMode {
        channel: usize,
        mode: LayerMode,
    },
    ToggleStep {
        channel: usize,
        step: usize,
    },
    SetVolume {
        channel: usize,
        value: f32,
    },
    ToggleMute {
        channel: usize,
    },
    ToggleSolo {
        channel: usize,
    },
    SetVelocity {
        channel: usize,
        step: usize,
        value: u8,
    },
    SetBpm {
        value: f64,
    },
    AdjustBpm {
        delta: f64,
    },
    BasicPattern,
    ClearPattern,
    Refresh,
    ReviewFormats,
    DismissImports {
        files: Vec<ReviewedFile>,
    },
    TogglePlayback,
    Stop,
    Audition {
        channel: usize,
    },
}
impl AppState {
    fn snapshot(&self) -> Snapshot {
        self.library
            .lock()
            .unwrap()
            .snapshot(self.watching.load(Ordering::Relaxed))
    }
    fn watch(&self) {
        self.watching.store(false, Ordering::Relaxed);
        let root = self.library.lock().unwrap().session.root_path.clone();
        let sender = self.events.clone();
        let result = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                let _ = sender.send(event.paths);
            }
        })
        .and_then(|mut watcher| {
            watcher.watch(Path::new(&root), RecursiveMode::Recursive)?;
            Ok(watcher)
        });
        *self.watcher.lock().unwrap() = result.ok();
        self.watching
            .store(self.watcher.lock().unwrap().is_some(), Ordering::Relaxed);
    }
    fn act(&self, action: Action) -> Result<Snapshot, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        l.message = None;
        let mut watch = false;
        let mut persist = true;
        fn channel(i: usize) -> Result<usize, String> {
            if i < 4 {
                Ok(i)
            } else {
                Err("Invalid voice".into())
            }
        }
        fn step(i: usize) -> Result<usize, String> {
            if i < 16 {
                Ok(i)
            } else {
                Err("Invalid step".into())
            }
        }
        match action {
            Action::Root { path } => {
                let root = dunce::canonicalize(&path)
                    .map_err(|e| format!("Could not open folder: {e}"))?;
                if !root.is_dir() {
                    return Err("Choose a folder containing your kits.".into());
                }
                managed::guard_legacy_path(&l.support, &root, true)?;
                l.session.remember();
                self.audio.send(Command::Stop)?;
                l.playing = false;
                l.session.root_path = root.to_string_lossy().into_owned();
                l.session.selected_kit.clear();
                for v in &mut l.session.voices {
                    v.kit_id.clear();
                    v.sample_path.clear();
                }
                l.scanner = Scanner::default();
                l.prepared.clear();
                l.scan(&HashSet::new(), true);
                watch = true;
            }
            Action::Demo => {
                let path = wav::create_demo(&l.support)?.to_string_lossy().into_owned();
                drop(l);
                return self.act(Action::Root { path });
            }
            Action::SelectKit { kit } => l.select_kit(kit)?,
            Action::SelectSource { channel: i, kit } => {
                let i = channel(i)?;
                if !l.catalog.kits.iter().any(|k| k.id == kit) {
                    return Err("Kit no longer exists".into());
                }
                l.session.remember();
                l.session.voices[i].kit_id = kit;
                l.restore(i);
            }
            Action::SelectSample { channel: i, path } => {
                let i = channel(i)?;
                if !path.is_empty() && !l.samples(i).iter().any(|s| s.id == path && s.readable) {
                    return Err("Sample no longer exists or cannot be played".into());
                }
                l.session.voices[i].sample_path = path;
            }
            Action::SetMode { channel: i, mode } => {
                let i = channel(i)?;
                l.session.voices[i].layer_mode = mode;
                if mode != LayerMode::Single && l.session.voices[i].sample_path.is_empty() {
                    l.session.voices[i].sample_path = l
                        .samples(i)
                        .iter()
                        .find(|s| s.readable)
                        .map(|s| s.id.clone())
                        .unwrap_or_default();
                }
            }
            Action::ToggleStep {
                channel: i,
                step: s,
            } => {
                let (i, s) = (channel(i)?, step(s)?);
                l.session.voices[i].steps[s] = !l.session.voices[i].steps[s]
            }
            Action::SetVolume { channel: i, value } => {
                let i = channel(i)?;
                if value.is_finite() {
                    l.session.voices[i].volume = value.clamp(0., 1.)
                }
            }
            Action::ToggleMute { channel: i } => {
                let i = channel(i)?;
                l.session.voices[i].muted = !l.session.voices[i].muted
            }
            Action::ToggleSolo { channel: i } => {
                let i = channel(i)?;
                l.session.voices[i].solo = !l.session.voices[i].solo
            }
            Action::SetVelocity {
                channel: i,
                step: s,
                value,
            } => {
                let (i, s) = (channel(i)?, step(s)?);
                l.session.voices[i].velocities[s] = value.clamp(1, 127)
            }
            Action::SetBpm { value } => {
                if value.is_finite() {
                    l.session.bpm = value.clamp(30., 300.)
                }
            }
            Action::AdjustBpm { delta } => {
                if delta.is_finite() {
                    l.session.bpm = (l.session.bpm + delta).clamp(30., 300.)
                }
            }
            Action::BasicPattern => {
                let hits = [
                    vec![0, 4, 8, 12],
                    vec![4, 12],
                    vec![0, 2, 4, 6, 8, 10, 12, 14],
                    vec![6, 14],
                ];
                for (i, v) in l.session.voices.iter_mut().enumerate() {
                    v.steps = (0..16).map(|n| hits[i].contains(&n)).collect();
                }
            }
            Action::ClearPattern => {
                for v in &mut l.session.voices {
                    v.steps.fill(false)
                }
            }
            Action::Refresh => {
                l.scan(&HashSet::new(), true);
                persist = false;
            }
            Action::ReviewFormats => {
                l.scan(&HashSet::new(), true);
                let root = l.session.root_path.clone();
                let catalog = l.catalog.clone();
                l.reviews.request_all(&root, &catalog);
                l.reviews.save(&l.support)?;
                l.message = Some("Sample formats checked.".into());
                persist = false;
            }
            Action::DismissImports { files } => {
                let root = l.session.root_path.clone();
                l.reviews.dismiss(&root, &files);
                l.reviews.save(&l.support)?;
                persist = false;
            }
            Action::TogglePlayback => {
                if !l.playing && l.banks.iter().all(Vec::is_empty) {
                    return Err("Select at least one readable WAV to play.".into());
                }
                l.playing = !l.playing;
                self.audio.send(if l.playing {
                    Command::Play
                } else {
                    Command::Stop
                })?;
                persist = false;
            }
            Action::Stop => {
                l.playing = false;
                self.audio.send(Command::Stop)?;
                persist = false;
            }
            Action::Audition { channel: i } => {
                self.audio.send(Command::Audition(channel(i)?))?;
                persist = false;
            }
        }
        l.synchronize(&self.audio)?;
        l.revision += 1;
        if persist {
            l.save()?;
        }
        drop(l);
        if watch {
            self.watch();
        }
        Ok(self.snapshot())
    }
}
#[tauri::command]
async fn get_snapshot(state: tauri::State<'_, Arc<AppState>>) -> Result<Snapshot, String> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || Ok(s.snapshot()))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn act(action: Action, state: tauri::State<'_, Arc<AppState>>) -> Result<Snapshot, String> {
    let s = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || s.act(action))
        .await
        .map_err(|e| e.to_string())?
}
#[derive(Serialize)]
struct Transport {
    #[serde(flatten)]
    status: Status,
    error: Option<String>,
}
#[tauri::command]
fn get_transport(state: tauri::State<'_, Arc<AppState>>) -> Transport {
    Transport {
        status: state.audio.meters.snapshot(),
        error: state.audio.error.lock().ok().and_then(|e| e.clone()),
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversionResult {
    name: String,
    target: Option<String>,
    backup: Option<String>,
    error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConversionReply {
    snapshot: Snapshot,
    results: Vec<ConversionResult>,
    backup_path: String,
}
impl AppState {
    fn convert_imports(&self, files: Vec<ReviewedFile>) -> Result<ConversionReply, String> {
        let mut l = self.library.lock().map_err(|_| "Library is unavailable")?;
        l.message = None;
        l.scan(&HashSet::new(), true);
        let candidates = l.reviews.pending(&l.session.root_path, &l.catalog);
        let backups = l.support.join("Conversion backups");
        let mut results = vec![];
        let mut processed = HashSet::new();
        for file in files {
            if !processed.insert(file.id.clone()) {
                continue;
            }
            let name = Path::new(&file.id)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let checked = candidates
                .iter()
                .find(|c| c.id == file.id && c.fingerprint == file.fingerprint && c.convertible);
            let result = if let Err(error) =
                managed::guard_legacy_path(&l.support, Path::new(&file.id), false)
            {
                Err(error)
            } else if checked.is_some() {
                conversion::convert(Path::new(&file.id), &file.fingerprint, &backups)
            } else {
                Err("Sample changed or is no longer eligible. Review the current file before converting.".into())
            };
            match result {
                Ok(done) => {
                    for voice in &mut l.session.voices {
                        if voice.sample_path == done.source {
                            voice.sample_path = done.target.clone();
                        }
                    }
                    results.push(ConversionResult {
                        name,
                        target: Some(done.target),
                        backup: Some(done.backup),
                        error: None,
                    });
                }
                Err(error) => results.push(ConversionResult {
                    name,
                    target: None,
                    backup: None,
                    error: Some(error),
                }),
            }
        }
        l.scan(&HashSet::new(), true);
        let root = l.session.root_path.clone();
        let converted_files: Vec<_> = results
            .iter()
            .filter(|r| r.error.is_none())
            .filter_map(|r| r.target.as_ref())
            .map(|id| ReviewedFile {
                id: id.clone(),
                fingerprint: catalog::fingerprint(Path::new(id)),
            })
            .collect();
        // The user has already reviewed these successful replacements. Keep
        // remaining kit warnings, but do not interpret our own writes as imports.
        l.reviews.dismiss(&root, &converted_files);
        // Files have already been published. Report their actual outcomes even
        // if persisting app preferences or reloading the audio engine fails.
        let mut warnings = l.message.take().into_iter().collect::<Vec<_>>();
        if let Err(error) = l.reviews.save(&l.support) {
            warnings.push(format!("Could not save review decisions: {error}"));
        }
        if let Err(error) = l.synchronize(&self.audio) {
            warnings.push(format!(
                "Could not reload playback: {error}. Refresh files to retry."
            ));
        }
        if let Err(error) = l.save() {
            warnings.push(format!("Could not save the session: {error}"));
        }
        l.message = (!warnings.is_empty()).then(|| warnings.join(" "));
        l.revision += 1;
        let reply = ConversionReply {
            snapshot: l.snapshot(self.watching.load(Ordering::Relaxed)),
            results,
            backup_path: backups.to_string_lossy().into(),
        };
        Ok(reply)
    }
}
#[tauri::command]
async fn convert_imports(
    files: Vec<ReviewedFile>,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ConversionReply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.convert_imports(files))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn show_backups(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    open_backup_folder(&state, "Conversion backups")
}
fn open_backup_folder(state: &AppState, folder: &str) -> Result<(), String> {
    let path = state
        .library
        .lock()
        .map_err(|_| "Library is unavailable")?
        .support
        .join(folder);
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer.exe")
        .arg(dunce::simplified(&path))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
async fn export_kit(
    name: String,
    parent: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut l = state.library.lock().unwrap();
        let selected: Vec<_> = (0..4)
            .map(|i| {
                l.samples(i)
                    .into_iter()
                    .find(|s| s.id == l.session.voices[i].sample_path)
            })
            .collect();
        let target = catalog::export(&selected, Path::new(&parent), &name)?;
        l.message = Some(format!("Kit {name} exported"));
        Ok(target.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn reveal(channel: Option<usize>, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let l = state.library.lock().unwrap();
    let path = match channel {
        Some(i) if i < 4 => l
            .samples(i)
            .into_iter()
            .find(|s| s.id == l.session.voices[i].sample_path)
            .map(|s| s.id)
            .ok_or("Select a sample first")?,
        Some(_) => return Err("Invalid voice".into()),
        None => l.session.root_path.clone(),
    };
    #[cfg(target_os = "macos")]
    {
        let mut c = std::process::Command::new("open");
        if channel.is_some() {
            c.arg("-R");
        }
        c.arg(path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = std::process::Command::new("explorer.exe");
        let shell_path = dunce::simplified(Path::new(&path)).to_string_lossy();
        c.arg(if channel.is_some() {
            format!("/select,{shell_path}")
        } else {
            shell_path.into_owned()
        });
        c.spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let support = if let Some(override_path) = std::env::var_os("RAMPLE_DATA_DIR") {
                PathBuf::from(override_path)
            } else {
                dirs::data_dir()
                    .ok_or("Could not locate the app data folder")?
                    .join("Rample Preview")
            };
            let managed = managed::ManagedLibrary::open(&support)?;
            let mut library = Library::new(support)?;
            let audio = AudioHost::new();
            library.scan(&HashSet::new(), false);
            library.synchronize(&audio)?;
            library.save()?;
            let (tx, rx) = std::sync::mpsc::channel();
            let state = Arc::new(AppState {
                managed: Mutex::new(managed),
                patch_active: AtomicBool::new(false),
                original_audition: Mutex::default(),
                library: Mutex::new(library),
                audio,
                watcher: Mutex::new(None),
                watching: AtomicBool::new(false),
                events: tx,
            });
            state.watch();
            app.manage(state.clone());
            let handle = app.handle().clone();
            let weak = Arc::downgrade(&state);
            std::thread::Builder::new()
                .name("library-watch".into())
                .spawn(move || {
                    let mut paths = HashSet::new();
                    let mut last_event = None;
                    let mut last_poll = Instant::now();
                    loop {
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(changed) => {
                                paths.extend(changed);
                                last_event = Some(Instant::now());
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(_) => {}
                        }
                        let due =
                            last_event.is_some_and(|at| at.elapsed() > Duration::from_millis(300));
                        let poll = last_poll.elapsed() > Duration::from_secs(4);
                        if !due && !poll {
                            continue;
                        }
                        let Some(state) = weak.upgrade() else { break };
                        let mut l = state.library.lock().unwrap();
                        let changed = l.scan(&paths, false);
                        if changed && !state.patch_active.load(Ordering::Relaxed) {
                            if let Err(e) = l.synchronize(&state.audio) {
                                l.message = Some(e);
                            }
                            let snapshot = l.snapshot(state.watching.load(Ordering::Relaxed));
                            let _ = handle.emit("library-changed", snapshot);
                        }
                        drop(l);
                        if !state.watching.load(Ordering::Relaxed) {
                            state.watch();
                        }
                        paths.clear();
                        last_event = None;
                        last_poll = Instant::now();
                    }
                })?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.state::<Arc<AppState>>();
                let _ = state.audio.send(Command::Stop);
                let _ = state.library.lock().unwrap().save();
            }
        })
        .invoke_handler(tauri::generate_handler![
            patches::patch_action,
            patches::close_patch,
            managed::list_samples,
            managed::query_samples,
            managed::update_sample_metadata,
            managed::import_samples,
            managed::list_trashed_samples,
            managed::retire_sample,
            managed::restore_sample,
            managed::audition_sample,
            managed::stop_sample_audition,
            managed::open_sample_audition,
            get_snapshot,
            get_transport,
            act,
            export_kit,
            convert_imports,
            show_backups,
            sample_edits::sample_revisions,
            editor::open_editor,
            editor::open_managed_editor,
            editor::save_managed_editor,
            editor::preview_editor,
            editor::save_editor,
            editor::close_editor,
            editor::audition_editor,
            editor::show_edit_backups,
            reveal
        ])
        .run(tauri::generate_context!())
        .expect("run Rample Preview");
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture() -> (
        tempfile::TempDir,
        Arc<AppState>,
        std::sync::mpsc::Receiver<Vec<PathBuf>>,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let (state, rx) = fixture_state(temp.path().join("support"));
        (temp, state, rx)
    }
    pub(super) fn fixture_state(
        support: PathBuf,
    ) -> (Arc<AppState>, std::sync::mpsc::Receiver<Vec<PathBuf>>) {
        let managed = managed::ManagedLibrary::open(&support).unwrap();
        let mut library = Library::new(support).unwrap();
        library.scan(&HashSet::new(), false);
        let audio = AudioHost::headless();
        library.synchronize(&audio).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let state = Arc::new(AppState {
            managed: Mutex::new(managed),
            patch_active: AtomicBool::new(false),
            original_audition: Mutex::default(),
            library: Mutex::new(library),
            audio,
            watcher: Mutex::new(None),
            watching: AtomicBool::new(false),
            events: tx,
        });
        (state, rx)
    }
    #[test]
    fn commands_remember_modes_and_keep_pattern_between_kits() {
        let (_temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        std::fs::create_dir(root.join("B12")).unwrap();
        std::fs::copy(
            root.join("A0").join("1_Sub kick.wav"),
            root.join("B12").join("1_Kick.wav"),
        )
        .unwrap();
        state.act(Action::Refresh).unwrap();
        state
            .act(Action::SetMode {
                channel: 0,
                mode: LayerMode::Reverse,
            })
            .unwrap();
        state.act(Action::SetBpm { value: 137. }).unwrap();
        let before = state.snapshot().session.voices[0].steps.clone();
        let second = state.act(Action::SelectKit { kit: "B12".into() }).unwrap();
        assert_eq!(second.session.voices[0].layer_mode, LayerMode::Single);
        assert_eq!(second.session.voices[0].steps, before);
        assert_eq!(second.session.bpm, 137.);
        let original = state.act(Action::SelectKit { kit: "A0".into() }).unwrap();
        assert_eq!(original.session.voices[0].layer_mode, LayerMode::Reverse);
        assert_eq!(original.session.voices[0].steps, before);
        let saved = session::load(Path::new(&original.support_path)).unwrap();
        assert_eq!(saved.voices[0].layer_mode, LayerMode::Reverse);
    }
    #[test]
    fn new_import_conversion_reloads_selected_audio_and_keeps_a_backup() {
        let (_temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        let path = root.join("A0").join("1_Imported.wav");
        wav::write_pcm16(&path, &vec![8000; 4800], 48000).unwrap();
        let original = std::fs::read(&path).unwrap();
        let detected = state.act(Action::Refresh).unwrap();
        assert_eq!(detected.imports.len(), 1);
        let candidate = &detected.imports[0];
        assert!(candidate.convertible);
        state
            .act(Action::SelectSample {
                channel: 0,
                path: path.to_string_lossy().into(),
            })
            .unwrap();
        let reply = state
            .convert_imports(vec![ReviewedFile {
                id: candidate.id.clone(),
                fingerprint: candidate.fingerprint.clone(),
            }])
            .unwrap();
        assert!(reply.results[0].error.is_none());
        assert!(reply.snapshot.imports.is_empty());
        assert_eq!(
            reply.snapshot.session.voices[0].sample_path,
            path.to_string_lossy()
        );
        assert_eq!(wav::inspect(&path).unwrap().rate, 44100);
        assert_eq!(
            std::fs::read(reply.results[0].backup.as_ref().unwrap()).unwrap(),
            original
        );
        assert_eq!(state.library.lock().unwrap().banks[0][0].spec.rate, 44100);
    }
    #[test]
    fn converting_a_stereo_layer_resolves_mixed_layers_without_reprompting() {
        let (_temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        let path = root.join("A0").join("1_Stereo.wav");
        wav::write_pcm16(&path, &vec![8000; 9600], 44100).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[22..24].copy_from_slice(&2u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&176400u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&4u16.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        let detected = state.act(Action::Refresh).unwrap();
        assert_eq!(detected.imports.len(), 1);
        let candidate = detected
            .imports
            .iter()
            .find(|c| c.id == path.to_string_lossy())
            .unwrap();
        assert!(candidate.convertible);
        let result = state
            .convert_imports(vec![ReviewedFile {
                id: candidate.id.clone(),
                fingerprint: candidate.fingerprint.clone(),
            }])
            .unwrap();
        assert!(result.results[0].error.is_none());
        assert!(result.snapshot.imports.is_empty());
        assert_eq!(wav::inspect(&path).unwrap().channels, 1);
        assert!(result.snapshot.catalog.kits[0].warnings.is_empty());
        assert!(state.act(Action::Refresh).unwrap().imports.is_empty());
    }
    #[test]
    fn conversion_reports_file_outcomes_even_if_saving_preferences_fails() {
        let (_temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        let path = root.join("A0").join("1_Imported.wav");
        wav::write_pcm16(&path, &vec![8000; 4800], 48000).unwrap();
        let detected = state.act(Action::Refresh).unwrap();
        let candidate = &detected.imports[0];
        std::fs::create_dir(Path::new(&detected.support_path).join("session.json")).unwrap();
        let reply = state
            .convert_imports(vec![ReviewedFile {
                id: candidate.id.clone(),
                fingerprint: candidate.fingerprint.clone(),
            }])
            .unwrap();
        assert!(reply.results[0].error.is_none());
        assert!(reply.results[0].backup.is_some());
        assert_eq!(wav::inspect(&path).unwrap().rate, 44100);
        assert!(reply
            .snapshot
            .message
            .unwrap()
            .contains("Could not save the session"));
    }
    #[test]
    fn conversion_requires_a_current_pending_review() {
        let (_temp, state, _) = fixture();
        let snapshot = state.snapshot();
        let path = PathBuf::from(&snapshot.session.voices[0].sample_path);
        let original = std::fs::read(&path).unwrap();
        let reply = state
            .convert_imports(vec![ReviewedFile {
                id: path.to_string_lossy().into(),
                fingerprint: catalog::fingerprint(&path),
            }])
            .unwrap();
        assert!(reply.results[0].error.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let bad = PathBuf::from(snapshot.session.root_path)
            .join("A0")
            .join("2_New.wav");
        wav::write_pcm16(&bad, &vec![8000; 4800], 48000).unwrap();
        let detected = state.act(Action::Refresh).unwrap();
        let candidate = &detected.imports[0];
        let file = ReviewedFile {
            id: candidate.id.clone(),
            fingerprint: candidate.fingerprint.clone(),
        };
        state
            .act(Action::DismissImports {
                files: vec![file.clone()],
            })
            .unwrap();
        assert!(state.act(Action::Refresh).unwrap().imports.is_empty());
        let refused = state.convert_imports(vec![file]).unwrap();
        assert!(refused.results[0].error.is_some());
        assert_eq!(wav::inspect(&bad).unwrap().rate, 48000);
        assert_eq!(state.act(Action::ReviewFormats).unwrap().imports.len(), 1);
    }
    #[test]
    fn folder_selection_keeps_paths_usable_by_the_shell() {
        let (_temp, state, _) = fixture();
        let root = state.snapshot().session.root_path;
        let result = state.act(Action::Root { path: root }).unwrap();
        assert!(Path::new(&result.session.root_path).is_absolute());
        assert!(result.banks.iter().all(|bank| !bank.is_empty()));
        #[cfg(target_os = "windows")]
        assert!(!result.session.root_path.starts_with(r"\\?\"));
        let saved = session::load(Path::new(&result.support_path)).unwrap();
        assert_eq!(saved.root_path, result.session.root_path);
    }
    #[test]
    fn tempo_adjustments_accumulate_and_clamp() {
        let (_temp, state, _) = fixture();
        state.act(Action::SetBpm { value: 137. }).unwrap();
        state.act(Action::AdjustBpm { delta: 1. }).unwrap();
        let next = state.act(Action::AdjustBpm { delta: 1. }).unwrap();
        assert_eq!(next.session.bpm, 139.);
        let high = state.act(Action::AdjustBpm { delta: 500. }).unwrap();
        assert_eq!(high.session.bpm, 300.);
        let low = state.act(Action::AdjustBpm { delta: -500. }).unwrap();
        assert_eq!(low.session.bpm, 30.);
        let saved = session::load(Path::new(&low.support_path)).unwrap();
        assert_eq!(saved.bpm, 30.);
    }
    #[test]
    fn native_watcher_reports_atomic_wav_replacement() {
        let (_temp, state, events) = fixture();
        state.watch();
        assert!(state.watching.load(Ordering::Relaxed));
        let root = PathBuf::from(state.snapshot().session.root_path);
        let path = root.join("A0").join("1_Sub kick.wav");
        let temp = path.with_extension("tmp");
        wav::write_pcm16(&temp, &vec![1200; 8820], 44100).unwrap();
        std::fs::rename(&temp, &path).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut observed = false;
        while Instant::now() < deadline {
            if let Ok(paths) = events.recv_timeout(Duration::from_millis(200)) {
                let mut l = state.library.lock().unwrap();
                l.scan(&paths.into_iter().collect(), false);
                if l.catalog.kits[0].samples[0].duration == 0.2 {
                    observed = true;
                    break;
                }
            }
        }
        assert!(
            observed,
            "Native filesystem event did not reflect the atomic save"
        );
    }
    #[test]
    fn deleted_samples_silence_banks_and_native_sd_files_stay_untouched() {
        let (_temp, state, _) = fixture();
        let root = PathBuf::from(state.snapshot().session.root_path);
        let save = root.join("_save");
        std::fs::create_dir(&save).unwrap();
        let bytes = b"synthetic native settings, preserve exactly";
        std::fs::write(save.join("A0.rpl"), bytes).unwrap();
        let original = state.snapshot().session.voices[0].sample_path.clone();
        std::fs::remove_file(&original).unwrap();
        let removed = state.act(Action::Refresh).unwrap();
        assert!(removed.banks[0].is_empty());
        assert_eq!(removed.session.voices[0].sample_path, original);
        assert!(removed.session.voices[0].steps[0]);
        state
            .act(Action::SetMode {
                channel: 1,
                mode: LayerMode::Random,
            })
            .unwrap();
        state
            .act(Action::ToggleStep {
                channel: 1,
                step: 3,
            })
            .unwrap();
        assert_eq!(std::fs::read(save.join("A0.rpl")).unwrap(), bytes);
    }
}
