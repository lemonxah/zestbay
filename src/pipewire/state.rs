use parking_lot::RwLock;
use std::cmp::Ordering;
use std::collections::HashMap;
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
        ports.sort_by(|a, b| a.direction.cmp(&b.direction).then_with(|| natural_cmp(&a.name, &b.name)));
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
        ports.sort_by(|a, b| natural_cmp(&a.name, &b.name));
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
        ports.sort_by(|a, b| natural_cmp(&a.name, &b.name));
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

    pub fn cleanup_node(&self, node_id: ObjectId) {
        let port_ids: Vec<ObjectId> = self
            .ports
            .read()
            .values()
            .filter(|p| p.node_id == node_id)
            .map(|p| p.id)
            .collect();

        {
            let mut links = self.links.write();
            links.retain(|_, l| {
                !port_ids.contains(&l.output_port_id) && !port_ids.contains(&l.input_port_id)
            });
        }

        {
            let mut ports = self.ports.write();
            for port_id in port_ids {
                ports.remove(&port_id);
            }
        }

        self.mark_changed();
    }
}
