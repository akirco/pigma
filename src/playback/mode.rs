use ncm_api::SongInfo;
use rand::seq::SliceRandom;

use super::types::PlayMode;

pub trait PlayStrategy: Send {
    fn next(&mut self, current_index: Option<usize>, queue: &[SongInfo]) -> Option<usize>;
    fn prev(&mut self, current_index: Option<usize>, queue: &[SongInfo]) -> Option<usize>;
}

pub struct Sequential;

impl PlayStrategy for Sequential {
    fn next(&mut self, ci: Option<usize>, queue: &[SongInfo]) -> Option<usize> {
        match ci {
            Some(i) if i + 1 < queue.len() => Some(i + 1),
            _ => None,
        }
    }

    fn prev(&mut self, ci: Option<usize>, _queue: &[SongInfo]) -> Option<usize> {
        match ci {
            Some(i) if i > 0 => Some(i - 1),
            _ => None,
        }
    }
}

pub struct RepeatOne;

impl PlayStrategy for RepeatOne {
    fn next(&mut self, ci: Option<usize>, _queue: &[SongInfo]) -> Option<usize> {
        ci
    }

    fn prev(&mut self, ci: Option<usize>, _queue: &[SongInfo]) -> Option<usize> {
        ci
    }
}

pub struct RepeatAll;

impl PlayStrategy for RepeatAll {
    fn next(&mut self, ci: Option<usize>, queue: &[SongInfo]) -> Option<usize> {
        if queue.is_empty() {
            return None;
        }
        let i = ci.unwrap_or(0);
        Some((i + 1) % queue.len())
    }

    fn prev(&mut self, ci: Option<usize>, queue: &[SongInfo]) -> Option<usize> {
        if queue.is_empty() {
            return None;
        }
        let i = ci.unwrap_or(0);
        Some((i + queue.len() - 1) % queue.len())
    }
}

pub struct ShuffleMode {
    order: Vec<usize>,
    pos: usize,
}

impl ShuffleMode {
    pub fn new(queue_len: usize, current_index: usize) -> Self {
        if queue_len <= 1 {
            return Self {
                order: (0..queue_len).collect(),
                pos: 0,
            };
        }
        let start = current_index.min(queue_len - 1);
        let mut tail: Vec<usize> = (0..queue_len).filter(|&i| i != start).collect();
        let mut rng = rand::thread_rng();
        tail.shuffle(&mut rng);
        let mut order = vec![start];
        order.extend(tail);
        Self { order, pos: 0 }
    }
}

impl PlayStrategy for ShuffleMode {
    fn next(&mut self, _ci: Option<usize>, queue: &[SongInfo]) -> Option<usize> {
        if self.order.is_empty() || queue.is_empty() {
            return None;
        }
        self.pos = (self.pos + 1) % self.order.len();
        Some(self.order[self.pos])
    }

    fn prev(&mut self, _ci: Option<usize>, _queue: &[SongInfo]) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        self.pos = (self.pos + self.order.len() - 1) % self.order.len();
        Some(self.order[self.pos])
    }
}

pub struct HeartbeatMode;

impl HeartbeatMode {
    pub fn new() -> Self {
        Self
    }
}

impl PlayStrategy for HeartbeatMode {
    fn next(&mut self, _ci: Option<usize>, _queue: &[SongInfo]) -> Option<usize> {
        None
    }

    fn prev(&mut self, _ci: Option<usize>, _queue: &[SongInfo]) -> Option<usize> {
        None
    }
}

pub fn create_strategy(
    mode: &PlayMode,
    queue_len: usize,
    current_index: Option<usize>,
) -> Box<dyn PlayStrategy> {
    match mode {
        PlayMode::Sequential => Box::new(Sequential),
        PlayMode::RepeatOne => Box::new(RepeatOne),
        PlayMode::RepeatAll => Box::new(RepeatAll),
        PlayMode::Shuffle => Box::new(ShuffleMode::new(queue_len, current_index.unwrap_or(0))),
        PlayMode::Heartbeat { .. } => Box::new(HeartbeatMode::new()),
    }
}
