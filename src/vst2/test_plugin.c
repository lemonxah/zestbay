/* Independent test fixture: no SDK headers. Compile as a Linux shared
 * library to exercise Rust/C ABI boundaries and the complete host lifecycle. */
#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

typedef struct Effect Effect;
typedef intptr_t (*Callback)(Effect *, int32_t, int32_t, intptr_t, void *, float);
typedef void (*Process)(Effect *, float **, float **, int32_t);
struct Effect {
    int32_t magic;
    Callback dispatcher;
    Process process;
    void (*set)(Effect *, int32_t, float);
    float (*get)(Effect *, int32_t);
    int32_t programs, params, inputs, outputs, flags;
    intptr_t reserved1, reserved2;
    int32_t latency, quality[2];
    float ratio;
    void *object, *user;
    int32_t id, version;
    Process replacing;
    void (*double_replacing)(Effect *, double **, double **, int32_t);
    char future[56];
};
typedef struct {
    int32_t type, bytes, delta, flags, length, offset;
    uint8_t data[4];
    int8_t detune;
    uint8_t velocity, reserved[2];
} Midi;
typedef struct { int32_t count; intptr_t reserved; Midi *events[2]; } Events;
typedef struct { int16_t top, left, bottom, right; } Rect;
typedef struct {
    Effect effect;
    Callback host;
    float values[2], rate;
    int block, open, active, started, editor, shell_index, midi_offset, program;
} Instance;

#if UINTPTR_MAX == UINT64_MAX
_Static_assert(sizeof(Effect) == 192, "AEffect size");
_Static_assert(offsetof(Effect, replacing) == 120, "processReplacing offset");
_Static_assert(offsetof(Events, events) == 16, "event array offset");
#endif
_Static_assert(sizeof(Midi) == 32, "MIDI event size");

static void set(Effect *e, int32_t i, float value) {
    Instance *s = (Instance *)e;
    assert(i >= 0 && i < e->params);
    s->values[i] = value;
}
static float get(Effect *e, int32_t i) { return ((Instance *)e)->values[i]; }
static void process(Effect *e, float **in, float **out, int32_t n) {
    Instance *s = (Instance *)e;
    assert(s->active && s->started && n <= s->block);
    assert(s->host(e, 16, 0, 0, NULL, 0) == (intptr_t)s->rate);
    assert(s->host(e, 23, 0, 0, NULL, 0) == 2);
    assert(s->host(e, 7, 0, 0, NULL, 0) != 0);
    for (int ch = 0; ch < e->outputs; ch++) {
        for (int i = 0; i < n; i++) {
            float value = (ch < e->inputs ? in[ch][i] : 0) * s->values[0];
            if (i >= s->midi_offset) value += 0.25f;
#ifdef LEGACY
            out[ch][i] += value;
#else
            out[ch][i] = value;
#endif
        }
    }
    s->midi_offset = INT32_MAX;
}
static intptr_t dispatch(Effect *e, int32_t op, int32_t index, intptr_t value, void *ptr, float opt) {
    Instance *s = (Instance *)e;
    static Rect rect = {0, 0, 100, 200};
    switch (op) {
    case 0: s->open = 1; break;
    case 1:
        assert(s->open && !s->active && !s->started && !s->editor);
        free(s); break;
    case 2: s->program = value; s->values[0] = value ? 0.75f : 0.5f; break;
    case 3: return s->program;
    case 8: strcpy(ptr, index ? "Toggle" : "Gain"); return 1;
    case 10: s->rate = opt; break;
    case 11: s->block = value; break;
    case 12:
        assert(s->open && s->rate > 0 && s->block > 0);
        s->active = value != 0; break;
    case 13: *(Rect **)ptr = &rect; return 1;
    case 14:
        assert(!s->editor && ptr);
        s->editor = 1;
        assert(s->host(e, 15, 320, 180, NULL, 0) == 1);
        break;
    case 15: assert(s->editor); s->editor = 0; break;
    case 19: assert(s->editor); break;
    case 23:
        *(void **)ptr = s->values;
        return sizeof(s->values);
    case 24:
        assert(!s->active && value == sizeof(s->values));
        memcpy(s->values, ptr, sizeof(s->values));
        return 0; /* Valid plugins often return zero here. */
    case 25: {
        Events *events = ptr;
        for (int i = 0; i < events->count; i++) {
            Midi *m = events->events[i];
            assert(m->type == 1 && m->bytes == 32);
            if ((m->data[0] & 0xf0) == 0x90) s->midi_offset = m->delta;
        }
        return 1;
    }
    case 35:
#ifdef SHELL
        if (e->id == 0x53484c4c) return 10;
#endif
        return 1;
    case 45: strcpy(ptr, "Fixture Gain"); return 1;
    case 47: strcpy(ptr, "ZestBay tests"); return 1;
    case 51: return strcmp(ptr, "receiveVstMidiEvent") == 0;
    case 56:
        /* flags is at byte offset 76 in VstParameterProperties. */
        if (index == 1) { *(int32_t *)((char *)ptr + 76) = 1; return 1; }
        return 0;
    case 70:
#ifdef SHELL
        if (s->shell_index++ < 2) { strcpy(ptr, "Shell child"); return 0x54535430 + s->shell_index; }
#endif
        return 0;
    case 71: assert(s->active); s->started = 1; break;
    case 72: s->started = 0; break;
    /* Test-only messages simulate plugin-driven callbacks. */
    case 1000:
        s->values[index] = opt;
        s->host(e, 0, index, 0, NULL, opt);
        break;
    case 1001: e->outputs = 1; s->host(e, 13, 0, 0, NULL, 0); break;
    case 1002: s->values[0] = opt; s->host(e, 42, 0, 0, NULL, 0); break;
    default: break;
    }
    return 0;
}

#ifdef LEGACY
Effect *entry(Callback host) __asm__("main");
#else
#define entry VSTPluginMain
#endif
Effect *entry(Callback host) {
    assert(host(NULL, 1, 0, 0, NULL, 0) == 2400);
    Instance *s = calloc(1, sizeof(*s));
    s->host = host;
    s->effect = (Effect){
        .magic = 0x56737450, .dispatcher = dispatch,
        .process = process, .set = set, .get = get,
        .programs = 2, .params = 2, .inputs = 2, .outputs = 2,
        .flags = 1 | 16, .id = 0x54535431, .version = 1,
#ifndef LEGACY
        .replacing = process,
#endif
    };
#ifndef NO_CHUNKS
    s->effect.flags |= 32;
#endif
#ifdef SHELL
    int32_t id = host(NULL, 2, 0, 0, NULL, 0);
    s->effect.id = id ? id : 0x53484c4c;
#endif
#ifdef INVALID
    s->effect.outputs = -1;
#endif
    s->values[0] = 0.5f;
    s->midi_offset = INT32_MAX;
    return &s->effect;
}
