// Zestins Synth: 16 voices of sine, saw or square (PolyBLEP) with a linear
// ADSR.  Handles note on/off, sustain pedal (CC 64), pitch bend (+-2
// semitones), all notes off (CC 123) and all sound off (CC 120).  MIDI is
// applied at its frame within the block.

#include "dsp.h"

#define VOICES 16

typedef enum { ENV_OFF, ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE } EnvStage;

typedef struct {
    EnvStage stage;
    float env;
    double phase;
    uint8_t channel, note;
    float velocity;
    bool held;      // key down
    bool sustained; // key up but held by the sustain pedal
    uint32_t age;   // for voice stealing
} Voice;

typedef struct {
    void *port[SYNTH_N];
    double rate;
    LV2_URID midi_event;
    Voice voice[VOICES];
    bool pedal[16];
    float bend[16]; // semitones
    uint32_t clock;
} Synth;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path;
    LV2_URID_Map *map = find_map(features);
    if (!map)
        return NULL;
    Synth *s = calloc(1, sizeof(Synth));
    if (!s)
        return NULL;
    s->rate = rate;
    s->midi_event = map->map(map->handle, LV2_MIDI__MidiEvent);
    return s;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < SYNTH_N)
        ((Synth *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Synth *s = h;
    memset(s->voice, 0, sizeof(s->voice));
    memset(s->pedal, 0, sizeof(s->pedal));
    memset(s->bend, 0, sizeof(s->bend));
}

static void release(Voice *v) {
    if (v->stage != ENV_OFF)
        v->stage = ENV_RELEASE;
    v->held = v->sustained = false;
}

static void note_on(Synth *s, uint8_t ch, uint8_t note, uint8_t vel) {
    Voice *pick = NULL;
    // Retrigger the same note, else a free voice, else the oldest one.
    for (int i = 0; i < VOICES && !pick; i++) {
        Voice *v = &s->voice[i];
        if (v->stage != ENV_OFF && v->channel == ch && v->note == note)
            pick = v;
    }
    for (int i = 0; i < VOICES && !pick; i++) {
        if (s->voice[i].stage == ENV_OFF)
            pick = &s->voice[i];
    }
    if (!pick) {
        pick = &s->voice[0];
        for (int i = 1; i < VOICES; i++) {
            if (s->voice[i].age < pick->age)
                pick = &s->voice[i];
        }
    }
    if (pick->stage == ENV_OFF)
        pick->phase = 0;
    pick->stage = ENV_ATTACK;
    pick->channel = ch;
    pick->note = note;
    pick->velocity = vel / 127.0f;
    pick->held = true;
    pick->sustained = false;
    pick->age = ++s->clock;
}

static void note_off(Synth *s, uint8_t ch, uint8_t note) {
    for (int i = 0; i < VOICES; i++) {
        Voice *v = &s->voice[i];
        if (v->stage == ENV_OFF || !v->held || v->channel != ch || v->note != note)
            continue;
        if (s->pedal[ch]) {
            v->held = false;
            v->sustained = true;
        } else {
            release(v);
        }
    }
}

static void handle_midi(Synth *s, const uint8_t *m, uint32_t size) {
    if (size < 2)
        return;
    uint8_t type = m[0] & 0xF0, ch = m[0] & 0x0F;
    switch (type) {
    case LV2_MIDI_MSG_NOTE_ON:
        if (size >= 3 && m[2] > 0) {
            note_on(s, ch, m[1] & 0x7F, m[2] & 0x7F);
            break;
        }
        // velocity 0: note off
        // fall through
    case LV2_MIDI_MSG_NOTE_OFF:
        note_off(s, ch, m[1] & 0x7F);
        break;
    case LV2_MIDI_MSG_CONTROLLER:
        if (size < 3)
            break;
        if (m[1] == LV2_MIDI_CTL_SUSTAIN) {
            s->pedal[ch] = m[2] >= 64;
            if (!s->pedal[ch]) {
                for (int i = 0; i < VOICES; i++) {
                    if (s->voice[i].sustained && s->voice[i].channel == ch)
                        release(&s->voice[i]);
                }
            }
        } else if (m[1] == LV2_MIDI_CTL_ALL_NOTES_OFF || m[1] == LV2_MIDI_CTL_ALL_SOUNDS_OFF) {
            for (int i = 0; i < VOICES; i++) {
                if (s->voice[i].channel != ch)
                    continue;
                if (m[1] == LV2_MIDI_CTL_ALL_SOUNDS_OFF)
                    s->voice[i].stage = ENV_OFF;
                else
                    release(&s->voice[i]);
            }
        }
        break;
    case LV2_MIDI_MSG_BENDER:
        if (size >= 3) {
            int v = ((m[2] & 0x7F) << 7) | (m[1] & 0x7F);
            s->bend[ch] = (float)(v - 8192) / 8192.0f * 2.0f;
        }
        break;
    default:
        break;
    }
}

static float osc(int wave, double p, double dt) {
    switch (wave) {
    case SYNTH_WAVE_SINE:
        return (float)sin(2.0 * M_PI * p);
    case SYNTH_WAVE_SQUARE: {
        double q = p + 0.5;
        q -= floor(q);
        return (p < 0.5 ? 1.0f : -1.0f) + poly_blep(p, dt) - poly_blep(q, dt);
    }
    default: // saw
        return (float)(2.0 * p - 1.0) - poly_blep(p, dt);
    }
}

typedef struct {
    int wave;
    float attack, decay, sustain, release; // per-sample steps / level
    float volume;
} Params;

static void render(Synth *s, const Params *p, float *out_l, float *out_r, uint32_t from,
                   uint32_t to) {
    for (uint32_t i = from; i < to; i++)
        out_l[i] = 0;
    for (int vi = 0; vi < VOICES; vi++) {
        Voice *v = &s->voice[vi];
        if (v->stage == ENV_OFF)
            continue;
        double freq = 440.0 * pow(2.0, (v->note - 69 + s->bend[v->channel]) / 12.0);
        double dt = fmin(freq / s->rate, 0.45);
        for (uint32_t i = from; i < to && v->stage != ENV_OFF; i++) {
            switch (v->stage) {
            case ENV_ATTACK:
                v->env += p->attack;
                if (v->env >= 1.0f) {
                    v->env = 1.0f;
                    v->stage = ENV_DECAY;
                }
                break;
            case ENV_DECAY:
                v->env -= p->decay;
                if (v->env <= p->sustain) {
                    v->env = p->sustain;
                    v->stage = ENV_SUSTAIN;
                }
                break;
            case ENV_SUSTAIN:
                v->env = p->sustain;
                break;
            case ENV_RELEASE:
                v->env -= p->release;
                if (v->env <= 0.0f) {
                    v->env = 0.0f;
                    v->stage = ENV_OFF;
                }
                break;
            default:
                break;
            }
            out_l[i] += osc(p->wave, v->phase, dt) * v->env * v->velocity;
            v->phase += dt;
            if (v->phase >= 1.0)
                v->phase -= 1.0;
        }
    }
    for (uint32_t i = from; i < to; i++) {
        out_l[i] *= p->volume;
        out_r[i] = out_l[i];
    }
}

static float step(float ms, double rate) { return 1.0f / fmaxf(1.0f, ms * 0.001f * (float)rate); }

static void run(LV2_Handle h, uint32_t n) {
    Synth *s = h;
    float *out_l = s->port[SYNTH_OUT_L], *out_r = s->port[SYNTH_OUT_R];
    if (!out_l || !out_r)
        return;
    Params p = {
        .wave = (int)(ctl(s->port[SYNTH_WAVE], 1) + 0.5f),
        .attack = step(ctl(s->port[SYNTH_ATTACK], 5), s->rate),
        .decay = step(ctl(s->port[SYNTH_DECAY], 200), s->rate),
        .sustain = clampf(ctl(s->port[SYNTH_SUSTAIN], 70) * 0.01f, 0, 1),
        .release = step(ctl(s->port[SYNTH_RELEASE], 300), s->rate),
        .volume = db_to_gain(ctl(s->port[SYNTH_VOLUME], -12)) * 0.25f,
    };

    uint32_t done = 0;
    const LV2_Atom_Sequence *in = s->port[SYNTH_MIDI_IN];
    if (in) {
        LV2_ATOM_SEQUENCE_FOREACH(in, ev) {
            uint32_t at = ev->time.frames < 0 ? 0 : (uint32_t)ev->time.frames;
            if (at > n)
                at = n;
            if (at > done) {
                render(s, &p, out_l, out_r, done, at);
                done = at;
            }
            if (ev->body.type == s->midi_event)
                handle_midi(s, (const uint8_t *)(ev + 1), ev->body.size);
        }
    }
    render(s, &p, out_l, out_r, done, n);
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_synth, "synth", instantiate, connect, activate, run, cleanup);
