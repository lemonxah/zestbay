//! VST2 discovery. Library inspection and shell enumeration run only in
//! isolated scan processes, since metadata queries execute plugin code.

use super::abi::{self, eff, *};
use super::host::LoadedEffect;
use crate::clap::scanner::ScanCache;
use crate::plugin::types::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SEARCH_DIRS: &[&str] = &[
    "~/.vst",
    "~/.vst2",
    "/usr/lib/vst",
    "/usr/local/lib/vst",
    "/usr/lib64/vst",
    "/usr/local/lib64/vst",
    "/usr/lib/vst2",
    "/usr/local/lib/vst2",
    "/usr/lib64/vst2",
    "/usr/local/lib64/vst2",
];

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
    let dirs = crate::plugin::paths::search_dirs("VST_PATH", SEARCH_DIRS, "vst", "VST2");
    crate::plugin::scan::discover(&dirs, "so", true)
}

pub(crate) fn scan_discovered(paths: &[PathBuf], progress: &(impl Fn() + Sync)) -> Vec<PluginInfo> {
    let mut plugins = crate::plugin::scan::scan_libraries("vst2", paths, progress);
    plugins.sort_by_cached_key(|p| p.name.to_lowercase());
    // A library can appear in both a system and a user directory. Prefer
    // the first search root, consistently with the other format scanners.
    let mut seen = HashSet::new();
    plugins.retain(|p| seen.insert(p.uri.clone()));
    CACHE.store(&plugins);
    log::info!("VST2: found {} plugins total", plugins.len());
    plugins
}

/// The filename disambiguates plugins that reuse the 32-bit VST2 ID while
/// keeping sessions portable when a library is moved to a different folder.
fn plugin_uri(path: &Path, id: i32) -> String {
    format!(
        "vst2:{:08x}:{}",
        id as u32,
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

pub(super) fn parse_id(uri: &str) -> Option<i32> {
    let rest = uri.strip_prefix("vst2:")?;
    let (id, filename) = rest.split_once(':')?;
    if id.len() != 8 || filename.is_empty() {
        return None;
    }
    u32::from_str_radix(id, 16).ok().map(|id| id as i32)
}

pub(crate) fn scan_vst2_file(path: &Path, plugins: &mut Vec<PluginInfo>) {
    unsafe {
        let Some(root) = LoadedEffect::open(path, 0, 48000.0) else {
            return;
        };
        if command(root.raw, eff::CATEGORY, 0) == 10 {
            let mut ids = HashSet::new();
            let mut children = Vec::new();
            for _ in 0..4096 {
                let mut name = [0u8; 256];
                let id = dispatch(
                    root.raw,
                    eff::SHELL_NEXT,
                    0,
                    0,
                    name.as_mut_ptr().cast(),
                    0.0,
                ) as i32;
                if id == 0 || !ids.insert(id) {
                    break;
                }
                children.push(id);
            }
            drop(root);
            for id in children {
                if let Some(child) = LoadedEffect::open(path, id, 48000.0)
                    && (*child.raw).unique_id == id
                {
                    plugins.push(describe(path, &child));
                }
            }
        } else {
            plugins.push(describe(path, &root));
        }
    }
}

fn describe(path: &Path, loaded: &LoadedEffect) -> PluginInfo {
    unsafe {
        let raw = loaded.raw;
        let mut name = abi::text(raw, eff::NAME, 0);
        if name.is_empty() {
            name = abi::text(raw, eff::PRODUCT, 0);
        }
        if name.is_empty() {
            name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        let vendor = abi::text(raw, eff::VENDOR, 0);
        let inputs = (*raw).num_inputs as usize;
        let outputs = (*raw).num_outputs as usize;
        let params = loaded.parameters();
        let mut ports = Vec::new();
        for (count, kind, prefix) in [
            (inputs, PluginPortType::AudioInput, "input"),
            (outputs, PluginPortType::AudioOutput, "output"),
        ] {
            for i in 0..count {
                ports.push(PluginPortInfo {
                    index: ports.len(),
                    symbol: format!("{prefix}_{i}"),
                    name: format!("{prefix} {}", i + 1),
                    port_type: kind,
                    default_value: 0.0,
                    min_value: 0.0,
                    max_value: 1.0,
                    is_toggle: false,
                    step_count: 0,
                    is_logarithmic: false,
                });
            }
        }
        for p in &params {
            ports.push(PluginPortInfo {
                index: p.port_index,
                symbol: format!("param_{}", p.port_index),
                name: p.name.clone(),
                port_type: PluginPortType::ControlInput,
                default_value: p.default,
                min_value: 0.0,
                max_value: 1.0,
                is_toggle: p.is_toggle,
                step_count: p.step_count,
                is_logarithmic: false,
            });
        }
        let category = match command(raw, eff::CATEGORY, 0) {
            2 => PluginCategory::Instrument,
            3 => PluginCategory::Analyser,
            5 | 7 => PluginCategory::Spatial,
            6 => PluginCategory::Reverb,
            11 => PluginCategory::Generator,
            _ if (*raw).flags & IS_SYNTH != 0 => PluginCategory::Instrument,
            _ => PluginCategory::Other("Effect".into()),
        };
        PluginInfo {
            uri: plugin_uri(path, (*raw).unique_id),
            name,
            format: PluginFormat::Vst2,
            category,
            author: (!vendor.is_empty()).then_some(vendor),
            ports,
            audio_inputs: inputs,
            audio_outputs: outputs,
            control_inputs: params.len(),
            control_outputs: 0,
            required_features: Vec::new(),
            compatible: loaded.can_process(),
            has_ui: (*raw).flags & HAS_EDITOR != 0,
            library_path: path.to_string_lossy().into_owned(),
        }
    }
}
