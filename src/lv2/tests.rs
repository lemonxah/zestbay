//! LV2 host tests against the Zestins plugins (zestins/): the bundle is
//! built with its own Makefile, loaded through lilv and run through
//! `Lv2PluginInstance`, the same path ZestBay uses for any LV2 plugin.

use super::host::Lv2PluginInstance;
use super::scanner;
use super::urid::UridMapper;
use crate::midi::processing::{MidiSink, RawMidiEvent};
use crate::plugin::types::{PluginInfo, PluginPortType};
use std::f32::consts::TAU;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, OnceLock};

const RATE: f64 = 48000.0;
const BLOCK: usize = 256;

/// Build zestins.lv2 once per test run (into target/, incrementally).
fn bundle_dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let build = root.join("target/zestins-test");
        let output = Command::new("make")
            .arg("-C")
            .arg(root.join("zestins"))
            .arg(format!("BUILD={}", build.display()))
            .output()
            .expect("make, a C compiler and the LV2/X11 headers to build zestins");
        assert!(
            output.status.success(),
            "building zestins failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        build.join("zestins.lv2")
    })
}

fn world() -> lilv::World {
    let world = lilv::World::new();
    let path = format!("{}/", bundle_dir().display());
    world.load_bundle(&world.new_file_uri(None, &path));
    world
}

/// MIDI sent on one output: (frame, bytes) per message.
type MidiEvents = Vec<(u32, Vec<u8>)>;

/// A Zestins plugin instance with helpers to drive it.
struct Plug {
    inst: Lv2PluginInstance,
    info: PluginInfo,
}

impl Plug {
    fn new(world: &lilv::World, name: &str) -> Self {
        let uri = format!("urn:zestins:{name}");
        let info = scanner::scan_plugins_with_world(world)
            .into_iter()
            .find(|p| p.uri == uri)
            .unwrap_or_else(|| panic!("{uri} is scanned"));
        let plugin = world
            .plugins()
            .iter()
            .find(|p| p.uri().as_uri() == Some(uri.as_str()))
            .expect("plugin is loaded");
        let inst = unsafe {
            Lv2PluginInstance::new(world, &plugin, &info, RATE, BLOCK as u32, &Arc::new(UridMapper::new()))
        }
        .unwrap_or_else(|| panic!("instantiate {uri}"));
        Self { inst, info }
    }

    fn port(&self, symbol: &str) -> usize {
        self.info
            .ports
            .iter()
            .find(|p| p.symbol == symbol)
            .unwrap_or_else(|| panic!("port {symbol}"))
            .index
    }

    fn set(&self, symbol: &str, value: f32) {
        self.inst.set_parameter(self.port(symbol), value);
    }

    fn output(&self, symbol: &str) -> f32 {
        let index = self.port(symbol);
        self.inst
            .port_updates
            .control_outputs
            .iter()
            .find(|s| s.port_index == index)
            .expect("control output")
            .value
            .load()
    }

    /// Run stereo audio through the plugin in blocks; returns [left, right].
    fn run(&mut self, left: &[f32], right: &[f32]) -> [Vec<f32>; 2] {
        let n = left.len();
        let mut out = [vec![0.0; n], vec![0.0; n]];
        let mut sink = MidiSink::new(0);
        for start in (0..n).step_by(BLOCK) {
            let len = BLOCK.min(n - start);
            let (l, r) = (&left[start..start + len], &right[start..start + len]);
            let [ol, or] = &mut out;
            let inputs: Vec<&[f32]> = if self.info.audio_inputs == 2 { vec![l, r] } else { vec![] };
            let mut outputs: [&mut [f32]; 2] = [&mut ol[start..start + len], &mut or[start..start + len]];
            sink.begin_cycle();
            sink.set_block(0, len);
            unsafe { self.inst.process(&inputs, &mut outputs, len, &[], &mut sink) };
        }
        out
    }

