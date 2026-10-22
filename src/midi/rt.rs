//! RT plumbing shared by the LV2 / CLAP / VST3 plugin filter nodes.
//!
//! Each filter is split into two halves:
//! - `FilterRt`: owned by the PipeWire RT callback (port pointers, scratch
//!   buffers, MIDI toggle state, the event queue producer).  Never touched
//!   by the main thread while the filter is connected.
//! - `FilterMain`: owned by the filter node on the PipeWire main loop
//!   (event queue consumer, retired mapping tables).
//!
//! Both halves share a `FilterShared` that only contains atomics.
//!
//! Nothing on the RT path allocates, locks, frees or sends on a channel.

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};

use super::filter::ResolvedMappings;
use super::processing::{
    LearnFlags, MAX_MIDI_EVENTS, MidiProcessingState, RawMidiEvent, RtEvent, chunk_midi_events,
    extract_midi_events, process_midi_buffer, write_midi_output,
};
use crate::plugin::cpu_stats::PluginTimingSlot;

/// Largest block ever passed to a plugin's `process()`.  Larger cycles are split.
pub const MAX_BLOCK: usize = 8192;

/// Capacity of the per-filter RT -> main event queue.  Events are dropped when full.
pub const RT_EVENT_CAPACITY: usize = 1024;

/// Cycles longer than this are considered bogus and skipped.
const MAX_CYCLE: usize = 1 << 20;

/// Split a cycle of `n` frames into consecutive `(start, len)` blocks of at most `max` frames.
pub fn block_chunks(n: usize, max: usize) -> impl Iterator<Item = (usize, usize)> {
    let max = max.max(1);
    (0..n.div_ceil(max)).map(move |i| {
        let start = i * max;
        (start, (n - start).min(max))
    })
}

/// Frames of the block `[start, start + len)` that lie inside a buffer of
/// `cap` frames (0 for a missing buffer).
#[inline]
pub fn available_frames(missing: bool, cap: usize, start: usize, len: usize) -> usize {
    if missing {
        0
    } else {
        cap.saturating_sub(start).min(len)
    }
}

/// Same as `pw_filter_get_dsp_buffer`, but also reports the buffer capacity
/// in frames, and returns null (instead of PipeWire's fixed-size silence
/// buffer) for a port without buffers.
///
/// # Safety
/// `port` must be null or a port_data pointer of a DSP port of a connected
/// filter; must be called from the RT process callback.
unsafe fn dsp_buffer(
    port: *mut std::ffi::c_void,
    n_samples: usize,
    output: bool,
) -> (*mut f32, usize) {
    if port.is_null() {
        return (std::ptr::null_mut(), 0);
    }
    unsafe {
        let b = pipewire::sys::pw_filter_dequeue_buffer(port);
        if b.is_null() {
            return (std::ptr::null_mut(), 0);
        }
        let mut result = (std::ptr::null_mut(), 0);
        let sb = (*b).buffer;
        if !sb.is_null() && (*sb).n_datas > 0 && !(*sb).datas.is_null() {
            let d = &mut *(*sb).datas;
            if !d.data.is_null() {
                let cap = d.maxsize as usize / std::mem::size_of::<f32>();
                if output && !d.chunk.is_null() {
                    (*d.chunk).offset = 0;
                    (*d.chunk).size = (n_samples.min(cap) * std::mem::size_of::<f32>()) as u32;
                    (*d.chunk).stride = std::mem::size_of::<f32>() as i32;
                    (*d.chunk).flags = 0;
                }
                result = (d.data as *mut f32, cap);
            }
        }
        pipewire::sys::pw_filter_queue_buffer(port, b);
        result
    }
}

// ---------------------------------------------------------------------------
// Lock-free mapping table swap
// ---------------------------------------------------------------------------

/// Holds the current `ResolvedMappings` as a raw `Arc` pointer.  The main
/// thread publishes new tables and keeps the old ones alive (see
/// `FilterMain`) until the RT thread can no longer be using them, so the RT
/// thread never drops the last reference.
pub struct MappingSwap {
    current: AtomicPtr<ResolvedMappings>,
    /// Incremented by the main thread after every swap.
    published: AtomicU64,
    /// Highest epoch the RT thread has finished a cycle with.
    rt_seen: AtomicU64,
    /// RT is between `rt_enter` and `rt_exit`.
    in_cycle: AtomicBool,
}

