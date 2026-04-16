use parking_lot::RwLock;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::types::*;

pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.as_bytes().iter().peekable();
    let mut bi = b.as_bytes().iter().peekable();

    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&&ac), Some(&&bc)) => {
                let a_digit = ac.is_ascii_digit();
                let b_digit = bc.is_ascii_digit();

                if a_digit && b_digit {
                    let mut an: u64 = 0;
                    while let Some(&&c) = ai.peek() {
                        if c.is_ascii_digit() {
                            an = an * 10 + (c - b'0') as u64;
                            ai.next();
                        } else {
                            break;
                        }
                    }
                    let mut bn: u64 = 0;
                    while let Some(&&c) = bi.peek() {
                        if c.is_ascii_digit() {
                            bn = bn * 10 + (c - b'0') as u64;
                            bi.next();
                        } else {
                            break;
                        }
                    }
                    match an.cmp(&bn) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                } else {
                    match ac.cmp(&bc) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        ord => return ord,
                    }
                }
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct GraphState {
    nodes: RwLock<HashMap<ObjectId, Node>>,
    ports: RwLock<HashMap<ObjectId, Port>>,
    links: RwLock<HashMap<ObjectId, Link>>,
    change_counter: RwLock<u64>,
}

impl GraphState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn mark_changed(&self) {
        let mut counter = self.change_counter.write();
        *counter = counter.wrapping_add(1);
    }

    pub fn change_counter(&self) -> u64 {
        *self.change_counter.read()
    }

    pub fn insert_node(&self, node: Node) {
        let media_type = node.media_type;
        let node_id = node.id;
        self.nodes.write().insert(node_id, node);

        if let Some(mt) = media_type {
            let mut ports = self.ports.write();
            for port in ports.values_mut() {
                if port.node_id == node_id && port.media_type.is_none() {
                    port.media_type = Some(mt);
                }
            }
        }

        self.mark_changed();
    }

    pub fn remove_node(&self, id: ObjectId) -> Option<Node> {
        let node = self.nodes.write().remove(&id);
        if node.is_some() {
            self.mark_changed();
        }
        node
    }

    pub fn get_node(&self, id: ObjectId) -> Option<Node> {
        self.nodes.read().get(&id).cloned()
    }

    pub fn get_all_nodes(&self) -> Vec<Node> {
        self.nodes.read().values().cloned().collect()
    }

    pub fn set_node_type(&self, id: ObjectId, node_type: NodeType) {
        if let Some(node) = self.nodes.write().get_mut(&id)
            && node.node_type != Some(node_type)
        {
            node.node_type = Some(node_type);
            self.mark_changed();
        }
    }

    pub fn set_node_description(&self, id: ObjectId, description: &str) {
        if let Some(node) = self.nodes.write().get_mut(&id)
            && node.description != description
        {
            node.description = description.to_string();
            self.mark_changed();
        }
    }

    pub fn insert_port(&self, port: Port) {
        self.ports.write().insert(port.id, port);
        self.mark_changed();
    }

    pub fn remove_port(&self, id: ObjectId) -> Option<Port> {
        let port = self.ports.write().remove(&id);
        if port.is_some() {
            self.mark_changed();
        }
        port
    }

    /// Remove all links that reference the given port and return their IDs.
    /// Call this after `remove_port` so that stale links don't linger in the
    /// graph when PipeWire removes ports before their parent node.
    pub fn cleanup_port(&self, port_id: ObjectId) -> Vec<ObjectId> {
        let mut links = self.links.write();
        let mut removed = Vec::new();
        links.retain(|&id, l| {
            if l.output_port_id == port_id || l.input_port_id == port_id {
                removed.push(id);
                false
            } else {
                true
            }
        });
        if !removed.is_empty() {
            drop(links);
            self.mark_changed();
        }
        removed
    }

    pub fn get_port(&self, id: ObjectId) -> Option<Port> {
        self.ports.read().get(&id).cloned()
    }

    pub fn get_ports_for_node(&self, node_id: ObjectId) -> Vec<Port> {
        let mut ports: Vec<Port> = self
            .ports
            .read()
            .values()
            .filter(|p| p.node_id == node_id)
            .cloned()
            .collect();
        ports.sort_by(|a, b| {
            a.direction.cmp(&b.direction).then_with(|| {
                // MIDI ports first within each direction group
                let a_midi = a.media_type == Some(MediaType::Midi);
                let b_midi = b.media_type == Some(MediaType::Midi);
                b_midi.cmp(&a_midi).then_with(|| natural_cmp(&a.name, &b.name))
            })
        });
        ports
    }

    pub fn get_input_ports(&self, node_id: ObjectId) -> Vec<Port> {
        let mut ports: Vec<Port> = self
            .ports
            .read()
            .values()
            .filter(|p| p.node_id == node_id && p.direction == PortDirection::Input)
            .cloned()
            .collect();
        ports.sort_by(|a, b| {
            // MIDI ports first
            let a_midi = a.media_type == Some(MediaType::Midi);
            let b_midi = b.media_type == Some(MediaType::Midi);
            b_midi.cmp(&a_midi).then_with(|| natural_cmp(&a.name, &b.name))
        });
        ports
    }

    pub fn get_output_ports(&self, node_id: ObjectId) -> Vec<Port> {
        let mut ports: Vec<Port> = self
            .ports
            .read()
            .values()
            .filter(|p| p.node_id == node_id && p.direction == PortDirection::Output)
            .cloned()
            .collect();
        ports.sort_by(|a, b| {
            // MIDI ports first
            let a_midi = a.media_type == Some(MediaType::Midi);
            let b_midi = b.media_type == Some(MediaType::Midi);
            b_midi.cmp(&a_midi).then_with(|| natural_cmp(&a.name, &b.name))
        });
        ports
    }

    pub fn insert_link(&self, link: Link) {
        self.links.write().insert(link.id, link);
        self.mark_changed();
    }

    pub fn remove_link(&self, id: ObjectId) -> Option<Link> {
        let link = self.links.write().remove(&id);
        if link.is_some() {
            self.mark_changed();
        }
        link
    }

    pub fn get_link(&self, id: ObjectId) -> Option<Link> {
        self.links.read().get(&id).cloned()
    }

    pub fn get_all_links(&self) -> Vec<Link> {
        self.links.read().values().cloned().collect()
    }

    pub fn find_link(&self, output_port_id: ObjectId, input_port_id: ObjectId) -> Option<Link> {
        self.links
            .read()
            .values()
            .find(|l| l.output_port_id == output_port_id && l.input_port_id == input_port_id)
            .cloned()
    }

    /// For a bridge node, returns the distinct port groups and a display name
    /// derived from the port.alias of the first port in each group.
    /// Returns a map from port_group -> device display name.
    pub fn get_bridge_port_groups(&self, node_id: ObjectId) -> BTreeMap<String, String> {
        let ports = self.ports.read();
        let mut groups: BTreeMap<String, String> = BTreeMap::new();
        for port in ports.values() {
            if port.node_id != node_id {
                continue;
            }
            if let Some(ref group) = port.port_group {
                if groups.contains_key(group) {
                    continue;
                }
                // Derive device name from port.alias: "DeviceName:PortName" -> "DeviceName"
                let device_name = if let Some(ref alias) = port.port_alias {
                    if let Some(colon_pos) = alias.find(':') {
                        alias[..colon_pos].to_string()
                    } else {
                        alias.clone()
                    }
                } else {
                    group.clone()
                };
                groups.insert(group.clone(), device_name);
            }
        }
        groups
    }

    /// Get ports for a bridge node filtered to a specific port group.
    pub fn get_ports_for_bridge_group(&self, node_id: ObjectId, group: &str) -> Vec<Port> {
        let mut ports: Vec<Port> = self
            .ports
            .read()
            .values()
            .filter(|p| {
                p.node_id == node_id
                    && p.port_group.as_deref() == Some(group)
            })
            .cloned()
            .collect();
        ports.sort_by(|a, b| {
            a.direction
                .cmp(&b.direction)
                .then_with(|| natural_cmp(&a.name, &b.name))
        });
        ports
    }

    /// Remove all ports and links belonging to a node.  Returns the IDs of
    /// links that were removed so the caller can emit proper events.
    pub fn cleanup_node(&self, node_id: ObjectId) -> Vec<ObjectId> {
        let port_ids: Vec<ObjectId> = self
            .ports
            .read()
            .values()
            .filter(|p| p.node_id == node_id)
            .map(|p| p.id)
            .collect();

        let mut removed_links = Vec::new();
        {
            let mut links = self.links.write();
            links.retain(|&id, l| {
                if port_ids.contains(&l.output_port_id)
                    || port_ids.contains(&l.input_port_id)
                {
                    removed_links.push(id);
                    false
                } else {
                    true
                }
            });
        }

        {
            let mut ports = self.ports.write();
            for port_id in port_ids {
                ports.remove(&port_id);
            }
        }

        self.mark_changed();
        removed_links
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: ObjectId, name: &str) -> Node {
        Node {
            id,
            name: name.to_string(),
            description: String::new(),
            media_type: Some(MediaType::Audio),
            node_type: Some(NodeType::Plugin),
            is_virtual: false,
            is_jack: false,
            is_bridge: false,
            ready: true,
        }
    }

    fn make_port(id: ObjectId, node_id: ObjectId, name: &str, dir: PortDirection) -> Port {
        Port {
            id,
            node_id,
            name: name.to_string(),
            direction: dir,
            media_type: Some(MediaType::Audio),
            channel: None,
            physical_index: None,
            port_group: None,
            port_alias: None,
        }
    }

    fn make_link(id: ObjectId, out_node: ObjectId, out_port: ObjectId, in_node: ObjectId, in_port: ObjectId) -> Link {
        Link {
            id,
            output_node_id: out_node,
            output_port_id: out_port,
            input_node_id: in_node,
            input_port_id: in_port,
            active: true,
        }
    }

    // ---- natural_cmp ----

    #[test]
    fn natural_cmp_equal() {
        assert_eq!(natural_cmp("abc", "abc"), Ordering::Equal);
    }

    #[test]
    fn natural_cmp_alphabetical() {
        assert_eq!(natural_cmp("abc", "def"), Ordering::Less);
        assert_eq!(natural_cmp("def", "abc"), Ordering::Greater);
    }

    #[test]
    fn natural_cmp_numeric_sorting() {
        assert_eq!(natural_cmp("item2", "item10"), Ordering::Less);
        assert_eq!(natural_cmp("item10", "item2"), Ordering::Greater);
        assert_eq!(natural_cmp("item10", "item10"), Ordering::Equal);
    }

    #[test]
    fn natural_cmp_mixed() {
        let mut items = vec!["plugin10", "plugin2", "plugin1", "plugin20"];
        items.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(items, vec!["plugin1", "plugin2", "plugin10", "plugin20"]);
    }

    #[test]
    fn natural_cmp_empty_strings() {
        assert_eq!(natural_cmp("", ""), Ordering::Equal);
        assert_eq!(natural_cmp("", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a", ""), Ordering::Greater);
    }

    #[test]
    fn natural_cmp_pure_numbers() {
        assert_eq!(natural_cmp("2", "10"), Ordering::Less);
        assert_eq!(natural_cmp("100", "20"), Ordering::Greater);
    }

    // ---- GraphState: nodes ----

    #[test]
    fn graph_state_insert_and_get_node() {
        let gs = GraphState::new();
        gs.insert_node(make_node(1, "Node1"));

        let node = gs.get_node(1);
        assert!(node.is_some());
        assert_eq!(node.unwrap().name, "Node1");
    }

    #[test]
    fn graph_state_remove_node() {
        let gs = GraphState::new();
        gs.insert_node(make_node(1, "Node1"));

        let removed = gs.remove_node(1);
        assert!(removed.is_some());
        assert!(gs.get_node(1).is_none());
    }

    #[test]
    fn graph_state_remove_nonexistent_node() {
        let gs = GraphState::new();
        assert!(gs.remove_node(999).is_none());
    }

    #[test]
    fn graph_state_get_all_nodes() {
        let gs = GraphState::new();
        gs.insert_node(make_node(1, "A"));
        gs.insert_node(make_node(2, "B"));

        let all = gs.get_all_nodes();
        assert_eq!(all.len(), 2);
    }

    // ---- GraphState: ports ----

    #[test]
    fn graph_state_insert_and_get_port() {
        let gs = GraphState::new();
        gs.insert_port(make_port(10, 1, "out_0", PortDirection::Output));

        let port = gs.get_port(10);
        assert!(port.is_some());
        assert_eq!(port.unwrap().name, "out_0");
    }

    #[test]
    fn graph_state_remove_port() {
        let gs = GraphState::new();
        gs.insert_port(make_port(10, 1, "out_0", PortDirection::Output));

        let removed = gs.remove_port(10);
        assert!(removed.is_some());
        assert!(gs.get_port(10).is_none());
    }

    #[test]
    fn graph_state_get_ports_for_node() {
        let gs = GraphState::new();
        gs.insert_port(make_port(10, 1, "output_0", PortDirection::Output));
        gs.insert_port(make_port(11, 1, "input_0", PortDirection::Input));
        gs.insert_port(make_port(12, 2, "output_0", PortDirection::Output)); // different node

        let ports = gs.get_ports_for_node(1);
        assert_eq!(ports.len(), 2);
    }

    #[test]
    fn graph_state_get_input_output_ports() {
        let gs = GraphState::new();
        gs.insert_port(make_port(10, 1, "output_0", PortDirection::Output));
        gs.insert_port(make_port(11, 1, "output_1", PortDirection::Output));
        gs.insert_port(make_port(12, 1, "input_0", PortDirection::Input));

        assert_eq!(gs.get_input_ports(1).len(), 1);
        assert_eq!(gs.get_output_ports(1).len(), 2);
    }

    // ---- GraphState: links ----

    #[test]
    fn graph_state_insert_and_get_link() {
        let gs = GraphState::new();
        gs.insert_link(make_link(100, 1, 10, 2, 20));

        let link = gs.get_link(100);
        assert!(link.is_some());
        assert_eq!(link.unwrap().output_port_id, 10);
    }

    #[test]
    fn graph_state_remove_link() {
        let gs = GraphState::new();
        gs.insert_link(make_link(100, 1, 10, 2, 20));

        let removed = gs.remove_link(100);
        assert!(removed.is_some());
        assert!(gs.get_link(100).is_none());
    }

    #[test]
    fn graph_state_find_link() {
        let gs = GraphState::new();
        gs.insert_link(make_link(100, 1, 10, 2, 20));

        assert!(gs.find_link(10, 20).is_some());
        assert!(gs.find_link(10, 99).is_none());
    }

    #[test]
    fn graph_state_get_all_links() {
        let gs = GraphState::new();
        gs.insert_link(make_link(100, 1, 10, 2, 20));
        gs.insert_link(make_link(101, 3, 30, 4, 40));

        assert_eq!(gs.get_all_links().len(), 2);
    }

    // ---- GraphState: cleanup ----

    #[test]
    fn graph_state_cleanup_port_removes_associated_links() {
        let gs = GraphState::new();
        gs.insert_link(make_link(100, 1, 10, 2, 20));
        gs.insert_link(make_link(101, 3, 30, 4, 40));
        gs.insert_link(make_link(102, 1, 10, 5, 50)); // also uses port 10

        let removed = gs.cleanup_port(10);
        assert_eq!(removed.len(), 2); // links 100 and 102
        assert!(gs.get_link(100).is_none());
        assert!(gs.get_link(102).is_none());
        assert!(gs.get_link(101).is_some()); // unaffected
    }

    #[test]
    fn graph_state_cleanup_node_removes_ports_and_links() {
        let gs = GraphState::new();
        gs.insert_port(make_port(10, 1, "out", PortDirection::Output));
        gs.insert_port(make_port(11, 1, "in", PortDirection::Input));
        gs.insert_port(make_port(20, 2, "out", PortDirection::Output));
        gs.insert_link(make_link(100, 1, 10, 2, 20));

        let removed_links = gs.cleanup_node(1);
        assert_eq!(removed_links.len(), 1);
        assert!(gs.get_port(10).is_none());
        assert!(gs.get_port(11).is_none());
        assert!(gs.get_port(20).is_some()); // different node
    }

    // ---- GraphState: change counter ----

    #[test]
    fn graph_state_change_counter_increments() {
        let gs = GraphState::new();
        let initial = gs.change_counter();

        gs.insert_node(make_node(1, "A"));
        assert!(gs.change_counter() > initial);

        let after_node = gs.change_counter();
        gs.insert_port(make_port(10, 1, "p", PortDirection::Output));
        assert!(gs.change_counter() > after_node);
    }

    // ---- GraphState: node media type propagates to ports ----

    #[test]
    fn graph_state_insert_node_propagates_media_type() {
        let gs = GraphState::new();
        // Insert port first (no media type yet)
        let mut port = make_port(10, 1, "p", PortDirection::Output);
        port.media_type = None;
        gs.insert_port(port);

        // Now insert node with Audio media type
        gs.insert_node(make_node(1, "A"));

        let port = gs.get_port(10).unwrap();
        assert_eq!(port.media_type, Some(MediaType::Audio));
    }

    // ---- GraphState: set_node_type / set_node_description ----

    #[test]
    fn graph_state_set_node_type() {
        let gs = GraphState::new();
        gs.insert_node(make_node(1, "A"));

        gs.set_node_type(1, NodeType::Sink);
        let node = gs.get_node(1).unwrap();
        assert_eq!(node.node_type, Some(NodeType::Sink));
    }

    #[test]
    fn graph_state_set_node_description() {
        let gs = GraphState::new();
        gs.insert_node(make_node(1, "A"));

        gs.set_node_description(1, "My Plugin");
        let node = gs.get_node(1).unwrap();
        assert_eq!(node.description, "My Plugin");
    }

    // ---- Node/Port display_name ----

    #[test]
    fn node_display_name_prefers_description() {
        let mut node = make_node(1, "name");
        node.description = "description".to_string();
        assert_eq!(node.display_name(), "description");
    }

    #[test]
    fn node_display_name_falls_back_to_name() {
        let node = make_node(1, "name");
        assert_eq!(node.display_name(), "name");
    }

    #[test]
    fn node_display_name_falls_back_to_unknown() {
        let mut node = make_node(1, "");
        node.description = "".to_string();
        assert_eq!(node.display_name(), "Unknown");
    }

    #[test]
    fn port_display_name_prefers_channel() {
        let mut port = make_port(10, 1, "name", PortDirection::Output);
        port.channel = Some("FL".to_string());
        assert_eq!(port.display_name(), "FL");
    }

    #[test]
    fn port_display_name_falls_back_to_name() {
        let port = make_port(10, 1, "output_0", PortDirection::Output);
        assert_eq!(port.display_name(), "output_0");
    }

    // ---- NodeType predicates ----

    #[test]
    fn node_type_has_outputs() {
        assert!(NodeType::Source.has_outputs());
        assert!(NodeType::StreamOutput.has_outputs());
        assert!(NodeType::Duplex.has_outputs());
        assert!(NodeType::Plugin.has_outputs());
        assert!(!NodeType::Sink.has_outputs());
        assert!(!NodeType::StreamInput.has_outputs());
    }

    #[test]
    fn node_type_has_inputs() {
        assert!(NodeType::Sink.has_inputs());
        assert!(NodeType::StreamInput.has_inputs());
        assert!(NodeType::Duplex.has_inputs());
        assert!(NodeType::Plugin.has_inputs());
        assert!(!NodeType::Source.has_inputs());
        assert!(!NodeType::StreamOutput.has_inputs());
    }

    // ---- Bridge port groups ----

    #[test]
    fn graph_state_bridge_port_groups() {
        let gs = GraphState::new();
        let mut port1 = make_port(10, 1, "p1", PortDirection::Output);
        port1.port_group = Some("group1".to_string());
        port1.port_alias = Some("Device1:Port1".to_string());

        let mut port2 = make_port(11, 1, "p2", PortDirection::Output);
        port2.port_group = Some("group2".to_string());
        port2.port_alias = Some("Device2:Port2".to_string());

        gs.insert_port(port1);
        gs.insert_port(port2);

        let groups = gs.get_bridge_port_groups(1);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups.get("group1").unwrap(), "Device1");
        assert_eq!(groups.get("group2").unwrap(), "Device2");
    }
}
