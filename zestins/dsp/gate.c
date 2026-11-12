// Zestins Gate: opens when the louder channel crosses the threshold, stays
// open for the hold time, then closes down to `range` with the release time.

#include "dsp.h"

typedef struct {
    void *port[GATE_N];
    double rate;
    float env;      // fast peak envelope of the input
    float gain;     // current gain (range..1)
    uint32_t hold;  // samples left before the gate may close
} Gate;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Gate *g = calloc(1, sizeof(Gate));
    if (g)
        g->rate = rate;
    return g;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < GATE_N)
        ((Gate *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Gate *g = h;
    g->env = 0;
    g->gain = 0;
    g->hold = 0;
}

static void run(LV2_Handle h, uint32_t n) {
    Gate *g = h;
    const float *in_l = g->port[GATE_IN_L], *in_r = g->port[GATE_IN_R];
    float *out_l = g->port[GATE_OUT_L], *out_r = g->port[GATE_OUT_R];
    if (!in_l || !in_r || !out_l || !out_r)
        return;

    float threshold = db_to_gain(ctl(g->port[GATE_THRESHOLD], -50));
    float attack = time_coef(ctl(g->port[GATE_ATTACK], 1), g->rate);
    float release = time_coef(ctl(g->port[GATE_RELEASE], 150), g->rate);
    uint32_t hold = (uint32_t)(ctl(g->port[GATE_HOLD], 50) * 0.001f * (float)g->rate);
    float floor = db_to_gain(ctl(g->port[GATE_RANGE], -90));
    float env_release = time_coef(10.0f, g->rate);

    bool open = false;
    for (uint32_t i = 0; i < n; i++) {
        float a = fmaxf(fabsf(in_l[i]), fabsf(in_r[i]));
        g->env = a > g->env ? a : a + (g->env - a) * env_release;
        if (g->env >= threshold)
            g->hold = hold + 1;
        float target;
        float coef;
        if (g->hold > 0) {
            g->hold--;
            target = 1.0f;
            coef = attack;
            open = true;
        } else {
            target = floor;
            coef = release;
        }
        g->gain = target + (g->gain - target) * coef;
        out_l[i] = in_l[i] * g->gain;
        out_r[i] = in_r[i] * g->gain;
    }
    if (g->port[GATE_OPEN])
        *(float *)g->port[GATE_OPEN] = open ? 1.0f : 0.0f;
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_gate, "gate", instantiate, connect, activate, run, cleanup);
