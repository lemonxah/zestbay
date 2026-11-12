// Writes the LV2 data (manifest.ttl and one .ttl per plugin) of the Zestins
// bundle from the specs in spec.c.  Usage: ttlgen <bundle directory>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "spec.h"

#define PREFIXES                                                         \
    "@prefix atom:   <http://lv2plug.in/ns/ext/atom#> .\n"               \
    "@prefix doap:   <http://usefulinc.com/ns/doap#> .\n"                \
    "@prefix foaf:   <http://xmlns.com/foaf/0.1/> .\n"                   \
    "@prefix lv2:    <http://lv2plug.in/ns/lv2core#> .\n"                \
    "@prefix midi:   <http://lv2plug.in/ns/ext/midi#> .\n"               \
    "@prefix pprops: <http://lv2plug.in/ns/ext/port-props#> .\n"         \
    "@prefix rdf:    <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"  \
    "@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .\n"        \
    "@prefix ui:     <http://lv2plug.in/ns/extensions/ui#> .\n"          \
    "@prefix units:  <http://lv2plug.in/ns/extensions/units#> .\n"       \
    "@prefix urid:   <http://lv2plug.in/ns/ext/urid#> .\n\n"

static const char *bundle_dir;

// "urn:zestins:midi-switch" -> "midi-switch"
static const char *short_name(const ZPlugin *p) {
    const char *colon = strrchr(p->uri, ':');
    return colon ? colon + 1 : p->uri;
}

static FILE *open_out(const char *name) {
    char path[4096];
    snprintf(path, sizeof(path), "%s/%s", bundle_dir, name);
    FILE *f = fopen(path, "w");
    if (!f) {
        perror(path);
        exit(1);
    }
    return f;
}

static const char *unit_uri(const char *unit) {
    static const char *const map[][2] = {
        { "dB", "units:db" }, { "ms", "units:ms" }, { "Hz", "units:hz" },
        { "%", "units:pc" },  { "st", "units:semitone12TET" },
    };
    for (size_t i = 0; unit && i < sizeof(map) / sizeof(map[0]); i++) {
        if (!strcmp(unit, map[i][0]))
            return map[i][1];
    }
    return NULL;
}

static int has_midi(const ZPlugin *p) {
    for (int i = 0; i < p->n_ports; i++) {
        if (p->ports[i].kind == ZP_MIDI_IN || p->ports[i].kind == ZP_MIDI_OUT)
            return 1;
    }
    return 0;
}

static void write_port(FILE *f, const ZPort *port, int index) {
    static const char *const classes[] = {
        [ZP_AUDIO_IN] = "lv2:InputPort , lv2:AudioPort",
        [ZP_AUDIO_OUT] = "lv2:OutputPort , lv2:AudioPort",
        [ZP_CONTROL_IN] = "lv2:InputPort , lv2:ControlPort",
        [ZP_CONTROL_OUT] = "lv2:OutputPort , lv2:ControlPort",
        [ZP_MIDI_IN] = "lv2:InputPort , atom:AtomPort",
        [ZP_MIDI_OUT] = "lv2:OutputPort , atom:AtomPort",
    };
    fprintf(f, "[\n        a %s ;\n", classes[port->kind]);
    fprintf(f, "        lv2:index %d ;\n        lv2:symbol \"%s\" ;\n        lv2:name \"%s\"",
            index, port->symbol, port->name);
    if (port->kind == ZP_MIDI_IN || port->kind == ZP_MIDI_OUT) {
        fprintf(f, " ;\n        atom:bufferType atom:Sequence ;\n"
                   "        atom:supports midi:MidiEvent");
    }
    if (port->kind == ZP_CONTROL_IN || port->kind == ZP_CONTROL_OUT) {
        if (port->kind == ZP_CONTROL_IN)
            fprintf(f, " ;\n        lv2:default %g", port->def);
        fprintf(f, " ;\n        lv2:minimum %g ;\n        lv2:maximum %g", port->min, port->max);
        const char *props[4];
        int n = 0;
        if (port->flags & ZF_TOGGLE)
            props[n++] = "lv2:toggled";
        if (port->flags & ZF_INTEGER)
            props[n++] = "lv2:integer";
        if (port->flags & ZF_ENUM)
            props[n++] = "lv2:enumeration";
        if (port->flags & ZF_LOG)
            props[n++] = "pprops:logarithmic";
        for (int i = 0; i < n; i++)
            fprintf(f, "%s%s", i ? " , " : " ;\n        lv2:portProperty ", props[i]);
        const char *unit = unit_uri(port->unit);
        if (unit)
            fprintf(f, " ;\n        units:unit %s", unit);
        if (port->labels) {
            for (int i = 0; port->labels[i]; i++) {
                fprintf(f, "%s[ rdfs:label \"%s\" ; rdf:value %g ]",
                        i ? " ,\n            " : " ;\n        lv2:scalePoint ", port->labels[i],
                        port->min + (float)i);
            }
        }
    }
    fprintf(f, "\n    ]");
}

