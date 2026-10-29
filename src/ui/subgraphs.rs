//! Subgraph and virtual device state for the UI bridge: the saved
//! definitions, which subgraph the graph view shows, and the view model
//! that turns subgraph edges into one card (main view) or into Input/Output
//! nodes (inside a subgraph).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::pipewire::{GraphState, MediaType, Node, Port, PortDirection, SubgraphRole, SubgraphSide};
use crate::subgraph::{NamedLink, PlanEnd, PlanLink, PlanMedia, SubgraphDef, VirtualDevice};

/// View node ids (subgraph cards, Input/Output nodes) start here: above
/// PipeWire ids and the bridge split ids (1,000,000+).
const VIEW_ID_BASE: u32 = 2_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewNodeKind {
    /// A subgraph on the main graph.
    Card,
    /// Inside a subgraph: where the subgraph's inputs come in.
    Input,
    /// Inside a subgraph: where its outputs leave.
    Output,
}

#[derive(Debug, Clone)]
pub struct ViewNode {
    pub kind: ViewNodeKind,
    pub subgraph: String,
    pub ports: Vec<Port>,
}

/// What the current view shows. Rebuilt with the node list.
#[derive(Debug, Default)]
pub struct ViewModel {
    pub nodes: HashMap<u32, ViewNode>,
    /// Port → the view node that shows it (edge ports only).
    pub port_owner: HashMap<u32, u32>,
    /// Real node → the view node standing for it (edge nodes only).
    pub real_owner: HashMap<u32, u32>,
    /// Real nodes shown as themselves.
    pub visible_real: HashSet<u32>,
}

/// The PipeWire nodes of one subgraph's edges (same node for both sides of
/// a plain pass-through edge).
#[derive(Debug, Default, Clone, Copy)]
pub struct Edges {
    pub in_ext: Option<u32>,
    pub in_int: Option<u32>,
    pub out_ext: Option<u32>,
    pub out_int: Option<u32>,
}

