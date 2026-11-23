use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;

#[derive(Debug)]
pub enum SandboxResult<T> {
    Ok(T),
    #[allow(dead_code)]
    Crashed {
        signal: Option<i32>,
        description: String,
    },
    Timeout,
    #[allow(dead_code)]
    ForkFailed(String),
}

pub fn exec_scan(format: &str, path: &std::path::Path) -> SandboxResult<Vec<crate::plugin::PluginInfo>> {
    use std::os::fd::AsRawFd;
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => return SandboxResult::ForkFailed(e.to_string()),
    };
    let mut child = match Command::new(exe)
        .arg("--scan-plugin")
        .arg(format)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return SandboxResult::ForkFailed(e.to_string()),
    };
    let stdout = child.stdout.take().expect("scanner stdout is piped");
    let (result, data) = collect_child_output(
        child.id() as i32,
        stdout.as_raw_fd(),
        Some(Instant::now() + Duration::from_secs(10)),
    );
    decode_scan_result(result, &data)
}

pub fn run_scan_main(args: &[String]) -> ! {
    use std::os::fd::FromRawFd;
    if args.len() != 2 {
        crate::exit_now(2);
    }
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        if libc::getppid() == 1 { libc::_exit(1); }
    }
    let result_fd = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
    if result_fd < 0 || unsafe { libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) } < 0 {
        crate::exit_now(1);
    }
    let mut result = unsafe { std::fs::File::from_raw_fd(result_fd) };
    let mut plugins = Vec::new();
    let path = std::path::Path::new(&args[1]);
    match args[0].as_str() {
        "clap" => crate::clap::scanner::scan_clap_file(path, &mut plugins),
        "vst3" => crate::vst3::scanner::scan_vst3_bundle(path, &mut plugins),
        "vst2" => crate::vst2::scanner::scan_vst2_file(path, &mut plugins),
        "ladspa" => crate::ladspa::scanner::scan_ladspa_file(path, &mut plugins),
        _ => crate::exit_now(2),
    }
    let ok = serde_json::to_writer(&mut result, &plugins).is_ok() && result.flush().is_ok();
    crate::exit_now(if ok { 0 } else { 1 });
}

fn decode_scan_result<T: for<'de> Deserialize<'de>>(result: WaitResult, data: &[u8]) -> SandboxResult<T> {
    match result {
        WaitResult::Exited(0) => match serde_json::from_slice(data) {
            Ok(value) => SandboxResult::Ok(value),
            Err(e) => SandboxResult::Crashed {
                signal: None,
                description: format!("invalid scanner output: {e}"),
            },
        },
        WaitResult::Exited(code) => SandboxResult::Crashed {
            signal: None,
            description: format!("scanner exited with code {code}"),
        },
        WaitResult::Signaled(sig) => SandboxResult::Crashed {
            signal: Some(sig),
            description: signal_name(sig).to_string(),
        },
        WaitResult::Timeout => SandboxResult::Timeout,
        WaitResult::Error(e) => SandboxResult::ForkFailed(e),
    }
}

