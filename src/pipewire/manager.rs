use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use libspa::utils::dict::DictRef;
use pipewire::{
    context::ContextRc, link::Link as PwLink, main_loop::MainLoopRc, registry::GlobalObject,
    types::ObjectType,
};

use super::state::GraphState;
use super::types::*;

#[derive(Debug)]
enum InternalOp {
    Connect {
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    },
    Disconnect {
        link_id: ObjectId,
    },
    AddPlugin {
        plugin_uri: String,
        instance_id: u64,
        display_name: String,
    },
    RemovePlugin {
        instance_id: u64,
    },
    OpenPluginUI {
        instance_id: u64,
    },
    ClosePluginUI {
        instance_id: u64,
    },
}

pub fn start(
    graph: Arc<GraphState>,
    tick_interval_ms: u64,
    operation_cooldown_ms: u64,
) -> (Receiver<PwEvent>, Sender<PwCommand>) {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();

    let cmd_tx_for_pw = cmd_tx.clone();

    let tick = tick_interval_ms.max(1);
    let cooldown = operation_cooldown_ms.max(1);

    std::thread::spawn(move || {
        if let Err(e) =
            run_pipewire_thread(graph, event_tx.clone(), cmd_rx, cmd_tx_for_pw, tick, cooldown)
        {
            log::error!("PipeWire thread error: {}", e);
            let _ = event_tx.send(PwEvent::Error(e.to_string()));
        }
    });

    (event_rx, cmd_tx)
}

