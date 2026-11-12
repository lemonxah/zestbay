// Zestins Compressor: feed-forward, stereo linked (louder channel), soft
// knee, attack/release smoothing in the dB domain, makeup gain.

#include "dsp.h"

typedef struct {
    void *port[COMP_N];
    double rate;
    float reduction_db; // smoothed gain reduction, >= 0
} Compressor;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Compressor *c = calloc(1, sizeof(Compressor));
    if (c)
        c->rate = rate;
    return c;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < COMP_N)
        ((Compressor *)h)->port[port] = data;
}

static void activate(LV2_Handle h) { ((Compressor *)h)->reduction_db = 0; }

// Static curve: gain reduction (dB, >= 0) for an input level in dB.
static float curve(float in_db, float threshold, float ratio, float knee) {
    float over = in_db - threshold;
    float slope = 1.0f - 1.0f / ratio;
    if (knee > 0 && fabsf(over) <= knee * 0.5f) {
        float k = over + knee * 0.5f;
        return slope * k * k / (2.0f * knee);
    }
    return over > 0 ? slope * over : 0.0f;
}

static void run(LV2_Handle h, uint32_t n) {
    Compressor *c = h;
    const float *in_l = c->port[COMP_IN_L], *in_r = c->port[COMP_IN_R];
    float *out_l = c->port[COMP_OUT_L], *out_r = c->port[COMP_OUT_R];
    if (!in_l || !in_r || !out_l || !out_r)
        return;

    float threshold = ctl(c->port[COMP_THRESHOLD], -18);
    float ratio = fmaxf(1.0f, ctl(c->port[COMP_RATIO], 4));
    float attack = time_coef(ctl(c->port[COMP_ATTACK], 10), c->rate);
    float release = time_coef(ctl(c->port[COMP_RELEASE], 150), c->rate);
    float knee = fmaxf(0.0f, ctl(c->port[COMP_KNEE], 6));
    float makeup = db_to_gain(ctl(c->port[COMP_MAKEUP], 0));

    float max_reduction = 0;
    for (uint32_t i = 0; i < n; i++) {
        float level = gain_to_db(fmaxf(fabsf(in_l[i]), fabsf(in_r[i])));
        float target = curve(level, threshold, ratio, knee);
        float coef = target > c->reduction_db ? attack : release;
        c->reduction_db = target + (c->reduction_db - target) * coef;
        float g = db_to_gain(-c->reduction_db) * makeup;
        out_l[i] = in_l[i] * g;
        out_r[i] = in_r[i] * g;
        if (c->reduction_db > max_reduction)
            max_reduction = c->reduction_db;
    }
    if (c->port[COMP_REDUCTION])
        *(float *)c->port[COMP_REDUCTION] = clampf(max_reduction, 0, 30);
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_compressor, "compressor", instantiate, connect, activate, run, cleanup);
