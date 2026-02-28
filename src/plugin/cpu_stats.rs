//! Lock-free per-plugin CPU usage tracking for real-time process callbacks.
//!
//! The RT audio threads write timing data via atomics (no locks), and the
//! UI thread reads snapshots periodically.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::types::PluginInstanceId;

/// Timing data for a single plugin, written from the RT thread.
pub struct PluginTimingSlot {
    /// Cumulative nanoseconds spent in `process()` on the RT thread.
    pub total_ns: AtomicU64,
    /// Number of process() calls since last reset.
    pub call_count: AtomicU64,
    /// Most recent single-call duration in nanoseconds.
    pub last_ns: AtomicU64,
    /// The quantum (buffer size) seen on the last call.
    pub last_quantum: AtomicU64,
    /// The sample rate seen on the last call.
    pub last_rate: AtomicU64,
    /// Cumulative nanoseconds spent in the worker thread (async, off RT).
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

    /// Called from the RT thread after each process() call.
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

    /// Read and reset the accumulated stats (called from the UI thread).
    pub fn take_snapshot(&self) -> PluginCpuSnapshot {
        let total = self.total_ns.swap(0, Ordering::Relaxed);
        let calls = self.call_count.swap(0, Ordering::Relaxed);
        let last = self.last_ns.load(Ordering::Relaxed);
        let quantum = self.last_quantum.load(Ordering::Relaxed) as u32;
        let rate = self.last_rate.load(Ordering::Relaxed) as u32;
        let worker_total = self.worker_total_ns.swap(0, Ordering::Relaxed);

        let avg_ns = if calls > 0 { total / calls } else { 0 };
        let worker_avg_ns = if calls > 0 { worker_total / calls } else { 0 };

        // DSP load: what fraction of the available buffer time was used
        // Only RT thread time counts toward the deadline
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

/// A snapshot of one plugin's CPU usage for a measurement window.
#[derive(Clone, Debug)]
pub struct PluginCpuSnapshot {
    pub avg_ns: u64,
    pub last_ns: u64,
    pub calls: u64,
    /// RT thread DSP load (% of buffer budget)
    pub dsp_percent: f64,
    /// Worker thread average time per buffer (ns)
    pub worker_avg_ns: u64,
    /// Worker thread load expressed as % of buffer budget (for context, not a deadline)
    pub worker_percent: f64,
}

/// Global registry of per-plugin timing slots.
pub struct PluginCpuTracker {
    slots: Mutex<HashMap<PluginInstanceId, (String, Arc<PluginTimingSlot>)>>,
}

impl PluginCpuTracker {
    pub fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }

    /// Register a plugin and return its timing slot for the RT thread to use.
    pub fn register(&self, id: PluginInstanceId, name: String) -> Arc<PluginTimingSlot> {
        let slot = Arc::new(PluginTimingSlot::new());
        self.slots.lock().unwrap().insert(id, (name, slot.clone()));
        slot
    }

    /// Unregister a plugin when it's removed.
    pub fn unregister(&self, id: PluginInstanceId) {
        self.slots.lock().unwrap().remove(&id);
    }

    /// Take snapshots of all plugins and return them sorted by DSP%.
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

/// Global singleton so filter callbacks can access it without passing through PipeWire.
static GLOBAL_TRACKER: OnceLock<PluginCpuTracker> = OnceLock::new();

pub fn global_cpu_tracker() -> &'static PluginCpuTracker {
    GLOBAL_TRACKER.get_or_init(PluginCpuTracker::new)
}
