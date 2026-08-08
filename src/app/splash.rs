use crate::state::LogLevel;

use super::App;
use crate::event::{Event, SplashEvent};
use crate::state::SplashLogEntry;
use crate::utils::clock_time;

pub(crate) fn send_event(tx: &tokio::sync::mpsc::UnboundedSender<Event>, event: Event) {
    if tx.send(event).is_err() {
        log::error!("Failed to send event: receiver dropped");
    }
}

pub(super) fn splash_status(progress: f64) -> &'static str {
    if progress < 0.3 {
        "INITIALIZING SYSTEM..."
    } else if progress < 0.6 {
        "CONNECTING TO SERVER..."
    } else if progress < 0.9 {
        "LOADING LIBRARY..."
    } else {
        "READY"
    }
}

pub(super) async fn check_network(_api: &ncm_api::NcmClient) -> bool {
    use tokio::net::TcpStream;
    TcpStream::connect("music.163.com:443").await.is_ok()
}

impl App {
    pub(super) fn start_splash_boot(&self) {
        let sender = self.state.events.sender();
        let client = self.service.client().clone();

        tokio::spawn(async move {
            let send = |progress: f64, text: &str, level: LogLevel| {
                send_event(
                    &sender,
                    SplashEvent::Tick {
                        progress,
                        log: Some(SplashLogEntry {
                            time: clock_time(),
                            text: text.to_string(),
                            level,
                        }),
                    }
                    .into(),
                );
            };

            send(0.05, "Initializing engine...", LogLevel::Success);

            send(0.10, "Checking network connectivity...", LogLevel::Info);
            let online = check_network(&client).await;

            if online {
                send(
                    0.25,
                    "Network: connected to music.163.com",
                    LogLevel::Success,
                );

                send(0.35, "Loading user session...", LogLevel::Info);
                if client.is_logged_in() {
                    send(0.45, "Session: cookies found", LogLevel::Success);
                } else {
                    send(0.45, "Session: not logged in", LogLevel::Info);
                }
            } else {
                send_event(&sender, SplashEvent::SetOffline.into());
                send(0.25, "Network: offline, offline mode", LogLevel::Warning);
            }

            send(0.98, "Ready.", LogLevel::Success);
            send_event(
                &sender,
                SplashEvent::Tick {
                    progress: 1.0,
                    log: None,
                }
                .into(),
            );
        });
    }

    pub(super) fn handle_splash_tick(&mut self, progress: f64, log: Option<SplashLogEntry>) {
        self.state.splash.progress = progress;
        if let Some(entry) = log {
            self.state.splash.logs.push(entry);
        }
        if progress >= 1.0 {
            self.state.splash.status = "READY".to_string();
            self.state.splash.boot_complete = true;
        } else {
            let new_status = splash_status(progress);
            if self.state.splash.status != new_status {
                self.state.splash.status = new_status.to_string();
            }
        }
    }
}
