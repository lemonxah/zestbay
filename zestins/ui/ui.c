// Zestins UI: one X11 UI for every Zestins plugin, laid out from the port
// specs in spec.c.  Controls become knobs, toggles, segmented choices or
// steppers; control outputs become level meters or activity lights.
//
// Embedded in the host's window (ui:parent) and driven by ui:idleInterface;
// draws with plain Xlib into a back buffer.
//
// Knobs: drag up/down (Shift for fine), mouse wheel, double-click to reset.

#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <lv2/ui/ui.h>

#include "../spec.h"

#define MAX_WIDGETS 32
#define PAD 14
#define HEADER 36
#define GAP 10
#define CELL_W 78
#define CELL_H 92
#define CHOICE_H 58 // label + button row
#define KNOB_D 44
#define METER_H 20
#define LED_H 22
#define STEPPER_W 112
#define DOUBLE_CLICK_MS 350

typedef enum { W_KNOB, W_TOGGLE, W_CHOICE, W_STEPPER, W_METER, W_LED } WidgetKind;

typedef struct {
    WidgetKind kind;
    uint32_t port;
    const ZPort *spec;
    int x, y, w, h;
    float value;
} Widget;

enum { C_BG, C_HEADER, C_BORDER, C_TRACK, C_TEXT, C_MUTED, C_ACCENT, C_ACCENT_DIM, C_HOT,
       C_AMBER, C_KNOB, N_COLORS };

static const char *const color_hex[N_COLORS] = {
    [C_BG] = "#1a1d23",     [C_HEADER] = "#14161b", [C_BORDER] = "#2a2f38",
    [C_TRACK] = "#2f353f",  [C_TEXT] = "#e8eaee",   [C_MUTED] = "#8b93a0",
    [C_ACCENT] = "#c9e265", [C_ACCENT_DIM] = "#4a5530", [C_HOT] = "#e5534b",
    [C_AMBER] = "#e0a84b",  [C_KNOB] = "#242830",
};

typedef struct {
    const ZPlugin *plugin;
    LV2UI_Write_Function write;
    LV2UI_Controller controller;
    LV2UI_Resize *resize;

    Display *dpy;
    Window win;
    Pixmap back;
    GC gc;
    XFontStruct *font, *title_font;
    unsigned long color[N_COLORS];
    int width, height;

    Widget widget[MAX_WIDGETS];
    int n_widgets;
    int drag;        // widget being dragged, -1 for none
    int drag_y;
    float drag_norm;
    int last_click;  // widget of the previous click, for double-click
    Time last_click_time;
    bool dirty;
} UI;

// ---- Values ---------------------------------------------------------------

static bool is_log(const ZPort *p) { return (p->flags & ZF_LOG) && p->min > 0; }

static float to_norm(const ZPort *p, float v) {
    if (p->max <= p->min)
        return 0;
    float n = is_log(p) ? logf(v / p->min) / logf(p->max / p->min)
                        : (v - p->min) / (p->max - p->min);
    return n < 0 ? 0 : n > 1 ? 1 : n;
}

static float from_norm(const ZPort *p, float n) {
    n = n < 0 ? 0 : n > 1 ? 1 : n;
    float v = is_log(p) ? p->min * powf(p->max / p->min, n) : p->min + n * (p->max - p->min);
    if (p->flags & (ZF_INTEGER | ZF_ENUM | ZF_TOGGLE))
        v = roundf(v);
    return v;
}

static int n_labels(const ZPort *p) {
    int n = 0;
    while (p->labels && p->labels[n])
        n++;
    return n;
}

