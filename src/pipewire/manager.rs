use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
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
        format: String,
        lv2_state: Vec<crate::lv2::state::StateEntry>,
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

    // Detect the PipeWire graph sample rate and quantum from core properties.
    // Default to 48000 Hz / 1024 frames; updated when the core info callback fires.
    let pw_sample_rate = Rc::new(AtomicU32::new(48000));
    let pw_quantum = Rc::new(AtomicU32::new(1024));

    let _core_listener = {
        let pw_sample_rate = pw_sample_rate.clone();
        let pw_quantum = pw_quantum.clone();
        core.add_listener_local()
            .info(move |info| {
                if let Some(props) = info.props() {
                    if let Some(rate_str) = props.get("default.clock.rate") {
                        if let Ok(rate) = rate_str.parse::<u32>() {
                            let prev = pw_sample_rate.swap(rate, Ordering::Relaxed);
                            if prev != rate {
                                log::info!("PipeWire sample rate detected: {} Hz", rate);
                            }
                        }
                    }
                    if let Some(quantum_str) = props.get("default.clock.quantum") {
                        if let Ok(q) = quantum_str.parse::<u32>() {
                            let prev = pw_quantum.swap(q, Ordering::Relaxed);
                            if prev != q {
                                log::info!("PipeWire quantum detected: {} frames", q);
                            }
                        }
                    }
                }
            })
            .register()
    };

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
                                if node.node_type == Some(NodeType::Plugin)
                                    && let Some(props) = global.props.as_ref()
                                    && let Some(id_str) = props.get("zestbay.plugin.instance_id")
                                    && let Ok(instance_id) = id_str.parse::<u64>()
                                {
                                    let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginAdded {
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

    let clap_instances: Rc<
        RefCell<HashMap<u64, std::rc::Rc<RefCell<crate::clap::host::ClapPluginInstance>>>>,
    > = Rc::new(RefCell::new(HashMap::new()));
    let clap_filters: Rc<RefCell<HashMap<u64, crate::clap::filter::ClapFilterNode>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let vst3_instances: Rc<
        RefCell<HashMap<u64, std::rc::Rc<RefCell<crate::vst3::host::Vst3PluginInstance>>>>,
    > = Rc::new(RefCell::new(HashMap::new()));
    let vst3_filters: Rc<RefCell<HashMap<u64, crate::vst3::filter::Vst3FilterNode>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // MIDI controller mapping state
    let midi_mapping_table: Rc<RefCell<crate::midi::MidiMappingTable>> =
        Rc::new(RefCell::new(crate::midi::MidiMappingTable::new()));
    let midi_learn_state: Rc<RefCell<Option<crate::midi::MidiLearnState>>> =
        Rc::new(RefCell::new(None));

    let _cmd_receiver = pw_cmd_rx.attach(mainloop.loop_(), {
        let pending_ops = pending_ops.clone();
        let lv2_instances = lv2_instances.clone();
        let lv2_filters = lv2_filters.clone();
        let clap_instances = clap_instances.clone();
        let clap_filters = clap_filters.clone();
        let vst3_instances = vst3_instances.clone();
        let vst3_filters = vst3_filters.clone();
        let event_tx = event_tx.clone();
        let midi_mapping_table = midi_mapping_table.clone();
        let midi_learn_state = midi_learn_state.clone();

        move |cmd| {
            match cmd {
                PwCommand::SetPluginParameter {
                    instance_id,
                    port_index,
                    value,
                } => {
                    if let Some(instance) = lv2_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().set_parameter(port_index, value);
                        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::ParameterChanged {
                            instance_id,
                            port_index,
                            value,
                        }));
                    } else if let Some(instance) = clap_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().set_parameter(port_index, value);
                        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::ParameterChanged {
                            instance_id,
                            port_index,
                            value,
                        }));
                    } else if let Some(instance) = vst3_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().set_parameter(port_index, value);
                        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::ParameterChanged {
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
                    } else if let Some(instance) = clap_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().bypassed = bypassed;
                    } else if let Some(instance) = vst3_instances.borrow().get(&instance_id) {
                        instance.borrow_mut().bypassed = bypassed;
                    }
                }
                PwCommand::StartMidiLearn {
                    instance_id,
                    port_index,
                    label,
                    mode,
                } => {
                    *midi_learn_state.borrow_mut() = Some(crate::midi::MidiLearnState {
                        target: crate::midi::MidiCcTarget {
                            instance_id,
                            port_index,
                        },
                        label,
                        mode,
                    });
                    if let Some(filter) = lv2_filters.borrow().get(&instance_id) {
                        filter.set_learn_mode(true);
                    } else if let Some(filter) = clap_filters.borrow().get(&instance_id) {
                        filter.set_learn_mode(true);
                    } else if let Some(filter) = vst3_filters.borrow().get(&instance_id) {
                        filter.set_learn_mode(true);
                    }
                    let _ = event_tx.send(PwEvent::Plugin(PluginEvent::MidiLearnStarted {
                        instance_id,
                        port_index,
                    }));
                }
                PwCommand::CancelMidiLearn => {
                    *midi_learn_state.borrow_mut() = None;
                    for filter in lv2_filters.borrow().values() {
                        filter.set_learn_mode(false);
                    }
                    for filter in clap_filters.borrow().values() {
                        filter.set_learn_mode(false);
                    }
                    for filter in vst3_filters.borrow().values() {
                        filter.set_learn_mode(false);
                    }
                    let _ = event_tx.send(PwEvent::Plugin(PluginEvent::MidiLearnCancelled));
                }
                PwCommand::AddMidiMapping(mapping) => {
                    handle_add_midi_mapping(
                        &midi_mapping_table,
                        &lv2_instances,
                        &lv2_filters,
                        &clap_instances,
                        &clap_filters,
                        &vst3_instances,
                        &vst3_filters,
                        &event_tx,
                        mapping,
                    );
                }
                PwCommand::RemoveMidiMapping(source) => {
                    let removed = midi_mapping_table.borrow_mut().remove(&source);
                    if removed.is_some() {
                        rebuild_resolved_mappings(
                            &midi_mapping_table,
                            &lv2_instances,
                            &lv2_filters,
                            &clap_instances,
                            &clap_filters,
                            &vst3_instances,
                            &vst3_filters,
                        );
                        let _ = event_tx.send(PwEvent::Plugin(
                            PluginEvent::MidiMappingRemoved(source),
                        ));
                    }
                }
                PwCommand::RemoveMidiMappingsForPlugin { instance_id } => {
                    midi_mapping_table.borrow_mut().remove_by_instance(instance_id);
                    rebuild_resolved_mappings(
                        &midi_mapping_table,
                        &lv2_instances,
                        &lv2_filters,
                        &clap_instances,
                        &clap_filters,
                        &vst3_instances,
                        &vst3_filters,
                    );
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
                            format,
                            lv2_state,
                        } => InternalOp::AddPlugin {
                            plugin_uri,
                            instance_id,
                            display_name,
                            format,
                            lv2_state,
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
                        | PwCommand::SetPluginBypass { .. }
                        | PwCommand::StartMidiLearn { .. }
                        | PwCommand::CancelMidiLearn
                        | PwCommand::AddMidiMapping(..)
                        | PwCommand::RemoveMidiMapping(..)
                        | PwCommand::RemoveMidiMappingsForPlugin { .. } => unreachable!(),
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
        let clap_instances = clap_instances.clone();
        let clap_filters = clap_filters.clone();
        let vst3_instances = vst3_instances.clone();
        let vst3_filters = vst3_filters.clone();
        let urid_mapper = urid_mapper.clone();
        let pw_sample_rate = pw_sample_rate.clone();
        let pw_quantum = pw_quantum.clone();

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
                format,
                lv2_state,
            } => {
                let sample_rate = pw_sample_rate.load(Ordering::Relaxed) as f64;
                let block_length = pw_quantum.load(Ordering::Relaxed);
                handle_add_plugin(
                    &core,
                    &event_tx,
                    &lv2_instances,
                    &lv2_filters,
                    &clap_instances,
                    &clap_filters,
                    &vst3_instances,
                    &vst3_filters,
                    &urid_mapper,
                    &plugin_uri,
                    instance_id,
                    &display_name,
                    &format,
                    sample_rate,
                    block_length,
                    &lv2_state,
                );
            }
            InternalOp::RemovePlugin { instance_id } => {
                // Try LV2 first, then CLAP, then VST3
                if lv2_instances.borrow().contains_key(&instance_id) {
                    {
                        let instances = lv2_instances.borrow();
                        if let Some(inst_rc) = instances.get(&instance_id) {
                            let inst = inst_rc.borrow();
                            if inst.has_state_interface() {
                                if let Some(state) = unsafe { inst.save_state() } {
                                    log::info!(
                                        "LV2 state: saved {} entries for instance {}",
                                        state.len(),
                                        instance_id
                                    );
                                    let _ = event_tx.send(PwEvent::Plugin(
                                        PluginEvent::Lv2StateSaved {
                                            instance_id,
                                            state,
                                        },
                                    ));
                                }
                            }
                        }
                    }
                    crate::lv2::ui::close_plugin_ui(instance_id);
                    lv2_filters.borrow_mut().remove(&instance_id);
                    lv2_instances.borrow_mut().remove(&instance_id);
                } else if clap_instances.borrow().contains_key(&instance_id) {
                    crate::clap::ui::close_clap_gui(instance_id, &event_tx);
                    clap_filters.borrow_mut().remove(&instance_id);
                    clap_instances.borrow_mut().remove(&instance_id);
                } else {
                    crate::vst3::ui::close_vst3_gui(instance_id, &event_tx);
                    vst3_filters.borrow_mut().remove(&instance_id);
                    vst3_instances.borrow_mut().remove(&instance_id);
                }
                let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginRemoved { instance_id }));
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
                    let lv2_handle = inst.lv2_handle_ptr();
                    let extension_data_fn = inst.extension_data_fn();
                    drop(inst);
                    handle_open_plugin_ui(
                        &event_tx,
                        &cmd_tx,
                        &plugin_uri,
                        instance_id,
                        control_values,
                        port_updates,
                        urid_mapper.clone(),
                        lv2_handle,
                        extension_data_fn,
                    );
                } else if let Some(instance) = clap_instances.borrow().get(&instance_id) {
                    let inst = instance.borrow();
                    let plugin_ptr = inst.plugin_ptr();
                    let display_name = inst.display_name.clone();
                    drop(inst);
                    unsafe {
                        crate::clap::ui::open_clap_gui(
                            plugin_ptr,
                            instance_id,
                            &display_name,
                            &event_tx,
                            &cmd_tx,
                        );
                    }
                } else if let Some(instance) = vst3_instances.borrow().get(&instance_id) {
                    let inst = instance.borrow();
                    let controller_ptr = inst.controller_ptr();
                    let display_name = inst.display_name.clone();
                    drop(inst);
                    unsafe {
                        crate::vst3::ui::open_vst3_gui(
                            controller_ptr,
                            instance_id,
                            &display_name,
                            &event_tx,
                            &cmd_tx,
                        );
                    }
                }
            }

            InternalOp::ClosePluginUI { instance_id } => {
                crate::lv2::ui::close_plugin_ui(instance_id);
                crate::clap::ui::close_clap_gui(instance_id, &event_tx);
                crate::vst3::ui::close_vst3_gui(instance_id, &event_tx);
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

    let is_plugin = props.get("zestbay.plugin.instance_id").is_some();

    let node_type = if is_plugin {
        Some(NodeType::Plugin)
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
    let is_bridge = effective_class.contains("Bridge");

    Some(Node {
        id: global.id,
        name,
        description,
        media_type,
        node_type,
        is_virtual,
        is_jack,
        is_bridge,
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

    let media_type = if props.get("format.dsp").map_or(false, |v| v.contains("midi")) {
        Some(MediaType::Midi)
    } else if name.starts_with("midi_") {
        Some(MediaType::Midi)
    } else {
        graph.get_node(node_id).and_then(|n| n.media_type)
    };
    let port_group = props.get("port.group").map(String::from);
    let port_alias = props.get("port.alias").map(String::from);

    Some(Port {
        id: global.id,
        node_id,
        name,
        direction,
        media_type,
        channel,
        physical_index,
        port_group,
        port_alias,
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
    clap_instances: &GlobalSharedMutHashMap<u64, crate::clap::host::ClapPluginInstance>,
    clap_filters: &Rc<RefCell<HashMap<u64, crate::clap::filter::ClapFilterNode>>>,
    vst3_instances: &GlobalSharedMutHashMap<u64, crate::vst3::host::Vst3PluginInstance>,
    vst3_filters: &Rc<RefCell<HashMap<u64, crate::vst3::filter::Vst3FilterNode>>>,
    urid_mapper: &Arc<crate::lv2::urid::UridMapper>,
    plugin_uri: &str,
    instance_id: u64,
    display_name: &str,
    format: &str,
    sample_rate: f64,
    block_length: u32,
    lv2_state: &[crate::lv2::state::StateEntry],
) {
    match format {
        "CLAP" => handle_add_clap_plugin(
            core,
            event_tx,
            clap_instances,
            clap_filters,
            plugin_uri,
            instance_id,
            display_name,
            sample_rate,
        ),
        "VST3" => handle_add_vst3_plugin(
            core,
            event_tx,
            vst3_instances,
            vst3_filters,
            plugin_uri,
            instance_id,
            display_name,
            sample_rate,
        ),
        _ => handle_add_lv2_plugin(
            core,
            event_tx,
            lv2_instances,
            lv2_filters,
            urid_mapper,
            plugin_uri,
            instance_id,
            display_name,
            sample_rate,
            block_length,
            lv2_state,
        ),
    }
}

fn handle_add_lv2_plugin(
    core: &pipewire::core::CoreRc,
    event_tx: &Sender<PwEvent>,
    lv2_instances: &GlobalSharedMutHashMap<u64, crate::lv2::host::Lv2PluginInstance>,
    lv2_filters: &Rc<RefCell<HashMap<u64, crate::lv2::filter::Lv2FilterNode>>>,
    urid_mapper: &Arc<crate::lv2::urid::UridMapper>,
    plugin_uri: &str,
    instance_id: u64,
    display_name: &str,
    sample_rate: f64,
    block_length: u32,
    lv2_state: &[crate::lv2::state::StateEntry],
) {
    let urid_clone = urid_mapper.clone();
    let uri_owned = plugin_uri.to_string();
    let sr = sample_rate;
    let bl = block_length;

    // Exec-probe: test-instantiate in a clean child process to catch segfaults
    if !crate::NO_PROBE.load(std::sync::atomic::Ordering::SeqCst) {
        let safe = crate::plugin::sandbox::exec_probe(
            "lv2",
            &uri_owned,
            sr,
            bl,
            Some(std::time::Duration::from_secs(10)),
        );
        if !safe {
            log::error!(
                "LV2 plugin '{}' ({}) crashed during sandbox probe — skipping",
                display_name, plugin_uri
            );
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!(
                    "Plugin '{}' crashed during safety probe (segfault). It has been blocked to protect ZestBay.",
                    display_name
                ),
                fatal: true,
            }));
            return;
        }
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let world = lilv::World::with_load_all();
        let uri_node = world.new_uri(&uri_owned);

        let lilv_plugin = world
            .plugins()
            .iter()
            .find(|p| p.uri().as_uri() == uri_node.as_uri());

        let lilv_plugin = match lilv_plugin {
            Some(p) => p,
            None => return Err(format!("Plugin not found: {}", uri_owned)),
        };

        let plugin_info = match build_plugin_info(&world, &lilv_plugin) {
            Some(info) => info,
            None => return Err(format!("Failed to parse plugin info: {}", uri_owned)),
        };

        let lv2_instance = unsafe {
            crate::lv2::host::Lv2PluginInstance::new(
                world,
                &lilv_plugin,
                &plugin_info,
                sr,
                bl,
                &urid_clone,
            )
        };

        match lv2_instance {
            Some(inst) => Ok((inst, plugin_info)),
            None => Err(format!("Failed to instantiate plugin: {}", uri_owned)),
        }
    }));

    let (lv2_instance, plugin_info) = match result {
        Ok(Ok((inst, info))) => (inst, info),
        Ok(Err(msg)) => {
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: msg,
                fatal: true,
            }));
            return;
        }
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            log::error!(
                "Plugin '{}' ({}) panicked during instantiation: {}",
                display_name, plugin_uri, panic_msg
            );
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("Plugin panicked during instantiation: {}", panic_msg),
                fatal: true,
            }));
            return;
        }
    };

    if !lv2_state.is_empty() && lv2_instance.has_state_interface() {
        log::info!(
            "LV2 state: restoring {} entries for '{}'",
            lv2_state.len(),
            display_name
        );
        unsafe {
            lv2_instance.restore_state(lv2_state);
        }
    }

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
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("Failed to create filter node: {}", e),
                fatal: true,
            }));
        }
    }
}