#[cfg(test)]
pub fn fork_scan<T, F>(closure: F, timeout: Option<Duration>) -> SandboxResult<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
    F: FnOnce() -> T,
{
    let mut pipe_fds = [0i32; 2];
    if unsafe { libc::pipe2(pipe_fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return SandboxResult::ForkFailed(format!(
            "pipe2() failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let (read_fd, write_fd) = (pipe_fds[0], pipe_fds[1]);

    let pid = unsafe { libc::fork() };

    match pid {
        -1 => {
            unsafe {
                libc::close(read_fd);
                libc::close(write_fd);
            }
            SandboxResult::ForkFailed("fork() failed".to_string())
        }

        0 => {
            unsafe { libc::close(read_fd) };

            unsafe {
                libc::signal(libc::SIGSEGV, libc::SIG_DFL);
                libc::signal(libc::SIGABRT, libc::SIG_DFL);
                libc::signal(libc::SIGBUS, libc::SIG_DFL);
                libc::signal(libc::SIGFPE, libc::SIG_DFL);
            }

            let result = closure();

            if let Ok(json) = serde_json::to_vec(&result) {
                let mut file = unsafe { std::os::unix::io::FromRawFd::from_raw_fd(write_fd) };
                let _: std::io::Result<()> = (|| {
                    let file: &mut std::fs::File = &mut file;
                    file.write_all(&json)?;
                    file.flush()?;
                    Ok(())
                })();
                std::mem::forget(file);
            }

            unsafe { libc::close(write_fd) };

            unsafe { libc::_exit(0) };
        }

        child_pid => {
            unsafe { libc::close(write_fd) };

            let deadline = timeout.map(|d| Instant::now() + d);
            let (wait_result, pipe_data) = collect_child_output(child_pid, read_fd, deadline);
            unsafe { libc::close(read_fd) };

            match wait_result {
                WaitResult::Exited(0) => {
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
                WaitResult::Timeout => SandboxResult::Timeout,
                WaitResult::Error(e) => SandboxResult::ForkFailed(format!("waitpid failed: {}", e)),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Ok,
    Crashed { signal: i32 },
    Failed { code: Option<i32> },
    Interrupted { signal: i32 },
    Timeout { secs: u64 },
    Unavailable(String),
}

impl ProbeOutcome {
    pub fn is_safe(&self) -> bool {
        matches!(self, ProbeOutcome::Ok | ProbeOutcome::Unavailable(_))
    }

    pub fn describe(&self) -> String {
        match self {
            ProbeOutcome::Ok => "passed the safety probe".to_string(),
            ProbeOutcome::Crashed { signal } => {
                format!("crashed during the safety probe ({})", signal_name(*signal))
            }
            ProbeOutcome::Failed { code: Some(c) } => {
                format!("failed to load during the safety probe (exit code {})", c)
            }
            ProbeOutcome::Failed { code: None } => {
                "failed to load during the safety probe".to_string()
            }
            ProbeOutcome::Interrupted { signal } => {
                format!("had its safety probe interrupted ({})", signal_name(*signal))
            }
            ProbeOutcome::Timeout { secs } => {
                format!("did not finish loading within {} s during the safety probe", secs)
            }
            ProbeOutcome::Unavailable(e) => format!("could not be safety-probed ({})", e),
        }
    }
}

pub fn exec_probe_outcome(
    format: &str,
    uri: &str,
    library_path: Option<&str>,
    sample_rate: f64,
    block_length: u32,
    timeout: Option<Duration>,
) -> ProbeOutcome {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            log::error!("sandbox: cannot determine current exe for probe: {}", e);
            return ProbeOutcome::Unavailable(e.to_string());
        }
    };

    let mut cmd = Command::new(&exe);
    cmd.arg("--probe-plugin")
        .arg(format)
        .arg(uri)
        .arg(sample_rate.to_string())
        .arg(block_length.to_string());
    if let Some(lib) = library_path.filter(|l| !l.is_empty()) {
        cmd.arg(lib);
    }
    cmd.env("RUST_LOG", "warn")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::error!("sandbox: failed to spawn probe process: {}", e);
            return ProbeOutcome::Unavailable(e.to_string());
        }
    };

    let (result, stderr_output) = wait_child_capturing_stderr(&mut child, timeout);
    let outcome = probe_outcome_from(result, timeout);

    if !outcome.is_safe() {
        log::warn!("sandbox: probe of {} '{}' {}", format, uri, outcome.describe());
        let text = String::from_utf8_lossy(&stderr_output);
        let lines: Vec<&str> = text.lines().collect();
        for line in &lines[lines.len().saturating_sub(50)..] {
            log::warn!("sandbox: probe stderr: {}", line);
        }
    }
    outcome
}

fn is_fault_signal(signal: i32) -> bool {
    matches!(
        signal,
        libc::SIGSEGV
            | libc::SIGBUS
            | libc::SIGILL
            | libc::SIGFPE
            | libc::SIGABRT
            | libc::SIGSYS
            | libc::SIGTRAP
    )
}

fn probe_outcome_from(result: ChildWait, timeout: Option<Duration>) -> ProbeOutcome {
    use std::os::unix::process::ExitStatusExt;
    match result {
        ChildWait::Exited(status) if status.success() => ProbeOutcome::Ok,
        ChildWait::Exited(status) => match status.signal() {
            Some(sig) if is_fault_signal(sig) => ProbeOutcome::Crashed { signal: sig },
            Some(sig) => ProbeOutcome::Interrupted { signal: sig },
            None => ProbeOutcome::Failed { code: status.code() },
        },
        ChildWait::TimedOut => ProbeOutcome::Timeout {
            secs: timeout.map(|d| d.as_secs()).unwrap_or(0),
        },
        ChildWait::Error(e) => ProbeOutcome::Unavailable(e),
    }
}

const STDERR_CAP: usize = 256 * 1024;

enum ChildWait {
    Exited(ExitStatus),
    TimedOut,
    Error(String),
}

fn wait_child_capturing_stderr(
    child: &mut Child,
    timeout: Option<Duration>,
) -> (ChildWait, Vec<u8>) {
    let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    if let Some(mut stderr) = child.stderr.take() {
        let buf = captured.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                        b.extend_from_slice(&chunk[..n]);
                        if b.len() > STDERR_CAP {
                            let excess = b.len() - STDERR_CAP;
                            b.drain(..excess);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = done_tx.send(());
        });
    } else {
        drop(done_tx);
    }

    let deadline = timeout.map(|d| Instant::now() + d);
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break ChildWait::Exited(status),
            Ok(None) => {
                if deadline.is_some_and(|dl| Instant::now() >= dl) {
                    let _ = child.kill();
                    let _ = child.wait();
                    break ChildWait::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break ChildWait::Error(e.to_string());
            }
        }
    };

    let _ = done_rx.recv_timeout(Duration::from_millis(500));
    let out = std::mem::take(&mut *captured.lock().unwrap_or_else(|e| e.into_inner()));
    (result, out)
}

pub fn run_probe_main(args: &[String]) -> ! {
    if args.len() < 4 {
        eprintln!(
            "probe: usage: --probe-plugin <format> <uri> <sample_rate> <block_length> [library_path]"
        );
        std::process::exit(2);
    }
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        if libc::getppid() == 1 {
            libc::_exit(1);
        }
    }
    let format = &args[0];
    let uri = &args[1];
    let sample_rate: f64 = args[2].parse().unwrap_or(48000.0);
    let block_length: u32 = args[3].parse().unwrap_or(1024);
    let library_path = args.get(4).filter(|s| !s.is_empty());

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
                ladspa_ids: Vec::new(),
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
                    &world,
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
            let info = match library_path {
                Some(lib) => Some(probe_plugin_info(uri, lib, crate::plugin::types::PluginFormat::Clap)),
                None => crate::clap::scanner::find_plugin(uri),
            };
            if let Some(info) = info {
                let _inst = unsafe {
                    crate::clap::host::ClapPluginInstance::new(
                        &info.library_path,
                        uri,
                        &info,
                        sample_rate,
                        None,
                    )
                };
            }
        }
        "vst3" => {
            let info = match library_path {
                Some(lib) => Some(probe_plugin_info(uri, lib, crate::plugin::types::PluginFormat::Vst3)),
                None => crate::vst3::scanner::find_plugin(uri),
            };
            if let Some(info) = info {
                let _inst = unsafe {
                    crate::vst3::host::Vst3PluginInstance::new(
                        &info.library_path,
                        uri,
                        &info,
                        sample_rate,
                        None,
                    )
                };
            }
        }
        "vst2" => {
            let info = match library_path {
                Some(lib) => Some(probe_plugin_info(uri, lib, crate::plugin::types::PluginFormat::Vst2)),
                None => crate::vst2::scanner::find_plugin(uri),
            };
            let Some(info) = info else { crate::exit_now(1); };
            let inst = unsafe {
                crate::vst2::host::Vst2PluginInstance::new(&info.library_path, uri, &info, sample_rate, None)
            };
            let Some(inst) = inst else { crate::exit_now(1); };
            let inputs = vec![vec![0.0; 64]; inst.audio_input_channels];
            let mut outputs = vec![vec![0.0; 64]; inst.audio_output_channels];
            let inputs: Vec<&[f32]> = inputs.iter().map(Vec::as_slice).collect();
            let mut outputs: Vec<&mut [f32]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
            unsafe { inst.process(&inputs, &mut outputs, 64, &[]); }
            drop(inst);
        }
        "ladspa" => {
            let info = match library_path {
                Some(lib) => Some(probe_plugin_info(uri, lib, crate::plugin::types::PluginFormat::Ladspa)),
                None => crate::ladspa::scanner::find_plugin(uri),
            };
            let Some(info) = info else { crate::exit_now(1); };
            let inst = unsafe {
                crate::ladspa::host::LadspaPluginInstance::new(&info.library_path, uri, &info, sample_rate, None)
            };
            let Some(inst) = inst else { crate::exit_now(1); };
            let inputs = vec![vec![0.0; 64]; inst.audio_input_channels];
            let mut outputs = vec![vec![0.0; 64]; inst.audio_output_channels];
            let inputs: Vec<&[f32]> = inputs.iter().map(Vec::as_slice).collect();
            let mut outputs: Vec<&mut [f32]> = outputs.iter_mut().map(Vec::as_mut_slice).collect();
            unsafe { inst.process(&inputs, &mut outputs, 64, &[]); }
            drop(inst);
        }
        other => {
            eprintln!("probe: unknown format '{}'", other);
            std::process::exit(2);
        }
    }

    std::process::exit(0);
}

