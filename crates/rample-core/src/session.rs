use crate::model::Session;
use std::{fs, io::Write, path::Path};

pub fn load(support: &Path) -> Result<Session, String> {
    let path = support.join("session.json");
    if !path.exists() {
        return Ok(Session::default());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        format!("Could not read session.json: {e}. The original file has been preserved.")
    })?;
    let mut session: Session = serde_json::from_value(value.clone()).map_err(|e| {
        format!("Could not migrate the saved session: {e}. The original file has been preserved.")
    })?;
    if value.get("schemaVersion").is_none() {
        let backup = support.join("session-swift-1.1.0.backup.json");
        if !backup.exists() {
            fs::write(&backup, &bytes)
                .map_err(|e| format!("Could not back up the Swift session: {e}"))?;
        }
    }
    session.normalize();
    Ok(session)
}
pub fn save(support: &Path, session: &Session) -> Result<(), String> {
    fs::create_dir_all(support).map_err(|e| e.to_string())?;
    let mut normalized = session.clone();
    normalized.normalize();
    normalized.remember();
    let bytes = serde_json::to_vec_pretty(&normalized).map_err(|e| e.to_string())?;
    let tmp = support.join(format!(".session-{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(&bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        fs::rename(&tmp, support.join("session.json")).map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(tmp);
    result
}
