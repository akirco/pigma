//! Animated page transitions: a background-coloured mask sweeps in, then the
//! destination page fades in from the same background (the web-style effect
//! used when clicking the player-bar cover to open the lyrics page).

use std::time::{Duration, Instant};

use ratatui::{Frame, style::Color};
use tachyonfx::{Effect, EffectTimer, Interpolation, Motion, fx};

use crate::state::Page;

/// Duration of the background mask sweeping up over the current page.
const SWEEP_MS: u32 = 240;
/// Duration of the destination page fading in from the background.
const FADE_MS: u32 = 260;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Sweep,
    Fade,
}

/// What the transition wants the app loop to do next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TransitionStep {
    /// Keep drawing the current phase.
    Running,
    /// The mask has covered the page: switch pages and start the fade.
    StartFade,
    /// The fade is finished; render normally from now on.
    Done,
}

pub(crate) struct PageTransition {
    phase: Phase,
    target: Page,
    effect: Effect,
    started: Instant,
    last_tick: Instant,
}

impl PageTransition {
    /// A solid background mask sweeping upward over the current page.
    pub(crate) fn sweep(bg: Color, target: Page) -> Self {
        Self::new(
            Phase::Sweep,
            target,
            fx::sweep_out(
                Motion::DownToUp,
                4,
                0,
                bg,
                EffectTimer::from_ms(SWEEP_MS, Interpolation::QuadOut),
            ),
        )
    }

    /// The destination page fading in from the background.
    pub(crate) fn fade(bg: Color, target: Page) -> Self {
        Self::new(
            Phase::Fade,
            target,
            fx::fade_from(
                bg,
                bg,
                EffectTimer::from_ms(FADE_MS, Interpolation::QuadOut),
            ),
        )
    }

    fn new(phase: Phase, target: Page, effect: Effect) -> Self {
        let now = Instant::now();
        Self {
            phase,
            target,
            effect,
            started: now,
            last_tick: now,
        }
    }

    /// Page to show once the sweeping mask has covered the screen.
    pub(crate) fn target(&self) -> Page {
        self.target
    }

    pub(crate) fn step(&self, now: Instant) -> TransitionStep {
        let elapsed = now.saturating_duration_since(self.started);
        match self.phase {
            Phase::Sweep if elapsed >= Duration::from_millis(SWEEP_MS as u64) => {
                TransitionStep::StartFade
            }
            Phase::Fade if elapsed >= Duration::from_millis(FADE_MS as u64) => TransitionStep::Done,
            _ => TransitionStep::Running,
        }
    }

    /// Advance and apply the effect to the frame that was just drawn.
    pub(crate) fn render(&mut self, frame: &mut Frame) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        let elapsed = elapsed.as_millis().min(u32::MAX as u128) as u32;
        if elapsed > 0 {
            let area = frame.area();
            self.effect.process(
                tachyonfx::Duration::from_millis(elapsed),
                frame.buffer_mut(),
                area,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_hands_over_to_the_fade_after_its_duration() {
        let transition = PageTransition::sweep(Color::Black, Page::Lyrics);
        assert_eq!(transition.target(), Page::Lyrics);
        assert_eq!(transition.step(Instant::now()), TransitionStep::Running);
        assert_eq!(
            transition.step(transition.started + Duration::from_millis(SWEEP_MS as u64)),
            TransitionStep::StartFade
        );

        let fade = PageTransition::fade(Color::Black, Page::Main);
        assert_eq!(fade.target(), Page::Main);
        assert_eq!(
            fade.step(fade.started + Duration::from_millis(FADE_MS as u64)),
            TransitionStep::Done
        );
    }
}
