use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct SavedState {
    pub lid_close_ac: u32,
    pub lid_close_dc: u32,
    pub sleep_idle_ac: u32,
    pub sleep_idle_dc: u32,
}

fn state_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| {
            let profile = std::env::var("USERPROFILE").unwrap_or_default();
            format!("{profile}\\AppData\\Local")
        });
    PathBuf::from(local).join("Clam")
}

fn state_file_path() -> PathBuf {
    state_dir().join("saved_state.json")
}

pub fn save_state(state: &SavedState) -> Result<(), String> {
    let dir = state_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("serialize: {e}"))?;
    fs::write(state_file_path(), json).map_err(|e| format!("write: {e}"))
}

pub fn load_state() -> Option<SavedState> {
    let data = fs::read_to_string(state_file_path()).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn clear_state() {
    let _ = fs::remove_file(state_file_path());
}

pub fn state_file_exists() -> bool {
    state_file_path().exists()
}
