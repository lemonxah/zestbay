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
    CLAP_SEARCH_DIRS
        .iter()
        .map(|d| {
            if d.starts_with('~') {
                PathBuf::from(d.replacen('~', &home, 1))
            } else {
                PathBuf::from(d)
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
                scan_clap_file(&path, plugins);
            } else {
                scan_directory(&path, plugins);
            }
        } else if path.extension().is_some_and(|e| e == "clap") {
            scan_clap_file(&path, plugins);
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

            let (audio_inputs, audio_outputs, control_inputs, has_ui, ports) =
                probe_plugin_ports(factory, &id);

            plugins.push(PluginInfo {
                uri: id,
                name,
                format: PluginFormat::Clap,
                category,
                author: vendor,
                ports,
                audio_inputs,
                audio_outputs,
                control_inputs,
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

/// Briefly instantiate a plugin to query its audio ports and parameters.
fn probe_plugin_ports(
    factory: &clap_sys::factory::plugin_factory::clap_plugin_factory,
    plugin_id: &str,
) -> (usize, usize, usize, bool, Vec<PluginPortInfo>) { unsafe {
    let c_id = match CString::new(plugin_id) {
        Ok(s) => s,
        Err(_) => return (0, 0, 0, false, Vec::new()),
    };

    let host = clap_sys::host::clap_host {
        clap_version: clap_sys::version::clap_version {
            major: 1,
            minor: 2,
            revision: 2,
        },
        host_data: std::ptr::null_mut(),
        name: c"ZestBay".as_ptr(),
        vendor: c"ZestBay".as_ptr(),
        url: c"https://github.com/lemonxah/zestbay".as_ptr(),
        version: c"0.1.0".as_ptr(),
        get_extension: Some(host_get_extension_noop),
        request_restart: Some(host_noop),
        request_process: Some(host_noop),
        request_callback: Some(host_noop),
    };

    let create = match factory.create_plugin {
        Some(f) => f,
        None => return (0, 0, 0, false, Vec::new()),
    };

    let plugin_ptr = create(factory, &host, c_id.as_ptr());
    if plugin_ptr.is_null() {
        return (0, 0, 0, false, Vec::new());
    }

    let plugin = &*plugin_ptr;

    let init_ok = match plugin.init {
        Some(f) => f(plugin_ptr),
        None => false,
    };
    if !init_ok {
        if let Some(destroy) = plugin.destroy {
            destroy(plugin_ptr);
        }
        return (0, 0, 0, false, Vec::new());
    }

    let mut audio_inputs = 0usize;
    let mut audio_outputs = 0usize;
    let mut control_inputs = 0usize;
    let mut has_ui = false;
    let mut ports = Vec::new();

    if let Some(get_ext) = plugin.get_extension {
        // Audio ports
        let ext = get_ext(
            plugin_ptr,
            clap_sys::ext::audio_ports::CLAP_EXT_AUDIO_PORTS.as_ptr(),
        );
        if !ext.is_null() {
            let ap = &*(ext as *const clap_sys::ext::audio_ports::clap_plugin_audio_ports);
            if let Some(count_fn) = ap.count {
                let in_count = count_fn(plugin_ptr, true);
                for idx in 0..in_count {
                    let mut info: clap_sys::ext::audio_ports::clap_audio_port_info =
                        std::mem::zeroed();
                    if let Some(get_fn) = ap.get {
                        if get_fn(plugin_ptr, idx, true, &mut info) {
                            let ch = info.channel_count as usize;
                            audio_inputs += ch;
                            for c in 0..ch {
                                let pname = read_clap_name(&info.name);
                                let suffix = if ch > 1 {
                                    format!(" ch{}", c + 1)
                                } else {
                                    String::new()
                                };
                                ports.push(PluginPortInfo {
                                    index: ports.len(),
                                    symbol: format!("audio_in_{}_{}", idx, c),
                                    name: format!("{}{}", pname, suffix),
                                    port_type: PluginPortType::AudioInput,
                                    default_value: 0.0,
                                    min_value: 0.0,
                                    max_value: 0.0,
                                });
                            }
                        }
                    }
                }

                let out_count = count_fn(plugin_ptr, false);
                for idx in 0..out_count {
                    let mut info: clap_sys::ext::audio_ports::clap_audio_port_info =
                        std::mem::zeroed();
                    if let Some(get_fn) = ap.get {
                        if get_fn(plugin_ptr, idx, false, &mut info) {
                            let ch = info.channel_count as usize;
                            audio_outputs += ch;
                            for c in 0..ch {
                                let pname = read_clap_name(&info.name);
                                let suffix = if ch > 1 {
                                    format!(" ch{}", c + 1)
                                } else {
                                    String::new()
                                };
                                ports.push(PluginPortInfo {
                                    index: ports.len(),
                                    symbol: format!("audio_out_{}_{}", idx, c),
                                    name: format!("{}{}", pname, suffix),
                                    port_type: PluginPortType::AudioOutput,
                                    default_value: 0.0,
                                    min_value: 0.0,
                                    max_value: 0.0,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Params
        let pext = get_ext(
            plugin_ptr,
            clap_sys::ext::params::CLAP_EXT_PARAMS.as_ptr(),
        );
        if !pext.is_null() {
            let pe = &*(pext as *const clap_sys::ext::params::clap_plugin_params);
            if let Some(count_fn) = pe.count {
                let n = count_fn(plugin_ptr);
                for idx in 0..n {
                    let mut info: clap_sys::ext::params::clap_param_info = std::mem::zeroed();
                    if let Some(get_info) = pe.get_info {
                        if get_info(plugin_ptr, idx, &mut info) {
                            let hidden =
                                info.flags & clap_sys::ext::params::CLAP_PARAM_IS_HIDDEN != 0;
                            let readonly =
                                info.flags & clap_sys::ext::params::CLAP_PARAM_IS_READONLY != 0;
                            if !hidden {
                                let name = read_clap_name(&info.name);
                                let pt = if readonly {
                                    PluginPortType::ControlOutput
                                } else {
                                    control_inputs += 1;
                                    PluginPortType::ControlInput
                                };
                                ports.push(PluginPortInfo {
                                    index: ports.len(),
                                    symbol: format!("param_{}", info.id),
                                    name,
                                    port_type: pt,
                                    default_value: info.default_value as f32,
                                    min_value: info.min_value as f32,
                                    max_value: info.max_value as f32,
                                });
                            }
                        }
                    }
                }
            }
        }

        // GUI
        let gui_ext = get_ext(plugin_ptr, clap_sys::ext::gui::CLAP_EXT_GUI.as_ptr());
        has_ui = !gui_ext.is_null();
    }

    if let Some(destroy) = plugin.destroy {
        destroy(plugin_ptr);
    }

    (audio_inputs, audio_outputs, control_inputs, has_ui, ports)
}}

fn read_clap_name(name: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = name
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| "?".to_string())
}

unsafe extern "C" fn host_get_extension_noop(
    _host: *const clap_sys::host::clap_host,
    _extension_id: *const std::ffi::c_char,
) -> *const std::ffi::c_void {
    std::ptr::null()
}

unsafe extern "C" fn host_noop(_host: *const clap_sys::host::clap_host) {}
