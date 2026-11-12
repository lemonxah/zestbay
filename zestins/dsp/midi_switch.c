// Zestins MIDI Switch: all incoming MIDI goes to the output picked by the
// Output control.  On a switch, notes still held and a pressed sustain pedal
// are released on the previous output so nothing hangs there.  Each output
// has an activity light that stays on for 150 ms after MIDI was sent.

#include "dsp.h"

typedef struct {
    void *port[SWITCH_N];
    LV2_URID atom_sequence, midi_event;
    uint32_t activity_hold;
    uint32_t activity[SWITCH_OUTPUTS]; // samples left with the light on
    int active;
    bool held[16][128];
    bool sustain[16];
} Switch;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path;
    LV2_URID_Map *map = find_map(features);
    if (!map)
        return NULL;
    Switch *s = calloc(1, sizeof(Switch));
    if (!s)
        return NULL;
    s->atom_sequence = map->map(map->handle, LV2_ATOM__Sequence);
    s->midi_event = map->map(map->handle, LV2_MIDI__MidiEvent);
    s->activity_hold = (uint32_t)(0.15 * rate);
    return s;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < SWITCH_N)
        ((Switch *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Switch *s = h;
    s->active = 0;
    memset(s->held, 0, sizeof(s->held));
    memset(s->sustain, 0, sizeof(s->sustain));
    memset(s->activity, 0, sizeof(s->activity));
}

// Release what is still sounding on output `idx`.
static void release_all(Switch *s, int idx, uint32_t capacity) {
    LV2_Atom_Sequence *out = s->port[SWITCH_OUT_1 + idx];
    for (int ch = 0; ch < 16; ch++) {
        for (int note = 0; note < 128; note++) {
            if (s->held[ch][note]) {
                const uint8_t off[3] = { (uint8_t)(LV2_MIDI_MSG_NOTE_OFF | ch), (uint8_t)note, 0 };
                midi_out_write(out, capacity, s->midi_event, 0, off, 3);
            }
            s->held[ch][note] = false;
        }
        if (s->sustain[ch]) {
            const uint8_t up[3] = { (uint8_t)(LV2_MIDI_MSG_CONTROLLER | ch), LV2_MIDI_CTL_SUSTAIN, 0 };
            midi_out_write(out, capacity, s->midi_event, 0, up, 3);
        }
        s->sustain[ch] = false;
    }
    s->activity[idx] = s->activity_hold;
}

// Remember which notes and pedals are down on the active output.
static void track(Switch *s, const uint8_t *msg, uint32_t size) {
    if (size < 3)
        return;
    uint8_t type = msg[0] & 0xF0, ch = msg[0] & 0x0F, note = msg[1] & 0x7F;
    if (type == LV2_MIDI_MSG_NOTE_ON)
        s->held[ch][note] = msg[2] > 0;
    else if (type == LV2_MIDI_MSG_NOTE_OFF)
        s->held[ch][note] = false;
    else if (type == LV2_MIDI_MSG_CONTROLLER && msg[1] == LV2_MIDI_CTL_SUSTAIN)
        s->sustain[ch] = msg[2] >= 64;
}

static void run(LV2_Handle h, uint32_t n) {
    Switch *s = h;
    uint32_t capacity[SWITCH_OUTPUTS];
    for (int i = 0; i < SWITCH_OUTPUTS; i++)
        capacity[i] = midi_out_begin(s->port[SWITCH_OUT_1 + i], s->atom_sequence);

    int sel = (int)(ctl(s->port[SWITCH_SELECT], 1) + 0.5f) - 1;
    sel = sel < 0 ? 0 : sel >= SWITCH_OUTPUTS ? SWITCH_OUTPUTS - 1 : sel;
    if (sel != s->active) {
        release_all(s, s->active, capacity[s->active]);
        s->active = sel;
    }

    const LV2_Atom_Sequence *in = s->port[SWITCH_MIDI_IN];
    LV2_Atom_Sequence *out = s->port[SWITCH_OUT_1 + s->active];
    if (in) {
        LV2_ATOM_SEQUENCE_FOREACH(in, ev) {
            if (ev->body.type != s->midi_event || ev->body.size == 0)
                continue;
            track(s, (const uint8_t *)(ev + 1), ev->body.size);
            if (out)
                lv2_atom_sequence_append_event(out, capacity[s->active], ev);
            s->activity[s->active] = s->activity_hold;
        }
    }

    for (int i = 0; i < SWITCH_OUTPUTS; i++) {
        float *light = s->port[SWITCH_ACTIVITY_1 + i];
        if (light)
            *light = s->activity[i] > 0 ? 1.0f : 0.0f;
        s->activity[i] = s->activity[i] > n ? s->activity[i] - n : 0;
    }
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_midi_switch, "midi-switch", instantiate, connect, activate, run,
                   cleanup);