fn handle_add_clap_plugin(
    core: &pipewire::core::CoreRc,
    event_tx: &Sender<PwEvent>,
    clap_instances: &GlobalSharedMutHashMap<u64, crate::clap::host::ClapPluginInstance>,
    clap_filters: &Rc<RefCell<HashMap<u64, crate::clap::filter::ClapFilterNode>>>,
    plugin_uri: &str,
    instance_id: u64,
    display_name: &str,
    sample_rate: f64,
) {
    let uri_owned = plugin_uri.to_string();
    let sr = sample_rate;

    // Exec-probe: test-instantiate in a clean child process to catch segfaults
    if !crate::NO_PROBE.load(std::sync::atomic::Ordering::SeqCst) {
        let safe = crate::plugin::sandbox::exec_probe(
            "clap",
            &uri_owned,
            sr,
            0,
            Some(std::time::Duration::from_secs(10)),
        );
        if !safe {
            log::error!(
                "CLAP plugin '{}' ({}) crashed during sandbox probe — skipping",
                display_name, plugin_uri
            );
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!(
                    "CLAP plugin '{}' crashed during safety probe (segfault). It has been blocked to protect ZestBay.",
                    display_name
                ),
                fatal: true,
            }));
            return;
        }
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let all_clap = crate::clap::scanner::scan_plugins();
        let clap_info = match all_clap.iter().find(|p| p.uri == uri_owned) {
            Some(info) => info.clone(),
            None => return Err(format!("CLAP plugin not found: {}", uri_owned)),
        };

        let library_path = &clap_info.library_path;
        let clap_instance = unsafe {
            crate::clap::host::ClapPluginInstance::new(
                library_path,
                &uri_owned,
                &clap_info,
                sr,
            )
        };

        match clap_instance {
            Some(inst) => Ok(inst),
            None => Err(format!("Failed to instantiate CLAP plugin: {}", uri_owned)),
        }
    }));

    let clap_instance = match result {
        Ok(Ok(inst)) => inst,
        Ok(Err(msg)) => {
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: msg,
                fatal: true,
            }));
            return;
        }
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            log::error!(
                "CLAP plugin '{}' ({}) panicked during instantiation: {}",
                display_name, plugin_uri, panic_msg
            );
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("CLAP plugin panicked during instantiation: {}", panic_msg),
                fatal: true,
            }));
            return;
        }
    };

    let audio_inputs = clap_instance.audio_input_channels;
    let audio_outputs = clap_instance.audio_output_channels;
    let instance_rc = std::rc::Rc::new(RefCell::new(clap_instance));

    let filter_config = crate::clap::filter::FilterConfig {
        instance_id,
        display_name: display_name.to_string(),
        audio_inputs,
        audio_outputs,
    };

    match crate::clap::filter::ClapFilterNode::new(
        core,
        filter_config,
        instance_rc.clone(),
        event_tx.clone(),
    ) {
        Ok(filter) => {
            clap_instances.borrow_mut().insert(instance_id, instance_rc);
            clap_filters.borrow_mut().insert(instance_id, filter);

            log::info!(
                "CLAP filter created for instance {}, waiting for node ID...",
                instance_id
            );
        }
        Err(e) => {
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("Failed to create CLAP filter node: {}", e),
                fatal: true,
            }));
        }
    }
}

