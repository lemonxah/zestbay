// Minimal LADSPA plugin used by the ABI tests: out = in * gain.
#include <ladspa.h>
#include <stdlib.h>

enum { P_GAIN, P_IN, P_OUT, P_LEVEL, P_COUNT };

typedef struct {
    LADSPA_Data *port[P_COUNT];
    int activated;
} Plug;

static LADSPA_Handle inst(const LADSPA_Descriptor *d, unsigned long sr) {
    (void)d; (void)sr;
    return calloc(1, sizeof(Plug));
}
static void connect(LADSPA_Handle h, unsigned long p, LADSPA_Data *loc) {
    ((Plug *)h)->port[p] = loc;
}
static void activate(LADSPA_Handle h) { ((Plug *)h)->activated = 1; }
static void run(LADSPA_Handle h, unsigned long n) {
    Plug *p = h;
    LADSPA_Data g = *p->port[P_GAIN];
    for (unsigned long i = 0; i < n; i++)
        p->port[P_OUT][i] = p->port[P_IN][i] * g;
    *p->port[P_LEVEL] = n ? p->port[P_OUT][n - 1] : 0;
}
static void cleanup(LADSPA_Handle h) { free(h); }

static const LADSPA_PortDescriptor pd[P_COUNT] = {
    LADSPA_PORT_INPUT | LADSPA_PORT_CONTROL,
    LADSPA_PORT_INPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_AUDIO,
    LADSPA_PORT_OUTPUT | LADSPA_PORT_CONTROL,
};
static const char *const names[P_COUNT] = {"Gain", "Input", "Output", "Level"};
static const LADSPA_PortRangeHint hints[P_COUNT] = {
    {LADSPA_HINT_BOUNDED_BELOW | LADSPA_HINT_BOUNDED_ABOVE | LADSPA_HINT_DEFAULT_1, 0.0f, 2.0f},
    {0, 0, 0}, {0, 0, 0}, {0, 0, 0},
};

#ifdef SECOND_PLUGIN
static LADSPA_Descriptor second;
#endif
static LADSPA_Descriptor first = {
    .UniqueID = 4242, .Label = "test_gain", .Properties = LADSPA_PROPERTY_HARD_RT_CAPABLE,
    .Name = "Test Gain", .Maker = "ZestBay", .Copyright = "None",
    .PortCount = P_COUNT, .PortDescriptors = pd, .PortNames = names, .PortRangeHints = hints,
    .instantiate = inst, .connect_port = connect, .activate = activate, .run = run,
    .cleanup = cleanup,
};

const LADSPA_Descriptor *ladspa_descriptor(unsigned long index) {
#ifdef SECOND_PLUGIN
    if (index == 1) { second = first; second.UniqueID = 4243; second.Label = "other_gain"; second.Name = "Other Gain"; return &second; }
#endif
    return index == 0 ? &first : 0;
}
