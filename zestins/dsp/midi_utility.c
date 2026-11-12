// Zestins MIDI Utility: passes only the chosen input channel (or all),
// moves messages to another channel, transposes notes and scales or fixes
// note velocities.  A note's off always matches its on, even when the
// settings change while it is held.

#include "dsp.h"

typedef struct {
    void *port[MUTIL_N];
    LV2_URID atom_sequence, midi_event;
    uint32_t activity_hold, activity;
    // Where each sounding input note went: channel + 1 and note (0 = not sounding).
    uint8_t sent_channel[16][128];
    uint8_t sent_note[16][128];
} MidiUtility;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path;
    LV2_URID_Map *map = find_map(features);
    if (!map)
        return NULL;
    MidiUtility *m = calloc(1, sizeof(MidiUtility));
    if (!m)
        return NULL;
    m->atom_sequence = map->map(map->handle, LV2_ATOM__Sequence);
    m->midi_event = map->map(map->handle, LV2_MIDI__MidiEvent);
    m->activity_hold = (uint32_t)(0.15 * rate);
    return m;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < MUTIL_N)
        ((MidiUtility *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    MidiUtility *m = h;
    memset(m->sent_channel, 0, sizeof(m->sent_channel));
    m->activity = 0;
}

static void run(LV2_Handle h, uint32_t n) {
    MidiUtility *m = h;
    LV2_Atom_Sequence *out = m->port[MUTIL_MIDI_OUT];
    uint32_t capacity = midi_out_begin(out, m->atom_sequence);

    int in_channel = (int)(ctl(m->port[MUTIL_IN_CHANNEL], 0) + 0.5f);   // 0 = all
    int out_channel = (int)(ctl(m->port[MUTIL_OUT_CHANNEL], 0) + 0.5f); // 0 = same
    int transpose = (int)lroundf(ctl(m->port[MUTIL_TRANSPOSE], 0));
    float velocity = ctl(m->port[MUTIL_VELOCITY], 100) * 0.01f;
    int fixed = (int)(ctl(m->port[MUTIL_FIXED_VELOCITY], 0) + 0.5f);

    const LV2_Atom_Sequence *in = m->port[MUTIL_MIDI_IN];
    if (in && out) {
        LV2_ATOM_SEQUENCE_FOREACH(in, ev) {
            if (ev->body.type != m->midi_event || ev->body.size == 0)
                continue;
            const uint8_t *src = (const uint8_t *)(ev + 1);
            uint8_t status = src[0];
            if (status >= 0xF0 || ev->body.size > 3) {
                // System messages pass untouched.
                lv2_atom_sequence_append_event(out, capacity, ev);
                continue;
            }
            uint8_t type = status & 0xF0, ch = status & 0x0F;
            if (in_channel > 0 && ch != in_channel - 1)
                continue;
            uint8_t msg[3] = { 0, 0, 0 };
            memcpy(msg, src, ev->body.size);
            uint8_t dest = out_channel > 0 ? (uint8_t)(out_channel - 1) : ch;
            msg[0] = (uint8_t)(type | dest);

            bool is_on = type == LV2_MIDI_MSG_NOTE_ON && ev->body.size == 3 && src[2] > 0;
            bool is_off = type == LV2_MIDI_MSG_NOTE_OFF ||
                          (type == LV2_MIDI_MSG_NOTE_ON && ev->body.size == 3 && src[2] == 0);
            if (is_on) {
                int note = src[1] + transpose;
                if (note < 0 || note > 127)
                    continue;
                int vel = fixed > 0 ? fixed : (int)lroundf(src[2] * velocity);
                msg[1] = (uint8_t)note;
                msg[2] = (uint8_t)(vel < 1 ? 1 : vel > 127 ? 127 : vel);
                m->sent_channel[ch][src[1]] = (uint8_t)(dest + 1);
                m->sent_note[ch][src[1]] = (uint8_t)note;
            } else if (is_off) {
                uint8_t sent = m->sent_channel[ch][src[1]];
                if (!sent)
                    continue; // never sent (filtered or out of range)
                msg[0] = (uint8_t)(LV2_MIDI_MSG_NOTE_OFF | (sent - 1));
                msg[1] = m->sent_note[ch][src[1]];
                msg[2] = ev->body.size == 3 ? src[2] : 0;
                m->sent_channel[ch][src[1]] = 0;
            } else if (type == LV2_MIDI_MSG_NOTE_PRESSURE) {
                int note = src[1] + transpose;
                if (note < 0 || note > 127)
                    continue;
                msg[1] = (uint8_t)note;
            }
            midi_out_write(out, capacity, m->midi_event, ev->time.frames, msg, ev->body.size);
            m->activity = m->activity_hold;
        }
    }

    if (m->port[MUTIL_ACTIVITY])
        *(float *)m->port[MUTIL_ACTIVITY] = m->activity > 0 ? 1.0f : 0.0f;
    m->activity = m->activity > n ? m->activity - n : 0;
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_midi_utility, "midi-utility", instantiate, connect, activate, run,
                   cleanup);
