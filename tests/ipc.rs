use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use pigma::ipc::{self, IpcEvent, QueueSnapshot, StatusSnapshot};
use tokio::io::AsyncBufReadExt;

fn channel() -> (
    tokio::sync::broadcast::Sender<StatusSnapshot>,
    tokio::sync::broadcast::Receiver<StatusSnapshot>,
) {
    tokio::sync::broadcast::channel(16)
}

/// A `SearchEngine` with no providers: the round trips exercise the IPC
/// plumbing without hitting any network.
fn stub_searcher() -> Arc<pigma::app::SearchEngine> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cache_dir =
        std::env::temp_dir().join(format!("pigma-ipc-test-cache-{}", std::process::id()));
    let cache = Arc::new(pigma::cache::CacheManager::new(
        cache_dir.clone(),
        cache_dir,
        String::new(),
    ));
    let api = Arc::new(ncm_api::NcmClient::builder().build().expect("client"));
    let service = pigma::service::ApiService::new(api, cache);
    let finder = Arc::new(
        sonar::SonarFinder::new(sonar::SearchConfig::new().with_providers(vec![])).expect("finder"),
    );
    let sonar_songs: Arc<Mutex<HashMap<u64, Arc<sonar::Song>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let search_results: pigma::app::SearchResults = Arc::new(Mutex::new(HashMap::new()));
    Arc::new(pigma::app::SearchEngine::new(
        service,
        finder,
        sonar_songs,
        search_results,
        20,
        vec![],
    ))
}