fn probe_plugin_info(
    uri: &str,
    library_path: &str,
    format: crate::plugin::types::PluginFormat,
) -> crate::plugin::types::PluginInfo {
    crate::plugin::types::PluginInfo {
        uri: uri.to_string(),
        name: uri.to_string(),
        format,
        category: crate::plugin::types::PluginCategory::Other(String::new()),
        author: None,
        ports: Vec::new(),
        audio_inputs: 0,
        audio_outputs: 0,
        control_inputs: 0,
        control_outputs: 0,
        required_features: Vec::new(),
        compatible: true,
        has_ui: false,
        library_path: library_path.to_string(),
        ladspa_ids: Vec::new(),
    }
}

enum WaitResult {
    Exited(i32),
    Signaled(i32),
    Timeout,
    Error(String),
}

fn decode_status(status: i32) -> WaitResult {
    if libc::WIFEXITED(status) {
        WaitResult::Exited(libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        WaitResult::Signaled(libc::WTERMSIG(status))
    } else {
        WaitResult::Exited(-1)
    }
}

fn try_reap(pid: i32) -> Option<WaitResult> {
    loop {
        let mut status = 0i32;
        let ret = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if ret < 0 {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Some(WaitResult::Error(errno.to_string()));
        }
        if ret == pid {
            return Some(decode_status(status));
        }
        return None;
    }
}

fn kill_and_reap(pid: i32) {
    unsafe { libc::kill(pid, libc::SIGKILL) };
    loop {
        let mut status = 0i32;
        let ret = unsafe { libc::waitpid(pid, &mut status, 0) };
        if ret < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        break;
    }
}

fn drain_nonblocking(fd: i32, out: &mut Vec<u8>) -> bool {
    let mut chunk = [0u8; 16384];
    loop {
        let n = unsafe { libc::read(fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n > 0 {
            out.extend_from_slice(&chunk[..n as usize]);
            continue;
        }
        if n == 0 {
            return true;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EINTR) => continue,
            Some(libc::EAGAIN) => return false,
            _ => return true,
        }
    }
}

fn collect_child_output(
    pid: i32,
    read_fd: i32,
    deadline: Option<Instant>,
) -> (WaitResult, Vec<u8>) {
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFL);
        libc::fcntl(read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    let mut data = Vec::new();
    let mut eof = false;
    loop {
        if !eof {
            eof = drain_nonblocking(read_fd, &mut data);
        }

        if let Some(res) = try_reap(pid) {
            if !eof {
                drain_nonblocking(read_fd, &mut data);
            }
            return (res, data);
        }

        let wait_ms = match deadline {
            Some(dl) => {
                let now = Instant::now();
                if now >= dl {
                    kill_and_reap(pid);
                    return (WaitResult::Timeout, data);
                }
                (dl - now).as_millis().min(50) as i32
            }
            None => 50,
        };

        if eof {
            std::thread::sleep(Duration::from_millis(wait_ms.clamp(1, 5) as u64));
        } else {
            let mut pfd = libc::pollfd {
                fd: read_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            unsafe { libc::poll(&mut pfd, 1, wait_ms.max(1)) };
        }
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
        libc::SIGTERM => "SIGTERM (terminated)",
        _ => "unknown signal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_scan_returns_value() {
        match fork_scan(|| vec![1u32, 2, 3], Some(Duration::from_secs(10))) {
            SandboxResult::Ok(v) => assert_eq!(v, vec![1, 2, 3]),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn fork_scan_large_output_succeeds() {
        let res = fork_scan(|| "x".repeat(1024 * 1024), Some(Duration::from_secs(20)));
        match res {
            SandboxResult::Ok(s) => assert_eq!(s.len(), 1024 * 1024),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn fork_scan_hung_child_times_out_promptly() {
        let start = Instant::now();
        let res: SandboxResult<u32> = fork_scan(
            || loop {
                unsafe { libc::sleep(1000) };
            },
            Some(Duration::from_millis(300)),
        );
        assert!(matches!(res, SandboxResult::Timeout), "got {:?}", res);
        assert!(start.elapsed() < Duration::from_secs(5), "took {:?}", start.elapsed());
    }

    #[test]
    fn fork_scan_reports_signal() {
        let res: SandboxResult<u32> = fork_scan(
            || {
                unsafe { libc::raise(libc::SIGSEGV) };
                0
            },
            Some(Duration::from_secs(10)),
        );
        match res {
            SandboxResult::Crashed { signal, .. } => assert_eq!(signal, Some(libc::SIGSEGV)),
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn large_stderr_does_not_hang() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("head -c 2000000 /dev/zero | tr '\\0' 'e' >&2; echo tail-marker >&2; exit 3")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn sh");
        let timeout = Some(Duration::from_secs(20));
        let (res, err) = wait_child_capturing_stderr(&mut child, timeout);
        assert!(err.len() <= STDERR_CAP);
        assert!(String::from_utf8_lossy(&err).contains("tail-marker"));
        assert_eq!(probe_outcome_from(res, timeout), ProbeOutcome::Failed { code: Some(3) });
    }

    #[test]
    fn probe_timeout_is_not_a_crash() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn sleep");
        let start = Instant::now();
        let timeout = Some(Duration::from_millis(300));
        let (res, _) = wait_child_capturing_stderr(&mut child, timeout);
        let outcome = probe_outcome_from(res, timeout);
        assert!(matches!(outcome, ProbeOutcome::Timeout { .. }), "got {:?}", outcome);
        assert!(!outcome.is_safe());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn probe_killed_from_outside_is_not_a_crash() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("kill -KILL $$")
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn sh");
        let (res, _) = wait_child_capturing_stderr(&mut child, Some(Duration::from_secs(10)));
        let outcome = probe_outcome_from(res, None);
        assert_eq!(outcome, ProbeOutcome::Interrupted { signal: libc::SIGKILL });
        assert!(!outcome.is_safe());
        assert!(!matches!(outcome, ProbeOutcome::Crashed { .. }));
        assert!(is_fault_signal(libc::SIGSEGV) && is_fault_signal(libc::SIGABRT));
        assert!(!is_fault_signal(libc::SIGTERM) && !is_fault_signal(libc::SIGINT));
    }

    #[test]
    fn probe_signal_is_crash() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("kill -SEGV $$")
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn sh");
        let (res, _) = wait_child_capturing_stderr(&mut child, Some(Duration::from_secs(10)));
        assert_eq!(
            probe_outcome_from(res, None),
            ProbeOutcome::Crashed { signal: libc::SIGSEGV }
        );
    }
}