    /// Run `n` frames with no audio input (generators, instruments).
    fn render(&mut self, n: usize) -> [Vec<f32>; 2] {
        let silent = vec![0.0; n];
        self.run(&silent, &silent)
    }

    /// One block with MIDI in; returns the audio and each MIDI output's events.
    fn midi_block(&mut self, input: &[RawMidiEvent]) -> ([Vec<f32>; 2], Vec<MidiEvents>) {
        let mut sink = MidiSink::new(self.inst.midi_output_names.len());
        sink.begin_cycle();
        sink.set_block(0, BLOCK);
        let mut out = [vec![0.0; BLOCK], vec![0.0; BLOCK]];
        {
            let [ol, or] = &mut out;
            let mut outputs: [&mut [f32]; 2] = [&mut ol[..], &mut or[..]];
            let n_out = self.info.audio_outputs.min(2);
            unsafe { self.inst.process(&[], &mut outputs[..n_out], BLOCK, input, &mut sink) };
        }
        let midi = (0..sink.port_count()).map(|i| decode(sink.output(i))).collect();
        (out, midi)
    }
}

/// (offset, bytes) of every control in a spa_pod_sequence.
fn decode(seq: &[u8]) -> MidiEvents {
    let u32_at = |at: usize| u32::from_ne_bytes(seq[at..at + 4].try_into().unwrap());
    let end = 8 + u32_at(0) as usize;
    let mut pos = 16;
    let mut out = Vec::new();
    while pos + 16 <= end {
        let size = u32_at(pos + 8) as usize;
        out.push((u32_at(pos), seq[pos + 16..pos + 16 + size].to_vec()));
        pos += (16 + size + 7) & !7;
    }
    out
}

fn ev(offset: u32, data: [u8; 3]) -> RawMidiEvent {
    RawMidiEvent { offset, data, size: 3 }
}

fn sine(freq: f32, amp: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| amp * (TAU * freq * i as f32 / RATE as f32).sin()).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// The last `ms` milliseconds of a signal.
fn tail(x: &[f32], ms: usize) -> &[f32] {
    &x[x.len() - ms * RATE as usize / 1000..]
}

fn secs(s: f32) -> usize {
    (s * RATE as f32) as usize
}

// ---- Bundle ---------------------------------------------------------------

#[test]
fn every_zestin_is_found_compatible_and_has_a_ui() {
    let world = world();
    let found: Vec<PluginInfo> = scanner::scan_plugins_with_world(&world)
        .into_iter()
        .filter(|p| p.uri.starts_with("urn:zestins:"))
        .collect();
    let mut names: Vec<&str> = found.iter().map(|p| p.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Zestins Compressor",
            "Zestins Delay",
            "Zestins EQ",
            "Zestins Gate",
            "Zestins MIDI Switch",
            "Zestins MIDI Utility",
            "Zestins Meter",
            "Zestins Reverb",
            "Zestins Synth",
            "Zestins Tone Generator",
            "Zestins Utility",
        ]
    );
    for p in &found {
        assert!(p.compatible, "{} needs unsupported features", p.name);
        assert!(p.has_ui, "{} has no UI", p.name);
    }
}

// ---- Audio effects ----------------------------------------------------------

#[test]
fn utility_gain_mute_flip_and_balance() {
    let world = world();
    let mut u = Plug::new(&world, "utility");
    let x = sine(1000.0, 0.5, secs(0.3));
    u.set("gain", -6.0206);
    let [l, r] = u.run(&x, &x);
    assert!((peak(tail(&l, 100)) - 0.25).abs() < 0.005);
    assert!((peak(tail(&r, 100)) - 0.25).abs() < 0.005);

    u.set("gain", 0.0);
    u.set("invert_l", 1.0);
    u.set("pan", 1.0);
    let [l, r] = u.run(&x, &x);
    assert!(peak(tail(&l, 100)) < 1e-3, "balance right silences the left");
    let same = |a: &[f32], b: &[f32], sign: f32| a.iter().zip(b).all(|(o, i)| (o - sign * i).abs() < 1e-3);
    assert!(same(tail(&r, 100), tail(&x, 100), 1.0), "right untouched");

    u.set("pan", 0.0);
    let [l, _] = u.run(&x, &x);
    assert!(same(tail(&l, 100), tail(&x, 100), -1.0), "left flipped");

    u.set("mute", 1.0);
    let [l, r] = u.run(&x, &x);
    assert!(peak(tail(&l, 100)) < 1e-4 && peak(tail(&r, 100)) < 1e-4);
}

