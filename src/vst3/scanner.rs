//! VST3 plugin scanner.
//!
//! Scans standard directories for `.vst3` bundles, dlopen's the shared
//! library inside each bundle, and enumerates plugins via `IPluginFactory`.

use std::ffi::CString;
use std::path::{Path, PathBuf};

use vst3::Steinberg::*;

use crate::plugin::types::*;

/// Standard Linux directories where VST3 plugins are installed.
const VST3_SEARCH_DIRS: &[&str] = &[
    "~/.vst3",
    "/usr/lib/vst3",
    "/usr/local/lib/vst3",
    "/usr/lib64/vst3",
    "/usr/local/lib64/vst3",
];

/// Scan all standard directories and return a list of VST3 plugin infos.
pub fn scan_plugins() -> Vec<PluginInfo> {
    let mut plugins = Vec::new();
    let dirs = expand_search_dirs();

    for dir in &dirs {
        if !dir.is_dir() {
            continue;
        }
        log::info!("VST3: scanning {}", dir.display());
        scan_directory(dir, &mut plugins);
    }

    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    log::info!("VST3: found {} plugins total", plugins.len());
    plugins
}

fn expand_search_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    VST3_SEARCH_DIRS
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
                log::debug!("VST3: skipping duplicate directory {}", path.display());
                None
            }
        })
        .collect()
}

fn scan_directory(dir: &Path, plugins: &mut Vec<PluginInfo>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::debug!("VST3: cannot read {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.extension().is_some_and(|e| e == "vst3") {
                sandboxed_scan_vst3_bundle(&path, plugins);
            } else {
                scan_directory(&path, plugins);
            }
        }
    }
}

fn sandboxed_scan_vst3_bundle(bundle_path: &Path, plugins: &mut Vec<PluginInfo>) {
    use crate::plugin::sandbox::{SandboxResult, fork_scan};

    let path_owned = bundle_path.to_path_buf();
    let timeout = std::time::Duration::from_secs(10);

    let result: SandboxResult<Vec<PluginInfo>> = fork_scan(
        move || {
            let mut found = Vec::new();
            scan_vst3_bundle(&path_owned, &mut found);
            found
        },
        Some(timeout),
    );

    match result {
        SandboxResult::Ok(found) => {
            log::debug!("VST3 sandbox: {} scanned OK ({} plugins)", bundle_path.display(), found.len());
            plugins.extend(found);
        }
        SandboxResult::Crashed { signal, description } => {
            log::warn!(
                "VST3 sandbox: {} crashed during scan (signal {:?}): {}",
                bundle_path.display(), signal, description
            );
        }
        SandboxResult::Timeout => {
            log::warn!("VST3 sandbox: {} timed out during scan", bundle_path.display());
        }
        SandboxResult::ForkFailed(e) => {
            log::error!("VST3 sandbox: fork failed for {}: {}", bundle_path.display(), e);
            scan_vst3_bundle(bundle_path, plugins);
        }
    }
}

/// Resolve the shared library path inside a .vst3 bundle.
/// VST3 bundle structure: `<name>.vst3/Contents/<arch>/<name>.so`
pub fn find_bundle_binary(bundle_path: &Path) -> Option<PathBuf> {
    // Try standard architecture directory names
    let arch_dirs = [
        "x86_64-linux",
        "i386-linux",
        "aarch64-linux",
        "armv7l-linux",
    ];

    for arch in &arch_dirs {
        let contents_dir = bundle_path.join("Contents").join(arch);
        if contents_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&contents_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e == "so") {
                        return Some(p);
                    }
                }
            }
        }
    }

    // Fallback: some bundles put the .so directly in the bundle root
    if let Ok(entries) = std::fs::read_dir(bundle_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "so") {
                return Some(p);
            }
        }
    }

    None
}

