# ZestBay Design Bible — "Zest Dark"

How ZestBay looks and how to build UI that keeps looking that way.
This is the working reference for the QML codebase. It is derived from the
redesign spec in `design/zestbay-redesign/ZESTBAY-REDESIGN.md` (the original
handoff, with reference renders in `screens/`), translated into the tokens and
components that exist in `qml/`.

**Order of authority:** `qml/Theme.qml` (values) → this bible (rules and
usage) → the redesign spec (intent, anything not covered here).

---

## 1. Principles

1. **Cool graphite ground, never pure black.** Surfaces step up in lightness
   as they come forward: canvas → panel → surface → menu.
2. **One accent.** Citrus lime in dark, deeper "pressed lime" in light. It is
   used for exactly these, and nothing else:
   - the primary action (one per dialog or toolbar),
   - switches (on state) and slider fill,
   - the selection ring and selected wires,
   - the plugin **On** state,
   - the **Rules active** chip and the dot on the Rules button,
   - the **default** chip on the default target node (same style as Rules
     active: `accentFaint` fill, accent dot and text),
   - focus rings and links,
   - the logo.
   If you want something to stand out and it isn't on this list, use weight,
   size or `textPrimary` — not the accent.
3. **Quiet cards.** Nodes and lists are flat cards with a hairline border. No
   coloured title bars. Category is a small glyph chip; signal type is the pin
   colour; connection state is filled (connected) vs hollow (free) pins.
4. **Sans for people, mono for machines.** IBM Plex Sans for UI text; IBM
   Plex Mono for port names, values, counts, shortcuts, readouts, badges.
5. **One dialog shell.** Title (+ optional subtitle and master switch) →
   optional filter row → card list with hairline dividers → footer with
   secondary actions left and the single primary action far right.
6. **Light is its own palette, not an inversion.** In light mode the canvas
   is the darkest grey and cards are white, so cards still lift off the ground.

---

## 2. Using the theme

- Every colour, size and font comes from the `Theme` singleton. **Never write a
  colour literal, font family or magic size in a screen.** If a value is
  missing, add a token to `Theme.qml` (both modes) and document it here.
