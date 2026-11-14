//! Playlist queue persistence: snapshot model, queue-file naming and the
//! ordered atomic writer.

mod model;
mod queues;
mod store;
mod write;

pub(super) use store::PlaylistStorage;

#[cfg(test)]
mod tests {
    //! Queue-storage unit tests: legacy history formats, atomic save round-trip and
    //! save-order sequencing.

    use std::{fs, sync::Arc};

    use ncm_api::SongInfo;

    use super::{super::PlayMode, model::SavedQueue, store::PlaylistStorage};

    #[test]
    fn deserialize_history_accepts_new_format() {
        let json = r#"{
            "queue": [],
            "history": [1, 2, 3],
            "current_index": null,
            "mode": "Sequential",
            "volume": 50.0
        }"#;
        let saved: SavedQueue = serde_json::from_str(json).unwrap();
        assert_eq!(saved.history, vec![1, 2, 3]);
        // Legacy files carry no `speed`/`pitch_locked`; both defaults must keep
        // the previous behaviour (normal speed, pitch-keeping stretch).
        assert_eq!(saved.speed, 1.0);
        assert!(saved.pitch_locked);
    }

    #[test]
    fn deserialize_history_accepts_legacy_format() {
        let json = r#"{
            "queue": [],
            "history": [{"id": 11, "name": "a"}, {"id": 22, "name": "b"}],
            "current_index": null,
            "mode": "Sequential",
            "volume": 50.0
        }"#;
        let saved: SavedQueue = serde_json::from_str(json).unwrap();
        assert_eq!(saved.history, vec![11, 22]);
    }

    #[test]
    fn deserialize_history_drops_malformed_entries() {
        let json = r#"{
                "queue": [],
                "history": [1, {"id": 2}, {"no_id": true}, "bad", null, 1.5, {"id": "3"}],
                "current_index": null,
                "mode": "Sequential",
                "volume": 50.0
            }"#;
        let saved: SavedQueue = serde_json::from_str(json).unwrap();
        assert_eq!(saved.history, vec![1, 2]);
    }

    /// The save path must round-trip through the atomic writer (temp file +
    /// rename) without leaving temp files behind.
    #[test]
    fn save_queue_sync_round_trips_without_temp_files() {
        let dir = std::env::temp_dir().join(format!(
            "pigma_storage_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let storage = PlaylistStorage::new(dir.clone());
        let song = Arc::new(SongInfo {
            id: 42,
            name: "song".into(),
            singer: "artist".into(),
            artist_id: 0,
            album: String::new(),
            album_id: 0,
            pic_url: String::new(),
            duration: 1_000,
            copyright: ncm_api::SongCopyright::Unknown,
        });

        storage.save_queue_sync(
            "q_test",
            "测试",
            &[song],
            &[],
            Some(0),
            &PlayMode::Sequential,
            0.5,
            1.2,
            true,
            0.25,
        );

        let saved = storage
            .load_queue_by_id("q_test")
            .expect("queue should load after an atomic save");
        assert_eq!(saved.queue.len(), 1);
        assert_eq!(saved.queue[0].id, 42);
        assert_eq!(saved.volume, 0.5);
        assert_eq!(saved.speed, 1.2);
        assert_eq!(storage.load_active_id().as_deref(), Some("q_test"));

        let temp_files: Vec<_> = fs::read_dir(&storage.base_dir)
            .expect("read playlists dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            temp_files.is_empty(),
            "temp files left behind: {temp_files:?}"
        );

        let _ = fs::remove_dir_all(dir);
    }

    /// Detached save tasks may finish out of order; only the newest sequence
    /// may reach disk, otherwise a restart restores an older queue/playhead.
    #[test]
    fn save_gate_keeps_the_newest_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "pigma_storage_gate_test_{}_{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let storage = PlaylistStorage::new(dir.clone());
        let queue_path = storage.queue_path("q_test", "测试");
        let active_path = storage.active_path();

        // Simulate the newer task acquiring the lock first...
        storage
            .save_gate
            .commit(2, &queue_path, &active_path, "q_test", r#"{"new":true}"#);
        // ...then an older task reaching it late must be skipped.
        storage
            .save_gate
            .commit(1, &queue_path, &active_path, "q_test", r#"{"old":true}"#);

        assert_eq!(
            fs::read_to_string(&queue_path).expect("read queue"),
            r#"{"new":true}"#
        );
        assert_eq!(
            fs::read_to_string(&active_path).expect("read active marker"),
            "q_test"
        );

        let _ = fs::remove_dir_all(dir);
    }
}
