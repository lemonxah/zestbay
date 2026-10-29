//! Subgraphs (issue #17) and virtual devices: the saved model, the PipeWire
//! nodes each one needs, and the pure planning of how links are rerouted
//! when plugins are grouped or ungrouped. See design/GROUPS.md.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::pipewire::{LoopbackSpec, OwnedNodeSpec, PassSpec};

/// Most channels a subgraph edge or virtual device may have.
pub const MAX_CHANNELS: u32 = 16;

fn two() -> u32 {
    2
}

fn default_subgraph_name() -> String {
    "Subgraph".into()
}

/// A saved subgraph: plugins grouped behind an input and an output edge.
/// Every field has a default, so files from older versions load; missing
/// ids are filled in by `upgrade_subgraphs`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubgraphDef {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_subgraph_name")]
    pub name: String,
    #[serde(default = "two")]
    pub inputs: u32,
    #[serde(default = "two")]
    pub outputs: u32,
    #[serde(default)]
    pub midi_in: bool,
    #[serde(default)]
    pub midi_out: bool,
    /// Offer the input edge to apps as an output device (Audio/Sink).
    #[serde(default)]
    pub as_sink: bool,
    /// Offer the output edge to apps as an input device (Audio/Source).
    #[serde(default)]
    pub as_source: bool,
    /// Stable ids of the member plugins.
    #[serde(default)]
    pub members: Vec<String>,
    /// Pan/zoom of the subgraph's own view (owned by the UI).
    #[serde(default)]
    pub view: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VirtualDeviceKind {
    /// Apps play into it; its stream feeds onward (like "Comms Sink").
    #[default]
    Sink,
    /// Apps record from it; its stream is fed from the graph.
    Source,
}

/// A virtual device ZestBay creates (a loopback pair). Older versions saved
/// only `name` and `channels`; the rest defaults (a sink) and
/// `upgrade_vdevs` fills in the id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDevice {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: VirtualDeviceKind,
    #[serde(default = "two")]
    pub channels: u32,
}

/// Bring loaded virtual devices up to date: give entries from older files
/// an id, keep channels in range and names unique. Returns the ids of the
/// entries that came from the old format (no id).
pub fn upgrade_vdevs(list: &mut Vec<VirtualDevice>) -> Vec<String> {
    let mut legacy = Vec::new();
    let mut names: Vec<String> = Vec::new();
    list.retain(|v| !v.name.trim().is_empty());
    for (i, v) in list.iter_mut().enumerate() {
        if v.id.is_empty() {
            v.id = format!("{}-{i}", new_id("vd"));
            legacy.push(v.id.clone());
        }
        v.channels = v.channels.clamp(1, MAX_CHANNELS);
        v.name = unique_name(v.name.trim(), &names);
        names.push(v.name.clone());
    }
    legacy
}

/// Same for subgraphs: ids for entries without one, unique names, I/O in
/// range. Returns whether anything changed.
pub fn upgrade_subgraphs(list: &mut [SubgraphDef]) -> bool {
    let mut changed = false;
    let mut names: Vec<String> = Vec::new();
    for (i, d) in list.iter_mut().enumerate() {
        if d.id.is_empty() {
            d.id = format!("{}-{i}", new_id("sg"));
            changed = true;
        }
        let (inputs, outputs) = (d.inputs.min(MAX_CHANNELS), d.outputs.min(MAX_CHANNELS));
        changed |= inputs != d.inputs || outputs != d.outputs;
        d.inputs = inputs;
        d.outputs = outputs;
        let name = unique_name(d.name.trim(), &names);
        changed |= name != d.name;
        d.name = name;
        names.push(d.name.clone());
    }
    changed
}

/// Channel positions for `n` channels: MONO, FL FR, otherwise AUX0…
pub fn channel_names(n: u32) -> Vec<String> {
    match n.min(MAX_CHANNELS) {
        0 => Vec::new(),
        1 => vec!["MONO".into()],
        2 => vec!["FL".into(), "FR".into()],
        n => (0..n).map(|i| format!("AUX{i}")).collect(),
    }
}

/// A short unique id ("sg-…", "vd-…").
pub fn new_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{:x}", nanos)
}

