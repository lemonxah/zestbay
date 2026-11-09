//! LADSPA discovery. Library inspection runs only in isolated scan
//! processes, since loading a library executes plugin code.

use super::abi::{self, *};
use crate::clap::scanner::ScanCache;
use crate::plugin::types::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SEARCH_DIRS: &[&str] = &[
    "~/.ladspa",
    "/usr/lib/ladspa",
    "/usr/local/lib/ladspa",
    "/usr/lib64/ladspa",
    "/usr/local/lib64/ladspa",
];

/// Sample rate used to resolve rate-dependent port ranges in the catalog.
const SCAN_RATE: f64 = 48000.0;

static CACHE: ScanCache = ScanCache::new();

pub fn cached_plugin(uri: &str) -> Option<PluginInfo> {
    CACHE.get(uri)
}

pub fn find_plugin(uri: &str) -> Option<PluginInfo> {
    CACHE.find(uri, || {
        scan_discovered(&discover_plugins(), &|| {});
    })
}

pub(crate) fn discover_plugins() -> Vec<PathBuf> {
    let dirs = crate::plugin::paths::search_dirs("LADSPA_PATH", SEARCH_DIRS, "ladspa", "LADSPA");
    crate::plugin::scan::discover(&dirs, "so", true)
}

pub(crate) fn scan_discovered(paths: &[PathBuf], progress: &(impl Fn() + Sync)) -> Vec<PluginInfo> {
    let mut plugins = crate::plugin::scan::scan_libraries("ladspa", paths, progress);
    plugins.sort_by_cached_key(|p| p.name.to_lowercase());
    // Prefer the first search root when a library is installed twice.
    let mut seen = HashSet::new();
    plugins.retain(|p| seen.insert(p.uri.clone()));
    CACHE.store(&plugins);
    log::info!("LADSPA: found {} plugins total", plugins.len());
    plugins
}

/// `ladspa:<file name>:<label>`. Labels are unique within a library (unique
/// IDs are not always), and the file name keeps sessions valid when a
/// library moves between search directories.
pub(super) fn plugin_uri(path: &Path, label: &str) -> String {
    format!(
        "ladspa:{}:{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        label
    )
}

/// The label part of a URI made by [`plugin_uri`].
pub(super) fn parse_label(uri: &str) -> Option<&str> {
    let (file, label) = uri.strip_prefix("ladspa:")?.split_once(':')?;
    (!file.is_empty() && !label.is_empty()).then_some(label)
}

pub(crate) fn scan_ladspa_file(path: &Path, plugins: &mut Vec<PluginInfo>) {
    unsafe {
        let Some(lib) = Library::open(path) else {
            return;
        };
        for index in 0..MAX_DESCRIPTORS {
            let desc = lib.descriptor(index);
            if desc.is_null() {
                break;
            }
            if let Some(info) = describe(path, &*desc) {
                plugins.push(info);
            }
        }
        // Keep the library loaded: this process exits right after reporting,
        // and unloading plugin code buys nothing.
        std::mem::forget(lib);
    }
}

/// `None` for descriptors the host cannot use.
unsafe fn describe(path: &Path, desc: &Descriptor) -> Option<PluginInfo> {
    unsafe {
        let label = abi::cstr(desc.label);
        let port_count = desc.port_count as usize;
        if label.is_empty()
            || port_count > MAX_PORTS
            || desc.instantiate.is_none()
            || desc.connect_port.is_none()
            || desc.run.is_none()
            || (port_count > 0
                && (desc.port_descriptors.is_null()
                    || desc.port_names.is_null()
                    || desc.port_range_hints.is_null()))
        {
            return None;
        }
        let mut name = abi::cstr(desc.name);
        if name.is_empty() {
            name = label.clone();
        }
        let maker = abi::cstr(desc.maker);

        let mut ports = Vec::new();
        let (mut ai, mut ao, mut ci, mut co) = (0, 0, 0, 0);
        for i in 0..port_count {
            let flags = *desc.port_descriptors.add(i);
            let mut port_name = abi::cstr(*desc.port_names.add(i));
            if port_name.is_empty() {
                port_name = format!("Port {}", i + 1);
            }
            let input = flags & PORT_INPUT != 0;
            let output = flags & PORT_OUTPUT != 0;
            let audio = flags & PORT_AUDIO != 0;
            let control = flags & PORT_CONTROL != 0;
            if input == output || audio == control {
                // Not a valid port; skip it (the host connects it to scratch).
                continue;
            }
            let (port_type, symbol) = match (audio, input) {
                (true, true) => {
                    ai += 1;
                    (PluginPortType::AudioInput, format!("audio_in_{}", ai - 1))
                }
                (true, false) => {
                    ao += 1;
                    (PluginPortType::AudioOutput, format!("audio_out_{}", ao - 1))
                }
                (false, true) => {
                    ci += 1;
                    (PluginPortType::ControlInput, format!("port_{i}"))
                }
                (false, false) => {
                    co += 1;
                    (PluginPortType::ControlOutput, format!("port_{i}"))
                }
            };
            let meta = if control {
                control_meta(*desc.port_range_hints.add(i), SCAN_RATE)
            } else {
                ControlMeta { min: -1.0, max: 1.0, default: 0.0, toggled: false, logarithmic: false, steps: 0 }
            };
            ports.push(PluginPortInfo {
                index: i,
                symbol,
                name: port_name,
                port_type,
                default_value: meta.default,
                min_value: meta.min,
                max_value: meta.max,
                is_toggle: meta.toggled,
                step_count: meta.steps,
                is_logarithmic: meta.logarithmic,
            });
        }

        let category = if ai == 0 && ao > 0 {
            PluginCategory::Generator
        } else if ao == 0 && ai > 0 {
            PluginCategory::Analyser
        } else {
            PluginCategory::from_class_label(&name)
        };
        Some(PluginInfo {
            uri: plugin_uri(path, &label),
            name,
            format: PluginFormat::Ladspa,
            category,
            author: (!maker.is_empty()).then_some(maker),
            ports,
            audio_inputs: ai,
            audio_outputs: ao,
            control_inputs: ci,
            control_outputs: co,
            required_features: Vec::new(),
            compatible: ai + ao > 0,
            has_ui: false,
            library_path: path.to_string_lossy().into_owned(),
        })
    }
}
