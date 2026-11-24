use super::*;

pub(super) fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

pub(super) fn config_path(filename: &str) -> PathBuf {
    crate::session::config_path(filename)
}

pub(super) static CONFIG_ERRORS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub(super) static LOAD_FAILED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub(super) static SAVE_BLOCKED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub(super) fn push_config_error(msg: String) {
    log::error!("{}", msg);
    if let Ok(mut errs) = CONFIG_ERRORS.lock() {
        errs.push(msg);
    }
}

pub(super) fn take_config_errors() -> Vec<String> {
    CONFIG_ERRORS
        .lock()
        .map(|mut errs| std::mem::take(&mut *errs))
        .unwrap_or_default()
}

pub(super) fn list_contains(list: &std::sync::Mutex<Vec<String>>, name: &str) -> bool {
    list.lock()
        .map(|l| l.iter().any(|n| n == name))
        .unwrap_or(false)
}

pub(super) fn list_clear(list: &std::sync::Mutex<Vec<String>>) {
    if let Ok(mut l) = list.lock() {
        l.clear();
    }
}

pub(super) fn list_set(list: &std::sync::Mutex<Vec<String>>, name: &str, present: bool) {
    if let Ok(mut l) = list.lock() {
        l.retain(|n| n != name);
        if present {
            l.push(name.to_string());
        }
    }
}

pub(super) fn timestamp_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let tod = (secs % 86400) as u32;
    let (y, mo, d) = days_to_ymd(secs / 86400);
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        y,
        mo,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

pub(super) fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if crate::session::writes_suspended() {
        return Ok(());
    }
    let result = (|| {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(dir)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config".to_string());
        let tmp = dir.join(format!(".{}.tmp-{}", name, std::process::id()));
        {
            let mut f = std::fs::File::create(&tmp)?;
            if let Err(e) = f.write_all(data).and_then(|_| f.sync_all()) {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if let Err(ref e) = result {
        log::error!("Failed to write {:?}: {}", path, e);
    }
    result
}

pub(super) fn write_config(filename: &str, data: &[u8], is_empty: bool) -> bool {
    if list_contains(&SAVE_BLOCKED, filename) {
        log::warn!(
            "Not saving {}: it failed to load and could not be backed up",
            filename
        );
        return false;
    }
    if list_contains(&LOAD_FAILED, filename) {
        if is_empty {
            log::warn!(
                "Not overwriting {} (failed to load) with empty content",
                filename
            );
            return false;
        }
        list_set(&LOAD_FAILED, filename, false);
    }
    write_atomic(&config_path(filename), data).is_ok()
}

pub(super) fn backup_corrupt_file(filename: &str, err: &dyn std::fmt::Display) {
    let path = config_path(filename);
    let backup = config_path(&format!("{}.corrupt-{}", filename, timestamp_string()));
    match std::fs::copy(&path, &backup) {
        Ok(_) => {
            list_set(&LOAD_FAILED, filename, true);
            push_config_error(format!(
                "{} could not be read ({}). A copy was saved as {}.",
                filename,
                err,
                backup.display()
            ));
        }
        Err(e) => {
            list_set(&SAVE_BLOCKED, filename, true);
            push_config_error(format!(
                "{} could not be read ({}) and could not be backed up ({}). \
                 It will not be overwritten this session.",
                filename, err, e
            ));
        }
    }
}

pub(super) fn load_config<T: serde::de::DeserializeOwned>(filename: &str) -> Result<Option<T>, ()> {
    let path = config_path(filename);
    let text = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            list_set(&SAVE_BLOCKED, filename, true);
            push_config_error(format!("{} could not be read: {}", filename, e));
            return Err(());
        }
    };
    match serde_json::from_str::<T>(&text) {
        Ok(v) => Ok(Some(v)),
        Err(e) => {
            backup_corrupt_file(filename, &e);
            Err(())
        }
    }
}

pub(super) fn read_config_json_text(filename: &str, default: &str) -> String {
    match load_config::<serde_json::Value>(filename) {
        Ok(Some(v)) => serde_json::to_string(&v).unwrap_or_else(|_| default.to_string()),
        _ => default.to_string(),
    }
}

pub(super) fn write_config_json_text(filename: &str, json: &str) {
    let parsed = match serde_json::from_str::<serde_json::Value>(json) {
        Ok(v) => v,
        Err(e) => {
            log::error!("Refusing to save invalid JSON to {}: {}", filename, e);
            return;
        }
    };
    let is_empty = match &parsed {
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    };
    write_config(filename, json.as_bytes(), is_empty);
}

pub(super) fn crash_marker_path() -> PathBuf {
    config_path(".zestbay-restoring")
}

pub(super) fn write_crash_marker(plugin_uris: &[String]) {
    let content = plugin_uris.join("\n");
    let _ = write_atomic(&crash_marker_path(), content.as_bytes());
}

pub(super) fn remove_crash_marker() {
    let path = crash_marker_path();
    let _ = std::fs::remove_file(&path);
}

pub(super) fn has_crash_marker() -> bool {
    crash_marker_path().exists()
}

pub(super) fn read_crash_marker() -> Vec<String> {
    let path = crash_marker_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => s
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub(super) fn known_good_plugins_path() -> PathBuf {
    config_path("plugins.known_good.json")
}

pub(super) fn copy_plugins_file(src: &std::path::Path, dst: &std::path::Path) -> bool {
    let content = match std::fs::read(src) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to read {:?}: {}", src, e);
            return false;
        }
    };
    if let Err(e) = serde_json::from_slice::<Vec<SavedPlugin>>(&content) {
        log::error!("{:?} is not a valid plugin list ({}); not copying", src, e);
        return false;
    }
    write_atomic(dst, &content).is_ok()
}

pub(super) fn save_known_good_plugins() {
    let src = config_path("plugins.json");
    let dst = known_good_plugins_path();
    if src.exists() && copy_plugins_file(&src, &dst) {
        log::info!("Saved known-good plugins snapshot to {:?}", dst);
    }
}

pub(super) fn has_known_good_plugins() -> bool {
    known_good_plugins_path().exists()
}

pub(super) fn restore_known_good_plugins() -> bool {
    let src = known_good_plugins_path();
    let dst = config_path("plugins.json");
    if !src.exists() {
        log::warn!("No known-good plugins snapshot to restore");
        return false;
    }
    if copy_plugins_file(&src, &dst) {
        log::info!("Restored plugins.json from known-good snapshot");
        list_set(&LOAD_FAILED, "plugins.json", false);
        true
    } else {
        log::error!("Failed to restore known-good plugins");
        false
    }
}