static void format_value(const ZPort *p, float v, char *buf, size_t len) {
    const char *u = p->unit;
    if (!strcmp(p->symbol, "pan")) {
        if (fabsf(v) < 0.005f)
            snprintf(buf, len, "C");
        else
            snprintf(buf, len, "%s %.0f", v < 0 ? "L" : "R", fabsf(v) * 100);
    } else if (!strcmp(p->symbol, "ratio")) {
        snprintf(buf, len, "%.1f:1", v);
    } else if (!strcmp(p->symbol, "fixed_velocity") && v < 0.5f) {
        snprintf(buf, len, "Off");
    } else if (!u) {
        snprintf(buf, len, (p->flags & ZF_INTEGER) ? "%.0f" : "%.2f", v);
    } else if (!strcmp(u, "Hz")) {
        if (v >= 1000)
            snprintf(buf, len, "%.2f kHz", v / 1000);
        else
            snprintf(buf, len, "%.0f Hz", v);
    } else if (!strcmp(u, "ms")) {
        snprintf(buf, len, v < 10 ? "%.1f ms" : "%.0f ms", v);
    } else if (!strcmp(u, "%")) {
        snprintf(buf, len, "%.0f%%", v);
    } else if (!strcmp(u, "st")) {
        snprintf(buf, len, "%+.0f st", v);
    } else {
        snprintf(buf, len, "%.1f %s", v, u);
    }
}

static void set_value(UI *ui, Widget *w, float v) {
    const ZPort *p = w->spec;
    v = v < p->min ? p->min : v > p->max ? p->max : v;
    if (p->flags & (ZF_INTEGER | ZF_ENUM | ZF_TOGGLE))
        v = roundf(v);
    if (v == w->value)
        return;
    w->value = v;
    ui->write(ui->controller, w->port, sizeof(float), 0, &v);
    ui->dirty = true;
}

// ---- Layout ---------------------------------------------------------------

static int text_width(XFontStruct *f, const char *s) {
    return f ? XTextWidth(f, s, (int)strlen(s)) : (int)strlen(s) * 7;
}

static int choice_segment_w(UI *ui, const ZPort *p) {
    int w = 34;
    for (int i = 0; p->labels && p->labels[i]; i++) {
        int t = text_width(ui->font, p->labels[i]) + 16;
        if (t > w)
            w = t;
    }
    return w;
}

static void layout(UI *ui) {
    const ZPlugin *pl = ui->plugin;
    int x = PAD, y = HEADER + PAD, row_h = 0, max_x = 0;
    ui->n_widgets = 0;

    // Controls, flowing left to right; ZF_ROW starts a new row.
    for (int i = 0; i < pl->n_ports && ui->n_widgets < MAX_WIDGETS; i++) {
        const ZPort *p = &pl->ports[i];
        if (p->kind != ZP_CONTROL_IN)
            continue;
        Widget *w = &ui->widget[ui->n_widgets++];
        w->port = (uint32_t)i;
        w->spec = p;
        w->value = p->def;
        if (p->flags & ZF_TOGGLE) {
            w->kind = W_TOGGLE;
            w->w = CELL_W;
        } else if (p->flags & ZF_ENUM) {
            int n = n_labels(p);
            if (n <= 5) {
                w->kind = W_CHOICE;
                w->w = n * choice_segment_w(ui, p);
            } else {
                w->kind = W_STEPPER;
                w->w = STEPPER_W;
            }
        } else {
            w->kind = W_KNOB;
            w->w = CELL_W;
        }
        w->h = (w->kind == W_CHOICE || w->kind == W_STEPPER) ? CHOICE_H : CELL_H;
        if ((p->flags & ZF_ROW) && x > PAD) {
            x = PAD;
            y += row_h + GAP;
            row_h = 0;
        }
        w->x = x;
        w->y = y;
        x += w->w + GAP;
        if (x - GAP > max_x)
            max_x = x - GAP;
        if (w->h > row_h)
            row_h = w->h;
    }
    if (row_h)
        y += row_h + GAP;

    int width = max_x + PAD;
    if (width < 360)
        width = 360;

    // Meters: one full-width bar per output.
    bool any_output = false;
    for (int i = 0; i < pl->n_ports && ui->n_widgets < MAX_WIDGETS; i++) {
        const ZPort *p = &pl->ports[i];
        if (p->kind != ZP_CONTROL_OUT || !(p->flags & ZF_METER))
            continue;
        if (!any_output)
            y += 4;
        any_output = true;
        Widget *w = &ui->widget[ui->n_widgets++];
        *w = (Widget){ W_METER, (uint32_t)i, p, PAD, y, width - 2 * PAD, METER_H, p->min };
        y += METER_H + 6;
    }

    // Lights, in one row.
    x = PAD;
    bool any_led = false;
    for (int i = 0; i < pl->n_ports && ui->n_widgets < MAX_WIDGETS; i++) {
        const ZPort *p = &pl->ports[i];
        if (p->kind != ZP_CONTROL_OUT || !(p->flags & ZF_LED))
            continue;
        if (!any_led && any_output)
            y += 4;
        any_led = true;
        Widget *w = &ui->widget[ui->n_widgets++];
        int lw = 26 + text_width(ui->font, p->name);
        *w = (Widget){ W_LED, (uint32_t)i, p, x, y, lw, LED_H, 0 };
        x += lw + GAP;
    }
    if (any_led)
        y += LED_H + 6;

    ui->width = width;
    ui->height = y + PAD - (any_output || any_led ? 6 : GAP);
}

