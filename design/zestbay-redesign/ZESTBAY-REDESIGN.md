# ZestBay — "Zest Dark" UI redesign spec

Handoff document for implementing the redesign in the Qt 6 frontend.
Source of truth for the visuals is the design canvas ("ZestBay redesign"); this
file carries every token, size and behaviour so the work can be done without
measuring screenshots. Values are CSS-style px at 1× — map them 1:1 to
device-independent pixels in Qt.

Scope covered: patchbay window, Patchbay Rules, Add Plugin, Preferences, About,
CPU Usage, Plugin Parameters, the three context menus, a dark **and** a light
theme, and the new logo (three candidates, pick one).

---

## 1. Design direction

- **Ground**: cool graphite, never pure black. The patchbay canvas carries a
  24 px dot grid so pan/zoom reads as movement.
- **One accent**: citrus lime in dark mode, a deeper "pressed lime" in light
  mode (bright lime fails contrast on white). The accent is used for exactly:
  primary action, switches, selection ring, selected wires, the "On" state,
  the Rules-active chip, and the logo. Nowhere else.
- **Nodes are quiet cards**: no coloured title bars. Category is a small glyph
  chip in the header; the signal type is the port pin colour; connection state
  is filled (connected) vs hollow (free) pins.
- **Type**: IBM Plex Sans for UI, IBM Plex Mono for anything machine-ish —
  port names, values, counts, shortcuts, readouts. Bundle the fonts or fall
  back to the system sans/mono; metrics below assume Plex.
- **Dialogs** share one shell: title row (18/600) + close, an optional filter
  row, a card list with hairline dividers, and one primary button per dialog.

---

## 2. Colour tokens

Every colour in the UI is one of these. Define them once, instantiate twice
(dark, light), and have every widget read the token, never a literal.

### 2.1 Surfaces and borders

| Token | Use | Dark | Light |
|---|---|---|---|
| `bg.canvas` | patchbay ground, logo cut-outs | `#121418` | `#e8eaed` |
| `bg.panel` | toolbar, status bar, dialog body | `#1a1d23` | `#f6f7f8` |
| `bg.surface` | node body, card lists, chart card, tooltip base | `#1f232a` | `#ffffff` |
| `bg.menu` | context menus | `#22262e` | `#ffffff` |
| `bg.raised` | ghost buttons, segmented controls, zoom cluster | `#22262e` | `#ffffff` |
| `bg.inset` | search fields, value fields | `#121418` | `#ffffff` |
| `bg.hover` | hovered list row / menu item | `#262b33` | `#eef0f2` |
| `bg.subtle` | small secondary buttons in nodes (UI / Params), count pills, bar tracks | `#2a2f38` | `#eef0f2` |
| `border.strong` | nodes, cards, buttons, inputs, menus | `#2f353f` | `#d4d8de` |
| `border.hairline` | list dividers, node header/footer rules | `#2a2f38` | `#e3e6ea` |
| `border.panel` | toolbar bottom edge, status bar top edge | `#262b33` | `#dde0e5` |
| `grid.dot` | canvas dot grid (1 px dots, 24 px pitch, offset 12,12) | `#262b33` | `#cfd3d9` |

Light-mode surface order is inverted on purpose: the canvas is the darkest
grey and nodes are white, so cards lift off the ground the same way they do
in dark mode.

### 2.2 Text

| Token | Use | Dark | Light |
|---|---|---|---|
| `text.primary` | body, titles, values | `#e8eaee` | `#1a1d23` |
| `text.strong` | wordmark | `#f2f3f5` | `#14161a` |
| `text.secondary` | port names, legend labels, icon buttons | `#aab1bc` | `#4b535e` |
| `text.icon` | toolbar icon glyphs | `#c9cfd8` | `#3a414b` |
| `text.muted` | captions, descriptions, axis labels, counts | `#8b93a0` | `#646c78` |
| `text.placeholder` | input placeholders | `#7b8390` | `#7b8390` |
| `text.disabled` | disabled menu items | `#6f7784` | `#9aa1ad` |
| `text.on-accent` | text/knob on an accent fill | `#141413` | `#ffffff` |
| `text.destructive` | "Remove plugin" | `#e66767` | `#c23434` |

