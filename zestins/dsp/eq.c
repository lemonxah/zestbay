// Zestins EQ: low cut (12 dB/oct), low shelf, mid peak and high shelf.
// Filters are redesigned only when a control changes.

#include "dsp.h"

enum { BAND_HP, BAND_LOW, BAND_MID, BAND_HIGH, BANDS };

typedef struct {
    void *port[EQ_N];
    double rate;
    Biquad f[2][BANDS];
    float last[EQ_N];
} Eq;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Eq *e = calloc(1, sizeof(Eq));
    if (e)
        e->rate = rate;
    return e;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < EQ_N)
        ((Eq *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Eq *e = h;
    for (int i = 0; i < EQ_N; i++)
        e->last[i] = NAN; // force a redesign on the first run
    memset(e->f, 0, sizeof(e->f));
}

static float param(Eq *e, int idx, float def, bool *changed) {
    float v = ctl(e->port[idx], def);
    if (v != e->last[idx]) {
        e->last[idx] = v;
        *changed = true;
    }
    return v;
}

static void design(Eq *e, int band, BiquadType type, double freq, double q, double gain) {
    for (int c = 0; c < 2; c++) {
        Biquad *f = &e->f[c][band];
        float z1 = f->z1, z2 = f->z2;
        biquad_design(f, type, e->rate, freq, q, gain);
        f->z1 = z1, f->z2 = z2; // keep the filter state: no click on changes
    }
}

static void run(LV2_Handle h, uint32_t n) {
    Eq *e = h;
    bool changed = false;
    float hp = param(e, EQ_HP_FREQ, 10, &changed);
    float low_gain = param(e, EQ_LOW_GAIN, 0, &changed);
    float low_freq = param(e, EQ_LOW_FREQ, 120, &changed);
    float mid_gain = param(e, EQ_MID_GAIN, 0, &changed);
    float mid_freq = param(e, EQ_MID_FREQ, 1000, &changed);
    float mid_q = param(e, EQ_MID_Q, 0.7f, &changed);
    float high_gain = param(e, EQ_HIGH_GAIN, 0, &changed);
    float high_freq = param(e, EQ_HIGH_FREQ, 8000, &changed);
    if (changed) {
        design(e, BAND_HP, BQ_HIGHPASS, hp, M_SQRT1_2, 0);
        design(e, BAND_LOW, BQ_LOWSHELF, low_freq, M_SQRT1_2, low_gain);
        design(e, BAND_MID, BQ_PEAK, mid_freq, fmax(mid_q, 0.1), mid_gain);
        design(e, BAND_HIGH, BQ_HIGHSHELF, high_freq, M_SQRT1_2, high_gain);
    }

    const float *in[2] = { e->port[EQ_IN_L], e->port[EQ_IN_R] };
    float *out[2] = { e->port[EQ_OUT_L], e->port[EQ_OUT_R] };
    for (int c = 0; c < 2; c++) {
        if (!in[c] || !out[c])
            continue;
        Biquad *f = e->f[c];
        for (uint32_t i = 0; i < n; i++) {
            float x = in[c][i];
            for (int b = 0; b < BANDS; b++)
                x = biquad_run(&f[b], x);
            out[c][i] = x;
        }
    }
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_eq, "eq", instantiate, connect, activate, run, cleanup);