impl Edges {
    pub fn all(&self) -> Vec<u32> {
        let mut v: Vec<u32> = [self.in_ext, self.in_int, self.out_ext, self.out_int]
            .into_iter()
            .flatten()
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Link work that waits for nodes to appear: `connects` (by name) go into
/// the pending links; once they are all up (or after a timeout) the links
/// still joining the `disconnects` port pairs are removed and the
/// `remove_keys` owned nodes too. Port pairs, not link ids: PipeWire reuses
/// a freed link id, so an id could point at one of the new links by then.
#[derive(Debug)]
pub struct Job {
    pub connects: Vec<NamedLink>,
    /// (output port, input port)
    pub disconnects: Vec<(u32, u32)>,
    pub remove_keys: Vec<String>,
    pub started: Instant,
}

pub struct Subgraphs {
    pub defs: Vec<SubgraphDef>,
    pub vdevs: Vec<VirtualDevice>,
    /// The subgraph the graph view is inside (`None` = main graph).
    pub view: Option<String>,
    ids: HashMap<String, u32>,
    next_id: u32,
    pub vm: ViewModel,
    pub jobs: Vec<Job>,
    /// When to create the saved virtual devices: once the graph is known,
    /// so a name already used by another device can be detected.
    pub vdev_start_at: Option<Instant>,
    /// Virtual devices upgraded from an older file this session.
    pub legacy_vdevs: Vec<String>,
}

impl Default for Subgraphs {
    fn default() -> Self {
        Self {
            defs: Vec::new(),
            vdevs: Vec::new(),
            view: None,
            ids: HashMap::new(),
            next_id: VIEW_ID_BASE,
            vm: ViewModel::default(),
            jobs: Vec::new(),
            vdev_start_at: None,
            legacy_vdevs: Vec::new(),
        }
    }
}

impl Subgraphs {
    /// Stable view id for `key` ("card:<id>", "in:<id>", "out:<id>").
    fn view_id(&mut self, key: String) -> u32 {
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.ids.insert(key, id);
        id
    }

    pub fn find(&self, id: &str) -> Option<&SubgraphDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut SubgraphDef> {
        self.defs.iter_mut().find(|d| d.id == id)
    }

    /// The subgraph a plugin (by stable id) belongs to.
    pub fn owner_of(&self, stable_id: &str) -> Option<&SubgraphDef> {
        self.defs.iter().find(|d| d.members.iter().any(|m| m == stable_id))
    }

    pub fn edges(nodes: &[Node], id: &str) -> Edges {
        let mut e = Edges::default();
        for n in nodes {
            let Some(tag) = n.subgraph.as_ref().filter(|t| t.id == id) else {
                continue;
            };
            match (tag.role, tag.side) {
                (SubgraphRole::In, SubgraphSide::Both) => {
                    e.in_ext = Some(n.id);
                    e.in_int = Some(n.id);
                }
                (SubgraphRole::In, SubgraphSide::External) => e.in_ext = Some(n.id),
                (SubgraphRole::In, SubgraphSide::Internal) => e.in_int = Some(n.id),
                (SubgraphRole::Out, SubgraphSide::Both) => {
                    e.out_ext = Some(n.id);
                    e.out_int = Some(n.id);
                }
                (SubgraphRole::Out, SubgraphSide::External) => e.out_ext = Some(n.id),
                (SubgraphRole::Out, SubgraphSide::Internal) => e.out_int = Some(n.id),
            }
        }
        e
    }

    /// Rebuild the view model. `member_node(stable_id)` gives a member
    /// plugin's live node id.
    pub fn build_view(
        &mut self,
        graph: &GraphState,
        nodes: &[Node],
        member_node: &dyn Fn(&str) -> Option<u32>,
    ) {
        if let Some(v) = &self.view
            && self.find(v).is_none()
        {
            self.view = None;
        }
        let mut vm = ViewModel::default();
        let all_members: HashSet<u32> = self
            .defs
            .iter()
            .flat_map(|d| d.members.iter())
            .filter_map(|m| member_node(m))
            .collect();
        let ports_of = |node: Option<u32>, dir: PortDirection| -> Vec<Port> {
            let mut ports: Vec<Port> = node
                .map(|id| graph.get_ports_for_node(id))
                .unwrap_or_default()
                .into_iter()
                .filter(|p| p.direction == dir)
                .collect();
            // Audio channels in order, MIDI last.
            ports.sort_by_key(|p| (p.media_type == Some(MediaType::Midi), p.id));
            ports
        };

        let defs = self.defs.clone();
        match self.view.clone() {
            None => {
                for n in nodes.iter().filter(|n| n.ready) {
                    if n.subgraph.is_none() && !all_members.contains(&n.id) {
                        vm.visible_real.insert(n.id);
                    }
                }
                for def in &defs {
                    let vid = self.view_id(format!("card:{}", def.id));
                    let e = Self::edges(nodes, &def.id);
                    let mut ports = ports_of(e.in_ext, PortDirection::Input);
                    ports.extend(ports_of(e.out_ext, PortDirection::Output));
                    for p in &ports {
                        vm.port_owner.insert(p.id, vid);
                    }
                    for id in [e.in_ext, e.out_ext].into_iter().flatten() {
                        vm.real_owner.insert(id, vid);
                    }
                    vm.nodes.insert(vid, ViewNode { kind: ViewNodeKind::Card, subgraph: def.id.clone(), ports });
                }
            }
            Some(id) => {
                let Some(def) = defs.iter().find(|d| d.id == id).cloned() else {
                    self.vm = vm;
                    return;
                };
                for m in &def.members {
                    if let Some(n) = member_node(m) {
                        vm.visible_real.insert(n);
                    }
                }
                let e = Self::edges(nodes, &def.id);
                if def.has_in() {
                    let vid = self.view_id(format!("in:{}", def.id));
                    let ports = ports_of(e.in_int, PortDirection::Output);
                    for p in &ports {
                        vm.port_owner.insert(p.id, vid);
                    }
                    if let Some(n) = e.in_int {
                        vm.real_owner.insert(n, vid);
                    }
                    vm.nodes.insert(vid, ViewNode { kind: ViewNodeKind::Input, subgraph: def.id.clone(), ports });
                }
                if def.has_out() {
                    let vid = self.view_id(format!("out:{}", def.id));
                    let ports = ports_of(e.out_int, PortDirection::Input);
                    for p in &ports {
                        vm.port_owner.insert(p.id, vid);
                    }
                    if let Some(n) = e.out_int {
                        vm.real_owner.insert(n, vid);
                    }
                    vm.nodes.insert(vid, ViewNode { kind: ViewNodeKind::Output, subgraph: def.id.clone(), ports });
                }
            }
        }
        self.vm = vm;
    }
}

/// Planner input: every live link touching `members`, with names.
pub fn plan_links(graph: &GraphState, members: &HashSet<u32>) -> Vec<PlanLink> {
    let mut out = Vec::new();
    for l in graph.get_all_links() {
        let om = members.contains(&l.output_node_id);
        let im = members.contains(&l.input_node_id);
        if !om && !im {
            continue;
        }
        let (Some(on), Some(inn), Some(op), Some(ip)) = (
            graph.get_node(l.output_node_id),
            graph.get_node(l.input_node_id),
            graph.get_port(l.output_port_id),
            graph.get_port(l.input_port_id),
        ) else {
            continue;
        };
        let midi = op.media_type == Some(MediaType::Midi) || ip.media_type == Some(MediaType::Midi);
        out.push(PlanLink {
            id: l.id,
            out: PlanEnd::new(on.display_name(), op.name.clone()),
            inp: PlanEnd::new(inn.display_name(), ip.name.clone()),
            media: if midi { PlanMedia::Midi } else { PlanMedia::Audio },
            out_member: om,
            in_member: im,
        });
    }
    out
}

/// Every live link touching any of `nodes`, by name.
pub fn named_links_touching(graph: &GraphState, nodes: &[u32]) -> Vec<(u32, NamedLink)> {
    let set: HashSet<u32> = nodes.iter().copied().collect();
    graph
        .get_all_links()
        .into_iter()
        .filter(|l| set.contains(&l.output_node_id) || set.contains(&l.input_node_id))
        .filter_map(|l| {
            let on = graph.get_node(l.output_node_id)?;
            let inn = graph.get_node(l.input_node_id)?;
            let op = graph.get_port(l.output_port_id)?;
            let ip = graph.get_port(l.input_port_id)?;
            Some((
                l.id,
                NamedLink {
                    out: PlanEnd::new(on.display_name(), op.name),
                    inp: PlanEnd::new(inn.display_name(), ip.name),
                },
            ))
        })
        .collect()
}

/// Whether a named link is live in the graph.
pub fn named_link_live(graph: &GraphState, nodes: &[Node], l: &NamedLink) -> bool {
    let find = |end: &PlanEnd, dir: PortDirection| -> Vec<u32> {
        nodes
            .iter()
            .filter(|n| n.display_name() == end.node)
            .flat_map(|n| graph.get_ports_for_node(n.id))
            .filter(|p| p.name == end.port && p.direction == dir)
            .map(|p| p.id)
            .collect()
    };
    let outs = find(&l.out, PortDirection::Output);
    let ins = find(&l.inp, PortDirection::Input);
    outs.iter().any(|o| ins.iter().any(|i| graph.find_link(*o, *i).is_some()))
}