// ---- Drawing --------------------------------------------------------------

static void color(UI *ui, int c) { XSetForeground(ui->dpy, ui->gc, ui->color[c]); }

static void fill_round(UI *ui, int x, int y, int w, int h, int r, int c) {
    color(ui, c);
    if (r * 2 > h)
        r = h / 2;
    if (r * 2 > w)
        r = w / 2;
    XFillRectangle(ui->dpy, ui->back, ui->gc, x + r, y, (unsigned)(w - 2 * r), (unsigned)h);
    XFillRectangle(ui->dpy, ui->back, ui->gc, x, y + r, (unsigned)w, (unsigned)(h - 2 * r));
    unsigned d = (unsigned)(2 * r);
    XFillArc(ui->dpy, ui->back, ui->gc, x, y, d, d, 90 * 64, 90 * 64);
    XFillArc(ui->dpy, ui->back, ui->gc, x + w - 2 * r, y, d, d, 0, 90 * 64);
    XFillArc(ui->dpy, ui->back, ui->gc, x, y + h - 2 * r, d, d, 180 * 64, 90 * 64);
    XFillArc(ui->dpy, ui->back, ui->gc, x + w - 2 * r, y + h - 2 * r, d, d, 270 * 64, 90 * 64);
}

static void text(UI *ui, XFontStruct *f, int x, int y, const char *s, int c) {
    color(ui, c);
    if (f)
        XSetFont(ui->dpy, ui->gc, f->fid);
    XDrawString(ui->dpy, ui->back, ui->gc, x, y, s, (int)strlen(s));
}

// Text centred horizontally on cx, with its baseline at y.
static void text_c(UI *ui, int cx, int y, const char *s, int c) {
    text(ui, ui->font, cx - text_width(ui->font, s) / 2, y, s, c);
}

static int ascent(XFontStruct *f) { return f ? f->ascent : 10; }

static void draw_knob(UI *ui, const Widget *w) {
    const ZPort *p = w->spec;
    int cx = w->x + w->w / 2;
    text_c(ui, cx, w->y + ascent(ui->font), p->name, C_MUTED);

    int kx = cx - KNOB_D / 2, ky = w->y + 20;
    float n = to_norm(p, w->value);
    // 270 degrees from bottom-left (225) clockwise to bottom-right (-45).
    XSetLineAttributes(ui->dpy, ui->gc, 4, LineSolid, CapRound, JoinRound);
    color(ui, C_TRACK);
    XDrawArc(ui->dpy, ui->back, ui->gc, kx, ky, KNOB_D, KNOB_D, 225 * 64, -270 * 64);
    // Bipolar controls fill from their zero point.
    float zero = (p->min < 0 && p->max > 0) ? to_norm(p, 0) : 0;
    int a0 = (int)((225 - 270 * zero) * 64);
    int sweep = (int)(-270 * (n - zero) * 64);
    if (sweep != 0) {
        color(ui, C_ACCENT);
        XDrawArc(ui->dpy, ui->back, ui->gc, kx, ky, KNOB_D, KNOB_D, a0, sweep);
    }
    // Cap and pointer.
    color(ui, C_KNOB);
    XFillArc(ui->dpy, ui->back, ui->gc, kx + 7, ky + 7, KNOB_D - 14, KNOB_D - 14, 0, 360 * 64);
    double ang = (225.0 - 270.0 * n) * M_PI / 180.0;
    int r0 = 4, r1 = KNOB_D / 2 - 9;
    int ccx = kx + KNOB_D / 2, ccy = ky + KNOB_D / 2;
    XSetLineAttributes(ui->dpy, ui->gc, 2, LineSolid, CapRound, JoinRound);
    color(ui, C_TEXT);
    XDrawLine(ui->dpy, ui->back, ui->gc, ccx + (int)(r0 * cos(ang)), ccy - (int)(r0 * sin(ang)),
              ccx + (int)(r1 * cos(ang)), ccy - (int)(r1 * sin(ang)));
    XSetLineAttributes(ui->dpy, ui->gc, 1, LineSolid, CapButt, JoinMiter);

    char buf[32];
    format_value(p, w->value, buf, sizeof(buf));
    text_c(ui, cx, ky + KNOB_D + 8 + ascent(ui->font), buf, C_TEXT);
}

