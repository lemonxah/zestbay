use super::host::LadspaPluginInstance;
use crate::midi::processing::{MidiSink, RawMidiEvent};
use crate::plugin::filter::{FilterNode, FilterProcessor};

pub use crate::plugin::filter::FilterConfig;

pub type LadspaFilterNode = FilterNode<LadspaPluginInstance>;

impl FilterProcessor for LadspaPluginInstance {
    const FORMAT: &'static str = "LADSPA";

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
