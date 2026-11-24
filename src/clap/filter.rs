use super::host::ClapPluginInstance;
use crate::midi::processing::{MidiSink, RawMidiEvent};
use crate::plugin::filter::{FilterNode, FilterProcessor};

pub use crate::plugin::filter::FilterConfig;

pub type ClapFilterNode = FilterNode<ClapPluginInstance>;

impl FilterProcessor for ClapPluginInstance {
    const FORMAT: &'static str = "CLAP";

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
            0
        }
    }
}