fn run_pipewire_thread(
    graph: Arc<GraphState>,
    event_tx: Sender<PwEvent>,
    cmd_rx: Receiver<PwCommand>,
    cmd_tx: Sender<PwCommand>,
    tick_interval_ms: u64,
    operation_cooldown_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    pipewire::init();

    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let pending_ops: Rc<RefCell<Vec<InternalOp>>> = Rc::new(RefCell::new(Vec::new()));
    let last_op_time: Rc<RefCell<Instant>> =
        Rc::new(RefCell::new(Instant::now() - Duration::from_secs(1)));
    let changes_pending: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    let _registry_listener = {
        let graph = graph.clone();
        let event_tx = event_tx.clone();
        let changes_pending = changes_pending.clone();

        registry
            .add_listener_local()
            .global({
                let graph = graph.clone();
                let event_tx = event_tx.clone();
                let changes_pending = changes_pending.clone();

                move |global| {
                    match global.type_ {
                        ObjectType::Node => {
                            if let Some(node) = parse_node(global) {
                                if node.node_type == Some(NodeType::Lv2Plugin)
                                    && let Some(props) = global.props.as_ref()
                                    && let Some(id_str) = props.get("zestbay.lv2.instance_id")
                                    && let Ok(instance_id) = id_str.parse::<u64>()
                                {
                                    let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginAdded {
                                        instance_id,
                                        pw_node_id: global.id,
                                        display_name: node.display_name().to_string(),
                                    }));
                                }
                                graph.insert_node(node.clone());
                                let _ = event_tx.send(PwEvent::NodeChanged(node));
                                *changes_pending.borrow_mut() = true;
                            }
                        }
                        ObjectType::Port => {
                            if let Some(port) = parse_port(global, &graph) {
                                log::debug!(
                                    "Port registered: id={} node={} name={:?} dir={:?}",
                                    port.id,
                                    port.node_id,
                                    port.name,
                                    port.direction
                                );
                                graph.insert_port(port.clone());
                                let _ = event_tx.send(PwEvent::PortChanged(port));
                                *changes_pending.borrow_mut() = true;
                            } else {
                                log::debug!(
                                    "Port global {} could not be parsed (props: {:?})",
                                    global.id,
                                    global.props.as_ref().map(|p| props_to_debug(p))
                                );
                            }
                        }
                        ObjectType::Link => {
                            if let Some(link) = parse_link_from_props(global) {
                                graph.insert_link(link.clone());
                                let _ = event_tx.send(PwEvent::LinkChanged(link));
                                *changes_pending.borrow_mut() = true;
                            }
                        }
                        _ => {}
                    }
                }
            })
            .global_remove({
                let graph = graph.clone();
                let event_tx = event_tx.clone();
                let changes_pending = changes_pending.clone();

                move |id| {
                    if graph.remove_node(id).is_some() {
                        graph.cleanup_node(id);
                        let _ = event_tx.send(PwEvent::NodeRemoved(id));
                        *changes_pending.borrow_mut() = true;
                    } else if let Some(port) = graph.remove_port(id) {
                        let _ = event_tx.send(PwEvent::PortRemoved {
                            port_id: id,
                            node_id: port.node_id,
                        });
                        *changes_pending.borrow_mut() = true;
                    } else if graph.remove_link(id).is_some() {
                        let _ = event_tx.send(PwEvent::LinkRemoved(id));
                        *changes_pending.borrow_mut() = true;
                    }
                }
            })
            .register()
    };

    let (pw_cmd_tx, pw_cmd_rx) = pipewire::channel::channel();
    std::thread::spawn({
        let pw_cmd_tx = pw_cmd_tx.clone();
        move || {
            while let Ok(cmd) = cmd_rx.recv() {
                if pw_cmd_tx.send(cmd).is_err() {
                    break;
                }
            }
        }
    });

    let (internal_tx, internal_rx) = pipewire::channel::channel::<InternalOp>();

    let lv2_instances: Rc<
        RefCell<HashMap<u64, std::rc::Rc<RefCell<crate::lv2::host::Lv2PluginInstance>>>>,
    > = Rc::new(RefCell::new(HashMap::new()));
    let lv2_filters: Rc<RefCell<HashMap<u64, crate::lv2::filter::Lv2FilterNode>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let urid_mapper = Arc::new(crate::lv2::urid::UridMapper::new());

    let _cmd_receiver = pw_cmd_rx.attach(mainloop.loop_(), {
        let pending_ops = pending_ops.clone();
        let lv2_instances = lv2_instances.clone();
        let event_tx = event_tx.clone();

        move |cmd| {
            match cmd {
                PwCommand::SetPluginParameter {
                    instance_id,
                    port_index,
                    value,
                } => {
                    if let Some(instance) = lv2_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().set_parameter(port_index, value);
                        let _ = event_tx.send(PwEvent::Lv2(Lv2Event::ParameterChanged {
                            instance_id,
                            port_index,
                            value,
                        }));
                    }
                }
                PwCommand::SetPluginBypass {
                    instance_id,
                    bypassed,
                } => {
                    if let Some(instance) = lv2_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().bypassed = bypassed;
                    }
                }
                cmd => {
                    let op = match cmd {
                        PwCommand::Connect {
                            output_port_id,
                            input_port_id,
                        } => InternalOp::Connect {
                            output_port_id,
                            input_port_id,
                        },
                        PwCommand::Disconnect { link_id } => InternalOp::Disconnect { link_id },
                        PwCommand::AddPlugin {
                            plugin_uri,
                            instance_id,
                            display_name,
                        } => InternalOp::AddPlugin {
                            plugin_uri,
                            instance_id,
                            display_name,
                        },
                        PwCommand::RemovePlugin { instance_id } => {
                            InternalOp::RemovePlugin { instance_id }
                        }
                        PwCommand::OpenPluginUI { instance_id } => {
                            InternalOp::OpenPluginUI { instance_id }
                        }
                        PwCommand::ClosePluginUI { instance_id } => {
                            InternalOp::ClosePluginUI { instance_id }
                        }
                        PwCommand::SetPluginParameter { .. }
                        | PwCommand::SetPluginBypass { .. } => unreachable!(),
                    };
                    pending_ops.borrow_mut().push(op);
                }
            }
        }
    });

    let _timer = mainloop.loop_().add_timer({
        let pending_ops = pending_ops.clone();
        let last_op_time = last_op_time.clone();
        let internal_tx = internal_tx.clone();
        let changes_pending = changes_pending.clone();
        let event_tx = event_tx.clone();

        move |_| {
            let now = Instant::now();

            {
                let mut ops = pending_ops.borrow_mut();
                let mut i = 0;
                while i < ops.len() {
                    match &ops[i] {
                        InternalOp::Connect { .. } | InternalOp::Disconnect { .. } => {
                            let op = ops.remove(i);
                            let _ = internal_tx.send(op);
                        }
                        _ => {
                            i += 1;
                        }
                    }
                }
            }

            if now.duration_since(*last_op_time.borrow())
                < Duration::from_millis(operation_cooldown_ms)
            {
                if *changes_pending.borrow() {
                    *changes_pending.borrow_mut() = false;
                    let _ = event_tx.send(PwEvent::BatchComplete);
                }
                return;
            }

            let op = if !pending_ops.borrow().is_empty() {
                Some(pending_ops.borrow_mut().remove(0))
            } else {
                None
            };
            if let Some(op) = op {
                let _ = internal_tx.send(op);
                *last_op_time.borrow_mut() = now;
            }

            if *changes_pending.borrow() {
                *changes_pending.borrow_mut() = false;
                let _ = event_tx.send(PwEvent::BatchComplete);
            }
        }
    });

    let _ = _timer.update_timer(
        Some(Duration::from_millis(tick_interval_ms)),
        Some(Duration::from_millis(tick_interval_ms)),
    );

    let _internal_receiver = internal_rx.attach(mainloop.loop_(), {
        let graph = graph.clone();
        let core = core.clone();
        let registry = registry.clone();
        let event_tx = event_tx.clone();
        let cmd_tx = cmd_tx.clone();
        let lv2_instances = lv2_instances.clone();
        let lv2_filters = lv2_filters.clone();
        let urid_mapper = urid_mapper.clone();

        move |op| match op {
            InternalOp::Connect {
                output_port_id,
                input_port_id,
            } => {
                create_link(&graph, &core, output_port_id, input_port_id);
            }
            InternalOp::Disconnect { link_id } => {
                registry.destroy_global(link_id);
            }
            InternalOp::AddPlugin {
                plugin_uri,
                instance_id,
                display_name,
            } => {
                handle_add_plugin(
                    &core,
                    &event_tx,
                    &lv2_instances,
                    &lv2_filters,
                    &urid_mapper,
                    &plugin_uri,
                    instance_id,
                    &display_name,
                );
            }
            InternalOp::RemovePlugin { instance_id } => {
                crate::lv2::ui::close_plugin_ui(instance_id);
                lv2_filters.borrow_mut().remove(&instance_id);
                lv2_instances.borrow_mut().remove(&instance_id);
                let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginRemoved { instance_id }));
            }

            InternalOp::OpenPluginUI { instance_id } => {
                if let Some(instance) = lv2_instances.borrow().get(&instance_id) {
                    let inst = instance.borrow();
                    let plugin_uri = inst.plugin_uri.clone();
                    let port_updates = inst.port_updates.clone();
                    let control_values: Vec<(usize, f32)> = inst
                        .control_inputs
                        .iter()
                        .map(|cp| (cp.index, cp.value))
                        .collect();
                    drop(inst);
                    handle_open_plugin_ui(
                        &event_tx,
                        &cmd_tx,
                        &plugin_uri,
                        instance_id,
                        control_values,
                        port_updates,
                        urid_mapper.clone(),
                    );
                }
            }

            InternalOp::ClosePluginUI { instance_id } => {
                crate::lv2::ui::close_plugin_ui(instance_id);
            }
        }
    });

    log::info!("PipeWire thread started");
    mainloop.run();

    Ok(())
}

