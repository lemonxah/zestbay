//! CLAP plugin scanner.
//!
//! Scans standard directories for `.clap` bundles, dlopen's each one,
//! and enumerates the plugins within via the CLAP plugin factory.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use crate::plugin::types::*;

/// Standard Linux directories where CLAP plugins are installed.
const CLAP_SEARCH_DIRS: &[&str] = &[
    "~/.clap",
    "/usr/lib/clap",
    "/usr/local/lib/clap",
    "/usr/lib64/clap",
    "/usr/local/lib64/clap",
];

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