fn tmp_socket(tag: &str) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        let dir = std::env::temp_dir().join(format!("pigma-ipc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{tag}.sock"))
    }
    #[cfg(windows)]
    {
        // Named pipes live in a global namespace, so make the name unique per
        // test (parallel-safe) and per process.
        std::path::PathBuf::from(format!(r"\\.\pipe\pigma-test-{}-{tag}", std::process::id()))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn status_round_trip() {
    ipc::set_socket_path_override(Some(tmp_socket("status")));

    let snapshot = StatusSnapshot {
        name: "Test Song".into(),
        artist: "Test Artist".into(),
        duration_ms: 100_000,
        position_ms: 25_000,
        volume: 0.5,
        playing: true,
        liked: true,
        ..Default::default()
    };
    let snapshot = Arc::new(Mutex::new(snapshot));

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let (status_tx, _status_rx) = channel();
    let _guard = ipc::start_server(
        Arc::clone(&snapshot),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        status_tx,
        tx,
        stub_searcher(),
    );

    // Give the server a moment to bind.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let got = ipc::fetch_status().await.expect("fetch status");
    assert_eq!(got.name, "Test Song");
    assert_eq!(got.artist, "Test Artist");
    assert_eq!(got.duration_ms, 100_000);
    assert_eq!(got.position_ms, 25_000);
    assert_eq!(got.volume, 0.5);
    assert!(got.playing);
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_round_trip() {
    ipc::set_socket_path_override(Some(tmp_socket("queue")));

    let queue = QueueSnapshot {
        current_index: Some(1),
        songs: vec![
            pigma::ipc::QueueEntry {
                id: 1,
                name: "Song A".into(),
                singer: "Artist A".into(),
                album: "Album A".into(),
                duration_ms: 1000,
            },
            pigma::ipc::QueueEntry {
                id: 2,
                name: "Song B".into(),
                singer: "Artist B".into(),
                album: "Album B".into(),
                duration_ms: 2000,
            },
        ],
    };
    let _guard = ipc::start_server(
        Arc::new(Mutex::new(StatusSnapshot::default())),
        Arc::new(Mutex::new(queue)),
        channel().0,
        tokio::sync::mpsc::unbounded_channel().0,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let got = ipc::fetch_queue().await.expect("fetch queue");
    assert_eq!(got.current_index, Some(1));
    assert_eq!(got.songs.len(), 2);
    assert_eq!(got.songs[1].name, "Song B");
    assert_eq!(got.songs[1].duration_ms, 2000);
}

#[tokio::test(flavor = "multi_thread")]
async fn msg_round_trip_forwards_event() {
    ipc::set_socket_path_override(Some(tmp_socket("msg")));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _guard = ipc::start_server(
        Arc::new(Mutex::new(StatusSnapshot::default())),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        channel().0,
        tx,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    ipc::send_msg(ipc::MsgAction::Pause)
        .await
        .expect("send msg");

    let event = rx.recv().await.expect("receive event");
    match event {
        pigma::event::Event::App(pigma::event::AppEvent::Ipc(IpcEvent::Pause)) => {}
        other => panic!("expected Ipc(Pause), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn msg_switch_list_round_trip() {
    ipc::set_socket_path_override(Some(tmp_socket("switch_list")));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _guard = ipc::start_server(
        Arc::new(Mutex::new(StatusSnapshot::default())),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        channel().0,
        tx,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    ipc::send_msg(ipc::MsgAction::SwitchList {
        endpoint: "recommend_songs".into(),
        playlist: Some(2),
    })
    .await
    .expect("send msg");

    let event = rx.recv().await.expect("receive event");
    match event {
        pigma::event::Event::App(pigma::event::AppEvent::Ipc(IpcEvent::SwitchList {
            endpoint,
            playlist,
        })) => {
            assert_eq!(endpoint, "recommend_songs");
            assert_eq!(playlist, Some(2));
        }
        other => panic!("expected Ipc(SwitchList), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn msg_play_song_id_forwards_event() {
    ipc::set_socket_path_override(Some(tmp_socket("play_id")));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _guard = ipc::start_server(
        Arc::new(Mutex::new(StatusSnapshot::default())),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        channel().0,
        tx,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    ipc::send_msg(ipc::MsgAction::Play {
        song_id: Some(187186),
    })
    .await
    .expect("send msg");

    let event = rx.recv().await.expect("receive event");
    match event {
        pigma::event::Event::App(pigma::event::AppEvent::Ipc(IpcEvent::Play { song_id })) => {
            assert_eq!(song_id, Some(187186));
        }
        other => panic!("expected Ipc(Play{{song_id}}), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn msg_toggle_play_forwards_event() {
    ipc::set_socket_path_override(Some(tmp_socket("toggle_play")));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let _guard = ipc::start_server(
        Arc::new(Mutex::new(StatusSnapshot::default())),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        channel().0,
        tx,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    ipc::send_msg(ipc::MsgAction::TogglePlay)
        .await
        .expect("send msg");

    let event = rx.recv().await.expect("receive event");
    match event {
        pigma::event::Event::App(pigma::event::AppEvent::Ipc(IpcEvent::TogglePlay)) => {}
        other => panic!("expected Ipc(TogglePlay), got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn subscribe_streams_updates() {
    ipc::set_socket_path_override(Some(tmp_socket("subscribe")));

    let snapshot = Arc::new(Mutex::new(StatusSnapshot {
        name: "Song A".into(),
        playing: true,
        ..Default::default()
    }));
    let (status_tx, _status_rx) = channel();
    let _guard = ipc::start_server(
        Arc::clone(&snapshot),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        status_tx.clone(),
        tokio::sync::mpsc::unbounded_channel().0,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut reader = ipc::subscribe_status().await.expect("subscribe");
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .expect("read initial snapshot");
    let initial: StatusSnapshot = serde_json::from_str(&line).unwrap();
    assert_eq!(initial.name, "Song A");

    // Mutate the shared snapshot and broadcast; the subscriber must receive it.
    *snapshot.lock().unwrap() = StatusSnapshot {
        name: "Song B".into(),
        artist: "Artist B".into(),
        playing: true,
        paused: true,
        liked: true,
        ..Default::default()
    };
    status_tx.send(snapshot.lock().unwrap().clone()).unwrap();

    line.clear();
    reader
        .read_line(&mut line)
        .await
        .expect("read streamed update");
    let update: StatusSnapshot = serde_json::from_str(&line).unwrap();
    assert_eq!(update.name, "Song B");
    assert_eq!(update.artist, "Artist B");
    assert!(update.paused);
}

#[tokio::test(flavor = "multi_thread")]
async fn search_round_trip() {
    ipc::set_socket_path_override(Some(tmp_socket("search")));

    let _guard = ipc::start_server(
        Arc::new(Mutex::new(StatusSnapshot::default())),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        channel().0,
        tokio::sync::mpsc::unbounded_channel().0,
        stub_searcher(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // The stub searcher has no providers, so it returns an empty list without
    // touching the network — this still exercises the request/reply plumbing.
    let results = ipc::search_songs("test").await.expect("search round trip");
    assert!(results.is_empty());
}
