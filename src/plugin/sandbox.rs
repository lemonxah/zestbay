//! Fork-based plugin sandbox ("cage") for crash isolation.
//!
//! Plugins loaded via `dlopen` can segfault, which kills the entire process.
//! `std::panic::catch_unwind` only catches Rust panics, not OS signals like
//! SIGSEGV.  This module provides fork-based isolation: the dangerous work
//! runs in a short-lived child process.  If the child segfaults, the parent
//! observes a non-zero exit status and continues running.
//!
//! Two main entry points:
//!
//! - [`fork_scan`] — Run a closure in a forked child and return serialized
//!   results via a pipe.  Used for plugin scanning where the result is plain
//!   data (e.g. `Vec<PluginInfo>`).
//!
//! - [`fork_probe`] — Run a closure in a forked child and report whether it
//!   completed without crashing.  Used to "test" whether instantiating a
//!   plugin is safe before doing it for real in the host process.

use std::io::{Read, Write};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Outcome of a forked operation.
#[derive(Debug)]
pub enum SandboxResult<T> {
    /// The child completed successfully and returned data.
    Ok(T),
    /// The child crashed (segfault, abort, killed by signal).
    Crashed {
        signal: Option<i32>,
        description: String,
    },
    /// The child timed out and was killed.
    Timeout,
    /// The fork or pipe setup itself failed.
    ForkFailed(String),
}

/// Run a closure in a forked child process and collect its serialized return
/// value via a pipe.
///
/// The closure receives no arguments and must return a `T: Serialize`.
/// The child serializes `T` to JSON, writes it to a pipe, and exits.
/// The parent reads the pipe and deserializes.
///
/// If the child segfaults or is killed by a signal, `SandboxResult::Crashed`
/// is returned.  The parent process is unaffected.
///
/// # Timeout
///
/// If `timeout` is `Some(duration)`, the parent will kill the child after
/// that duration and return `SandboxResult::Timeout`.
///
/// # Safety
///
/// This function calls `libc::fork()`.  After fork, the child must not use
/// any async runtime, multi-threaded state, or lock-dependent code from the
/// parent.  The closure should only do self-contained work (dlopen, C FFI
/// calls, etc.) which is exactly what plugin scanning does.
pub fn fork_scan<T, F>(closure: F, timeout: Option<Duration>) -> SandboxResult<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
    F: FnOnce() -> T,
{
    // Create a pipe for child → parent communication
    let mut pipe_fds = [0i32; 2];
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return SandboxResult::ForkFailed("pipe() failed".to_string());
    }
    let (read_fd, write_fd) = (pipe_fds[0], pipe_fds[1]);

    let pid = unsafe { libc::fork() };

    match pid {
        -1 => {
            // Fork failed
            unsafe {
                libc::close(read_fd);
                libc::close(write_fd);
            }
            SandboxResult::ForkFailed("fork() failed".to_string())
        }

        0 => {
            // ===== CHILD PROCESS =====
            // Close read end
            unsafe { libc::close(read_fd) };

            // Reset signal handlers to default so segfaults terminate the child
            // (in case the parent installed custom handlers)
            unsafe {
                libc::signal(libc::SIGSEGV, libc::SIG_DFL);
                libc::signal(libc::SIGABRT, libc::SIG_DFL);
                libc::signal(libc::SIGBUS, libc::SIG_DFL);
                libc::signal(libc::SIGFPE, libc::SIG_DFL);
            }

            // Run the closure
            let result = closure();

            // Serialize and write to pipe
            if let Ok(json) = serde_json::to_vec(&result) {
                let mut file = unsafe { std::os::unix::io::FromRawFd::from_raw_fd(write_fd) };
                let _: std::io::Result<()> = (|| {
                    let file: &mut std::fs::File = &mut file;
                    file.write_all(&json)?;
                    file.flush()?;
                    Ok(())
                })();
                // Don't drop the File — we'll _exit immediately
                std::mem::forget(file);
            }

            unsafe { libc::close(write_fd) };

            // Use _exit to avoid running destructors / atexit handlers in the
            // child, which could corrupt shared state (PipeWire, Qt, etc.)
            unsafe { libc::_exit(0) };
        }

        child_pid => {
            // ===== PARENT PROCESS =====
            // Close write end
            unsafe { libc::close(write_fd) };

            // Set up timeout if requested
            let deadline = timeout.map(|d| std::time::Instant::now() + d);

            // Wait for child with optional timeout
            let wait_result = wait_for_child(child_pid, deadline);

            // Read pipe data regardless of wait result (child may have written
            // partial data before crashing)
            let mut pipe_data = Vec::new();
            {
                let mut file: std::fs::File =
                    unsafe { std::os::unix::io::FromRawFd::from_raw_fd(read_fd) };
                let _ = file.read_to_end(&mut pipe_data);
                // File will close read_fd on drop
            }

            match wait_result {
                WaitResult::Exited(0) => {
                    // Child exited successfully — deserialize the result
                    match serde_json::from_slice(&pipe_data) {
                        Ok(value) => SandboxResult::Ok(value),
                        Err(e) => SandboxResult::Crashed {
                            signal: None,
                            description: format!(
                                "child exited OK but output deserialization failed: {}",
                                e
                            ),
                        },
                    }
                }
                WaitResult::Exited(code) => SandboxResult::Crashed {
                    signal: None,
                    description: format!("child exited with code {}", code),
                },
                WaitResult::Signaled(sig) => SandboxResult::Crashed {
                    signal: Some(sig),
                    description: format!("child killed by signal {} ({})", sig, signal_name(sig)),
                },
                WaitResult::Timeout => {
                    // Kill the child
                    unsafe {
                        libc::kill(child_pid, libc::SIGKILL);
                    }
                    // Reap the zombie
                    let mut status = 0i32;
                    unsafe {
                        libc::waitpid(child_pid, &mut status, 0);
                    }
                    SandboxResult::Timeout
                }
                WaitResult::Error(e) => SandboxResult::ForkFailed(format!("waitpid failed: {}", e)),
            }
        }
    }
}

