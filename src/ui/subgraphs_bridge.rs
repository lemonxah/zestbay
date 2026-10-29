//! AppController invokables for subgraphs (issue #17) and virtual devices.
//! A child module of `qobject_bridge`, so it shares the controller's state
//! and private helpers.

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::time::{Duration, Instant};

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use super::*;
use crate::patchbay::EndpointMap;
use crate::pipewire::OwnedNodeSpec;

/// How long a reroute waits for its new links before removing the old ones
/// anyway.
const JOB_TIMEOUT: Duration = Duration::from_secs(5);

const TEMPLATE_DIR: &str = "subgraph_templates";

/// Delay before saved virtual devices are created at startup (the graph
/// has to be known to detect name clashes).
const VDEV_START_DELAY: Duration = Duration::from_secs(2);

/// Keep the file as it was before an upgrade rewrote it (once).
fn keep_pre_upgrade_copy(file: &str) {
    let src = config_path(file);
    let dst = config_path(&format!("{file}.pre-upgrade"));
    if src.exists() && !dst.exists() {
        match std::fs::copy(&src, &dst) {
            Ok(_) => log::info!("Kept the pre-upgrade {} as {}", file, dst.display()),
            Err(e) => log::warn!("Could not keep a copy of {}: {}", file, e),
        }
    }
}

/// A saved subgraph: settings, plugins (with state), internal wiring and
/// layout. Nodes in `links`/`layout` are "#<member index>", "@in", "@out".
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SubgraphTemplate {
    version: u32,
    name: String,
    inputs: u32,
    outputs: u32,
    #[serde(default)]
    midi_in: bool,
    #[serde(default)]
    midi_out: bool,
    #[serde(default)]
    as_sink: bool,
    #[serde(default)]
    as_source: bool,
    plugins: Vec<SavedPlugin>,
    #[serde(default)]
    links: Vec<TemplateLink>,
    #[serde(default)]
    layout: HashMap<String, [f64; 2]>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TemplateLink {
    out_node: String,
    out_port: String,
    in_node: String,
    in_port: String,
}

fn cfg_u32(cfg: &serde_json::Value, key: &str) -> Option<u32> {
    cfg.get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f.max(0.0) as u64)))
        .map(|v| (v as u32).min(sg::MAX_CHANNELS))
}

fn cfg_bool(cfg: &serde_json::Value, key: &str) -> Option<bool> {
    cfg.get(key).and_then(|v| v.as_bool())
}

fn cfg_name(cfg: &serde_json::Value) -> Option<String> {
    cfg.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().replace(['"', '\\', '\n', '\r'], ""))
        .filter(|s| !s.is_empty())
}

/// "Name #3" → "Name" (the suffix `unique_display_name` adds).
fn strip_copy_suffix(name: &str) -> &str {
    match name.rsplit_once(" #") {
        Some((base, n)) if !base.is_empty() && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => base,
        _ => name,
    }
}

/// A file name for a template: letters, digits, space, - and _.
fn template_file_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' })
        .collect();
    let stem = stem.trim().to_string();
    if stem.is_empty() { "subgraph".into() } else { stem }
}

impl qobject::AppController {
    // ------------------------------------------------------------------
    // Internal helpers (used by qobject_bridge too)
    // ------------------------------------------------------------------

    /// Load subgraphs.json and virtual_devices.json, upgrading files from
    /// older versions (missing ids, kinds…; the original is kept as
    /// `<file>.pre-upgrade`), and create the subgraphs' nodes. Virtual
    /// devices are created a moment later, once the graph is known
    /// (`start_virtual_devices`).
    pub(super) fn load_subgraphs(mut self: Pin<&mut Self>) {
        let mut defs: Vec<SubgraphDef> = load_config("subgraphs.json").ok().flatten().unwrap_or_default();
        let mut vdevs: Vec<VirtualDevice> =
            load_config("virtual_devices.json").ok().flatten().unwrap_or_default();
        let defs_upgraded = sg::upgrade_subgraphs(&mut defs);
        let legacy = sg::upgrade_vdevs(&mut vdevs);
        if defs_upgraded {
            keep_pre_upgrade_copy("subgraphs.json");
        }
        if !legacy.is_empty() {
            keep_pre_upgrade_copy("virtual_devices.json");
            log::info!("virtual_devices.json: upgraded {} entries from an older version", legacy.len());
        }
        if !defs.is_empty() || !vdevs.is_empty() {
            log::info!("Restoring {} subgraphs and {} virtual devices", defs.len(), vdevs.len());
        }
        for def in &defs {
            self.as_mut().sync_subgraph_nodes(def);
        }
        {
            let sgs = &mut self.as_mut().rust_mut().subgraphs;
            sgs.defs = defs;
            sgs.vdevs = vdevs;
            sgs.legacy_vdevs = legacy;
            sgs.vdev_start_at = (!sgs.vdevs.is_empty()).then(|| Instant::now() + VDEV_START_DELAY);
        }
        if defs_upgraded || !self.rust().subgraphs.legacy_vdevs.is_empty() {
            self.persist_subgraphs();
        }
    }