/// Device priority for nodes ZestBay exposes: below hardware, so a new
/// device never becomes the default on its own.
const DEVICE_PRIORITY: &str = "100";

impl SubgraphDef {
    pub fn has_in(&self) -> bool {
        self.inputs > 0 || (self.midi_in && !self.as_sink)
    }
    pub fn has_out(&self) -> bool {
        self.outputs > 0 || (self.midi_out && !self.as_source)
    }
    /// MIDI only passes through plain (non-device) edges.
    pub fn midi_in_active(&self) -> bool {
        self.midi_in && !self.as_sink
    }
    pub fn midi_out_active(&self) -> bool {
        self.midi_out && !self.as_source
    }

    // Node names (display names, which links and rules are keyed by).
    pub fn in_ext_node(&self) -> String {
        format!("{} In", self.name)
    }
    pub fn in_int_node(&self) -> String {
        if self.as_sink { format!("{} In (internal)", self.name) } else { self.in_ext_node() }
    }
    pub fn out_ext_node(&self) -> String {
        format!("{} Out", self.name)
    }
    pub fn out_int_node(&self) -> String {
        if self.as_source { format!("{} Out (internal)", self.name) } else { self.out_ext_node() }
    }

    // Port names per channel.
    pub fn in_ext_port(&self, ch: &str) -> String {
        if self.as_sink { format!("playback_{ch}") } else { format!("in_{ch}") }
    }
    pub fn in_int_port(&self, ch: &str) -> String {
        if self.as_sink { format!("output_{ch}") } else { format!("out_{ch}") }
    }
    pub fn out_int_port(&self, ch: &str) -> String {
        if self.as_source { format!("input_{ch}") } else { format!("in_{ch}") }
    }
    pub fn out_ext_port(&self, ch: &str) -> String {
        if self.as_source { format!("capture_{ch}") } else { format!("out_{ch}") }
    }

    pub fn in_key(&self) -> String {
        format!("sg:{}:in", self.id)
    }
    pub fn out_key(&self) -> String {
        format!("sg:{}:out", self.id)
    }

    fn tags(&self, role: &str, side: &str) -> Vec<(String, String)> {
        vec![
            ("zestbay.subgraph.id".into(), self.id.clone()),
            ("zestbay.subgraph.role".into(), role.into()),
            ("zestbay.subgraph.side".into(), side.into()),
        ]
    }

    /// The owned nodes this subgraph needs, by key (`None` = not needed).
    pub fn node_specs(&self) -> Vec<(String, Option<OwnedNodeSpec>)> {
        let in_spec = if !self.has_in() {
            None
        } else if self.as_sink {
            let channels = channel_names(self.inputs.max(1));
            let mut capture = self.tags("in", "ext");
            capture.extend([
                ("media.class".into(), "Audio/Sink".into()),
                ("node.name".into(), format!("zestbay.subgraph.{}.in", self.id)),
                ("node.description".into(), self.in_ext_node()),
                ("priority.session".into(), DEVICE_PRIORITY.into()),
                ("priority.driver".into(), DEVICE_PRIORITY.into()),
            ]);
            let mut playback = self.tags("in", "int");
            playback.extend([
                ("node.name".into(), format!("zestbay.subgraph.{}.in.int", self.id)),
                ("node.description".into(), self.in_int_node()),
                ("node.autoconnect".into(), "false".into()),
                ("node.dont-reconnect".into(), "true".into()),
            ]);
            Some(OwnedNodeSpec::Loopback(LoopbackSpec {
                description: self.in_ext_node(),
                channels,
                capture_props: capture,
                playback_props: playback,
            }))
        } else {
            Some(OwnedNodeSpec::Pass(PassSpec {
                node_name: format!("zestbay.subgraph.{}.in", self.id),
                description: self.in_ext_node(),
                channels: channel_names(self.inputs),
                midi: self.midi_in_active(),
                props: self.tags("in", "both"),
            }))
        };

        let out_spec = if !self.has_out() {
            None
        } else if self.as_source {
            let channels = channel_names(self.outputs.max(1));
            let mut capture = self.tags("out", "int");
            capture.extend([
                ("node.name".into(), format!("zestbay.subgraph.{}.out.int", self.id)),
                ("node.description".into(), self.out_int_node()),
                ("node.autoconnect".into(), "false".into()),
                ("node.dont-reconnect".into(), "true".into()),
            ]);
            let mut playback = self.tags("out", "ext");
            playback.extend([
                ("media.class".into(), "Audio/Source".into()),
                ("node.name".into(), format!("zestbay.subgraph.{}.out", self.id)),
                ("node.description".into(), self.out_ext_node()),
                ("priority.session".into(), DEVICE_PRIORITY.into()),
                ("priority.driver".into(), DEVICE_PRIORITY.into()),
            ]);
            Some(OwnedNodeSpec::Loopback(LoopbackSpec {
                description: self.out_ext_node(),
                channels,
                capture_props: capture,
                playback_props: playback,
            }))
        } else {
            Some(OwnedNodeSpec::Pass(PassSpec {
                node_name: format!("zestbay.subgraph.{}.out", self.id),
                description: self.out_ext_node(),
                channels: channel_names(self.outputs),
                midi: self.midi_out_active(),
                props: self.tags("out", "both"),
            }))
        };

        vec![(self.in_key(), in_spec), (self.out_key(), out_spec)]
    }
}

