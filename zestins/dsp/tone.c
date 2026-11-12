// Zestins Tone Generator: sine, square, saw (both band-limited with
// PolyBLEP), white or pink noise at a set level.  Same signal on both sides.

#include "dsp.h"

typedef struct {
    void *port[TONE_N];
    double rate;
    double phase;
    uint32_t rng;
    float pink[7];
    Smooth level;
} Tone;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Tone *t = calloc(1, sizeof(Tone));
    if (t)
        t->rate = rate;
    return t;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < TONE_N)
        ((Tone *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Tone *t = h;
    t->phase = 0;
    t->rng = 0x2545F491u;
    memset(t->pink, 0, sizeof(t->pink));
    smooth_init(&t->level, 0, t->rate);
}

// White noise in [-1, 1) from a xorshift32 generator.
static float white(Tone *t) {
    uint32_t x = t->rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    t->rng = x;
    return (float)x / 2147483648.0f - 1.0f;
}

// Pink noise: Paul Kellet's filter on white noise.
static float pink(Tone *t) {
    float w = white(t);
    float *b = t->pink;
    b[0] = 0.99886f * b[0] + w * 0.0555179f;
    b[1] = 0.99332f * b[1] + w * 0.0750759f;
    b[2] = 0.96900f * b[2] + w * 0.1538520f;
    b[3] = 0.86650f * b[3] + w * 0.3104856f;
    b[4] = 0.55000f * b[4] + w * 0.5329522f;
    b[5] = -0.7616f * b[5] - w * 0.0168980f;
    float out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362f;
    b[6] = w * 0.115926f;
    return out * 0.11f;
}

static void run(LV2_Handle h, uint32_t n) {
    Tone *t = h;
    float *out_l = t->port[TONE_OUT_L], *out_r = t->port[TONE_OUT_R];
    if (!out_l || !out_r)
        return;
    int wave = (int)(ctl(t->port[TONE_WAVE], 0) + 0.5f);
    double freq = clampf(ctl(t->port[TONE_FREQ], 440), 1, (float)t->rate * 0.45f);
    double dt = freq / t->rate;
    float level = ctl(t->port[TONE_ON], 1) > 0.5f ? db_to_gain(ctl(t->port[TONE_LEVEL], -18)) : 0;

    for (uint32_t i = 0; i < n; i++) {
        double p = t->phase;
        float s;
        switch (wave) {
        case TONE_SQUARE: {
            double q = p + 0.5;
            s = (p < 0.5 ? 1.0f : -1.0f) + poly_blep(p, dt) - poly_blep(q - floor(q), dt);
            break;
        }
        case TONE_SAW:
            s = (float)(2.0 * p - 1.0) - poly_blep(p, dt);
            break;
        case TONE_WHITE:
            s = white(t);
            break;
        case TONE_PINK:
            s = pink(t);
            break;
        default:
            s = (float)sin(2.0 * M_PI * p);
            break;
        }
        t->phase += dt;
        if (t->phase >= 1.0)
            t->phase -= 1.0;
        float v = s * smooth_next(&t->level, level);
        out_l[i] = v;
        out_r[i] = v;
    }
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_tone, "tone", instantiate, connect, activate, run, cleanup);
