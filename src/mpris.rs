//! MPRIS2 integration for desktop media controls.

use std::sync::{Arc, Mutex};

use mpris_server::{LoopStatus, Metadata, PlaybackStatus, Player, Time, TrackId};
use tokio::sync::{broadcast, mpsc};

use crate::{
    event::{AppEvent, Event},
    ipc::{IpcEvent, StatusSnapshot},
};

/// Keep the MPRIS worker alive for as long as the application is running.
pub struct MprisGuard {
    _thread: std::thread::JoinHandle<()>,
}

/// Start the MPRIS service on a local Tokio runtime.
///
/// `mpris-server::Player` deliberately uses local state, so it is kept on its
/// own current-thread runtime. DBus callbacks only enqueue normal app events;
/// all playback mutation remains on the app event loop.
pub fn start(
    status: Arc<Mutex<StatusSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
) -> MprisGuard {
    let thread = std::thread::Builder::new()
        .name("pigma-mpris".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    log::warn!("mpris: failed to create runtime: {error}");
                    return;
                }
            };
            runtime.block_on(run(status, status_tx, event_tx));
        })
        .expect("failed to start MPRIS thread");

    MprisGuard { _thread: thread }
}

async fn run(
    status: Arc<Mutex<StatusSnapshot>>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    event_tx: mpsc::UnboundedSender<Event>,
) {
    let player = match Player::builder("pigma")
        .identity("pigma")
        .desktop_entry("pigma")
        .can_go_next(true)
        .can_go_previous(true)
        .can_play(true)
        .can_pause(true)
        .can_seek(true)
        .can_control(true)
        .build()
        .await
    {
        Ok(player) => player,
        Err(error) => {
            log::warn!("mpris: failed to claim DBus name: {error}");
            return;
        }
    };

    connect_callbacks(&player, &event_tx);
    let mut updates = status_tx.subscribe();
    let initial = status
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_default();
    apply_snapshot(&player, &initial).await;

    let server_task = player.run();
    tokio::pin!(server_task);
    loop {
        tokio::select! {
            _ = &mut server_task => break,
            update = updates.recv() => match update {
                Ok(snapshot) => apply_snapshot(&player, &snapshot).await,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if let Ok(snapshot) = status.lock().map(|snapshot| snapshot.clone()) {
                        apply_snapshot(&player, &snapshot).await;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

fn connect_callbacks(player: &Player, event_tx: &mpsc::UnboundedSender<Event>) {
    let tx = event_tx.clone();
    player.connect_next(move |_| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::Next)));
    });
    let tx = event_tx.clone();
    player.connect_previous(move |_| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::Previous)));
    });
    let tx = event_tx.clone();
    player.connect_pause(move |_| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::Pause)));
    });
    let tx = event_tx.clone();
    player.connect_play(move |_| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::Play { song_id: None })));
    });
    let tx = event_tx.clone();
    player.connect_play_pause(move |_| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::TogglePlay)));
    });
    let tx = event_tx.clone();
    player.connect_stop(move |_| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::Stop)));
    });
    let tx = event_tx.clone();
    player.connect_seek(move |_, offset| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::SeekRelative {
            seconds: offset.as_micros() as f64 / 1_000_000.0,
        })));
    });
    let tx = event_tx.clone();
    player.connect_set_position(move |_, _, position| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::SeekAbsolute {
            seconds: position.as_micros().max(0) as f64 / 1_000_000.0,
        })));
    });
    let tx = event_tx.clone();
    player.connect_set_volume(move |_, volume| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::Volume {
            delta: None,
            absolute: Some(volume.clamp(0.0, 1.0)),
        })));
    });
    let tx = event_tx.clone();
    player.connect_set_loop_status(move |_, loop_status| {
        let status = match loop_status {
            LoopStatus::None => "none",
            LoopStatus::Track => "track",
            LoopStatus::Playlist => "playlist",
        };
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::SetLoopStatus {
            status: status.into(),
        })));
    });
    let tx = event_tx.clone();
    player.connect_set_shuffle(move |_, shuffle| {
        let _ = tx.send(Event::App(AppEvent::Ipc(IpcEvent::SetShuffle { shuffle })));
    });
}

async fn apply_snapshot(player: &Player, snapshot: &StatusSnapshot) {
    let playback_status = if snapshot.playing && !snapshot.paused {
        PlaybackStatus::Playing
    } else if snapshot.paused {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Stopped
    };
    let _ = player.set_playback_status(playback_status).await;
    let _ = player.set_volume(snapshot.volume.clamp(0.0, 1.0)).await;
    player.set_position(Time::from_millis(snapshot.position_ms as i64));

    let loop_status = match snapshot.mode.as_str() {
        "repeat_one" => LoopStatus::Track,
        "repeat_all" => LoopStatus::Playlist,
        _ => LoopStatus::None,
    };
    let _ = player.set_loop_status(loop_status).await;
    let _ = player.set_shuffle(snapshot.mode == "shuffle").await;

    let mut metadata = Metadata::builder()
        .trackid(track_id(snapshot.id))
        .title(snapshot.name.clone())
        .artist([snapshot.artist.clone()])
        .album(snapshot.album.clone())
        .length(Time::from_millis(snapshot.duration_ms as i64));
    if !snapshot.art_url.is_empty() {
        metadata = metadata.art_url(snapshot.art_url.clone());
    }
    if !snapshot.lyrics.is_empty() {
        metadata = metadata.lyrics(snapshot.lyrics.clone());
    }
    let _ = player.set_metadata(metadata.build()).await;
}

fn track_id(id: u64) -> TrackId {
    TrackId::try_from(format!("/org/pigma/track/{id}")).unwrap_or(TrackId::NO_TRACK)
}
