use std::ffi::CString;
use std::path::{Path, PathBuf};

use vst3::Steinberg::*;

use crate::plugin::types::*;

const VST3_SEARCH_DIRS: &[&str] = &[
    "~/.vst3",
    "/usr/lib/vst3",
    "/usr/local/lib/vst3",
    "/usr/lib64/vst3",
    "/usr/local/lib64/vst3",
];

static CACHE: crate::clap::scanner::ScanCache = crate::clap::scanner::ScanCache::new();

pub fn cached_plugin(uri: &str) -> Option<PluginInfo> {
    CACHE.get(uri)
}

pub fn find_plugin(uri: &str) -> Option<PluginInfo> {
    CACHE.find(uri, || {
        scan_plugins();
    })
}

pub fn scan_plugins() -> Vec<PluginInfo> {
    scan_discovered(&discover_plugins(), &|| {})
}

pub(crate) fn discover_plugins() -> Vec<PathBuf> {
    crate::plugin::scan::discover(&expand_search_dirs(), "vst3", false)
}

pub(crate) fn scan_discovered(paths: &[PathBuf], progress: &(impl Fn() + Sync)) -> Vec<PluginInfo> {
    let mut plugins = crate::plugin::scan::scan_libraries("vst3", paths, progress);
    plugins.sort_by_cached_key(|p| p.name.to_lowercase());
    log::info!("VST3: found {} plugins total", plugins.len());
    CACHE.store(&plugins);
    plugins
}

fn expand_search_dirs() -> Vec<PathBuf> {
    crate::plugin::paths::search_dirs("VST3_PATH", VST3_SEARCH_DIRS, "VST3")
}

const ARCH_DIRS: &[&str] = &["x86_64-linux", "aarch64-linux", "i386-linux", "armv7l-linux"];

fn arch_dir_for(arch: &str) -> String {
    match arch {
        "x86" => "i386-linux".to_string(),
        "arm" => "armv7l-linux".to_string(),
        other => format!("{other}-linux"),
    }
}

fn arch_dir_candidates() -> Vec<String> {
    let native = arch_dir_for(std::env::consts::ARCH);
    let mut dirs = vec![native.clone()];
    dirs.extend(ARCH_DIRS.iter().filter(|d| **d != native).map(|d| d.to_string()));
    dirs
}

pub fn find_bundle_binary(bundle_path: &Path) -> Option<PathBuf> {
    for arch in arch_dir_candidates() {
        let contents_dir = bundle_path.join("Contents").join(arch);
        if contents_dir.is_dir()
            && let Ok(entries) = std::fs::read_dir(&contents_dir)
        {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e == "so") {
                    return Some(p);
                }
            }
        }
    }

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

