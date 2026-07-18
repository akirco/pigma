use std::fs;
use std::path::PathBuf;

use ncm_api::SongInfo;
use serde::{Deserialize, Serialize};

use super::types::PlayMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQueue {
    pub queue: Vec<SongInfo>,
    pub history: Vec<SongInfo>,
    pub current_index: Option<usize>,
    pub mode: PlayMode,
    pub volume: f64,
    #[serde(default)]
    pub progress: f64,
}

#[derive(Serialize)]
struct SavedQueueRef<'a> {
    queue: &'a [SongInfo],
    history: &'a [SongInfo],
    current_index: Option<usize>,
    mode: &'a PlayMode,
    volume: f64,
    progress: f64,
}

pub struct PlaylistStorage {
    base_dir: PathBuf,
}

impl PlaylistStorage {
    pub fn new() -> Self {
        let base_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pigma")
            .join("playlists");
        let _ = fs::create_dir_all(&base_dir);
        Self { base_dir }
    }

    pub fn auto_save_path(&self) -> PathBuf {
        self.base_dir.join("play_queue.json")
    }

    pub fn list_playlists(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".json") && name != "play_queue.json" {
                    names.push(name.trim_end_matches(".json").to_string());
                }
            }
        }
        names.sort();
        names
    }

    pub fn save_auto(
        &self,
        queue: &[SongInfo],
        history: &[SongInfo],
        current_index: Option<usize>,
        mode: &PlayMode,
        volume: f64,
        progress: f64,
    ) {
        let saved = SavedQueueRef {
            queue,
            history,
            current_index,
            mode,
            volume,
            progress,
        };
        if let Ok(json) = serde_json::to_string_pretty(&saved) {
            let _ = fs::write(self.auto_save_path(), json);
        }
    }

    pub fn load_auto(&self) -> Option<SavedQueue> {
        let path = self.auto_save_path();
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save_playlist(&self, name: &str, songs: &[SongInfo]) -> bool {
        let path = self.base_dir.join(format!("{}.json", name));
        if let Ok(json) = serde_json::to_string_pretty(songs) {
            fs::write(path, json).is_ok()
        } else {
            false
        }
    }

    pub fn load_playlist(&self, name: &str) -> Option<Vec<SongInfo>> {
        let path = self.base_dir.join(format!("{}.json", name));
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn delete_playlist(&self, name: &str) -> bool {
        let path = self.base_dir.join(format!("{}.json", name));
        fs::remove_file(path).is_ok()
    }
}