static void draw_toggle(UI *ui, const Widget *w) {
    int cx = w->x + w->w / 2;
    text_c(ui, cx, w->y + ascent(ui->font), w->spec->name, C_MUTED);
    bool on = w->value > 0.5f;
    int bw = 56, bh = 26, bx = cx - bw / 2, by = w->y + 20 + (KNOB_D - bh) / 2;
    fill_round(ui, bx, by, bw, bh, bh / 2, on ? C_ACCENT : C_TRACK);
    int dot = bh - 8;
    int dx = on ? bx + bw - dot - 4 : bx + 4;
    color(ui, on ? C_HEADER : C_MUTED);
    XFillArc(ui->dpy, ui->back, ui->gc, dx, by + 4, (unsigned)dot, (unsigned)dot, 0, 360 * 64);
    text_c(ui, cx, w->y + 20 + KNOB_D + 8 + ascent(ui->font), on ? "On" : "Off",
           on ? C_TEXT : C_MUTED);
}

static void draw_choice(UI *ui, const Widget *w) {
    const ZPort *p = w->spec;
    text(ui, ui->font, w->x, w->y + ascent(ui->font), p->name, C_MUTED);
    int n = n_labels(p);
    int seg = w->w / (n ? n : 1);
    int by = w->y + 22;
    fill_round(ui, w->x, by, w->w, 28, 6, C_TRACK);
    int sel = (int)roundf(w->value - p->min);
    for (int i = 0; i < n; i++) {
        int sx = w->x + i * seg;
        if (i == sel)
            fill_round(ui, sx + 2, by + 2, seg - 4, 24, 5, C_ACCENT);
        text_c(ui, sx + seg / 2, by + 14 + ascent(ui->font) / 2 - 1, p->labels[i],
               i == sel ? C_HEADER : C_TEXT);
    }
}

static void draw_stepper(UI *ui, const Widget *w) {
    const ZPort *p = w->spec;
    text(ui, ui->font, w->x, w->y + ascent(ui->font), p->name, C_MUTED);
    int by = w->y + 22;
    fill_round(ui, w->x, by, w->w, 28, 6, C_TRACK);
    int idx = (int)roundf(w->value - p->min);
    const char *label = (idx >= 0 && idx < n_labels(p)) ? p->labels[idx] : "?";
    int mid = by + 14 + ascent(ui->font) / 2 - 1;
    text_c(ui, w->x + 14, mid, "<", C_MUTED);
    text_c(ui, w->x + w->w - 14, mid, ">", C_MUTED);
    text_c(ui, w->x + w->w / 2, mid, label, C_ACCENT);
}

static void draw_meter(UI *ui, const Widget *w) {
    const ZPort *p = w->spec;
    int label_w = 78, value_w = 64;
    int mid = w->y + w->h / 2 + ascent(ui->font) / 2 - 1;
    text(ui, ui->font, w->x, mid, p->name, C_MUTED);
    int bx = w->x + label_w, bw = w->w - label_w - value_w, bh = 8;
    int by = w->y + (w->h - bh) / 2;
    fill_round(ui, bx, by, bw, bh, 4, C_TRACK);
    float n = to_norm(p, w->value);
    bool reduction = !strcmp(p->symbol, "reduction");
    int fill = (int)(n * (float)bw);
    if (fill > 0) {
        if (reduction) {
            fill_round(ui, bx, by, fill, bh, 4, C_AMBER);
        } else {
            // Above 0 dBFS turns red.
            int zero = p->max > 0 ? (int)(to_norm(p, 0) * (float)bw) : bw;
            fill_round(ui, bx, by, fill < zero ? fill : zero, bh, 4, C_ACCENT);
            if (fill > zero)
                fill_round(ui, bx + zero, by, fill - zero, bh, 2, C_HOT);
        }
    }
    char buf[32];
    if (!reduction && w->value <= p->min)
        snprintf(buf, sizeof(buf), "-inf");
    else
        format_value(p, w->value, buf, sizeof(buf));
    int tw = text_width(ui->font, buf);
    text(ui, ui->font, w->x + w->w - tw, mid, buf, C_TEXT);
}