pub(crate) fn scan_vst3_bundle(bundle_path: &Path, plugins: &mut Vec<PluginInfo>) {
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

    unsafe {
        let lib = libc::dlopen(c_path.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if lib.is_null() {
            let err = std::ffi::CStr::from_ptr(libc::dlerror());
            log::debug!("VST3: dlopen failed for {}: {:?}", so_str, err);
            return;
        }

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

        let factory2: Option<vst3::ComPtr<IPluginFactory2>> = factory.cast();

        let count = factory.countClasses();

        for i in 0..count {
            let mut info: PClassInfo = std::mem::zeroed();
            if factory.getClassInfo(i, &mut info) != kResultOk {
                continue;
            }

            let category_str = read_cstr(&info.category);
            if category_str != "Audio Module Class" {
                continue;
            }

            let name = read_cstr(&info.name);
            let uri = tuid_to_hex(&info.cid);

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

            let is_instrument = sub_categories.contains("Instrument")
                || plugin_category == PluginCategory::Instrument;
            let (audio_inputs, audio_outputs) = if is_instrument {
                (0, 2)
            } else {
                (2, 2)
            };

            plugins.push(PluginInfo {
                uri,
                name,
                format: PluginFormat::Vst3,
                category: plugin_category,
                author: vendor,
                ports: Vec::new(),
                audio_inputs,
                audio_outputs,
                control_inputs: 0,
                control_outputs: 0,
                required_features: Vec::new(),
                compatible: true,
                has_ui: true,
                library_path: bundle_str.to_string(),
                ladspa_ids: Vec::new(),
            });
        }

        drop(factory2);
        drop(factory);

        call_module_exit(lib);
    }
}

unsafe fn call_module_exit(lib: *mut std::ffi::c_void) {
    unsafe {
        let sym = libc::dlsym(lib, c"ModuleExit".as_ptr());
        if !sym.is_null() {
            let module_exit: unsafe extern "system" fn() -> bool = std::mem::transmute(sym);
            module_exit();
        }
    }
}

fn tuid_to_hex(tuid: &[std::ffi::c_char; 16]) -> String {
    tuid.iter()
        .map(|&b| format!("{:02X}", b as u8))
        .collect()
}

pub fn hex_to_tuid(hex: &str) -> Option<[std::ffi::c_char; 16]> {
    if hex.len() != 32 {
        return None;
    }
    let mut tuid = [0 as std::ffi::c_char; 16];
    for i in 0..16 {
        let byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
        tuid[i] = byte as std::ffi::c_char;
    }
    Some(tuid)
}

fn read_cstr(buf: &[std::ffi::c_char]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(bytes).unwrap_or_else(|_| "?".to_string())
}

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

    if parts.contains(&"Fx") {
        return PluginCategory::Filter;
    }
    if parts.contains(&"Instrument") {
        return PluginCategory::Instrument;
    }

    PluginCategory::Other("VST3".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_tuid_valid() {
        let hex = "0123456789ABCDEF0123456789ABCDEF";
        let tuid = hex_to_tuid(hex).expect("valid 32-char hex should parse");
        assert_eq!(tuid.len(), 16);
        assert_eq!(tuid[0] as u8, 0x01);
        assert_eq!(tuid[1] as u8, 0x23);
        assert_eq!(tuid[15] as u8, 0xEF);
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
        assert_eq!(back, hex.to_uppercase());
    }

    #[test]
    fn hex_to_tuid_wrong_length() {
        assert!(hex_to_tuid("").is_none());
        assert!(hex_to_tuid("0123456789ABCDEF").is_none());
        assert!(hex_to_tuid("0123456789ABCDEF0123456789ABCDEF00").is_none());
    }

    #[test]
    fn hex_to_tuid_invalid_hex_chars() {
        assert!(hex_to_tuid("ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ").is_none());
        assert!(hex_to_tuid("0123456789ABCDEF0123456789ABCDEG").is_none());
    }

    #[test]
    fn tuid_to_hex_all_zeros() {
        let tuid = [0 as std::ffi::c_char; 16];
        assert_eq!(tuid_to_hex(&tuid), "00000000000000000000000000000000");
    }

    #[test]
    fn tuid_to_hex_all_ff() {
        let tuid = [0xFF_u8 as std::ffi::c_char; 16];
        assert_eq!(tuid_to_hex(&tuid), "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF");
    }

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
        assert_eq!(category_from_subcategories("Reverb|Delay"), PluginCategory::Reverb);
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

    #[test]
    fn find_bundle_binary_nonexistent_path() {
        let result = find_bundle_binary(std::path::Path::new("/nonexistent/plugin.vst3"));
        assert!(result.is_none());
    }

    #[test]
    fn find_bundle_binary_standard_layout() {
        let tmp = std::env::temp_dir().join("test_plugin.vst3");
        let arch_dir = tmp.join("Contents").join(arch_dir_for(std::env::consts::ARCH));
        std::fs::create_dir_all(&arch_dir).unwrap();
        let so_path = arch_dir.join("test_plugin.so");
        std::fs::write(&so_path, b"fake").unwrap();

        let result = find_bundle_binary(&tmp);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), so_path);

        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn arch_dir_names() {
        assert_eq!(arch_dir_for("x86_64"), "x86_64-linux");
        assert_eq!(arch_dir_for("aarch64"), "aarch64-linux");
        assert_eq!(arch_dir_for("x86"), "i386-linux");
        assert_eq!(arch_dir_for("arm"), "armv7l-linux");
        let c = arch_dir_candidates();
        assert_eq!(c[0], arch_dir_for(std::env::consts::ARCH));
        for d in ARCH_DIRS {
            assert_eq!(c.iter().filter(|x| x == d).count(), 1);
        }
    }

    #[test]
    fn find_bundle_binary_prefers_host_arch() {
        let tmp = std::env::temp_dir().join("test_multiarch.vst3");
        let _ = std::fs::remove_dir_all(&tmp);
        let native = arch_dir_for(std::env::consts::ARCH);
        let mut expected = None;
        for arch in ARCH_DIRS.iter().map(|s| s.to_string()).chain([native.clone()]) {
            let dir = tmp.join("Contents").join(&arch);
            std::fs::create_dir_all(&dir).unwrap();
            let so = dir.join("multi.so");
            std::fs::write(&so, b"fake").unwrap();
            if arch == native {
                expected = Some(so);
            }
        }
        assert_eq!(find_bundle_binary(&tmp), expected);
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn find_bundle_binary_fallback_root() {
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
        let arch_dir = tmp.join("Contents").join(arch_dir_for(std::env::consts::ARCH));
        std::fs::create_dir_all(&arch_dir).unwrap();
        std::fs::write(arch_dir.join("readme.txt"), b"not a plugin").unwrap();

        let result = find_bundle_binary(&tmp);
        assert!(result.is_none());

        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
