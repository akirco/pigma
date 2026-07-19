use std::fs;
use std::io::BufReader;

use ncm_api::SongInfo;
use rodio::Source;

pub fn scan_local_music(dir: &std::path::Path) -> Vec<SongInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let extensions = ["mp3", "flac", "wav", "ogg", "aac", "m4a", "wma"];
    let mut songs = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if !extensions.contains(&ext.as_str()) {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let duration = fs::File::open(&path)
            .ok()
            .and_then(|f| rodio::Decoder::new(BufReader::new(f)).ok())
            .and_then(|d| d.total_duration())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        songs.push(SongInfo {
            id: 0,
            name,
            singer: "本地".into(),
            artist_id: 0,
            album: path.to_string_lossy().to_string(),
            album_id: 0,
            pic_url: String::new(),
            duration,
            copyright: ncm_api::SongCopyright::Free,
        });
    }
    songs.sort_by(|a, b| a.name.cmp(&b.name));
    songs
}
