// Zestins Delay: stereo echo up to 2 s.  The feedback path is lowpassed by
// `tone`; ping-pong sends each echo to the other side.  The delay time is
// smoothed and read with linear interpolation, so changing it glides.

#include "dsp.h"

typedef struct {
    void *port[DELAY_N];
    double rate;
    float *buf[2];
    uint32_t size; // power of two
    uint32_t pos;
    Smooth time; // samples
    OnePole damp[2];
} Delay;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Delay *dl = calloc(1, sizeof(Delay));
    if (!dl)
        return NULL;
    dl->rate = rate;
    uint32_t need = (uint32_t)(2.1 * rate) + 4;
    dl->size = 1;
    while (dl->size < need)
        dl->size <<= 1;
    dl->buf[0] = calloc(dl->size, sizeof(float));
    dl->buf[1] = calloc(dl->size, sizeof(float));
    if (!dl->buf[0] || !dl->buf[1]) {
        free(dl->buf[0]);
        free(dl->buf[1]);
        free(dl);
        return NULL;
    }
    return dl;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < DELAY_N)
        ((Delay *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Delay *dl = h;
    memset(dl->buf[0], 0, dl->size * sizeof(float));
    memset(dl->buf[1], 0, dl->size * sizeof(float));
    dl->pos = 0;
    smooth_init(&dl->time, ctl(dl->port[DELAY_TIME], 375) * 0.001f * (float)dl->rate, dl->rate);
    dl->time.coef = time_coef(80.0f, dl->rate); // glide over ~80 ms
    dl->damp[0].z = dl->damp[1].z = 0;
}

static float read_at(const Delay *dl, int c, float delay) {
    float p = (float)dl->pos - delay;
    while (p < 0)
        p += (float)dl->size;
    uint32_t i0 = (uint32_t)p;
    float frac = p - (float)i0;
    uint32_t mask = dl->size - 1;
    float a = dl->buf[c][i0 & mask], b = dl->buf[c][(i0 + 1) & mask];
    return a + (b - a) * frac;
}

static void run(LV2_Handle h, uint32_t n) {
    Delay *dl = h;
    const float *in_l = dl->port[DELAY_IN_L], *in_r = dl->port[DELAY_IN_R];
    float *out_l = dl->port[DELAY_OUT_L], *out_r = dl->port[DELAY_OUT_R];
    if (!in_l || !in_r || !out_l || !out_r)
        return;

    float max_delay = (float)(dl->size - 2);
    float target = clampf(ctl(dl->port[DELAY_TIME], 375) * 0.001f * (float)dl->rate, 1, max_delay);
    float feedback = clampf(ctl(dl->port[DELAY_FEEDBACK], 40) * 0.01f, 0, 0.95f);
    float mix = clampf(ctl(dl->port[DELAY_MIX], 30) * 0.01f, 0, 1);
    bool ping_pong = ctl(dl->port[DELAY_PING_PONG], 0) > 0.5f;
    float tone = ctl(dl->port[DELAY_TONE], 8000);
    onepole_set(&dl->damp[0], tone, dl->rate);
    onepole_set(&dl->damp[1], tone, dl->rate);
    uint32_t mask = dl->size - 1;

    for (uint32_t i = 0; i < n; i++) {
        float d = smooth_next(&dl->time, target);
        float wet_l = read_at(dl, 0, d), wet_r = read_at(dl, 1, d);
        float fb_l = onepole_run(&dl->damp[0], wet_l) * feedback;
        float fb_r = onepole_run(&dl->damp[1], wet_r) * feedback;
        float x_l = in_l[i], x_r = in_r[i];
        if (ping_pong) {
            // Mono input enters on the left; echoes alternate sides.
            dl->buf[0][dl->pos] = 0.5f * (x_l + x_r) + fb_r;
            dl->buf[1][dl->pos] = fb_l;
        } else {
            dl->buf[0][dl->pos] = x_l + fb_l;
            dl->buf[1][dl->pos] = x_r + fb_r;
        }
        dl->pos = (dl->pos + 1) & mask;
        out_l[i] = x_l * (1 - mix) + wet_l * mix;
        out_r[i] = x_r * (1 - mix) + wet_r * mix;
    }
}

static void cleanup(LV2_Handle h) {
    Delay *dl = h;
    free(dl->buf[0]);
    free(dl->buf[1]);
    free(dl);
}

ZESTINS_DESCRIPTOR(zestins_delay, "delay", instantiate, connect, activate, run, cleanup);