- `Theme.mode` is `"system"` (follows the desktop via `Qt.styleHints`; when
  the scheme is Unknown — older Qt, qt6ct on sway-style compositors — it
  judges by the system palette's window colour),
  `"dark"` or `"light"`. It is stored in preferences (`theme`) and set at
  startup. `Theme.dark` is the resolved boolean.
- Every top-level window is a `ZbWindow` (theme colours, palette, default
  font). The Qt Quick Controls style is forced to **Basic** (`src/main.rs`) so
  nothing the desktop style draws leaks through; every control is ours.
- Canvas-painted UI (the patchbay, charts) reads the same tokens. Convert with
  `"" + Theme.token` when assigning to `ctx.fillStyle` / `ctx.strokeStyle`.
- `Theme.tint(colour, alpha)` gives a token at an alpha (badge and chip fills).

### 2.1 Surfaces and borders

| Token | Use | Dark | Light |
|---|---|---|---|
| `bgCanvas` | patchbay ground, logo cut-outs | `#121418` | `#e8eaed` |
| `bgPanel` | toolbar, status bar, dialog body, window | `#1a1d23` | `#f6f7f8` |
| `bgSurface` | node body, card lists, chart card | `#1f232a` | `#ffffff` |
| `bgMenu` | context menus | `#22262e` | `#ffffff` |
| `bgRaised` | ghost buttons, segmented controls, zoom cluster | `#22262e` | `#ffffff` |
| `bgInset` | search fields, value fields, segmented track | `#121418` | `#ffffff` |
| `bgHover` | hovered list row / menu item | `#262b33` | `#eef0f2` |
| `bgSubtle` | subtle buttons (UI / Params), count pills, bar tracks | `#2a2f38` | `#eef0f2` |
| `borderStrong` | nodes, cards, buttons, inputs, menus | `#2f353f` | `#d4d8de` |
| `borderHairline` | list dividers, node header/footer rules | `#2a2f38` | `#e3e6ea` |
| `borderPanel` | toolbar bottom edge, status bar top edge | `#262b33` | `#dde0e5` |
| `gridDot` | canvas dot grid (raised from the spec's `#262b33` / `#cfd3d9` for visibility) | `#3a404b` | `#b9bec7` |
| `menuSeparator` | menu separators (strong in dark, hairline in light) | | |

### 2.2 Text

| Token | Use |
|---|---|
| `textPrimary` | body, titles, values |
| `textStrong` | the wordmark only |
| `textSecondary` | port names, legend labels |
| `textIcon` | toolbar and subtle-button glyphs |
| `textMuted` | captions, descriptions, axis labels, counts |
| `textPlaceholder` | input placeholders |
| `textDisabled` | disabled items |
| `textOnAccent` | text / knob on an accent fill |
| `textDestructive` | destructive actions ("Remove plugin") |

All clear 4.5:1 on the surfaces they're used on. Don't put `textMuted` on
`bgSubtle` for anything people must read.

### 2.3 Accent

| Token | Use |
|---|---|
| `accent` | fills and text in the places listed in §1.2 |
| `accentFaint` | chip / On backgrounds |
| `accentRing` | selection halo, focus ring, focused search |
| `accentHover` | primary button hover, link hover |
| `logoMark` | logo colour (accent in dark, `textPrimary` in light) |

### 2.4 Signals, categories, badges

| Token | Meaning |
|---|---|
| `sigAudio` / `sigMidi` / `sigVideo` | pin and wire colour by signal type |
| `wireOpacity`, `wireWidth`, `wireWidthActive` | wires: 0.8/0.85 opacity, 1.75 px; selected/hover 2.25 px, opacity 1 |
| `catHardware` (mic, headphones, camera) | hardware capture/playback |
| `catVirtual` (speaker, mic) | virtual sinks/sources |
| `catApp` (wave) | application streams |
| `catPlugin` (plug) | plugins |
| `catMidi` (din) | MIDI devices |
| `chipAlpha` / `badgeAlpha` | category chip / badge background alpha |
| `badgeTextTeal/Blue/Violet/Orange` | badge text |

Badge tones (`ZbBadge.tone`): **Source / CLAP → teal · Sink / LV2 → blue ·
Plugin → violet · App Out / VST3 → orange · "no UI" and node-header format
tags → neutral** (outline only).

### 2.5 Charts

`chartSeries[0..7]` is a fixed slot order: slot 0 = process CPU, then plugins
in the order they first appear. **A slot is never reassigned** when a series
goes away. Beyond eight series, fold the rest into "Other" (`chartOther`). `chartGrid` for
gridlines, `chartBaseline` for the zero line, crosshair `textMuted` dashed 2/3.

---

## 3. Typography

Fonts are bundled (`fonts/`, OFL) and loaded by `Theme`: use
`Theme.fontSans` / `Theme.fontMono`, never a family name.

| Role | Face | Size token / weight |
|---|---|---|
| window / dialog title | Sans | `sizeTitle` 18 / DemiBold, letter-spacing −1 % |
| hero number (CPU) | Sans | `sizeHero` 32 / DemiBold, tabular |
| wordmark | Sans | 14 (toolbar), 22 (lockup), 24 (About) / DemiBold |
| section label | Sans | `sizeSection` 11 / DemiBold, UPPERCASE, +8 % — use `ZbSectionLabel` |
| body, row title | Sans | `sizeBody` 13 / Normal (row titles Medium) |
| plugin list item | Sans | `sizeListPrimary` 14 / DemiBold |
| button | Sans | `sizeButton` 12 / Medium (primary DemiBold) |
| caption, description | Sans | `sizeCaption` 12 / Normal, `textMuted` |
| node title | Sans | `sizeNodeTitle` 12 / DemiBold, elide, never wrap |
| port name | Mono | `sizeMono` 11, `textSecondary` |
| values, counts, readouts, shortcuts, axis labels | Mono | `sizeMono` 11 |
| value fields, table numbers | Mono | `sizeValue` 12 |
| badges / tags | Mono | `sizeBadge` 10 / Medium (9 in node headers) |
| table header | Mono | `sizeTableHeader` 10, UPPERCASE, +4 % |

Weights in QML: 400 `Font.Normal`, 500 `Font.Medium`, 600 `Font.DemiBold`.

---

## 4. Shape and spacing

- **Radii:** window 12 · node, card, chart card, menu `radiusCard` 10 ·
  button, input, search, tooltip `radiusControl` 8 · small button, menu item,
  value field, badge `radiusSmall` 6 · glyph chip `radiusChip` 5 · tag
  `radiusTag` 4 · switch = height / 2.
- **Spacing:** 4 px grid. Dialog padding `dialogPadding` 24. Card inner
  padding 14–16. Toolbar padding 16.
- **Heights:** toolbar controls and dialog buttons `controlHeight` 32 · rules
  row 52 · plugin row 64 · table row 44 · table header 30 · menu item 32 ·
  compact buttons in dense rows 26 · segmented 30 outer / 24 inner.
- **Hit targets:** at least 26 px tall in dense lists, 32 px elsewhere.
- **Shadows:** node card and menu shadows are tokens (`shadowNode*`,
  `shadowMenu*`). On the canvas use `ctx.shadowColor/Blur/OffsetY`; in QML
  `ZbMenu` fakes the menu shadow with layered rectangles (no effects module).

---

## 5. Components (`qml/Zb*.qml`)

Use these instead of raw Controls. If a screen needs a variant, extend the
component rather than restyling it locally.

| Component | Use | Key properties |
|---|---|---|
| `ZbWindow` | root of every top-level window | (ApplicationWindow) |
| `ZbButton` | all buttons | `kind`: `"primary"` / `"ghost"` (default) / `"subtle"`; `iconName`; `trailingIconName` (dropdowns); `compact` (26 px); `destructive`; no text + icon = square icon button |
| `ZbSwitch` | all toggles | `compact` (32×18 in lists); `text` → labelled switch, label left |
| `ZbTextField` | plain text input (names, patterns) | sans 13, 32 px; same focus treatment as search |
| `ZbSearchField` | search and filter fields | `large` (40 px, Add Plugin); focus = accent border + ring |
| `ZbValueField` | numeric entry | 64×26 mono, right-aligned |
| `ZbSlider` | sliders | accent fill, 14 px handle; `flush` to align with labels beneath |
| `ZbSegmented` | segmented filters | `model` (labels), `currentIndex`, `activated(index)` |
| `ZbBadge` | type / format badges, tags | `tone` (incl. `"danger"`), `small`; `formatTone("LV2")` helper |
| `ZbCountPill` | counts on buttons | `text` |
| `ZbCard` | card lists, chart card, info grids | surface + strong border, radius 10 |
| `ZbDivider` | rules between rows; toolbar dividers | `vertical`, `strong` |
| `ZbSectionLabel` | UPPERCASE section headings | `text` |
| `ZbDialogHeader` | top of every dialog | `title`, `subtitle`; children go right (e.g. a labelled `ZbSwitch`) |
| `ZbMenu` / `ZbMenuItem` / `ZbMenuSeparator` | every menu | item `destructive`, `shortcutHint` |
| `ZbScrollBar` | every vertical scroll bar in dialogs and lists | beside the content, never over it (§5.3) |
| `ZbWheelScroll` | mouse-wheel scrolling inside every Flickable / ListView | `flickable` (§5.3) |
| `ZbDialog` | in-window confirmations, rename, errors | `title`, `primaryText`, `secondaryText`, `primaryEnabled`; content as children |
| `ZbTooltip` | tooltips | `text` |
| `ZbIcon` | stroke glyphs | `name`, `size`, `color` |
| `ZbLogo` | the logo mark | `size`, `color` (defaults to `logoMark`) |

Layout gotcha: an elided `Text` inside a `RowLayout` needs
`Layout.maximumWidth: Math.ceil(implicitWidth)` (and `Layout.fillWidth` where
it should shrink). Layouts round fractional widths down, which otherwise
elides text that fits.

### 5.1 Button rules

- **One primary per surface.** The toolbar's is **Add plugin**; each dialog
  has at most one, at the far right of its footer.
- Ghost for everything else at top level; subtle inside cards and node footers.
- Hover: ghost → `bgHover`; primary → `accentHover`; subtle → `borderStrong`.
- Destructive actions live in menus (`ZbMenuItem { destructive: true }`) or
  as an icon button with confirmation — never as a red filled button.

### 5.2 Icons

Glyphs live in `qml/Glyphs.qml` as SVG path data, drawn with round caps and
joins at their native stroke width. `ZbIcon` and `ZbLogo` antialias with the
Qt 6.6+ curve renderer (no layer; crisp at any scale factor) and fall back to
a multisampled layer on older Qt — don't put vector shapes in multisampled
layers elsewhere: those vanish on some HiDPI/software renderers. Available: `speaker mic headphones camera
wave plug din pin pinFill search plus minus fit trash reset arrow block settings
menu close check grid chevronDown chevronRight` (`pinFill` is the filled head drawn
under `pin` for the pinned state). Sizes: 12 in node chips, 14–16 in toolbars and
buttons. To add one, take the path from the design source (convert
`<circle>`/`<rect>` to path data, see `Glyphs._circle`), keep 1.5 px strokes,
and add it to the list above. The patchbay canvas draws the same data with
`ctx.path = Glyphs.icons[name].d`.

### 5.3 Scrolling

Scroll bars sit **beside** the content in a reserved column, never on top of
rows, text or controls:

- Use `ZbScrollBar` (10 px wide, `scrollBarWidth`), shown only while the
  content overflows.
- The scrolled view's right edge stops at the bar with a `scrollGap` (4 px)
  gap while the bar is visible, so rows and their right-aligned controls
  (switches, trash buttons, value fields) are never covered.
- Inside a `ZbCard`, the bar lives inside the card's padding on the right,
  aligned to the list's top and bottom — not over the card border.
- Delegates take their width from the view (`ListView.view.width`), so they
  shrink when the bar appears.
- Never attach a bare `ScrollBar.vertical: ScrollBar {}` (the default
  overlays the content). The pattern is in the header of `qml/ZbScrollBar.qml`.
- Every Flickable / ListView gets `ZbWheelScroll { flickable: <id> }` inside
  it. Qt's own wheel scrolling is velocity-based and feels very slow with
  real mice; ZbWheelScroll scrolls a fixed step per notch (desktop "lines per
  wheel step" × `wheelLineHeight`) and uses touchpad pixel deltas as-is.
- The patchbay canvas is the exception: its scroll bars overlay the canvas
  edges, because the canvas has no content box to sit beside.

---

## 6. Patterns

### 6.1 Dialog shell

```
ZbWindow (bgPanel)
└─ ColumnLayout, margins dialogPadding (24), spacing 16
   ├─ ZbDialogHeader { title; subtitle; [labelled master ZbSwitch] }
   ├─ filter row: ZbSearchField + mono count ("n rules · m enabled") + right-aligned options
   ├─ ZbCard (fills) → ListView of rows, ZbDivider between rows, clip: true
   └─ footer: secondary ZbButtons left · spacer · single primary right
```

Dialogs are separate windows with the window manager's frame, so there is **no
in-dialog close button and no window radius** (spec §7 "Window chrome").
Escape closes a dialog. Fixed sizes from the spec: Rules 760×780, Add Plugin
880×700, Preferences 720×760, About 420×380, CPU usage 680×680, Parameters
600×720.

### 6.2 List rows

- **Rule row (52):** line 1 = source 13/Medium + badge + `arrow` glyph
  (`textMuted`) + target + badge, one line, elided; line 2 = "n port mappings"
  mono 11 muted. Right: compact switch, 28×28 trash icon button.
  **Block rules** use the `block` glyph (⊘, `textDestructive`) instead of the
  arrow, and line 2 starts with a `danger` **Blocked** badge ("never
  connected" when it has no port mappings). The Add rule dialog opens with a
  Connect / Block segmented control whose caption explains the choice.
- **Plugin row (64):** name 14/DemiBold + format badge; meta = "Category ·
  Author" 12 muted, a 1×10 divider, "a in · b out · c controls" mono 11. Right:
  "no UI" tag if applicable, then **Add** (ghost; becomes primary on row hover,
  row takes `bgHover`).
- **Parameter row:** line 1 = name 13/Medium · value field · **MIDI** pill
  (mono 10, 26 tall) · reset icon button 26×26; line 2 = the control (compact
  switch + On/Off for booleans, otherwise a full-width slider with min/max
  labels beneath in mono 10 muted).
- **Settings row:** title 13/Medium, description 12 muted, control right.
- **Table:** header 30 px mono 10 uppercase; rows 44 px; numeric columns
  right-aligned mono 12; a series dot before names ties rows to a chart.

### 6.3 Menus

220 wide, items 32 tall, 13/Normal, hover `bgHover`. Separators group items.
Checkable items (`checkable: true`) show a compact switch on the right — never
a dot or tick — so on/off reads the same as everywhere else. Toggling one keeps
the menu open (handle `onToggled`, not `onTriggered`); plain actions close it.
Pick-one lists (filters) use `selected: true` instead: a `check` glyph on the
current choice, and choosing closes the menu.
Order for the context menus:

- Canvas: Delete selected links · Block selected links (destructive) · — ·
  Add plugin… · — · Unhide all · Fit to view `Home` · Reset zoom · Auto layout
- Wire (right-click a wire): Disconnect `Del` · Block this connection
  (destructive). When the wire is part of the selection both act on the whole
  selection ("Disconnect 3 links", "Block 3 connections").
- Device / sink node: Hide · Pin/Unpin · — · Set as default
- Plugin node: Hide · Pin/Unpin · — · Set as default · — · Rename… · Open
  UI… · — · Remove plugin (destructive)

### 6.3.1 Tray menu

Drawn by the desktop (DBusMenu), so colours and fonts follow the system
theme; we control items, labels, theme icons, check marks and grouping only.
Order mirrors the app menu: Show/Hide ZestBay · Plugin UIs ▸ (always
present; "No active plugins" when empty, "(no UI)" entries disabled) · — ·
Add plugin… · Patchbay rules… · ☑ Rules enabled · ☑ Exclusive mode · — ·
Preferences… · Quit. Any state the menu shows must call `TrayState::refresh()`
when it changes, or the host keeps showing its cached menu.

### 6.4 Main window

Toolbar (52) → canvas → status bar (32). There is no menu bar: the hamburger
button holds the old File / Help items.

- **Toolbar:** logo mark (26 tall) + wordmark 14 · search 280×32 · spacer · **Add plugin**
  (primary) · **Rules** (ghost, accent dot when active + count pill) · divider
  · zoom cluster [− | 100% | + | fit] (one bordered group, % cell 44 wide mono
  11) · grid cluster [grid toggle | 24px ▾] (toggle on = `accentFaint` +
  accent, like a plugin's On; the size opens a pick-one menu of 8–96 px) ·
  divider · settings · menu. The search field shrinks to 160 on narrow
  windows.
- **Status bar:** mono 11 muted. "N nodes · M links", legend (8 px dots Audio /
  MIDI / Video, hollow ring Unconnected), then right: 64×14 CPU sparkline
  (`catHardware`, 1.5 px) + "CPU x%", then the Rules-active chip
  (`accentFaint`, `accent` text, 6 px dot, 20 tall, radius 6).

### 6.5 Patchbay canvas

- Ground `bgCanvas`; 1.5 px `gridDot` dots on the grid-size pitch (24 by
  default) at multiples of the grid size, in
  scene space (pans and zooms with the graph). Order: grid → wires → nodes →
  drag wire.
- **Node card:** width 184 (208 for plugins), radius 10, `bgSurface`, strong
  border, node shadow. Header 34 with hairline below: 18×18 category chip
  (radius 5, colour at `chipAlpha`) with a 12 px glyph, title 12/DemiBold
  elided, plugins add a 9 px neutral format tag, the default target node
  shows the accent **default** chip, and every node ends the header with the
  **pin toggle** —
  `pin` glyph, solid (`textIcon`) when pinned, faint `textDisabled` outline
  when not, tooltip on hover. Pinned state must always be visible. Port rows 22
  (4 above the first, 8 below the last), inputs left, outputs right, label mono
  11 `textSecondary`. Plugin footer 34 with hairline above: **On** / **UI** /
  **Params** 22-px subtle buttons sharing the card width (10 px margins, 6 px
  gaps, proportional widths, labels centred) (On active = `accentFaint` + accent text + 6 px
  dot).
- **Height** = 34 + 12 + 22 × rows (+ 34 plugin footer) + 2.
- **Pin:** 10 px circle centred on the card's outer edge. Connected = filled
  signal colour; free = `bgSurface` fill with a 2 px signal ring. Hover → 12 px.
- **Collapsed ports:** a plugin's unconnected ports beyond the connected ones
  fold into one row "⌄ n more outputs" (mono 10 muted); click expands.
- **Wire:** cubic bezier from output pin to input pin, control points offset
  horizontally by `clamp(0.5·|dx|, 32, 200)`, round caps. Hit-test with an
  8 px band, not the visible stroke.
- **Dragging a wire:** same curve to the cursor, dashed 4/4 in the signal
  colour; valid target pins get an `accentRing` halo. Left-drag connects.
- **Block wire (right-drag from a pin):** the same dashed curve in
  `textDestructive` with a small ⊘ at the cursor; dropping it on a pin adds a
  block rule for the node pair and removes its links. A right-click on a pin
  without dragging opens the node menu; Escape cancels either drag.
- **Blocking (behaviour):** a block rule means the source is never connected
  to the target. It is enforced whenever rules are on, in exclusive and
  non-exclusive mode, and wins over connect rules and the default node, so a
  blocked app with nothing else routing it stays unconnected. Blocking a pair
  replaces connect rules for exactly that pair; connecting the pair by hand
  lifts its block (wildcard blocks stay and are edited in the rules dialog).
  The rule-enforcement loop guard applies, so ZestBay stops removing a link the
  session manager keeps re-creating after 3 tries in 30 s.
- **Selection:** accent border + 3 px `accentRing` halo; the node's wires and
  pins turn accent.
- **Search:** non-matching nodes at 35 % opacity; matching ports get a 2 px
  accent ring.
- **Snapping (Shift-drag):** aligning edges snaps flush; stacking above/below
  or placing beside another node keeps the **snap gap** (preference
  `snap_gap_px`, default 10 graph units = 10 px at 100 % zoom). Only nodes at
  least partly on screen count, so a zoomed-in drag never snaps to something
  out of view. Guides are dashed `textMuted` lines on the edge being snapped to.
- **Snap to grid** (preference `snap_to_grid`, **on by default**; toolbar
  and menu toggles; grid size
  `grid_size_px`, default 24): node top-left corners land on multiples of the
  grid size when dragging (single and group), placing, dropping and on Auto
  layout (Rust `layout::snap_to_grid` rounds and pushes colliding nodes down by
  whole grid steps). Switching it on aligns every node once. It replaces
  Shift-snap while on. The dot grid always uses the grid size as its pitch.

---

## 7. Logo

The logo is a **lime zest arc** and an **orange cable** that loops into a
zest slice and ends in an audio jack — citrus and patching in one line. The
master artwork is `images/logo/zestbay-master.png`; everything else is
generated from it.

- **Colours:** brand orange `#f48d11` and lime `#96dc2e` (`Theme.logoOrange`
  / `Theme.logoLime`; light mode uses deeper `#e07c00` / `#6aaa12` so the lime
  still reads on white). These are logo colours only — UI accents still use
  `accent`.
- **Mark vs. full logo:** inside the app the mark (arc + cable) is used
  everywhere: toolbar (`ZbLogo`, 26 px tall), tray, app icon. The **full logo
  with the "ZestBay" wordmark** ("Zest" lime, "Bay" orange) appears in the app
  **only on the About screen** (`ZbLogo { withName: true }`); outside the app
  it heads the README.
- **Solid fills only:** no outlines, glows or background. The background of
  the master is removed when tracing.
- **App icon** = the mark centred on a `bgPanel` (dark) rounded square,
  radius 22 % of the size. **Tray** = the mark alone, centred in a square,
  always sent as embedded pixmaps (22/32/64), never as a theme name, so an
  older installed icon can't win.
- **Pipeline:** `images/trace-logo.py` traces the master (per colour, from a
  soft coverage map) into `images/logo/zestbay-mark.svg`,
  `images/logo/zestbay-logo.svg` and the LOGO block in `qml/Glyphs.qml`.
  `images/make-icons.py` then renders `images/logo/zestbay-launcher.svg`,
  `images/zesticon.png`, `images/zesttray.png`,
  `images/zesttray-{22,32,64}.argb` and the Flatpak icons. Change the master
  and rerun both; never edit the generated files by hand.
- The earlier "Z" logo is kept in `images/logo/alt/` for reference.

## 8. Checklist for any new or changed UI

- [ ] Root is `ZbWindow`; content uses `Zb*` components.
- [ ] No colour literals, font families or ad-hoc sizes; all from `Theme`.
- [ ] Accent appears only in the places listed in §1.2; at most one primary
      button.
- [ ] Mono for machine text (ports, values, counts, shortcuts); sans for the rest.
- [ ] Looks right in **both** dark and light (set Theme → Light in Preferences).
- [ ] Hit targets ≥ 26 px in dense lists, ≥ 32 px elsewhere; Escape closes dialogs.
- [ ] Scroll bars are `ZbScrollBar`s beside the content, never overlaying it;
      every scroll area has a `ZbWheelScroll`.
- [ ] New QML files are listed in `build.rs`; new singletons also in `qml/qmldir`.
- [ ] Works on Qt 6.4 (no `MultiEffect`, no Qt 6.5+ only properties without a
      fallback).
