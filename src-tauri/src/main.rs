#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod build_patch;
mod destination_backups;
mod editor;
mod library_snapshots;
mod managed;
mod patches;
mod sample_edits;
mod sd_import;
use rample_core::{
    audio::{native::AudioHost, Command, Status},
    catalog, conversion,
    model::{LayerMode, Session},
    wav::{self, PreparedSample},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::Manager;

/// Transient state of the open Patch: transport, the sample editor, the
/// prepared preview and the last deletion. Durable data lives in the Library.
struct Workspace {
    support: PathBuf,
    playing: bool,
    editor: Option<editor::EditorState>,
    patch_preview: Option<patches::PatchPreview>,
    /// The last deleted Kit, Patch or Layer, for undo.
    deleted: Option<patches::Deleted>,
}
struct AppState {
    managed: Mutex<managed::ManagedLibrary>,
    original_audition: Mutex<managed::AuditionSession>,
    workspace: Mutex<Workspace>,
    audio: AudioHost,
}
impl AppState {
    /// Opens the Library in `support` and plays its open Patch through `audio`.
    fn open(support: PathBuf, audio: AudioHost) -> Result<Arc<Self>, String> {
        std::fs::create_dir_all(&support).map_err(|e| e.to_string())?;
        let state = Arc::new(Self {
            managed: Mutex::new(managed::ManagedLibrary::open(&support)?),
            original_audition: Mutex::default(),
            workspace: Mutex::new(Workspace {
                support,
                playing: false,
                editor: None,
                patch_preview: None,
                deleted: None,
            }),
            audio,
        });
        state.patch_action(patches::PatchAction::Open)?;
        Ok(state)
    }
}
/// Sequencer, mixer and audition controls of the open Patch.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Action {
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
    TogglePlayback,
    Stop,
    Audition {
        channel: usize,
    },
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
fn open_backup_folder(state: &AppState, folder: &str) -> Result<(), String> {
    let path = state
        .workspace
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
            app.manage(AppState::open(support, AudioHost::new())?);
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.state::<Arc<AppState>>();
                let _ = state.audio.send(Command::Stop);
            }
        })
        .invoke_handler(tauri::generate_handler![
            library_snapshots::library_recovery_snapshots,
            library_snapshots::backup_library,
            library_snapshots::restore_library,
            sd_import::prepare_sd_import,
            sd_import::sd_native_blob,
            sd_import::finish_sd_import,
            patches::patch_action,
            build_patch::review_patch_build,
            build_patch::apply_patch_build,
            destination_backups::patch_build_backups,
            destination_backups::restore_patch_build,
            destination_backups::show_patch_build_backups,
            managed::list_samples,
            managed::query_samples,
            managed::update_sample_metadata,
            managed::replace_sample_tag,
            managed::set_sample_tags,
            managed::import_samples,
            managed::list_trashed_samples,
            managed::retire_sample,
            managed::restore_sample,
            managed::audition_sample,
            managed::stop_sample_audition,
            managed::open_sample_audition,
            get_transport,
            managed::export_sample,
            managed::sample_waveform,
            sample_edits::sample_revisions,
            editor::open_managed_editor,
            editor::save_managed_editor,
            editor::preview_editor,
            editor::close_editor,
            editor::audition_editor,
        ])
        .run(tauri::generate_context!())
        .expect("run Rample Preview");
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture() -> (tempfile::TempDir, Arc<AppState>, ()) {
        let temp = tempfile::tempdir().unwrap();
        let state = fixture_state(temp.path().join("support")).0;
        (temp, state, ())
    }
    /// Opens a Library in `support` with headless audio, as on a restart.
    pub(super) fn fixture_state(support: PathBuf) -> (Arc<AppState>, ()) {
        (AppState::open(support, AudioHost::headless()).unwrap(), ())
    }
    /// Imports the bundled four-voice demo card as a Patch named "Demo", with
    /// Kit A0 selected, as a ready-made Library for tests.
    pub(super) fn seed_demo(state: &AppState) -> patches::PatchSnapshot {
        let support = state.workspace.lock().unwrap().support.clone();
        let card = wav::create_demo(&support).unwrap();
        let review = state.prepare_sd_import(card).unwrap();
        state
            .finish_sd_import(
                &review.id,
                sd_import::SdImportChoice::New {
                    name: "Demo".into(),
                },
            )
            .unwrap()
    }
    #[test]
    fn a_new_library_opens_empty_and_the_demo_card_imports_as_a_patch() {
        let (temp, state, _) = fixture();
        let empty = state.patch_action(patches::PatchAction::Open).unwrap();
        assert!(empty.graph.patches.is_empty());
        assert!(empty.session.is_none());
        // No folder session or demo is created on startup.
        assert!(!temp.path().join("support/session.json").exists());
        assert!(!temp.path().join("support/Demo SD").exists());
        let demo = seed_demo(&state);
        let patch = demo.graph.current().unwrap();
        assert_eq!(patch.name, "Demo");
        assert_eq!(patch.kit().unwrap().address(), "A0");
        assert!(patch
            .kit()
            .unwrap()
            .voices
            .iter()
            .all(|v| v.layers.len() == 1));
    }
    #[test]
    fn tempo_adjustments_accumulate_clamp_and_persist() {
        let (temp, state, _) = fixture();
        seed_demo(&state);
        let run = |action| {
            state
                .patch_action(patches::PatchAction::Musical { action })
                .unwrap()
        };
        run(Action::SetBpm { value: 137. });
        run(Action::AdjustBpm { delta: 1. });
        assert_eq!(
            run(Action::AdjustBpm { delta: 1. }).session.unwrap().bpm,
            139.
        );
        assert_eq!(
            run(Action::AdjustBpm { delta: 500. }).session.unwrap().bpm,
            300.
        );
        assert_eq!(
            run(Action::AdjustBpm { delta: -500. }).session.unwrap().bpm,
            30.
        );
        let (restarted, _) = fixture_state(temp.path().join("support"));
        let reopened = restarted.patch_action(patches::PatchAction::Open).unwrap();
        assert_eq!(reopened.session.unwrap().bpm, 30.);
    }
}