    /// Create the saved virtual devices, skipping any whose name another
    /// device already uses (e.g. one from PipeWire config): two devices with
    /// one name would confuse rules and saved links. Entries upgraded from
    /// an older file that clash are dropped (the old file is kept as
    /// .pre-upgrade); current ones stay saved but aren't created.
    pub(super) fn start_virtual_devices(mut self: Pin<&mut Self>) {
        let Some(graph) = self.rust().graph.clone() else { return };
        let nodes = graph.get_all_nodes();
        if nodes.is_empty() {
            // PipeWire isn't up yet: try again shortly.
            self.as_mut().rust_mut().subgraphs.vdev_start_at = Some(Instant::now() + VDEV_START_DELAY);
            return;
        }
        self.as_mut().rust_mut().subgraphs.vdev_start_at = None;
        let taken = |vd: &VirtualDevice| {
            nodes.iter().any(|n| {
                n.ready
                    && n.vdev_id.is_none()
                    && n.subgraph.is_none()
                    && n.display_name() == vd.name
                    && n.node_type
                        == Some(match vd.kind {
                            VirtualDeviceKind::Sink => crate::pipewire::NodeType::Sink,
                            VirtualDeviceKind::Source => crate::pipewire::NodeType::Source,
                        })
            })
        };
        let vdevs = self.rust().subgraphs.vdevs.clone();
        let legacy = std::mem::take(&mut self.as_mut().rust_mut().subgraphs.legacy_vdevs);
        let mut dropped = Vec::new();
        let mut skipped = Vec::new();
        for vd in &vdevs {
            if taken(vd) {
                if legacy.contains(&vd.id) {
                    dropped.push(vd.clone());
                } else {
                    skipped.push(vd.name.clone());
                }
                continue;
            }
            self.send_owned(vd.key(), Some(vd.node_spec()));
        }
        if !dropped.is_empty() {
            self.as_mut()
                .rust_mut()
                .subgraphs
                .vdevs
                .retain(|v| !dropped.iter().any(|d| d.id == v.id));
            self.persist_subgraphs();
            self.as_mut().subgraphs_changed();
        }
        let mut notes = Vec::new();
        if !dropped.is_empty() {
            let names: Vec<&str> = dropped.iter().map(|d| d.name.as_str()).collect();
            notes.push(format!(
                "Virtual devices from an older ZestBay version were not recreated because a device with the same name already exists (e.g. from your PipeWire config): {}. The old list is kept as virtual_devices.json.pre-upgrade.",
                names.join(", ")
            ));
        }
        if !skipped.is_empty() {
            notes.push(format!(
                "Not created because another device already uses the name: {}. Rename the other device or this one to use it.",
                skipped.join(", ")
            ));
        }
        for n in notes {
            log::warn!("{}", n);
            self.as_mut().error_occurred(QString::from(&n));
        }
    }

    pub(super) fn persist_subgraphs(&self) {
        let sgs = &self.rust().subgraphs;
        let defs = serde_json::to_string_pretty(&sgs.defs).unwrap_or_default();
        write_config("subgraphs.json", defs.as_bytes(), sgs.defs.is_empty());
        let vdevs = serde_json::to_string_pretty(&sgs.vdevs).unwrap_or_default();
        write_config("virtual_devices.json", vdevs.as_bytes(), sgs.vdevs.is_empty());
    }

