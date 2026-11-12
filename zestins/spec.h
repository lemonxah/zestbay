// Zestins plugin specs: the single description of every plugin's ports.
//
// The DSP code uses the port enums below, ttlgen.c writes the LV2 .ttl files
// from zestins_plugins[], and the X11 UI lays itself out from the same table,
// so the three can't drift apart.

#ifndef ZESTINS_SPEC_H
#define ZESTINS_SPEC_H

#define ZESTINS_URI(name) "urn:zestins:" name
#define ZESTINS_UI_URI(name) "urn:zestins:" name "#ui"

typedef enum {
    ZP_AUDIO_IN,
    ZP_AUDIO_OUT,
    ZP_CONTROL_IN,
    ZP_CONTROL_OUT,
    ZP_MIDI_IN,
    ZP_MIDI_OUT,
} ZPortKind;

enum {
    ZF_TOGGLE = 1 << 0,  // on/off control
    ZF_INTEGER = 1 << 1, // whole numbers only
    ZF_LOG = 1 << 2,     // logarithmic scale (min must be > 0)
    ZF_ENUM = 1 << 3,    // one of `labels`, values min, min + 1, ...
    ZF_METER = 1 << 4,   // output shown as a level bar
    ZF_LED = 1 << 5,     // output shown as an on/off light (value > 0.5)
    ZF_ROW = 1 << 6,     // UI: start a new row of controls at this port
};

typedef struct {
    const char *symbol;
    const char *name;
    ZPortKind kind;
    float min, max, def;
    unsigned flags;
    const char *unit;          // "dB", "ms", "Hz", "%", "st" or NULL
    const char *const *labels; // ZF_ENUM labels, NULL-terminated
} ZPort;

typedef struct {
    const char *uri;
    const char *ui_uri;
    const char *name;
    const char *lv2_class; // e.g. "lv2:DelayPlugin"
    const char *comment;
    const ZPort *ports;
    int n_ports;
} ZPlugin;

extern const ZPlugin zestins_plugins[];
extern const int zestins_n_plugins;

const ZPlugin *zestins_find(const char *uri);

// ---- Port indices -------------------------------------------------------

enum { UTIL_IN_L, UTIL_IN_R, UTIL_OUT_L, UTIL_OUT_R, UTIL_GAIN, UTIL_PAN, UTIL_MUTE,
       UTIL_INVERT_L, UTIL_INVERT_R, UTIL_MONO, UTIL_N };

enum { METER_IN_L, METER_IN_R, METER_OUT_L, METER_OUT_R, METER_PEAK_L, METER_PEAK_R,
       METER_RMS_L, METER_RMS_R, METER_N };

enum { EQ_IN_L, EQ_IN_R, EQ_OUT_L, EQ_OUT_R, EQ_HP_FREQ, EQ_LOW_GAIN, EQ_LOW_FREQ,
       EQ_MID_GAIN, EQ_MID_FREQ, EQ_MID_Q, EQ_HIGH_GAIN, EQ_HIGH_FREQ, EQ_N };

enum { GATE_IN_L, GATE_IN_R, GATE_OUT_L, GATE_OUT_R, GATE_THRESHOLD, GATE_ATTACK, GATE_HOLD,
       GATE_RELEASE, GATE_RANGE, GATE_OPEN, GATE_N };

enum { COMP_IN_L, COMP_IN_R, COMP_OUT_L, COMP_OUT_R, COMP_THRESHOLD, COMP_RATIO, COMP_ATTACK,
       COMP_RELEASE, COMP_KNEE, COMP_MAKEUP, COMP_REDUCTION, COMP_N };

enum { DELAY_IN_L, DELAY_IN_R, DELAY_OUT_L, DELAY_OUT_R, DELAY_TIME, DELAY_FEEDBACK,
       DELAY_TONE, DELAY_MIX, DELAY_PING_PONG, DELAY_N };

enum { TONE_OUT_L, TONE_OUT_R, TONE_WAVE, TONE_FREQ, TONE_LEVEL, TONE_ON, TONE_N };
enum { TONE_SINE, TONE_SQUARE, TONE_SAW, TONE_WHITE, TONE_PINK };

enum { SYNTH_MIDI_IN, SYNTH_OUT_L, SYNTH_OUT_R, SYNTH_WAVE, SYNTH_ATTACK, SYNTH_DECAY,
       SYNTH_SUSTAIN, SYNTH_RELEASE, SYNTH_VOLUME, SYNTH_N };
enum { SYNTH_WAVE_SINE, SYNTH_WAVE_SAW, SYNTH_WAVE_SQUARE };

#define SWITCH_OUTPUTS 4
enum { SWITCH_MIDI_IN, SWITCH_OUT_1, SWITCH_SELECT = SWITCH_OUT_1 + SWITCH_OUTPUTS,
       SWITCH_ACTIVITY_1, SWITCH_N = SWITCH_ACTIVITY_1 + SWITCH_OUTPUTS };

enum { MUTIL_MIDI_IN, MUTIL_MIDI_OUT, MUTIL_IN_CHANNEL, MUTIL_OUT_CHANNEL, MUTIL_TRANSPOSE,
       MUTIL_VELOCITY, MUTIL_FIXED_VELOCITY, MUTIL_ACTIVITY, MUTIL_N };

enum { VERB_IN_L, VERB_IN_R, VERB_OUT_L, VERB_OUT_R, VERB_SIZE, VERB_DAMPING, VERB_WIDTH,
       VERB_MIX, VERB_FREEZE, VERB_N };

#endif
