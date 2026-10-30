//! Immutable recipes define saved sounds; Originals and their source metadata never change.
use crate::*;
use rample_core::editing::{self, Settings};
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleRevision {
    pub id: String,
    pub settings: Settings,
}
impl managed::ManagedLibrary {
    pub fn sample_edits(&self, id: &str) -> Result<SampleRevision, String> {
        let original = self.prepare(id)?;
        let saved: Option<(String, String)> = self.db.query_row(
            "SELECT r.id,r.settings FROM sample_revisions r JOIN sample_current c ON c.revision_id=r.id WHERE c.sample_id=?1", [id],
            |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
        match saved {
            Some((id, settings)) => Ok(SampleRevision {
                id,
                settings: serde_json::from_str(&settings).map_err(|e| e.to_string())?,
            }),
            None => {
                let revision = SampleRevision {
                    id: format!("original-{id}"),
                    settings: Settings::original(
                        original.spec.frames as f64 / original.spec.rate as f64,
                    ),
                };
                self.db.execute("INSERT OR IGNORE INTO sample_revisions(id,sample_id,settings) VALUES (?1,?2,?3)",params![revision.id,id,serde_json::to_string(&revision.settings).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
                self.db.execute("INSERT OR IGNORE INTO sample_current(sample_id,revision_id) VALUES (?1,?2)",params![id,revision.id]).map_err(|e|e.to_string())?;
                Ok(revision)
            }
        }
    }
    pub fn revisions(&self, id: &str) -> Result<Vec<SampleRevision>, String> {
        self.sample_edits(id)?;
        let mut q = self
            .db
            .prepare("SELECT id,settings FROM sample_revisions WHERE sample_id=?1 ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let revisions = q
            .query_map([id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .map(|r| {
                let (id, settings) = r.map_err(|e| e.to_string())?;
                Ok(SampleRevision {
                    id,
                    settings: serde_json::from_str(&settings).map_err(|e| e.to_string())?,
                })
            })
            .collect();
        revisions
    }
    pub fn prepare_revision(
        &self,
        id: &str,
        revision: &str,
    ) -> Result<Arc<PreparedSample>, String> {
        let saved = self
            .revisions(id)?
            .into_iter()
            .find(|r| r.id == revision)
            .ok_or("Sample Revision does not belong to this Sample.")?;
        if saved.id.starts_with("original-") {
            self.prepare_target(id, true)
        } else {
            self.render_edits(id, &saved.id, &saved.settings)
        }
    }
    pub fn render_edits(
        &self,
        id: &str,
        revision: &str,
        settings: &Settings,
    ) -> Result<Arc<PreparedSample>, String> {
        let source = self.prepare(id)?;
        settings.validate(&source.spec)?;
        let mut unedited = Settings::original(source.spec.frames as f64 / source.spec.rate as f64);
        // With dry effects these controls do not change the sound. Keep the same
        // preparation quality as the initial saved Revision, including after Reset.
        unedited.echo_ms = settings.echo_ms;
        unedited.echo_feedback = settings.echo_feedback;
        unedited.reverb_seconds = settings.reverb_seconds;
        if *settings == unedited {
            return self.prepare_target(id, true);
        }
        let path = self
            .root
            .join("Derived")
            .join(format!("edit-v1-{revision}.wav"));
        if managed::validate_wav(&path).is_err() {
            let mut staged = tempfile::NamedTempFile::new_in(self.root.join("Incoming"))
                .map_err(|e| e.to_string())?;
            editing::render(Path::new(&source.path), settings, staged.as_file_mut())?;
            managed::validate_wav(staged.path())?;
            // prepare rechecks the preserved Original before publication.
            self.prepare(id)?;
            staged.as_file().sync_all().map_err(|e| e.to_string())?;
            staged.persist(&path).map_err(|e| e.to_string())?;
        }
        PreparedSample::load(&path, revision)
    }
    /// Save revision and occurrence retarget together, so a crash cannot publish half a Variant.
    pub fn publish_edits(
        &mut self,
        id: &str,
        expected: &str,
        settings: &Settings,
        variant_name: Option<&str>,
        mut graph: patches::PatchGraph,
        occurrence: &str,
    ) -> Result<String, String> {
        if self.sample_edits(id)?.id != expected {
            return Err("This Sample changed. Reopen the editor.".into());
        }
        let revision = Uuid::new_v4().to_string();
        self.render_edits(id, &revision, settings)?;
        let sample = self
            .list()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or("Sample no longer exists.")?;
        let target = if let Some(name) = variant_name {
            if name.trim().is_empty() {
                return Err("Variant name cannot be empty.".into());
            }
            let target = Uuid::new_v4().to_string();
            let layer = graph
                .patches
                .iter_mut()
                .flat_map(|p| &mut p.kits)
                .flat_map(|k| &mut k.voices)
                .flat_map(|v| &mut v.layers)
                .find(|l| l.id == occurrence && l.sample_id == id)
                .ok_or("Layer changed. Reopen the editor.")?;
            layer.sample_id = target.clone();
            target
        } else {
            id.to_owned()
        };
        let transaction = self.db.transaction().map_err(|e| e.to_string())?;
        if let Some(name) = variant_name {
            transaction
                .execute(
                    "INSERT INTO samples(id,original_id,name,tags) VALUES (?1,?2,?3,?4)",
                    params![
                        target,
                        sample.original_id,
                        name,
                        serde_json::to_string(&sample.tags).map_err(|e| e.to_string())?
                    ],
                )
                .map_err(|e| e.to_string())?;
            transaction.execute("INSERT INTO patch_state(id,payload) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",[serde_json::to_string(&graph).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        }
        transaction
            .execute(
                "INSERT INTO sample_revisions(id,sample_id,settings) VALUES (?1,?2,?3)",
                params![
                    revision,
                    target,
                    serde_json::to_string(settings).map_err(|e| e.to_string())?
                ],
            )
            .map_err(|e| e.to_string())?;
        transaction.execute("INSERT INTO sample_current(sample_id,revision_id) VALUES (?1,?2) ON CONFLICT(sample_id) DO UPDATE SET revision_id=excluded.revision_id",params![target,revision]).map_err(|e|e.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(target)
    }
}
impl AppState {
    pub fn prepare_sample_revision(
        &self,
        id: String,
        revision: String,
    ) -> Result<Arc<PreparedSample>, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .prepare_revision(&id, &revision)
    }
    pub fn sample_revisions(&self, id: String) -> Result<Vec<SampleRevision>, String> {
        self.managed
            .lock()
            .map_err(|_| "Library is unavailable")?
            .revisions(&id)
    }
}

#[tauri::command]
pub async fn sample_revisions(
    id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<SampleRevision>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.sample_revisions(id))
        .await
        .map_err(|e| e.to_string())?
}
