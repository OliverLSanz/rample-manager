//! Managed arrangements use occurrence IDs; physical audio paths stay in prepared assets.
use crate::*;
use rample_core::{audio::LayerSample, model::Voice};
use uuid::Uuid;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchGraph {
    pub selected_patch: String,
    pub patches: Vec<Patch>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    pub id: String,
    pub name: String,
    pub selected_kit: String,
    pub kits: Vec<PatchKit>,
    pub bpm: f64,
    /// Sequencer/mixer memory is shared between Kits, as in the folder workflow.
    pub voices: Vec<Voice>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchKit {
    pub id: String,
    pub bank: String,
    pub number: u8,
    pub voices: Vec<PatchVoice>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchVoice {
    pub layers: Vec<Layer>,
    pub selected_layer: String,
    pub mode: LayerMode,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub id: String,
    pub sample_id: String,
}
impl PatchGraph {
    pub fn current(&self) -> Option<&Patch> {
        self.patches.iter().find(|p| p.id == self.selected_patch)
    }
    fn current_mut(&mut self) -> Result<&mut Patch, String> {
        self.patches
            .iter_mut()
            .find(|p| p.id == self.selected_patch)
            .ok_or("Select a Patch first.".into())
    }
}
impl Patch {
    pub fn kit(&self) -> Option<&PatchKit> {
        self.kits.iter().find(|k| k.id == self.selected_kit)
    }
    fn kit_mut(&mut self) -> Result<&mut PatchKit, String> {
        self.kits
            .iter_mut()
            .find(|k| k.id == self.selected_kit)
            .ok_or("Select a Kit first.".into())
    }
    fn voice_mut(&mut self, channel: usize) -> Result<&mut PatchVoice, String> {
        self.kit_mut()?
            .voices
            .get_mut(channel)
            .ok_or("Invalid Voice.".into())
    }
    fn session(&self) -> Session {
        let mut voices = self.voices.clone();
        if let Some(kit) = self.kit() {
            for (voice, arrangement) in voices.iter_mut().zip(&kit.voices) {
                voice.kit_id = kit.id.clone();
                voice.sample_path = arrangement.selected_layer.clone();
                voice.layer_mode = arrangement.mode;
            }
        }
        Session {
            bpm: self.bpm,
            voices,
            selected_kit: self.selected_kit.clone(),
            ..Session::default()
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSnapshot {
    pub graph: PatchGraph,
    pub session: Option<Session>,
    pub samples: Vec<managed::LibrarySample>,
    pub validation: Vec<String>,
    pub playing: bool,
}
#[cfg(test)]
impl PatchSnapshot {
    fn kit(&self) -> Option<&PatchKit> {
        self.graph.current()?.kit()
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PatchAction {
    Open,
    CreatePatch {
        name: String,
    },
    RenamePatch {
        name: String,
    },
    SelectPatch {
        patch: String,
    },
    CreateKit {
        bank: String,
        number: u8,
    },
    AddressKit {
        bank: String,
        number: u8,
    },
    SelectKit {
        kit: String,
    },
    AddLayer {
        channel: usize,
        sample: String,
    },
    RemoveLayer {
        channel: usize,
        layer: String,
    },
    MoveLayer {
        channel: usize,
        layer: String,
        to: usize,
    },
    SelectLayer {
        channel: usize,
        layer: String,
    },
    Musical {
        action: Action,
    },
}
fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("Patch name cannot be empty.".into())
    } else {
        Ok(())
    }
}
fn valid_address(patch: &Patch, bank: &str, number: u8, except: &str) -> Result<(), String> {
    if bank.len() != 1 || !bank.as_bytes()[0].is_ascii_uppercase() || number > 99 {
        return Err("Choose a Bank from A to Z and a Kit number from 0 to 99.".into());
    }
    if patch
        .kits
        .iter()
        .any(|k| k.id != except && k.bank == bank && k.number == number)
    {
        return Err("That Bank/Kit address is already used in this Patch.".into());
    }
    Ok(())
}
impl AppState {
    pub fn patch_action(&self, action: PatchAction) -> Result<PatchSnapshot, String> {
        // Same lock order as legacy application commands, including workspace handoff.
        let mut legacy = self.library.lock().map_err(|_| "Library is unavailable")?;
        let store = self.managed.lock().map_err(|_| "Library is unavailable")?;
        let mut graph = store.load_patches()?;
        let mut trigger = None;
        let mut transport = None;
        let mut reset = !self.patch_active.load(Ordering::Relaxed);
        match action {
            PatchAction::Open => {}
            PatchAction::CreatePatch { name } => {
                valid_name(&name)?;
                let id = Uuid::new_v4().to_string();
                graph.patches.push(Patch {
                    id: id.clone(),
                    name,
                    selected_kit: String::new(),
                    kits: vec![],
                    bpm: 120.,
                    voices: vec![Voice::default(); 4],
                });
                graph.selected_patch = id;
                reset = true;
            }
            PatchAction::RenamePatch { name } => {
                valid_name(&name)?;
                graph.current_mut()?.name = name;
            }
            PatchAction::SelectPatch { patch } => {
                if !graph.patches.iter().any(|p| p.id == patch) {
                    return Err("Patch no longer exists.".into());
                }
                graph.selected_patch = patch;
                reset = true;
            }
            PatchAction::CreateKit { bank, number } => {
                let patch = graph.current_mut()?;
                valid_address(patch, &bank, number, "")?;
                let id = Uuid::new_v4().to_string();
                patch.kits.push(PatchKit {
                    id: id.clone(),
                    bank,
                    number,
                    voices: vec![PatchVoice::default(); 4],
                });
                patch.selected_kit = id;
            }
            PatchAction::AddressKit { bank, number } => {
                let patch = graph.current_mut()?;
                valid_address(patch, &bank, number, &patch.selected_kit)?;
                let kit = patch.kit_mut()?;
                kit.bank = bank;
                kit.number = number;
            }
            PatchAction::SelectKit { kit } => {
                let patch = graph.current_mut()?;
                if !patch.kits.iter().any(|k| k.id == kit) {
                    return Err("Kit no longer exists.".into());
                }
                patch.selected_kit = kit;
            }
            PatchAction::AddLayer { channel, sample } => {
                if !store.list()?.iter().any(|s| s.id == sample) {
                    return Err("Sample no longer exists.".into());
                }
                let voice = graph.current_mut()?.voice_mut(channel)?;
                let id = Uuid::new_v4().to_string();
                voice.layers.push(Layer {
                    id: id.clone(),
                    sample_id: sample,
                });
                if voice.selected_layer.is_empty() {
                    voice.selected_layer = id;
                }
            }
            PatchAction::RemoveLayer { channel, layer } => {
                let voice = graph.current_mut()?.voice_mut(channel)?;
                let index = voice
                    .layers
                    .iter()
                    .position(|l| l.id == layer)
                    .ok_or("Layer no longer exists.")?;
                voice.layers.remove(index);
                if voice.selected_layer == layer {
                    voice.selected_layer = voice
                        .layers
                        .first()
                        .map(|l| l.id.clone())
                        .unwrap_or_default();
                }
            }
            PatchAction::MoveLayer { channel, layer, to } => {
                let voice = graph.current_mut()?.voice_mut(channel)?;
                if to >= voice.layers.len() {
                    return Err("Invalid Layer position.".into());
                }
                let from = voice
                    .layers
                    .iter()
                    .position(|l| l.id == layer)
                    .ok_or("Layer no longer exists.")?;
                let layer = voice.layers.remove(from);
                voice.layers.insert(to, layer);
            }
            PatchAction::SelectLayer { channel, layer } => {
                let voice = graph.current_mut()?.voice_mut(channel)?;
                if !voice.layers.iter().any(|l| l.id == layer) {
                    return Err("Layer no longer exists.".into());
                }
                voice.selected_layer = layer;
            }
            PatchAction::Musical { action } => {
                let patch = graph.current_mut()?;
                match action {
                    Action::SetMode { channel, mode } => patch.voice_mut(channel)?.mode = mode,
                    Action::SetBpm { value } if value.is_finite() => {
                        patch.bpm = value.clamp(30., 300.)
                    }
                    Action::AdjustBpm { delta } if delta.is_finite() => {
                        patch.bpm = (patch.bpm + delta).clamp(30., 300.)
                    }
                    Action::TogglePlayback => transport = Some(!legacy.playing),
                    Action::Stop => transport = Some(false),
                    Action::Audition { channel } if channel < 4 => trigger = Some(channel),
                    Action::BasicPattern => {
                        for (i, voice) in patch.voices.iter_mut().enumerate() {
                            let hits: &[usize] = match i {
                                0 => &[0, 4, 8, 12],
                                1 => &[4, 12],
                                2 => &[0, 2, 4, 6, 8, 10, 12, 14],
                                _ => &[6, 14],
                            };
                            voice.steps = (0..16).map(|s| hits.contains(&s)).collect();
                        }
                    }
                    Action::ClearPattern => {
                        patch.voices.iter_mut().for_each(|v| v.steps.fill(false))
                    }
                    action => {
                        let channel = match &action {
                            Action::ToggleStep { channel, .. }
                            | Action::SetVelocity { channel, .. }
                            | Action::SetVolume { channel, .. }
                            | Action::ToggleMute { channel }
                            | Action::ToggleSolo { channel } => *channel,
                            _ => return Err("This action belongs to the folder workflow.".into()),
                        };
                        let voice = patch.voices.get_mut(channel).ok_or("Invalid Voice.")?;
                        match action {
                            Action::ToggleStep { step, .. } if step < 16 => {
                                voice.steps[step] = !voice.steps[step]
                            }
                            Action::SetVelocity { step, value, .. } if step < 16 => {
                                voice.velocities[step] = value.clamp(1, 127)
                            }
                            Action::SetVolume { value, .. } if value.is_finite() => {
                                voice.volume = value.clamp(0., 1.)
                            }
                            Action::ToggleMute { .. } => voice.muted = !voice.muted,
                            Action::ToggleSolo { .. } => voice.solo = !voice.solo,
                            _ => return Err("Invalid musical control.".into()),
                        }
                    }
                }
            }
        }
        // Durable arrangement first. A missing/corrupt asset remains a visible draft issue.
        store.save_patches(&graph)?;
        let samples = store.list()?;
        let mut validation = vec![];
        for patch in &graph.patches {
            for kit in &patch.kits {
                let address = format!("{} · {}{}", patch.name, kit.bank, kit.number);
                if kit.voices[0].layers.is_empty() {
                    validation.push(format!("{address}: Voice 1 needs a Sample."));
                }
                for (i, voice) in kit.voices.iter().enumerate() {
                    if voice.layers.len() > 12 {
                        validation.push(format!("{address}: Voice {} has {} Layers; Rample supports at most 12. All draft Layers are retained.", i+1, voice.layers.len()));
                    }
                }
            }
        }
        let session = graph.current().map(Patch::session);
        let mut ready: [Vec<LayerSample>; 4] = std::array::from_fn(|_| vec![]);
        if let Some(kit) = graph.current().and_then(Patch::kit) {
            // Prepare each distinct Sample once, outside the audio callback.
            let mut prepared = HashMap::new();
            for (i, voice) in kit.voices.iter().enumerate() {
                for layer in &voice.layers {
                    let result = prepared
                        .entry(layer.sample_id.clone())
                        .or_insert_with(|| store.prepare_rample(&layer.sample_id));
                    match result {
                        Ok(sample) => ready[i].push(LayerSample {
                            id: layer.id.clone(),
                            sample: sample.clone(),
                        }),
                        Err(error) => validation.push(format!(
                            "Voice {}, Layer {}: {error}",
                            i + 1,
                            voice.layers.iter().position(|l| l.id == layer.id).unwrap() + 1
                        )),
                    }
                }
            }
        }
        if reset {
            self.audio.send(Command::Stop)?;
            self.audio.send(Command::Edit(None))?;
            legacy.playing = false;
        }
        self.patch_active.store(true, Ordering::Relaxed);
        self.audio.send(Command::Settings(
            session
                .as_ref()
                .map(|s| s.voices.clone())
                .unwrap_or_else(|| vec![Voice::default(); 4]),
            session.as_ref().map(|s| s.bpm).unwrap_or(120.),
        ))?;
        for (channel, bank) in ready.into_iter().enumerate() {
            self.audio.send(Command::LayerBank(channel, bank))?;
        }
        if let Some(playing) = transport {
            self.audio.send(if playing {
                Command::Play
            } else {
                Command::Stop
            })?;
            legacy.playing = playing;
        }
        if let Some(channel) = trigger {
            self.audio.send(Command::Audition(channel))?;
        }
        Ok(PatchSnapshot {
            graph,
            session,
            samples,
            validation,
            playing: legacy.playing,
        })
    }
    pub fn close_patch(&self) -> Result<(), String> {
        let mut legacy = self.library.lock().map_err(|_| "Library is unavailable")?;
        self.audio.send(Command::Stop)?;
        legacy.playing = false;
        // Explicitly restore all banks, including empty ones, after workspace handoff.
        for i in 0..4 {
            self.audio.send(Command::Bank(i, vec![]))?;
        }
        legacy.banks = std::array::from_fn(|_| vec![]);
        legacy.synchronize(&self.audio)?;
        self.patch_active.store(false, Ordering::Relaxed);
        Ok(())
    }
}
#[tauri::command]
pub async fn patch_action(
    action: PatchAction,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<PatchSnapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.patch_action(action))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn close_patch(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.close_patch())
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;
    #[test]
    fn patches_share_samples_and_persist_duplicate_occurrences_selection_and_drafts() {
        let (temp, state, _) = fixture();
        let legacy = state.snapshot().session;
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let id = state.import_samples(vec![source.clone()]).unwrap().samples[0]
            .id
            .clone();
        let run = |action| state.patch_action(action).unwrap();
        let first = run(PatchAction::CreatePatch {
            name: "Live set".into(),
        });
        let first_id = first.graph.selected_patch.clone();
        run(PatchAction::CreateKit {
            bank: "A".into(),
            number: 0,
        });
        let one = run(PatchAction::AddLayer {
            channel: 0,
            sample: id.clone(),
        });
        let selected = one.kit().unwrap().voices[0].selected_layer.clone();
        let two = run(PatchAction::AddLayer {
            channel: 0,
            sample: id.clone(),
        });
        let second = two.kit().unwrap().voices[0].layers[1].id.clone();
        assert_ne!(selected, second);
        run(PatchAction::SelectLayer {
            channel: 0,
            layer: second.clone(),
        });
        let moved = run(PatchAction::MoveLayer {
            channel: 0,
            layer: second.clone(),
            to: 0,
        });
        assert_eq!(moved.kit().unwrap().voices[0].selected_layer, second);
        assert_eq!(moved.kit().unwrap().voices[0].layers[0].id, second);
        run(PatchAction::Musical {
            action: Action::SetMode {
                channel: 0,
                mode: LayerMode::Reverse,
            },
        });
        run(PatchAction::Musical {
            action: Action::SetBpm { value: 137. },
        });
        run(PatchAction::Musical {
            action: Action::ToggleStep {
                channel: 0,
                step: 3,
            },
        });
        run(PatchAction::CreateKit {
            bank: "Z".into(),
            number: 99,
        });
        assert!(run(PatchAction::Open)
            .validation
            .iter()
            .any(|w| w.contains("Voice 1")));
        let first_kit = moved.kit().unwrap().id.clone();
        let restored = run(PatchAction::SelectKit { kit: first_kit });
        assert_eq!(restored.session.as_ref().unwrap().bpm, 137.);
        assert!(restored.session.as_ref().unwrap().voices[0].steps[3]);
        assert_eq!(restored.kit().unwrap().voices[0].mode, LayerMode::Reverse);
        assert_eq!(restored.kit().unwrap().voices[0].selected_layer, second);
        for _ in 0..11 {
            run(PatchAction::AddLayer {
                channel: 0,
                sample: id.clone(),
            });
        }
        let invalid = run(PatchAction::Open);
        assert_eq!(invalid.kit().unwrap().voices[0].layers.len(), 13);
        assert!(invalid.validation.iter().any(|w| w.contains("12")));
        run(PatchAction::CreatePatch {
            name: "Studio".into(),
        });
        run(PatchAction::CreateKit {
            bank: "B".into(),
            number: 12,
        });
        run(PatchAction::AddLayer {
            channel: 3,
            sample: id.clone(),
        });
        assert_eq!(state.list_samples().unwrap().len(), 1);
        std::fs::remove_file(source).unwrap();
        let restored = run(PatchAction::SelectPatch {
            patch: first_id.clone(),
        });
        assert_eq!(restored.kit().unwrap().voices[0].layers.len(), 13);
        state.close_patch().unwrap();
        assert_eq!(state.snapshot().session, legacy);
        let restarted = managed::ManagedLibrary::open(&temp.path().join("support")).unwrap();
        let graph = restarted.load_patches().unwrap();
        assert_eq!(graph.selected_patch, first_id);
        assert_eq!(graph.patches.len(), 2);
        assert_eq!(graph.current().unwrap().bpm, 137.);
        assert_eq!(
            graph.current().unwrap().kit().unwrap().voices[0].layers[0].id,
            second
        );
    }
}
