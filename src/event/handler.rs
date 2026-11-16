//! The event channels: terminal input plus internal events, delivered to the
//! app loop through two queues.
//!
//! Lifecycle/control events use an unbounded queue so a slow frame can never
//! drop `Quit`, authentication results, playback completion/errors, or IPC
//! requests. High-frequency events that are safe to coalesce (mouse movement,
//! redraw requests and playback progress) use a bounded queue; when it is full
//! the newest event is dropped, which is harmless because the next one carries
//! fresh state.

use std::{
    fmt,
    time::{Duration, Instant},
};

use crossterm::event::{Event as CrosstermEvent, MouseEventKind};
use eyre::OptionExt;
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc::{self, error::TrySendError};

use super::model::{AppEvent, Event, PlaybackEvent};

/// Bound the rate of pointer-move events delivered to the app. A terminal can
/// emit hundreds per second; the hover position only needs to be fresh enough
/// for a 60 Hz UI.
const MOUSE_MOVE_INTERVAL: Duration = Duration::from_millis(16);

/// Capacity of the coalescing queue. At 60 mouse moves/s this is roughly one
/// second of backlog; a UI that falls further behind only loses intermediate
/// updates, never control events.
const COALESCED_CAPACITY: usize = 64;

/// Error returned when the event receiver has been dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSendError;

impl fmt::Display for EventSendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("event receiver dropped")
    }
}

impl std::error::Error for EventSendError {}

/// Cloneable handle used by background tasks to send app events.
///
/// Automatically routes high-frequency, state-carrying events to the bounded
/// coalescing queue and everything else to the unbounded control queue.
#[derive(Clone)]
pub struct EventSender {
    control: mpsc::UnboundedSender<Event>,
    coalesced: mpsc::Sender<Event>,
}

impl EventSender {
    /// Send an event. Coalescing-queue overflow is not an error: the event is
    /// dropped because a newer one will arrive shortly.
    pub fn send<E: Into<Event>>(&self, event: E) -> Result<(), EventSendError> {
        let event = event.into();
        if is_coalesced(&event) {
            match self.coalesced.try_send(event) {
                Ok(()) => Ok(()),
                Err(TrySendError::Full(_)) => {
                    log::debug!("event loop is behind; dropped a coalesced event");
                    Ok(())
                }
                Err(TrySendError::Closed(_)) => Err(EventSendError),
            }
        } else {
            self.control.send(event).map_err(|_| EventSendError)
        }
    }

    /// Raw control-queue sender for consumers that only forward lifecycle
    /// events (the IPC server).
    pub(crate) fn control_sender(&self) -> mpsc::UnboundedSender<Event> {
        self.control.clone()
    }
}

/// Whether an event may be dropped when the coalescing queue is full.
fn is_coalesced(event: &Event) -> bool {
    match event {
        Event::Crossterm(CrosstermEvent::Mouse(mouse)) => mouse.kind == MouseEventKind::Moved,
        Event::App(AppEvent::Redraw) => true,
        Event::App(AppEvent::Playback(PlaybackEvent::Progress { .. })) => true,
        _ => false,
    }
}

/// Bridges crossterm terminal events into the app event channels.
pub struct EventHandler {
    sender: EventSender,
    control: mpsc::UnboundedReceiver<Event>,
    coalesced: mpsc::Receiver<Event>,
    /// Alternate which queue `try_recv` drains first, so a steady control
    /// stream cannot indefinitely starve redraws or pointer updates.
    prefer_coalesced: bool,
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(true)
    }
}

impl EventHandler {
    /// Create the event channels. With `with_terminal = true` a task reading
    /// crossterm terminal events is spawned; pass `false` for headless daemon
    /// mode (no terminal available).
    pub fn new(with_terminal: bool) -> Self {
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let (coalesced_tx, coalesced_rx) = mpsc::channel(COALESCED_CAPACITY);
        let sender = EventSender {
            control: control_tx,
            coalesced: coalesced_tx,
        };
        if with_terminal {
            let actor = EventTask::new(sender.clone());
            tokio::spawn(async { actor.run().await });
        }
        Self {
            sender,
            control: control_rx,
            coalesced: coalesced_rx,
            prefer_coalesced: false,
        }
    }

    pub async fn next(&mut self) -> eyre::Result<Event> {
        tokio::select! {
            event = self.control.recv() => event.ok_or_eyre("Failed to receive event"),
            event = self.coalesced.recv() => event.ok_or_eyre("Failed to receive event"),
        }
    }

