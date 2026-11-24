#include <assert.h>
#include "../dsp/midi_utility.c"

static uint32_t process(MidiUtility *instance, const uint8_t *message, uint32_t size,
                        uint8_t result[3]) {
    uint64_t input_buffer[16] = {0};
    uint64_t output_buffer[16] = {0};
    LV2_Atom_Sequence *input = (LV2_Atom_Sequence *)input_buffer;
    LV2_Atom_Sequence *output = (LV2_Atom_Sequence *)output_buffer;
    input->atom.type = 1;
    input->atom.size = sizeof(LV2_Atom_Sequence_Body) + sizeof(LV2_Atom_Event) + lv2_atom_pad_size(size);
    LV2_Atom_Event *event = (LV2_Atom_Event *)(input + 1);
    event->body.type = 2;
    event->body.size = size;
    memcpy(event + 1, message, size);
    output->atom.size = sizeof(output_buffer) - sizeof(LV2_Atom);
    instance->atom_sequence = 1;
    instance->midi_event = 2;
    instance->port[MUTIL_MIDI_IN] = input;
    instance->port[MUTIL_MIDI_OUT] = output;
    run(instance, 64);
    if (output->atom.size == sizeof(LV2_Atom_Sequence_Body))
        return 0;
    const LV2_Atom_Event *out = (const LV2_Atom_Event *)(output + 1);
    assert(out->body.size <= 3);
    memcpy(result, out + 1, out->body.size);
    return out->body.size;
}

int main(void) {
    MidiUtility instance = {0};
    uint8_t result[3];
    for (unsigned status = 0x80; status < 0xF0; status++) {
        unsigned type = status & 0xF0;
        unsigned expected = type == 0xC0 || type == 0xD0 ? 2 : 3;
        for (unsigned size = 0; size <= 4; size++) {
            uint8_t message[4] = {(uint8_t)status, 60, 100, 0};
            assert(midi_message_valid(message, size) == (size == expected));
            if (size != expected)
                assert(process(&instance, message, size, result) == 0);
        }
        for (unsigned bad = 128; bad <= 255; bad++) {
            uint8_t message[3] = {(uint8_t)status, (uint8_t)bad, 100};
            assert(process(&instance, message, expected, result) == 0);
            message[1] = 60;
            message[2] = (uint8_t)bad;
            if (expected == 3)
                assert(process(&instance, message, expected, result) == 0);
        }
    }
    float transpose = 12;
    instance.port[MUTIL_TRANSPOSE] = &transpose;
    const uint8_t on[3] = {0x9f, 60, 100};
    const uint8_t off[3] = {0x8f, 60, 0};
    assert(process(&instance, on, 3, result) == 3);
    assert(result[0] == 0x9f && result[1] == 72 && result[2] == 100);
    transpose = -12;
    assert(process(&instance, off, 3, result) == 3);
    assert(result[0] == 0x8f && result[1] == 72 && result[2] == 0);
    return 0;
}
