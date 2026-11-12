// Zestins Meter: peak (with a 20 dB/s fall) and RMS (300 ms) per channel,
// reported in dBFS on control outputs.  Audio passes through.

#include "dsp.h"

typedef struct {
    void *port[METER_N];
    double rate;
    float peak[2];
    float ms[2]; // running mean square
    float ms_coef;
} Meter;

static LV2_Handle instantiate(const LV2_Descriptor *d, double rate, const char *path,
                              const LV2_Feature *const *features) {
    (void)d, (void)path, (void)features;
    Meter *m = calloc(1, sizeof(Meter));
    if (m) {
        m->rate = rate;
        m->ms_coef = time_coef(300.0f, rate);
    }
    return m;
}

static void connect(LV2_Handle h, uint32_t port, void *data) {
    if (port < METER_N)
        ((Meter *)h)->port[port] = data;
}

static void activate(LV2_Handle h) {
    Meter *m = h;
    m->peak[0] = m->peak[1] = 0;
    m->ms[0] = m->ms[1] = 0;
}

static void report(float *port, float db) {
    if (port)
        *port = clampf(db, -70.0f, 6.0f);
}

static void run(LV2_Handle h, uint32_t n) {
    Meter *m = h;
    const float *in[2] = { m->port[METER_IN_L], m->port[METER_IN_R] };
    float *out[2] = { m->port[METER_OUT_L], m->port[METER_OUT_R] };
    // Peak falls by 20 dB per second.
    float fall = powf(10.0f, -1.0f / (float)m->rate);

    for (int c = 0; c < 2; c++) {
        if (!in[c] || !out[c])
            continue;
        float peak = m->peak[c], ms = m->ms[c];
        for (uint32_t i = 0; i < n; i++) {
            float x = in[c][i];
            out[c][i] = x;
            float a = fabsf(x);
            peak = a > peak ? a : peak * fall;
            ms = x * x + (ms - x * x) * m->ms_coef;
        }
        m->peak[c] = peak;
        m->ms[c] = ms;
    }
    report(m->port[METER_PEAK_L], gain_to_db(m->peak[0]));
    report(m->port[METER_PEAK_R], gain_to_db(m->peak[1]));
    report(m->port[METER_RMS_L], gain_to_db(sqrtf(m->ms[0])));
    report(m->port[METER_RMS_R], gain_to_db(sqrtf(m->ms[1])));
}

static void cleanup(LV2_Handle h) { free(h); }

ZESTINS_DESCRIPTOR(zestins_meter, "meter", instantiate, connect, activate, run, cleanup);