    /// Drain events already queued for this frame without blocking.
    pub fn try_recv(&mut self) -> Option<Event> {
        if self.prefer_coalesced {
            self.prefer_coalesced = false;
            self.coalesced
                .try_recv()
                .ok()
                .or_else(|| self.control.try_recv().ok())
        } else {
            self.prefer_coalesced = true;
            self.control
                .try_recv()
                .ok()
                .or_else(|| self.coalesced.try_recv().ok())
        }
    }

    pub fn send<E: Into<Event>>(&mut self, event: E) {
        if let Err(e) = self.sender.send(event) {
            log::error!("Failed to send event: {e}");
        }
    }

    pub fn sender(&self) -> EventSender {
        self.sender.clone()
    }

    /// Control-only sender for the IPC server, whose events must never be
    /// dropped and are all low-frequency.
    pub(crate) fn control_sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.control_sender()
    }
}

struct EventTask {
    sender: EventSender,
    last_mouse_move: Option<Instant>,
}

impl EventTask {
    fn new(sender: EventSender) -> Self {
        Self {
            sender,
            last_mouse_move: None,
        }
    }

    async fn run(mut self) -> eyre::Result<()> {
        let mut reader = crossterm::event::EventStream::new();
        loop {
            // Do not pattern-match inside `select!`: a stream that ends or
            // errors must end the loop, not leave every branch disabled (which
            // panics and silently kills terminal input for the session).
            let crossterm_event = tokio::select! {
                _ = self.sender.control.closed() => break,
                event = reader.next().fuse() => event,
            };
            match crossterm_event {
                Some(Ok(evt)) => {
                    if let crossterm::event::Event::Mouse(mouse) = &evt
                        && mouse.kind == MouseEventKind::Moved
                    {
                        if self
                            .last_mouse_move
                            .is_some_and(|last| last.elapsed() < MOUSE_MOVE_INTERVAL)
                        {
                            continue;
                        }
                        self.last_mouse_move = Some(Instant::now());
                    }
                    self.send(Event::Crossterm(evt));
                }
                Some(Err(err)) => {
                    log::warn!("terminal event stream error: {err}");
                    break;
                }
                None => {
                    log::warn!("terminal event stream ended");
                    break;
                }
            }
        }
        Ok(())
    }

    fn send(&self, event: Event) {
        if let Err(e) = self.sender.send(event) {
            log::error!("EventTask: failed to send event: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(handler: &mut EventHandler) -> Vec<Event> {
        let mut events = Vec::new();
        while let Some(event) = handler.try_recv() {
            events.push(event);
        }
        events
    }

    #[test]
    fn control_events_are_not_dropped_by_a_full_coalescing_queue() {
        let mut handler = EventHandler::new(false);
        let sender = handler.sender();

        // Fill the bounded queue first, then enqueue a control event.
        for _ in 0..(COALESCED_CAPACITY * 3) {
            sender.send(AppEvent::Redraw).expect("send redraw");
        }
        sender.send(AppEvent::Quit).expect("send quit");

        let events = drain(&mut handler);
        assert!(matches!(events.first(), Some(Event::App(AppEvent::Quit))));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::App(AppEvent::Redraw)))
                .count(),
            COALESCED_CAPACITY
        );
    }

    #[test]
    fn coalesced_queue_is_bounded_but_keeps_the_control_queue_intact() {
        let mut handler = EventHandler::new(false);
        let sender = handler.sender();

        for _ in 0..COALESCED_CAPACITY * 4 {
            sender.send(AppEvent::Redraw).expect("send redraw");
        }
        sender
            .send(AppEvent::Toast("kept".into()))
            .expect("send toast");

        let events = drain(&mut handler);
        assert_eq!(events.len(), COALESCED_CAPACITY + 1);
        assert!(matches!(
            events.first(),
            Some(Event::App(AppEvent::Toast(_)))
        ));
    }

    #[test]
    fn progress_and_redraw_are_coalesced_but_finished_is_a_control_event() {
        assert!(is_coalesced(&Event::App(AppEvent::Redraw)));
        assert!(is_coalesced(&Event::App(AppEvent::Playback(
            PlaybackEvent::Progress {
                position: Duration::ZERO,
                total: None,
            }
        ))));
        assert!(!is_coalesced(&Event::App(AppEvent::Playback(
            PlaybackEvent::Finished
        ))));
        assert!(!is_coalesced(&Event::App(AppEvent::Quit)));
    }
}