/// Run a closure in a forked child to test whether it crashes.
///
/// Returns `true` if the child exited normally (exit code 0),
/// `false` if it crashed, timed out, or failed.
///
/// This is useful as a "probe" before actually instantiating a plugin
/// in the host process: if the probe child survives, the plugin is
/// likely safe to load.
pub fn fork_probe<F>(closure: F, timeout: Option<Duration>) -> bool
where
    F: FnOnce(),
{
    let pid = unsafe { libc::fork() };

    match pid {
        -1 => {
            log::error!("sandbox: fork() failed for probe");
            false
        }

        0 => {
            // ===== CHILD PROCESS =====
            unsafe {
                libc::signal(libc::SIGSEGV, libc::SIG_DFL);
                libc::signal(libc::SIGABRT, libc::SIG_DFL);
                libc::signal(libc::SIGBUS, libc::SIG_DFL);
                libc::signal(libc::SIGFPE, libc::SIG_DFL);
            }

            closure();

            unsafe { libc::_exit(0) };
        }

        child_pid => {
            // ===== PARENT PROCESS =====
            let deadline = timeout.map(|d| std::time::Instant::now() + d);
            let wait_result = wait_for_child(child_pid, deadline);

            match wait_result {
                WaitResult::Exited(0) => true,
                WaitResult::Exited(code) => {
                    log::warn!("sandbox: probe child exited with code {}", code);
                    false
                }
                WaitResult::Signaled(sig) => {
                    log::warn!(
                        "sandbox: probe child killed by signal {} ({})",
                        sig,
                        signal_name(sig)
                    );
                    false
                }
                WaitResult::Timeout => {
                    log::warn!("sandbox: probe child timed out, killing");
                    unsafe {
                        libc::kill(child_pid, libc::SIGKILL);
                    }
                    let mut status = 0i32;
                    unsafe {
                        libc::waitpid(child_pid, &mut status, 0);
                    }
                    false
                }
                WaitResult::Error(e) => {
                    log::error!("sandbox: probe waitpid failed: {}", e);
                    false
                }
            }
        }
    }
}

