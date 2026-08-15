use std::sync::{Arc, Mutex};

use pigma::ipc::{self, IpcEvent, QueueSnapshot, StatusSnapshot};

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
        ..Default::default()
    };
    let snapshot = Arc::new(Mutex::new(snapshot));

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let _guard = ipc::start_server(
        Arc::clone(&snapshot),
        Arc::new(Mutex::new(QueueSnapshot::default())),
        tx,
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
        tokio::sync::mpsc::unbounded_channel().0,
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
        tx,
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
        tx,
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