impl VirtualDevice {
    pub fn key(&self) -> String {
        format!("vd:{}", self.id)
    }

    pub fn node_spec(&self) -> OwnedNodeSpec {
        let tag = || ("zestbay.vdev.id".to_string(), self.id.clone());
        let device = |class: &str, suffix: &str| -> Vec<(String, String)> {
            vec![
                tag(),
                ("media.class".into(), class.into()),
                ("node.name".into(), format!("zestbay.vdev.{}{}", self.id, suffix)),
                ("node.description".into(), self.name.clone()),
                ("priority.session".into(), DEVICE_PRIORITY.into()),
                ("priority.driver".into(), DEVICE_PRIORITY.into()),
            ]
        };
        // The stream half is routed by ZestBay (rules, the default node or
        // by hand), not by the session manager.
        let stream = |suffix: &str| -> Vec<(String, String)> {
            vec![
                tag(),
                ("node.name".into(), format!("zestbay.vdev.{}{}", self.id, suffix)),
                ("node.description".into(), self.name.clone()),
                ("node.autoconnect".into(), "false".into()),
            ]
        };
        let (capture_props, playback_props) = match self.kind {
            VirtualDeviceKind::Sink => (device("Audio/Sink", ""), stream(".out")),
            VirtualDeviceKind::Source => (stream(".in"), device("Audio/Source", "")),
        };
        OwnedNodeSpec::Loopback(LoopbackSpec {
            description: self.name.clone(),
            channels: channel_names(self.channels.clamp(1, MAX_CHANNELS)),
            capture_props,
            playback_props,
        })
    }
}

// ---------------------------------------------------------------------------
// Rerouting plans
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMedia {
    Audio,
    Midi,
}

/// One end of a link, by display name (as links and rules are keyed).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanEnd {
    pub node: String,
    pub port: String,
}

impl PlanEnd {
    pub fn new(node: impl Into<String>, port: impl Into<String>) -> Self {
        Self { node: node.into(), port: port.into() }
    }
}