All of these clear 4.5:1 on the surfaces they sit on (muted is 5.1:1 on
`bg.surface` dark, 5.3:1 light).

### 2.3 Accent

| Token | Dark | Light |
|---|---|---|
| `accent` | `#c9e265` | `#5d7a00` |
| `accent.faint` (chip/On backgrounds) | `rgba(201,226,101,0.14)` | `rgba(93,122,0,0.12)` |
| `accent.ring` (selection halo, focused search) | `rgba(201,226,101,0.26)` | `rgba(93,122,0,0.22)` |
| `accent.hover` (links) | `#e3f29a` | `#3f5400` |

Dark: `text.on-accent` is near-black on lime (≈12:1). Light: white on
`#5d7a00` is 4.95:1 and `#5d7a00` as text on white is also 4.95:1 — one light
accent serves both fills and text.

### 2.4 Signal colours (port pins and wires)

| Signal | Dark | Light |
|---|---|---|
| audio | `#63b3ff` | `#2a78d6` |
| MIDI | `#e879b5` | `#d55181` |
| video | `#f0a040` | `#c98500` |

Wires use the signal colour at 0.8 opacity (dark) / 0.85 (light), 1.75 px.
Wires of the selected node: `accent`, 2.25 px, opacity 1.

### 2.5 Node category colours (header glyph chip)

Chip background is the colour at 16 % alpha (dark) / 12 % (light); the glyph
is drawn in the colour itself.

| Category | Glyph | Dark | Light |
|---|---|---|---|
| hardware (capture and playback) | mic / headphones / camera | `#39c6b0` | `#0f8f7c` |
| virtual sink or source | speaker / mic | `#5aa9ff` | `#2a78d6` |
| application stream | waveform | `#f0a040` | `#d95926` |
| plugin | plug | `#a78bfa` | `#6250d6` |
| MIDI device | DIN connector | `#e879b5` | `#c2459a` |

### 2.6 Badges

Rule endpoint types reuse the category colours (background at 14 % / 12 %):
Source → hardware teal, Sink → blue, Plugin → violet, App Out → orange.
Badge text: dark `#5fd6c3` / `#8fc3ff` / `#b9a5fb` / `#f5b35a`;
light `#0b7a69` / `#1f5fae` / `#4f3fb8` / `#b3461a`.

Plugin formats: LV2 → blue, VST3 → orange, CLAP → teal, same text colours.
"no UI" and the LV2 tag inside node headers are neutral: `text.muted` text,
`border.strong` 1 px outline, no fill.

### 2.7 Chart series (CPU Usage)

Fixed order, never re-assigned when a series disappears:
Process CPU, Pianoteq 9, LSP Mixer x4 Stereo, LSP Gate Mono → slots 1–4.

| Slot | Dark | Light |
|---|---|---|
| 1 blue | `#3987e5` | `#2a78d6` |
| 2 orange | `#d95926` | `#eb6834` |
| 3 aqua | `#199e70` | `#1baf7a` |
| 4 yellow | `#c98500` | `#eda100` |

Beyond four plugins, slots 5–8 are magenta `#d55181`/`#e87ba4`, green
`#008300`, violet `#9085e9`/`#6250d6`, red `#e66767`/`#e34948`; beyond
eight, fold into "Other". These were validated for colour-blind separation
on both surfaces. Gridlines `#262b33` / `#e3e6ea`; baseline `#383c45` /
`#c9ced6`; crosshair `text.muted` dashed 2/3.

### 2.8 Shadows

| Use | Dark | Light |
|---|---|---|
| node card | `0 8px 24px rgba(0,0,0,0.35)` | `0 4px 14px rgba(20,24,32,0.10)` |
| menu, tooltip | `0 12px 32px rgba(0,0,0,0.45)` | `0 12px 32px rgba(20,24,32,0.18)` |
| selected node | `0 0 0 3px accent.ring` + node shadow | same |

