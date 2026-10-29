# ZestBay redesign — handoff bundle

Give this folder to Claude Code (or drop it into the repo under `design/`).

Read in this order:

1. `ZESTBAY-REDESIGN.md` — the spec. Every colour token for dark and light,
   type sizes, radii, control heights, each component, each screen, the logo
   geometry, and Qt implementation notes. This is the source of truth.
2. `screens/` — reference renders of every board at 2× (so a 1600×960 window
   is a 3200×1920 PNG). Use them to check the result, not to measure from;
   the numbers are in the spec.
   - `Main.png` / `MainLight.png` — patchbay window, dark / light
   - `Rules.png` / `RulesLight.png` — Patchbay Rules dialog
   - `AddPlugin.png` — Add Plugin dialog
   - `Preferences.png` — Preferences dialog
   - `About.png` — About dialog
   - `CpuUsage.png` / `CpuUsageLight.png` — CPU Usage window
   - `Params.png` — plugin Parameters window
   - `Menus.png` — the three context menus, dark and light
   - `Logo.png` — the three logo candidates at several sizes
3. `logo/` — the three candidate marks as 64×64 SVG. Pick one; export the
   tray/launcher sizes from it as the spec describes.
4. `design-source/` — the board sources from the design canvas (HTML
   mockups). They are design files, not application code: useful when an
   exact value is needed that the spec does not spell out (every inline
   style there is the measured value), and for nothing else.

Notes

- Renders used fallback system fonts; the intended faces are IBM Plex Sans
  and IBM Plex Mono (OFL). Metrics in the spec assume Plex.
- The light theme is a second token set, not an inverted palette. Dialogs
  that are only shown in dark (Add Plugin, Preferences, About, Parameters)
  follow the token table mechanically.
- Items listed under "New behaviours proposed" in the spec are optional;
  everything else is the existing feature set restyled.