fn handle_add_vst3_plugin(
    core: &pipewire::core::CoreRc,
    event_tx: &Sender<PwEvent>,
    vst3_instances: &GlobalSharedMutHashMap<u64, crate::vst3::host::Vst3PluginInstance>,
    vst3_filters: &Rc<RefCell<HashMap<u64, crate::vst3::filter::Vst3FilterNode>>>,
    plugin_uri: &str,
    instance_id: u64,
    display_name: &str,
    sample_rate: f64,
) {
    let uri_owned = plugin_uri.to_string();
    let sr = sample_rate;

    // Exec-probe: test-instantiate in a clean child process to catch segfaults
    if !crate::NO_PROBE.load(std::sync::atomic::Ordering::SeqCst) {
        let safe = crate::plugin::sandbox::exec_probe(
            "vst3",
            &uri_owned,
            sr,
            0,
            Some(std::time::Duration::from_secs(10)),
        );
        if !safe {
            log::error!(
                "VST3 plugin '{}' ({}) crashed during sandbox probe — skipping",
                display_name, plugin_uri
            );
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!(
                    "VST3 plugin '{}' crashed during safety probe (segfault). It has been blocked to protect ZestBay.",
                    display_name
                ),
                fatal: true,
            }));
            return;
        }
    }

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let all_vst3 = crate::vst3::scanner::scan_plugins();
        let vst3_info = match all_vst3.iter().find(|p| p.uri == uri_owned) {
            Some(info) => info.clone(),
            None => return Err(format!("VST3 plugin not found: {}", uri_owned)),
        };

        let library_path = &vst3_info.library_path;
        let vst3_instance = unsafe {
            crate::vst3::host::Vst3PluginInstance::new(
                library_path,
                &uri_owned,
                &vst3_info,
                sr,
            )
        };

        match vst3_instance {
            Some(inst) => Ok(inst),
            None => Err(format!("Failed to instantiate VST3 plugin: {}", uri_owned)),
        }
    }));

    let vst3_instance = match result {
        Ok(Ok(inst)) => inst,
        Ok(Err(msg)) => {
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: msg,
                fatal: true,
            }));
            return;
        }
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            log::error!(
                "VST3 plugin '{}' ({}) panicked during instantiation: {}",
                display_name, plugin_uri, panic_msg
            );
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("VST3 plugin panicked during instantiation: {}", panic_msg),
                fatal: true,
            }));
            return;
        }
    };

    let audio_inputs = vst3_instance.audio_input_channels;
    let audio_outputs = vst3_instance.audio_output_channels;
    let instance_rc = std::rc::Rc::new(RefCell::new(vst3_instance));

    let filter_config = crate::vst3::filter::FilterConfig {
        instance_id,
        display_name: display_name.to_string(),
        audio_inputs,
        audio_outputs,
    };

    match crate::vst3::filter::Vst3FilterNode::new(
        core,
        filter_config,
        instance_rc.clone(),
        event_tx.clone(),
    ) {
        Ok(filter) => {
            vst3_instances.borrow_mut().insert(instance_id, instance_rc);
            vst3_filters.borrow_mut().insert(instance_id, filter);

            log::info!(
                "VST3 filter created for instance {}, waiting for node ID...",
                instance_id
            );
        }
        Err(e) => {
            let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: format!("Failed to create VST3 filter node: {}", e),
                fatal: true,
            }));
        }
    }
}