fn parse_node(global: &GlobalObject<&DictRef>) -> Option<Node> {
    let props = global.props.as_ref()?;

    let name = props.get("node.name").unwrap_or_default().to_string();
    let description = props
        .get("node.description")
        .or_else(|| props.get("node.nick"))
        .unwrap_or_default()
        .to_string();
    let media_class = props.get("media.class").unwrap_or_default().to_string();

    let effective_class = if !media_class.is_empty() {
        media_class.clone()
    } else {
        let mt = props.get("media.type").unwrap_or_default();
        let mc = props.get("media.category").unwrap_or_default();
        if !mt.is_empty() || !mc.is_empty() {
            format!("{}/{}", mt, mc)
        } else {
            String::new()
        }
    };

    let media_type = if effective_class.contains("Audio") {
        Some(MediaType::Audio)
    } else if effective_class.contains("Video") {
        Some(MediaType::Video)
    } else if effective_class.contains("Midi") {
        Some(MediaType::Midi)
    } else {
        None
    };

    let is_lv2 = props.get("zestbay.lv2.instance_id").is_some();

    let node_type = if is_lv2 {
        Some(NodeType::Lv2Plugin)
    } else if effective_class.contains("Sink") {
        Some(NodeType::Sink)
    } else if effective_class.contains("Source") && !effective_class.contains("Stream") {
        Some(NodeType::Source)
    } else if effective_class.contains("Stream/Output") || effective_class.contains("Playback") {
        Some(NodeType::StreamOutput)
    } else if effective_class.contains("Stream/Input") || effective_class.contains("Record") {
        Some(NodeType::StreamInput)
    } else if effective_class.contains("Duplex") || effective_class.contains("Bridge") {
        Some(NodeType::Duplex)
    } else {
        None
    };

    let is_virtual = props
        .get("node.virtual")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let is_jack = props
        .get("client.api")
        .map(|v| v == "jack")
        .unwrap_or(false);

    Some(Node {
        id: global.id,
        name,
        description,
        media_type,
        node_type,
        is_virtual,
        is_jack,
        ready: true,
    })
}

