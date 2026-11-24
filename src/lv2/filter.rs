use super::host::Lv2PluginInstance;
use crate::midi::processing::{MidiSink, RawMidiEvent};
use crate::plugin::filter::{FilterNode, FilterProcessor};

pub use crate::plugin::filter::FilterConfig;

pub type Lv2FilterNode = FilterNode<Lv2PluginInstance>;

impl FilterProcessor for Lv2PluginInstance {
    const FORMAT: &'static str = "LV2";

    unsafe fn process_cycle(
        instance: *mut Self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        samples: usize,
        midi: &[RawMidiEvent],
        midi_out: &mut MidiSink,
    ) -> u64 {
        unsafe {
            let instance = &mut *instance;
            instance.process(inputs, outputs, samples, midi, midi_out);
            instance.last_worker_ns
        }
    }
}
