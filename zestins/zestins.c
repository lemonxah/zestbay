// Entry point of zestins.so: the LV2 descriptor of every Zestins plugin.

#include <stddef.h>

#include <lv2/core/lv2.h>

extern const LV2_Descriptor zestins_utility, zestins_meter, zestins_eq, zestins_gate,
    zestins_compressor, zestins_delay, zestins_tone, zestins_synth, zestins_midi_switch,
    zestins_midi_utility, zestins_reverb;

static const LV2_Descriptor *const descriptors[] = {
    &zestins_utility, &zestins_meter, &zestins_eq,          &zestins_gate,
    &zestins_compressor, &zestins_delay, &zestins_tone,     &zestins_synth,
    &zestins_midi_switch, &zestins_midi_utility, &zestins_reverb,
};

LV2_SYMBOL_EXPORT const LV2_Descriptor *lv2_descriptor(uint32_t index) {
    return index < sizeof(descriptors) / sizeof(descriptors[0]) ? descriptors[index] : NULL;
}
