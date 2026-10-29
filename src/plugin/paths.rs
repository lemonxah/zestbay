//! Plugin search paths, shared by the format scanners.
//!
//! Order: the format's environment variable (`CLAP_PATH`, `VST3_PATH`), the
//! standard directories, then — inside a Flatpak — the app's own plugin
//! folders (`app_plugin_dir`), the plugin extension point and the host's
//! system directories (mounted under `/run/host` only when the user grants
//! `--filesystem=host-os`).  Missing directories are kept; the scanners skip
//! them.

use std::collections::HashSet;
use std::path::PathBuf;

/// Mount point of the Linux audio plugin extensions
/// (`org.freedesktop.LinuxAudio.Plugins.*`) in a Flatpak.
const FLATPAK_EXTENSION_DIR: &str = "/app/extensions/Plugins";

/// Where a Flatpak sees the host's `/usr` (with `--filesystem=host-os`).
const FLATPAK_HOST_USR: &str = "/run/host/usr";

/// True when running inside a Flatpak sandbox.
pub fn in_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

/// Inside a Flatpak, the app's own folder for one plugin format
/// (`~/.var/app/<app id>/data/<lv2|vst3|clap>` on the host). The app can
/// always read and write it, no permission needed, so users drop their own
/// plugins (e.g. Pianoteq) there. `None` outside a Flatpak.
pub fn app_plugin_dir(format: &str) -> Option<PathBuf> {
    if !in_flatpak() {
        return None;
    }
    dirs::data_dir().map(|d| d.join(format))
}

/// Create the app's plugin folders (Flatpak only) so they're easy to find.
pub fn ensure_app_plugin_dirs() {
    for format in ["lv2", "vst3", "clap"] {
        if let Some(dir) = app_plugin_dir(format)
            && let Err(e) = std::fs::create_dir_all(&dir)
        {
            log::warn!("Could not create {}: {}", dir.display(), e);
        }
    }
}

/// The Flatpak application ID, when running inside one.
pub fn flatpak_app_id() -> Option<String> {
    std::env::var("FLATPAK_ID").ok().filter(|id| !id.is_empty())
}

/// Directories to scan for plugins of one format.
///
/// `env_var` is a colon-separated override list, `defaults` the standard
/// directories (a leading `~` means `$HOME`), and `subdir` the format's
/// directory name under the Flatpak extension point and the host's lib dirs
/// (`"clap"`, `"vst3"`).
pub fn search_dirs(env_var: &str, defaults: &[&str], subdir: &str, label: &str) -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let expand = |d: &str| -> PathBuf {
        match d.strip_prefix('~') {
            Some(rest) => PathBuf::from(format!("{home}{rest}")),
            None => PathBuf::from(d),
        }
    };

    let mut candidates: Vec<PathBuf> = std::env::var(env_var)
        .map(|v| v.split(':').filter(|s| !s.is_empty()).map(expand).collect())
        .unwrap_or_default();
    candidates.extend(defaults.iter().map(|d| expand(d)));
    if in_flatpak() {
        candidates.extend(app_plugin_dir(subdir));
        candidates.push(PathBuf::from(format!("{FLATPAK_EXTENSION_DIR}/{subdir}")));
        candidates.extend(host_lib_dirs().map(|lib| PathBuf::from(format!("{lib}/{subdir}"))));
    }

    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| {
            let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let new = seen.insert(canonical);
            if !new {
                log::debug!("{}: skipping duplicate directory {}", label, path.display());
            }
            new
        })
        .collect()
}

/// `LV2_PATH` for a Flatpak: lilv only knows the paths compiled into it, so
/// the user dir, the extension point and the host's directories are listed
/// explicitly.  `None` outside a Flatpak or when `LV2_PATH` is already set.
pub fn flatpak_lv2_path() -> Option<String> {
    if !in_flatpak() || std::env::var_os("LV2_PATH").is_some() {
        return None;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs = vec![format!("{home}/.lv2")];
    dirs.extend(app_plugin_dir("lv2").map(|d| d.to_string_lossy().into_owned()));
    dirs.extend([
        format!("{FLATPAK_EXTENSION_DIR}/lv2"),
        "/app/lib/lv2".to_string(),
    ]);
    dirs.extend(host_lib_dirs().map(|lib| format!("{lib}/lv2")));
    // Drop directories that resolve to one already listed (e.g. lib64 -> lib
    // on Arch), or lilv loads every plugin twice and warns about each.
    let mut seen = HashSet::new();
    dirs.retain(|d| {
        let path = PathBuf::from(d);
        seen.insert(std::fs::canonicalize(&path).unwrap_or(path))
    });
    Some(dirs.join(":"))
}

fn host_lib_dirs() -> impl Iterator<Item = String> {
    ["lib", "lib64", "local/lib", "local/lib64"]
        .into_iter()
        .map(|lib| format!("{FLATPAK_HOST_USR}/{lib}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_expand_home_and_dedup() {
        let home = std::env::var("HOME").unwrap_or_default();
        let dirs = search_dirs(
            "ZESTBAY_TEST_UNSET_PATH",
            &["~/.clap", "/nonexistent/clap", "/nonexistent/clap"],
            "clap",
            "TEST",
        );
        assert_eq!(dirs[0], PathBuf::from(format!("{home}/.clap")));
        assert_eq!(
            dirs.iter().filter(|d| d.as_path() == std::path::Path::new("/nonexistent/clap")).count(),
            1
        );
    }
}
