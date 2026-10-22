//! Thread roles and the RT suspend gate shared by the CLAP and VST3 hosts.
//!
//! ZestBay treats the PipeWire main-loop thread as the plugin "main thread":
//! instantiation, parameters, state, GUIs, timers, fd callbacks and run loops
//! all happen there.  The PipeWire data-loop thread that calls `process()` is
//! the audio thread.  Both roles are tracked with thread-locals so that the
//! checks are lock-free and safe to call from the RT thread.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

thread_local! {
    static IS_MAIN: Cell<bool> = const { Cell::new(false) };
    static IS_AUDIO: Cell<bool> = const { Cell::new(false) };
}

/// Mark the calling thread as the plugin main thread.
pub fn set_main_thread() {
    IS_MAIN.with(|c| c.set(true));
}

pub fn is_main_thread() -> bool {
    IS_MAIN.with(|c| c.get())
}

/// Mark the calling thread as an audio thread.  Cheap; called on every
/// `process()` entry.
#[inline]
pub fn mark_audio_thread() {
    IS_AUDIO.with(|c| c.set(true));
}

pub fn is_audio_thread() -> bool {
    IS_AUDIO.with(|c| c.get())
}

/// Run `f` with the calling thread acting as the audio thread.  Only used
/// while the real audio thread is provably not processing the plugin
/// (see [`RtGate::suspend`]), e.g. for `stop_processing` during teardown.
pub fn with_audio_thread<R>(f: impl FnOnce() -> R) -> R {
    let prev = IS_AUDIO.with(|c| c.replace(true));
    let r = f();
    IS_AUDIO.with(|c| c.set(prev));
    r
}

/// Lock-free handshake that lets the main thread exclude the audio thread
/// from a plugin's `process()` (Dekker-style, SeqCst on both sides).
///
/// The audio thread brackets its work with [`enter`](Self::enter) /
/// [`leave`](Self::leave); when `enter` returns false it must not touch the
/// plugin for that block.  The main thread calls [`suspend`](Self::suspend),
/// does its work, then [`resume`](Self::resume).
pub struct RtGate {
    in_process: AtomicBool,
    suspended: AtomicBool,
}

impl Default for RtGate {
    fn default() -> Self {
        Self::new()
    }
}

impl RtGate {
    pub const fn new() -> Self {
        Self {
            in_process: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
        }
    }

    /// Audio thread: returns false if the plugin is suspended.
    #[inline]
    pub fn enter(&self) -> bool {
        self.in_process.store(true, Ordering::SeqCst);
        if self.suspended.load(Ordering::SeqCst) {
            self.in_process.store(false, Ordering::Release);
            return false;
        }
        true
    }

    /// Audio thread: end of a block started with a successful `enter`.
    #[inline]
    pub fn leave(&self) {
        self.in_process.store(false, Ordering::Release);
    }

    /// Main thread: block new process calls and wait for a running one to
    /// finish.  Returns false (and stays resumed) on timeout.
    pub fn suspend(&self, timeout: Duration) -> bool {
        self.suspended.store(true, Ordering::SeqCst);
        let start = Instant::now();
        while self.in_process.load(Ordering::SeqCst) {
            if start.elapsed() > timeout {
                self.suspended.store(false, Ordering::SeqCst);
                return false;
            }
            std::thread::yield_now();
        }
        true
    }

    pub fn resume(&self) {
        self.suspended.store(false, Ordering::SeqCst);
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::SeqCst)
    }
}

/// Copy inputs to outputs (extra outputs are silenced).  Used for bypass and
/// for blocks where the plugin cannot be processed.
pub fn passthrough(inputs: &[&[f32]], outputs: &mut [&mut [f32]], n: usize) {
    for (i, output) in outputs.iter_mut().enumerate() {
        let m = output.len().min(n);
        match inputs.get(i) {
            Some(input) => {
                let k = m.min(input.len());
                output[..k].copy_from_slice(&input[..k]);
                output[k..m].fill(0.0);
            }
            None => output[..m].fill(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn thread_roles_are_per_thread() {
        set_main_thread();
        assert!(is_main_thread());
        let other = std::thread::spawn(|| (is_main_thread(), is_audio_thread()))
            .join()
            .unwrap();
        assert_eq!(other, (false, false));
    }

    #[test]
    fn with_audio_thread_restores_flag() {
        std::thread::spawn(|| {
            assert!(!is_audio_thread());
            with_audio_thread(|| assert!(is_audio_thread()));
            assert!(!is_audio_thread());
            mark_audio_thread();
            assert!(is_audio_thread());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn gate_blocks_enter_while_suspended() {
        let g = RtGate::new();
        assert!(g.enter());
        g.leave();
        assert!(g.suspend(Duration::from_millis(10)));
        assert!(!g.enter());
        g.resume();
        assert!(g.enter());
        g.leave();
    }

    #[test]
    fn gate_suspend_times_out_while_processing() {
        let g = RtGate::new();
        assert!(g.enter());
        assert!(!g.suspend(Duration::from_millis(5)));
        assert!(!g.is_suspended());
        g.leave();
        assert!(g.suspend(Duration::from_millis(5)));
    }

    #[test]
    fn gate_suspend_waits_for_running_block() {
        let g = Arc::new(RtGate::new());
        assert!(g.enter());
        let g2 = g.clone();
        let t = std::thread::spawn(move || g2.suspend(Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(20));
        g.leave();
        assert!(t.join().unwrap());
        assert!(!g.enter());
    }

    #[test]
    fn passthrough_copies_and_silences() {
        let a = [1.0f32, 2.0, 3.0];
        let inputs: [&[f32]; 1] = [&a];
        let mut o1 = [9.0f32; 3];
        let mut o2 = [9.0f32; 3];
        {
            let mut outputs: [&mut [f32]; 2] = [&mut o1, &mut o2];
            passthrough(&inputs, &mut outputs, 3);
        }
        assert_eq!(o1, [1.0, 2.0, 3.0]);
        assert_eq!(o2, [0.0, 0.0, 0.0]);
    }
}
