use std::time::Duration;

use super::{App, event::send_event};
use crate::{
    event::SplashEvent,
    state::{LogLevel, SplashLogEntry},
    time::clock_time,
};

/// Deadline for the startup reachability probe. Kept short on purpose: the
/// splash screen must not stall on a dead network.
const NETWORK_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

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

async fn check_network(api: &ncm_api::NcmClient) -> bool {
    // Probe through the configured client so the user's proxy is honoured, and
    // bound the wait: a raw `TcpStream::connect` ignored both the proxy and any
    // timeout, so proxied users were wrongly sent to offline/local mode and a
    // dead network could stall startup for the OS TCP timeout (minutes).
    tokio::time::timeout(NETWORK_CHECK_TIMEOUT, api.search_hot())
        .await
        .is_ok_and(|result| result.is_ok())
}

fn splash_duration_from_config(secs: f64) -> Duration {
    if secs.is_finite() && secs >= 0.0 {
        Duration::try_from_secs_f64(secs).unwrap_or(Duration::from_secs(2))
    } else {
        Duration::from_secs(2)
    }
}

impl App {
    /// Minimum splash duration, sanitized against negative/NaN/infinite config
    /// values (which would panic in `Duration::from_secs_f64`).
    pub(super) fn splash_duration(&self) -> Duration {
        splash_duration_from_config(self.config.splash_duration_secs)
    }

    pub(super) fn start_splash_boot(&self) {
        let sender = self.events.sender();
        let client = self.service.client().clone();
        let duration = self.splash_duration();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_splash_durations_fall_back_instead_of_panicking() {
        assert_eq!(splash_duration_from_config(-1.0), Duration::from_secs(2));
        assert_eq!(
            splash_duration_from_config(f64::NAN),
            Duration::from_secs(2)
        );
        assert_eq!(
            splash_duration_from_config(f64::INFINITY),
            Duration::from_secs(2)
        );
        assert_eq!(splash_duration_from_config(1e300), Duration::from_secs(2));
        assert_eq!(splash_duration_from_config(0.0), Duration::ZERO);
        assert_eq!(
            splash_duration_from_config(1.5),
            Duration::from_millis(1500)
        );
    }
}