#[test]
fn meter_reports_peak_and_rms() {
    let world = world();
    let mut m = Plug::new(&world, "meter");
    let x = sine(997.0, 0.5, secs(1.0));
    let silence = vec![0.0; x.len()];
    let [l, r] = m.run(&x, &silence);
    assert_eq!(l, x, "audio passes through");
    assert_eq!(r, silence);
    assert!((m.output("peak_l") - -6.02).abs() < 0.2, "peak {}", m.output("peak_l"));
    assert!((m.output("rms_l") - -9.03).abs() < 0.3, "rms {}", m.output("rms_l"));
    assert_eq!(m.output("peak_r"), -70.0);
    assert_eq!(m.output("rms_r"), -70.0);
}

#[test]
fn eq_is_flat_by_default_and_bands_work() {
    let world = world();
    let x = sine(1000.0, 0.25, secs(0.5));
    let level = |p: &mut Plug, x: &[f32]| db(rms(tail(&p.run(x, x)[0], 200))) - db(rms(tail(x, 200)));

    let mut eq = Plug::new(&world, "eq");
    assert!(level(&mut eq, &x).abs() < 0.2, "flat");
    eq.set("mid_gain", 12.0);
    assert!((level(&mut eq, &x) - 12.0).abs() < 0.5, "mid boost");

    let mut eq = Plug::new(&world, "eq");
    eq.set("hp_freq", 1000.0);
    let low = sine(100.0, 0.25, secs(0.5));
    assert!(level(&mut eq, &low) < -30.0, "low cut");
}

#[test]
fn gate_closes_below_threshold_and_opens_above() {
    let world = world();
    let mut g = Plug::new(&world, "gate");
    let quiet = sine(440.0, db_to_amp(-60.0), secs(1.0));
    let [l, _] = g.run(&quiet, &quiet);
    assert!(peak(tail(&l, 200)) < 1e-5, "closed");
    assert_eq!(g.output("open"), 0.0);

    let loud = sine(440.0, db_to_amp(-20.0), secs(0.3));
    let [l, _] = g.run(&loud, &loud);
    assert!((peak(tail(&l, 100)) - db_to_amp(-20.0)).abs() < 1e-3, "open at unity");
    assert_eq!(g.output("open"), 1.0);
}

fn db_to_amp(d: f32) -> f32 {
    10f32.powf(d / 20.0)
}

#[test]
fn compressor_reduces_above_threshold() {
    let world = world();
    let mut c = Plug::new(&world, "compressor");
    c.set("knee", 0.0);
    // -6 dBFS peaks, threshold -18, 4:1 -> about 9 dB of reduction.
    let x = sine(1000.0, 0.5, secs(1.0));
    let [l, _] = c.run(&x, &x);
    let reduction = c.output("reduction");
    assert!((reduction - 9.0).abs() < 1.0, "reduction {reduction}");
    let out_db = db(peak(tail(&l, 200)));
    assert!((out_db - -15.0).abs() < 1.5, "output {out_db} dB");

    // Below the threshold nothing happens.
    let quiet = sine(1000.0, db_to_amp(-30.0), secs(1.0));
    let [l, _] = c.run(&quiet, &quiet);
    assert!(c.output("reduction") < 0.1);
    assert!((db(peak(tail(&l, 200))) - -30.0).abs() < 0.2);
}