static void draw_led(UI *ui, const Widget *w) {
    bool on = w->value > 0.5f;
    int d = 10, ly = w->y + (w->h - d) / 2;
    if (on) {
        color(ui, C_ACCENT_DIM);
        XFillArc(ui->dpy, ui->back, ui->gc, w->x - 3, ly - 3, (unsigned)(d + 6),
                 (unsigned)(d + 6), 0, 360 * 64);
    }
    color(ui, on ? C_ACCENT : C_TRACK);
    XFillArc(ui->dpy, ui->back, ui->gc, w->x, ly, (unsigned)d, (unsigned)d, 0, 360 * 64);
    text(ui, ui->font, w->x + d + 8, w->y + w->h / 2 + ascent(ui->font) / 2 - 1, w->spec->name,
         on ? C_TEXT : C_MUTED);
}

static void draw(UI *ui) {
    color(ui, C_BG);
    XFillRectangle(ui->dpy, ui->back, ui->gc, 0, 0, (unsigned)ui->width, (unsigned)ui->height);
    color(ui, C_HEADER);
    XFillRectangle(ui->dpy, ui->back, ui->gc, 0, 0, (unsigned)ui->width, HEADER);
    color(ui, C_BORDER);
    XDrawLine(ui->dpy, ui->back, ui->gc, 0, HEADER, ui->width, HEADER);

    // "Zestins MIDI Switch" -> brand mark + "MIDI Switch".
    const char *name = ui->plugin->name;
    if (!strncmp(name, "Zestins ", 8))
        name += 8;
    color(ui, C_ACCENT);
    XFillArc(ui->dpy, ui->back, ui->gc, PAD, HEADER / 2 - 5, 10, 10, 0, 360 * 64);
    int base = HEADER / 2 + ascent(ui->title_font) / 2 - 1;
    text(ui, ui->title_font, PAD + 18, base, name, C_TEXT);
    const char *brand = "ZESTINS";
    text(ui, ui->font, ui->width - PAD - text_width(ui->font, brand), base, brand, C_MUTED);

    for (int i = 0; i < ui->n_widgets; i++) {
        const Widget *w = &ui->widget[i];
        switch (w->kind) {
        case W_KNOB: draw_knob(ui, w); break;
        case W_TOGGLE: draw_toggle(ui, w); break;
        case W_CHOICE: draw_choice(ui, w); break;
        case W_STEPPER: draw_stepper(ui, w); break;
        case W_METER: draw_meter(ui, w); break;
        case W_LED: draw_led(ui, w); break;
        }
    }
    XCopyArea(ui->dpy, ui->back, ui->win, ui->gc, 0, 0, (unsigned)ui->width,
              (unsigned)ui->height, 0, 0);
    XFlush(ui->dpy);
    ui->dirty = false;
}

// ---- Input ----------------------------------------------------------------

static int widget_at(UI *ui, int x, int y) {
    for (int i = 0; i < ui->n_widgets; i++) {
        const Widget *w = &ui->widget[i];
        if (w->kind != W_METER && w->kind != W_LED && x >= w->x && x < w->x + w->w &&
            y >= w->y && y < w->y + w->h)
            return i;
    }
    return -1;
}

static void step(UI *ui, Widget *w, int dir) {
    const ZPort *p = w->spec;
    if (p->flags & (ZF_INTEGER | ZF_ENUM | ZF_TOGGLE))
        set_value(ui, w, w->value + (float)dir);
    else
        set_value(ui, w, from_norm(p, to_norm(p, w->value) + 0.01f * (float)dir));
}

