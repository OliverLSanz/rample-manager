use crate::{
    catalog::{self, Catalog},
    conversion,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewedFile {
    pub id: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub fingerprint: String,
    pub name: String,
    pub kit: String,
    pub issues: Vec<String>,
    pub convertible: bool,
    pub target: String,
}
#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
struct LibraryReview {
    fingerprints: BTreeMap<String, String>,
    pending: BTreeSet<String>,
}
#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reviews {
    libraries: BTreeMap<String, LibraryReview>,
}
impl Reviews {
    pub fn load(support: &Path) -> Result<Self, String> {
        let path = support.join("sample-reviews.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| {
            format!("Could not read sample reviews: {e}. The original file is preserved.")
        })
    }
    pub fn save(&self, support: &Path) -> Result<(), String> {
        fs::create_dir_all(support).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(support).map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(support.join("sample-reviews.json"))
            .map_err(|e| e.error.to_string())?;
        Ok(())
    }
    pub fn update(&mut self, root: &str, catalog: &Catalog) -> bool {
        if catalog.error.is_some() {
            return false;
        }
        let previous = self.libraries.entry(root.into()).or_default();
        let mut next = LibraryReview::default();
        for sample in catalog
            .kits
            .iter()
            .flat_map(|k| &k.samples)
            .chain(&catalog.unassigned)
        {
            let stamp = catalog::fingerprint(Path::new(&sample.id));
            if !sample.warnings.is_empty()
                && (previous.fingerprints.get(&sample.id) != Some(&stamp)
                    || previous.pending.contains(&sample.id))
            {
                next.pending.insert(sample.id.clone());
            }
            next.fingerprints.insert(sample.id.clone(), stamp);
        }
        let changed = previous != &next;
        *previous = next;
        changed
    }
    pub fn pending(&self, root: &str, catalog: &Catalog) -> Vec<Candidate> {
        let Some(state) = self.libraries.get(root) else {
            return vec![];
        };
        catalog
            .kits
            .iter()
            .flat_map(|kit| {
                kit.samples
                    .iter()
                    .map(move |sample| (kit.id.as_str(), sample))
            })
            .chain(
                catalog
                    .unassigned
                    .iter()
                    .map(|sample| ("Outside a kit", sample)),
            )
            .filter_map(|(kit, sample)| {
                if !state.pending.contains(&sample.id) {
                    return None;
                }
                let path = Path::new(&sample.id);
                let target = if path
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
                {
                    path.to_owned()
                } else {
                    path.with_extension("wav")
                };
                let mut issues = sample.warnings.clone();
                let collision = target != path && target.exists();
                if collision {
                    issues.push(
                        "A WAV with this name already exists. Rename this sample first.".into(),
                    );
                }
                let readonly = fs::metadata(path).is_ok_and(|m| m.permissions().readonly());
                if readonly {
                    issues
                        .push("The file is read-only. Make it writable before converting.".into());
                }
                let info = conversion::inspect(path);
                if info.as_ref().is_ok_and(|info| info.channels > 2) {
                    issues.push("More than two channels will be mixed to mono.".into());
                }
                let convertible = !readonly
                    && sample.needs_conversion
                    && sample.voice > 0
                    && !collision
                    && info.is_ok();
                Some(Candidate {
                    id: sample.id.clone(),
                    fingerprint: state
                        .fingerprints
                        .get(&sample.id)
                        .cloned()
                        .unwrap_or_default(),
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    kit: kit.to_owned(),
                    issues,
                    convertible,
                    target: target
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                })
            })
            .collect()
    }
    pub fn dismiss(&mut self, root: &str, files: &[ReviewedFile]) {
        if let Some(state) = self.libraries.get_mut(root) {
            for file in files {
                if state.fingerprints.get(&file.id) == Some(&file.fingerprint) {
                    state.pending.remove(&file.id);
                }
            }
        }
    }
    pub fn request_all(&mut self, root: &str, catalog: &Catalog) {
        if let Some(state) = self.libraries.get_mut(root) {
            state.pending.extend(
                catalog
                    .kits
                    .iter()
                    .flat_map(|k| &k.samples)
                    .chain(&catalog.unassigned)
                    .filter(|s| !s.warnings.is_empty())
                    .map(|s| s.id.clone()),
            );
        }
    }
}
