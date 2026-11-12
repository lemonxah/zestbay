// Port tables of every Zestins plugin (see spec.h).

#include <stddef.h>
#include <string.h>

#include "spec.h"

#define AUDIO_IN(sym, nm) { sym, nm, ZP_AUDIO_IN, 0, 0, 0, 0, NULL, NULL }
#define AUDIO_OUT(sym, nm) { sym, nm, ZP_AUDIO_OUT, 0, 0, 0, 0, NULL, NULL }
#define MIDI_IN(sym, nm) { sym, nm, ZP_MIDI_IN, 0, 0, 0, 0, NULL, NULL }
#define MIDI_OUT(sym, nm) { sym, nm, ZP_MIDI_OUT, 0, 0, 0, 0, NULL, NULL }
#define CTRL(sym, nm, lo, hi, d, fl, u) { sym, nm, ZP_CONTROL_IN, lo, hi, d, fl, u, NULL }
#define CHOICE(sym, nm, lo, hi, d, fl, lbl) \
    { sym, nm, ZP_CONTROL_IN, lo, hi, d, (fl) | ZF_ENUM | ZF_INTEGER, NULL, lbl }
#define TOGGLE(sym, nm, d, fl) { sym, nm, ZP_CONTROL_IN, 0, 1, d, (fl) | ZF_TOGGLE, NULL, NULL }
#define OUT(sym, nm, lo, hi, fl, u) { sym, nm, ZP_CONTROL_OUT, lo, hi, lo, fl, u, NULL }