fn parse_port(global: &GlobalObject<&DictRef>, graph: &Arc<GraphState>) -> Option<Port> {
    let props = global.props.as_ref()?;

    let node_id: ObjectId = match props.get("node.id") {
        Some(v) => match v.parse() {
            Ok(id) => id,
            Err(_) => {
                log::debug!("Port {}: failed to parse node.id {:?}", global.id, v);
                return None;
            }
        },
        None => {
            log::debug!("Port {}: missing node.id property", global.id);
            return None;
        }
    };
    let name = props.get("port.name").unwrap_or_default().to_string();
    let channel = props.get("audio.channel").map(String::from);
    let physical_index = props.get("port.physical").and_then(|s| s.parse().ok());

    let direction = match props.get("port.direction") {
        Some("in") => PortDirection::Input,
        Some("out") => PortDirection::Output,
        Some(other) => {
            log::debug!(
                "Port {} (node {}): unknown port.direction {:?}",
                global.id,
                node_id,
                other
            );
            return None;
        }
        None => {
            if name.starts_with("input") || name.starts_with("playback") {
                log::debug!(
                    "Port {} (node {}): missing port.direction, inferred Input from name {:?}",
                    global.id,
                    node_id,
                    name
                );
                PortDirection::Input
            } else if name.starts_with("output")
                || name.starts_with("capture")
                || name.starts_with("monitor")
            {
                log::debug!(
                    "Port {} (node {}): missing port.direction, inferred Output from name {:?}",
                    global.id,
                    node_id,
                    name
                );
                PortDirection::Output
            } else {
                log::warn!(
                    "Port {} (node {}): missing port.direction, cannot infer from name {:?}. Props: {:?}",
                    global.id,
                    node_id,
                    name,
                    props_to_debug(props)
                );
                return None;
            }
        }
    };

    let media_type = graph.get_node(node_id).and_then(|n| n.media_type);

    Some(Port {
        id: global.id,
        node_id,
        name,
        direction,
        media_type,
        channel,
        physical_index,
    })
}

