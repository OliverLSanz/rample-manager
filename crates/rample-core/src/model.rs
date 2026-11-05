use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LayerMode {
    #[default]
    #[serde(rename = "manual")]
    Single,
    #[serde(rename = "random")]
    Random,
    #[serde(rename = "cyclic")]
    Sequential,
    #[serde(rename = "reverseCyclic")]
    Reverse,
    #[serde(rename = "velocity")]
    Velocity,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Voice {
    #[serde(rename = "kitID")]
    pub kit_id: String,
    pub sample_path: String,
    pub steps: Vec<bool>,
    pub volume: f32,
    pub muted: bool,
    pub solo: bool,
    pub layer_mode: LayerMode,
    pub velocities: Vec<u8>,
}
impl Default for Voice {
    fn default() -> Self {
        Self {
            kit_id: String::new(),
            sample_path: String::new(),
            steps: vec![false; 16],
            volume: 0.8,
            muted: false,
            solo: false,
            layer_mode: LayerMode::Single,
            velocities: vec![100; 16],
        }
    }
}
impl Voice {
    pub fn normalize(&mut self) {
        self.steps.resize(16, false);
        self.steps.truncate(16);
        self.velocities.resize(16, 100);
        self.velocities.truncate(16);
        self.velocities
            .iter_mut()
            .for_each(|v| *v = (*v).clamp(1, 127));
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0., 1.)
        } else {
            0.8
        };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
/// The open Patch as the sequencer plays it: its selected Kit, tempo and Voices.
pub struct Session {
    pub selected_kit: String,
    pub bpm: f64,
    pub voices: Vec<Voice>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            selected_kit: String::new(),
            bpm: 120.,
            voices: vec![Voice::default(); 4],
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct LayerSelector {
    pub last: Option<String>,
    rng: u64,
}
impl LayerSelector {
    pub fn reset(&mut self) {
        self.last = None;
    }
    pub fn choose(
        &mut self,
        paths: &[String],
        selected: &str,
        mode: LayerMode,
        velocity: u8,
    ) -> Option<usize> {
        if paths.is_empty() || selected.is_empty() {
            return None;
        }
        let anchor = paths.iter().position(|p| p == selected).unwrap_or(0);
        let previous = self
            .last
            .as_ref()
            .and_then(|p| paths.iter().position(|x| x == p));
        let index = match mode {
            LayerMode::Single => paths.iter().position(|p| p == selected)?,
            LayerMode::Sequential => previous.map(|n| (n + 1) % paths.len()).unwrap_or(anchor),
            LayerMode::Reverse => previous
                .map(|n| (n + paths.len() - 1) % paths.len())
                .unwrap_or(anchor),
            LayerMode::Velocity => {
                (usize::from(velocity.clamp(1, 127)) * paths.len() / 128).min(paths.len() - 1)
            }
            LayerMode::Random => {
                if self.rng == 0 {
                    self.rng = rand::random::<u64>().max(1)
                }
                self.rng ^= self.rng << 13;
                self.rng ^= self.rng >> 7;
                self.rng ^= self.rng << 17;
                (self.rng as usize) % paths.len()
            }
        };
        self.last = Some(paths[index].clone());
        Some(index)
    }
}
