//! CLAP plugin scanner.
//!
//! Scans standard directories for `.clap` bundles, dlopen's each one,
//! and enumerates the plugins within via the CLAP plugin factory.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::plugin::types::*;

/// Standard Linux directories where CLAP plugins are installed.
const CLAP_SEARCH_DIRS: &[&str] = &[
    "~/.clap",
    "/usr/lib/clap",
    "/usr/local/lib/clap",
    "/usr/lib64/clap",
    "/usr/local/lib64/clap",
];

/// Results of the last full scan, keyed by URI (shared shape with VST3).
pub(crate) struct ScanCache {
    inner: Mutex<Option<(HashMap<String, PluginInfo>, Instant)>>,
}

/// A cache miss triggers a full rescan at most this often.
const MISS_RESCAN_COOLDOWN: Duration = Duration::from_secs(30);

impl ScanCache {
    pub(crate) const fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    pub(crate) fn store(&self, plugins: &[PluginInfo]) {
        let map = plugins.iter().map(|p| (p.uri.clone(), p.clone())).collect();
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some((map, Instant::now()));
    }

    pub(crate) fn get(&self, uri: &str) -> Option<PluginInfo> {
        let lock = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        lock.as_ref().and_then(|(map, _)| map.get(uri).cloned())
    }

    /// Whether a miss may trigger a full rescan (never scanned, or the last
    /// scan is older than the cooldown).
    pub(crate) fn miss_should_rescan(&self) -> bool {
        let lock = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match lock.as_ref() {
            None => true,
            Some((_, at)) => at.elapsed() >= MISS_RESCAN_COOLDOWN,
        }
    }

    /// Cached lookup; on a miss rescans with `scan` (rate limited).
    pub(crate) fn find(&self, uri: &str, scan: impl FnOnce()) -> Option<PluginInfo> {
        if let Some(info) = self.get(uri) {
            return Some(info);
        }
        if !self.miss_should_rescan() {
            return None;
        }
        scan();
        self.get(uri)
    }
}

static CACHE: ScanCache = ScanCache::new();

/// Look up a single plugin by URI.  Served from the cache of the last
/// `scan_plugins()`; a miss rescans at most once per cooldown period.
pub fn find_plugin(uri: &str) -> Option<PluginInfo> {
    CACHE.find(uri, || {
        scan_plugins();
    })
}

/// Scan all standard directories and return a list of CLAP plugin infos.
pub fn scan_plugins() -> Vec<PluginInfo> {
    let mut plugins = Vec::new();
    let dirs = expand_search_dirs();

    for dir in &dirs {
        if !dir.is_dir() {
            continue;
        }
        log::info!("CLAP: scanning {}", dir.display());
        scan_directory(dir, &mut plugins);
    }

    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    log::info!("CLAP: found {} plugins total", plugins.len());
    CACHE.store(&plugins);
    plugins
}

fn expand_search_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    CLAP_SEARCH_DIRS
        .iter()
        .filter_map(|d| {
            let path = if d.starts_with('~') {
                PathBuf::from(d.replacen('~', &home, 1))
            } else {
                PathBuf::from(d)
            };
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(canonical) {
                Some(path)
            } else {
                log::debug!("CLAP: skipping duplicate directory {}", path.display());
                None
            }
        })
        .collect()
}

fn scan_directory(dir: &Path, plugins: &mut Vec<PluginInfo>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("CLAP: cannot read {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.extension().is_some_and(|e| e == "clap") {
                sandboxed_scan_clap_file(&path, plugins);
            } else {
                scan_directory(&path, plugins);
            }
        } else if path.extension().is_some_and(|e| e == "clap") {
            sandboxed_scan_clap_file(&path, plugins);
        }
    }
}

