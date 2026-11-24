use super::host::Vst2PluginInstance;
use crate::midi::processing::{MidiSink, RawMidiEvent};
use crate::plugin::filter::{FilterNode, FilterProcessor};

pub use crate::plugin::filter::FilterConfig;

pub type Vst2FilterNode = FilterNode<Vst2PluginInstance>;

impl FilterProcessor for Vst2PluginInstance {
    const FORMAT: &'static str = "VST2";

    unsafe fn process_cycle(
        instance: *mut Self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        samples: usize,
        midi: &[RawMidiEvent],
        _midi_out: &mut MidiSink,
    ) -> u64 {
        unsafe {
            let instance = &*instance;
            instance.process(inputs, outputs, samples, midi);
            0
        }
    }
}