    /// stable id → live node id of every running plugin.
    pub(super) fn member_nodes(&self) -> HashMap<String, u32> {
        self.rust()
            .plugin_manager
            .as_ref()
            .map(|mgr| {
                mgr.active_instances()
                    .values()
                    .filter_map(|i| i.pw_node_id.map(|n| (i.stable_id.clone(), n)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The node a link end is drawn on in the current view, or `None` when
    /// the view doesn't show it.
    pub(super) fn view_node_for(&self, port_id: u32, real_node: u32) -> Option<u32> {
        let vm = &self.rust().subgraphs.vm;
        if let Some(&v) = vm.port_owner.get(&port_id) {
            return Some(v);
        }
        if !vm.visible_real.contains(&real_node) {
            return None;
        }
        Some(
            self.rust()
                .bridge_split
                .resolve_port_virtual_node(port_id)
                .unwrap_or(real_node),
        )
    }

    /// JSON for the subgraph cards / Input and Output nodes of this view.
    pub(super) fn view_nodes_json(&self) -> Vec<serde_json::Value> {
        let sgs = &self.rust().subgraphs;
        let members = self.member_nodes();
        let mut ids: Vec<&u32> = sgs.vm.nodes.keys().collect();
        ids.sort();
        ids.into_iter()
            .filter_map(|vid| {
                let vn = &sgs.vm.nodes[vid];
                let def = sgs.find(&vn.subgraph)?;
                let missing = def.members.iter().filter(|m| !members.contains_key(*m)).count();
                let (name, type_str, key) = match vn.kind {
                    ViewNodeKind::Card => (def.name.clone(), "Subgraph", format!("Subgraph:{}", def.id)),
                    ViewNodeKind::Input => ("Input".to_string(), "SubgraphInput", format!("SubgraphIn:{}", def.id)),
                    ViewNodeKind::Output => ("Output".to_string(), "SubgraphOutput", format!("SubgraphOut:{}", def.id)),
                };
                Some(serde_json::json!({
                    "id": vid,
                    "name": name,
                    "type": type_str,
                    "mediaType": "Audio",
                    "isVirtual": false,
                    "isJack": false,
                    "layoutKey": key,
                    "ready": true,
                    "subgraphId": def.id,
                    "subgraphName": def.name,
                    "memberCount": def.members.len(),
                    "missing": missing,
                    "inputs": def.inputs,
                    "outputs": def.outputs,
                    "asSink": def.as_sink,
                    "asSource": def.as_source,
                }))
            })
            .collect()
    }

    /// Finish reroutes whose new links are up (or timed out): remove the
    /// links they replace.
    pub(super) fn process_subgraph_jobs(mut self: Pin<&mut Self>) {
        let Some(graph) = self.rust().graph.clone() else { return };
        let nodes = graph.get_all_nodes();
        let jobs = std::mem::take(&mut self.as_mut().rust_mut().subgraphs.jobs);
        let mut keep = Vec::new();
        let mut disconnects = Vec::new();
        let mut removes = Vec::new();
        for job in jobs {
            let up = job.connects.iter().all(|l| sgui::named_link_live(&graph, &nodes, l));
            if up || job.started.elapsed() >= JOB_TIMEOUT {
                if !up {
                    log::warn!("Subgraph reroute: not all new links came up; finishing anyway");
                }
                disconnects.extend(job.disconnects);
                removes.extend(job.remove_keys);
            } else {
                keep.push(job);
            }
        }
        self.as_mut().rust_mut().subgraphs.jobs = keep;
        if let Some(ref tx) = self.rust().cmd_tx {
            for (out_port, in_port) in disconnects {
                if let Some(link) = graph.find_link(out_port, in_port) {
                    let _ = tx.send(PwCommand::Disconnect { link_id: link.id });
                }
            }
        }
        for key in removes {
            self.send_owned(key, None);
        }
    }

    fn send_owned(&self, key: String, spec: Option<OwnedNodeSpec>) {
        if let Some(ref tx) = self.rust().cmd_tx {
            let _ = tx.send(PwCommand::SetOwnedNode { key, spec });
        }
    }

    fn sync_subgraph_nodes(mut self: Pin<&mut Self>, def: &SubgraphDef) {
        for name in sg::edge_node_names(def) {
            self.as_mut().rust_mut().forgotten_link_nodes.remove(&name);
        }
        for (key, spec) in def.node_specs() {
            self.send_owned(key, spec);
        }
    }

    /// Remember links (by name) so they are made as soon as both ends exist.
    fn queue_links(mut self: Pin<&mut Self>, links: &[NamedLink]) {
        {
            let mut rust = self.as_mut().rust_mut();
            for l in links {
                let saved = SavedPluginLink {
                    output_node_name: l.out.node.clone(),
                    output_port_name: l.out.port.clone(),
                    input_node_name: l.inp.node.clone(),
                    input_port_name: l.inp.port.clone(),
                };
                if !rust.pending_links.iter().any(|p| p.link == saved) {
                    rust.pending_links.push(PendingLink::new(saved));
                }
            }
            rust.links_retry_due = true;
        }
        self.as_mut().mark_links_dirty();
    }

    /// Rewrite rules for new endpoints; saves and signals when they change.
    fn remap_rules_with(mut self: Pin<&mut Self>, targets: &EndpointMap, sources: &EndpointMap, skip: &HashSet<String>) {
        let changed = match self.as_mut().rust_mut().patchbay.as_mut() {
            Some(pb) => {
                let a = pb.remap_rules(targets, true, skip);
                let b = pb.remap_rules(sources, false, skip);
                a || b
            }
            None => false,
        };
        if changed {
            log::info!("Subgraph: rules updated for the new endpoints");
            save_rules(self.rust().patchbay.as_ref());
            self.as_mut().rules_changed();
        }
    }

    /// Plugins (node id, stable id, display name) among `ids` that aren't in
    /// a subgraph yet.
    fn groupable_plugins(&self, ids: &[u32]) -> Vec<(u32, String, String)> {
        let Some(mgr) = self.rust().plugin_manager.as_ref() else { return Vec::new() };
        let sgs = &self.rust().subgraphs;
        let mut out: Vec<(u32, String, String)> = mgr
            .active_instances()
            .values()
            .filter_map(|i| {
                let node = i.pw_node_id?;
                (ids.contains(&node) && sgs.owner_of(&i.stable_id).is_none())
                    .then(|| (node, i.stable_id.clone(), i.display_name.clone()))
            })
            .collect();
        out.sort_by_key(|(n, ..)| *n);
        out
    }

    /// Names of the sinks and sources in the graph that aren't ZestBay's.
    fn device_names(&self) -> Vec<String> {
        let Some(graph) = self.rust().graph.as_ref() else { return Vec::new() };
        graph
            .get_all_nodes()
            .into_iter()
            .filter(|n| n.vdev_id.is_none() && n.subgraph.is_none())
            .filter(|n| {
                matches!(
                    n.node_type,
                    Some(crate::pipewire::NodeType::Sink) | Some(crate::pipewire::NodeType::Source)
                )
            })
            .map(|n| n.display_name().to_string())
            .collect()
    }

    fn subgraph_names(&self, except: Option<&str>) -> Vec<String> {
        self.rust()
            .subgraphs
            .defs
            .iter()
            .filter(|d| Some(d.id.as_str()) != except)
            .map(|d| d.name.clone())
            .collect()
    }

    fn saved_plugin_for(&self, stable_id: &str) -> Option<SavedPlugin> {
        use base64::Engine;
        let info = self.rust().plugin_manager.as_ref()?.find_by_stable_id(stable_id)?;
        Some(SavedPlugin {
            stable_id: info.stable_id.clone(),
            uri: info.plugin_uri.clone(),
            display_name: info.display_name.clone(),
            bypassed: info.bypassed,
            parameters: info
                .parameters
                .iter()
                .map(|p| SavedPluginParam { port_index: p.port_index, symbol: p.symbol.clone(), value: p.value })
                .collect(),
            format: info.format.as_str().to_string(),
            lv2_state: info.lv2_state.clone(),
            plugin_state: info
                .plugin_state
                .as_ref()
                .map(|blob| base64::engine::general_purpose::STANDARD.encode(blob)),
        })
    }

    /// A template of a live subgraph (call after collecting plugin states).
    fn build_template(&self, def: &SubgraphDef, name: &str) -> Option<SubgraphTemplate> {
        let graph = self.rust().graph.clone()?;
        let plugins: Vec<SavedPlugin> = def.members.iter().filter_map(|m| self.saved_plugin_for(m)).collect();
        let mut alias: HashMap<String, String> = HashMap::new();
        for (i, p) in plugins.iter().enumerate() {
            alias.insert(p.display_name.clone(), format!("#{i}"));
        }
        alias.insert(def.in_int_node(), "@in".into());
        alias.insert(def.out_int_node(), "@out".into());

        let members = self.member_nodes();
        let nodes = graph.get_all_nodes();
        let e = Subgraphs::edges(&nodes, &def.id);
        let mut touch: Vec<u32> = def.members.iter().filter_map(|m| members.get(m).copied()).collect();
        touch.extend([e.in_int, e.out_int].into_iter().flatten());
        let mut links: Vec<TemplateLink> = Vec::new();
        for (_, l) in sgui::named_links_touching(&graph, &touch) {
            if let (Some(o), Some(i)) = (alias.get(&l.out.node), alias.get(&l.inp.node)) {
                links.push(TemplateLink {
                    out_node: o.clone(),
                    out_port: l.out.port,
                    in_node: i.clone(),
                    in_port: l.inp.port,
                });
            }
        }

        let saved: HashMap<String, Vec<f64>> =
            serde_json::from_str(&read_config_json_text("layout.json", "{}")).unwrap_or_default();
        let mut layout = HashMap::new();
        let mut put = |key: String, alias: String| {
            if let Some(p) = saved.get(&key).filter(|p| p.len() >= 2) {
                layout.insert(alias, [p[0], p[1]]);
            }
        };
        for (i, p) in plugins.iter().enumerate() {
            put(format!("{}:{}", p.format, p.display_name), format!("#{i}"));
        }
        put(format!("SubgraphIn:{}", def.id), "@in".into());
        put(format!("SubgraphOut:{}", def.id), "@out".into());

        Some(SubgraphTemplate {
            version: 1,
            name: name.to_string(),
            inputs: def.inputs,
            outputs: def.outputs,
            midi_in: def.midi_in,
            midi_out: def.midi_out,
            as_sink: def.as_sink,
            as_source: def.as_source,
            plugins,
            links,
            layout,
        })
    }

    /// Create a subgraph from a template at (x, y). Returns
    /// `{subgraphId, layout}` (layout key → [x, y] for the new nodes).
    fn insert_template(mut self: Pin<&mut Self>, tpl: SubgraphTemplate, x: f64, y: f64) -> QString {
        let taken = self.subgraph_names(None);
        let mut def = SubgraphDef {
            id: sg::new_id("sg"),
            name: sg::unique_name(&tpl.name, &taken),
            inputs: tpl.inputs.min(sg::MAX_CHANNELS),
            outputs: tpl.outputs.min(sg::MAX_CHANNELS),
            midi_in: tpl.midi_in,
            midi_out: tpl.midi_out,
            as_sink: tpl.as_sink,
            as_source: tpl.as_source,
            members: Vec::new(),
            view: serde_json::Value::Null,
        };

        let mut names: Vec<String> = Vec::new();
        let mut layout = serde_json::Map::new();
        layout.insert(format!("Subgraph:{}", def.id), serde_json::json!([x, y]));
        for (i, mut sp) in tpl.plugins.into_iter().enumerate() {
            sp.stable_id = uuid::Uuid::new_v4().to_string();
            // "Gate #2" copies as "Gate #3", not "Gate #2 #2".
            let base = strip_copy_suffix(&sp.display_name).to_string();
            sp.display_name = self.unique_display_name(&base);
            if let Some(p) = tpl.layout.get(&format!("#{i}")) {
                layout.insert(format!("{}:{}", sp.format, sp.display_name), serde_json::json!(p));
            }
            names.push(sp.display_name.clone());
            let (_, sid) = self.as_mut().spawn_saved_plugin(sp);
            def.members.push(sid);
        }
        for (alias, key) in [("@in", format!("SubgraphIn:{}", def.id)), ("@out", format!("SubgraphOut:{}", def.id))] {
            if let Some(p) = tpl.layout.get(alias) {
                layout.insert(key, serde_json::json!(p));
            }
        }

        let resolve = |n: &str| -> Option<String> {
            match n {
                "@in" => Some(def.in_int_node()),
                "@out" => Some(def.out_int_node()),
                _ => n.strip_prefix('#').and_then(|i| i.parse::<usize>().ok()).and_then(|i| names.get(i).cloned()),
            }
        };
        let links: Vec<NamedLink> = tpl
            .links
            .iter()
            .filter_map(|l| {
                Some(NamedLink {
                    out: PlanEnd::new(resolve(&l.out_node)?, l.out_port.clone()),
                    inp: PlanEnd::new(resolve(&l.in_node)?, l.in_port.clone()),
                })
            })
            .collect();

        persist_active_plugins(self.rust().plugin_manager.as_ref());
        self.as_mut().sync_subgraph_nodes(&def);
        let id = def.id.clone();
        self.as_mut().rust_mut().subgraphs.defs.push(def);
        self.persist_subgraphs();
        self.as_mut().queue_links(&links);
        self.as_mut().subgraphs_changed();
        QString::from(&serde_json::json!({ "subgraphId": id, "layout": layout }).to_string())
    }

    // ------------------------------------------------------------------
    // Invokables
    // ------------------------------------------------------------------

    pub fn get_subgraphs_json(self: Pin<&mut Self>) -> QString {
        let members = self.member_nodes();
        let names: HashMap<String, String> = self
            .rust()
            .plugin_manager
            .as_ref()
            .map(|mgr| {
                mgr.active_instances()
                    .values()
                    .map(|i| (i.stable_id.clone(), i.display_name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let sgs = &self.rust().subgraphs;
        let list: Vec<serde_json::Value> = sgs
            .defs
            .iter()
            .map(|d| {
                serde_json::json!({
                    "id": d.id,
                    "name": d.name,
                    "inputs": d.inputs,
                    "outputs": d.outputs,
                    "midiIn": d.midi_in,
                    "midiOut": d.midi_out,
                    "asSink": d.as_sink,
                    "asSource": d.as_source,
                    "memberCount": d.members.len(),
                    "missing": d.members.iter().filter(|m| !members.contains_key(*m)).count(),
                    "plugins": d.members.iter().filter_map(|m| names.get(m)).collect::<Vec<_>>(),
                    "view": d.view,
                })
            })
            .collect();
        QString::from(&serde_json::json!({
            "current": sgs.view.clone().unwrap_or_default(),
            "subgraphs": list,
        }).to_string())
    }

    pub fn get_subgraph_view(self: Pin<&mut Self>) -> QString {
        QString::from(&self.rust().subgraphs.view.clone().unwrap_or_default())
    }

    pub fn set_subgraph_view(mut self: Pin<&mut Self>, subgraph_id: QString) {
        let id = subgraph_id.to_string();
        let view = (!id.is_empty() && self.rust().subgraphs.find(&id).is_some()).then_some(id);
        if self.rust().subgraphs.view == view {
            return;
        }
        log::info!("Graph view: {}", view.as_deref().unwrap_or("main"));
        self.as_mut().rust_mut().subgraphs.view = view;
        self.as_mut().graph_changed();
    }

    pub fn infer_subgraph_io(self: Pin<&mut Self>, node_ids_json: QString) -> QString {
        let ids: Vec<u32> = serde_json::from_str(&node_ids_json.to_string()).unwrap_or_default();
        let plugins = self.groupable_plugins(&ids);
        let Some(graph) = self.rust().graph.clone() else { return QString::from("{}") };
        let set: HashSet<u32> = plugins.iter().map(|(n, ..)| *n).collect();
        let io = sg::infer_io(&sgui::plan_links(&graph, &set));
        let name = sg::unique_name("Chain", &self.subgraph_names(None));
        QString::from(&serde_json::json!({
            "plugins": plugins.len(),
            "pluginNames": plugins.iter().map(|(.., n)| n).collect::<Vec<_>>(),
            "name": name,
            "inputs": io.inputs,
            "outputs": io.outputs,
            "midiIn": io.midi_in,
            "midiOut": io.midi_out,
        }).to_string())
    }

    pub fn create_subgraph(mut self: Pin<&mut Self>, node_ids_json: QString, config_json: QString) -> QString {
        let ids: Vec<u32> = serde_json::from_str(&node_ids_json.to_string()).unwrap_or_default();
        let cfg: serde_json::Value = serde_json::from_str(&config_json.to_string()).unwrap_or_default();
        let plugins = self.groupable_plugins(&ids);
        let Some(graph) = self.rust().graph.clone() else { return QString::default() };
        if plugins.is_empty() {
            log::warn!("create_subgraph: no plugins to group among {:?}", ids);
            return QString::default();
        }

        let taken = self.subgraph_names(None);
        let def = SubgraphDef {
            id: sg::new_id("sg"),
            name: sg::unique_name(&cfg_name(&cfg).unwrap_or_else(|| "Chain".into()), &taken),
            inputs: cfg_u32(&cfg, "inputs").unwrap_or(2),
            outputs: cfg_u32(&cfg, "outputs").unwrap_or(2),
            midi_in: cfg_bool(&cfg, "midiIn").unwrap_or(false),
            midi_out: cfg_bool(&cfg, "midiOut").unwrap_or(false),
            as_sink: cfg_bool(&cfg, "asSink").unwrap_or(false),
            as_source: cfg_bool(&cfg, "asSource").unwrap_or(false),
            members: plugins.iter().map(|(_, s, _)| s.clone()).collect(),
            view: serde_json::Value::Null,
        };
        log::info!("Creating subgraph '{}' from {} plugins", def.name, plugins.len());

        let set: HashSet<u32> = plugins.iter().map(|(n, ..)| *n).collect();
        let plan = sg::plan_group(&def, &sgui::plan_links(&graph, &set));

        // Rules that pointed at member ports from outside now point at the
        // subgraph's edges (heuristic rules too, even with nothing linked).
        let member_names: HashSet<String> = plugins.iter().map(|(.., n)| n.clone()).collect();
        let mut targets = EndpointMap::default();
        for (from, to) in &plan.port_map_in {
            targets.add((&from.node, &from.port), (&to.node, &to.port));
        }
        let mut sources = EndpointMap::default();
        for (from, to) in &plan.port_map_out {
            sources.add((&from.node, &from.port), (&to.node, &to.port));
        }
        for name in &member_names {
            if def.has_in() {
                targets.nodes.entry(name.clone()).or_insert_with(|| vec![def.in_ext_node()]);
            }
            if def.has_out() {
                sources.nodes.entry(name.clone()).or_insert_with(|| vec![def.out_ext_node()]);
            }
        }
        self.as_mut().remap_rules_with(&targets, &sources, &member_names);

        self.as_mut().sync_subgraph_nodes(&def);
        let id = def.id.clone();
        self.as_mut().rust_mut().subgraphs.defs.push(def);
        self.persist_subgraphs();
        self.as_mut().queue_links(&plan.connects);
        let disconnects: Vec<(u32, u32)> = plan
            .disconnects
            .iter()
            .filter_map(|id| graph.get_link(*id))
            .map(|l| (l.output_port_id, l.input_port_id))
            .collect();
        self.as_mut().rust_mut().subgraphs.jobs.push(sgui::Job {
            connects: plan.connects,
            disconnects,
            remove_keys: Vec::new(),
            started: Instant::now(),
        });
        self.as_mut().subgraphs_changed();
        self.as_mut().graph_changed();
        QString::from(&id)
    }

    pub fn update_subgraph(mut self: Pin<&mut Self>, subgraph_id: QString, config_json: QString) {
        let id = subgraph_id.to_string();
        let cfg: serde_json::Value = serde_json::from_str(&config_json.to_string()).unwrap_or_default();
        let Some(old) = self.rust().subgraphs.find(&id).cloned() else { return };
        let Some(graph) = self.rust().graph.clone() else { return };

        let mut new = old.clone();
        if let Some(name) = cfg_name(&cfg)
            && name != old.name
        {
            new.name = sg::unique_name(&name, &self.subgraph_names(Some(&id)));
        }
        new.inputs = cfg_u32(&cfg, "inputs").unwrap_or(new.inputs);
        new.outputs = cfg_u32(&cfg, "outputs").unwrap_or(new.outputs);
        new.midi_in = cfg_bool(&cfg, "midiIn").unwrap_or(new.midi_in);
        new.midi_out = cfg_bool(&cfg, "midiOut").unwrap_or(new.midi_out);
        new.as_sink = cfg_bool(&cfg, "asSink").unwrap_or(new.as_sink);
        new.as_source = cfg_bool(&cfg, "asSource").unwrap_or(new.as_source);
        if new == old {
            return;
        }
        log::info!("Updating subgraph '{}'", new.name);

        // The edges are rebuilt: carry their links (and rules) across by
        // channel.
        let nodes = graph.get_all_nodes();
        let edges = Subgraphs::edges(&nodes, &id).all();
        let pairs: HashMap<PlanEnd, PlanEnd> = sg::edge_endpoint_map(&old, &new).into_iter().collect();
        let old_names = sg::edge_node_names(&old);
        let map_end = |e: &PlanEnd| -> Option<PlanEnd> {
            if old_names.contains(&e.node) { pairs.get(e).cloned() } else { Some(e.clone()) }
        };
        let carried: Vec<NamedLink> = sgui::named_links_touching(&graph, &edges)
            .into_iter()
            .filter_map(|(_, l)| Some(NamedLink { out: map_end(&l.out)?, inp: map_end(&l.inp)? }))
            .collect();

        let mut rules_map = EndpointMap::default();
        for (a, b) in &pairs {
            rules_map.ports.entry((a.node.clone(), a.port.clone())).or_default().push((b.node.clone(), b.port.clone()));
        }
        rules_map.nodes.insert(old.in_ext_node(), vec![new.in_ext_node()]);
        rules_map.nodes.insert(old.out_ext_node(), vec![new.out_ext_node()]);
        self.as_mut().remap_rules_with(&rules_map, &rules_map, &HashSet::new());

        for name in &old_names {
            self.as_mut().forget_plugin_links(name);
        }
        if let Some(def) = self.as_mut().rust_mut().subgraphs.find_mut(&id) {
            *def = new.clone();
        }
        self.as_mut().sync_subgraph_nodes(&new);
        self.persist_subgraphs();
        self.as_mut().queue_links(&carried);
        self.as_mut().subgraphs_changed();
        self.as_mut().graph_changed();
    }

    pub fn ungroup_subgraph(mut self: Pin<&mut Self>, subgraph_id: QString) {
        let id = subgraph_id.to_string();
        let Some(def) = self.rust().subgraphs.find(&id).cloned() else { return };
        let Some(graph) = self.rust().graph.clone() else { return };
        log::info!("Ungrouping subgraph '{}'", def.name);

        let nodes = graph.get_all_nodes();
        let edges = Subgraphs::edges(&nodes, &id).all();
        let links: Vec<NamedLink> = sgui::named_links_touching(&graph, &edges).into_iter().map(|(_, l)| l).collect();
        let direct = sg::plan_ungroup(&def, &links);

        // Rules on the edges go back to the plugins behind them.
        let mut targets = EndpointMap::default();
        let mut sources = EndpointMap::default();
        for (ext, int, is_in) in sg::edge_port_pairs(&def) {
            if is_in {
                for l in links.iter().filter(|l| l.out == int) {
                    targets.add((&ext.node, &ext.port), (&l.inp.node, &l.inp.port));
                }
            } else {
                for l in links.iter().filter(|l| l.inp == int) {
                    sources.add((&ext.node, &ext.port), (&l.out.node, &l.out.port));
                }
            }
        }
        self.as_mut().remap_rules_with(&targets, &sources, &HashSet::new());

        for (key, _) in def.node_specs() {
            self.send_owned(key, None);
        }
        for name in sg::edge_node_names(&def) {
            self.as_mut().forget_plugin_links(&name);
        }
        {
            let sgs = &mut self.as_mut().rust_mut().subgraphs;
            sgs.defs.retain(|d| d.id != id);
            if sgs.view.as_deref() == Some(id.as_str()) {
                sgs.view = None;
            }
        }
        self.persist_subgraphs();
        self.as_mut().queue_links(&direct);
        self.as_mut().subgraphs_changed();
        self.as_mut().graph_changed();
    }

    pub fn delete_subgraph(mut self: Pin<&mut Self>, subgraph_id: QString) {
        let id = subgraph_id.to_string();
        let Some(def) = self.rust().subgraphs.find(&id).cloned() else { return };
        log::info!("Deleting subgraph '{}' and its {} plugins", def.name, def.members.len());
        let instances: Vec<u64> = def
            .members
            .iter()
            .filter_map(|m| self.rust().plugin_manager.as_ref()?.instance_id_for_stable_id(m))
            .collect();
        if let Some(ref tx) = self.rust().cmd_tx {
            for instance_id in instances {
                let _ = tx.send(PwCommand::RemovePlugin { instance_id });
            }
        }
        for (key, _) in def.node_specs() {
            self.send_owned(key, None);
        }
        for name in sg::edge_node_names(&def) {
            self.as_mut().forget_plugin_links(&name);
        }
        {
            let sgs = &mut self.as_mut().rust_mut().subgraphs;
            sgs.defs.retain(|d| d.id != id);
            if sgs.view.as_deref() == Some(id.as_str()) {
                sgs.view = None;
            }
        }
        self.persist_subgraphs();
        self.as_mut().subgraphs_changed();
        self.as_mut().graph_changed();
    }

    pub fn save_subgraph_view_state(mut self: Pin<&mut Self>, subgraph_id: QString, json: QString) {
        let id = subgraph_id.to_string();
        let value: serde_json::Value = serde_json::from_str(&json.to_string()).unwrap_or_default();
        if let Some(def) = self.as_mut().rust_mut().subgraphs.find_mut(&id) {
            def.view = value;
        }
        self.persist_subgraphs();
    }

    pub fn duplicate_subgraph(mut self: Pin<&mut Self>, subgraph_id: QString, x: f64, y: f64) -> QString {
        let Some(def) = self.rust().subgraphs.find(&subgraph_id.to_string()).cloned() else {
            return QString::default();
        };
        if self.plugins_persistable() {
            self.as_mut().collect_plugin_states_blocking(Duration::from_secs(2));
        }
        match self.build_template(&def, &def.name) {
            Some(tpl) => self.insert_template(tpl, x, y),
            None => QString::default(),
        }
    }

    pub fn save_subgraph_template(mut self: Pin<&mut Self>, subgraph_id: QString, name: QString) -> QString {
        let Some(def) = self.rust().subgraphs.find(&subgraph_id.to_string()).cloned() else {
            return QString::default();
        };
        let name = Some(name.to_string().trim().to_string()).filter(|n| !n.is_empty()).unwrap_or(def.name.clone());
        if self.plugins_persistable() {
            self.as_mut().collect_plugin_states_blocking(Duration::from_secs(2));
        }
        let Some(tpl) = self.build_template(&def, &name) else { return QString::default() };
        let dir = config_path(TEMPLATE_DIR);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!("Cannot create {}: {}", dir.display(), e);
            return QString::default();
        }
        let stem = template_file_stem(&name);
        let file = (1..)
            .map(|i| if i == 1 { format!("{stem}.json") } else { format!("{stem} {i}.json") })
            .find(|f| !dir.join(f).exists())
            .unwrap_or_else(|| format!("{stem}.json"));
        let json = serde_json::to_string_pretty(&tpl).unwrap_or_default();
        match write_atomic(&dir.join(&file), json.as_bytes()) {
            Ok(()) => {
                log::info!("Saved subgraph template {}", file);
                self.as_mut().subgraphs_changed();
                QString::from(&file)
            }
            Err(e) => {
                log::error!("Saving template {}: {}", file, e);
                QString::default()
            }
        }
    }

    pub fn list_subgraph_templates_json(self: Pin<&mut Self>) -> QString {
        let dir = config_path(TEMPLATE_DIR);
        let mut list: Vec<serde_json::Value> = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|entry| {
                        let file = entry.file_name().to_string_lossy().to_string();
                        if !file.ends_with(".json") {
                            return None;
                        }
                        let tpl: SubgraphTemplate =
                            serde_json::from_str(&std::fs::read_to_string(entry.path()).ok()?).ok()?;
                        Some(serde_json::json!({
                            "file": file,
                            "name": tpl.name,
                            "inputs": tpl.inputs,
                            "outputs": tpl.outputs,
                            "asSink": tpl.as_sink,
                            "asSource": tpl.as_source,
                            "plugins": tpl.plugins.iter().map(|p| p.display_name.clone()).collect::<Vec<_>>(),
                        }))
                    })
                    .collect()
            })
            .unwrap_or_default();
        list.sort_by_key(|v| v["name"].as_str().unwrap_or_default().to_lowercase());
        QString::from(&serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()))
    }

    pub fn insert_subgraph_template(self: Pin<&mut Self>, file: QString, x: f64, y: f64) -> QString {
        let file = file.to_string();
        if !is_plain_file_name(&file) {
            return QString::default();
        }
        let path = config_path(TEMPLATE_DIR).join(&file);
        let tpl: Option<SubgraphTemplate> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        match tpl {
            Some(tpl) => self.insert_template(tpl, x, y),
            None => {
                log::error!("Cannot read subgraph template {}", path.display());
                QString::default()
            }
        }
    }

    pub fn delete_subgraph_template(mut self: Pin<&mut Self>, file: QString) {
        let file = file.to_string();
        if !is_plain_file_name(&file) {
            return;
        }
        let path = config_path(TEMPLATE_DIR).join(&file);
        if let Err(e) = std::fs::remove_file(&path) {
            log::error!("Deleting template {}: {}", path.display(), e);
        }
        self.as_mut().subgraphs_changed();
    }

    pub fn get_virtual_devices_json(self: Pin<&mut Self>) -> QString {
        QString::from(&serde_json::to_string(&self.rust().subgraphs.vdevs).unwrap_or_else(|_| "[]".into()))
    }

    pub fn add_virtual_device(mut self: Pin<&mut Self>, name: QString, kind: QString, channels: u32) -> QString {
        let name = name.to_string().trim().replace(['"', '\\', '\n', '\r'], "");
        if name.is_empty() {
            return QString::default();
        }
        let mut taken: Vec<String> = self.rust().subgraphs.vdevs.iter().map(|v| v.name.clone()).collect();
        taken.extend(self.device_names());
        let vd = VirtualDevice {
            id: sg::new_id("vd"),
            name: sg::unique_name(&name, &taken),
            kind: if kind.to_string() == "source" { VirtualDeviceKind::Source } else { VirtualDeviceKind::Sink },
            channels: channels.clamp(1, sg::MAX_CHANNELS),
        };
        log::info!("Adding virtual device '{}' ({:?}, {} ch)", vd.name, vd.kind, vd.channels);
        self.send_owned(vd.key(), Some(vd.node_spec()));
        self.as_mut().rust_mut().forgotten_link_nodes.remove(&vd.name);
        let id = vd.id.clone();
        self.as_mut().rust_mut().subgraphs.vdevs.push(vd);
        self.persist_subgraphs();
        self.as_mut().subgraphs_changed();
        QString::from(&id)
    }

    pub fn update_virtual_device(mut self: Pin<&mut Self>, id: QString, name: QString, channels: u32) {
        let id = id.to_string();
        let name = name.to_string().trim().replace(['"', '\\', '\n', '\r'], "");
        let Some(old) = self.rust().subgraphs.vdevs.iter().find(|v| v.id == id).cloned() else { return };
        let mut vd = old.clone();
        if !name.is_empty() && name != old.name {
            let mut taken: Vec<String> = self
                .rust()
                .subgraphs
                .vdevs
                .iter()
                .filter(|v| v.id != id)
                .map(|v| v.name.clone())
                .collect();
            taken.extend(self.device_names());
            vd.name = sg::unique_name(&name, &taken);
        }
        vd.channels = channels.clamp(1, sg::MAX_CHANNELS);
        if vd == old {
            return;
        }
        if vd.name != old.name {
            self.as_mut().forget_plugin_links(&old.name);
        }
        self.send_owned(vd.key(), Some(vd.node_spec()));
        if let Some(v) = self.as_mut().rust_mut().subgraphs.vdevs.iter_mut().find(|v| v.id == id) {
            *v = vd;
        }
        self.persist_subgraphs();
        self.as_mut().subgraphs_changed();
    }

    pub fn remove_virtual_device(mut self: Pin<&mut Self>, id: QString) {
        let id = id.to_string();
        let Some(vd) = self.rust().subgraphs.vdevs.iter().find(|v| v.id == id).cloned() else { return };
        log::info!("Removing virtual device '{}'", vd.name);
        self.send_owned(vd.key(), None);
        self.as_mut().forget_plugin_links(&vd.name);
        self.as_mut().rust_mut().subgraphs.vdevs.retain(|v| v.id != id);
        self.persist_subgraphs();
        self.as_mut().subgraphs_changed();
    }
}
