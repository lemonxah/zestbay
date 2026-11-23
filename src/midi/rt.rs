use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};

use super::filter::ResolvedMappings;
use super::processing::{
    LearnFlags, MAX_MIDI_EVENTS, MidiProcessingState, MidiSink, RawMidiEvent, RtEvent,
    chunk_midi_events, extract_midi_events, process_midi_buffer, write_midi_output,
    write_midi_sequence,
};
use crate::plugin::cpu_stats::PluginTimingSlot;

pub const MAX_BLOCK: usize = 8192;

pub const RT_EVENT_CAPACITY: usize = 1024;

const MAX_CYCLE: usize = 1 << 20;

pub fn block_chunks(n: usize, max: usize) -> impl Iterator<Item = (usize, usize)> {
    let max = max.max(1);
    (0..n.div_ceil(max)).map(move |i| {
        let start = i * max;
        (start, (n - start).min(max))
    })
}

#[inline]
pub fn available_frames(missing: bool, cap: usize, start: usize, len: usize) -> usize {
    if missing {
        0
    } else {
        cap.saturating_sub(start).min(len)
    }
}

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

pub struct MappingSwap {
    current: AtomicPtr<ResolvedMappings>,
    published: AtomicU64,
    rt_seen: AtomicU64,
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

    pub fn publish(&self, new: Arc<ResolvedMappings>) -> (u64, Arc<ResolvedMappings>) {
        let old = self
            .current
            .swap(Arc::into_raw(new) as *mut _, Ordering::SeqCst);
        let epoch = self.published.fetch_add(1, Ordering::SeqCst) + 1;
        (epoch, unsafe { Arc::from_raw(old) })
    }

    pub fn rt_enter(&self) -> (u64, *const ResolvedMappings) {
        self.in_cycle.store(true, Ordering::SeqCst);
        let epoch = self.published.load(Ordering::SeqCst);
        let ptr = self.current.load(Ordering::SeqCst);
        (epoch, ptr)
    }

    pub fn rt_exit(&self, epoch: u64) {
        self.rt_seen.store(epoch, Ordering::SeqCst);
        self.in_cycle.store(false, Ordering::SeqCst);
    }

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
            unsafe { drop(Arc::from_raw(p)) };
        }
    }
}

pub struct FilterShared {
    pub shutting_down: AtomicBool,
    pub ready: AtomicBool,
    pub graph_rate: AtomicU32,
    pub enabled: AtomicBool,
    pub learn: LearnFlags,
    pub mappings: MappingSwap,
}

pub struct FilterPorts {
    pub inputs: Vec<*mut std::ffi::c_void>,
    pub outputs: Vec<*mut std::ffi::c_void>,
    pub midi_in: *mut std::ffi::c_void,
    pub midi_outs: Vec<*mut std::ffi::c_void>,
}

pub fn add_midi_ports(
    ports: &mut FilterPorts,
    display_name: &str,
    plugin_midi_outputs: &[String],
    mut add_port: impl FnMut(libspa::sys::spa_direction, &std::ffi::CString) -> *mut std::ffi::c_void,
) {
    let name = std::ffi::CString::new("midi_in").unwrap();
    ports.midi_in = add_port(libspa::sys::SPA_DIRECTION_INPUT, &name);
    if ports.midi_in.is_null() {
        log::error!("Failed to add MIDI input port for {}", display_name);
    }
    for out_name in super::processing::midi_out_port_names(plugin_midi_outputs) {
        let name = std::ffi::CString::new(out_name.as_str()).unwrap();
        let port = add_port(libspa::sys::SPA_DIRECTION_OUTPUT, &name);
        if port.is_null() {
            log::error!("Failed to add MIDI output port '{}' for {}", out_name, display_name);
        }
        ports.midi_outs.push(port);
    }
}