/// Run `scan_clap_file` inside a forked child process so that a segfault
/// in the plugin's dlopen / init / factory code only kills the child.
fn sandboxed_scan_clap_file(path: &Path, plugins: &mut Vec<PluginInfo>) {
    use crate::plugin::sandbox::{SandboxResult, fork_scan};

    let path_owned = path.to_path_buf();
    let timeout = std::time::Duration::from_secs(10);

    let result: SandboxResult<Vec<PluginInfo>> = fork_scan(
        move || {
            let mut found = Vec::new();
            scan_clap_file(&path_owned, &mut found);
            found
        },
        Some(timeout),
    );

    match result {
        SandboxResult::Ok(found) => {
            log::debug!("CLAP sandbox: {} scanned OK ({} plugins)", path.display(), found.len());
            plugins.extend(found);
        }
        SandboxResult::Crashed { signal, description } => {
            log::warn!(
                "CLAP sandbox: {} crashed during scan (signal {:?}): {}",
                path.display(), signal, description
            );
        }
        SandboxResult::Timeout => {
            log::warn!("CLAP sandbox: {} timed out during scan", path.display());
        }
        SandboxResult::ForkFailed(e) => {
            log::error!("CLAP sandbox: fork failed for {}: {}", path.display(), e);
            // Fallback: scan in-process (same as before sandboxing)
            scan_clap_file(path, plugins);
        }
    }
}

fn scan_clap_file(path: &Path, plugins: &mut Vec<PluginInfo>) {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => return,
    };

    log::debug!("CLAP: loading {}", path_str);

    let c_path = match CString::new(path_str) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Safety: All CLAP interactions use raw C FFI via dlopen.
    unsafe {
        let lib = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            let err = CStr::from_ptr(libc::dlerror());
            log::debug!("CLAP: dlopen failed for {}: {:?}", path_str, err);
            return;
        }

        let entry_ptr = libc::dlsym(lib, c"clap_entry".as_ptr());
        if entry_ptr.is_null() {
            log::debug!("CLAP: no clap_entry in {}", path_str);
            libc::dlclose(lib);
            return;
        }

        let entry = &*(entry_ptr as *const clap_sys::entry::clap_plugin_entry);

        if entry.clap_version.major < 1 {
            log::debug!("CLAP: unsupported version in {}", path_str);
            libc::dlclose(lib);
            return;
        }

        let init_ok = match entry.init {
            Some(init_fn) => init_fn(c_path.as_ptr()),
            None => {
                libc::dlclose(lib);
                return;
            }
        };

        if !init_ok {
            log::warn!("CLAP: init() returned false for {}", path_str);
            if let Some(deinit) = entry.deinit {
                deinit();
            }
            libc::dlclose(lib);
            return;
        }

        let factory_ptr = match entry.get_factory {
            Some(get_factory) => {
                get_factory(clap_sys::factory::plugin_factory::CLAP_PLUGIN_FACTORY_ID.as_ptr())
            }
            None => {
                if let Some(deinit) = entry.deinit {
                    deinit();
                }
                libc::dlclose(lib);
                return;
            }
        };

        if factory_ptr.is_null() {
            if let Some(deinit) = entry.deinit {
                deinit();
            }
            libc::dlclose(lib);
            return;
        }

        let factory = &*(factory_ptr
            as *const clap_sys::factory::plugin_factory::clap_plugin_factory);

        let count = match factory.get_plugin_count {
            Some(f) => f(factory),
            None => 0,
        };

        for i in 0..count {
            let desc_ptr = match factory.get_plugin_descriptor {
                Some(f) => f(factory, i),
                None => continue,
            };
            if desc_ptr.is_null() {
                continue;
            }
            let desc = &*desc_ptr;

            let id = if desc.id.is_null() {
                continue;
            } else {
                CStr::from_ptr(desc.id)
                    .to_str()
                    .unwrap_or("")
                    .to_string()
            };

            let name = if desc.name.is_null() {
                id.clone()
            } else {
                CStr::from_ptr(desc.name)
                    .to_str()
                    .unwrap_or(&id)
                    .to_string()
            };

            let vendor = if !desc.vendor.is_null() {
                let s = CStr::from_ptr(desc.vendor)
                    .to_str()
                    .unwrap_or("")
                    .to_string();
                if s.is_empty() { None } else { Some(s) }
            } else {
                None
            };

            let features = parse_features(desc.features);
            let category = category_from_features(&features);

            // Infer audio layout from descriptor features (no instantiation).
            // Accurate port info is discovered at instantiation time in host.rs.
            let is_instrument = features.iter().any(|f| f == "instrument");
            let (audio_inputs, audio_outputs) = if is_instrument {
                (0, 2) // Instruments: no audio input, stereo output
            } else {
                (2, 2) // Effects: stereo in/out (default assumption)
            };

            // GUI support is checked at instantiation time via get_extension.
            // Most CLAP plugins ship with a GUI, so default to true.
            let has_ui = true;

            plugins.push(PluginInfo {
                uri: id,
                name,
                format: PluginFormat::Clap,
                category,
                author: vendor,
                ports: Vec::new(), // Populated at instantiation time
                audio_inputs,
                audio_outputs,
                control_inputs: 0,
                control_outputs: 0,
                required_features: Vec::new(),
                compatible: true,
                has_ui,
                library_path: path_str.to_string(),
            });
        }

        if let Some(deinit) = entry.deinit {
            deinit();
        }
        // Do NOT dlclose — plugin descriptor strings live inside the .so
    }
}

