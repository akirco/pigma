//! Local music library scan.
//!
//! The scan deliberately does not follow symlinks: a symlinked directory can
//! form a cycle (`~/Music/loop -> .`) and recurse forever. Decoded durations are
//! cached by `(path, mtime, size)` so a rescan only probes new/changed files
//! instead of reopening every track in the library.

use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    io::BufReader,
    path::Path,
    time::UNIX_EPOCH,
};

use ncm_api::SongInfo;
use rodio::Source;
use serde::{Deserialize, Serialize};

const EXTENSIONS: &[&str] = &["mp3", "flac", "wav", "ogg", "aac", "m4a", "wma"];
/// Guard against pathological trees even without symlinks.
const MAX_DEPTH: usize = 32;
const CACHE_FILE: &str = "local_music_index.json";

#[derive(Default, Serialize, Deserialize)]
struct LocalScanCache {
    #[serde(default)]
    entries: HashMap<String, CachedTrack>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct CachedTrack {
    /// File mtime in nanoseconds since the Unix epoch.
    mtime_ns: u64,
    size: u64,
    /// Decoded duration in milliseconds.
    duration: u64,
}

/// Scan `dir` recursively for playable audio files.
pub(super) fn scan_local_music(dir: &Path) -> Vec<SongInfo> {
    let cache_path = crate::persist::pigma_cache_dir().join(CACHE_FILE);
    scan_local_music_cached(dir, &cache_path)
}

/// Scan with an explicit metadata-cache path. Split out so tests stay hermetic
/// and never touch the user's real cache.
fn scan_local_music_cached(dir: &Path, cache_path: &Path) -> Vec<SongInfo> {
    let previous = load_cache(cache_path);
    let mut entries = HashMap::new();
    let mut songs = Vec::new();
    walk(dir, 0, &previous.entries, &mut entries, &mut songs);

    let cache = LocalScanCache { entries };
    let changed = cache.entries.len() != previous.entries.len()
        || cache
            .entries
            .iter()
            .any(|(path, track)| previous.entries.get(path) != Some(track));
    if changed
        && let Ok(json) = serde_json::to_string(&cache)
        && let Err(e) = crate::persist::write_atomic(cache_path, json.as_bytes())
    {
        log::warn!("Failed to save local music index: {e}");
    }

    songs.sort_by(|a, b| a.name.cmp(&b.name));
    songs
}

fn walk(
    dir: &Path,
    depth: usize,
    cache: &HashMap<String, CachedTrack>,
    entries: &mut HashMap<String, CachedTrack>,
    songs: &mut Vec<SongInfo>,
) {
    if depth > MAX_DEPTH {
        log::debug!("local scan: max depth reached at {}", dir.display());
        return;
    }
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for dir_entry in read_dir.flatten() {
        // Never follow symlinks: a directory link can point back to an
        // ancestor and turn the recursion into a loop.
        let Ok(file_type) = dir_entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        let path = dir_entry.path();
        if file_type.is_dir() {
            walk(&path, depth + 1, cache, entries, songs);
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if !EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let Ok(meta) = dir_entry.metadata() else {
            continue;
        };
        let (mtime_ns, size) = file_stamp(&meta);
        let key = path.to_string_lossy().into_owned();
        let duration = cache
            .get(&key)
            .filter(|cached| cached.mtime_ns == mtime_ns && cached.size == size)
            .map(|cached| cached.duration)
            .unwrap_or_else(|| decode_duration(&path));
        entries.insert(
            key,
            CachedTrack {
                mtime_ns,
                size,
                duration,
            },
        );

        let name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.to_string_lossy().hash(&mut hasher);
        // Clear the top bit so the id can never collide with the sonar song-id
        // flag (1<<63); otherwise ~half of local files get misrouted to the
        // network resolver instead of `resolve_local`.
        let id = hasher.finish() & !(1u64 << 63);
        songs.push(SongInfo {
            id,
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
}

fn file_stamp(meta: &fs::Metadata) -> (u64, u64) {
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|age| age.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    (mtime_ns, meta.len())
}

fn decode_duration(path: &Path) -> u64 {
    fs::File::open(path)
        .ok()
        .and_then(|file| rodio::Decoder::new(BufReader::new(file)).ok())
        .and_then(|decoder| decoder.total_duration())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn load_cache(path: &Path) -> LocalScanCache {
    fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs, path::PathBuf};

    use super::{CachedTrack, LocalScanCache, file_stamp, scan_local_music_cached};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pigma_scan_{tag}_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// An unchanged file must reuse the cached duration instead of decoding it
    /// again (the fixture is not even valid audio, so a decode would yield 0).
    #[test]
    fn reuses_cached_duration_for_unchanged_files() {
        let dir = temp_dir("cache_hit");
        let audio = dir.join("song.mp3");
        fs::write(&audio, b"not audio").expect("write fixture");

        let meta = fs::metadata(&audio).expect("metadata");
        let (mtime_ns, size) = file_stamp(&meta);
        let cache_path = dir.join("index.json");
        let cache = LocalScanCache {
            entries: HashMap::from([(
                audio.to_string_lossy().into_owned(),
                CachedTrack {
                    mtime_ns,
                    size,
                    duration: 12_345,
                },
            )]),
        };
        fs::write(
            &cache_path,
            serde_json::to_string(&cache).expect("serialize cache"),
        )
        .expect("write cache");

        let songs = scan_local_music_cached(&dir, &cache_path);

        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].name, "song");
        assert_eq!(songs[0].duration, 12_345);

        let _ = fs::remove_dir_all(dir);
    }

    /// A symlinked directory pointing back to an ancestor must not recurse.
    #[cfg(unix)]
    #[test]
    fn symlinked_directory_does_not_loop() {
        let dir = temp_dir("symlink");
        fs::write(dir.join("song.mp3"), b"not audio").expect("write fixture");
        std::os::unix::fs::symlink(&dir, dir.join("loop")).expect("create symlink");

        let songs = scan_local_music_cached(&dir, &dir.join("index.json"));

        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].name, "song");

        let _ = fs::remove_dir_all(dir);
    }
}
