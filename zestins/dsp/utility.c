// Zestins Utility: gain, balance, mute, polarity flip and mono sum.

#include "dsp.h"

typedef struct {
    void *port[UTIL_N];
    double rate;
    Smooth gain_l, gain_r;
} Utility;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Utility *u = calloc(1, sizeof(Utility));
    if (u)
        u->rate = rate;
    return u;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < UTIL_N)
        ((Utility *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Utility *u = h;
    smooth_init(&u->gain_l, 1.0f, u->rate);
    smooth_init(&u->gain_r, 1.0f, u->rate);
}

static void run(LV2_Handle h, uint32_t n) {
    Utility *u = h;
    const float *in_l = u->port[UTIL_IN_L], *in_r = u->port[UTIL_IN_R];
    float *out_l = u->port[UTIL_OUT_L], *out_r = u->port[UTIL_OUT_R];
    if (!in_l || !in_r || !out_l || !out_r)
        return;

    float gain = ctl(u->port[UTIL_MUTE], 0) > 0.5f ? 0.0f : db_to_gain(ctl(u->port[UTIL_GAIN], 0));
    float pan = clampf(ctl(u->port[UTIL_PAN], 0), -1, 1);
    // Balance: turning one side down leaves the other at unity.
    float target_l = gain * fminf(1.0f, 1.0f - pan);
    float target_r = gain * fminf(1.0f, 1.0f + pan);
    if (ctl(u->port[UTIL_INVERT_L], 0) > 0.5f)
        target_l = -target_l;
    if (ctl(u->port[UTIL_INVERT_R], 0) > 0.5f)
        target_r = -target_r;
    bool mono = ctl(u->port[UTIL_MONO], 0) > 0.5f;

    for (uint32_t i = 0; i < n; i++) {
        float l = in_l[i], r = in_r[i];
        if (mono)
            l = r = 0.5f * (l + r);
        out_l[i] = l * smooth_next(&u->gain_l, target_l);
        out_r[i] = r * smooth_next(&u->gain_r, target_r);
    }
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_utility, "utility", instantiate, connect, activate, run, cleanup);