---

## 3. Typography

| Role | Face | Size / weight | Notes |
|---|---|---|---|
| window / dialog title | Plex Sans | 18 / 600 | letter-spacing −0.01em |
| hero number (CPU) | Plex Sans | 32 / 600 | tabular figures, −0.02em |
| wordmark | Plex Sans | 14 / 600 (toolbar), 22 / 600 (lockup), 24 / 600 (About) | −0.02em at large sizes |
| section label | Plex Sans | 11 / 600 | UPPERCASE, +0.08em, `text.muted` |
| body / row title | Plex Sans | 13 / 400 (500 for row titles) | line-height 1.3 |
| list item primary (plugins) | Plex Sans | 14 / 600 | |
| button | Plex Sans | 12 / 500 (primary 600) | |
| description / caption | Plex Sans | 12 / 400 | `text.muted` |
| node title | Plex Sans | 12 / 600 | ellipsis, never wrap |
| port name | Plex Mono | 11 / 400 | `text.secondary` |
| values, counts, readouts, shortcuts, axis labels | Plex Mono | 11 / 400 | |
| badges / tags | Plex Mono | 10 / 500 | 9 px inside node headers |
| table header | Plex Mono | 10 / 400 | UPPERCASE, +0.04em |

---

## 4. Shape and spacing

- Radii: window 12 · node, card list, chart card, menu 10 · button, input,
  search, tooltip 8 · small button, menu item, value field, badge 6 · glyph
  chip 5 · tag 4 · switch track = height/2.
- Spacing grid: 4 px. Dialog padding 24. Card list inner padding 14–16.
  Toolbar padding 16. Node header padding 10, port rows padded 14.
- Control heights: toolbar controls and dialog buttons 32 · list row
  (rules) 52 · plugin row 64 · table row 44 · table header 30 · menu item 32
  · node header 34 · port row 22 · node footer 34 · node footer buttons 22
  · compact buttons in parameter rows 26 · segmented filter 30 outer / 24 inner.
- Switch: standard 40×22 track, 18 knob, 2 px inset. Compact (in lists)
  32×18, 14 knob. Track on = `accent`, knob on = `text.on-accent`; track off =
  `border.strong` (dark) / `#cfd3d9` (light), knob off = `#9aa1ad` (dark) /
  white (light).
- Minimum hit target for anything clickable: 26 px tall in dense lists, 32 px
  elsewhere.

---

## 5. Components

### 5.1 Toolbar (replaces the menu bar)

52 px, `bg.panel`, 1 px `border.panel` below. Left to right:

1. Mark (22 px) + "ZestBay" wordmark.
2. Search field, 280×32, `bg.inset`, `border.strong`, radius 8, magnifier
   icon, placeholder "Search nodes and ports". Filters the graph live:
   non-matching nodes dim to 35 % opacity; matching ports get a 2 px
   `accent` ring. (New behaviour — optional.)
3. Spacer.
4. **Add plugin** — primary button (accent fill, + icon).
5. **Rules** — ghost button with a 7 px `accent` dot when rules are active and
   a count pill (`bg.subtle`, mono 10). Opens Patchbay Rules.
6. Divider (1×20, `border.strong`).
7. Zoom cluster: one bordered group, 32 tall — [−] [100%] [+] [fit]. The
   percentage cell is 44 wide, mono 11, with hairline dividers either side.
8. Divider.
9. Gear icon button (Preferences) and hamburger icon button (menu holding the
   old File / Help items: Snapshot…, Backups…, CPU usage, About, Quit).

Ghost button: `bg.raised`, 1 px `border.strong`, `text.primary`. Icon-only
buttons are 32×32 with the same style.

### 5.2 Status bar

32 px, `bg.panel`, 1 px `border.panel` above, Plex Mono 11 `text.muted`.
Left: "`N` nodes · `M` links". Then a legend: 8 px dots for Audio / MIDI /
Video and a hollow ring for "Unconnected". Right: a 64×14 CPU sparkline
(hardware teal, 1.5 px) + "CPU 0.0%", then a "Rules active" chip
(`accent.faint` fill, `accent` text, 6 px dot, 20 px tall, radius 6).

