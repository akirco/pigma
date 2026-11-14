//! TUI entry point and per-frame rendering hook.

use std::sync::Arc;

use ratatui::{DefaultTerminal, Frame};

use super::App;
use crate::{event::AuthEvent, state::Page, ui};

impl App {
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> eyre::Result<()> {
        self.start_splash_boot();
        let _ipc_guard = crate::ipc::start_server(
            Arc::clone(&self.ipc.status),
            Arc::clone(&self.ipc.queue),
            self.ipc.status_tx.clone(),
            self.events.sender(),
            Arc::clone(&self.search.engine),
        );
        while self.state.running {
            self.ipc.update(&self.playback);
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events().await?;

            let splash_ready = self.state.splash.shown_at.elapsed().as_secs_f64()
                >= self.config.splash_duration_secs;
            if self.state.splash.boot_complete
                && splash_ready
                && self.state.navigation.page == Page::Splash
            {
                if self.state.offline {
                    self.navigate_to_local();
                } else if self.awaiting_login_status {
                    // The startup session lookup is still in flight: stay on the
                    // splash. Its Success/Error handlers clear the flag and enter
                    // the main page, so login-gated endpoints are loaded with the
                    // UID already available rather than racing a false "未登录".
                } else if self.service.client().is_logged_in() && !self.login_status_requested {
                    self.login_status_requested = true;
                    self.awaiting_login_status = true;
                    let service = self.service.clone();
                    let sender = self.events.sender();
                    tokio::spawn(async move {
                        // Never strand the splash on a hung request: a timeout
                        // still emits an event that releases the wait below.
                        let result = tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            service.login_status(),
                        )
                        .await;
                        let event = match result {
                            Ok(Ok(info)) => AuthEvent::Success(info),
                            Ok(Err(e)) => {
                                log::error!("Failed to get login status: {e}");
                                AuthEvent::Error(e.to_string())
                            }
                            Err(_) => {
                                log::warn!("Startup login status lookup timed out");
                                AuthEvent::Error("获取登录状态超时".to_string())
                            }
                        };
                        if sender.send(event.into()).is_err() {
                            log::error!("Failed to send login status event: receiver dropped");
                        }
                    });
                } else {
                    self.navigate_to_main();
                }
            }
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        ui::draw(frame, self);
    }
}
