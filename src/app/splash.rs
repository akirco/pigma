use crate::state::LogLevel;

use super::App;
use crate::event::{Event, SplashEvent};
use crate::state::SplashLogEntry;
use crate::utils::clock_time;

pub(super) fn send_event(tx: &tokio::sync::mpsc::UnboundedSender<Event>, event: Event) {
    if tx.send(event).is_err() {
        log::error!("Failed to send event: receiver dropped");
    }
}

fn splash_status(progress: f64) -> &'static str {
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

async fn check_network(_api: &ncm_api::NcmClient) -> bool {
    use tokio::net::TcpStream;
    TcpStream::connect("music.163.com:443").await.is_ok()
}

impl App {
    pub(super) fn start_splash_boot(&self) {
        let sender = self.state.events.sender();
        let client = self.service.client().clone();
        let duration = std::time::Duration::from_secs_f64(self.config.splash_duration_secs);

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
            // Pace the boot sequence across `splash_duration_secs` so the progress
            // bar animates; real async work may naturally take longer.
            let start = std::time::Instant::now();
            let wait_to = |frac: f64| {
                let target = start + duration.mul_f64(frac);
                let rem = target.saturating_duration_since(std::time::Instant::now());
                tokio::time::sleep(rem)
            };

            send(0.05, "Initializing engine...", LogLevel::Success);

            wait_to(0.12).await;
            send(0.12, "Checking network connectivity...", LogLevel::Info);
            let online = check_network(&client).await;
            wait_to(0.30).await;

            if online {
                send(
                    0.30,
                    "Network: connected to music.163.com",
                    LogLevel::Success,
                );
                wait_to(0.38).await;

                send(0.38, "Loading user session...", LogLevel::Info);
                if client.is_logged_in() {
                    send(0.48, "Session: cookies found", LogLevel::Success);
                } else {
                    send(0.48, "Session: not logged in", LogLevel::Info);
                }
            } else {
                send_event(&sender, SplashEvent::SetOffline.into());
                send(0.30, "Network: offline, offline mode", LogLevel::Warning);
            }

            wait_to(0.60).await;
            send(0.98, "Ready.", LogLevel::Success);
            wait_to(1.0).await;
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