/// Probe a plugin by spawning a **clean** child process via `fork + exec`.
///
/// Unlike [`fork_probe`], this does *not* inherit the parent's in-memory
/// state (PipeWire connections, lilv worlds, Qt, etc.), so plugins that rely
/// on clean global state will not false-positive as crashed.
///
/// `format` must be one of `"lv2"`, `"clap"`, or `"vst3"`.
/// `uri` is the plugin URI.
/// `sample_rate` is passed to the probe for instantiation.
/// `block_length` is only used for LV2 probes.
///
/// Returns `true` if the probe process exited successfully.
pub fn exec_probe(
    format: &str,
    uri: &str,
    sample_rate: f64,
    block_length: u32,
    timeout: Option<Duration>,
) -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log::error!("sandbox: cannot determine current exe for probe: {}", e);
            return true; // fail-open: allow the plugin
        }
    };

    let mut child = match Command::new(&exe)
        .arg("--probe-plugin")
        .arg(format)
        .arg(uri)
        .arg(sample_rate.to_string())
        .arg(block_length.to_string())
        .env("RUST_LOG", "debug")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("sandbox: failed to spawn probe process: {}", e);
            return true; // fail-open
        }
    };

    let result = if let Some(dur) = timeout {
        // Poll with timeout
        let deadline = std::time::Instant::now() + dur;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        log::warn!("sandbox: probe process timed out, killing");
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    log::error!("sandbox: probe wait error: {}", e);
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        }
    } else {
        child.wait().ok()
    };

    // Capture stderr for diagnostics
    let stderr_output = child
        .stderr
        .take()
        .and_then(|mut e| {
            let mut buf = String::new();
            e.read_to_string(&mut buf).ok().map(|_| buf)
        })
        .unwrap_or_default();

    match result {
        Some(status) if status.success() => true,
        Some(status) => {
            log::warn!("sandbox: probe process exited with {:?}", status);
            if !stderr_output.is_empty() {
                for line in stderr_output.lines().take(50) {
                    log::warn!("sandbox: probe stderr: {}", line);
                }
            }
            false
        }
        None => {
            log::warn!("sandbox: probe process timed out or failed");
            false
        }
    }
}