impl MappingSwap {
    pub fn new(initial: Arc<ResolvedMappings>) -> Self {
        Self {
            current: AtomicPtr::new(Arc::into_raw(initial) as *mut _),
            published: AtomicU64::new(0),
            rt_seen: AtomicU64::new(0),
            in_cycle: AtomicBool::new(false),
        }
    }

    /// Main thread: install `new`, returning the previous table and the epoch
    /// after which it may be released.
    pub fn publish(&self, new: Arc<ResolvedMappings>) -> (u64, Arc<ResolvedMappings>) {
        let old = self
            .current
            .swap(Arc::into_raw(new) as *mut _, Ordering::SeqCst);
        let epoch = self.published.fetch_add(1, Ordering::SeqCst) + 1;
        // SAFETY: `old` came from Arc::into_raw in new()/publish()
        (epoch, unsafe { Arc::from_raw(old) })
    }

    /// RT: start using the table.  The returned pointer stays valid until the
    /// matching `rt_exit`.
    pub fn rt_enter(&self) -> (u64, *const ResolvedMappings) {
        self.in_cycle.store(true, Ordering::SeqCst);
        let epoch = self.published.load(Ordering::SeqCst);
        let ptr = self.current.load(Ordering::SeqCst);
        (epoch, ptr)
    }

    /// RT: done with the table obtained from `rt_enter`.
    pub fn rt_exit(&self, epoch: u64) {
        self.rt_seen.store(epoch, Ordering::SeqCst);
        self.in_cycle.store(false, Ordering::SeqCst);
    }

    /// Main thread: whether a table retired at `epoch` can no longer be in use by RT.
    /// Must be evaluated after the corresponding `publish`.
    fn releasable(&self) -> impl Fn(u64) -> bool {
        let idle = !self.in_cycle.load(Ordering::SeqCst);
        let seen = self.rt_seen.load(Ordering::SeqCst);
        move |epoch| idle || epoch <= seen
    }
}