### 5.3 Patchbay canvas

- Background `bg.canvas` with `grid.dot` dots, 24 px pitch, offset (12,12),
  drawn in view space (the grid pans and scales with the scene).
- Nodes: see 5.4. Nodes paint above wires; wires paint above the grid.
- Wire: cubic bezier from the output pin centre to the input pin centre;
  control points are offset horizontally by `clamp(0.5·|dx|, 32, 200)`,
  zero vertical offset, so wires leave and enter horizontally. Width 1.75,
  round caps. Hover: opacity 1, width 2.25. Hit-test with an 8 px stroked
  outline of the path, not the 1.75 px visible stroke.
- Dragging a new wire: same curve from the pin to the cursor, dashed 4/4 in
  the signal colour; valid target pins get the `accent.ring` halo.
- Selection: node gets `accent` border + 3 px `accent.ring` halo; its wires
  and connected pins turn `accent`.
- Context menus: section 5.9.

### 5.4 Node card

Widths: 184 for devices/sinks/streams, 208 for plugins (two columns of
ports). Height = 34 (header) + 12 + 22·rows (+ 34 if a plugin footer) + 2.

- Body `bg.surface`, 1 px `border.strong`, radius 10, node shadow.
- Header, 34 px, hairline below: 18×18 category chip (radius 5) with a 12 px
  stroke glyph (1.5 px stroke, round caps), title 12/600 ellipsised,
  and for plugins a right-aligned 9 px mono format tag (LV2 / VST3 / CLAP).
- Port rows, 22 px, 4 px above the first and 8 px below the last. Inputs on
  the left, outputs on the right, both in one row when a plugin has both.
  Labels Plex Mono 11 `text.secondary`.
- Pin: 10 px circle whose centre sits exactly on the node's outer edge
  (6 px outside the padding box). Filled with the signal colour when
  connected; when free, `bg.surface` fill with a 2 px signal-colour ring.
  Hover a pin: scale to 12 px and show the full port name in a tooltip.
- Collapsed ports (optional, plugins with many unconnected ports): after the
  connected ports, one 22 px row with a chevron and "`n` more outputs" in
  mono 10 `text.muted`; clicking expands. The mockup shows this on
  Pianoteq (output_2…15).
- Plugin footer, 34 px, hairline above: three 22 px buttons, radius 6 —
  **On** (toggle; active = `accent.faint` fill, `accent` 11/600 text, 6 px
  dot), **UI**, **Params** (`bg.subtle`, `text.icon`, 11/500).

### 5.5 Dialog shell

`bg.panel`, 1 px `border.strong`, radius 12 (only visible with client-side
decorations; with the WM frame drop the radius and the in-dialog ×).
Header: 20–22 px top padding, title 18/600 left, optional subtitle 12
`text.muted` beneath, controls right (a labelled switch where the dialog has
a master toggle, then the ×, 32×32 ghost icon button).
Content cards: `bg.surface`, `border.strong`, radius 10, rows separated by
`border.hairline`. Footer: 16 px above, 20 px below; secondary actions left,
the single primary action at the far right.

### 5.6 Buttons

| Kind | Fill | Border | Text |
|---|---|---|---|
| primary | `accent` | none | `text.on-accent` 12/600 |
| ghost | `bg.raised` | `border.strong` | `text.primary` 12/500 |
| subtle (inside cards/nodes) | `bg.subtle` | none | `text.icon` 11/500 |
| icon | as ghost, 32×32 or 28×28 | | `text.muted` glyph |
| destructive menu item | — | — | `text.destructive` |

Hover: ghost → `bg.hover`; primary → `accent.hover`; subtle → `border.strong`.
Focus ring: 2 px `accent.ring` outside the control.

### 5.7 Inputs