fn props_to_debug(props: &DictRef) -> Vec<(String, String)> {
    props
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn parse_link_from_props(global: &GlobalObject<&DictRef>) -> Option<Link> {
    let props = global.props.as_ref()?;

    Some(Link {
        id: global.id,
        output_node_id: props.get("link.output.node")?.parse().ok()?,
        output_port_id: props.get("link.output.port")?.parse().ok()?,
        input_node_id: props.get("link.input.node")?.parse().ok()?,
        input_port_id: props.get("link.input.port")?.parse().ok()?,
        active: false,
    })
}

type GlobalSharedMutHashMap<K, V> = Rc<RefCell<HashMap<K, Rc<RefCell<V>>>>>;

#[allow(clippy::too_many_arguments)]
fn handle_add_plugin(
    core: &pipewire::core::CoreRc,
    event_tx: &Sender<PwEvent>,
    lv2_instances: &GlobalSharedMutHashMap<u64, crate::lv2::host::Lv2PluginInstance>,
    lv2_filters: &Rc<RefCell<HashMap<u64, crate::lv2::filter::Lv2FilterNode>>>,
    urid_mapper: &Arc<crate::lv2::urid::UridMapper>,
    plugin_uri: &str,
    instance_id: u64,
    display_name: &str,
) {
    let world = lilv::World::with_load_all();
    let uri_node = world.new_uri(plugin_uri);

    let lilv_plugin = world
        .plugins()
        .iter()
        .find(|p| p.uri().as_uri() == uri_node.as_uri());

    let lilv_plugin = match lilv_plugin {
        Some(p) => p,
        None => {
            let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("Plugin not found: {}", plugin_uri),
                fatal: true,
            }));
            return;
        }
    };

    let plugin_info = match build_plugin_info(&world, &lilv_plugin) {
        Some(info) => info,
        None => {
            let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("Failed to parse plugin info: {}", plugin_uri),
                fatal: true,
            }));
            return;
        }
    };

    let sample_rate = 48000.0;

    let lv2_instance = unsafe {
        crate::lv2::host::Lv2PluginInstance::new(
            world,
            &lilv_plugin,
            &plugin_info,
            sample_rate,
            urid_mapper,
        )
    };

    let lv2_instance = match lv2_instance {
        Some(inst) => inst,
        None => {
            let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("Failed to instantiate plugin: {}", plugin_uri),
                fatal: true,
            }));
            return;
        }
    };

    let instance_rc = std::rc::Rc::new(RefCell::new(lv2_instance));

    let filter_config = crate::lv2::filter::FilterConfig {
        instance_id,
        display_name: display_name.to_string(),
        audio_inputs: plugin_info.audio_inputs,
        audio_outputs: plugin_info.audio_outputs,
        sample_rate: sample_rate as u32,
    };

    match crate::lv2::filter::Lv2FilterNode::new(
        core,
        filter_config,
        instance_rc.clone(),
        event_tx.clone(),
    ) {
        Ok(filter) => {
            lv2_instances.borrow_mut().insert(instance_id, instance_rc);
            lv2_filters.borrow_mut().insert(instance_id, filter);

            log::info!(
                "LV2 filter created for instance {}, waiting for node ID...",
                instance_id
            );
        }
        Err(e) => {
            let _ = event_tx.send(PwEvent::Lv2(Lv2Event::PluginError {
                instance_id: Some(instance_id),
                message: format!("Failed to create filter node: {}", e),
                fatal: true,
            }));
        }
    }
}