static void press(UI *ui, const XButtonEvent *e) {
    int i = widget_at(ui, e->x, e->y);
    if (i < 0)
        return;
    Widget *w = &ui->widget[i];
    if (e->button == Button4 || e->button == Button5) {
        step(ui, w, e->button == Button4 ? 1 : -1);
        return;
    }
    if (e->button != Button1)
        return;
    bool dbl = ui->last_click == i && e->time - ui->last_click_time < DOUBLE_CLICK_MS;
    ui->last_click = i;
    ui->last_click_time = e->time;
    switch (w->kind) {
    case W_KNOB:
        if (dbl) {
            set_value(ui, w, w->spec->def);
            return;
        }
        ui->drag = i;
        ui->drag_y = e->y;
        ui->drag_norm = to_norm(w->spec, w->value);
        break;
    case W_TOGGLE:
        set_value(ui, w, w->value > 0.5f ? 0.0f : 1.0f);
        break;
    case W_CHOICE: {
        int n = n_labels(w->spec);
        int seg = (e->x - w->x) * n / w->w;
        set_value(ui, w, w->spec->min + (float)seg);
        break;
    }
    case W_STEPPER:
        step(ui, w, e->x < w->x + w->w / 2 ? -1 : 1);
        break;
    default:
        break;
    }
}

static void motion(UI *ui, const XMotionEvent *e) {
    if (ui->drag < 0)
        return;
    Widget *w = &ui->widget[ui->drag];
    float range = (e->state & ShiftMask) ? 1000.0f : 200.0f;
    set_value(ui, w, from_norm(w->spec, ui->drag_norm + (float)(ui->drag_y - e->y) / range));
}

// ---- LV2 UI ---------------------------------------------------------------

static unsigned long alloc_color(Display *dpy, const char *hex, unsigned long fallback) {
    XColor c;
    Colormap cmap = DefaultColormap(dpy, DefaultScreen(dpy));
    if (XParseColor(dpy, cmap, hex, &c) && XAllocColor(dpy, cmap, &c))
        return c.pixel;
    return fallback;
}

static XFontStruct *load_font(Display *dpy, const char *const *names) {
    for (int i = 0; names[i]; i++) {
        XFontStruct *f = XLoadQueryFont(dpy, names[i]);
        if (f)
            return f;
    }
    return NULL;
}

static void cleanup(LV2UI_Handle h) {
    UI *ui = h;
    if (ui->dpy) {
        if (ui->font)
            XFreeFont(ui->dpy, ui->font);
        if (ui->title_font && ui->title_font != ui->font)
            XFreeFont(ui->dpy, ui->title_font);
        if (ui->back)
            XFreePixmap(ui->dpy, ui->back);
        if (ui->gc)
            XFreeGC(ui->dpy, ui->gc);
        if (ui->win)
            XDestroyWindow(ui->dpy, ui->win);
        XCloseDisplay(ui->dpy);
    }
    free(ui);
}