/// A live link as the planner sees it.
#[derive(Debug, Clone)]
pub struct PlanLink {
    pub id: u32,
    pub out: PlanEnd,
    pub inp: PlanEnd,
    pub media: PlanMedia,
    /// Whether each end belongs to the selection being grouped.
    pub out_member: bool,
    pub in_member: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedLink {
    pub out: PlanEnd,
    pub inp: PlanEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InferredIo {
    pub inputs: u32,
    pub outputs: u32,
    pub midi_in: bool,
    pub midi_out: bool,
}

/// Inputs/outputs for a new subgraph from the links crossing the selection:
/// one channel per distinct member port wired from (to) outside. Stereo
/// when nothing crosses.
pub fn infer_io(links: &[PlanLink]) -> InferredIo {
    let (ins, outs) = boundary_ports(links);
    let count = |m: &BTreeMap<PlanEnd, usize>| (m.len() as u32).clamp(1, MAX_CHANNELS);
    let midi_in = links.iter().any(|l| l.media == PlanMedia::Midi && l.in_member && !l.out_member);
    let midi_out = links.iter().any(|l| l.media == PlanMedia::Midi && l.out_member && !l.in_member);
    let any_in = links.iter().any(|l| l.in_member && !l.out_member);
    let any_out = links.iter().any(|l| l.out_member && !l.in_member);
    InferredIo {
        inputs: if ins.is_empty() && (midi_in || any_in) { 0 } else if ins.is_empty() { 2 } else { count(&ins) },
        outputs: if outs.is_empty() && (midi_out || any_out) { 0 } else if outs.is_empty() { 2 } else { count(&outs) },
        midi_in,
        midi_out,
    }
}

/// Distinct member audio ports wired from outside (inputs) and to outside
/// (outputs), each numbered in a stable order (node, then port name).
fn boundary_ports(links: &[PlanLink]) -> (BTreeMap<PlanEnd, usize>, BTreeMap<PlanEnd, usize>) {
    let mut ins: BTreeMap<PlanEnd, usize> = BTreeMap::new();
    let mut outs: BTreeMap<PlanEnd, usize> = BTreeMap::new();
    for l in links.iter().filter(|l| l.media == PlanMedia::Audio) {
        if l.in_member && !l.out_member {
            ins.insert(l.inp.clone(), 0);
        }
        if l.out_member && !l.in_member {
            outs.insert(l.out.clone(), 0);
        }
    }
    for (i, v) in ins.values_mut().enumerate() {
        *v = i;
    }
    for (i, v) in outs.values_mut().enumerate() {
        *v = i;
    }
    (ins, outs)
}

/// What grouping does to the links: crossing links are replaced by links
/// through the subgraph's edges. `port_map_in` / `port_map_out` map a member
/// port to the edge port that now stands for it (for rewriting rules).
#[derive(Debug, Clone, Default)]
pub struct GroupPlan {
    pub connects: Vec<NamedLink>,
    pub disconnects: Vec<u32>,
    /// Member input (node, port) → (In ext node, In ext port).
    pub port_map_in: HashMap<PlanEnd, PlanEnd>,
    /// Member output (node, port) → (Out ext node, Out ext port).
    pub port_map_out: HashMap<PlanEnd, PlanEnd>,
}

pub fn plan_group(def: &SubgraphDef, links: &[PlanLink]) -> GroupPlan {
    let (ins, outs) = boundary_ports(links);
    let in_ch = channel_names(def.inputs);
    let out_ch = channel_names(def.outputs);
    let mut plan = GroupPlan::default();
    let push = |plan: &mut GroupPlan, l: NamedLink| {
        if !plan.connects.contains(&l) {
            plan.connects.push(l);
        }
    };

    for l in links {
        let into = l.in_member && !l.out_member;
        let from = l.out_member && !l.in_member;
        if !into && !from {
            continue;
        }
        match (l.media, into) {
            (PlanMedia::Audio, true) => {
                if in_ch.is_empty() {
                    continue;
                }
                let ch = &in_ch[ins[&l.inp] % in_ch.len()];
                let ext = PlanEnd::new(def.in_ext_node(), def.in_ext_port(ch));
                let int = PlanEnd::new(def.in_int_node(), def.in_int_port(ch));
                push(&mut plan, NamedLink { out: l.out.clone(), inp: ext.clone() });
                push(&mut plan, NamedLink { out: int, inp: l.inp.clone() });
                plan.port_map_in.insert(l.inp.clone(), ext);
            }
            (PlanMedia::Audio, false) => {
                if out_ch.is_empty() {
                    continue;
                }
                let ch = &out_ch[outs[&l.out] % out_ch.len()];
                let int = PlanEnd::new(def.out_int_node(), def.out_int_port(ch));
                let ext = PlanEnd::new(def.out_ext_node(), def.out_ext_port(ch));
                push(&mut plan, NamedLink { out: l.out.clone(), inp: int });
                push(&mut plan, NamedLink { out: ext.clone(), inp: l.inp.clone() });
                plan.port_map_out.insert(l.out.clone(), ext);
            }
            (PlanMedia::Midi, true) => {
                if !def.midi_in_active() {
                    continue;
                }
                let ext = PlanEnd::new(def.in_ext_node(), "midi_in");
                push(&mut plan, NamedLink { out: l.out.clone(), inp: ext.clone() });
                push(&mut plan, NamedLink { out: PlanEnd::new(def.in_int_node(), "midi_out"), inp: l.inp.clone() });
                plan.port_map_in.insert(l.inp.clone(), ext);
            }
            (PlanMedia::Midi, false) => {
                if !def.midi_out_active() {
                    continue;
                }
                let ext = PlanEnd::new(def.out_ext_node(), "midi_out");
                push(&mut plan, NamedLink { out: l.out.clone(), inp: PlanEnd::new(def.out_int_node(), "midi_in") });
                push(&mut plan, NamedLink { out: ext.clone(), inp: l.inp.clone() });
                plan.port_map_out.insert(l.out.clone(), ext);
            }
        }
        plan.disconnects.push(l.id);
    }
    plan
}

/// What ungrouping does: every path outside → In → member (and member →
/// Out → outside) becomes a direct link again. `links` are all live links
/// touching the subgraph's edge nodes; the edges are removed afterwards.
pub fn plan_ungroup(def: &SubgraphDef, links: &[NamedLink]) -> Vec<NamedLink> {
    let mut direct = Vec::new();
    for (ext, int, is_in) in edge_port_pairs(def) {
        // In: outside → ext input, int output → member.
        // Out: member → int input, ext output → outside.
        let (srcs, dsts): (Vec<&NamedLink>, Vec<&NamedLink>) = if is_in {
            (
                links.iter().filter(|l| l.inp == ext).collect(),
                links.iter().filter(|l| l.out == int).collect(),
            )
        } else {
            (
                links.iter().filter(|l| l.inp == int).collect(),
                links.iter().filter(|l| l.out == ext).collect(),
            )
        };
        for s in &srcs {
            for d in &dsts {
                let l = NamedLink { out: s.out.clone(), inp: d.inp.clone() };
                if !direct.contains(&l) {
                    direct.push(l);
                }
            }
        }
    }
    direct
}

/// The (external, internal, is the In edge) port pairs of a subgraph's
/// edges: one per channel, plus MIDI.
pub fn edge_port_pairs(def: &SubgraphDef) -> Vec<(PlanEnd, PlanEnd, bool)> {
    let mut pairs = Vec::new();
    if def.has_in() {
        for ch in channel_names(def.inputs) {
            pairs.push((
                PlanEnd::new(def.in_ext_node(), def.in_ext_port(&ch)),
                PlanEnd::new(def.in_int_node(), def.in_int_port(&ch)),
                true,
            ));
        }
        if def.midi_in_active() {
            pairs.push((PlanEnd::new(def.in_ext_node(), "midi_in"), PlanEnd::new(def.in_int_node(), "midi_out"), true));
        }
    }
    if def.has_out() {
        for ch in channel_names(def.outputs) {
            pairs.push((
                PlanEnd::new(def.out_ext_node(), def.out_ext_port(&ch)),
                PlanEnd::new(def.out_int_node(), def.out_int_port(&ch)),
                false,
            ));
        }
        if def.midi_out_active() {
            pairs.push((PlanEnd::new(def.out_ext_node(), "midi_out"), PlanEnd::new(def.out_int_node(), "midi_in"), false));
        }
    }
    pairs
}

/// Every edge endpoint of `old` → the endpoint standing for it in `new`
/// (same edge, side and channel index; MIDI to MIDI). Endpoints with no
/// counterpart (fewer channels, MIDI or the edge switched off) are absent.
pub fn edge_endpoint_map(old: &SubgraphDef, new: &SubgraphDef) -> Vec<(PlanEnd, PlanEnd)> {
    let mut out = Vec::new();
    let pair = |out: &mut Vec<(PlanEnd, PlanEnd)>, a: (String, String), b: (String, String)| {
        let (a, b) = (PlanEnd::new(a.0, a.1), PlanEnd::new(b.0, b.1));
        if a != b {
            out.push((a, b));
        }
    };
    if old.has_in() && new.has_in() {
        let (oc, nc) = (channel_names(old.inputs), channel_names(new.inputs));
        for (o, n) in oc.iter().zip(&nc) {
            pair(&mut out, (old.in_ext_node(), old.in_ext_port(o)), (new.in_ext_node(), new.in_ext_port(n)));
            pair(&mut out, (old.in_int_node(), old.in_int_port(o)), (new.in_int_node(), new.in_int_port(n)));
        }
        if old.midi_in_active() && new.midi_in_active() {
            pair(&mut out, (old.in_ext_node(), "midi_in".into()), (new.in_ext_node(), "midi_in".into()));
            pair(&mut out, (old.in_int_node(), "midi_out".into()), (new.in_int_node(), "midi_out".into()));
        }
    }
    if old.has_out() && new.has_out() {
        let (oc, nc) = (channel_names(old.outputs), channel_names(new.outputs));
        for (o, n) in oc.iter().zip(&nc) {
            pair(&mut out, (old.out_int_node(), old.out_int_port(o)), (new.out_int_node(), new.out_int_port(n)));
            pair(&mut out, (old.out_ext_node(), old.out_ext_port(o)), (new.out_ext_node(), new.out_ext_port(n)));
        }
        if old.midi_out_active() && new.midi_out_active() {
            pair(&mut out, (old.out_int_node(), "midi_in".into()), (new.out_int_node(), "midi_in".into()));
            pair(&mut out, (old.out_ext_node(), "midi_out".into()), (new.out_ext_node(), "midi_out".into()));
        }
    }
    out
}

/// Names of all edge nodes of a subgraph (display names).
pub fn edge_node_names(def: &SubgraphDef) -> Vec<String> {
    let mut v = vec![def.in_ext_node(), def.in_int_node(), def.out_ext_node(), def.out_int_node()];
    v.dedup();
    v
}

/// A name not in `taken`: `base`, then "base 2", "base 3"…
pub fn unique_name(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.iter().any(|t| t == n))
        .unwrap_or_else(|| base.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(inputs: u32, outputs: u32) -> SubgraphDef {
        SubgraphDef {
            id: "sg-1".into(),
            name: "Mic Chain".into(),
            inputs,
            outputs,
            midi_in: false,
            midi_out: false,
            as_sink: false,
            as_source: false,
            members: vec![],
            view: serde_json::Value::Null,
        }
    }

    fn link(id: u32, on: &str, op: &str, inn: &str, ip: &str, om: bool, im: bool) -> PlanLink {
        PlanLink {
            id,
            out: PlanEnd::new(on, op),
            inp: PlanEnd::new(inn, ip),
            media: if op.starts_with("midi") { PlanMedia::Midi } else { PlanMedia::Audio },
            out_member: om,
            in_member: im,
        }
    }

    /// Mic → Gate → Comp → Headset, Gate and Comp selected.
    fn chain() -> Vec<PlanLink> {
        vec![
            link(1, "Mic", "capture_MONO", "Gate", "input_0", false, true),
            link(2, "Gate", "output_0", "Comp", "input_0", true, true),
            link(3, "Comp", "output_0", "Headset", "playback_FL", true, false),
            link(4, "Comp", "output_0", "Headset", "playback_FR", true, false),
        ]
    }

    #[test]
    fn channel_names_by_count() {
        assert_eq!(channel_names(1), vec!["MONO"]);
        assert_eq!(channel_names(2), vec!["FL", "FR"]);
        assert_eq!(channel_names(3), vec!["AUX0", "AUX1", "AUX2"]);
        assert_eq!(channel_names(99).len(), MAX_CHANNELS as usize);
    }

    #[test]
    fn infer_counts_distinct_member_ports() {
        let io = infer_io(&chain());
        assert_eq!(io, InferredIo { inputs: 1, outputs: 1, midi_in: false, midi_out: false });
        // Nothing crossing: stereo default.
        let io = infer_io(&[]);
        assert_eq!((io.inputs, io.outputs), (2, 2));
        // MIDI only in: no audio inputs.
        let io = infer_io(&[link(1, "Piano", "midi_out", "Synth", "midi_in", false, true)]);
        assert_eq!((io.inputs, io.midi_in), (0, true));
    }

    #[test]
    fn group_reroutes_crossing_links_only() {
        let d = def(1, 1);
        let plan = plan_group(&d, &chain());
        assert_eq!(plan.disconnects, vec![1, 3, 4]);
        let c = |on: &str, op: &str, inn: &str, ip: &str| NamedLink { out: PlanEnd::new(on, op), inp: PlanEnd::new(inn, ip) };
        assert!(plan.connects.contains(&c("Mic", "capture_MONO", "Mic Chain In", "in_MONO")));
        assert!(plan.connects.contains(&c("Mic Chain In", "out_MONO", "Gate", "input_0")));
        assert!(plan.connects.contains(&c("Comp", "output_0", "Mic Chain Out", "in_MONO")));
        assert!(plan.connects.contains(&c("Mic Chain Out", "out_MONO", "Headset", "playback_FL")));
        assert!(plan.connects.contains(&c("Mic Chain Out", "out_MONO", "Headset", "playback_FR")));
        // Comp → Out appears once even though two links left Comp's port.
        assert_eq!(plan.connects.iter().filter(|l| l.inp.node == "Mic Chain Out").count(), 1);
        assert_eq!(plan.port_map_in[&PlanEnd::new("Gate", "input_0")], PlanEnd::new("Mic Chain In", "in_MONO"));
    }

    #[test]
    fn device_edges_use_loopback_port_names() {
        let mut d = def(2, 2);
        d.as_sink = true;
        d.as_source = true;
        let links = vec![
            link(1, "App", "output_FL", "EQ", "in_l", false, true),
            link(2, "EQ", "out_l", "Speakers", "playback_FL", true, false),
        ];
        let plan = plan_group(&d, &links);
        let nodes: Vec<(&str, &str)> = plan.connects.iter().flat_map(|l| [(l.out.node.as_str(), l.out.port.as_str()), (l.inp.node.as_str(), l.inp.port.as_str())]).collect();
        assert!(nodes.contains(&("Mic Chain In", "playback_FL")));
        assert!(nodes.contains(&("Mic Chain In (internal)", "output_FL")));
        assert!(nodes.contains(&("Mic Chain Out (internal)", "input_FL")));
        assert!(nodes.contains(&("Mic Chain Out", "capture_FL")));
    }

    #[test]
    fn ungroup_restores_direct_links() {
        let d = def(1, 1);
        let plan = plan_group(&d, &chain());
        let direct = plan_ungroup(&d, &plan.connects);
        let c = |on: &str, op: &str, inn: &str, ip: &str| NamedLink { out: PlanEnd::new(on, op), inp: PlanEnd::new(inn, ip) };
        assert_eq!(direct.len(), 3);
        assert!(direct.contains(&c("Mic", "capture_MONO", "Gate", "input_0")));
        assert!(direct.contains(&c("Comp", "output_0", "Headset", "playback_FL")));
        assert!(direct.contains(&c("Comp", "output_0", "Headset", "playback_FR")));
    }

    #[test]
    fn midi_passes_through_plain_edges() {
        let mut d = def(0, 2);
        d.midi_in = true;
        let links = vec![link(1, "Keys", "midi_out", "Piano", "midi_in", false, true)];
        let plan = plan_group(&d, &links);
        assert!(plan.connects.contains(&NamedLink { out: PlanEnd::new("Keys", "midi_out"), inp: PlanEnd::new("Mic Chain In", "midi_in") }));
        assert!(plan.connects.contains(&NamedLink { out: PlanEnd::new("Mic Chain In", "midi_out"), inp: PlanEnd::new("Piano", "midi_in") }));
        // A device edge drops MIDI.
        d.as_sink = true;
        assert!(!d.midi_in_active());
    }

    #[test]
    fn specs_follow_device_options() {
        let mut d = def(2, 2);
        let specs = d.node_specs();
        assert!(matches!(specs[0].1, Some(OwnedNodeSpec::Pass(_))));
        d.as_source = true;
        let specs = d.node_specs();
        let Some(OwnedNodeSpec::Loopback(lb)) = &specs[1].1 else { panic!("loopback expected") };
        assert!(lb.playback_props.contains(&("media.class".into(), "Audio/Source".into())));
        assert!(lb.capture_props.contains(&("node.autoconnect".into(), "false".into())));
        // No inputs and no MIDI: no In edge.
        let d = def(0, 2);
        assert!(d.node_specs()[0].1.is_none());
    }

    #[test]
    fn virtual_sink_spec() {
        let v = VirtualDevice { id: "vd-1".into(), name: "Comms Sink".into(), kind: VirtualDeviceKind::Sink, channels: 2 };
        let OwnedNodeSpec::Loopback(lb) = v.node_spec() else { panic!() };
        assert!(lb.capture_props.contains(&("media.class".into(), "Audio/Sink".into())));
        assert!(lb.capture_props.contains(&("node.description".into(), "Comms Sink".into())));
        assert!(lb.playback_props.contains(&("node.autoconnect".into(), "false".into())));
        assert_eq!(lb.channels, vec!["FL", "FR"]);
    }

    #[test]
    fn edge_map_follows_rename_and_device_switch() {
        let old = def(2, 2);
        let mut new = old.clone();
        new.name = "Vocal Chain".into();
        new.as_source = true;
        let map = edge_endpoint_map(&old, &new);
        let has = |a: (&str, &str), b: (&str, &str)| map.contains(&(PlanEnd::new(a.0, a.1), PlanEnd::new(b.0, b.1)));
        assert!(has(("Mic Chain In", "in_FL"), ("Vocal Chain In", "in_FL")));
        assert!(has(("Mic Chain Out", "in_FR"), ("Vocal Chain Out (internal)", "input_FR")));
        assert!(has(("Mic Chain Out", "out_FR"), ("Vocal Chain Out", "capture_FR")));
        // Fewer channels: the dropped channel has no counterpart.
        let mut mono = old.clone();
        mono.inputs = 1;
        let map = edge_endpoint_map(&old, &mono);
        assert!(map.iter().any(|(a, b)| a.port == "in_FL" && b.port == "in_MONO"));
        assert!(!map.iter().any(|(a, _)| a.port == "in_FR"));
    }

    #[test]
    fn unique_names() {
        let taken = vec!["Mic Chain".to_string(), "Mic Chain 2".to_string()];
        assert_eq!(unique_name("Mic Chain", &taken), "Mic Chain 3");
        assert_eq!(unique_name("Other", &taken), "Other");
    }

    #[test]
    fn old_virtual_devices_file_upgrades() {
        // As written by older ZestBay versions: no id, no kind.
        let json = r#"[{"name":"Headphones","channels":2},{"name":"Media","channels":2},{"name":"Media"},{"name":"  "}]"#;
        let mut list: Vec<VirtualDevice> = serde_json::from_str(json).unwrap();
        let legacy = upgrade_vdevs(&mut list);
        assert_eq!(list.len(), 3);
        assert_eq!(legacy.len(), 3);
        assert!(list.iter().all(|v| !v.id.is_empty() && v.kind == VirtualDeviceKind::Sink && v.channels == 2));
        assert_eq!(list[2].name, "Media 2");
        let ids: std::collections::HashSet<_> = list.iter().map(|v| v.id.clone()).collect();
        assert_eq!(ids.len(), 3);
        // Already current: nothing to upgrade.
        assert!(upgrade_vdevs(&mut list).is_empty());
    }

    #[test]
    fn old_subgraphs_file_upgrades() {
        let mut list: Vec<SubgraphDef> = serde_json::from_str(r#"[{"name":"A","members":["x"]},{"name":"A","inputs":99}]"#).unwrap();
        assert!(upgrade_subgraphs(&mut list));
        assert!(!list[0].id.is_empty() && list[0].inputs == 2);
        assert_eq!(list[1].name, "A 2");
        assert_eq!(list[1].inputs, MAX_CHANNELS);
        assert!(!upgrade_subgraphs(&mut list));
    }

    #[test]
    fn def_json_round_trip_and_defaults() {
        let json = r#"{"id":"sg-1","name":"X","inputs":2,"outputs":2}"#;
        let d: SubgraphDef = serde_json::from_str(json).unwrap();
        assert!(!d.as_sink && d.members.is_empty());
        let back: SubgraphDef = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }
}