Search/filter fields: 32 tall (40 in Add Plugin), `bg.inset`, 1 px
`border.strong`, radius 8, 10 px side padding, magnifier 14 px `text.muted`.
Focused: border `accent` + 3 px `accent.ring` (the Add Plugin mock shows the
focused state). Value fields (Parameters): 64×26, mono 12, right-aligned,
same colours, radius 6.

Sliders: groove 4 px, radius 2, `border.strong` unfilled / `accent` filled
portion; handle 14 px circle `accent` with a 2 px `bg.surface` ring. Range
labels beneath in mono 10 `text.muted`.

### 5.8 List rows

- Rules row (52): line 1 = source name 13/500 + type badge + arrow glyph
  (`text.muted`) + target name + badge, ellipsised as one line; line 2 =
  "`n` port mappings" mono 11 `text.muted`. Right: compact switch, then a
  28×28 trash icon button.
- Plugin row (64): name 14/600 + format badge; meta line = "Category ·
  Author" 12 `text.muted`, a 1×10 divider, then "`a` in · `b` out ·
  `c` controls" mono 11. Right: "no UI" tag if applicable, then **Add**
  (ghost 30 px; becomes primary on row hover, and the row takes `bg.hover`).
- Parameter row (see 6.7).
- Table (DSP load): header row 30 px mono 10 uppercase; data rows 44 px;
  numeric columns right-aligned mono 12; an 8 px series dot before each
  plugin name ties the row to the chart.

### 5.9 Context menus

220 wide, `bg.menu`, 1 px `border.strong`, radius 10, 6 px inner padding,
menu shadow. Items 32 tall, radius 6, 10 px side padding, 13/400; hover
`bg.hover`; disabled `text.disabled`; destructive `text.destructive`.
Separators 1 px `border.strong` (dark) / `border.hairline` (light) with 6 px
vertical and 4 px horizontal margin. Shortcut hints right-aligned mono 10
`text.muted` (only "Home" on Fit to view is known).

- Canvas: Delete selected links (disabled when none) · — · Add plugin… · —
  · Unhide all · Fit to view `Home` · Reset zoom · Auto layout.
- Device / sink node: Hide · Unpin · — · Set as default.
- Plugin node: Hide · Unpin · — · Set as default · — · Rename… · Open UI… ·
  — · Remove plugin (destructive).

### 5.10 Tooltip

`bg.hover` (dark) / white (light), 1 px `border.strong`, radius 8, 8×10
padding, menu shadow, 11 px text; rows = 7 px series dot + name + mono value.

---

## 6. Screens

### 6.1 Patchbay window (Main / MainLight)

Toolbar (5.1) · canvas (5.3) · status bar (5.2). The mock lays the graph out
left-to-right by signal flow in six columns, 64 px apart; that is also the
intended result of **Auto layout**: sources → apps/plugins → virtual sinks →
mixers → output sinks → hardware playback, nodes within a column stacked
with 26–28 px gaps and ordered to minimise wire crossings.

### 6.2 Patchbay Rules (760×780)

Header: "Auto-connect rules" + subtitle "Learned automatically when you
connect ports by hand. Disabled rules are kept but not applied." + labelled
master switch "Rules" + ×. Filter row: search "Filter rules" + "`n` rules ·
`m` enabled" mono. Card list of rules (5.8). Footer: Add rule (+ icon),
Backups…, Save backup — spacer — Snapshot connections, **Apply rules now**.

### 6.3 Add Plugin (880×700)

Header "Add plugin" + ×. Search field 40 px tall, 14 px text, focused on
open, placeholder "Search by name, author or category". Filter row:
segmented control All · LV2 · VST3 · CLAP (30 px, `bg.inset` track, active
segment `bg.subtle`), "`908 of 930` plugins" mono, spacer, "Compatible only"
labelled compact switch. Plugin list card (5.8). Esc closes; Enter adds the
highlighted row. No footer.

### 6.4 Preferences (720×760)

Header "Preferences" + ×. Sections with uppercase labels:
- **General** card: three toggle rows (title 13/500, description 12 muted,
  standard switch right): Start minimized to tray · Close to tray ·
  Auto-learn patchbay rules.
