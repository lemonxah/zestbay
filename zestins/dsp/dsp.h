// Small DSP helpers shared by the Zestins plugins.

#ifndef ZESTINS_DSP_H
#define ZESTINS_DSP_H

#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <lv2/atom/atom.h>
#include <lv2/atom/util.h>
#include <lv2/core/lv2.h>
#include <lv2/midi/midi.h>
#include <lv2/urid/urid.h>

#include "../spec.h"

#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

static inline float db_to_gain(float db) { return db <= -90.0f ? 0.0f : powf(10.0f, db * 0.05f); }

static inline float gain_to_db(float g) { return g > 1e-6f ? 20.0f * log10f(g) : -120.0f; }

static inline float clampf(float v, float lo, float hi) { return v < lo ? lo : v > hi ? hi : v; }

// Read a control port, falling back to `def` while it is unconnected.
static inline float ctl(const float *port, float def) { return port ? *port : def; }

// Coefficient of a one-pole smoother/envelope that covers ~63% in `ms`.
static inline float time_coef(float ms, double rate) {
    return ms <= 0.0f ? 0.0f : expf(-1.0f / (ms * 0.001f * (float)rate));
}

// One-pole parameter smoothing (~20 ms) against zipper noise.
typedef struct {
    float value, coef;
} Smooth;

static inline void smooth_init(Smooth *s, float value, double rate) {
    s->value = value;
    s->coef = time_coef(20.0f, rate);
}

static inline float smooth_next(Smooth *s, float target) {
    s->value = target + (s->value - target) * s->coef;
    return s->value;
}

// RBJ biquad (Audio EQ Cookbook), transposed direct form II.
typedef struct {
    float b0, b1, b2, a1, a2;
    float z1, z2;
} Biquad;

static inline float biquad_run(Biquad *f, float x) {
    float y = f->b0 * x + f->z1;
    f->z1 = f->b1 * x - f->a1 * y + f->z2;
    f->z2 = f->b2 * x - f->a2 * y;
    return y;
}

static inline void biquad_set(Biquad *f, double b0, double b1, double b2, double a0, double a1,
                              double a2) {
    f->b0 = (float)(b0 / a0);
    f->b1 = (float)(b1 / a0);
    f->b2 = (float)(b2 / a0);
    f->a1 = (float)(a1 / a0);
    f->a2 = (float)(a2 / a0);
}

typedef enum { BQ_HIGHPASS, BQ_LOWSHELF, BQ_PEAK, BQ_HIGHSHELF } BiquadType;

static inline void biquad_design(Biquad *f, BiquadType type, double rate, double freq,
                                 double q, double gain_db) {
    freq = fmin(freq, rate * 0.45);
    double A = pow(10.0, gain_db / 40.0);
    double w0 = 2.0 * M_PI * freq / rate;
    double cw = cos(w0), sw = sin(w0);
    double alpha = sw / (2.0 * q);
    switch (type) {
    case BQ_HIGHPASS:
        biquad_set(f, (1 + cw) / 2, -(1 + cw), (1 + cw) / 2, 1 + alpha, -2 * cw, 1 - alpha);
        break;
    case BQ_PEAK:
        biquad_set(f, 1 + alpha * A, -2 * cw, 1 - alpha * A, 1 + alpha / A, -2 * cw,
                   1 - alpha / A);
        break;
    case BQ_LOWSHELF: {
        double s = 2 * sqrt(A) * alpha;
        biquad_set(f, A * ((A + 1) - (A - 1) * cw + s), 2 * A * ((A - 1) - (A + 1) * cw),
                   A * ((A + 1) - (A - 1) * cw - s), (A + 1) + (A - 1) * cw + s,
                   -2 * ((A - 1) + (A + 1) * cw), (A + 1) + (A - 1) * cw - s);
        break;
    }
    case BQ_HIGHSHELF: {
        double s = 2 * sqrt(A) * alpha;
        biquad_set(f, A * ((A + 1) + (A - 1) * cw + s), -2 * A * ((A - 1) + (A + 1) * cw),
                   A * ((A + 1) + (A - 1) * cw - s), (A + 1) - (A - 1) * cw + s,
                   2 * ((A - 1) - (A + 1) * cw), (A + 1) - (A - 1) * cw - s);
        break;
    }
    }
}

// PolyBLEP correction for a step discontinuity at phase 0 of a naive
// oscillator (p = phase in [0, 1), dt = phase step per sample).
static inline float poly_blep(double p, double dt) {
    if (p < dt) {
        p /= dt;
        return (float)(p + p - p * p - 1.0);
    }
    if (p > 1.0 - dt) {
        p = (p - 1.0) / dt;
        return (float)(p * p + p + p + 1.0);
    }
    return 0.0f;
}

// One-pole lowpass (used for damping in feedback paths).
typedef struct {
    float z, a;
} OnePole;

static inline void onepole_set(OnePole *f, float freq, double rate) {
    f->a = expf(-2.0f * (float)M_PI * clampf(freq, 10.0f, (float)rate * 0.49f) / (float)rate);
}

static inline float onepole_run(OnePole *f, float x) {
    f->z = x + (f->z - x) * f->a;
    return f->z;
}

// Find the urid:map feature (NULL when the host does not provide it).
static inline LV2_URID_Map *find_map(const LV2_Feature *const *features) {
    for (int i = 0; features && features[i]; i++) {
        if (!strcmp(features[i]->URI, LV2_URID__map))
            return (LV2_URID_Map *)features[i]->data;
    }
    return NULL;
}

// Prepare an output atom port for writing: returns its capacity and turns
// it into an empty sequence.
static inline uint32_t midi_out_begin(LV2_Atom_Sequence *out, LV2_URID sequence_type) {
    if (!out)
        return 0;
    uint32_t capacity = out->atom.size;
    lv2_atom_sequence_clear(out);
    out->atom.type = sequence_type;
    return capacity;
}

// Append a MIDI message of up to 3 bytes to an output sequence.
static inline void midi_out_write(LV2_Atom_Sequence *out, uint32_t capacity, LV2_URID midi_type,
                                  int64_t frames, const uint8_t *msg, uint32_t size) {
    struct {
        LV2_Atom_Event event;
        uint8_t msg[3];
    } ev;
    if (!out || size == 0 || size > 3)
        return;
    ev.event.time.frames = frames;
    ev.event.body.type = midi_type;
    ev.event.body.size = size;
    memcpy(ev.msg, msg, size);
    lv2_atom_sequence_append_event(out, capacity, &ev.event);
}

// Every plugin exports one descriptor (see zestins.c).
#define ZESTINS_DESCRIPTOR(var, name, inst, conn, act, run_fn, clean)                   \
    const LV2_Descriptor var = { ZESTINS_URI(name), inst, conn, act, run_fn, NULL, clean, \
                                 NULL }

#endif