fn build_plugin_info(
    world: &lilv::World,
    plugin: &lilv::plugin::Plugin,
) -> Option<crate::lv2::Lv2PluginInfo> {
    let input_class = world.new_uri("http://lv2plug.in/ns/lv2core#InputPort");
    let output_class = world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
    let audio_class = world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
    let control_class = world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort");
    let atom_class = world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort");

    let uri = plugin.uri().as_uri()?.to_string();
    let name = plugin.name().as_str()?.to_string();
    let category = crate::lv2::Lv2PluginCategory::from_class_label(
        plugin.class().label().as_str().unwrap_or("Plugin"),
    );
    let author = plugin
        .author_name()
        .and_then(|n| n.as_str().map(String::from));

    let mut ports = Vec::new();
    let mut audio_inputs = 0usize;
    let mut audio_outputs = 0usize;
    let mut control_inputs = 0usize;
    let mut control_outputs = 0usize;

    let port_ranges = plugin.port_ranges_float();

    for (i, port_range) in port_ranges.iter().enumerate() {
        let port = plugin.port_by_index(i)?;

        let port_symbol = port
            .symbol()
            .and_then(|s| s.as_str().map(String::from))
            .unwrap_or_else(|| format!("port_{}", i));

        let port_name = port
            .name()
            .and_then(|n| n.as_str().map(String::from))
            .unwrap_or_else(|| port_symbol.clone());

        let is_input = port.is_a(&input_class);
        let is_output = port.is_a(&output_class);
        let is_audio = port.is_a(&audio_class);
        let is_control = port.is_a(&control_class);
        let is_atom = port.is_a(&atom_class);

        let port_type = if is_audio && is_input {
            audio_inputs += 1;
            crate::lv2::Lv2PortType::AudioInput
        } else if is_audio && is_output {
            audio_outputs += 1;
            crate::lv2::Lv2PortType::AudioOutput
        } else if is_control && is_input {
            control_inputs += 1;
            crate::lv2::Lv2PortType::ControlInput
        } else if is_control && is_output {
            control_outputs += 1;
            crate::lv2::Lv2PortType::ControlOutput
        } else if is_atom && is_input {
            crate::lv2::Lv2PortType::AtomInput
        } else if is_atom && is_output {
            crate::lv2::Lv2PortType::AtomOutput
        } else {
            continue;
        };

        ports.push(crate::lv2::Lv2PortInfo {
            index: i,
            symbol: port_symbol,
            name: port_name,
            port_type,
            default_value: port_range.default,
            min_value: port_range.min,
            max_value: port_range.max,
        });
    }

    Some(crate::lv2::Lv2PluginInfo {
        uri,
        name,
        category,
        author,
        ports,
        audio_inputs,
        audio_outputs,
        control_inputs,
        control_outputs,
        required_features: Vec::new(),
        compatible: true,
    })
}

fn handle_open_plugin_ui(
    event_tx: &Sender<PwEvent>,
    cmd_tx: &Sender<PwCommand>,
    plugin_uri: &str,
    instance_id: u64,
    control_values: Vec<(usize, f32)>,
    port_updates: crate::lv2::SharedPortUpdates,
    urid_mapper: Arc<crate::lv2::urid::UridMapper>,
) {
    crate::lv2::ui::open_plugin_ui(
        plugin_uri,
        instance_id,
        cmd_tx.clone(),
        event_tx.clone(),
        control_values,
        port_updates,
        urid_mapper,
    );
}

fn create_link(
    graph: &Arc<GraphState>,
    core: &pipewire::core::CoreRc,
    output_port_id: ObjectId,
    input_port_id: ObjectId,
) {
    let output_port = match graph.get_port(output_port_id) {
        Some(p) if p.direction == PortDirection::Output => p,
        _ => {
            log::warn!("Invalid output port {}", output_port_id);
            return;
        }
    };

    let input_port = match graph.get_port(input_port_id) {
        Some(p) if p.direction == PortDirection::Input => p,
        _ => {
            log::warn!("Invalid input port {}", input_port_id);
            return;
        }
    };

    if output_port.node_id == input_port.node_id {
        log::warn!(
            "Rejected self-loop: port {} and port {} belong to the same node {}",
            output_port_id, input_port_id, output_port.node_id
        );
        return;
    }

    log::debug!("Creating link {} -> {}", output_port_id, input_port_id);

    let props = pipewire::properties::properties! {
        *pipewire::keys::LINK_OUTPUT_NODE => output_port.node_id.to_string(),
        *pipewire::keys::LINK_OUTPUT_PORT => output_port_id.to_string(),
        *pipewire::keys::LINK_INPUT_NODE => input_port.node_id.to_string(),
        *pipewire::keys::LINK_INPUT_PORT => input_port_id.to_string(),
        *pipewire::keys::OBJECT_LINGER => "true",
    };

    if let Err(e) = core.create_object::<PwLink>("link-factory", &props) {
        log::error!("Failed to create link: {}", e);
    }
}