/// Entry point for `--probe-plugin` subprocess.
///
/// Call this from `main()` when `--probe-plugin` is detected.
/// It will instantiate the plugin and exit.  Never returns on success
/// (calls `std::process::exit`).
pub fn run_probe_main(args: &[String]) -> ! {
    // args: [format, uri, sample_rate, block_length]
    if args.len() < 4 {
        eprintln!("probe: usage: --probe-plugin <format> <uri> <sample_rate> <block_length>");
        std::process::exit(2);
    }
    let format = &args[0];
    let uri = &args[1];
    let sample_rate: f64 = args[2].parse().unwrap_or(48000.0);
    let block_length: u32 = args[3].parse().unwrap_or(1024);

    match format.as_str() {
        "lv2" => {
            let world = lilv::World::with_load_all();
            let uri_node = world.new_uri(uri);
            let lilv_plugin = world
                .plugins()
                .iter()
                .find(|p| p.uri().as_uri() == uri_node.as_uri());
            let lp = match lilv_plugin {
                Some(p) => p,
                None => {
                    eprintln!("probe: LV2 plugin not found: {}", uri);
                    std::process::exit(1);
                }
            };
            // Only classify ports for the target plugin — avoid scanning every
            // LV2 plugin on the system, which can corrupt memory if any other
            // plugin's .so is buggy.
            let classification = match crate::lv2::scanner::classify_lv2_ports(&world, &lp) {
                Some(c) => c,
                None => {
                    eprintln!("probe: failed to classify ports for {}", uri);
                    std::process::exit(1);
                }
            };
            let required_features: Vec<String> = lp
                .required_features()
                .iter()
                .filter_map(|n| n.as_uri().map(String::from))
                .collect();
            let info = crate::lv2::Lv2PluginInfo {
                uri: uri.to_string(),
                name: lp.name().as_str().unwrap_or("").to_string(),
                category: crate::lv2::Lv2PluginCategory::from_class_label(
                    lp.class().label().as_str().unwrap_or("Plugin"),
                ),
                author: lp.author_name().and_then(|n| n.as_str().map(String::from)),
                ports: classification.ports,
                audio_inputs: classification.audio_inputs,
                audio_outputs: classification.audio_outputs,
                control_inputs: classification.control_inputs,
                control_outputs: classification.control_outputs,
                required_features,
                compatible: true,
                has_ui: false,
                format: crate::lv2::PluginFormat::Lv2,
                library_path: String::new(),
            };
            eprintln!(
                "probe: LV2 plugin found: {} (ports: {} audio_in, {} audio_out, {} ctrl_in)",
                info.name, info.audio_inputs, info.audio_outputs, info.control_inputs
            );
            eprintln!("probe: required features: {:?}", info.required_features);
            eprintln!(
                "probe: instantiating with sr={} bl={}",
                sample_rate, block_length
            );
            let urid_mapper =
                std::sync::Arc::new(crate::lv2::urid::UridMapper::new());
            let _inst = unsafe {
                crate::lv2::host::Lv2PluginInstance::new(
                    world,
                    &lp,
                    &info,
                    sample_rate,
                    block_length,
                    &urid_mapper,
                )
            };
            eprintln!("probe: instantiation completed successfully");
        }
        "clap" => {
            let all_clap = crate::clap::scanner::scan_plugins();
            if let Some(info) = all_clap.iter().find(|p| p.uri == *uri) {
                let _inst = unsafe {
                    crate::clap::host::ClapPluginInstance::new(
                        &info.library_path,
                        uri,
                        info,
                        sample_rate,
                    )
                };
            }
        }
        "vst3" => {
            let all_vst3 = crate::vst3::scanner::scan_plugins();
            if let Some(info) = all_vst3.iter().find(|p| p.uri == *uri) {
                let _inst = unsafe {
                    crate::vst3::host::Vst3PluginInstance::new(
                        &info.library_path,
                        uri,
                        info,
                        sample_rate,
                    )
                };
            }
        }
        other => {
            eprintln!("probe: unknown format '{}'", other);
            std::process::exit(2);
        }
    }

    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

enum WaitResult {
    Exited(i32),
    Signaled(i32),
    Timeout,
    Error(String),
}

fn wait_for_child(pid: i32, deadline: Option<std::time::Instant>) -> WaitResult {
    loop {
        let mut status = 0i32;
        let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };

        if ret < 0 {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return WaitResult::Error(errno.to_string());
        }

        if ret == pid {
            // Child finished
            if libc::WIFEXITED(status) {
                return WaitResult::Exited(libc::WEXITSTATUS(status));
            }
            if libc::WIFSIGNALED(status) {
                return WaitResult::Signaled(libc::WTERMSIG(status));
            }
            return WaitResult::Exited(-1);
        }

        // ret == 0 means child still running
        if let Some(dl) = deadline {
            if std::time::Instant::now() >= dl {
                return WaitResult::Timeout;
            }
        }

        // Sleep briefly before polling again
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn signal_name(sig: i32) -> &'static str {
    match sig {
        libc::SIGSEGV => "SIGSEGV (segmentation fault)",
        libc::SIGABRT => "SIGABRT (aborted)",
        libc::SIGBUS => "SIGBUS (bus error)",
        libc::SIGFPE => "SIGFPE (floating point exception)",
        libc::SIGKILL => "SIGKILL (killed)",
        libc::SIGILL => "SIGILL (illegal instruction)",
        _ => "unknown signal",
    }
}