fn parse_features(features_ptr: *const *const std::ffi::c_char) -> Vec<String> {
    let mut features = Vec::new();
    if features_ptr.is_null() {
        return features;
    }
    unsafe {
        let mut i = 0;
        loop {
            let p = *features_ptr.add(i);
            if p.is_null() {
                break;
            }
            if let Ok(s) = CStr::from_ptr(p).to_str() {
                features.push(s.to_string());
            }
            i += 1;
        }
    }
    features
}

fn category_from_features(features: &[String]) -> PluginCategory {
    for f in features {
        let f = f.to_lowercase();
        match f.as_str() {
            "reverb" => return PluginCategory::Reverb,
            "delay" => return PluginCategory::Delay,
            "distortion" => return PluginCategory::Distortion,
            "compressor" => return PluginCategory::Compressor,
            "limiter" => return PluginCategory::Limiter,
            "equalizer" | "eq" => return PluginCategory::Equaliser,
            "filter" => return PluginCategory::Filter,
            "chorus" | "flanger" | "phaser" => return PluginCategory::Chorus,
            "amplifier" => return PluginCategory::Amplifier,
            "mixer" => return PluginCategory::Mixer,
            "instrument" | "synthesizer" | "sampler" => return PluginCategory::Instrument,
            "analyzer" => return PluginCategory::Analyser,
            "utility" => return PluginCategory::Utility,
            "spatial" | "surround" => return PluginCategory::Spatial,
            "generator" => return PluginCategory::Generator,
            _ => {}
        }
    }
    if features.iter().any(|f| f == "audio-effect") {
        return PluginCategory::Filter;
    }
    if features.iter().any(|f| f == "instrument") {
        return PluginCategory::Instrument;
    }
    PluginCategory::Other("CLAP".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(uri: &str) -> PluginInfo {
        PluginInfo {
            uri: uri.to_string(),
            name: uri.to_string(),
            format: PluginFormat::Clap,
            category: PluginCategory::Utility,
            author: None,
            ports: Vec::new(),
            audio_inputs: 0,
            audio_outputs: 0,
            control_inputs: 0,
            control_outputs: 0,
            required_features: Vec::new(),
            compatible: true,
            has_ui: false,
            library_path: format!("/x/{uri}.clap"),
        }
    }

    // ---- ScanCache ----

    #[test]
    fn cache_hit_does_not_scan() {
        let cache = ScanCache::new();
        cache.store(&[info("a"), info("b")]);
        let found = cache.find("b", || panic!("must not scan on a hit"));
        assert_eq!(found.unwrap().library_path, "/x/b.clap");
    }

    #[test]
    fn cache_miss_scans_once_then_rate_limits() {
        let cache = ScanCache::new();
        let mut scans = 0;
        assert!(cache.find("c", || {
            scans += 1;
            cache_store_for_test(&cache, &[info("c")]);
        })
        .is_some());
        assert_eq!(scans, 1);
        // A recent scan: a miss for an unknown URI does not rescan.
        assert!(cache.find("missing", || panic!("rate limited")).is_none());
    }

    fn cache_store_for_test(cache: &ScanCache, plugins: &[PluginInfo]) {
        cache.store(plugins);
    }

    // ---- category_from_features ----

    #[test]
    fn category_reverb() {
        assert_eq!(category_from_features(&[s("reverb")]), PluginCategory::Reverb);
    }

    #[test]
    fn category_delay() {
        assert_eq!(category_from_features(&[s("delay")]), PluginCategory::Delay);
    }

    #[test]
    fn category_distortion() {
        assert_eq!(category_from_features(&[s("distortion")]), PluginCategory::Distortion);
    }

    #[test]
    fn category_compressor() {
        assert_eq!(category_from_features(&[s("compressor")]), PluginCategory::Compressor);
    }

    #[test]
    fn category_limiter() {
        assert_eq!(category_from_features(&[s("limiter")]), PluginCategory::Limiter);
    }

    #[test]
    fn category_eq_variants() {
        assert_eq!(category_from_features(&[s("equalizer")]), PluginCategory::Equaliser);
        assert_eq!(category_from_features(&[s("eq")]), PluginCategory::Equaliser);
    }

    #[test]
    fn category_filter() {
        assert_eq!(category_from_features(&[s("filter")]), PluginCategory::Filter);
    }

    #[test]
    fn category_chorus_variants() {
        assert_eq!(category_from_features(&[s("chorus")]), PluginCategory::Chorus);
        assert_eq!(category_from_features(&[s("flanger")]), PluginCategory::Chorus);
        assert_eq!(category_from_features(&[s("phaser")]), PluginCategory::Chorus);
    }

    #[test]
    fn category_amplifier() {
        assert_eq!(category_from_features(&[s("amplifier")]), PluginCategory::Amplifier);
    }

    #[test]
    fn category_mixer() {
        assert_eq!(category_from_features(&[s("mixer")]), PluginCategory::Mixer);
    }

    #[test]
    fn category_instrument_variants() {
        assert_eq!(category_from_features(&[s("instrument")]), PluginCategory::Instrument);
        assert_eq!(category_from_features(&[s("synthesizer")]), PluginCategory::Instrument);
        assert_eq!(category_from_features(&[s("sampler")]), PluginCategory::Instrument);
    }

    #[test]
    fn category_analyzer() {
        assert_eq!(category_from_features(&[s("analyzer")]), PluginCategory::Analyser);
    }

    #[test]
    fn category_utility() {
        assert_eq!(category_from_features(&[s("utility")]), PluginCategory::Utility);
    }

    #[test]
    fn category_spatial_variants() {
        assert_eq!(category_from_features(&[s("spatial")]), PluginCategory::Spatial);
        assert_eq!(category_from_features(&[s("surround")]), PluginCategory::Spatial);
    }

    #[test]
    fn category_generator() {
        assert_eq!(category_from_features(&[s("generator")]), PluginCategory::Generator);
    }

    #[test]
    fn category_audio_effect_fallback() {
        // "audio-effect" without specific category → Filter
        assert_eq!(category_from_features(&[s("audio-effect")]), PluginCategory::Filter);
    }

    #[test]
    fn category_instrument_broad_fallback() {
        // "instrument" as a broad category feature (case-sensitive match)
        assert_eq!(category_from_features(&[s("instrument")]), PluginCategory::Instrument);
    }

    #[test]
    fn category_unknown_fallback() {
        assert_eq!(
            category_from_features(&[s("something-unknown")]),
            PluginCategory::Other("CLAP".to_string()),
        );
    }

    #[test]
    fn category_empty_features() {
        assert_eq!(
            category_from_features(&[]),
            PluginCategory::Other("CLAP".to_string()),
        );
    }

    #[test]
    fn category_first_recognized_wins() {
        assert_eq!(
            category_from_features(&[s("audio-effect"), s("reverb"), s("delay")]),
            PluginCategory::Reverb,
        );
    }

    #[test]
    fn category_case_insensitive() {
        assert_eq!(category_from_features(&[s("REVERB")]), PluginCategory::Reverb);
        assert_eq!(category_from_features(&[s("Delay")]), PluginCategory::Delay);
    }

    fn s(v: &str) -> String {
        v.to_string()
    }
}
