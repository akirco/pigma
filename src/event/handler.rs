//! The event channel: terminal input plus internal events, delivered to the app
//! loop through a single mpsc queue.

use eyre::OptionExt;
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;

use super::model::Event;

/// Bridges crossterm terminal events into an internal mpsc event channel.
pub struct EventHandler {
    sender: mpsc::UnboundedSender<Event>,
    receiver: mpsc::UnboundedReceiver<Event>,
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(true)
    }
}

impl EventHandler {
    /// Create an event channel. With `with_terminal = true` a task reading
    /// crossterm terminal events is spawned; pass `false` for headless daemon
    /// mode (no terminal available).
    pub fn new(with_terminal: bool) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        if with_terminal {
            let actor = EventTask::new(sender.clone());
            tokio::spawn(async { actor.run().await });
        }
        Self { sender, receiver }
    }

    pub async fn next(&mut self) -> eyre::Result<Event> {
        self.receiver
            .recv()
            .await
            .ok_or_eyre("Failed to receive event")
    }

    pub fn send<E: Into<Event>>(&mut self, event: E) {
        if let Err(e) = self.sender.send(event.into()) {
            log::error!("Failed to send event: {}", e);
        }
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<Event> {
        self.sender.clone()
    }
}

struct EventTask {
    sender: mpsc::UnboundedSender<Event>,
}

impl EventTask {
    fn new(sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { sender }
    }

    async fn run(self) -> eyre::Result<()> {
        let mut reader = crossterm::event::EventStream::new();
        loop {
            let crossterm_event = reader.next().fuse();

            tokio::select! {
              _ = self.sender.closed() => {
                break;
              }
              Some(Ok(evt)) = crossterm_event => {
                self.send(Event::Crossterm(evt));
              }
            };
        }
        Ok(())
    }

    fn send(&self, event: Event) {
        if self.sender.send(event).is_err() {
            log::error!("EventTask: failed to send event: receiver dropped");
        }
    }
}
