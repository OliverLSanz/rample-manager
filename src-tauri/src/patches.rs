//! Managed arrangements use occurrence IDs; physical audio paths stay in prepared assets.
use crate::*;
use rample_core::{audio::LayerSample, model::Voice};
use uuid::Uuid;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchGraph {
    pub selected_patch: String,
    pub patches: Vec<Patch>,
    /// The user's default Layer mode for new Kits and for imported Kits that
    /// were never saved with STORE. The device's global LAYER setting is not read.
    #[serde(default)]
    pub default_layer_mode: LayerMode,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    #[serde(default)]
    pub imported_sd: Option<crate::sd_import::ImportedSd>,
    pub id: String,
    pub name: String,
    pub selected_kit: String,
    pub kits: Vec<PatchKit>,
    pub bpm: f64,
    /// Sequencer/mixer memory is shared between the Kits of a Patch.
    pub voices: Vec<Voice>,
    #[serde(default)]
    pub voice_kits: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchKit {
    pub id: String,
    pub bank: String,
    pub number: u8,
    pub voices: Vec<PatchVoice>,
}
impl PatchKit {
    /// The Bank/Kit address, which is also its folder name, such as `A0`.
    pub fn address(&self) -> String {
        format!("{}{}", self.bank, self.number)
    }
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
    pub fn voice_kit(&self, channel: usize) -> Option<&PatchKit> {
        let Some(id) = self.voice_kits.get(channel) else {
            return self.kit();
        };
        self.kits.iter().find(|k| &k.id == id)
    }
    fn voice_mut(&mut self, channel: usize) -> Result<&mut PatchVoice, String> {
        let id = self
            .voice_kits
            .get(channel)
            .unwrap_or(&self.selected_kit)
            .clone();
        self.kits
            .iter_mut()
            .find(|k| k.id == id)
            .ok_or("Select a Kit first.")?
            .voices
            .get_mut(channel)
            .ok_or("Invalid Voice.".into())
    }
    fn session(&self) -> Session {
        let mut voices = self.voices.clone();
        for (channel, voice) in voices.iter_mut().enumerate() {
            if let Some(kit) = self.voice_kit(channel) {
                let arrangement = &kit.voices[channel];
                voice.kit_id = kit.id.clone();
                voice.sample_path = arrangement.selected_layer.clone();
                voice.layer_mode = arrangement.mode;
            }
        }
        Session {
            bpm: self.bpm,
            voices,
            selected_kit: self.selected_kit.clone(),
        }
    }
}
/// The last deleted Kit or Patch, kept in memory so it can be undone until
/// another deletion, an undo or a Library restore replaces it.
#[derive(Clone)]
pub(super) enum Deleted {
    Kit {
        patch: String,
        index: usize,
        kit: PatchKit,
        selected: bool,
    },
    Patch {
        index: usize,
        patch: Box<Patch>,
        selected: bool,
        /// SD intakes that recognized this Patch, so re-importing its card
        /// offers it again after undo.
        intakes: Vec<String>,
    },
    Layer {
        patch: String,
        kit: String,
        address: String,
        channel: usize,
        index: usize,
        layer: Layer,
        selected: bool,
    },
}
impl Deleted {
    fn label(&self) -> String {
        match self {
            Deleted::Kit { kit, .. } => format!("Kit {}", kit.address()),
            Deleted::Layer {
                address,
                channel,
                index,
                ..
            } => format!("Layer {} of {address} Voice {}", index + 1, channel + 1),
            Deleted::Patch { patch, .. } => format!("Patch {}", patch.name),
        }
    }
}
/// A copy with fresh Kit and Layer IDs, keeping each Voice's selection.
fn copy_kit(kit: &PatchKit) -> PatchKit {
    PatchKit {
        id: Uuid::new_v4().to_string(),
        bank: kit.bank.clone(),
        number: kit.number,
        voices: kit
            .voices
            .iter()
            .map(|voice| {
                let layers: Vec<Layer> = voice
                    .layers
                    .iter()
                    .map(|l| Layer {
                        id: Uuid::new_v4().to_string(),
                        sample_id: l.sample_id.clone(),
                    })
                    .collect();
                let selected = voice
                    .layers
                    .iter()
                    .position(|l| l.id == voice.selected_layer)
                    .and_then(|i| layers.get(i))
                    .map(|l| l.id.clone())
                    .unwrap_or_default();
                PatchVoice {
                    layers,
                    selected_layer: selected,
                    mode: voice.mode,
                }
            })
            .collect(),
    }
}
/// After removing the selected Kit, select its neighbour in Rample order.
fn neighbour(kits: &[PatchKit], removed: &PatchKit) -> String {
    let mut sorted: Vec<&PatchKit> = kits.iter().collect();
    sorted.sort_by(|a, b| a.bank.cmp(&b.bank).then(a.number.cmp(&b.number)));
    sorted
        .iter()
        .find(|k| (&k.bank, k.number) > (&removed.bank, removed.number))
        .or(sorted.last())
        .map(|k| k.id.clone())
        .unwrap_or_default()
}
#[derive(Clone)]
pub(super) struct PatchPreview {
    key: String,
    samples: Vec<managed::LibrarySample>,
    errors: Vec<String>,
    pub(super) ready: [Vec<LayerSample>; 4],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSnapshot {
    pub graph: PatchGraph,
    pub session: Option<Session>,
    pub samples: Vec<managed::LibrarySample>,
    /// Normalized peaks of each prepared Layer of the playing Voices, by Layer ID.
    /// They describe the audio actually auditioned, including saved Sample Edits.
    pub waveforms: BTreeMap<String, Vec<f32>>,
    pub validation: Vec<String>,
    pub playing: bool,
    /// What `UndoDelete` would restore, such as `Kit A3`.
    pub undo: Option<String>,
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
    SetDefaultMode {
        mode: LayerMode,
    },
    CreateKit {
        bank: String,
        number: u8,
    },
    /// Moves `kit`, or the selected Kit when absent, to another address.
    AddressKit {
        bank: String,
        number: u8,
        #[serde(default)]
        kit: Option<String>,
    },
    DuplicateKit {
        kit: String,
        bank: String,
        number: u8,
    },
    DeleteKit {
        kit: String,
    },
    DuplicatePatch {
        patch: String,
        name: String,
    },
    DeletePatch {
        patch: String,
    },
    /// Restores the last deleted Kit or Patch.
    UndoDelete,
    SelectKit {
        kit: String,
    },
    AddLayer {
        channel: usize,
        sample: String,
    },
    /// Adds a Layer to Voice `channel` of any Kit in any Patch, without
    /// changing the open Patch or Kit.
    AddLayerTo {
        patch: String,
        kit: String,
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
/// Appends a Layer to the playing Voice, or to `channel` of `(patch, kit)`.
fn add_layer(
    graph: &mut PatchGraph,
    store: &managed::ManagedLibrary,
    target: Option<(String, String)>,
    channel: usize,
    sample: String,
) -> Result<(), String> {
    if !store.list()?.iter().any(|s| s.id == sample && !s.retired) {
        return Err("Sample is unavailable. Restore retired Samples from Trash first.".into());
    }
    let (patch, voice) = match &target {
        Some((patch, kit)) => {
            let patch = graph
                .patches
                .iter_mut()
                .find(|p| &p.id == patch)
                .ok_or("Patch no longer exists.")?;
            let index = patch
                .kits
                .iter()
                .position(|k| &k.id == kit)
                .ok_or("Kit no longer exists.")?;
            if channel >= patch.kits[index].voices.len() {
                return Err("Invalid Voice.".into());
            }
            (patch, (index, channel))
        }
        None => {
            let patch = graph.current_mut()?;
            let id = patch
                .voice_kits
                .get(channel)
                .unwrap_or(&patch.selected_kit)
                .clone();
            let index = patch
                .kits
                .iter()
                .position(|k| k.id == id)
                .ok_or("Select a Kit first.")?;
            if channel >= patch.kits[index].voices.len() {
                return Err("Invalid Voice.".into());
            }
            (patch, (index, channel))
        }
    };
    let voice = &mut patch.kits[voice.0].voices[voice.1];
    let id = Uuid::new_v4().to_string();
    voice.layers.push(Layer {
        id: id.clone(),
        sample_id: sample.clone(),
    });
    if voice.selected_layer.is_empty() {
        voice.selected_layer = id;
    }
    if let Some(imported) = &mut patch.imported_sd {
        if let Some(index) = imported
            .unassigned
            .iter()
            .position(|file| file.sample_id.as_deref() == Some(&sample))
        {
            imported.unassigned.remove(index);
        }
    }
    Ok(())
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
/// Rample structure rules shared by draft validation and Build Patch. Drafts keep
/// every Layer; these issues only block a build.
pub(super) fn kit_issues(patch: &Patch) -> Vec<String> {
    let mut issues = vec![];
    for kit in &patch.kits {
        let address = kit.address();
        if !catalog::is_kit_name(&address) || kit.number > 99 {
            issues.push(format!(
                "{address}: Use a Bank from A to Z and a Kit number from 0 to 99."
            ));
        }
        if patch
            .kits
            .iter()
            .any(|other| other.id != kit.id && other.bank == kit.bank && other.number == kit.number)
        {
            issues.push(format!(
                "{address}: Another Kit uses the same Bank/Kit address."
            ));
        }
        if kit
            .voices
            .first()
            .is_none_or(|voice| voice.layers.is_empty())
        {
            issues.push(format!("{address}: Voice 1 needs a Sample."));
        }
        for (i, voice) in kit.voices.iter().enumerate() {
            if voice.layers.len() > 12 {
                issues.push(format!("{address}: Voice {} has {} Layers; Rample supports at most 12. All draft Layers are retained.", i + 1, voice.layers.len()));
            }
        }
    }
    issues
}
impl AppState {
    pub fn patch_action(&self, action: PatchAction) -> Result<PatchSnapshot, String> {
        // Lock order: workspace, then Library, as in every other command.
        let mut workspace = self
            .workspace
            .lock()
            .map_err(|_| "Library is unavailable")?;
        let store = self.managed.lock().map_err(|_| "Library is unavailable")?;
        self.patch_action_locked(action, &mut workspace, &store)
    }
    pub(super) fn patch_action_locked(
        &self,
        action: PatchAction,
        workspace: &mut Workspace,
        store: &managed::ManagedLibrary,
    ) -> Result<PatchSnapshot, String> {
        let mut graph = store.load_patches()?;
        let refresh = matches!(&action, PatchAction::Open);
        let persist = !matches!(
            &action,
            PatchAction::Open
                | PatchAction::Musical {
                    action: Action::Stop | Action::TogglePlayback | Action::Audition { .. }
                }
        );
        let mut trigger = None;
        let mut transport = None;
        let mut deleted = None;
        let mut undone = false;
        // SD intakes whose recognized Patch changes with this action.
        let mut intake_links: Vec<(String, Option<String>)> = vec![];
        // The first action after startup or a Library restore loads every bank.
        let mut reset = workspace.patch_preview.is_none();
        match action {
            PatchAction::Open => {}
            PatchAction::CreatePatch { name } => {
                valid_name(&name)?;
                let id = Uuid::new_v4().to_string();
                graph.patches.push(Patch {
                    id: id.clone(),
                    imported_sd: None,
                    name,
                    selected_kit: String::new(),
                    kits: vec![],
                    bpm: 120.,
                    voices: vec![Voice::default(); 4],
                    voice_kits: vec![],
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
            PatchAction::SetDefaultMode { mode } => {
                // Applies to Kits created or imported from now on; existing
                // Kits keep their own modes.
                graph.default_layer_mode = mode;
            }
            PatchAction::CreateKit { bank, number } => {
                let mode = graph.default_layer_mode;
                let patch = graph.current_mut()?;
                valid_address(patch, &bank, number, "")?;
                let id = Uuid::new_v4().to_string();
                let voice = PatchVoice {
                    mode,
                    ..PatchVoice::default()
                };
                patch.kits.push(PatchKit {
                    id: id.clone(),
                    bank,
                    number,
                    voices: vec![voice; 4],
                });
                patch.selected_kit = id;
                patch.voice_kits.clear();
            }
            PatchAction::AddressKit { bank, number, kit } => {
                let patch = graph.current_mut()?;
                let id = kit.unwrap_or_else(|| patch.selected_kit.clone());
                valid_address(patch, &bank, number, &id)?;
                let kit = patch
                    .kits
                    .iter_mut()
                    .find(|k| k.id == id)
                    .ok_or("Kit no longer exists.")?;
                kit.bank = bank;
                kit.number = number;
            }
            PatchAction::DuplicateKit { kit, bank, number } => {
                let patch = graph.current_mut()?;
                valid_address(patch, &bank, number, "")?;
                let source = patch
                    .kits
                    .iter()
                    .find(|k| k.id == kit)
                    .ok_or("Kit no longer exists.")?;
                let mut copy = copy_kit(source);
                copy.bank = bank;
                copy.number = number;
                patch.selected_kit = copy.id.clone();
                patch.kits.push(copy);
                patch.voice_kits.clear();
            }
            PatchAction::DeleteKit { kit } => {
                let patch = graph.current_mut()?;
                let index = patch
                    .kits
                    .iter()
                    .position(|k| k.id == kit)
                    .ok_or("Kit no longer exists.")?;
                let removed = patch.kits.remove(index);
                let selected = patch.selected_kit == removed.id;
                if selected {
                    patch.selected_kit = neighbour(&patch.kits, &removed);
                    patch.voice_kits.clear();
                }
                if patch.voice_kits.contains(&removed.id) {
                    patch.voice_kits.clear();
                }
                deleted = Some(Deleted::Kit {
                    patch: patch.id.clone(),
                    index,
                    kit: removed,
                    selected,
                });
            }
            PatchAction::DuplicatePatch { patch, name } => {
                valid_name(&name)?;
                let index = graph
                    .patches
                    .iter()
                    .position(|p| p.id == patch)
                    .ok_or("Patch no longer exists.")?;
                let source = &graph.patches[index];
                let ids: HashMap<String, String> = source
                    .kits
                    .iter()
                    .map(|k| (k.id.clone(), Uuid::new_v4().to_string()))
                    .collect();
                let copy = Patch {
                    id: Uuid::new_v4().to_string(),
                    // A copy is not the card it was imported from: its Import
                    // report and recognition stay with the original.
                    imported_sd: None,
                    name,
                    selected_kit: ids.get(&source.selected_kit).cloned().unwrap_or_default(),
                    kits: source
                        .kits
                        .iter()
                        .map(|k| PatchKit {
                            id: ids[&k.id].clone(),
                            ..copy_kit(k)
                        })
                        .collect(),
                    bpm: source.bpm,
                    voices: source.voices.clone(),
                    voice_kits: source
                        .voice_kits
                        .iter()
                        .filter_map(|id| ids.get(id).cloned())
                        .collect(),
                };
                graph.selected_patch = copy.id.clone();
                graph.patches.insert(index + 1, copy);
                reset = true;
            }
            PatchAction::DeletePatch { patch } => {
                let index = graph
                    .patches
                    .iter()
                    .position(|p| p.id == patch)
                    .ok_or("Patch no longer exists.")?;
                let removed = graph.patches.remove(index);
                let selected = graph.selected_patch == removed.id;
                if selected {
                    graph.selected_patch = graph
                        .patches
                        .get(index)
                        .or(graph.patches.last())
                        .map(|p| p.id.clone())
                        .unwrap_or_default();
                    reset = true;
                }
                let intakes = store.intakes_of(&removed.id)?;
                intake_links.extend(intakes.iter().map(|id| (id.clone(), None)));
                deleted = Some(Deleted::Patch {
                    index,
                    patch: Box::new(removed),
                    selected,
                    intakes,
                });
            }
            PatchAction::UndoDelete => {
                match workspace
                    .deleted
                    .clone()
                    .ok_or("There is nothing to undo.")?
                {
                    Deleted::Kit {
                        patch,
                        index,
                        kit,
                        selected,
                    } => {
                        let target = graph
                            .patches
                            .iter_mut()
                            .find(|p| p.id == patch)
                            .ok_or("The Patch of that Kit no longer exists.")?;
                        if target
                            .kits
                            .iter()
                            .any(|k| k.bank == kit.bank && k.number == kit.number)
                        {
                            return Err(format!(
                                "{} is now used by another Kit. Move that Kit, then undo again.",
                                kit.address()
                            ));
                        }
                        if selected {
                            target.selected_kit = kit.id.clone();
                            target.voice_kits.clear();
                        }
                        let index = index.min(target.kits.len());
                        target.kits.insert(index, kit);
                        if graph.selected_patch != patch {
                            graph.selected_patch = patch;
                            reset = true;
                        }
                    }
                    Deleted::Layer {
                        patch,
                        kit,
                        channel,
                        index,
                        layer,
                        selected,
                        ..
                    } => {
                        let target = graph
                            .patches
                            .iter_mut()
                            .find(|p| p.id == patch)
                            .ok_or("The Patch of that Layer no longer exists.")?;
                        let shown = target.voice_kit(channel).map(|k| k.id.clone());
                        let voice = target
                            .kits
                            .iter_mut()
                            .find(|k| k.id == kit)
                            .ok_or("The Kit of that Layer no longer exists.")?
                            .voices
                            .get_mut(channel)
                            .ok_or("Invalid Voice.")?;
                        if selected || voice.layers.is_empty() {
                            voice.selected_layer = layer.id.clone();
                        }
                        let index = index.min(voice.layers.len());
                        voice.layers.insert(index, layer);
                        // Show the restored Layer where it belongs.
                        if shown.as_deref() != Some(kit.as_str()) {
                            target.selected_kit = kit;
                            target.voice_kits.clear();
                        }
                        if graph.selected_patch != patch {
                            graph.selected_patch = patch;
                            reset = true;
                        }
                    }
                    Deleted::Patch {
                        index,
                        patch,
                        selected,
                        intakes,
                    } => {
                        if selected || graph.patches.is_empty() {
                            graph.selected_patch = patch.id.clone();
                            reset = true;
                        }
                        intake_links
                            .extend(intakes.into_iter().map(|id| (id, Some(patch.id.clone()))));
                        let index = index.min(graph.patches.len());
                        graph.patches.insert(index, *patch);
                    }
                }
                undone = true;
            }
            PatchAction::SelectKit { kit } => {
                let patch = graph.current_mut()?;
                if !patch.kits.iter().any(|k| k.id == kit) {
                    return Err("Kit no longer exists.".into());
                }
                patch.selected_kit = kit;
                patch.voice_kits.clear();
            }
            PatchAction::AddLayer { channel, sample } => {
                add_layer(&mut graph, store, None, channel, sample)?;
            }
            PatchAction::AddLayerTo {
                patch,
                kit,
                channel,
                sample,
            } => {
                add_layer(&mut graph, store, Some((patch, kit)), channel, sample)?;
            }
            PatchAction::RemoveLayer { channel, layer } => {
                let patch = graph.current_mut()?;
                let kit = patch
                    .voice_kits
                    .get(channel)
                    .unwrap_or(&patch.selected_kit)
                    .clone();
                let patch_id = patch.id.clone();
                let address = patch
                    .kits
                    .iter()
                    .find(|k| k.id == kit)
                    .map(PatchKit::address)
                    .unwrap_or_default();
                let voice = patch.voice_mut(channel)?;
                let index = voice
                    .layers
                    .iter()
                    .position(|l| l.id == layer)
                    .ok_or("Layer no longer exists.")?;
                let removed = voice.layers.remove(index);
                let selected = voice.selected_layer == layer;
                if selected {
                    voice.selected_layer = voice
                        .layers
                        .first()
                        .map(|l| l.id.clone())
                        .unwrap_or_default();
                }
                deleted = Some(Deleted::Layer {
                    patch: patch_id,
                    kit,
                    address,
                    channel,
                    index,
                    layer: removed,
                    selected,
                });
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
                    Action::TogglePlayback => transport = Some(!workspace.playing),
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
                            _ => return Err("Invalid musical control.".into()),
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
        if persist {
            if intake_links.is_empty() {
                store.save_patches(&graph)?;
            } else {
                store.save_patches_and_intakes(&graph, &intake_links)?;
            }
        }
        if deleted.is_some() {
            workspace.deleted = deleted;
        } else if undone {
            workspace.deleted = None;
        }
        let mut validation = vec![];
        for patch in &graph.patches {
            if let Some(imported) = &patch.imported_sd {
                if !imported.unassigned.is_empty() {
                    validation.push(format!("{}: {} unassigned source occurrences need explicit placement with Add Sample. Their audio is kept in the Library.", patch.name, imported.unassigned.len()));
                }
                match &imported.native {
                    // Native settings are read-only (ADR 0007): only report
                    // what Import could not restore, never a pending write.
                    Some(native) => {
                        for issue in native.all_issues() {
                            validation.push(format!("{}: {issue}", patch.name));
                        }
                    }
                    None => validation.push(format!("{}: Layer modes were not read from this SD Import. Import the SD again and choose Update existing Patch to read its stored Layer modes.", patch.name)),
                }
            }
            validation.extend(
                kit_issues(patch)
                    .into_iter()
                    .map(|issue| format!("{} · {issue}", patch.name)),
            );
        }
        let session = graph.current().map(Patch::session);
        // Musical controls reuse ready assets: no probing, hashing, decoding or
        // bank replacement for tempo, sequencer, mixer, selection or mode changes.
        // Future saved-revision publication must invalidate this transient cache.
        let key = serde_json::to_string(&graph.current().map(|patch| {
            (
                &patch.selected_kit,
                (0..4)
                    .map(|channel| {
                        patch
                            .voice_kit(channel)
                            .map(|kit| &kit.voices[channel].layers)
                    })
                    .collect::<Vec<_>>(),
            )
        }))
        .map_err(|e| e.to_string())?;
        let reload = refresh
            || workspace
                .patch_preview
                .as_ref()
                .is_none_or(|cache| cache.key != key);
        let preview = if reload {
            let mut ready: [Vec<LayerSample>; 4] = std::array::from_fn(|_| vec![]);
            let mut errors = vec![];
            if let Some(patch) = graph.current() {
                let mut prepared = HashMap::new();
                for (i, bank) in ready.iter_mut().enumerate() {
                    let Some(kit) = patch.voice_kit(i) else {
                        continue;
                    };
                    let voice = &kit.voices[i];
                    for (position, layer) in voice.layers.iter().enumerate() {
                        let result = prepared
                            .entry(layer.sample_id.clone())
                            .or_insert_with(|| store.prepare_rample(&layer.sample_id));
                        match result {
                            Ok(sample) => bank.push(LayerSample {
                                id: layer.id.clone(),
                                sample: sample.clone(),
                            }),
                            Err(error) => errors.push(format!(
                                "Voice {}, Layer {}: {error}",
                                i + 1,
                                position + 1
                            )),
                        }
                    }
                }
            }
            PatchPreview {
                key,
                samples: store.list()?,
                errors,
                ready,
            }
        } else {
            workspace.patch_preview.as_ref().unwrap().clone()
        };
        validation.extend(preview.errors.clone());
        if reset {
            self.audio.send(Command::Stop)?;
            self.audio.send(Command::Edit(None))?;
            workspace.playing = false;
        }
        self.audio.send(Command::Settings(
            session
                .as_ref()
                .map(|s| s.voices.clone())
                .unwrap_or_else(|| vec![Voice::default(); 4]),
            session.as_ref().map(|s| s.bpm).unwrap_or(120.),
        ))?;
        if reload || reset {
            for (channel, bank) in preview.ready.iter().enumerate() {
                self.audio.send(Command::LayerBank(channel, bank.clone()))?;
            }
        }
        workspace.patch_preview = Some(preview.clone());
        if let Some(playing) = transport {
            self.audio.send(if playing {
                Command::Play
            } else {
                Command::Stop
            })?;
            workspace.playing = playing;
        }
        if let Some(channel) = trigger {
            self.audio.send(Command::Audition(channel))?;
        }
        let waveforms = preview
            .ready
            .iter()
            .flatten()
            .map(|layer| (layer.id.clone(), layer.sample.waveform.clone()))
            .collect();
        Ok(PatchSnapshot {
            graph,
            session,
            samples: preview.samples,
            waveforms,
            validation,
            playing: workspace.playing,
            undo: workspace.deleted.as_ref().map(Deleted::label),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;
    #[test]
    fn musical_controls_keep_ready_audio_until_arrangement_refresh() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let id = state.import_samples(vec![source]).unwrap().samples[0]
            .id
            .clone();
        state
            .patch_action(PatchAction::CreatePatch { name: "Set".into() })
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
                sample: id,
            })
            .unwrap();
        // Owned storage loss should be reported on refresh, not turn a mixer change
        // into a new decode or abruptly replace an already-prepared playing bank.
        std::fs::rename(
            temp.path().join("support/Library/Originals"),
            temp.path().join("safe-originals"),
        )
        .unwrap();
        let changed = state
            .patch_action(PatchAction::Musical {
                action: Action::ToggleMute { channel: 0 },
            })
            .unwrap();
        assert!(changed.validation.is_empty());
        assert!(changed.session.unwrap().voices[0].muted);
        let layer = changed.graph.current().unwrap().kit().unwrap().voices[0].layers[0]
            .id
            .clone();
        // Musical controls reuse the prepared peaks instead of reading audio again.
        assert_eq!(changed.waveforms[&layer].len(), 100);
        assert!(changed.waveforms[&layer].iter().any(|peak| *peak > 0.99));
        let refreshed = state.patch_action(PatchAction::Open).unwrap();
        assert!(refreshed
            .validation
            .iter()
            .any(|issue| issue.contains("Layer 1")));
    }
    #[test]
    fn patches_share_samples_and_persist_duplicate_occurrences_selection_and_drafts() {
        let (temp, state, _) = fixture();
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

    fn kit_at<'a>(view: &'a PatchSnapshot, address: &str) -> Option<&'a PatchKit> {
        view.graph
            .current()?
            .kits
            .iter()
            .find(|k| k.address() == address)
    }
    #[test]
    fn deleted_kits_select_a_neighbour_and_undo_restores_them_exactly() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0]
            .id
            .clone();
        let run = |action| state.patch_action(action).unwrap();
        run(PatchAction::CreatePatch { name: "Set".into() });
        for number in [0, 1, 2] {
            run(PatchAction::CreateKit {
                bank: "A".into(),
                number,
            });
        }
        run(PatchAction::SelectKit {
            kit: kit_at(&run(PatchAction::Open), "A1").unwrap().id.clone(),
        });
        let with_layer = run(PatchAction::AddLayer { channel: 0, sample });
        let original = kit_at(&with_layer, "A1").unwrap().clone();
        assert_eq!(original.voices[0].layers.len(), 1);
        let deleted = run(PatchAction::DeleteKit {
            kit: original.id.clone(),
        });
        assert!(kit_at(&deleted, "A1").is_none());
        // The next Kit in Rample order is selected.
        assert_eq!(deleted.kit().unwrap().address(), "A2");
        assert_eq!(deleted.undo.as_deref(), Some("Kit A1"));
        // Other edits in between do not prevent undo.
        run(PatchAction::Musical {
            action: Action::SetBpm { value: 99. },
        });
        let restored = run(PatchAction::UndoDelete);
        let kit = kit_at(&restored, "A1").unwrap();
        assert_eq!(kit.id, original.id);
        assert_eq!(kit.voices[0].layers[0].id, original.voices[0].layers[0].id);
        assert_eq!(restored.kit().unwrap().id, original.id);
        assert_eq!(restored.undo, None);
        assert!(state.patch_action(PatchAction::UndoDelete).is_err());
        // Undo survives saving: the restored Kit is durable.
        assert!(kit_at(&run(PatchAction::Open), "A1").is_some());
        // An address reused meanwhile is reported, keeping the undo.
        let a2 = kit_at(&restored, "A2").unwrap().id.clone();
        run(PatchAction::DeleteKit { kit: original.id });
        run(PatchAction::AddressKit {
            bank: "A".into(),
            number: 1,
            kit: Some(a2),
        });
        let error = state.patch_action(PatchAction::UndoDelete).err().unwrap();
        assert!(error.contains("A1 is now used by another Kit"), "{error}");
        assert_eq!(run(PatchAction::Open).undo.as_deref(), Some("Kit A1"));
    }
    #[test]
    fn duplicated_kits_and_patches_get_new_identities_and_keep_their_arrangement() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0]
            .id
            .clone();
        let run = |action| state.patch_action(action).unwrap();
        run(PatchAction::CreatePatch { name: "Set".into() });
        run(PatchAction::CreateKit {
            bank: "A".into(),
            number: 0,
        });
        run(PatchAction::AddLayer {
            channel: 1,
            sample: sample.clone(),
        });
        let two = run(PatchAction::AddLayer { channel: 1, sample });
        let second = two.kit().unwrap().voices[1].layers[1].id.clone();
        let source = run(PatchAction::SelectLayer {
            channel: 1,
            layer: second,
        });
        let kit = source.kit().unwrap().clone();
        assert!(state
            .patch_action(PatchAction::DuplicateKit {
                kit: kit.id.clone(),
                bank: "A".into(),
                number: 0,
            })
            .is_err());
        let copied = run(PatchAction::DuplicateKit {
            kit: kit.id.clone(),
            bank: "C".into(),
            number: 7,
        });
        let copy = copied.kit().unwrap();
        assert_eq!(copy.address(), "C7");
        assert_ne!(copy.id, kit.id);
        assert_ne!(copy.voices[1].layers[0].id, kit.voices[1].layers[0].id);
        assert_eq!(copy.voices[1].layers.len(), 2);
        // The selection follows the same position.
        assert_eq!(copy.voices[1].selected_layer, copy.voices[1].layers[1].id);
        assert_eq!(kit_at(&copied, "A0").unwrap().id, kit.id);

        let patch = copied.graph.selected_patch.clone();
        let duplicated = run(PatchAction::DuplicatePatch {
            patch: patch.clone(),
            name: "Set copy".into(),
        });
        let copy = duplicated.graph.current().unwrap();
        assert_eq!(copy.name, "Set copy");
        assert_ne!(copy.id, patch);
        assert_eq!(copy.kits.len(), 2);
        assert_eq!(copy.kit().unwrap().address(), "C7");
        let original = duplicated
            .graph
            .patches
            .iter()
            .find(|p| p.id == patch)
            .unwrap();
        assert!(copy
            .kits
            .iter()
            .all(|k| original.kits.iter().all(|o| o.id != k.id)));
        // Editing the copy leaves the original alone.
        let edited = run(PatchAction::RemoveLayer {
            channel: 1,
            layer: copy.kit().unwrap().voices[1].layers[0].id.clone(),
        });
        let original = edited.graph.patches.iter().find(|p| p.id == patch).unwrap();
        assert_eq!(original.kit().unwrap().voices[1].layers.len(), 2);
    }

    #[test]
    fn layers_can_be_added_to_any_kit_without_changing_the_open_one() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0]
            .id
            .clone();
        // A Sample's waveform is its current Rample sound, read off the callback.
        let peaks = state.sample_waveform(sample.clone()).unwrap();
        assert_eq!(peaks.len(), 100);
        assert!(peaks.iter().any(|p| *p > 0.99));
        let run = |action| state.patch_action(action).unwrap();
        let other = run(PatchAction::CreatePatch {
            name: "Other".into(),
        });
        let other_kit = run(PatchAction::CreateKit {
            bank: "B".into(),
            number: 3,
        });
        let (other, other_kit) = (
            other.graph.selected_patch,
            other_kit.kit().unwrap().id.clone(),
        );
        run(PatchAction::CreatePatch {
            name: "Open".into(),
        });
        let open = run(PatchAction::CreateKit {
            bank: "A".into(),
            number: 0,
        });
        let added = run(PatchAction::AddLayerTo {
            patch: other.clone(),
            kit: other_kit.clone(),
            channel: 2,
            sample: sample.clone(),
        });
        assert_eq!(added.graph.selected_patch, open.graph.selected_patch);
        assert!(added.kit().unwrap().voices[2].layers.is_empty());
        let target = added.graph.patches.iter().find(|p| p.id == other).unwrap();
        let voice = &target.kits[0].voices[2];
        assert_eq!(voice.layers[0].sample_id, sample);
        assert_eq!(voice.selected_layer, voice.layers[0].id);
        assert!(state
            .patch_action(PatchAction::AddLayerTo {
                patch: other,
                kit: "missing".into(),
                channel: 0,
                sample,
            })
            .is_err());
    }

    #[test]
    fn removed_layers_come_back_in_place_with_undo() {
        let (temp, state, _) = fixture();
        let source = temp.path().join("Kick.wav");
        wav::write_pcm16(&source, &[8192; 4410], 44100).unwrap();
        let sample = state.import_samples(vec![source]).unwrap().samples[0]
            .id
            .clone();
        let run = |action| state.patch_action(action).unwrap();
        run(PatchAction::CreatePatch { name: "Set".into() });
        run(PatchAction::CreateKit {
            bank: "A".into(),
            number: 4,
        });
        for _ in 0..3 {
            run(PatchAction::AddLayer {
                channel: 1,
                sample: sample.clone(),
            });
        }
        let before = run(PatchAction::Open).kit().unwrap().voices[1].clone();
        let middle = before.layers[1].id.clone();
        run(PatchAction::SelectLayer {
            channel: 1,
            layer: middle.clone(),
        });
        let removed = run(PatchAction::RemoveLayer {
            channel: 1,
            layer: middle.clone(),
        });
        assert_eq!(removed.kit().unwrap().voices[1].layers.len(), 2);
        assert_eq!(removed.undo.as_deref(), Some("Layer 2 of A4 Voice 2"));
        // Undo also works after switching to another Kit, and shows it again.
        let a4 = removed.kit().unwrap().id.clone();
        run(PatchAction::CreateKit {
            bank: "A".into(),
            number: 5,
        });
        let restored = run(PatchAction::UndoDelete);
        assert_eq!(restored.kit().unwrap().id, a4);
        let voice = &restored.kit().unwrap().voices[1];
        assert_eq!(
            voice.layers.iter().map(|l| &l.id).collect::<Vec<_>>(),
            before.layers.iter().map(|l| &l.id).collect::<Vec<_>>()
        );
        assert_eq!(voice.selected_layer, middle);
    }
}
