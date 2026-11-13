//! Sessions: a folder holding one complete setup (plugins, links, rules,
//! MIDI mappings, layout...). Settings, presets and the template library are
//! shared by every session.
//!
//! Sessions live in `~/.config/zestbay/sessions/<name>/`. Which one opens is
//! decided at startup (`--session <name>`, `--session_path <dir>`, otherwise
//! the one remembered in `current_session`, otherwise "default"). Sessions
//! can also be switched while running (see `AppController::begin_session_swap`).

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DEFAULT_NAME: &str = "default";

/// Files and folders that belong to a session. Everything else in the config
/// folder (preferences, window, presets, templates...) is shared.
const SESSION_ENTRIES: &[&str] = &[
    "plugins.json",
    "plugins.known_good.json",
    "links.json",
    "rules.json",
    "rule_backups",
    "subgraphs.json",
    "virtual_devices.json",
    "midi_mappings.json",
    "default_node.txt",
    "layout.json",
    "hidden.json",
    "docked-meters.json",
    "pinned.json",
    "viewport.json",
    "plugin-state",
    ".zestbay-restoring",
];

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// The session remembered from last time.
    Remembered,
    Name(String),
    Path(PathBuf),
}

struct Active {
    dir: PathBuf,
    /// Session name; None when opened by path.
    name: Option<String>,
}

static ACTIVE: RwLock<Option<Active>> = RwLock::new(None);

/// Set while one session is being taken down for another: nothing may be
/// written, or the old session's leftovers would land in the new folder.
static WRITES_SUSPENDED: AtomicBool = AtomicBool::new(false);

pub fn suspend_writes(on: bool) {
    WRITES_SUSPENDED.store(on, Ordering::SeqCst);
}

pub fn writes_suspended() -> bool {
    WRITES_SUSPENDED.load(Ordering::SeqCst)
}

/// The shared config folder.
pub fn root_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("zestbay")
}

pub fn sessions_dir() -> PathBuf {
    root_dir().join("sessions")
}

/// The open session's folder (the shared config folder until `init` ran).
pub fn dir() -> PathBuf {
    ACTIVE
        .read()
        .ok()
        .and_then(|a| a.as_ref().map(|a| a.dir.clone()))
        .unwrap_or_else(root_dir)
}

/// Name of the open session, or None when it was opened by path.
pub fn current_name() -> Option<String> {
    ACTIVE
        .read()
        .ok()
        .and_then(|a| a.as_ref().and_then(|a| a.name.clone()))
}

/// Whether `filename` (a name relative to the config folder, possibly with a
/// suffix such as `.corrupt-…`) belongs to the session.
pub fn is_session_file(filename: &str) -> bool {
    SESSION_ENTRIES
        .iter()
        .any(|e| filename == *e || filename.strip_prefix(e).is_some_and(|rest| rest.starts_with('.')))
}

/// Where a config file lives.
pub fn config_path(filename: &str) -> PathBuf {
    if is_session_file(filename) {
        dir().join(filename)
    } else {
        root_dir().join(filename)
    }
}

/// A usable session name: one plain folder name, no hidden or path-like names.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name == name.trim()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

/// Names of the existing sessions, sorted.
pub fn list() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(sessions_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_name(n))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Create an empty session.
pub fn create(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err("Use a plain name without slashes, and not starting with a dot.".into());
    }
    let dir = sessions_dir().join(name);
    if dir.exists() {
        return Err(format!("A session called \"{name}\" already exists."));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create the session: {e}"))
}

/// Delete a session and everything in it. The open session can't be
/// deleted.
pub fn delete(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err("That is not a session name.".into());
    }
    if current_name().as_deref() == Some(name) {
        return Err("The open session can't be deleted. Switch to another one first.".into());
    }
    let dir = sessions_dir().join(name);
    if !dir.is_dir() {
        return Err(format!("There is no session called \"{name}\"."));
    }
    // A session opened by path may be this very folder.
    if dir.canonicalize().ok() == Some(self::dir()) {
        return Err("The open session can't be deleted. Switch to another one first.".into());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("Could not delete the session: {e}"))?;
    log::info!("Deleted session \"{name}\"");
    Ok(())
}

fn remembered_path() -> PathBuf {
    root_dir().join("current_session")
}

/// The session to open when none is asked for.
fn remembered() -> String {
    std::fs::read_to_string(remembered_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|n| valid_name(n) && sessions_dir().join(n).is_dir())
        .unwrap_or_else(|| DEFAULT_NAME.to_string())
}

/// Make `name` the session that opens by default.
pub fn remember(name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(root_dir())?;
    std::fs::write(remembered_path(), name)
}

/// Pick the session to use for this run. Call once, before anything reads or
/// writes a config file.
pub fn init(choice: Choice) -> Result<(), String> {
    let active = match choice {
        Choice::Path(path) => {
            if path.exists() && !path.is_dir() {
                return Err(format!("{} is not a folder.", path.display()));
            }
            std::fs::create_dir_all(&path)
                .map_err(|e| format!("Could not create {}: {e}", path.display()))?;
            let dir = path.canonicalize().unwrap_or(path);
            Active { dir, name: None }
        }
        Choice::Name(name) => open_named(&name)?,
        Choice::Remembered => open_named(&remembered())?,
    };
    log::info!("Session: {}", active.dir.display());
    set_active(active);
    Ok(())
}

