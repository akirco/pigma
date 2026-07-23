use rand::seq::SliceRandom;

use super::types::PlayMode;

pub trait PlayStrategy: Send {
    fn next(&mut self, current_index: Option<usize>, queue_len: usize) -> Option<usize>;
    fn prev(&mut self, current_index: Option<usize>, queue_len: usize) -> Option<usize>;
}

#[derive(Clone)]
pub enum Strategy {
    Sequential,
    RepeatOne,
    RepeatAll,
    Shuffle { order: Vec<usize>, pos: usize },
    Heartbeat,
}

impl PlayStrategy for Strategy {
    fn next(&mut self, ci: Option<usize>, queue_len: usize) -> Option<usize> {
        match self {
            Strategy::Sequential => match ci {
                Some(i) if i + 1 < queue_len => Some(i + 1),
                _ => None,
            },
            Strategy::RepeatOne => ci,
            Strategy::RepeatAll => {
                if queue_len == 0 {
                    return None;
                }
                let i = ci.unwrap_or(0);
                Some((i + 1) % queue_len)
            }
            Strategy::Shuffle { order, pos } => {
                if order.is_empty() || queue_len == 0 {
                    return None;
                }
                *pos = (*pos + 1) % order.len();
                Some(order[*pos])
            }
            Strategy::Heartbeat => None,
        }
    }

    fn prev(&mut self, ci: Option<usize>, queue_len: usize) -> Option<usize> {
        match self {
            Strategy::Sequential => match ci {
                Some(i) if i > 0 => Some(i - 1),
                _ => None,
            },
            Strategy::RepeatOne => ci,
            Strategy::RepeatAll => {
                if queue_len == 0 {
                    return None;
                }
                let i = ci.unwrap_or(0);
                Some((i + queue_len - 1) % queue_len)
            }
            Strategy::Shuffle { order, pos } => {
                if order.is_empty() {
                    return None;
                }
                *pos = (*pos + order.len() - 1) % order.len();
                Some(order[*pos])
            }
            Strategy::Heartbeat => None,
        }
    }
}

pub fn create_strategy(
    mode: &PlayMode,
    queue_len: usize,
    current_index: Option<usize>,
) -> Strategy {
    match mode {
        PlayMode::Sequential => Strategy::Sequential,
        PlayMode::RepeatOne => Strategy::RepeatOne,
        PlayMode::RepeatAll => Strategy::RepeatAll,
        PlayMode::Shuffle => Strategy::Shuffle {
            order: {
                let start = current_index.unwrap_or(0).min(queue_len.saturating_sub(1));
                let mut tail: Vec<usize> = (0..queue_len).filter(|&i| i != start).collect();
                tail.shuffle(&mut rand::rng());
                let mut order = vec![start];
                order.extend(tail);
                order
            },
            pos: 0,
        },
        PlayMode::Heartbeat { .. } => Strategy::Heartbeat,
    }
}