fn scan_vst3_bundle(bundle_path: &Path, plugins: &mut Vec<PluginInfo>) {
    let so_path = match find_bundle_binary(bundle_path) {
        Some(p) => p,
        None => {
            log::debug!("VST3: no .so found in {}", bundle_path.display());
            return;
        }
    };

    let so_str = match so_path.to_str() {
        Some(s) => s,
        None => return,
    };

    let bundle_str = match bundle_path.to_str() {
        Some(s) => s,
        None => return,
    };

    log::info!("VST3: loading bundle {}", bundle_path.display());

    let c_path = match CString::new(so_str) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Safety: All VST3 interactions use raw C FFI via dlopen.
    unsafe {
        let lib = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            let err = std::ffi::CStr::from_ptr(libc::dlerror());
            log::debug!("VST3: dlopen failed for {}: {:?}", so_str, err);
            return;
        }

        // Call ModuleEntry (Linux VST3 requirement)
        let module_entry_sym = libc::dlsym(lib, c"ModuleEntry".as_ptr());
        if !module_entry_sym.is_null() {
            let module_entry: unsafe extern "system" fn(*mut std::ffi::c_void) -> bool =
                std::mem::transmute(module_entry_sym);
            if !module_entry(lib) {
                log::debug!("VST3: ModuleEntry returned false for {}", so_str);
                libc::dlclose(lib);
                return;
            }
        }

        // Get the factory
        let get_factory_sym = libc::dlsym(lib, c"GetPluginFactory".as_ptr());
        if get_factory_sym.is_null() {
            log::debug!("VST3: no GetPluginFactory in {}", so_str);
            call_module_exit(lib);
            libc::dlclose(lib);
            return;
        }

        let get_factory: unsafe extern "system" fn() -> *mut IPluginFactory =
            std::mem::transmute(get_factory_sym);
        let factory_raw = get_factory();
        if factory_raw.is_null() {
            log::debug!("VST3: GetPluginFactory returned null for {}", so_str);
            call_module_exit(lib);
            libc::dlclose(lib);
            return;
        }

        let factory = match vst3::ComPtr::<IPluginFactory>::from_raw(factory_raw) {
            Some(f) => f,
            None => {
                call_module_exit(lib);
                libc::dlclose(lib);
                return;
            }
        };

        // Try to get IPluginFactory2 for richer info
        let factory2: Option<vst3::ComPtr<IPluginFactory2>> = factory.cast();

        let count = factory.countClasses();

        for i in 0..count {
            let mut info: PClassInfo = std::mem::zeroed();
            if factory.getClassInfo(i, &mut info) != kResultOk {
                continue;
            }

            // Only interested in Audio Module Class (processor components)
            let category_str = read_cstr(&info.category);
            if category_str != "Audio Module Class" {
                continue;
            }

            let name = read_cstr(&info.name);
            let uri = tuid_to_hex(&info.cid);

            // Try to get extended info (vendor, subcategories) from IPluginFactory2
            let mut vendor: Option<String> = None;
            let mut sub_categories = String::new();

            if let Some(ref f2) = factory2 {
                let mut info2: PClassInfo2 = std::mem::zeroed();
                if f2.getClassInfo2(i, &mut info2) == kResultOk {
                    let v = read_cstr(&info2.vendor);
                    if !v.is_empty() {
                        vendor = Some(v);
                    }
                    sub_categories = read_cstr(&info2.subCategories);
                }
            }

            let plugin_category = category_from_subcategories(&sub_categories);

            // Infer audio bus layout from subcategories.
            // We do NOT instantiate the component during scanning because
            // many plugins (synths, amp sims) load heavy resources during
            // IComponent::initialize() which can OOM-kill the process.
            // Accurate bus info is discovered at instantiation time in host.rs.
            let is_instrument = sub_categories.contains("Instrument")
                || plugin_category == PluginCategory::Instrument;
            let (audio_inputs, audio_outputs) = if is_instrument {
                (0, 2) // Instruments: no audio input, stereo output
            } else {
                (2, 2) // Effects: stereo in/out (default assumption)
            };

            plugins.push(PluginInfo {
                uri,
                name,
                format: PluginFormat::Vst3,
                category: plugin_category,
                author: vendor,
                ports: Vec::new(), // Populated at instantiation time
                audio_inputs,
                audio_outputs,
                control_inputs: 0,
                control_outputs: 0,
                required_features: Vec::new(),
                compatible: true,
                // VST3 has no metadata-level flag for GUI support, and
                // createView() requires instantiation (which we avoid during
                // scanning to prevent OOM).  Nearly all VST3 plugins ship
                // with a GUI, so default to true.  The host can refine this
                // at instantiation time via IEditController::createView().
                has_ui: true,
                library_path: bundle_str.to_string(),
            });
        }

        // Release the factory before module exit
        drop(factory2);
        drop(factory);

        call_module_exit(lib);
        // Do NOT dlclose — plugin descriptor strings may live inside the .so
    }
}