#[test]
fn delay_echoes_after_the_set_time() {
    let world = world();
    let mut d = Plug::new(&world, "delay");
    d.set("time", 10.0);
    d.set("mix", 100.0);
    d.set("feedback", 0.0);
    d.render(secs(2.0)); // let the delay time glide to 10 ms
    let mut x = vec![0.0; secs(0.05)];
    x[0] = 1.0;
    let [l, r] = d.run(&x, &x);
    let at = l.iter().position(|v| v.abs() > 0.5).expect("an echo");
    assert!((at as i64 - 480).abs() <= 2, "echo at sample {at}");
    assert_eq!(l, r);
    assert!(peak(&l[..400]) < 1e-6, "fully wet: no dry signal");
}

#[test]
fn reverb_adds_a_tail_and_mix_zero_is_dry() {
    let world = world();
    let mut v = Plug::new(&world, "reverb");
    let mut x = vec![0.0; secs(1.0)];
    x[0] = 1.0;
    let [l, r] = v.run(&x, &x);
    let late = &l[secs(0.1)..secs(0.5)];
    assert!(rms(late) > 1e-4, "tail rms {}", rms(late));
    assert_ne!(l, r, "stereo spread");

    v.set("mix", 0.0);
    v.render(secs(0.5));
    let y = sine(500.0, 0.3, secs(0.2));
    let [l, _] = v.run(&y, &y);
    let (l, y) = (tail(&l, 50), tail(&y, 50));
    assert!(l.iter().zip(y).all(|(o, i)| (o - i).abs() < 1e-3), "dry only");
}

// ---- Sources ----------------------------------------------------------------

#[test]
fn tone_generator_plays_at_level_and_frequency() {
    let world = world();
    let mut t = Plug::new(&world, "tone");
    t.set("freq", 1000.0);
    t.set("level", -6.0206);
    let [l, r] = t.render(secs(1.0));
    let last = tail(&l, 500);
    assert!((peak(last) - 0.5).abs() < 0.01, "peak {}", peak(last));
    assert_eq!(l, r);
    let crossings = last.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    assert!((crossings as i64 - 500).abs() <= 2, "{crossings} cycles in 0.5 s");

    for wave in [1.0, 2.0, 3.0, 4.0] {
        t.set("wave", wave);
        let [l, _] = t.render(secs(0.2));
        let p = peak(tail(&l, 100));
        assert!(p > 0.05 && p < 1.0, "wave {wave}: peak {p}");
    }

    t.set("on", 0.0);
    let [l, _] = t.render(secs(0.5));
    assert!(peak(tail(&l, 100)) < 1e-5);
}

#[test]
fn synth_plays_notes_and_releases_them() {
    let world = world();
    let mut s = Plug::new(&world, "synth");
    assert!(s.inst.midi_output_names.is_empty());
    s.set("wave", 0.0); // sine
    s.set("release", 50.0);
    let (first, _) = s.midi_block(&[ev(0, [0x90, 69, 127])]);
    assert!(peak(&first[0]) > 0.0);
    let [l, r] = s.render(secs(0.5));
    assert_eq!(l, r);
    let crossings = l.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    assert!((crossings as i64 - 220).abs() <= 2, "A4: {crossings} cycles in 0.5 s");

    s.midi_block(&[ev(0, [0x80, 69, 0])]);
    let [l, _] = s.render(secs(0.2));
    assert!(peak(tail(&l, 50)) < 1e-6, "released");

    // Sustain pedal keeps the note after its key is released.
    s.midi_block(&[ev(0, [0xB0, 64, 127]), ev(1, [0x90, 60, 100]), ev(2, [0x80, 60, 0])]);
    let [l, _] = s.render(secs(0.3));
    assert!(peak(tail(&l, 50)) > 0.01, "sustained");
    s.midi_block(&[ev(0, [0xB0, 64, 0])]);
    let [l, _] = s.render(secs(0.2));
    assert!(peak(tail(&l, 50)) < 1e-6, "pedal up releases");
}

