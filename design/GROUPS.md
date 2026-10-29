# Subgraphs (issue #17) and virtual devices

Wrap a chain of plugins into one block with a chosen number of inputs and
outputs, open it to edit what's inside, reuse it as a template, and offer it
to other apps as an audio device. ZestBay can also create virtual devices
(sink or source loopback pairs) from the UI, like the ones usually written
into PipeWire config.

## Decisions

| Question | Decision |
|---|---|
| Edges | **Real PipeWire nodes** ("Mic Chain In" / "Mic Chain Out") |
| Members | **Plugins only**; devices and apps stay outside and connect to the subgraph's ports |
| Viewing | **Open into its own view** (breadcrumb back); members are hidden on the main graph |
| As a device | **Per edge**: the In edge can be an output device (Audio/Sink apps play into), the Out edge an input device (Audio/Source apps record from) |
| Nesting | Not supported |

## How it works

Each subgraph has an **In** edge (its inputs) and an **Out** edge (its
outputs). An edge has an *external* side (the ports on the subgraph's card)
and an *internal* side (the Input/Output node inside the subgraph):

| Edge | Plain | Shown as a device |
|---|---|---|
| In | one pass-through `pw_filter` "Mic Chain In": inputs `in_<CH>` (external), outputs `out_<CH>` (internal), optional `midi_in`/`midi_out` | in-process `libpipewire-module-loopback`: Audio/Sink "Mic Chain In" (`playback_<CH>`, external) + stream "Mic Chain In (internal)" (`output_<CH>`) |
| Out | pass-through "Mic Chain Out": inputs `in_<CH>` (internal), outputs `out_<CH>` (external) | loopback: stream "Mic Chain Out (internal)" (`input_<CH>`) + Audio/Source "Mic Chain Out" (`capture_<CH>`, external) |

- Channels: 1 = MONO, 2 = FL FR, otherwise AUX0…; ports carry `audio.channel`,
  so heuristic matching, rules and auto-learn treat edges like any device.
- **Why loopbacks for devices:** a `pw_filter` with `media.class=Audio/Sink`
  is listed by WirePlumber but never routed to (tested: `pw-play --target`
  fell back to the default sink). Loopback streams are adapters and route
  normally. MIDI only passes through plain edges.
- Nodes are recognised by `node.name`: `zestbay.subgraph.<id>.in`,
  `.in.int`, `.out`, `.out.int`, and `zestbay.vdev.<id>[.in|.out]`; the media
  class tells a device edge apart. (They also carry `zestbay.subgraph.*` /
  `zestbay.vdev.id` properties, but the registry doesn't pass custom
  properties on, so those are only for other tools.) Devices use
  `priority.session = 100`, so they never become the default on their own.
- **Rules:** links out of an In edge and into an Out edge are internal
  wiring: never learned, never removed by exclusive mode. Internal halves of
  device edges aren't rule ends. Grouping rewrites rules that pointed at
  member ports from outside to the edge ports (and back on ungroup).
- **Links** of plugins, subgraph edges and virtual devices are remembered in
  `links.json` (by display name, so edge names are unique per subgraph).

### Grouping and ungrouping

- **Group:** the dialog's inputs/outputs are inferred from the links crossing
  the selection (one channel per distinct member port wired from/to outside;
  stereo when nothing crosses). New links (outside → In, In → member, member
  → Out, Out → outside) are queued; once they are up (or after 5 s) the old
  crossing links are removed, so audio keeps flowing.
- **Edit** (name, I/O, MIDI, device options): the edges are rebuilt and
  their links and rules carried across by channel.
- **Ungroup:** every outside → In → member and member → Out → outside path
  becomes a direct link again; the edges are removed.
- **Delete:** removes the subgraph and its plugins.

### Saved state

- `subgraphs.json`: id, name, inputs, outputs, midiIn, midiOut, asSink,
  asSource, member plugin stable ids, the subgraph view's pan/zoom.
- `virtual_devices.json`: id, name, kind (`sink` | `source`), channels.
- `subgraph_templates/<name>.json`: settings, plugins (with parameters and
  LV2 state / CLAP-VST3 state), internal links and layout; members are
  `#<index>`, the edges `@in` / `@out`.
- Layout: the card is `Subgraph:<id>`, the inside nodes `SubgraphIn:<id>` /
  `SubgraphOut:<id>`, members keep their own keys (all in `layout.json`).
- Startup: plugins are restored, then edges and virtual devices are created;
  saved links connect once both ends exist.

## UI

- Select plugins → right-click → **Group into subgraph…** (or the canvas
  menu's **Group into subgraph…**, for the selection).
- **Card:** subgraph chip and glyph, stacked edge, member count tag (`2/3`
  when plugins are missing), `DEV` tag when shown as a device; the subgraph's
  external ports; footer **Open subgraph**. Double-click also opens it.
- **Card menu:** Open subgraph · Edit subgraph… · Duplicate · Save as
  template… · — · Hide · Pin · — · Ungroup · Delete subgraph.
- **Inside:** members between **Input** and **Output** nodes; breadcrumb
  **Graph › Mic Chain** (Graph or Escape goes back). A plugin added while
  inside joins the subgraph. Each view keeps its own pan/zoom.
- **Templates:** Add plugin › **Subgraphs** tab lists saved templates
  (Insert, delete); not offered inside a subgraph.
- **Virtual devices:** canvas menu **Add virtual device…** (name, output or
  input device, channels); on a virtual device node: **Edit virtual device…**
  and **Remove virtual device**. They exist while ZestBay runs.

## Code

| Where | What |
|---|---|
| `src/subgraph.rs` | model (`SubgraphDef`, `VirtualDevice`), node specs, planning (`infer_io`, `plan_group`, `plan_ungroup`, `edge_endpoint_map`) + tests |
| `src/pipewire/owned.rs` | pass-through filter and loopback module on the PipeWire thread (`PwCommand::SetOwnedNode`) |
| `src/ui/subgraphs.rs` | view model (cards, Input/Output nodes, link rewriting) |
| `src/ui/subgraphs_bridge.rs` | AppController invokables, templates, reroute jobs |
| `src/patchbay/manager.rs` | `remap_rules`, internal wiring exemptions |
| `qml/GraphView.qml`, `SubgraphDialog.qml`, `VirtualDeviceDialog.qml`, `PluginBrowser.qml` | UI |

## Later

- Nested subgraphs.
- Import/export of template files between machines.
- True subgraph bypass (In straight to Out) instead of bypassing members.
- Virtual devices that outlive ZestBay (written to PipeWire config).