/// Call ModuleExit if available.
unsafe fn call_module_exit(lib: *mut std::ffi::c_void) {
    unsafe {
        let sym = libc::dlsym(lib, c"ModuleExit".as_ptr());
        if !sym.is_null() {
            let module_exit: unsafe extern "system" fn() -> bool = std::mem::transmute(sym);
            module_exit();
        }
    }
}

/// Convert a TUID ([c_char; 16]) to a hex string for use as the plugin URI.
fn tuid_to_hex(tuid: &[std::ffi::c_char; 16]) -> String {
    tuid.iter()
        .map(|&b| format!("{:02X}", b as u8))
        .collect()
}

/// Parse a TUID from a hex string.
pub fn hex_to_tuid(hex: &str) -> Option<[std::ffi::c_char; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut tuid = [0i8; 16];
    for i in 0..16 {
        let byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        tuid[i] = byte as i8;
    }
    Some(tuid)
}

/// Read a null-terminated C string from a fixed-size char array.
fn read_cstr(buf: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| "?".to_string())
}

/// Map VST3 pipe-delimited subcategory string to our PluginCategory.
fn category_from_subcategories(sub: &str) -> PluginCategory {
    let parts: Vec<&str> = sub.split('|').map(|s| s.trim()).collect();

    for part in &parts {
        let p = part.to_lowercase();
        match p.as_str() {
            "reverb" => return PluginCategory::Reverb,
            "delay" => return PluginCategory::Delay,
            "distortion" => return PluginCategory::Distortion,
            "dynamics" => return PluginCategory::Dynamics,
            "eq" => return PluginCategory::Equaliser,
            "filter" => return PluginCategory::Filter,
            "chorus" | "flanger" | "phaser" | "modulation" => return PluginCategory::Chorus,
            "compressor" => return PluginCategory::Compressor,
            "limiter" => return PluginCategory::Limiter,
            "amplifier" | "amp" => return PluginCategory::Amplifier,
            "mixer" => return PluginCategory::Mixer,
            "instrument" | "synth" | "sampler" | "drum" => return PluginCategory::Instrument,
            "analyzer" => return PluginCategory::Analyser,
            "spatial" | "surround" => return PluginCategory::Spatial,
            "generator" => return PluginCategory::Generator,
            "utility" | "tools" => return PluginCategory::Utility,
            _ => {}
        }
    }

    // Broad categories
    if parts.iter().any(|&p| p == "Fx") {
        return PluginCategory::Filter;
    }
    if parts.iter().any(|&p| p == "Instrument") {
        return PluginCategory::Instrument;
    }

    PluginCategory::Other("VST3".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- hex_to_tuid / tuid_to_hex round-trip ----

    #[test]
    fn hex_to_tuid_valid() {
        let hex = "0123456789ABCDEF0123456789ABCDEF";
        let tuid = hex_to_tuid(hex).expect("valid 32-char hex should parse");
        assert_eq!(tuid.len(), 16);
        assert_eq!(tuid[0], 0x01_u8 as i8);
        assert_eq!(tuid[1], 0x23_u8 as i8);
        assert_eq!(tuid[15], 0xEF_u8 as i8);
    }

    #[test]
    fn hex_to_tuid_roundtrip() {
        let hex = "DEADBEEF01234567CAFEBABE89ABCDEF";
        let tuid = hex_to_tuid(hex).unwrap();
        let back = tuid_to_hex(&tuid);
        assert_eq!(back, hex);
    }

    #[test]
    fn hex_to_tuid_lowercase() {
        let hex = "deadbeef01234567cafebabe89abcdef";
        let tuid = hex_to_tuid(hex).unwrap();
        let back = tuid_to_hex(&tuid);
        // tuid_to_hex produces uppercase
        assert_eq!(back, hex.to_uppercase());
    }

    #[test]
    fn hex_to_tuid_wrong_length() {
        assert!(hex_to_tuid("").is_none());
        assert!(hex_to_tuid("0123456789ABCDEF").is_none()); // 16 chars (too short)
        assert!(hex_to_tuid("0123456789ABCDEF0123456789ABCDEF00").is_none()); // 34 chars (too long)
    }

    #[test]
    fn hex_to_tuid_invalid_hex_chars() {
        assert!(hex_to_tuid("ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ").is_none());
        assert!(hex_to_tuid("0123456789ABCDEF0123456789ABCDEG").is_none());
    }

    #[test]
    fn tuid_to_hex_all_zeros() {
        let tuid = [0i8; 16];
        assert_eq!(tuid_to_hex(&tuid), "00000000000000000000000000000000");
    }

    #[test]
    fn tuid_to_hex_all_ff() {
        let tuid = [-1i8; 16]; // 0xFF as i8 = -1
        assert_eq!(tuid_to_hex(&tuid), "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF");
    }

    // ---- read_cstr ----

    #[test]
    fn read_cstr_basic() {
        let buf: Vec<std::ffi::c_char> = b"Hello\0World"
            .iter()
            .map(|&b| b as std::ffi::c_char)
            .collect();
        assert_eq!(read_cstr(&buf), "Hello");
    }

    #[test]
    fn read_cstr_empty() {
        let buf: Vec<std::ffi::c_char> = vec![0];
        assert_eq!(read_cstr(&buf), "");
    }

    #[test]
    fn read_cstr_no_null_terminator() {
        let buf: Vec<std::ffi::c_char> = b"NoNull"
            .iter()
            .map(|&b| b as std::ffi::c_char)
            .collect();
        assert_eq!(read_cstr(&buf), "NoNull");
    }

    // ---- category_from_subcategories ----

    #[test]
    fn category_reverb() {
        assert_eq!(category_from_subcategories("Fx|Reverb"), PluginCategory::Reverb);
    }

    #[test]
    fn category_delay() {
        assert_eq!(category_from_subcategories("Fx|Delay"), PluginCategory::Delay);
    }

    #[test]
    fn category_distortion() {
        assert_eq!(category_from_subcategories("Fx|Distortion"), PluginCategory::Distortion);
    }

    #[test]
    fn category_dynamics() {
        assert_eq!(category_from_subcategories("Fx|Dynamics"), PluginCategory::Dynamics);
    }

    #[test]
    fn category_eq() {
        assert_eq!(category_from_subcategories("Fx|EQ"), PluginCategory::Equaliser);
    }

    #[test]
    fn category_filter() {
        assert_eq!(category_from_subcategories("Fx|Filter"), PluginCategory::Filter);
    }

    #[test]
    fn category_chorus_variants() {
        assert_eq!(category_from_subcategories("Fx|Chorus"), PluginCategory::Chorus);
        assert_eq!(category_from_subcategories("Fx|Flanger"), PluginCategory::Chorus);
        assert_eq!(category_from_subcategories("Fx|Phaser"), PluginCategory::Chorus);
        assert_eq!(category_from_subcategories("Fx|Modulation"), PluginCategory::Chorus);
    }

    #[test]
    fn category_compressor() {
        assert_eq!(category_from_subcategories("Fx|Compressor"), PluginCategory::Compressor);
    }

    #[test]
    fn category_limiter() {
        assert_eq!(category_from_subcategories("Fx|Limiter"), PluginCategory::Limiter);
    }

    #[test]
    fn category_amplifier() {
        assert_eq!(category_from_subcategories("Fx|Amplifier"), PluginCategory::Amplifier);
        assert_eq!(category_from_subcategories("Fx|Amp"), PluginCategory::Amplifier);
    }

    #[test]
    fn category_mixer() {
        assert_eq!(category_from_subcategories("Fx|Mixer"), PluginCategory::Mixer);
    }

    #[test]
    fn category_instrument_variants() {
        assert_eq!(category_from_subcategories("Instrument|Synth"), PluginCategory::Instrument);
        assert_eq!(category_from_subcategories("Instrument|Sampler"), PluginCategory::Instrument);
        assert_eq!(category_from_subcategories("Instrument|Drum"), PluginCategory::Instrument);
        assert_eq!(category_from_subcategories("Instrument"), PluginCategory::Instrument);
    }

    #[test]
    fn category_analyzer() {
        assert_eq!(category_from_subcategories("Fx|Analyzer"), PluginCategory::Analyser);
    }

    #[test]
    fn category_spatial() {
        assert_eq!(category_from_subcategories("Fx|Spatial"), PluginCategory::Spatial);
        assert_eq!(category_from_subcategories("Fx|Surround"), PluginCategory::Spatial);
    }

    #[test]
    fn category_generator() {
        assert_eq!(category_from_subcategories("Generator"), PluginCategory::Generator);
    }

    #[test]
    fn category_utility() {
        assert_eq!(category_from_subcategories("Fx|Tools"), PluginCategory::Utility);
        assert_eq!(category_from_subcategories("Utility"), PluginCategory::Utility);
    }

    #[test]
    fn category_fx_fallback() {
        // "Fx" with no recognized subcategory → Filter
        assert_eq!(category_from_subcategories("Fx"), PluginCategory::Filter);
    }

    #[test]
    fn category_unknown_fallback() {
        assert_eq!(
            category_from_subcategories("SomethingUnknown"),
            PluginCategory::Other("VST3".to_string()),
        );
    }

    #[test]
    fn category_empty_string() {
        assert_eq!(
            category_from_subcategories(""),
            PluginCategory::Other("VST3".to_string()),
        );
    }

    #[test]
    fn category_first_match_wins() {
        // "Reverb" appears first → Reverb, not Delay
        assert_eq!(category_from_subcategories("Reverb|Delay"), PluginCategory::Reverb);
        // "Delay" appears first → Delay
        assert_eq!(category_from_subcategories("Delay|Reverb"), PluginCategory::Delay);
    }

    #[test]
    fn category_case_insensitive() {
        assert_eq!(category_from_subcategories("REVERB"), PluginCategory::Reverb);
        assert_eq!(category_from_subcategories("reverb"), PluginCategory::Reverb);
        assert_eq!(category_from_subcategories("Reverb"), PluginCategory::Reverb);
    }

    #[test]
    fn category_whitespace_trimming() {
        assert_eq!(category_from_subcategories("Fx | Reverb "), PluginCategory::Reverb);
        assert_eq!(category_from_subcategories(" Delay | Fx "), PluginCategory::Delay);
    }

    // ---- find_bundle_binary ----

    #[test]
    fn find_bundle_binary_nonexistent_path() {
        let result = find_bundle_binary(std::path::Path::new("/nonexistent/plugin.vst3"));
        assert!(result.is_none());
    }

    #[test]
    fn find_bundle_binary_standard_layout() {
        // Create a temp directory mimicking a .vst3 bundle
        let tmp = std::env::temp_dir().join("test_plugin.vst3");
        let arch_dir = tmp.join("Contents").join("x86_64-linux");
        std::fs::create_dir_all(&arch_dir).unwrap();
        let so_path = arch_dir.join("test_plugin.so");
        std::fs::write(&so_path, b"fake").unwrap();

        let result = find_bundle_binary(&tmp);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), so_path);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn find_bundle_binary_fallback_root() {
        // .so directly in bundle root (no Contents/<arch>/)
        let tmp = std::env::temp_dir().join("test_fallback.vst3");
        std::fs::create_dir_all(&tmp).unwrap();
        let so_path = tmp.join("fallback.so");
        std::fs::write(&so_path, b"fake").unwrap();

        let result = find_bundle_binary(&tmp);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), so_path);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn find_bundle_binary_no_so_file() {
        let tmp = std::env::temp_dir().join("test_no_so.vst3");
        let arch_dir = tmp.join("Contents").join("x86_64-linux");
        std::fs::create_dir_all(&arch_dir).unwrap();
        // Write a non-.so file
        std::fs::write(arch_dir.join("readme.txt"), b"not a plugin").unwrap();

        let result = find_bundle_binary(&tmp);
        assert!(result.is_none());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