static LV2UI_Handle instantiate(const LV2UI_Descriptor *desc, const char *plugin_uri,
                                const char *bundle_path, LV2UI_Write_Function write,
                                LV2UI_Controller controller, LV2UI_Widget *widget,
                                const LV2_Feature *const *features) {
    (void)bundle_path;
    const ZPlugin *plugin = zestins_find(plugin_uri);
    for (int i = 0; !plugin && i < zestins_n_plugins; i++) {
        if (!strcmp(zestins_plugins[i].ui_uri, desc->URI))
            plugin = &zestins_plugins[i];
    }
    Window parent = 0;
    LV2UI_Resize *resize = NULL;
    for (int i = 0; features && features[i]; i++) {
        if (!strcmp(features[i]->URI, LV2_UI__parent))
            parent = (Window)(uintptr_t)features[i]->data;
        else if (!strcmp(features[i]->URI, LV2_UI__resize))
            resize = features[i]->data;
    }
    if (!plugin || !parent)
        return NULL;

    UI *ui = calloc(1, sizeof(UI));
    if (!ui)
        return NULL;
    ui->plugin = plugin;
    ui->write = write;
    ui->controller = controller;
    ui->resize = resize;
    ui->drag = -1;
    ui->last_click = -1;
    ui->dpy = XOpenDisplay(NULL);
    if (!ui->dpy) {
        free(ui);
        return NULL;
    }

    static const char *const fonts[] = {
        "-*-dejavu sans-medium-r-normal--12-*-*-*-p-*-iso10646-1",
        "-*-helvetica-medium-r-normal--12-*-*-*-p-*-iso8859-1",
        "-misc-fixed-medium-r-semicondensed--13-*-*-*-*-*-iso8859-1", "fixed", NULL,
    };
    static const char *const title_fonts[] = {
        "-*-dejavu sans-bold-r-normal--14-*-*-*-p-*-iso10646-1",
        "-*-helvetica-bold-r-normal--14-*-*-*-p-*-iso8859-1",
        "-misc-fixed-bold-r-semicondensed--13-*-*-*-*-*-iso8859-1", NULL,
    };
    ui->font = load_font(ui->dpy, fonts);
    ui->title_font = load_font(ui->dpy, title_fonts);
    if (!ui->title_font)
        ui->title_font = ui->font;

    int screen = DefaultScreen(ui->dpy);
    for (int c = 0; c < N_COLORS; c++)
        ui->color[c] = alloc_color(ui->dpy, color_hex[c],
                                   c == C_TEXT ? WhitePixel(ui->dpy, screen)
                                               : BlackPixel(ui->dpy, screen));

    layout(ui);
    ui->win = XCreateSimpleWindow(ui->dpy, parent, 0, 0, (unsigned)ui->width,
                                  (unsigned)ui->height, 0, 0, ui->color[C_BG]);
    XSelectInput(ui->dpy, ui->win,
                 ExposureMask | ButtonPressMask | ButtonReleaseMask | PointerMotionMask |
                     StructureNotifyMask);
    ui->gc = XCreateGC(ui->dpy, ui->win, 0, NULL);
    ui->back = XCreatePixmap(ui->dpy, ui->win, (unsigned)ui->width, (unsigned)ui->height,
                             (unsigned)DefaultDepth(ui->dpy, screen));
    XMapWindow(ui->dpy, ui->win);
    if (resize)
        resize->ui_resize(resize->handle, ui->width, ui->height);
    ui->dirty = true;
    XFlush(ui->dpy);

    *widget = (LV2UI_Widget)(uintptr_t)ui->win;
    return ui;
}

static void port_event(LV2UI_Handle h, uint32_t port, uint32_t size, uint32_t format,
                       const void *buffer) {
    UI *ui = h;
    if (format != 0 || size != sizeof(float))
        return;
    float v = *(const float *)buffer;
    for (int i = 0; i < ui->n_widgets; i++) {
        Widget *w = &ui->widget[i];
        if (w->port == port && w->value != v) {
            w->value = v;
            ui->dirty = true;
        }
    }
}

static int idle(LV2UI_Handle h) {
    UI *ui = h;
    while (XPending(ui->dpy)) {
        XEvent e;
        XNextEvent(ui->dpy, &e);
        switch (e.type) {
        case Expose:
            ui->dirty = true;
            break;
        case ButtonPress:
            press(ui, &e.xbutton);
            break;
        case ButtonRelease:
            if (e.xbutton.button == Button1)
                ui->drag = -1;
            break;
        case MotionNotify:
            motion(ui, &e.xmotion);
            break;
        default:
            break;
        }
    }
    if (ui->dirty)
        draw(ui);
    return 0;
}

static const LV2UI_Idle_Interface idle_iface = { idle };

static const void *extension_data(const char *uri) {
    return strcmp(uri, LV2_UI__idleInterface) ? NULL : &idle_iface;
}

LV2_SYMBOL_EXPORT const LV2UI_Descriptor *lv2ui_descriptor(uint32_t index) {
    static LV2UI_Descriptor descriptors[64];
    static bool ready = false;
    if (!ready) {
        for (int i = 0; i < zestins_n_plugins && i < 64; i++) {
            descriptors[i] = (LV2UI_Descriptor){ zestins_plugins[i].ui_uri, instantiate, cleanup,
                                                 port_event, extension_data };
        }
        ready = true;
    }
    return index < (uint32_t)zestins_n_plugins ? &descriptors[index] : NULL;
}
