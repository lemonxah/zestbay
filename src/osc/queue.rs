use std::collections::VecDeque;
use std::sync::Mutex;

use super::{OscAction, OscRequest};

const CAPACITY: usize = 1024;
const DRAIN_LIMIT: usize = 128;

#[derive(Default)]
pub(super) struct RequestQueue(Mutex<VecDeque<OscRequest>>);

impl RequestQueue {
    pub(super) fn push(&self, request: OscRequest) {
        let mut pending = self.0.lock().unwrap();
        if let OscAction::Param { plugin, param, .. } = &request.action {
            let existing = pending
                .iter()
                .enumerate()
                .rev()
                .take_while(|(_, r)| matches!(r.action, OscAction::Param { .. }))
                .find_map(|(i, r)| match &r.action {
                    OscAction::Param {
                        plugin: p,
                        param: q,
                        ..
                    } if p == plugin && q == param => Some(i),
                    _ => None,
                });
            if let Some(i) = existing {
                pending.remove(i);
            }
        }
        if pending.len() == CAPACITY {
            if let Some(i) = pending
                .iter()
                .position(|r| matches!(r.action, OscAction::Param { .. }))
            {
                pending.remove(i);
            } else {
                return;
            }
        }
        pending.push_back(request);
    }

    pub(super) fn drain(&self) -> Vec<OscRequest> {
        let mut pending = self.0.lock().unwrap();
        let count = pending.len().min(DRAIN_LIMIT);
        pending.drain(..count).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: OscAction) -> OscRequest {
        OscRequest {
            action,
            from: "127.0.0.1:9001".parse().unwrap(),
        }
    }

    fn param(name: &str, value: f32, normalized: bool) -> OscRequest {
        request(OscAction::Param {
            plugin: "synth".into(),
            param: name.into(),
            value,
            normalized,
        })
    }

    #[test]
    fn flood_keeps_latest_value_and_bounds_work() {
        let queue = RequestQueue::default();
        for i in 0..10_000 {
            queue.push(param(&i.to_string(), i as f32, false));
        }
        assert_eq!(queue.0.lock().unwrap().len(), CAPACITY);
        queue.push(param("9999", 0.5, true));
        let mut all = Vec::new();
        loop {
            let batch = queue.drain();
            assert!(batch.len() <= DRAIN_LIMIT);
            if batch.is_empty() {
                break;
            }
            all.extend(batch);
        }
        assert_eq!(all.len(), CAPACITY);
        assert_eq!(all.last().unwrap().action, param("9999", 0.5, true).action);
    }

    #[test]
    fn coalescing_preserves_command_barriers() {
        let queue = RequestQueue::default();
        queue.push(param("gain", 0.2, false));
        queue.push(request(OscAction::Preset {
            plugin: "synth".into(),
            name: "init".into(),
        }));
        queue.push(param("gain", 0.3, false));
        queue.push(param("gain", 0.4, true));
        let actions: Vec<_> = queue.drain().into_iter().map(|r| r.action).collect();
        assert_eq!(
            actions,
            vec![
                param("gain", 0.2, false).action,
                OscAction::Preset {
                    plugin: "synth".into(),
                    name: "init".into()
                },
                param("gain", 0.4, true).action,
            ]
        );
    }

    #[test]
    fn commands_survive_parameter_saturation() {
        let queue = RequestQueue::default();
        for i in 0..CAPACITY {
            queue.push(param(&i.to_string(), 0.0, false));
        }
        queue.push(request(OscAction::LoadSession {
            name: "live".into(),
        }));
        for i in 0..CAPACITY {
            queue.push(param(&i.to_string(), 1.0, false));
        }
        assert_eq!(queue.0.lock().unwrap().len(), CAPACITY);
        assert!(
            queue
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|r| matches!(r.action, OscAction::LoadSession { .. }))
        );
    }
}
