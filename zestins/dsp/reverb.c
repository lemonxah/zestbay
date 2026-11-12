// Zestins Reverb: a Schroeder/Moorer room (the classic "Freeverb" layout):
// eight damped feedback combs in parallel feeding four allpasses, per
// channel, with the right channel's delays slightly longer for width.
// Freeze holds the current tail indefinitely.

#include "dsp.h"

#define COMBS 8
#define ALLPASSES 4
#define SPREAD 23
#define FIXED_GAIN 0.015f

// Delay lengths in samples at 44.1 kHz; scaled to the actual rate.
static const int comb_tuning[COMBS] = { 1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617 };
static const int allpass_tuning[ALLPASSES] = { 556, 441, 341, 225 };

typedef struct {
    float *buf;
    uint32_t size, pos;
    float store; // damping lowpass state
} Comb;

typedef struct {
    float *buf;
    uint32_t size, pos;
} Allpass;

typedef struct {
    void *port[VERB_N];
    double rate;
    Comb comb[2][COMBS];
    Allpass allpass[2][ALLPASSES];
    Smooth wet, dry;
} Reverb;

static void cleanup(LV2_Handle h) {
    Reverb *r = h;
    for (int c = 0; c < 2; c++) {
        for (int i = 0; i < COMBS; i++)
            free(r->comb[c][i].buf);
        for (int i = 0; i < ALLPASSES; i++)
            free(r->allpass[c][i].buf);
    }
    free(r);
}

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Reverb *r = calloc(1, sizeof(Reverb));
    if (!r)
        return NULL;
    r->rate = rate;
    double scale = rate / 44100.0;
    for (int c = 0; c < 2; c++) {
        int spread = c ? SPREAD : 0;
        for (int i = 0; i < COMBS; i++) {
            Comb *cb = &r->comb[c][i];
            cb->size = (uint32_t)((comb_tuning[i] + spread) * scale) + 1;
            cb->buf = calloc(cb->size, sizeof(float));
            if (!cb->buf)
                goto fail;
        }
        for (int i = 0; i < ALLPASSES; i++) {
            Allpass *ap = &r->allpass[c][i];
            ap->size = (uint32_t)((allpass_tuning[i] + spread) * scale) + 1;
            ap->buf = calloc(ap->size, sizeof(float));
            if (!ap->buf)
                goto fail;
        }
    }
    return r;
fail:
    cleanup(r);
    return NULL;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < VERB_N)
        ((Reverb *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Reverb *r = h;
    for (int c = 0; c < 2; c++) {
        for (int i = 0; i < COMBS; i++) {
            memset(r->comb[c][i].buf, 0, r->comb[c][i].size * sizeof(float));
            r->comb[c][i].pos = 0;
            r->comb[c][i].store = 0;
        }
        for (int i = 0; i < ALLPASSES; i++) {
            memset(r->allpass[c][i].buf, 0, r->allpass[c][i].size * sizeof(float));
            r->allpass[c][i].pos = 0;
        }
    }
    float mix = clampf(ctl(r->port[VERB_MIX], 25) * 0.01f, 0, 1);
    smooth_init(&r->wet, mix * 3.0f, r->rate);
    smooth_init(&r->dry, 1.0f - mix, r->rate);
}

static inline float comb_run(Comb *cb, float x, float feedback, float damp) {
    float y = cb->buf[cb->pos];
    cb->store = y * (1.0f - damp) + cb->store * damp + 1e-20f;
    cb->buf[cb->pos] = x + cb->store * feedback;
    if (++cb->pos >= cb->size)
        cb->pos = 0;
    return y;
}

static inline float allpass_run(Allpass *ap, float x) {
    float b = ap->buf[ap->pos];
    ap->buf[ap->pos] = x + b * 0.5f + 1e-20f;
    if (++ap->pos >= ap->size)
        ap->pos = 0;
    return b - x;
}

static void run(LV2_Handle h, uint32_t n) {
    Reverb *r = h;
    const float *in_l = r->port[VERB_IN_L], *in_r = r->port[VERB_IN_R];
    float *out_l = r->port[VERB_OUT_L], *out_r = r->port[VERB_OUT_R];
    if (!in_l || !in_r || !out_l || !out_r)
        return;

    bool freeze = ctl(r->port[VERB_FREEZE], 0) > 0.5f;
    float size = clampf(ctl(r->port[VERB_SIZE], 50) * 0.01f, 0, 1);
    float feedback = freeze ? 1.0f : size * 0.28f + 0.7f;
    float damp = freeze ? 0.0f : clampf(ctl(r->port[VERB_DAMPING], 50) * 0.01f, 0, 1) * 0.4f;
    float input_gain = freeze ? 0.0f : FIXED_GAIN;
    float width = clampf(ctl(r->port[VERB_WIDTH], 100) * 0.01f, 0, 1);
    float mix = clampf(ctl(r->port[VERB_MIX], 25) * 0.01f, 0, 1);
    float wet_target = mix * 3.0f, dry_target = 1.0f - mix;
    float wet1 = 0.5f * width + 0.5f, wet2 = 0.5f * (1.0f - width);

    for (uint32_t i = 0; i < n; i++) {
        float x = (in_l[i] + in_r[i]) * input_gain;
        float acc[2] = { 0, 0 };
        for (int c = 0; c < 2; c++) {
            for (int k = 0; k < COMBS; k++)
                acc[c] += comb_run(&r->comb[c][k], x, feedback, damp);
            for (int k = 0; k < ALLPASSES; k++)
                acc[c] = allpass_run(&r->allpass[c][k], acc[c]);
        }
        float wet = smooth_next(&r->wet, wet_target);
        float dry = smooth_next(&r->dry, dry_target);
        float l = in_l[i], rr = in_r[i];
        out_l[i] = l * dry + (acc[0] * wet1 + acc[1] * wet2) * wet;
        out_r[i] = rr * dry + (acc[1] * wet1 + acc[0] * wet2) * wet;
    }
}

ZESTINS_DESCRIPTOR(zestins_reverb, "reverb", instantiate, connect, activate, run, cleanup);