fn set_active(active: Active) {
    if let Ok(mut slot) = ACTIVE.write() {
        *slot = Some(active);
    }
}

/// Open another existing session while running, and make it the one that
/// opens next time.
pub fn activate(name: &str) -> Result<(), String> {
    if !list().iter().any(|n| n == name) {
        return Err(format!("There is no session called \"{name}\"."));
    }
    let active = open_named(name)?;
    log::info!("Session: {}", active.dir.display());
    set_active(active);
    if let Err(e) = remember(name) {
        log::warn!("Could not remember the session: {e}");
    }
    Ok(())
}

fn open_named(name: &str) -> Result<Active, String> {
    if !valid_name(name) {
        return Err(format!(
            "\"{name}\" is not a valid session name: use a plain name without slashes, not starting with a dot."
        ));
    }
    let dir = sessions_dir().join(name);
    if !dir.exists() {
        if name == DEFAULT_NAME {
            migrate_legacy(&dir);
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    Ok(Active {
        dir,
        name: Some(name.to_string()),
    })
}

/// Before sessions existed everything sat directly in the config folder.
/// The first time the default session opens, copy that into it. The old
/// files stay where they are.
fn migrate_legacy(dest: &Path) {
    let root = root_dir();
    let have_any = SESSION_ENTRIES
        .iter()
        .filter(|e| **e != ".zestbay-restoring")
        .any(|e| root.join(e).exists());
    if !have_any {
        return;
    }
    let tmp = dest.with_extension("migrating");
    let _ = std::fs::remove_dir_all(&tmp);
    let copied = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&tmp)?;
        for entry in SESSION_ENTRIES.iter().filter(|e| **e != ".zestbay-restoring") {
            let src = root.join(entry);
            if src.is_dir() {
                copy_dir(&src, &tmp.join(entry))?;
            } else if src.is_file() {
                std::fs::copy(&src, tmp.join(entry))?;
            }
        }
        std::fs::rename(&tmp, dest)
    })();
    match copied {
        Ok(()) => log::info!(
            "Copied the existing setup into the \"{}\" session",
            DEFAULT_NAME
        ),
        Err(e) => {
            log::error!("Could not copy the existing setup into a session: {e}");
            let _ = std::fs::remove_dir_all(&tmp);
        }
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &dst.join(entry.file_name()))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------

/// `--session <name>` / `--session=<name>`, `--session_path <dir>` /
/// `--session-path <dir>` (also with `=`).
pub fn parse_args(args: &[String]) -> Result<Choice, String> {
    let mut choice = None;
    let mut i = 0;
    while i < args.len() {
        let (flag, inline) = match args[i].split_once('=') {
            Some((f, v)) => (f, Some(v.to_string())),
            None => (args[i].as_str(), None),
        };
        let is_path = match flag {
            "--session" => false,
            "--session_path" | "--session-path" => true,
            _ => {
                i += 1;
                continue;
            }
        };
        let value = match inline {
            Some(v) => v,
            None => {
                i += 1;
                args.get(i)
                    .cloned()
                    .ok_or_else(|| format!("{flag} needs a value"))?
            }
        };
        if value.is_empty() {
            return Err(format!("{flag} needs a value"));
        }
        if choice.is_some() {
            return Err("Give only one of --session and --session_path, once.".into());
        }
        choice = Some(if is_path {
            Choice::Path(PathBuf::from(value))
        } else {
            Choice::Name(value)
        });
        i += 1;
    }
    Ok(choice.unwrap_or(Choice::Remembered))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn session_files_are_told_apart() {
        for f in [
            "plugins.json",
            "plugins.json.corrupt-2026",
            "plugins.known_good.json",
            "links.json.pre-upgrade",
            "rules.json",
            "rule_backups",
            "plugin-state",
            ".zestbay-restoring",
            "default_node.txt",
        ] {
            assert!(is_session_file(f), "{f}");
        }
        for f in [
            "preferences.json",
            "presets.json",
            "window.json",
            "subgraph_templates",
            "preset-files",
            "plugins.jsonx",
            "current_session",
        ] {
            assert!(!is_session_file(f), "{f}");
        }
    }

    #[test]
    fn names() {
        assert!(valid_name("studio"));
        assert!(valid_name("Live set 2"));
        for bad in ["", " x", "x ", ".hidden", "a/b", "..", "a\\b", "a\nb"] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn command_line() {
        assert_eq!(parse_args(&args(&[])), Ok(Choice::Remembered));
        assert_eq!(
            parse_args(&args(&["--session", "studio"])),
            Ok(Choice::Name("studio".into()))
        );
        assert_eq!(
            parse_args(&args(&["--safe-mode", "--session=live"])),
            Ok(Choice::Name("live".into()))
        );
        assert_eq!(
            parse_args(&args(&["--session_path", "/tmp/x"])),
            Ok(Choice::Path("/tmp/x".into()))
        );
        assert_eq!(
            parse_args(&args(&["--session-path=/tmp/y"])),
            Ok(Choice::Path("/tmp/y".into()))
        );
        assert!(parse_args(&args(&["--session"])).is_err());
        assert!(parse_args(&args(&["--session="])).is_err());
        assert!(parse_args(&args(&["--session", "a", "--session_path", "/b"])).is_err());
    }
}