static void write_plugin(const ZPlugin *p) {
    char name[256];
    snprintf(name, sizeof(name), "%s.ttl", short_name(p));
    FILE *f = open_out(name);
    fputs(PREFIXES, f);
    fprintf(f, "<urn:zestins>\n    a doap:Project ;\n    doap:name \"Zestins\" ;\n"
               "    doap:maintainer [ foaf:name \"ZestBay\" ] .\n\n");
    fprintf(f, "<%s>\n    a lv2:Plugin , %s ;\n", p->uri, p->lv2_class);
    fprintf(f, "    doap:name \"%s\" ;\n", p->name);
    fprintf(f, "    doap:license <https://spdx.org/licenses/MIT> ;\n");
    fprintf(f, "    lv2:project <urn:zestins> ;\n");
    fprintf(f, "    rdfs:comment \"%s\" ;\n", p->comment);
    fprintf(f, "    lv2:optionalFeature lv2:hardRTCapable ;\n");
    if (has_midi(p))
        fprintf(f, "    lv2:requiredFeature urid:map ;\n");
    fprintf(f, "    ui:ui <%s> ;\n    lv2:port ", p->ui_uri);
    for (int i = 0; i < p->n_ports; i++) {
        if (i)
            fprintf(f, " , ");
        write_port(f, &p->ports[i], i);
    }
    fprintf(f, " .\n\n");

    // UI: an X11 UI driven by the host's idle calls; outputs are reported
    // to it through port notifications.
    fprintf(f, "<%s>\n    a ui:X11UI ;\n", p->ui_uri);
    fprintf(f, "    lv2:requiredFeature ui:parent ;\n");
    fprintf(f, "    lv2:optionalFeature ui:resize ;\n");
    fprintf(f, "    lv2:extensionData ui:idleInterface");
    for (int i = 0; i < p->n_ports; i++) {
        if (p->ports[i].kind != ZP_CONTROL_OUT)
            continue;
        fprintf(f, " ;\n    ui:portNotification [\n        ui:plugin <%s> ;\n"
                   "        lv2:symbol \"%s\" ;\n        ui:protocol ui:floatProtocol\n    ]",
                p->uri, p->ports[i].symbol);
    }
    fprintf(f, " .\n");
    fclose(f);
}

static void write_manifest(void) {
    FILE *f = open_out("manifest.ttl");
    fputs(PREFIXES, f);
    for (int i = 0; i < zestins_n_plugins; i++) {
        const ZPlugin *p = &zestins_plugins[i];
        fprintf(f, "<%s>\n    a lv2:Plugin ;\n    lv2:binary <zestins.so> ;\n"
                   "    rdfs:seeAlso <%s.ttl> .\n\n",
                p->uri, short_name(p));
        fprintf(f, "<%s>\n    a ui:X11UI ;\n    ui:binary <zestins_ui.so> ;\n"
                   "    rdfs:seeAlso <%s.ttl> .\n\n",
                p->ui_uri, short_name(p));
    }
    fclose(f);
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <bundle directory>\n", argv[0]);
        return 2;
    }
    bundle_dir = argv[1];
    write_manifest();
    for (int i = 0; i < zestins_n_plugins; i++)
        write_plugin(&zestins_plugins[i]);
    return 0;
}