impl Drop for MappingSwap {
    fn drop(&mut self) {
        let p = *self.current.get_mut();
        if !p.is_null() {
            // SAFETY: from Arc::into_raw; RT is gone once the swap is dropped
            unsafe { drop(Arc::from_raw(p)) };
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// State shared between the RT callback and the main thread (atomics only).
pub struct FilterShared {
    pub shutting_down: AtomicBool,
    /// Set by `state_changed` once the filter reached PAUSED/STREAMING.
    pub ready: AtomicBool,
    pub graph_rate: AtomicU32,
    pub learn: LearnFlags,
    pub mappings: MappingSwap,
}

/// Port data pointers of a filter (null where a port could not be created).
pub struct FilterPorts {
    pub inputs: Vec<*mut std::ffi::c_void>,
    pub outputs: Vec<*mut std::ffi::c_void>,
    pub midi_in: *mut std::ffi::c_void,
    pub midi_out: *mut std::ffi::c_void,
}

/// Create the two halves of a filter.
pub fn new_filter_halves(
    ports: FilterPorts,
    has_midi_in: bool,
    cpu_slot: Arc<PluginTimingSlot>,
) -> (FilterMain, FilterRt) {
    let shared = Arc::new(FilterShared {
        shutting_down: AtomicBool::new(false),
        ready: AtomicBool::new(false),
        graph_rate: AtomicU32::new(0),
        learn: LearnFlags::new(),
        mappings: MappingSwap::new(Arc::new(ResolvedMappings::empty())),
    });
    let (producer, consumer) = rtrb::RingBuffer::new(RT_EVENT_CAPACITY);
    let n_in = ports.inputs.len();
    let n_out = ports.outputs.len();

    let main = FilterMain {
        shared: shared.clone(),
        events: RefCell::new(consumer),
        retired: RefCell::new(Vec::new()),
        node_id_sent: Cell::new(false),
    };
    let rt = FilterRt {
        shared,
        in_bufs: vec![(std::ptr::null(), 0); n_in],
        out_bufs: vec![(std::ptr::null_mut(), 0); n_out],
        scratch: vec![0.0; (n_in + n_out) * MAX_BLOCK],
        in_slices: Vec::with_capacity(n_in),
        out_slices: Vec::with_capacity(n_out),
        ports,
        has_midi_in,
        cpu_slot,
        midi_state: MidiProcessingState::new(),
        events: producer,
        midi_events: [RawMidiEvent::EMPTY; MAX_MIDI_EVENTS],
        chunk_events: [RawMidiEvent::EMPTY; MAX_MIDI_EVENTS],
        last_rate: 0,
    };
    (main, rt)
}

// ---------------------------------------------------------------------------
// Main-thread half
// ---------------------------------------------------------------------------

pub struct FilterMain {
    pub shared: Arc<FilterShared>,
    events: RefCell<rtrb::Consumer<RtEvent>>,
    retired: RefCell<Vec<(u64, Arc<ResolvedMappings>)>>,
    node_id_sent: Cell<bool>,
}

impl FilterMain {
    pub fn graph_rate(&self) -> u32 {
        self.shared.graph_rate.load(Ordering::Relaxed)
    }

    pub fn set_learn_mode(&self, enabled: bool) {
        self.shared.learn.set(enabled);
    }

    pub fn set_shutting_down(&self) {
        self.shared.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn update_mappings(&self, mappings: Arc<ResolvedMappings>) {
        let retired = self.shared.mappings.publish(mappings);
        self.retired.borrow_mut().push(retired);
        self.release_retired();
    }

    /// Drop retired mapping tables the RT thread can no longer reference.
    fn release_retired(&self) {
        let mut retired = self.retired.borrow_mut();
        if retired.is_empty() {
            return;
        }
        let releasable = self.shared.mappings.releasable();
        retired.retain(|(epoch, _)| !releasable(*epoch));
    }

    /// Convert queued RT events into `PwEvent`s.  `node_id` is only called
    /// until the PluginAdded event has been sent.
    pub fn drain_events(
        &self,
        event_tx: &std::sync::mpsc::Sender<crate::pipewire::PwEvent>,
        instance_id: u64,
        display_name: &str,
        node_id: impl Fn() -> u32,
    ) {
        use crate::pipewire::{PluginEvent, PwEvent};

        if !self.node_id_sent.get() && self.shared.ready.load(Ordering::Acquire) {
            let id = node_id();
            if id != 0 && id != u32::MAX {
                log::info!(
                    "Plugin filter node ID resolved: instance {} -> pw_node {}",
                    instance_id,
                    id
                );
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginAdded {
                    instance_id,
                    pw_node_id: id,
                    display_name: display_name.to_string(),
                }));
                self.node_id_sent.set(true);
            }
        }

        let mut events = self.events.borrow_mut();
        while let Ok(ev) = events.pop() {
            let pw_event = match ev {
                RtEvent::ParameterChanged {
                    instance_id,
                    port_index,
                    value,
                } => PluginEvent::ParameterChanged {
                    instance_id,
                    port_index,
                    value,
                },
                RtEvent::LearnCaptured {
                    channel,
                    number,
                    message_type,
                } => PluginEvent::MidiCcReceived {
                    device_name: display_name.to_string(),
                    channel,
                    cc: number,
                    message_type,
                },
            };
            let _ = event_tx.send(PwEvent::Plugin(pw_event));
        }
        drop(events);

        self.release_retired();
    }
}

// ---------------------------------------------------------------------------
// RT half
// ---------------------------------------------------------------------------

pub struct FilterRt {
    shared: Arc<FilterShared>,
    ports: FilterPorts,
    has_midi_in: bool,
    cpu_slot: Arc<PluginTimingSlot>,
    midi_state: MidiProcessingState,
    events: rtrb::Producer<RtEvent>,
    /// Port buffers for the current cycle with their capacity in frames.
    in_bufs: Vec<(*const f32, usize)>,
    out_bufs: Vec<(*mut f32, usize)>,
    /// `MAX_BLOCK` frames per port: inputs first, then outputs.  Used for
    /// ports without a buffer (or with one too small for the cycle).
    scratch: Vec<f32>,
    /// Preallocated slice lists handed to the plugin; emptied after every
    /// cycle so no reference outlives the buffers.
    in_slices: Vec<&'static [f32]>,
    out_slices: Vec<&'static mut [f32]>,
    midi_events: [RawMidiEvent; MAX_MIDI_EVENTS],
    chunk_events: [RawMidiEvent; MAX_MIDI_EVENTS],
    last_rate: u32,
}

impl FilterRt {
    /// Run one PipeWire process cycle.  `process` is called once per block of
    /// at most `MAX_BLOCK` frames with (inputs, outputs, frames, midi events)
    /// and returns the worker time (ns) spent on behalf of that block.
    ///
    /// # Safety
    /// Must be called from the filter's RT process callback with the
    /// `position` pointer PipeWire passed in.
    pub unsafe fn run_cycle<F>(
        &mut self,
        position: *mut libspa::sys::spa_io_position,
        mut process: F,
    ) where
        F: FnMut(&[&[f32]], &mut [&mut [f32]], usize, &[RawMidiEvent]) -> u64,
    {
        if self.shared.shutting_down.load(Ordering::Acquire) || position.is_null() {
            return;
        }

        let (n_samples, rate) = unsafe {
            (
                (*position).clock.duration as usize,
                (*position).clock.rate.denom,
            )
        };
        if n_samples == 0 || n_samples > MAX_CYCLE {
            return;
        }
        if rate != 0 && rate != self.last_rate {
            self.last_rate = rate;
            self.shared.graph_rate.store(rate, Ordering::Relaxed);
        }

        let mut n_midi_events = 0usize;
        unsafe {
            let midi_in_buf = if !self.ports.midi_in.is_null() {
                pipewire::sys::pw_filter_get_dsp_buffer(self.ports.midi_in, 0)
            } else {
                std::ptr::null_mut()
            };

            if self.has_midi_in {
                n_midi_events = extract_midi_events(midi_in_buf, &mut self.midi_events);
            }

            if !midi_in_buf.is_null() {
                let (epoch, mappings) = self.shared.mappings.rt_enter();
                if !mappings.is_null() {
                    process_midi_buffer(
                        midi_in_buf,
                        &mut self.midi_state,
                        &self.shared.learn,
                        &*mappings,
                        &mut self.events,
                    );
                }
                self.shared.mappings.rt_exit(epoch);
            }

            write_midi_output(self.ports.midi_out, midi_in_buf);

            for (i, port) in self.ports.inputs.iter().enumerate() {
                let (p, cap) = dsp_buffer(*port, n_samples, false);
                self.in_bufs[i] = (p as *const f32, cap);
            }
            for (i, port) in self.ports.outputs.iter().enumerate() {
                self.out_bufs[i] = dsp_buffer(*port, n_samples, true);
            }
        }

        let n_in = self.in_bufs.len();
        let t0 = std::time::Instant::now();
        let mut worker_ns = 0u64;
        for (start, len) in block_chunks(n_samples, MAX_BLOCK) {
            self.in_slices.clear();
            self.out_slices.clear();
            let scratch = self.scratch.as_mut_ptr();

            // SAFETY (both loops): a port buffer is only addressed within its
            // capacity; scratch region k holds MAX_BLOCK >= len frames.
            for (i, &(p, cap)) in self.in_bufs.iter().enumerate() {
                let avail = available_frames(p.is_null(), cap, start, len);
                let s: &'static [f32] = unsafe {
                    if avail == len {
                        std::slice::from_raw_parts(p.add(start), len)
                    } else {
                        let region = scratch.add(i * MAX_BLOCK);
                        if avail > 0 {
                            std::ptr::copy_nonoverlapping(p.add(start), region, avail);
                        }
                        std::ptr::write_bytes(region.add(avail), 0, len - avail);
                        std::slice::from_raw_parts(region, len)
                    }
                };
                self.in_slices.push(s);
            }
            for (j, &(p, cap)) in self.out_bufs.iter().enumerate() {
                let avail = available_frames(p.is_null(), cap, start, len);
                let s: &'static mut [f32] = unsafe {
                    if avail == len {
                        std::slice::from_raw_parts_mut(p.add(start), len)
                    } else {
                        std::slice::from_raw_parts_mut(scratch.add((n_in + j) * MAX_BLOCK), len)
                    }
                };
                self.out_slices.push(s);
            }

            let n_ev = chunk_midi_events(
                &self.midi_events[..n_midi_events],
                start,
                len,
                n_samples,
                &mut self.chunk_events,
            );
            worker_ns += process(
                &self.in_slices,
                &mut self.out_slices,
                len,
                &self.chunk_events[..n_ev],
            );
            self.out_slices.clear();

            // Outputs whose buffer only partly covers this block were
            // rendered into scratch: copy back what fits.
            for (j, &(p, cap)) in self.out_bufs.iter().enumerate() {
                let avail = available_frames(p.is_null(), cap, start, len);
                if avail > 0 && avail < len {
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            scratch.add((n_in + j) * MAX_BLOCK),
                            p.add(start),
                            avail,
                        );
                    }
                }
            }
        }
        self.in_slices.clear();
        self.out_slices.clear();

        let elapsed = t0.elapsed().as_nanos() as u64;
        self.cpu_slot
            .record(elapsed, worker_ns, n_samples as u32, rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_chunks_small_cycle_is_one_block() {
        let v: Vec<_> = block_chunks(256, MAX_BLOCK).collect();
        assert_eq!(v, vec![(0, 256)]);
    }

    #[test]
    fn block_chunks_exact_max() {
        let v: Vec<_> = block_chunks(MAX_BLOCK, MAX_BLOCK).collect();
        assert_eq!(v, vec![(0, MAX_BLOCK)]);
    }

    #[test]
    fn block_chunks_splits_large_cycle() {
        let v: Vec<_> = block_chunks(20000, 8192).collect();
        assert_eq!(v, vec![(0, 8192), (8192, 8192), (16384, 20000 - 16384)]);
        assert_eq!(v.iter().map(|c| c.1).sum::<usize>(), 20000);
    }

    #[test]
    fn available_frames_bounds() {
        assert_eq!(available_frames(true, 8192, 0, 256), 0);
        assert_eq!(available_frames(false, 8192, 0, 256), 256);
        assert_eq!(available_frames(false, 10000, 8192, 8192), 10000 - 8192);
        assert_eq!(available_frames(false, 8192, 8192, 100), 0);
        assert_eq!(available_frames(false, 4096, 8192, 100), 0);
    }

    #[test]
    fn block_chunks_empty() {
        assert_eq!(block_chunks(0, 8192).count(), 0);
    }

    #[test]
    fn mapping_swap_retires_until_rt_moves_on() {
        let swap = MappingSwap::new(Arc::new(ResolvedMappings::empty()));
        // RT is mid-cycle with the initial table
        let (e0, p0) = swap.rt_enter();
        let initial = p0;

        let (epoch, old) = swap.publish(Arc::new(ResolvedMappings::empty()));
        assert_eq!(Arc::as_ptr(&old), initial);
        assert!(!swap.releasable()(epoch));

        // RT finishes the old cycle: still possibly using an epoch < retired
        swap.rt_exit(e0);
        // ...but it is idle now, so releasing is safe
        assert!(swap.releasable()(epoch));

        // A new cycle sees the new table and the new epoch
        let (e1, p1) = swap.rt_enter();
        assert_eq!(e1, epoch);
        assert_ne!(p1, initial);
        // Conservative while a cycle is running
        assert!(!swap.releasable()(epoch));
        swap.rt_exit(e1);
        assert!(swap.releasable()(epoch));
    }

    #[test]
    fn mapping_swap_in_cycle_blocks_release_of_newer_epochs() {
        let swap = MappingSwap::new(Arc::new(ResolvedMappings::empty()));
        let (e, _) = swap.rt_enter();
        swap.rt_exit(e);
        let (e_in, _) = swap.rt_enter();
        let (epoch, _old) = swap.publish(Arc::new(ResolvedMappings::empty()));
        assert!(epoch > e_in);
        assert!(!swap.releasable()(epoch));
        swap.rt_exit(e_in);
        assert!(swap.releasable()(epoch));
    }
}
