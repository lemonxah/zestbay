use std::collections::HashSet;
use std::path::PathBuf;

pub fn search_dirs(env_var: &str, defaults: &[&str], label: &str) -> Vec<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_expand_home_and_dedup() {
        let home = std::env::var("HOME").unwrap_or_default();
        let dirs = search_dirs(
            "ZESTBAY_TEST_UNSET_PATH",
            &["~/.clap", "/nonexistent/clap", "/nonexistent/clap"],
            "TEST",
        );
        assert_eq!(dirs[0], PathBuf::from(format!("{home}/.clap")));
        assert_eq!(
            dirs.iter().filter(|d| d.as_path() == std::path::Path::new("/nonexistent/clap")).count(),
            1
        );
    }
}