// ---- MIDI -------------------------------------------------------------------

#[test]
fn midi_switch_reports_every_output() {
    let world = world();
    let s = Plug::new(&world, "midi-switch");
    assert_eq!(s.inst.midi_output_names, ["Out 1", "Out 2", "Out 3", "Out 4"]);
    let of = |t| s.info.ports.iter().filter(|p| p.port_type == t).count();
    assert_eq!(of(PluginPortType::AtomInput), 1);
    assert_eq!(of(PluginPortType::AtomOutput), 4);
}

#[test]
fn midi_switch_follows_the_selected_output() {
    let world = world();
    let mut s = Plug::new(&world, "midi-switch");

    let (_, outs) = s.midi_block(&[ev(10, [0x90, 60, 100]), ev(20, [0xB0, 64, 127])]);
    assert_eq!(outs[0], vec![(10, vec![0x90, 60, 100]), (20, vec![0xB0, 64, 127])]);
    assert!(outs[1..].iter().all(|o| o.is_empty()));
    assert_eq!(s.output("activity_1"), 1.0);
    assert_eq!(s.output("activity_3"), 0.0);

    // Switching releases the held note and the pedal on Out 1.
    s.set("output", 3.0);
    let (_, outs) = s.midi_block(&[ev(5, [0x90, 64, 90])]);
    assert_eq!(outs[0], vec![(0, vec![0x80, 60, 0]), (0, vec![0xB0, 64, 0])]);
    assert!(outs[1].is_empty() && outs[3].is_empty());
    assert_eq!(outs[2], vec![(5, vec![0x90, 64, 90])]);
    assert_eq!(s.output("activity_3"), 1.0);

    // Activity lights go out 150 ms after the last message.
    for _ in 0..30 {
        s.midi_block(&[]);
    }
    assert_eq!(s.output("activity_1"), 0.0);
    assert_eq!(s.output("activity_3"), 0.0);
}

#[test]
fn midi_switch_bypass_sends_input_to_the_first_output() {
    let world = world();
    let mut s = Plug::new(&world, "midi-switch");
    s.set("output", 2.0);
    s.inst.set_bypassed(true);
    let (_, outs) = s.midi_block(&[ev(7, [0x90, 60, 100])]);
    assert_eq!(outs[0], vec![(7, vec![0x90, 60, 100])]);
    assert!(outs[1..].iter().all(|o| o.is_empty()));
}

#[test]
fn midi_utility_transposes_remaps_and_keeps_note_offs_matched() {
    let world = world();
    let mut m = Plug::new(&world, "midi-utility");
    assert_eq!(m.inst.midi_output_names, ["Out"]);
    m.set("transpose", 12.0);
    m.set("out_channel", 3.0);
    m.set("velocity", 50.0);
    let (_, outs) = m.midi_block(&[ev(4, [0x90, 60, 100])]);
    assert_eq!(outs[0], vec![(4, vec![0x92, 72, 50])]);
    assert_eq!(m.output("activity"), 1.0);

    // Settings change while the note is held: its off still matches its on.
    m.set("transpose", 0.0);
    m.set("out_channel", 0.0);
    let (_, outs) = m.midi_block(&[ev(0, [0x80, 60, 0]), ev(1, [0xB0, 7, 100])]);
    assert_eq!(outs[0], vec![(0, vec![0x82, 72, 0]), (1, vec![0xB0, 7, 100])]);

    // Input channel filter and fixed velocity.
    m.set("in_channel", 2.0);
    m.set("fixed_velocity", 90.0);
    let (_, outs) = m.midi_block(&[ev(0, [0x90, 60, 10]), ev(1, [0x91, 62, 10])]);
    assert_eq!(outs[0], vec![(1, vec![0x91, 62, 90])]);
}