pub fn new_filter_halves(
    ports: FilterPorts,
    has_midi_in: bool,
    plugin_midi_outs: usize,
    cpu_slot: Arc<PluginTimingSlot>,
    enabled: bool,
) -> (FilterMain, FilterRt) {
    let shared = Arc::new(FilterShared {
        shutting_down: AtomicBool::new(false),
        ready: AtomicBool::new(false),
        graph_rate: AtomicU32::new(0),
        enabled: AtomicBool::new(enabled),
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
        midi_sink: MidiSink::new(plugin_midi_outs),
        midi_state: MidiProcessingState::new(),
        events: producer,
        midi_events: [RawMidiEvent::EMPTY; MAX_MIDI_EVENTS],
        chunk_events: [RawMidiEvent::EMPTY; MAX_MIDI_EVENTS],
        last_rate: 0,
    };
    (main, rt)
}

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

    pub fn set_enabled(&self, enabled: bool) {
        self.shared.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed)
    }

    pub fn set_shutting_down(&self) {
        self.shared.shutting_down.store(true, Ordering::SeqCst);
    }

    pub fn update_mappings(&self, mappings: Arc<ResolvedMappings>) {
        let retired = self.shared.mappings.publish(mappings);
        self.retired.borrow_mut().push(retired);
        self.release_retired();
    }

    fn release_retired(&self) {
        let mut retired = self.retired.borrow_mut();
        if retired.is_empty() {
            return;
        }
        let releasable = self.shared.mappings.releasable();
        retired.retain(|(epoch, _)| !releasable(*epoch));
    }

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

pub struct FilterRt {
    shared: Arc<FilterShared>,
    ports: FilterPorts,
    has_midi_in: bool,
    cpu_slot: Arc<PluginTimingSlot>,
    midi_sink: MidiSink,
    midi_state: MidiProcessingState,
    events: rtrb::Producer<RtEvent>,
    in_bufs: Vec<(*const f32, usize)>,
    out_bufs: Vec<(*mut f32, usize)>,
    scratch: Vec<f32>,
    in_slices: Vec<&'static [f32]>,
    out_slices: Vec<&'static mut [f32]>,
    midi_events: [RawMidiEvent; MAX_MIDI_EVENTS],
    chunk_events: [RawMidiEvent; MAX_MIDI_EVENTS],
    last_rate: u32,
}

impl FilterRt {
    pub unsafe fn run_cycle<F>(
        &mut self,
        position: *mut libspa::sys::spa_io_position,
        mut process: F,
    ) where
        F: FnMut(&[&[f32]], &mut [&mut [f32]], usize, &[RawMidiEvent], &mut MidiSink) -> u64,
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

            if !self.shared.enabled.load(Ordering::Relaxed) {
                for &out in &self.ports.midi_outs {
                    write_midi_sequence(out, &[]);
                }
                for port in &self.ports.outputs {
                    let (p, cap) = dsp_buffer(*port, n_samples, true);
                    if !p.is_null() {
                        std::ptr::write_bytes(p, 0, n_samples.min(cap));
                    }
                }
                self.cpu_slot.record(0, 0, n_samples as u32, rate);
                return;
            }

            if self.midi_sink.port_count() == 0
                && let Some(&out) = self.ports.midi_outs.first()
            {
                write_midi_output(out, midi_in_buf);
            }

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
        self.midi_sink.begin_cycle();
        for (start, len) in block_chunks(n_samples, MAX_BLOCK) {
            self.midi_sink.set_block(start, len);
            self.in_slices.clear();
            self.out_slices.clear();
            let scratch = self.scratch.as_mut_ptr();

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
                &mut self.midi_sink,
            );
            self.out_slices.clear();

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

        for i in 0..self.midi_sink.port_count() {
            if let Some(&port) = self.ports.midi_outs.get(i) {
                unsafe { write_midi_sequence(port, self.midi_sink.output(i)) };
            }
        }

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
        let (e0, p0) = swap.rt_enter();
        let initial = p0;

        let (epoch, old) = swap.publish(Arc::new(ResolvedMappings::empty()));
        assert_eq!(Arc::as_ptr(&old), initial);
        assert!(!swap.releasable()(epoch));

        swap.rt_exit(e0);
        assert!(swap.releasable()(epoch));

        let (e1, p1) = swap.rt_enter();
        assert_eq!(e1, epoch);
        assert_ne!(p1, initial);
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
