use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::types::PluginInstanceId;

pub struct PluginTimingSlot {
    pub total_ns: AtomicU64,
    pub call_count: AtomicU64,
    pub last_ns: AtomicU64,
    pub last_quantum: AtomicU64,
    pub last_rate: AtomicU64,
    pub worker_total_ns: AtomicU64,
}

impl PluginTimingSlot {
    pub fn new() -> Self {
        Self {
            total_ns: AtomicU64::new(0),
            call_count: AtomicU64::new(0),
            last_ns: AtomicU64::new(0),
            last_quantum: AtomicU64::new(0),
            last_rate: AtomicU64::new(0),
            worker_total_ns: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn record(&self, elapsed_ns: u64, worker_ns: u64, quantum: u32, rate: u32) {
        self.total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
        self.call_count.fetch_add(1, Ordering::Relaxed);
        self.last_ns.store(elapsed_ns, Ordering::Relaxed);
        self.last_quantum.store(quantum as u64, Ordering::Relaxed);
        self.last_rate.store(rate as u64, Ordering::Relaxed);
        if worker_ns > 0 {
            self.worker_total_ns.fetch_add(worker_ns, Ordering::Relaxed);
        }
    }

    pub fn take_snapshot(&self) -> PluginCpuSnapshot {
        let total = self.total_ns.swap(0, Ordering::Relaxed);
        let calls = self.call_count.swap(0, Ordering::Relaxed);
        let last = self.last_ns.load(Ordering::Relaxed);
        let quantum = self.last_quantum.load(Ordering::Relaxed) as u32;
        let rate = self.last_rate.load(Ordering::Relaxed) as u32;
        let worker_total = self.worker_total_ns.swap(0, Ordering::Relaxed);

        let avg_ns = total.checked_div(calls).unwrap_or(0);
        let worker_avg_ns = worker_total.checked_div(calls).unwrap_or(0);

        let budget_ns = if rate > 0 && quantum > 0 {
            (quantum as f64 / rate as f64) * 1_000_000_000.0
        } else {
            0.0
        };

        let dsp_pct = if budget_ns > 0.0 {
            (avg_ns as f64 / budget_ns) * 100.0
        } else {
            0.0
        };

        let worker_pct = if budget_ns > 0.0 {
            (worker_avg_ns as f64 / budget_ns) * 100.0
        } else {
            0.0
        };

        PluginCpuSnapshot {
            avg_ns,
            last_ns: last,
            calls,
            dsp_percent: dsp_pct,
            worker_avg_ns,
            worker_percent: worker_pct,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PluginCpuSnapshot {
    pub avg_ns: u64,
    pub last_ns: u64,
    pub calls: u64,
    pub dsp_percent: f64,
    pub worker_avg_ns: u64,
    pub worker_percent: f64,
}

pub struct PluginCpuTracker {
    slots: Mutex<HashMap<PluginInstanceId, (String, Arc<PluginTimingSlot>)>>,
}

impl PluginCpuTracker {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, id: PluginInstanceId, name: String) -> Arc<PluginTimingSlot> {
        let slot = Arc::new(PluginTimingSlot::new());
        self.slots.lock().unwrap().insert(id, (name, slot.clone()));
        slot
    }

    pub fn rename(&self, id: PluginInstanceId, name: &str) {
        if let Some(entry) = self.slots.lock().unwrap().get_mut(&id) {
            entry.0 = name.to_string();
        }
    }

    pub fn unregister(&self, id: PluginInstanceId) {
        self.slots.lock().unwrap().remove(&id);
    }

    pub fn take_all_snapshots(&self) -> Vec<(PluginInstanceId, String, PluginCpuSnapshot)> {
        let slots = self.slots.lock().unwrap();
        let mut results: Vec<_> = slots
            .iter()
            .map(|(id, (name, slot))| (*id, name.clone(), slot.take_snapshot()))
            .collect();
        results.sort_by(|a, b| {
            b.2.dsp_percent
                .partial_cmp(&a.2.dsp_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}

static GLOBAL_TRACKER: OnceLock<PluginCpuTracker> = OnceLock::new();

pub fn global_cpu_tracker() -> &'static PluginCpuTracker {
    GLOBAL_TRACKER.get_or_init(PluginCpuTracker::new)
}
