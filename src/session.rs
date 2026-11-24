use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DEFAULT_NAME: &str = "default";

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
    "node-style.json",
    "viewport.json",
    "plugin-state",
    ".zestbay-restoring",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Remembered,
    Name(String),
    Path(PathBuf),
}

struct Active {
    dir: PathBuf,
    name: Option<String>,
}

static ACTIVE: RwLock<Option<Active>> = RwLock::new(None);

static WRITES_SUSPENDED: AtomicBool = AtomicBool::new(false);

pub fn suspend_writes(on: bool) {
    WRITES_SUSPENDED.store(on, Ordering::SeqCst);
}

pub fn writes_suspended() -> bool {
    WRITES_SUSPENDED.load(Ordering::SeqCst)
}

pub fn root_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("zestbay")
}

pub fn sessions_dir() -> PathBuf {
    root_dir().join("sessions")
}

pub fn dir() -> PathBuf {
    ACTIVE
        .read()
        .ok()
        .and_then(|a| a.as_ref().map(|a| a.dir.clone()))
        .unwrap_or_else(root_dir)
}

pub fn current_name() -> Option<String> {
    ACTIVE
        .read()
        .ok()
        .and_then(|a| a.as_ref().and_then(|a| a.name.clone()))
}

pub fn is_session_file(filename: &str) -> bool {
    SESSION_ENTRIES.iter().any(|e| {
        filename == *e
            || filename
                .strip_prefix(e)
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

pub fn config_path(filename: &str) -> PathBuf {
    if is_session_file(filename) {
        dir().join(filename)
    } else {
        root_dir().join(filename)
    }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name == name.trim()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

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
    remove_session_directory(&dir, &self::dir())?;
    log::info!("Deleted session \"{name}\"");
    Ok(())
}

fn remove_session_directory(dir: &Path, active: &Path) -> Result<(), String> {
    let target = dir
        .canonicalize()
        .map_err(|e| format!("Could not locate the session: {e}"))?;
    let active = active
        .canonicalize()
        .map_err(|e| format!("Could not locate the open session: {e}"))?;
    if active.starts_with(&target) {
        return Err("The open session can't be deleted. Switch to another one first.".into());
    }
    std::fs::remove_dir_all(dir).map_err(|e| format!("Could not delete the session: {e}"))
}

fn remembered_path() -> PathBuf {
    root_dir().join("current_session")
}

fn remembered() -> String {
    std::fs::read_to_string(remembered_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|n| valid_name(n) && sessions_dir().join(n).is_dir())
        .unwrap_or_else(|| DEFAULT_NAME.to_string())
}

pub fn remember(name: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(root_dir())?;
    std::fs::write(remembered_path(), name)
}

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
    open_named_at(&root_dir(), name, migrate_legacy)
}

fn open_named_at(
    root: &Path,
    name: &str,
    migrate: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<Active, String> {
    if !valid_name(name) {
        return Err(format!(
            "\"{name}\" is not a valid session name: use a plain name without slashes, not starting with a dot."
        ));
    }
    let dir = root.join("sessions").join(name);
    if !dir.exists() {
        if name == DEFAULT_NAME {
            migrate(root, &dir).map_err(|e| {
                format!(
                    "Could not migrate the existing setup: {e}. Your original files are unchanged."
                )
            })?;
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    Ok(Active {
        dir,
        name: Some(name.to_string()),
    })
}

fn migrate_legacy(root: &Path, dest: &Path) -> std::io::Result<()> {
    let have_any = SESSION_ENTRIES
        .iter()
        .filter(|e| **e != ".zestbay-restoring")
        .any(|e| root.join(e).exists());
    if !have_any {
        return Ok(());
    }
    let tmp = dest.with_file_name(format!(".default.migrating-{}", uuid::Uuid::new_v4()));
    let copied = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&tmp)?;
        for entry in SESSION_ENTRIES
            .iter()
            .filter(|e| **e != ".zestbay-restoring")
        {
            let src = root.join(entry);
            if src.is_dir() {
                copy_dir(&src, &tmp.join(entry))?;
            } else if src.is_file() {
                std::fs::copy(&src, tmp.join(entry))?;
            }
        }
        std::fs::rename(&tmp, dest)
    })();
    if copied.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        log::info!(
            "Copied the existing setup into the \"{}\" session",
            DEFAULT_NAME
        );
    }
    copied
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

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("zestbay-session-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn deletion_protects_active_directory_and_ancestors() {
        let root = Directory::new();
        let parent = root.0.join("studio");
        let active = parent.join("live");
        let sibling = root.0.join("studio-other");
        std::fs::create_dir_all(&active).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(active.join("plugins.json"), "saved state").unwrap();
        assert!(remove_session_directory(&active, &active).is_err());
        assert!(remove_session_directory(&parent, &active).is_err());
        assert!(remove_session_directory(&sibling, &active).is_ok());
        assert_eq!(
            std::fs::read_to_string(active.join("plugins.json")).unwrap(),
            "saved state"
        );
    }

    #[cfg(unix)]
    #[test]
    fn deletion_protects_active_directory_through_symlinks() {
        let root = Directory::new();
        let active = root.0.join("studio/live");
        let alias = root.0.join("alias");
        std::fs::create_dir_all(&active).unwrap();
        std::os::unix::fs::symlink(root.0.join("studio"), &alias).unwrap();
        assert!(remove_session_directory(&alias, &active).is_err());
        assert!(active.exists());
    }

    #[test]
    fn failed_migration_can_be_retried_without_an_empty_session() {
        let root = Directory::new();
        std::fs::write(root.0.join("plugins.json"), "legacy state").unwrap();
        let result = open_named_at(&root.0, DEFAULT_NAME, |_, _| {
            Err(std::io::Error::other("copy failed"))
        });
        assert!(result.is_err());
        assert!(!root.0.join("sessions/default").exists());
        let active = open_named_at(&root.0, DEFAULT_NAME, migrate_legacy).unwrap();
        assert_eq!(
            std::fs::read_to_string(active.dir.join("plugins.json")).unwrap(),
            "legacy state"
        );
        assert_eq!(
            std::fs::read_to_string(root.0.join("plugins.json")).unwrap(),
            "legacy state"
        );
    }

    #[test]
    fn migration_does_not_remove_a_session_named_default_migrating() {
        let root = Directory::new();
        let other = root.0.join("sessions/default.migrating");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("plugins.json"), "other session").unwrap();
        std::fs::write(root.0.join("plugins.json"), "legacy state").unwrap();
        open_named_at(&root.0, DEFAULT_NAME, migrate_legacy).unwrap();
        assert_eq!(
            std::fs::read_to_string(other.join("plugins.json")).unwrap(),
            "other session"
        );
        assert_eq!(
            std::fs::read_dir(root.0.join("sessions")).unwrap().count(),
            2
        );
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