#define STEREO_IO(P)                         \
    [P##_IN_L] = AUDIO_IN("in_l", "In L"),   \
    [P##_IN_R] = AUDIO_IN("in_r", "In R"),   \
    [P##_OUT_L] = AUDIO_OUT("out_l", "Out L"), \
    [P##_OUT_R] = AUDIO_OUT("out_r", "Out R")

#define COUNT(a) ((int)(sizeof(a) / sizeof((a)[0])))

static const ZPort utility_ports[UTIL_N] = {
    STEREO_IO(UTIL),
    [UTIL_GAIN] = CTRL("gain", "Gain", -60, 24, 0, 0, "dB"),
    [UTIL_PAN] = CTRL("pan", "Balance", -1, 1, 0, 0, NULL),
    [UTIL_MUTE] = TOGGLE("mute", "Mute", 0, ZF_ROW),
    [UTIL_INVERT_L] = TOGGLE("invert_l", "Flip L", 0, 0),
    [UTIL_INVERT_R] = TOGGLE("invert_r", "Flip R", 0, 0),
    [UTIL_MONO] = TOGGLE("mono", "Mono", 0, 0),
};

static const ZPort meter_ports[METER_N] = {
    STEREO_IO(METER),
    [METER_PEAK_L] = OUT("peak_l", "Peak L", -70, 6, ZF_METER, "dB"),
    [METER_PEAK_R] = OUT("peak_r", "Peak R", -70, 6, ZF_METER, "dB"),
    [METER_RMS_L] = OUT("rms_l", "RMS L", -70, 6, ZF_METER, "dB"),
    [METER_RMS_R] = OUT("rms_r", "RMS R", -70, 6, ZF_METER, "dB"),
};

static const ZPort eq_ports[EQ_N] = {
    STEREO_IO(EQ),
    [EQ_HP_FREQ] = CTRL("hp_freq", "Low Cut", 10, 1000, 10, ZF_LOG, "Hz"),
    [EQ_LOW_GAIN] = CTRL("low_gain", "Low", -15, 15, 0, 0, "dB"),
    [EQ_LOW_FREQ] = CTRL("low_freq", "Low Freq", 40, 600, 120, ZF_LOG, "Hz"),
    [EQ_MID_GAIN] = CTRL("mid_gain", "Mid", -15, 15, 0, ZF_ROW, "dB"),
    [EQ_MID_FREQ] = CTRL("mid_freq", "Mid Freq", 150, 8000, 1000, ZF_LOG, "Hz"),
    [EQ_MID_Q] = CTRL("mid_q", "Mid Q", 0.3f, 6, 0.7f, ZF_LOG, NULL),
    [EQ_HIGH_GAIN] = CTRL("high_gain", "High", -15, 15, 0, ZF_ROW, "dB"),
    [EQ_HIGH_FREQ] = CTRL("high_freq", "High Freq", 1500, 18000, 8000, ZF_LOG, "Hz"),
};

static const ZPort gate_ports[GATE_N] = {
    STEREO_IO(GATE),
    [GATE_THRESHOLD] = CTRL("threshold", "Threshold", -80, 0, -50, 0, "dB"),
    [GATE_ATTACK] = CTRL("attack", "Attack", 0.1f, 50, 1, ZF_LOG, "ms"),
    [GATE_HOLD] = CTRL("hold", "Hold", 0, 500, 50, 0, "ms"),
    [GATE_RELEASE] = CTRL("release", "Release", 5, 2000, 150, ZF_LOG, "ms"),
    [GATE_RANGE] = CTRL("range", "Range", -90, 0, -90, 0, "dB"),
    [GATE_OPEN] = OUT("open", "Open", 0, 1, ZF_LED, NULL),
};

static const ZPort compressor_ports[COMP_N] = {
    STEREO_IO(COMP),
    [COMP_THRESHOLD] = CTRL("threshold", "Threshold", -60, 0, -18, 0, "dB"),
    [COMP_RATIO] = CTRL("ratio", "Ratio", 1, 20, 4, ZF_LOG, NULL),
    [COMP_ATTACK] = CTRL("attack", "Attack", 0.1f, 100, 10, ZF_LOG, "ms"),
    [COMP_RELEASE] = CTRL("release", "Release", 10, 2000, 150, ZF_LOG, "ms"),
    [COMP_KNEE] = CTRL("knee", "Knee", 0, 12, 6, ZF_ROW, "dB"),
    [COMP_MAKEUP] = CTRL("makeup", "Makeup", 0, 24, 0, 0, "dB"),
    [COMP_REDUCTION] = OUT("reduction", "Reduction", 0, 30, ZF_METER, "dB"),
};

static const ZPort delay_ports[DELAY_N] = {
    STEREO_IO(DELAY),
    [DELAY_TIME] = CTRL("time", "Time", 1, 2000, 375, ZF_LOG, "ms"),
    [DELAY_FEEDBACK] = CTRL("feedback", "Feedback", 0, 95, 40, 0, "%"),
    [DELAY_TONE] = CTRL("tone", "Tone", 500, 20000, 8000, ZF_LOG, "Hz"),
    [DELAY_MIX] = CTRL("mix", "Mix", 0, 100, 30, 0, "%"),
    [DELAY_PING_PONG] = TOGGLE("ping_pong", "Ping-Pong", 0, ZF_ROW),
};

static const char *const tone_waves[] = { "Sine", "Square", "Saw", "White", "Pink", NULL };
static const ZPort tone_ports[TONE_N] = {
    [TONE_OUT_L] = AUDIO_OUT("out_l", "Out L"),
    [TONE_OUT_R] = AUDIO_OUT("out_r", "Out R"),
    [TONE_WAVE] = CHOICE("wave", "Wave", 0, 4, 0, 0, tone_waves),
    [TONE_FREQ] = CTRL("freq", "Frequency", 20, 20000, 440, ZF_LOG | ZF_ROW, "Hz"),
    [TONE_LEVEL] = CTRL("level", "Level", -60, 0, -18, 0, "dB"),
    [TONE_ON] = TOGGLE("on", "On", 1, 0),
};

static const char *const synth_waves[] = { "Sine", "Saw", "Square", NULL };
static const ZPort synth_ports[SYNTH_N] = {
    [SYNTH_MIDI_IN] = MIDI_IN("midi_in", "MIDI In"),
    [SYNTH_OUT_L] = AUDIO_OUT("out_l", "Out L"),
    [SYNTH_OUT_R] = AUDIO_OUT("out_r", "Out R"),
    [SYNTH_WAVE] = CHOICE("wave", "Wave", 0, 2, 1, 0, synth_waves),
    [SYNTH_ATTACK] = CTRL("attack", "Attack", 1, 2000, 5, ZF_LOG | ZF_ROW, "ms"),
    [SYNTH_DECAY] = CTRL("decay", "Decay", 1, 2000, 200, ZF_LOG, "ms"),
    [SYNTH_SUSTAIN] = CTRL("sustain", "Sustain", 0, 100, 70, 0, "%"),
    [SYNTH_RELEASE] = CTRL("release", "Release", 1, 5000, 300, ZF_LOG, "ms"),
    [SYNTH_VOLUME] = CTRL("volume", "Volume", -60, 0, -12, 0, "dB"),
};

static const char *const switch_outputs[] = { "Out 1", "Out 2", "Out 3", "Out 4", NULL };
static const ZPort switch_ports[SWITCH_N] = {
    [SWITCH_MIDI_IN] = MIDI_IN("in", "In"),
    [SWITCH_OUT_1 + 0] = MIDI_OUT("out_1", "Out 1"),
    [SWITCH_OUT_1 + 1] = MIDI_OUT("out_2", "Out 2"),
    [SWITCH_OUT_1 + 2] = MIDI_OUT("out_3", "Out 3"),
    [SWITCH_OUT_1 + 3] = MIDI_OUT("out_4", "Out 4"),
    [SWITCH_SELECT] = CHOICE("output", "Output", 1, 4, 1, 0, switch_outputs),
    [SWITCH_ACTIVITY_1 + 0] = OUT("activity_1", "Out 1", 0, 1, ZF_LED, NULL),
    [SWITCH_ACTIVITY_1 + 1] = OUT("activity_2", "Out 2", 0, 1, ZF_LED, NULL),
    [SWITCH_ACTIVITY_1 + 2] = OUT("activity_3", "Out 3", 0, 1, ZF_LED, NULL),
    [SWITCH_ACTIVITY_1 + 3] = OUT("activity_4", "Out 4", 0, 1, ZF_LED, NULL),
};

static const char *const in_channels[] = { "All", "1", "2", "3", "4", "5", "6", "7", "8", "9",
                                           "10", "11", "12", "13", "14", "15", "16", NULL };
static const char *const out_channels[] = { "Same", "1", "2", "3", "4", "5", "6", "7", "8", "9",
                                            "10", "11", "12", "13", "14", "15", "16", NULL };
static const ZPort midi_utility_ports[MUTIL_N] = {
    [MUTIL_MIDI_IN] = MIDI_IN("in", "In"),
    [MUTIL_MIDI_OUT] = MIDI_OUT("out", "Out"),
    [MUTIL_IN_CHANNEL] = CHOICE("in_channel", "In Channel", 0, 16, 0, 0, in_channels),
    [MUTIL_OUT_CHANNEL] = CHOICE("out_channel", "Out Channel", 0, 16, 0, 0, out_channels),
    [MUTIL_TRANSPOSE] = CTRL("transpose", "Transpose", -48, 48, 0, ZF_INTEGER | ZF_ROW, "st"),
    [MUTIL_VELOCITY] = CTRL("velocity", "Velocity", 0, 200, 100, 0, "%"),
    [MUTIL_FIXED_VELOCITY] = CTRL("fixed_velocity", "Fixed Vel", 0, 127, 0, ZF_INTEGER, NULL),
    [MUTIL_ACTIVITY] = OUT("activity", "Activity", 0, 1, ZF_LED, NULL),
};

static const ZPort reverb_ports[VERB_N] = {
    STEREO_IO(VERB),
    [VERB_SIZE] = CTRL("size", "Size", 0, 100, 50, 0, "%"),
    [VERB_DAMPING] = CTRL("damping", "Damping", 0, 100, 50, 0, "%"),
    [VERB_WIDTH] = CTRL("width", "Width", 0, 100, 100, 0, "%"),
    [VERB_MIX] = CTRL("mix", "Mix", 0, 100, 25, 0, "%"),
    [VERB_FREEZE] = TOGGLE("freeze", "Freeze", 0, ZF_ROW),
};

#define PLUGIN(id, nm, cls, cmt, ports) \
    { ZESTINS_URI(id), ZESTINS_UI_URI(id), nm, cls, cmt, ports, COUNT(ports) }

const ZPlugin zestins_plugins[] = {
    PLUGIN("utility", "Zestins Utility", "lv2:UtilityPlugin",
           "Gain, balance, mute, polarity flip and mono sum for a stereo signal.", utility_ports),
    PLUGIN("meter", "Zestins Meter", "lv2:AnalyserPlugin",
           "Peak and RMS level meters; the audio passes through unchanged.", meter_ports),
    PLUGIN("eq", "Zestins EQ", "lv2:EQPlugin",
           "Low cut, low shelf, mid peak and high shelf equaliser.", eq_ports),
    PLUGIN("gate", "Zestins Gate", "lv2:GatePlugin",
           "Noise gate with attack, hold, release and range.", gate_ports),
    PLUGIN("compressor", "Zestins Compressor", "lv2:CompressorPlugin",
           "Stereo-linked compressor with soft knee and gain reduction meter.", compressor_ports),
    PLUGIN("delay", "Zestins Delay", "lv2:DelayPlugin",
           "Stereo echo with feedback tone control and ping-pong mode.", delay_ports),
    PLUGIN("tone", "Zestins Tone Generator", "lv2:OscillatorPlugin",
           "Test signal source: sine, square, saw, white or pink noise.", tone_ports),
    PLUGIN("synth", "Zestins Synth", "lv2:InstrumentPlugin",
           "Simple 16-voice synth with ADSR envelope.", synth_ports),
    PLUGIN("midi-switch", "Zestins MIDI Switch", "lv2:MIDIPlugin",
           "Sends all incoming MIDI to one of four outputs. Held notes and sustain are released "
           "on the old output when switching.",
           switch_ports),
    PLUGIN("midi-utility", "Zestins MIDI Utility", "lv2:MIDIPlugin",
           "Channel filter and remap, transpose and velocity scaling.", midi_utility_ports),
    PLUGIN("reverb", "Zestins Reverb", "lv2:ReverbPlugin",
           "Stereo room reverb with size, damping, width and freeze.", reverb_ports),
};

const int zestins_n_plugins = COUNT(zestins_plugins);

const ZPlugin *zestins_find(const char *uri) {
    for (int i = 0; i < zestins_n_plugins; i++) {
        if (uri && !strcmp(zestins_plugins[i].uri, uri))
            return &zestins_plugins[i];
    }
    return NULL;
}
