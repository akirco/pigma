//! TUI entry point and per-frame rendering hook.

use std::{sync::Arc, time::Duration};

use ratatui::{DefaultTerminal, Frame};

use super::App;
use crate::{
    event::AuthEvent,
    state::Page,
    ui::{self, PageTransition, TransitionStep},
};

/// How long a toast stays on screen; the single source for frame-start expiry.
const TOAST_TTL_SECS: u64 = 2;

impl App {
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> eyre::Result<()> {
        self.start_splash_boot();
        let _ipc_guard = crate::ipc::start_server(
            Arc::clone(&self.ipc.status),
            Arc::clone(&self.ipc.queue),
            self.ipc.status_tx.clone(),
            self.events.control_sender(),
            Arc::clone(&self.search.engine),
        );
        while self.state.running {
            self.ipc.update(&self.playback);
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events().await?;

            let splash_ready = self.state.splash.shown_at.elapsed() >= self.splash_duration();
            if self.state.splash.boot_complete
                && splash_ready
                && self.state.navigation.page == Page::Splash
                && self.page_transition.is_none()
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
                        if sender.send(event).is_err() {
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
        self.begin_frame();
        self.advance_page_transition();
        let render_start = std::time::Instant::now();
        let outcome = ui::draw(frame, self);
        self.render_page_transition(frame);
        let render = render_start.elapsed();
        self.apply_render_outcome(outcome);
        // Prefetch after the frame: the renderer settles on the visible window
        // this pass, so the URL list is only accurate once `ui::draw` returned.
        // `ui` stays free of network work and of state writes; its layout
        // results arrive through `RenderOutcome` and are applied above.
        // `prefetch_content_covers` skips URLs already cached or in flight, so
        // one call per frame costs a pass over the visible window at most.
        let prefetch_start = std::time::Instant::now();
        self.prefetch_content_covers();
        self.draw_stats.record(render, prefetch_start.elapsed());
    }

    /// Start the shared page-change animation. The actual page switch happens
    /// when the sweeping mask has covered the screen, so callers keep the old
    /// page until `advance_page_transition` hands over.
    pub(crate) fn start_page_transition(&mut self, target: Page) {
        if target == self.state.navigation.page || self.page_transition.is_some() {
            return;
        }
        if !self.animate_pages {
            self.state.navigation.page = target;
            return;
        }
        let bg = self.theme_registry.resolve(&self.config.default_theme).bg;
        self.page_transition = Some(PageTransition::sweep(bg, target));
    }

    /// Move the page transition to its next phase. The sweeping mask delays the
    /// page switch until it has covered the screen; the destination page is
    /// then drawn under the fade-in effect.
    fn advance_page_transition(&mut self) {
        let now = std::time::Instant::now();
        match self.page_transition.as_ref().map(|t| t.step(now)) {
            Some(TransitionStep::StartFade) => {
                let target = self
                    .page_transition
                    .as_ref()
                    .map(PageTransition::target)
                    .unwrap_or(Page::Lyrics);
                self.state.navigation.page = target;
                let bg = self.theme_registry.resolve(&self.config.default_theme).bg;
                self.page_transition = Some(PageTransition::fade(bg, target));
            }
            Some(TransitionStep::Done) => self.page_transition = None,
            _ => {}
        }
    }

    /// Apply the active transition shader to the frame that was just drawn.
    fn render_page_transition(&mut self, frame: &mut Frame) {
        if let Some(transition) = self.page_transition.as_mut() {
            transition.render(frame);
        }
    }

    /// Frame-start bookkeeping. The tick and toast expiry used to live inside
    /// `ui::draw`, which made the renderer a state writer for no good reason.
    fn begin_frame(&mut self) {
        let now = std::time::Instant::now();
        let steps = (now.duration_since(self.state.last_tick).as_millis() / 80).max(1) as u64;
        self.state.last_tick = now;
        self.state.tick = self.state.tick.wrapping_add(steps);
        if let Some(t) = self.state.toast_time
            && t.elapsed() > Duration::from_secs(TOAST_TTL_SECS)
        {
            self.state.toast_time = None;
        }
    }

    /// The one write-back point for geometry-derived values. Input handlers run
    /// after `draw` in the event loop, so they read the layout of the frame the
    /// user is actually looking at rather than the previous frame's leftovers.
    fn apply_render_outcome(&mut self, outcome: ui::RenderOutcome) {
        if let Some(layout) = outcome.content_layout {
            let nav = &mut self.state.navigation;
            nav.content_columns = layout.columns;
            nav.content_scroll = layout.scroll;
            nav.content_visible = layout.visible;
            nav.content_hovered = layout.hovered;
            nav.clamp_selection();
        }
        self.set_playerbar_area(outcome.playerbar_area);
        self.set_playerbar_hits(outcome.playerbar_hits);
        if let Some(max) = outcome.help_max_scroll {
            self.state.help.max_scroll = max;
            self.state.help.scroll = self.state.help.scroll.min(max);
        }
        if let Some(x) = outcome.queue_tab_scroll_x {
            self.state.navigation.queue_tab_scroll_x = x;
        }
        if let Some((body, offset)) = outcome.queue_body {
            self.state.navigation.queue_body = body;
            self.state.navigation.queue_offset = offset;
        }
        if let Some((area, hits)) = outcome.queue_tabs {
            self.state.navigation.queue_tab_area = area;
            self.state.navigation.queue_tab_hits = hits;
        }
    }
}
