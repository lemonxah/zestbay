use super::*;

impl qobject::AppController {
    pub fn get_cpu_history(self: Pin<&mut Self>) -> QString {
        let json = serde_json::to_string(&self.rust().cpu_history).unwrap_or_default();
        QString::from(&json)
    }

    pub fn get_plugin_cpu_json(self: Pin<&mut Self>) -> QString {
        use crate::plugin::cpu_stats::global_cpu_tracker;

        let snapshots = global_cpu_tracker().take_all_snapshots();
        let is_meter = |id: u64| {
            self.rust()
                .plugin_manager
                .as_ref()
                .and_then(|mgr| mgr.get_instance(id))
                .is_some_and(|info| crate::ladspa::builtin::is_builtin_uri(&info.plugin_uri))
        };
        let items: Vec<serde_json::Value> = snapshots
            .into_iter()
            .map(|(id, name, snap)| {
                serde_json::json!({
                    "id": id,
                    "name": name,
                    "isMeter": is_meter(id),
                    "dspPercent": (snap.dsp_percent * 100.0).round() / 100.0,
                    "avgUs": snap.avg_ns / 1000,
                    "lastUs": snap.last_ns / 1000,
                    "calls": snap.calls,
                    "workerPercent": (snap.worker_percent * 100.0).round() / 100.0,
                    "workerAvgUs": snap.worker_avg_ns / 1000,
                })
            })
            .collect();
        let json = serde_json::to_string(&items).unwrap_or_default();
        QString::from(&json)
    }
}

pub(super) fn read_process_cpu_ticks() -> u64 {
    let Ok(stat) = std::fs::read_to_string("/proc/self/stat") else {
        return 0;
    };
    let fields: Vec<&str> = stat.split_whitespace().collect();
    if fields.len() < 15 {
        return 0;
    }
    let utime: u64 = fields[13].parse().unwrap_or(0);
    let stime: u64 = fields[14].parse().unwrap_or(0);
    utime + stime
}

pub(super) fn clock_ticks_per_sec() -> f64 {
    static TICKS: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *TICKS.get_or_init(|| {
        unsafe extern "C" {
            fn sysconf(name: i32) -> i64;
        }
        const _SC_CLK_TCK: i32 = 2;
        let val = unsafe { sysconf(_SC_CLK_TCK) };
        if val > 0 { val as f64 } else { 100.0 }
    })
}

pub(super) fn num_cpus() -> f64 {
    static CPUS: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *CPUS.get_or_init(|| {
        unsafe extern "C" {
            fn sysconf(name: i32) -> i64;
        }
        const _SC_NPROCESSORS_ONLN: i32 = 84;
        let val = unsafe { sysconf(_SC_NPROCESSORS_ONLN) };
        if val > 0 { val as f64 } else { 1.0 }
    })
}

pub(super) fn measure_cpu_usage(
    prev_ticks: &mut u64,
    prev_time: &mut Option<Instant>,
    avg: &mut f64,
) -> String {
    const ALPHA: f64 = 0.15;

    let current_ticks = read_process_cpu_ticks();
    let now = Instant::now();

    if let Some(prev) = *prev_time {
        let elapsed = now.duration_since(prev).as_secs_f64();
        if elapsed > 0.0 {
            let delta_ticks = current_ticks.saturating_sub(*prev_ticks) as f64;
            let cpu_secs = delta_ticks / clock_ticks_per_sec();
            let sample = (cpu_secs / elapsed / num_cpus()) * 100.0;
            *avg = *avg * (1.0 - ALPHA) + sample * ALPHA;
        }
    }

    *prev_ticks = current_ticks;
    *prev_time = Some(now);
    format!("{:.1}%", *avg)
}
