//! Background catalog discovery and bounded, isolated plugin scanning.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};

use super::types::PluginInfo;

pub enum ScanEvent {
    Progress { completed: usize, total: usize },
    Complete(Vec<PluginInfo>),
}

pub fn start() -> std::io::Result<Receiver<ScanEvent>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("zestbay-catalog".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let clap = crate::clap::scanner::discover_plugins();
            let vst3 = crate::vst3::scanner::discover_plugins();
            // One step for LV2's metadata catalog, then one per native library.
            let total = 1 + clap.len() + vst3.len();
            let completed = AtomicUsize::new(0);
            let _ = tx.send(ScanEvent::Progress {
                completed: 0,
                total,
            });
            let progress = || {
                let completed = completed.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = tx.send(ScanEvent::Progress { completed, total });
            };
            let mut plugins = crate::lv2::scanner::scan_plugins();
            progress();
            plugins.extend(crate::clap::scanner::scan_discovered(&clap, &progress));
            plugins.extend(crate::vst3::scanner::scan_discovered(&vst3, &progress));
            plugins.sort_by_cached_key(|p| p.name.to_lowercase());
            log::info!(
                "Plugin catalog: {} plugins in {:?}",
                plugins.len(),
                started.elapsed()
            );
            let _ = tx.send(ScanEvent::Complete(plugins));
        })?;
    Ok(rx)
}

/// Follow symlinks, but visit each directory and bundle only once. Search
/// roots may overlap or link back into each other.
pub fn discover(dirs: &[PathBuf], extension: &str, files_allowed: bool) -> Vec<PathBuf> {
    fn visit(
        path: &Path,
        extension: &str,
        files_allowed: bool,
        seen: &mut HashSet<PathBuf>,
        found: &mut Vec<PathBuf>,
    ) {
        let Ok(canonical) = path.canonicalize() else {
            return;
        };
        if !seen.insert(canonical.clone()) {
            return;
        }
        let is_dir = path.is_dir();
        if path.extension().is_some_and(|e| e == extension)
            && (is_dir || (files_allowed && path.is_file()))
        {
            found.push(canonical);
        } else if is_dir && let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                visit(&entry.path(), extension, files_allowed, seen, found);
            }
        }
    }
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for dir in dirs {
        visit(dir, extension, files_allowed, &mut seen, &mut found);
    }
    found
}

pub fn scan_libraries(
    format: &str,
    paths: &[PathBuf],
    progress: &(impl Fn() + Sync),
) -> Vec<PluginInfo> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4);
    parallel_scan(paths, workers, |path| {
        use super::sandbox::{SandboxResult, exec_scan};
        let result = exec_scan(format, path);
        progress();
        match result {
            SandboxResult::Ok(found) => found,
            error => {
                log::warn!("{} scan of {} failed: {:?}", format, path.display(), error);
                Vec::new()
            }
        }
    })
}

fn parallel_scan<T: Send>(
    paths: &[PathBuf],
    workers: usize,
    scan: impl Fn(&Path) -> Vec<T> + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers.max(1).min(paths.len()) {
            let (next, scan) = (&next, &scan);
            handles.push(scope.spawn(move || {
                let mut results = Vec::new();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(index) else { break };
                    results.push((index, scan(path)));
                }
                results
            }));
        }
        let mut results: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.join().expect("scan worker panicked"))
            .collect();
        results.sort_by_key(|(index, _)| *index);
        results.into_iter().flat_map(|(_, items)| items).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_overlap_but_respect_limit_and_scan_every_path_once() {
        let paths: Vec<_> = (0..12).map(|i| PathBuf::from(i.to_string())).collect();
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let first_pair = std::sync::Barrier::new(2);
        let result = parallel_scan(&paths, 2, |p| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(count, Ordering::SeqCst);
            let index: usize = p.to_str().unwrap().parse().unwrap();
            if index < 2 {
                first_pair.wait();
            }
            active.fetch_sub(1, Ordering::SeqCst);
            vec![index]
        });
        assert_eq!(result, (0..12).collect::<Vec<_>>());
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn discovery_deduplicates_overlapping_roots_and_symlink_cycles() {
        let root = std::env::temp_dir().join(format!("zestbay-scan-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("nested/test.clap"), "").unwrap();
        std::os::unix::fs::symlink(&root, root.join("nested/loop")).unwrap();
        let found = discover(&[root.clone(), root.join("nested")], "clap", true);
        assert_eq!(found, vec![root.join("nested/test.clap")]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