fn build_plugin_info(
    world: &lilv::World,
    plugin: &lilv::plugin::Plugin,
) -> Option<crate::lv2::Lv2PluginInfo> {
    let uri = plugin.uri().as_uri()?.to_string();
    let name = plugin.name().as_str()?.to_string();
    let category = crate::lv2::Lv2PluginCategory::from_class_label(
        plugin.class().label().as_str().unwrap_or("Plugin"),
    );
    let author = plugin
        .author_name()
        .and_then(|n| n.as_str().map(String::from));

    let classification = crate::lv2::scanner::classify_lv2_ports(world, plugin)?;

    let required_features: Vec<String> = plugin
        .required_features()
        .iter()
        .filter_map(|n| n.as_uri().map(String::from))
        .collect();

    Some(crate::lv2::Lv2PluginInfo {
        uri,
        name,
        category,
        author,
        ports: classification.ports,
        audio_inputs: classification.audio_inputs,
        audio_outputs: classification.audio_outputs,
        control_inputs: classification.control_inputs,
        control_outputs: classification.control_outputs,
        required_features,
        compatible: true,
        has_ui: false,
        format: crate::lv2::PluginFormat::Lv2,
        library_path: String::new(),
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
    lv2_handle: *mut std::ffi::c_void,
    extension_data_fn: Option<unsafe extern "C" fn(*const std::os::raw::c_char) -> *const std::ffi::c_void>,
) {
    crate::lv2::ui::open_plugin_ui(
        plugin_uri,
        instance_id,
        cmd_tx.clone(),
        event_tx.clone(),
        control_values,
        port_updates,
        urid_mapper,
        lv2_handle,
        extension_data_fn,
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

// ---------------------------------------------------------------------------
// MIDI mapping helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_add_midi_mapping(
    midi_mapping_table: &Rc<RefCell<crate::midi::MidiMappingTable>>,
    lv2_instances: &GlobalSharedMutHashMap<u64, crate::lv2::host::Lv2PluginInstance>,
    lv2_filters: &Rc<RefCell<HashMap<u64, crate::lv2::filter::Lv2FilterNode>>>,
    clap_instances: &GlobalSharedMutHashMap<u64, crate::clap::host::ClapPluginInstance>,
    clap_filters: &Rc<RefCell<HashMap<u64, crate::clap::filter::ClapFilterNode>>>,
    vst3_instances: &GlobalSharedMutHashMap<u64, crate::vst3::host::Vst3PluginInstance>,
    vst3_filters: &Rc<RefCell<HashMap<u64, crate::vst3::filter::Vst3FilterNode>>>,
    event_tx: &Sender<PwEvent>,
    mapping: crate::midi::MidiCcMapping,
) {
    let existing = midi_mapping_table
        .borrow()
        .get(&mapping.source)
        .cloned();

    if let Some(ref existing_mapping) = existing {
        if existing_mapping.target == mapping.target {
            return;
        }
        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::MidiMappingConflict {
            source: mapping.source.clone(),
            existing_label: existing_mapping.label.clone(),
        }));
        return;
    }

    {
        let old_source = midi_mapping_table
            .borrow()
            .find_by_target(&mapping.target)
            .map(|m| m.source.clone());
        if let Some(old_source) = old_source {
            midi_mapping_table.borrow_mut().remove(&old_source);
        }
    }

    midi_mapping_table.borrow_mut().insert(mapping.clone());

    rebuild_resolved_mappings(
        midi_mapping_table,
        lv2_instances,
        lv2_filters,
        clap_instances,
        clap_filters,
        vst3_instances,
        vst3_filters,
    );

    let _ = event_tx.send(PwEvent::Plugin(PluginEvent::MidiMappingAdded(mapping)));
}

#[allow(clippy::too_many_arguments)]
fn rebuild_resolved_mappings(
    midi_mapping_table: &Rc<RefCell<crate::midi::MidiMappingTable>>,
    lv2_instances: &GlobalSharedMutHashMap<u64, crate::lv2::host::Lv2PluginInstance>,
    lv2_filters: &Rc<RefCell<HashMap<u64, crate::lv2::filter::Lv2FilterNode>>>,
    clap_instances: &GlobalSharedMutHashMap<u64, crate::clap::host::ClapPluginInstance>,
    clap_filters: &Rc<RefCell<HashMap<u64, crate::clap::filter::ClapFilterNode>>>,
    vst3_instances: &GlobalSharedMutHashMap<u64, crate::vst3::host::Vst3PluginInstance>,
    vst3_filters: &Rc<RefCell<HashMap<u64, crate::vst3::filter::Vst3FilterNode>>>,
) {
    let table = midi_mapping_table.borrow();

    let mut per_instance: HashMap<u64, Vec<crate::midi::filter::ResolvedMappingEntry>> =
        HashMap::new();

    for mapping in table.all_mappings() {
        let instance_id = mapping.target.instance_id;
        let port_index = mapping.target.port_index;

        let entry = resolve_mapping_entry(
            mapping,
            instance_id,
            port_index,
            lv2_instances,
            clap_instances,
            vst3_instances,
        );

        if let Some(entry) = entry {
            per_instance.entry(instance_id).or_default().push(entry);
        }
    }

    let lv2_f = lv2_filters.borrow();
    for (id, filter) in lv2_f.iter() {
        let entries = per_instance.remove(id).unwrap_or_default();
        let resolved = Arc::new(crate::midi::filter::ResolvedMappings::new(entries));
        filter.update_mappings(resolved);
    }

    let clap_f = clap_filters.borrow();
    for (id, filter) in clap_f.iter() {
        let entries = per_instance.remove(id).unwrap_or_default();
        let resolved = Arc::new(crate::midi::filter::ResolvedMappings::new(entries));
        filter.update_mappings(resolved);
    }

    let vst3_f = vst3_filters.borrow();
    for (id, filter) in vst3_f.iter() {
        let entries = per_instance.remove(id).unwrap_or_default();
        let resolved = Arc::new(crate::midi::filter::ResolvedMappings::new(entries));
        filter.update_mappings(resolved);
    }
}

fn detect_logarithmic(min: f32, max: f32) -> bool {
    min > 0.0 && max / min > 100.0
}

fn resolve_mapping_entry(
    mapping: &crate::midi::MidiCcMapping,
    instance_id: u64,
    port_index: usize,
    lv2_instances: &GlobalSharedMutHashMap<u64, crate::lv2::host::Lv2PluginInstance>,
    clap_instances: &GlobalSharedMutHashMap<u64, crate::clap::host::ClapPluginInstance>,
    vst3_instances: &GlobalSharedMutHashMap<u64, crate::vst3::host::Vst3PluginInstance>,
) -> Option<crate::midi::filter::ResolvedMappingEntry> {
    if let Some(inst_rc) = lv2_instances.borrow().get(&instance_id) {
        let inst = inst_rc.borrow();
        let port_updates = inst.port_updates.clone();

        if let Some(cp) = inst.control_inputs.iter().find(|cp| cp.index == port_index) {
            return Some(crate::midi::filter::ResolvedMappingEntry {
                port_updates,
                port_index,
                instance_id,
                min: cp.min,
                max: cp.max,
                mode: mapping.mode,
                source: mapping.source.clone(),
                is_logarithmic: detect_logarithmic(cp.min, cp.max),
                is_toggle: cp.is_toggle,
            });
        }
        return None;
    }

    if let Some(inst_rc) = clap_instances.borrow().get(&instance_id) {
        let inst = inst_rc.borrow();
        let port_updates = inst.port_updates.clone();

        if let Some(p) = inst.params.iter().find(|p| p.port_index == port_index) {
            return Some(crate::midi::filter::ResolvedMappingEntry {
                port_updates,
                port_index,
                instance_id,
                min: p.min as f32,
                max: p.max as f32,
                mode: mapping.mode,
                source: mapping.source.clone(),
                is_logarithmic: detect_logarithmic(p.min as f32, p.max as f32),
                is_toggle: p.is_toggle,
            });
        }
        return None;
    }

    if let Some(inst_rc) = vst3_instances.borrow().get(&instance_id) {
        let inst = inst_rc.borrow();
        let port_updates = inst.port_updates.clone();

        if let Some(p) = inst.params.iter().find(|p| p.port_index == port_index) {
            return Some(crate::midi::filter::ResolvedMappingEntry {
                port_updates,
                port_index,
                instance_id,
                min: 0.0,
                max: 1.0,
                mode: mapping.mode,
                source: mapping.source.clone(),
                is_logarithmic: false,
                is_toggle: p.is_toggle,
            });
        }
        return None;
    }

    log::warn!(
        "MIDI mapping target instance {} not found, skipping",
        instance_id
    );
    None
}