- **Timing** card (label + inline hint "Trade responsiveness against
  reliability."): three slider rows — title + description left, a value
  read-out box right (mono 12, unit in muted), the slider underneath, range
  labels below. Rule settle time 0–100 ms · Poll interval 16–500 ms ·
  Parameter save debounce 100–10 000 ms.
- Suggested addition for the theme work: an **Appearance** card with one
  segmented row "Theme: System · Dark · Light" (System follows the desktop's
  colour scheme).
Footer: Reset to defaults (ghost) left; "Changes apply immediately." muted
right. No Close button.

### 6.5 About (420×380)

× top-right only. Centred: mark at 48 px, "ZestBay" 24/600, tagline 13
muted. Card with a two-column key/value grid (96 px label column, hairline
between rows): Version `0.8.7` (mono) · Qt `6.12.0` (mono) · Author lemonxah
· Source link github.com/lemonxah/zestbay (accent link) · License MIT.

### 6.6 CPU Usage (680×680)

Header "CPU usage" + ×. Hero block: "Process CPU" caption, 32 px tabular
number, "last 60 s · log scale" mono caption; right-aligned legend chips
(8 px series dot + name + mono value) wrapping to two lines.
Chart card: 616×220 plot, 44 px left gutter for y labels. **Log y-axis**
with gridlines at 0.01 % · 0.1 % · 1 % · 10 % · 100 % — this is what makes
0.03 % plugin loads and a 95 % spike readable on one axis (the old
5/10/25/50/75 scale could not). x-axis −60 s … now, ticks every 15 s. 2 px
series lines in the slot colours, round joins. Zero values clamp to the
baseline. Hover: dashed crosshair, 4 px markers with a 2 px surface ring, and
a tooltip listing every series at that time, sorted by value.
"Plugin DSP load" section: uppercase label, "Total `0.14%` DSP" right.
Table columns: Plugin (with series dot) · DSP % · Avg · Last · Share of total
(6 px bar in the series colour on a `bg.subtle` track + mono percentage).
Rows sorted by DSP % descending.

### 6.7 Plugin Parameters (600×720)

Header: 28 px plugin category chip, plugin name 18/600, the plugin URI as a
mono 11 muted link beneath; right: "Bypass" labelled standard switch, ×.
Filter row: "Filter parameters" search, "`34` parameters" mono, **Reset all**
ghost. Card list, one row per parameter, 12×14 padding, hairline dividers:
- Row line 1: name 13/500 — value field 64×26 — **MIDI** (mono 10 pill,
  26 tall, = MIDI learn) — ↺ reset icon button 26×26.
- Row line 2: the control — a compact switch + "On/Off" label for boolean
  parameters, or a full-width slider with min/max labels beneath
  (balance shows "L −1.000 … R 1.000").
The list scrolls; the header and filter row stay fixed.

### 6.8 Logo (choose one)

All three are single-colour marks that work from 16 px (tray) to 256 px,
with no clustered holes or segments. Drawn on a 64×64 grid, stroke ends
round. Mark colour = `accent`; on light surfaces use `text.primary` instead
of the dark-mode lime.

- **A · Wedge** — a quarter-disc with one separate rind arc and one
  "pith" line in the surface colour. Most literally "zest"; the risk is that
  at a glance it reads as a pie chart or a play button.
  Geometry: wedge `M17 47 V21 A26 26 0 0 1 43 47 Z` filled and stroked 6
  (round joins) · rind `M17 15 A32 32 0 0 1 49 47` stroke 3 · pith line
  `M22 42 L36.4 27.6` stroke 3.5 in the surface colour.
- **B · Patch** — two 6.5 px pins joined by the canvas's own S-curve,
  which together read as a Z. Most "patchbay", least citrus; the one that
  looks best at 16 px. Geometry: circles at (19,19) and (45,45) r 6.5 ·
  wire `M19 19 C47 19 17 45 45 45` stroke 5.5.
- **C · Peel** — one stroke spiralling outward: a curl of zest that is also
  a coiled cable. Most distinctive at 48 px+, softest at 16 px.
  Geometry: `M34 32 A4 4 0 0 1 34 40 A8 8 0 0 1 34 24 A12 12 0 0 1 34 48
  A16 16 0 0 1 34 16` stroke 5.5.

Recommendation: **B** for the app icon and tray (reads at every size and
says what the app is), with the lime accent carrying the "zest". Export the
chosen mark as SVG plus 16/22/32/48/128/256 PNGs; the tray icon is the mark
alone, the launcher icon is the mark on a `bg.panel` rounded square (radius
22 % of the size).

The marks in the toolbar / About mock-ups are a placeholder circle-and-wedge;
replace with the chosen option.

---

## 7. Qt implementation notes (guidance, not code)

- **Theme engine**: keep the token table in one place and build two
  `QPalette`s from it. Suggested role mapping — Window ← `bg.panel`, Base ←
  `bg.surface`, AlternateBase ← `bg.raised`, Button ← `bg.raised`,
  WindowText/Text/ButtonText ← `text.primary`, PlaceholderText ←
  `text.placeholder`, Highlight ← `accent`, HighlightedText ←
  `text.on-accent`, Mid ← `border.strong`, Light ← `border.hairline`,
  Link ← `accent`, Disabled group text ← `text.disabled`. Apply with
  `QApplication::setPalette` and re-apply on theme change; custom-painted
  widgets (nodes, wires, switches, chart) should read tokens from the same
  object rather than the palette so the extra roles (signal colours,
  category colours, accent.faint) have one home.
- **Following the desktop**: Qt 6.5+ exposes the system colour scheme via
  `QStyleHints::colorScheme()` and emits a change signal; the Preferences
  "Theme" row overrides it. On KDE the WM frame will already follow the
  system scheme, which is the main reason to offer "System".
- **Stylesheet**: radii, paddings, row heights and the segmented control are
  QSS territory; generate the stylesheet from the token table per theme
  rather than hand-maintaining two copies. Avoid QSS on the graph view — it
  is painted directly.
- **Graph view**: `QGraphicsView` with the dot grid drawn in
  `drawBackground` from a cached tile; nodes as items with device-coordinate
  caching; wires as path items with the bezier rule in 5.3 and a stroked
  shape for hit-testing; pins as child items straddling the node boundary.
  Keep wires in a lower z-band than nodes and the drag-wire above both.
- **Fonts**: load Plex Sans/Mono with `QFontDatabase::addApplicationFont`
  (OFL-licensed, bundling is fine) and fall back to the system families;
  set the mono face explicitly on port labels, values, counts and the
  status bar rather than relying on `QFontDatabase::systemFont(FixedFont)`.
- **Icons**: ship the glyphs as stroke SVGs drawn in `currentColor` and tint
  them per theme at load (QIcon from a colourised SVG renderer); 12 px in node
  chips, 14–16 px in toolbars, 1.5 px strokes.
- **Switch**: a checkable `QAbstractButton` subclass painting the pill from
  the token table; animate the knob 120 ms.
- **Chart**: a plain painted widget — log mapping for y, 2 px antialiased
  polylines, hover crosshair from `mouseMoveEvent`, tooltip as a child
  frame; keep 60 s of samples per series.
- **Menus**: `QMenu` with QSS for item height/radius; mark the destructive
  action with a dedicated object name so its colour comes from
  `text.destructive`.
- **Window chrome**: the mock-ups draw an in-dialog × assuming client-side
  decorations. If you keep the WM frame, drop the × and the 12 px window
  radius and keep everything else.

---

## 8. New behaviours proposed by the redesign (opt in)

1. Live search/filter in the toolbar (nodes + ports).
2. Collapsing of unconnected ports on plugins with many ports.
3. Format filter segmented control and "Compatible only" switch in Add Plugin.
4. Filter field and "Reset all" in Plugin Parameters.
5. Chart hover crosshair + tooltip, and the log y-axis, in CPU Usage.
6. "Theme" preference (System / Dark / Light).
7. Hamburger menu absorbing File / Help; menu bar removed.

Everything else is the existing feature set restyled.
