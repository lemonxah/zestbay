use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libspa::utils::dict::DictRef;
use pipewire::{
    context::ContextRc, core::CoreRc, link::Link as PwLink, main_loop::MainLoopRc,
    registry::GlobalObject, registry::RegistryRc, types::ObjectType,
};

use super::state::GraphState;
use super::types::*;

type StateEntry = crate::lv2::state::StateEntry;
type InstanceMap<V> = RefCell<HashMap<u64, Rc<RefCell<V>>>>;

/// Backoff between reconnection attempts after the PipeWire connection is lost.
const RECONNECT_MIN: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(8);
/// How long a reloaded/reconnected plugin's links are retried by port name.
const RELINK_TIMEOUT_RELOAD: Duration = Duration::from_secs(10);
const RELINK_TIMEOUT_RECONNECT: Duration = Duration::from_secs(30);
/// Sandbox probes run in parallel on worker threads; instantiation stays in order.
const MAX_PARALLEL_PROBES: usize = 4;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Minimum time between two sample-rate reloads of the same instance.
const MIN_RELOAD_INTERVAL: Duration = Duration::from_secs(5);
/// A plugin that needs a layout-changing restart again this soon after being
/// (re)loaded counts as a rapid repeat.
const RESTART_LOOP_WINDOW: Duration = Duration::from_secs(10);
/// Rapid restart reloads allowed before the plugin is left silenced.
const MAX_RAPID_RESTART_RELOADS: u32 = 2;
/// How often parameter changes made inside plugins are reported.
const PARAM_REPORT_INTERVAL: Duration = Duration::from_millis(100);

enum InternalOp {
    Connect {
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    },
    /// Destroy `link_id`, but only if it still connects the same two ports
    /// (PipeWire reuses object IDs).
    Disconnect {
        link_id: ObjectId,
        output_port_id: ObjectId,
        input_port_id: ObjectId,
    },
    AddPlugin(Box<AddRequest>),
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

enum ProbeStatus {
    Pending,
    Running(Receiver<crate::plugin::sandbox::ProbeOutcome>),
    Done(crate::plugin::sandbox::ProbeOutcome),
}

/// Everything needed to (re-)create a plugin instance.
struct AddRequest {
    plugin_uri: String,
    instance_id: u64,
    display_name: String,
    format: String,
    stable_id: String,
    lv2_state: Vec<StateEntry>,
    plugin_state: Option<Vec<u8>>,
    /// Parameter values applied after state restore (reload / reconnect).
    params: Vec<(usize, f32)>,
    bypassed: bool,
    /// Rate to instantiate at; None = current graph rate.
    sample_rate: Option<u32>,
    probe: ProbeStatus,
}

impl AddRequest {
    /// Poll the sandbox probe. Returns true once a verdict is available.
    fn probe_finished(&mut self) -> bool {
        match &self.probe {
            ProbeStatus::Done(_) => true,
            ProbeStatus::Pending => false,
            ProbeStatus::Running(rx) => match rx.try_recv() {
                Ok(outcome) => {
                    self.probe = ProbeStatus::Done(outcome);
                    true
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => false,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Probe worker died; fail open like exec_probe does.
                    self.probe = ProbeStatus::Done(crate::plugin::sandbox::ProbeOutcome::Ok);
                    true
                }
            },
        }
    }
}

/// Detects plugins that need a layout-changing restart (and so a reload)
/// again right after every reload.
#[derive(Default)]
struct RestartLoopGuard {
    last_reload: Option<Instant>,
    rapid: u32,
}

impl RestartLoopGuard {
    /// A restart-triggered reload is wanted at `now`.  Returns false when the
    /// plugin keeps needing one right after being reloaded.
    fn allow(&mut self, now: Instant) -> bool {
        match self.last_reload {
            Some(t) if now.duration_since(t) < RESTART_LOOP_WINDOW => self.rapid += 1,
            _ => self.rapid = 0,
        }
        if self.rapid > MAX_RAPID_RESTART_RELOADS {
            return false;
        }
        self.last_reload = Some(now);
        true
    }
}

/// An LV2 instance whose in-process UI did not confirm closing in time. Its
/// filter is gone; the instance stays alive until the UI is destroyed.
struct Lv2Zombie {
    _instance: Rc<RefCell<crate::lv2::host::Lv2PluginInstance>>,
    pending: crate::lv2::ui::PendingUiClose,
}

#[derive(Clone)]
struct PluginMeta {
    plugin_uri: String,
    display_name: String,
    format: String,
    stable_id: String,
}

/// One side of a link to re-create by name after a plugin reload/reconnect.
#[derive(Clone)]
struct RelinkEnd {
    /// Plugin endpoint: resolved through the instance's current node.
    instance_id: Option<u64>,
    /// Last known node id (tried first for non-plugin endpoints).
    node_id: ObjectId,
    node_name: String,
    port_name: String,
}

#[derive(Clone)]
struct PendingRelink {
    output: RelinkEnd,
    input: RelinkEnd,
    deadline: Instant,
}

/// State that survives a PipeWire reconnect.
#[derive(Default)]
struct Carryover {
    midi_mappings: crate::midi::MidiMappingTable,
    plugins: Vec<AddRequest>,
    relinks: Vec<PendingRelink>,
    lv2_zombies: Vec<Lv2Zombie>,
}

/// Routes UI commands to the currently running PipeWire loop. While the
/// connection is down, commands that still make sense later are deferred.
#[derive(Default)]
struct Router {
    tx: Option<pipewire::channel::Sender<PwCommand>>,
    deferred: Vec<PwCommand>,
}

pub fn start(
    graph: Arc<GraphState>,
    tick_interval_ms: u64,
    operation_cooldown_ms: u64,
) -> (Receiver<PwEvent>, Sender<PwCommand>) {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<PwCommand>();

    let router = Arc::new(Mutex::new(Router::default()));

    std::thread::spawn({
        let router = router.clone();
        let event_tx = event_tx.clone();
        move || {
            while let Ok(cmd) = cmd_rx.recv() {
                let mut r = router.lock().unwrap_or_else(|e| e.into_inner());
                let cmd = match r.tx.as_ref() {
                    Some(tx) => match tx.send(cmd) {
                        Ok(()) => continue,
                        Err(cmd) => cmd,
                    },
                    None => cmd,
                };
                match cmd {
                    PwCommand::RequestPluginStates => {
                        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginStatesDone));
                    }
                    PwCommand::Connect { .. }
                    | PwCommand::Disconnect { .. }
                    | PwCommand::OpenPluginUI { .. }
                    | PwCommand::ClosePluginUI { .. }
                    | PwCommand::StartMidiLearn { .. }
                    | PwCommand::CancelMidiLearn => {
                        log::debug!("PipeWire disconnected; dropping {:?}", cmd);
                    }
                    other => r.deferred.push(other),
                }
            }
        }
    });

    // The tick also pumps CLAP/VST3 GUIs (this thread is their main thread),
    // so keep it short enough for plugin windows to stay responsive.
    let tick = tick_interval_ms.clamp(1, 20);
    let cooldown = operation_cooldown_ms.max(1);
    let cmd_tx_for_pw = cmd_tx.clone();

    std::thread::spawn(move || {
        pipewire::init();
        let urid_mapper = Arc::new(crate::lv2::urid::UridMapper::new());
        let mut carry = Carryover::default();
        let mut backoff = RECONNECT_MIN;
        let mut outage_reported = false;

        loop {
            let mut connected = false;
            let result = run_pipewire_thread(
                &graph,
                &event_tx,
                &cmd_tx_for_pw,
                &router,
                &urid_mapper,
                &mut carry,
                tick,
                cooldown,
                &mut connected,
            );
            router.lock().unwrap_or_else(|e| e.into_inner()).tx = None;

            let msg = match result {
                Ok(()) => "PipeWire main loop exited".to_string(),
                Err(e) => e,
            };
            if connected {
                outage_reported = false;
                backoff = RECONNECT_MIN;
                clear_graph(&graph, &event_tx);
            }
            if !outage_reported {
                log::error!("PipeWire: {} — reconnecting", msg);
                let event = PwEvent::Error(format!("{} — reconnecting to PipeWire…", msg));
                if event_tx.send(event).is_err() {
                    break;
                }
                outage_reported = true;
            } else {
                log::debug!("PipeWire reconnect failed: {}", msg);
            }

            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(RECONNECT_MAX);
        }
    });

    (event_rx, cmd_tx)
}

/// Forget every object of a dead connection and tell the UI.
fn clear_graph(graph: &GraphState, event_tx: &Sender<PwEvent>) {
    for link in graph.get_all_links() {
        let _ = event_tx.send(PwEvent::LinkRemoved(link.id));
    }
    for node in graph.get_all_nodes() {
        for port in graph.get_ports_for_node(node.id) {
            let _ = event_tx.send(PwEvent::PortRemoved {
                port_id: port.id,
                node_id: port.node_id,
            });
        }
        let _ = event_tx.send(PwEvent::NodeRemoved(node.id));
    }
    graph.clear();
    let _ = event_tx.send(PwEvent::BatchComplete);
}

#[allow(clippy::too_many_arguments)]
fn run_pipewire_thread(
    graph: &Arc<GraphState>,
    event_tx: &Sender<PwEvent>,
    cmd_tx: &Sender<PwCommand>,
    router: &Arc<Mutex<Router>>,
    urid_mapper: &Arc<crate::lv2::urid::UridMapper>,
    carry: &mut Carryover,
    tick_interval_ms: u64,
    operation_cooldown_ms: u64,
    connected: &mut bool,
) -> Result<(), String> {
    let mainloop = MainLoopRc::new(None).map_err(|e| e.to_string())?;
    let context = ContextRc::new(&mainloop, None).map_err(|e| e.to_string())?;
    let core = context
        .connect_rc(None)
        .map_err(|e| format!("Cannot connect to PipeWire: {}", e))?;
    let registry = core.get_registry_rc().map_err(|e| e.to_string())?;
    *connected = true;

    let host = Rc::new(Host {
        graph: graph.clone(),
        core: core.clone(),
        registry: registry.clone(),
        event_tx: event_tx.clone(),
        cmd_tx: cmd_tx.clone(),
        urid_mapper: urid_mapper.clone(),
        // Config defaults until the core info / process clock tell us more.
        default_rate: Cell::new(48000),
        live_rate: Cell::new(0),
        quantum: Cell::new(1024),
        operation_cooldown: Duration::from_millis(operation_cooldown_ms),
        lv2_instances: RefCell::new(HashMap::new()),
        lv2_filters: RefCell::new(HashMap::new()),
        clap_instances: RefCell::new(HashMap::new()),
        clap_filters: RefCell::new(HashMap::new()),
        vst3_instances: RefCell::new(HashMap::new()),
        vst3_filters: RefCell::new(HashMap::new()),
        meta: RefCell::new(HashMap::new()),
        midi_mapping_table: RefCell::new(std::mem::take(&mut carry.midi_mappings)),
        pending_ops: RefCell::new(VecDeque::new()),
        relinks: RefCell::new(Vec::new()),
        last_reload: RefCell::new(HashMap::new()),
        restart_guards: RefCell::new(HashMap::new()),
        silenced: RefCell::new(HashSet::new()),
        lv2_zombies: RefCell::new(std::mem::take(&mut carry.lv2_zombies)),
        reported_params: RefCell::new(HashMap::new()),
        last_param_report: Cell::new(Instant::now()),
        last_op_time: Cell::new(Instant::now() - Duration::from_secs(1)),
        changes_pending: Cell::new(false),
    });

    let lost: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    let _core_listener = {
        let host = host.clone();
        let mainloop = mainloop.clone();
        let lost = lost.clone();
        let router = router.clone();
        core.add_listener_local()
            .info(move |info| {
                if let Some(props) = info.props() {
                    if let Some(rate) = props
                        .get("default.clock.rate")
                        .and_then(|s| s.parse::<u32>().ok())
                        && host.default_rate.replace(rate) != rate
                    {
                        log::info!("PipeWire default sample rate: {} Hz", rate);
                    }
                    if let Some(q) = props
                        .get("default.clock.quantum")
                        .and_then(|s| s.parse::<u32>().ok())
                        && host.quantum.replace(q) != q
                    {
                        log::info!("PipeWire default quantum: {} frames", q);
                    }
                }
            })
            .error(move |id, seq, res, message| {
                if id == pipewire::core::PW_ID_CORE && res == -libc::EPIPE {
                    log::error!("PipeWire connection lost: {}", message);
                    // Stop routing commands into this loop before it stops.
                    router.lock().unwrap_or_else(|e| e.into_inner()).tx = None;
                    *lost.borrow_mut() = Some(format!("PipeWire connection lost ({})", message));
                    mainloop.quit();
                } else {
                    log::warn!(
                        "PipeWire error: id={} seq={} res={} {}",
                        id,
                        seq,
                        res,
                        message
                    );
                }
            })
            .register()
    };

    let _registry_listener = {
        let host_g = host.clone();
        let host_r = host.clone();
        registry
            .add_listener_local()
            .global(move |global| host_g.on_global(global))
            .global_remove(move |id| host_r.on_global_remove(id))
            .register()
    };

    let (pw_cmd_tx, pw_cmd_rx) = pipewire::channel::channel::<PwCommand>();
    let _cmd_receiver = pw_cmd_rx.attach(mainloop.loop_(), {
        let host = host.clone();
        move |cmd| host.handle_command(cmd)
    });

    // Re-create what the previous connection had, then replay commands that
    // arrived while disconnected (in order).
    {
        let mut ops = host.pending_ops.borrow_mut();
        for req in carry.plugins.drain(..) {
            ops.push_back(InternalOp::AddPlugin(Box::new(req)));
        }
    }
    {
        let deadline = Instant::now() + RELINK_TIMEOUT_RECONNECT;
        let mut relinks = host.relinks.borrow_mut();
        for mut r in carry.relinks.drain(..) {
            r.deadline = deadline;
            relinks.push(r);
        }
    }
    {
        let mut r = router.lock().unwrap_or_else(|e| e.into_inner());
        for cmd in std::mem::take(&mut r.deferred) {
            let _ = pw_cmd_tx.send(cmd);
        }
        r.tx = Some(pw_cmd_tx);
    }

    let timer = mainloop.loop_().add_timer({
        let host = host.clone();
        move |_| host.tick()
    });
    let _ = timer.update_timer(
        Some(Duration::from_millis(tick_interval_ms)),
        Some(Duration::from_millis(tick_interval_ms)),
    );

    log::info!("PipeWire thread connected");
    mainloop.run();

    let _ = timer.update_timer(None, None);
    let (plugins, relinks) = host.teardown_all();
    carry.plugins = plugins;
    carry.relinks = relinks;
    carry.midi_mappings = host.midi_mapping_table.take();
    carry.lv2_zombies = host.lv2_zombies.take();

    Err(lost
        .borrow_mut()
        .take()
        .unwrap_or_else(|| "PipeWire main loop stopped".to_string()))
}

/// All state owned by the PipeWire thread for one connection.
struct Host {
    graph: Arc<GraphState>,
    core: CoreRc,
    registry: RegistryRc,
    event_tx: Sender<PwEvent>,
    cmd_tx: Sender<PwCommand>,
    urid_mapper: Arc<crate::lv2::urid::UridMapper>,
    /// `default.clock.rate` from the core properties (config default).
    default_rate: Cell<u32>,
    /// Rate actually observed on a running filter's process clock (0 = unknown).
    live_rate: Cell<u32>,
    /// `default.clock.quantum`, passed to LV2 as the nominal block length.
    quantum: Cell<u32>,
    operation_cooldown: Duration,

    lv2_instances: InstanceMap<crate::lv2::host::Lv2PluginInstance>,
    lv2_filters: RefCell<HashMap<u64, crate::lv2::filter::Lv2FilterNode>>,
    clap_instances: InstanceMap<crate::clap::host::ClapPluginInstance>,
    clap_filters: RefCell<HashMap<u64, crate::clap::filter::ClapFilterNode>>,
    vst3_instances: InstanceMap<crate::vst3::host::Vst3PluginInstance>,
    vst3_filters: RefCell<HashMap<u64, crate::vst3::filter::Vst3FilterNode>>,
    meta: RefCell<HashMap<u64, PluginMeta>>,

    midi_mapping_table: RefCell<crate::midi::MidiMappingTable>,

    pending_ops: RefCell<VecDeque<InternalOp>>,
    relinks: RefCell<Vec<PendingRelink>>,
    last_reload: RefCell<HashMap<u64, Instant>>,
    restart_guards: RefCell<HashMap<u64, RestartLoopGuard>>,
    /// Kept parked (silent) after restart-reloading too often.
    silenced: RefCell<HashSet<u64>>,
    lv2_zombies: RefCell<Vec<Lv2Zombie>>,
    /// Parameter values last reported to the UI, per instance.
    reported_params: RefCell<HashMap<u64, Vec<(usize, f32)>>>,
    last_param_report: Cell<Instant>,
    last_op_time: Cell<Instant>,
    changes_pending: Cell<bool>,
}

impl Host {
    fn current_rate(&self) -> u32 {
        match self.live_rate.get() {
            0 => self.default_rate.get(),
            r => r,
        }
    }

    // -----------------------------------------------------------------------
    // Registry
    // -----------------------------------------------------------------------

    fn on_global(&self, global: &GlobalObject<&DictRef>) {
        match global.type_ {
            ObjectType::Node => {
                if let Some(node) = parse_node(global) {
                    // PluginAdded is sent once per instantiation by the
                    // plugin's filter (drain_events), not from here.
                    self.graph.insert_node(node.clone());
                    let _ = self.event_tx.send(PwEvent::NodeChanged(node));
                    self.changes_pending.set(true);
                }
            }
            ObjectType::Port => {
                if let Some(port) = parse_port(global, &self.graph) {
                    log::debug!(
                        "Port registered: id={} node={} name={:?} dir={:?}",
                        port.id,
                        port.node_id,
                        port.name,
                        port.direction
                    );
                    self.graph.insert_port(port.clone());
                    let _ = self.event_tx.send(PwEvent::PortChanged(port));
                    self.changes_pending.set(true);
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
                    self.graph.insert_link(link.clone());
                    let _ = self.event_tx.send(PwEvent::LinkChanged(link));
                    self.changes_pending.set(true);
                }
            }
            _ => {}
        }
    }

    fn on_global_remove(&self, id: ObjectId) {
        // PipeWire destroys links together with their ports/nodes, so links
        // dropped here are only forgotten — never destroyed by us (the ID may
        // already belong to a new object).
        if self.graph.remove_node(id).is_some() {
            for link_id in self.graph.cleanup_node(id) {
                let _ = self.event_tx.send(PwEvent::LinkRemoved(link_id));
            }
            let _ = self.event_tx.send(PwEvent::NodeRemoved(id));
            self.changes_pending.set(true);
        } else if let Some(port) = self.graph.remove_port(id) {
            for link_id in self.graph.cleanup_port(id) {
                let _ = self.event_tx.send(PwEvent::LinkRemoved(link_id));
            }
            let _ = self.event_tx.send(PwEvent::PortRemoved {
                port_id: id,
                node_id: port.node_id,
            });
            self.changes_pending.set(true);
        } else if self.graph.remove_link(id).is_some() {
            let _ = self.event_tx.send(PwEvent::LinkRemoved(id));
            self.changes_pending.set(true);
        }
    }

    // -----------------------------------------------------------------------
    // Commands
    // -----------------------------------------------------------------------

    fn handle_command(&self, cmd: PwCommand) {
        match cmd {
            PwCommand::SetPluginParameter {
                instance_id,
                port_index,
                value,
            } => {
                // The clamped value the slot now holds.
                let applied: Option<Option<f32>> =
                    if let Some(inst) = self.lv2_instances.borrow().get(&instance_id) {
                        let inst = inst.borrow_mut();
                        inst.set_parameter(port_index, value);
                        Some(slot_value(&inst.port_updates, port_index))
                    } else if let Some(inst) = self.clap_instances.borrow().get(&instance_id) {
                        let inst = inst.borrow_mut();
                        inst.set_parameter(port_index, value);
                        Some(slot_value(&inst.port_updates, port_index))
                    } else if let Some(inst) = self.vst3_instances.borrow().get(&instance_id) {
                        let inst = inst.borrow_mut();
                        inst.set_parameter(port_index, value);
                        Some(slot_value(&inst.port_updates, port_index))
                    } else {
                        None
                    };
                match applied {
                    Some(Some(value)) => {
                        if let Some(prev) = self.reported_params.borrow_mut().get_mut(&instance_id) {
                            upsert_param(prev, port_index, value);
                        }
                        let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::ParameterChanged {
                            instance_id,
                            port_index,
                            value,
                        }));
                    }
                    Some(None) => {}
                    // Not instantiated yet (e.g. replayed after a reconnect):
                    // apply it when the queued add runs.
                    None => {
                        self.patch_queued_add(instance_id, |req| {
                            upsert_param(&mut req.params, port_index, value)
                        });
                    }
                }
            }
            PwCommand::SetPluginBypass {
                instance_id,
                bypassed,
            } => {
                if let Some(inst) = self.lv2_instances.borrow().get(&instance_id) {
                    inst.borrow_mut().set_bypassed(bypassed);
                } else if let Some(inst) = self.clap_instances.borrow().get(&instance_id) {
                    inst.borrow_mut().set_bypassed(bypassed);
                } else if let Some(inst) = self.vst3_instances.borrow().get(&instance_id) {
                    inst.borrow_mut().set_bypassed(bypassed);
                } else {
                    self.patch_queued_add(instance_id, |req| req.bypassed = bypassed);
                }
            }
            PwCommand::StartMidiLearn {
                instance_id,
                port_index,
                ..
            } => {
                // Only one filter learns at a time.
                self.set_learn_mode_all(false);
                if let Some(filter) = self.lv2_filters.borrow().get(&instance_id) {
                    filter.set_learn_mode(true);
                } else if let Some(filter) = self.clap_filters.borrow().get(&instance_id) {
                    filter.set_learn_mode(true);
                } else if let Some(filter) = self.vst3_filters.borrow().get(&instance_id) {
                    filter.set_learn_mode(true);
                }
                let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::MidiLearnStarted {
                    instance_id,
                    port_index,
                }));
            }
            PwCommand::CancelMidiLearn => {
                self.set_learn_mode_all(false);
                let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::MidiLearnCancelled));
            }
            PwCommand::AddMidiMapping(mapping) => self.add_midi_mapping(mapping),
            PwCommand::RemoveMidiMapping(source) => {
                let removed = self.midi_mapping_table.borrow_mut().remove(&source);
                if removed.is_some() {
                    self.rebuild_resolved_mappings();
                    let _ = self
                        .event_tx
                        .send(PwEvent::Plugin(PluginEvent::MidiMappingRemoved(source)));
                }
            }
            PwCommand::RemoveMidiMappingsForPlugin { instance_id } => {
                self.remove_midi_mappings_where(|m| m.target.instance_id == instance_id);
            }
            PwCommand::RemoveMidiMappingsForDevice { device_name } => {
                // Learned mappings store the name reported with the captured
                // CC (the plugin node's display name), so match on that.
                let n = self.remove_midi_mappings_where(|m| m.source.device_name == device_name);
                log::info!("Removed {} MIDI mapping(s) for device '{}'", n, device_name);
            }
            PwCommand::RequestPluginStates => self.send_plugin_states(),
            PwCommand::Connect {
                output_port_id,
                input_port_id,
            } => self.queue_op(InternalOp::Connect {
                output_port_id,
                input_port_id,
            }),
            PwCommand::Disconnect { link_id } => match self.graph.get_link(link_id) {
                Some(link) => self.queue_op(InternalOp::Disconnect {
                    link_id,
                    output_port_id: link.output_port_id,
                    input_port_id: link.input_port_id,
                }),
                None => log::debug!("Disconnect: link {} is not in the graph, ignoring", link_id),
            },
            PwCommand::AddPlugin {
                plugin_uri,
                instance_id,
                display_name,
                format,
                lv2_state,
                plugin_state,
                stable_id,
            } => self.queue_op(InternalOp::AddPlugin(Box::new(AddRequest {
                plugin_uri,
                instance_id,
                display_name,
                format,
                stable_id,
                lv2_state,
                plugin_state,
                params: Vec::new(),
                bypassed: false,
                sample_rate: None,
                probe: if crate::NO_PROBE.load(std::sync::atomic::Ordering::SeqCst) {
                    ProbeStatus::Done(crate::plugin::sandbox::ProbeOutcome::Ok)
                } else {
                    ProbeStatus::Pending
                },
            }))),
            PwCommand::RemovePlugin { instance_id } => {
                self.queue_op(InternalOp::RemovePlugin { instance_id })
            }
            PwCommand::OpenPluginUI { instance_id } => {
                self.queue_op(InternalOp::OpenPluginUI { instance_id })
            }
            PwCommand::ClosePluginUI { instance_id } => {
                self.queue_op(InternalOp::ClosePluginUI { instance_id })
            }
        }
    }

    fn queue_op(&self, op: InternalOp) {
        self.pending_ops.borrow_mut().push_back(op);
    }

    fn set_learn_mode_all(&self, enabled: bool) {
        for filter in self.lv2_filters.borrow().values() {
            filter.set_learn_mode(enabled);
        }
        for filter in self.clap_filters.borrow().values() {
            filter.set_learn_mode(enabled);
        }
        for filter in self.vst3_filters.borrow().values() {
            filter.set_learn_mode(enabled);
        }
    }

    /// Apply `f` to the queued (not yet instantiated) add of `instance_id`,
    /// if there is one.
    fn patch_queued_add(&self, instance_id: u64, f: impl FnOnce(&mut AddRequest)) {
        let mut ops = self.pending_ops.borrow_mut();
        // The last queued add is the one that ends up live.
        let queued = ops.iter_mut().rev().find_map(|op| match op {
            InternalOp::AddPlugin(req) if req.instance_id == instance_id => Some(req),
            _ => None,
        });
        match queued {
            Some(req) => f(req),
            None => log::debug!("Instance {} is not loaded; command ignored", instance_id),
        }
    }

    /// The plugin is not loaded this session; the UI keeps its saved
    /// configuration.  Sends `PluginSkipped` + a non-fatal `PluginError`.
    fn skip_plugin(&self, instance_id: u64, reason: &str) {
        let _ = self
            .event_tx
            .send(PwEvent::Plugin(PluginEvent::PluginSkipped { instance_id }));
        let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
            instance_id: Some(instance_id),
            message: format!(
                "{}. It was skipped for this session; its saved settings are kept.",
                reason
            ),
            fatal: false,
        }));
    }

    fn handle_op(&self, op: InternalOp) {
        match op {
            InternalOp::Connect {
                output_port_id,
                input_port_id,
            } => create_link(&self.graph, &self.core, output_port_id, input_port_id),
            InternalOp::Disconnect {
                link_id,
                output_port_id,
                input_port_id,
            } => match self.graph.get_link(link_id) {
                Some(l) if l.output_port_id == output_port_id && l.input_port_id == input_port_id => {
                    self.registry.destroy_global(link_id);
                }
                Some(_) => log::warn!(
                    "Not destroying link {}: the ID now refers to a different link",
                    link_id
                ),
                None => log::debug!("Link {} already gone", link_id),
            },
            InternalOp::AddPlugin(req) => {
                if let ProbeStatus::Done(outcome) = &req.probe
                    && !outcome.is_safe()
                {
                    log::error!(
                        "{} plugin '{}' ({}) {} — skipping",
                        req.format,
                        req.display_name,
                        req.plugin_uri,
                        outcome.describe()
                    );
                    if matches!(outcome, crate::plugin::sandbox::ProbeOutcome::Crashed { .. }) {
                        let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                            instance_id: Some(req.instance_id),
                            message: format!(
                                "Plugin '{}' {}. It has been blocked to protect ZestBay.",
                                req.display_name,
                                outcome.describe()
                            ),
                            fatal: true,
                        }));
                    } else {
                        // Timeouts and load failures may be transient.
                        self.skip_plugin(req.instance_id, &format!("It {}", outcome.describe()));
                    }
                    return;
                }
                self.add_plugin(&req);
            }
            InternalOp::RemovePlugin { instance_id } => {
                self.remove_plugin(instance_id);
                self.restart_guards.borrow_mut().remove(&instance_id);
                self.silenced.borrow_mut().remove(&instance_id);
                self.relinks.borrow_mut().retain(|r| {
                    r.output.instance_id != Some(instance_id)
                        && r.input.instance_id != Some(instance_id)
                });
                let _ = self
                    .event_tx
                    .send(PwEvent::Plugin(PluginEvent::PluginRemoved { instance_id }));
            }
            InternalOp::OpenPluginUI { instance_id } => self.open_plugin_ui(instance_id),
            InternalOp::ClosePluginUI { instance_id } => {
                // The instance stays alive, so an unconfirmed close is harmless.
                let _ = crate::lv2::ui::close_plugin_ui(instance_id);
                crate::clap::ui::close_clap_gui(instance_id, &self.event_tx);
                crate::vst3::ui::close_vst3_gui(instance_id, &self.event_tx);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Timer
    // -----------------------------------------------------------------------

    fn tick(&self) {
        let now = Instant::now();

        self.drain_filter_events();
        self.report_param_changes(now);
        self.release_lv2_zombies();
        self.check_sample_rates(now);

        // Link and UI operations are cheap: run them right away instead of
        // queueing them behind plugin adds (and their probes).  A UI op for
        // an instance with an add/remove still queued keeps its place.
        let quick_ops: Vec<InternalOp> = {
            let mut ops = self.pending_ops.borrow_mut();
            let mut quick = Vec::new();
            let mut rest = VecDeque::with_capacity(ops.len());
            let mut busy: HashSet<u64> = HashSet::new();
            for op in ops.drain(..) {
                match op {
                    InternalOp::Connect { .. } | InternalOp::Disconnect { .. } => quick.push(op),
                    InternalOp::OpenPluginUI { instance_id }
                    | InternalOp::ClosePluginUI { instance_id }
                        if !busy.contains(&instance_id) =>
                    {
                        quick.push(op)
                    }
                    InternalOp::AddPlugin(ref req) => {
                        busy.insert(req.instance_id);
                        rest.push_back(op);
                    }
                    InternalOp::RemovePlugin { instance_id } => {
                        busy.insert(instance_id);
                        rest.push_back(op);
                    }
                    other => rest.push_back(other),
                }
            }
            *ops = rest;
            quick
        };
        for op in quick_ops {
            self.handle_op(op);
        }

        self.start_probes();
        self.resolve_relinks(now);

        // Plugin operations: one per cooldown, strictly in queue order. An add
        // whose sandbox probe is still running holds back everything after it.
        if now.duration_since(self.last_op_time.get()) >= self.operation_cooldown {
            let ready = match self.pending_ops.borrow_mut().front_mut() {
                None => false,
                Some(InternalOp::AddPlugin(req)) => req.probe_finished(),
                Some(_) => true,
            };
            if ready {
                let op = self.pending_ops.borrow_mut().pop_front();
                if let Some(op) = op {
                    self.last_op_time.set(now);
                    self.handle_op(op);
                }
            }
        }

        self.pump_plugin_hosts(now);

        if self.changes_pending.replace(false) {
            let _ = self.event_tx.send(PwEvent::BatchComplete);
        }
    }

    /// CLAP/VST3 main-thread work: GUI timers/fds/X11 events, plugin
    /// callbacks, controller sync and restart requests. This thread is the
    /// plugins' main thread, so this must run on every tick.
    fn pump_plugin_hosts(&self, now: Instant) {
        crate::clap::ui::idle();
        crate::vst3::ui::idle();

        // A failed restart parks the instance (audio thread excluded) until
        // it is dropped, so a parked instance is reloaded on every tick
        // until that succeeds (or the loop guard gives up).
        let mut parked: Vec<u64> = Vec::new();
        let clap: Vec<(u64, Rc<RefCell<crate::clap::host::ClapPluginInstance>>)> = self
            .clap_instances
            .borrow()
            .iter()
            .map(|(id, inst)| (*id, inst.clone()))
            .collect();
        for (id, inst) in clap {
            let mut i = inst.borrow_mut();
            i.idle();
            if i.needs_restart() {
                i.restart();
            }
            if i.is_parked() {
                parked.push(id);
            }
        }
        let vst3: Vec<(u64, Rc<RefCell<crate::vst3::host::Vst3PluginInstance>>)> = self
            .vst3_instances
            .borrow()
            .iter()
            .map(|(id, inst)| (*id, inst.clone()))
            .collect();
        for (id, inst) in vst3 {
            let mut i = inst.borrow_mut();
            i.idle();
            if i.needs_restart() {
                i.restart();
            }
            if i.is_parked() {
                parked.push(id);
            }
        }

        for id in parked {
            self.reload_parked(id, now);
        }
    }

    /// Reload an instance a failed restart left parked.  Not throttled (it
    /// is silent until reloaded), but plugins that keep needing it right
    /// after every reload are left silenced.
    fn reload_parked(&self, instance_id: u64, now: Instant) {
        if self.silenced.borrow().contains(&instance_id) {
            return;
        }
        let allowed = self
            .restart_guards
            .borrow_mut()
            .entry(instance_id)
            .or_default()
            .allow(now);
        if !allowed {
            self.silenced.borrow_mut().insert(instance_id);
            let name = self
                .meta
                .borrow()
                .get(&instance_id)
                .map(|m| m.display_name.clone())
                .unwrap_or_default();
            log::error!(
                "'{}' (instance {}) keeps changing its audio layout after reloads; leaving it silenced",
                name,
                instance_id
            );
            let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::PluginError {
                instance_id: Some(instance_id),
                message: "It keeps changing its audio layout right after being reloaded. \
                          It has been silenced; remove and re-add it to try again."
                    .to_string(),
                fatal: false,
            }));
            return;
        }
        log::info!("Instance {} needs a reload after a failed restart", instance_id);
        self.reload_plugin(instance_id, self.current_rate(), now);
    }

    /// Report parameter changes made inside plugins (GUI edits, output
    /// params, rescans, MIDI mappings) that no command echoed.
    fn report_param_changes(&self, now: Instant) {
        if now.duration_since(self.last_param_report.get()) < PARAM_REPORT_INTERVAL {
            return;
        }
        self.last_param_report.set(now);

        let mut current: Vec<(u64, Vec<(usize, f32)>)> = Vec::new();
        for (id, inst) in self.lv2_instances.borrow().iter() {
            current.push((*id, slot_values(&inst.borrow().port_updates)));
        }
        for (id, inst) in self.clap_instances.borrow().iter() {
            current.push((*id, slot_values(&inst.borrow().port_updates)));
        }
        for (id, inst) in self.vst3_instances.borrow().iter() {
            current.push((*id, slot_values(&inst.borrow().port_updates)));
        }

        let mut reported = self.reported_params.borrow_mut();
        for (instance_id, values) in current {
            let Some(prev) = reported.get_mut(&instance_id) else {
                // New instance: its values so far are the baseline.
                reported.insert(instance_id, values);
                continue;
            };
            for (port_index, value) in diff_params(prev, &values) {
                let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::ParameterChanged {
                    instance_id,
                    port_index,
                    value,
                }));
            }
        }
    }

    /// Free LV2 instances whose late UI close has been acknowledged.
    fn release_lv2_zombies(&self) {
        if self.lv2_zombies.borrow().is_empty() {
            return;
        }
        let zombies = std::mem::take(&mut *self.lv2_zombies.borrow_mut());
        let (done, keep): (Vec<Lv2Zombie>, Vec<Lv2Zombie>) =
            zombies.into_iter().partition(|z| z.pending.is_done());
        self.lv2_zombies.borrow_mut().extend(keep);
        if !done.is_empty() {
            log::info!("Freeing {} LV2 instance(s) whose UI has now closed", done.len());
        }
        drop(done);
    }

    fn drain_filter_events(&self) {
        for filter in self.lv2_filters.borrow().values() {
            filter.drain_events(&self.event_tx);
        }
        for filter in self.clap_filters.borrow().values() {
            filter.drain_events(&self.event_tx);
        }
        for filter in self.vst3_filters.borrow().values() {
            filter.drain_events(&self.event_tx);
        }
    }

    /// Start sandbox probes for queued adds (on worker threads so the
    /// PipeWire loop keeps running).
    fn start_probes(&self) {
        let mut ops = self.pending_ops.borrow_mut();
        let mut running = ops
            .iter()
            .filter(|op| matches!(op, InternalOp::AddPlugin(r) if matches!(r.probe, ProbeStatus::Running(_))))
            .count();
        let rate = self.current_rate() as f64;
        let quantum = self.quantum.get();
        for op in ops.iter_mut() {
            if running >= MAX_PARALLEL_PROBES {
                break;
            }
            let InternalOp::AddPlugin(req) = op else {
                continue;
            };
            if !matches!(req.probe, ProbeStatus::Pending) {
                continue;
            }
            let (fmt, block) = match req.format.as_str() {
                "CLAP" => ("clap", 0),
                "VST3" => ("vst3", 0),
                _ => ("lv2", quantum),
            };
            let uri = req.plugin_uri.clone();
            // CLAP/VST3: hand the probe the library path so the child doesn't scan.
            let lib = match fmt {
                "clap" => crate::clap::scanner::find_plugin(&uri).map(|i| i.library_path),
                "vst3" => crate::vst3::scanner::find_plugin(&uri).map(|i| i.library_path),
                _ => None,
            };
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("zestbay-probe".into())
                .spawn(move || {
                    let outcome = crate::plugin::sandbox::exec_probe_outcome(
                        fmt,
                        &uri,
                        lib.as_deref(),
                        rate,
                        block,
                        Some(PROBE_TIMEOUT),
                    );
                    let _ = tx.send(outcome);
                });
            req.probe = match spawned {
                Ok(_) => {
                    running += 1;
                    ProbeStatus::Running(rx)
                }
                Err(e) => {
                    log::error!("Failed to spawn probe thread: {}", e);
                    ProbeStatus::Done(crate::plugin::sandbox::ProbeOutcome::Ok)
                }
            };
        }
    }

    // -----------------------------------------------------------------------
    // Plugin lifecycle
    // -----------------------------------------------------------------------

    /// Instantiate `req`.  On failure the plugin is skipped for this session
    /// (non-fatal) and false is returned.
    fn add_plugin(&self, req: &AddRequest) -> bool {
        let sr = req.sample_rate.unwrap_or_else(|| self.current_rate()) as f64;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match req.format.as_str() {
                "CLAP" => self.instantiate_clap(req, sr),
                "VST3" => self.instantiate_vst3(req, sr),
                _ => self.instantiate_lv2(req, sr),
            }
        }));

        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(msg)) => Some(msg),
            Err(panic_info) => {
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                Some(format!("Plugin panicked during instantiation: {}", panic_msg))
            }
        };
        if let Some(message) = error {
            log::error!(
                "{} plugin '{}' ({}): {}",
                req.format,
                req.display_name,
                req.plugin_uri,
                message
            );
            self.skip_plugin(req.instance_id, &message);
            return false;
        }

        self.meta.borrow_mut().insert(
            req.instance_id,
            PluginMeta {
                plugin_uri: req.plugin_uri.clone(),
                display_name: req.display_name.clone(),
                format: req.format.clone(),
                stable_id: req.stable_id.clone(),
            },
        );
        // New filters start without resolved mappings; mappings restored
        // before the plugin existed take effect now.
        self.rebuild_resolved_mappings();
        log::info!(
            "{} filter created for instance {} at {} Hz, waiting for node ID...",
            req.format,
            req.instance_id,
            sr
        );
        true
    }

    fn instantiate_lv2(&self, req: &AddRequest, sr: f64) -> Result<(), String> {
        // One loaded world per thread, reused (reloaded only on a miss).
        let (world, lilv_plugin) = crate::lv2::find_in_thread_world(&req.plugin_uri)
            .ok_or_else(|| format!("Plugin not found: {}", req.plugin_uri))?;
        let plugin_info = build_plugin_info(&world, &lilv_plugin)
            .ok_or_else(|| format!("Failed to parse plugin info: {}", req.plugin_uri))?;

        let mut inst = unsafe {
            crate::lv2::host::Lv2PluginInstance::new(
                &world,
                &lilv_plugin,
                &plugin_info,
                sr,
                self.quantum.get(),
                &self.urid_mapper,
            )
        }
        .ok_or_else(|| format!("Failed to instantiate plugin: {}", req.plugin_uri))?;

        if !req.stable_id.is_empty() {
            inst.set_state_scope(&req.stable_id);
        }
        if !req.lv2_state.is_empty() && inst.has_state_interface() {
            log::info!(
                "LV2 state: restoring {} entries for '{}'",
                req.lv2_state.len(),
                req.display_name
            );
            unsafe {
                inst.restore_state(&req.lv2_state);
            }
        }
        for &(port_index, value) in &req.params {
            inst.set_parameter(port_index, value);
        }
        if req.bypassed {
            inst.set_bypassed(true);
        }

        let has_midi_in = plugin_info
            .ports
            .iter()
            .any(|p| p.port_type == crate::plugin::types::PluginPortType::AtomInput);
        let has_midi_out = plugin_info
            .ports
            .iter()
            .any(|p| p.port_type == crate::plugin::types::PluginPortType::AtomOutput);
        let instance_rc = Rc::new(RefCell::new(inst));

        let filter_config = crate::lv2::filter::FilterConfig {
            instance_id: req.instance_id,
            display_name: req.display_name.clone(),
            audio_inputs: plugin_info.audio_inputs,
            audio_outputs: plugin_info.audio_outputs,
            sample_rate: sr as u32,
            has_midi_in,
            has_midi_out,
        };
        let filter = crate::lv2::filter::Lv2FilterNode::new(
            &self.core,
            filter_config,
            instance_rc.clone(),
            self.event_tx.clone(),
        )
        .map_err(|e| format!("Failed to create filter node: {}", e))?;

        self.lv2_instances
            .borrow_mut()
            .insert(req.instance_id, instance_rc);
        self.lv2_filters.borrow_mut().insert(req.instance_id, filter);
        Ok(())
    }

    fn instantiate_clap(&self, req: &AddRequest, sr: f64) -> Result<(), String> {
        let info = crate::clap::scanner::find_plugin(&req.plugin_uri)
            .ok_or_else(|| format!("CLAP plugin not found: {}", req.plugin_uri))?;
        // The saved state is loaded before activation (state-dependent ports).
        let inst = unsafe {
            crate::clap::host::ClapPluginInstance::new(
                &info.library_path,
                &req.plugin_uri,
                &info,
                sr,
                req.plugin_state.as_deref(),
            )
        }
        .ok_or_else(|| format!("Failed to instantiate CLAP plugin: {}", req.plugin_uri))?;

        for &(port_index, value) in &req.params {
            inst.set_parameter(port_index, value);
        }
        if req.bypassed {
            inst.set_bypassed(true);
        }

        let filter_config = crate::clap::filter::FilterConfig {
            instance_id: req.instance_id,
            display_name: req.display_name.clone(),
            audio_inputs: inst.audio_input_channels,
            audio_outputs: inst.audio_output_channels,
            has_midi_in: inst.has_midi_in,
            has_midi_out: inst.has_midi_out,
        };
        let instance_rc = Rc::new(RefCell::new(inst));
        let filter = crate::clap::filter::ClapFilterNode::new(
            &self.core,
            filter_config,
            instance_rc.clone(),
            self.event_tx.clone(),
        )
        .map_err(|e| format!("Failed to create CLAP filter node: {}", e))?;

        self.clap_instances
            .borrow_mut()
            .insert(req.instance_id, instance_rc);
        self.clap_filters.borrow_mut().insert(req.instance_id, filter);
        Ok(())
    }

    fn instantiate_vst3(&self, req: &AddRequest, sr: f64) -> Result<(), String> {
        let info = crate::vst3::scanner::find_plugin(&req.plugin_uri)
            .ok_or_else(|| format!("VST3 plugin not found: {}", req.plugin_uri))?;
        // The saved state is restored before setup/activation.
        let inst = unsafe {
            crate::vst3::host::Vst3PluginInstance::new(
                &info.library_path,
                &req.plugin_uri,
                &info,
                sr,
                req.plugin_state.as_deref(),
            )
        }
        .ok_or_else(|| format!("Failed to instantiate VST3 plugin: {}", req.plugin_uri))?;

        for &(port_index, value) in &req.params {
            inst.set_parameter(port_index, value);
        }
        if req.bypassed {
            inst.set_bypassed(true);
        }

        let filter_config = crate::vst3::filter::FilterConfig {
            instance_id: req.instance_id,
            display_name: req.display_name.clone(),
            audio_inputs: inst.audio_input_channels,
            audio_outputs: inst.audio_output_channels,
            has_midi_in: inst.has_midi_in,
            has_midi_out: inst.has_midi_out,
        };
        let instance_rc = Rc::new(RefCell::new(inst));
        let filter = crate::vst3::filter::Vst3FilterNode::new(
            &self.core,
            filter_config,
            instance_rc.clone(),
            self.event_tx.clone(),
        )
        .map_err(|e| format!("Failed to create VST3 filter node: {}", e))?;

        self.vst3_instances
            .borrow_mut()
            .insert(req.instance_id, instance_rc);
        self.vst3_filters.borrow_mut().insert(req.instance_id, filter);
        Ok(())
    }

    /// Tear down one plugin: close its UI (blocks until closed), destroy the
    /// filter (stops processing), then drop the instance. Emits nothing.
    fn remove_plugin(&self, instance_id: u64) -> bool {
        let found = if self.lv2_instances.borrow().contains_key(&instance_id) {
            let ui = crate::lv2::ui::close_plugin_ui(instance_id);
            let filter = self.lv2_filters.borrow_mut().remove(&instance_id);
            drop(filter);
            let inst = self.lv2_instances.borrow_mut().remove(&instance_id);
            match (ui, inst) {
                // An in-process UI may still use the instance: keep it alive
                // until the GTK thread confirms the close.
                (crate::lv2::ui::UiCloseStatus::Pending(pending), Some(instance)) => {
                    log::warn!(
                        "Keeping LV2 instance {} alive until its UI has closed",
                        instance_id
                    );
                    self.lv2_zombies.borrow_mut().push(Lv2Zombie {
                        _instance: instance,
                        pending,
                    });
                }
                (_, inst) => drop(inst),
            }
            true
        } else if self.clap_instances.borrow().contains_key(&instance_id) {
            crate::clap::ui::close_clap_gui(instance_id, &self.event_tx);
            let filter = self.clap_filters.borrow_mut().remove(&instance_id);
            drop(filter);
            let inst = self.clap_instances.borrow_mut().remove(&instance_id);
            drop(inst);
            true
        } else if self.vst3_instances.borrow().contains_key(&instance_id) {
            crate::vst3::ui::close_vst3_gui(instance_id, &self.event_tx);
            let filter = self.vst3_filters.borrow_mut().remove(&instance_id);
            drop(filter);
            let inst = self.vst3_instances.borrow_mut().remove(&instance_id);
            drop(inst);
            true
        } else {
            false
        };
        self.meta.borrow_mut().remove(&instance_id);
        self.reported_params.borrow_mut().remove(&instance_id);
        found
    }

    /// Capture everything needed to re-create a live plugin as it is now.
    fn snapshot_plugin(&self, instance_id: u64) -> Option<AddRequest> {
        let meta = self.meta.borrow().get(&instance_id).cloned()?;
        let (lv2_state, plugin_state, params, bypassed) =
            if let Some(inst) = self.lv2_instances.borrow().get(&instance_id) {
                let inst = inst.borrow();
                let state = if inst.has_state_interface() {
                    unsafe { inst.save_state() }.unwrap_or_default()
                } else {
                    Vec::new()
                };
                let params = inst
                    .get_parameters()
                    .iter()
                    .map(|p| (p.port_index, p.value))
                    .collect();
                (state, None, params, inst.is_bypassed())
            } else if let Some(inst) = self.clap_instances.borrow().get(&instance_id) {
                let inst = inst.borrow();
                let params = inst
                    .get_parameters()
                    .iter()
                    .map(|p| (p.port_index, p.value))
                    .collect();
                (Vec::new(), inst.get_state(), params, inst.is_bypassed())
            } else if let Some(inst) = self.vst3_instances.borrow().get(&instance_id) {
                let inst = inst.borrow();
                let params = inst
                    .get_parameters()
                    .iter()
                    .map(|p| (p.port_index, p.value))
                    .collect();
                (Vec::new(), inst.get_state(), params, inst.is_bypassed())
            } else {
                return None;
            };

        Some(AddRequest {
            plugin_uri: meta.plugin_uri,
            instance_id,
            display_name: meta.display_name,
            format: meta.format,
            stable_id: meta.stable_id,
            lv2_state,
            plugin_state,
            params,
            bypassed,
            sample_rate: None,
            // Already proven safe in this process.
            probe: ProbeStatus::Done(crate::plugin::sandbox::ProbeOutcome::Ok),
        })
    }

    fn plugin_node_id(&self, instance_id: u64) -> Option<ObjectId> {
        let id = if let Some(f) = self.lv2_filters.borrow().get(&instance_id) {
            f.node_id()
        } else if let Some(f) = self.clap_filters.borrow().get(&instance_id) {
            f.node_id()
        } else if let Some(f) = self.vst3_filters.borrow().get(&instance_id) {
            f.node_id()
        } else {
            return None;
        };
        (id != 0 && id != u32::MAX).then_some(id)
    }

    fn instance_for_node(&self, node_id: ObjectId) -> Option<u64> {
        let ids: Vec<u64> = self.meta.borrow().keys().copied().collect();
        ids.into_iter()
            .find(|&id| self.plugin_node_id(id) == Some(node_id))
    }

    /// Links currently attached to the plugin's node, as name-based relinks.
    fn plugin_links(
        &self,
        instance_id: u64,
        keep_node_ids: bool,
        deadline: Instant,
    ) -> Vec<(ObjectId, PendingRelink)> {
        let Some(node_id) = self.plugin_node_id(instance_id) else {
            return Vec::new();
        };
        let end = |node_id: ObjectId, port_id: ObjectId| -> Option<RelinkEnd> {
            let node = self.graph.get_node(node_id)?;
            let port = self.graph.get_port(port_id)?;
            Some(RelinkEnd {
                instance_id: self.instance_for_node(node_id),
                node_id: if keep_node_ids { node_id } else { u32::MAX },
                node_name: node.name,
                port_name: port.name,
            })
        };
        self.graph
            .get_all_links()
            .into_iter()
            .filter(|l| l.output_node_id == node_id || l.input_node_id == node_id)
            .filter_map(|l| {
                Some((
                    l.id,
                    PendingRelink {
                        output: end(l.output_node_id, l.output_port_id)?,
                        input: end(l.input_node_id, l.input_port_id)?,
                        deadline,
                    },
                ))
            })
            .collect()
    }

    fn resolve_end(&self, end: &RelinkEnd, direction: PortDirection) -> Option<ObjectId> {
        let node_id = match end.instance_id {
            Some(iid) => self.plugin_node_id(iid)?,
            None => match self.graph.get_node(end.node_id) {
                Some(n) if n.name == end.node_name => n.id,
                _ => {
                    self.graph
                        .get_all_nodes()
                        .into_iter()
                        .find(|n| n.name == end.node_name)?
                        .id
                }
            },
        };
        let ports = match direction {
            PortDirection::Output => self.graph.get_output_ports(node_id),
            PortDirection::Input => self.graph.get_input_ports(node_id),
        };
        ports
            .into_iter()
            .find(|p| p.name == end.port_name)
            .map(|p| p.id)
    }

    fn resolve_relinks(&self, now: Instant) {
        if self.relinks.borrow().is_empty() {
            return;
        }
        let relinks = std::mem::take(&mut *self.relinks.borrow_mut());
        let mut keep = Vec::new();
        for r in relinks {
            let out = self.resolve_end(&r.output, PortDirection::Output);
            let inp = self.resolve_end(&r.input, PortDirection::Input);
            match (out, inp) {
                (Some(out), Some(inp)) => {
                    if self.graph.find_link(out, inp).is_none() {
                        log::info!(
                            "Re-linking {}:{} -> {}:{}",
                            r.output.node_name,
                            r.output.port_name,
                            r.input.node_name,
                            r.input.port_name
                        );
                        create_link(&self.graph, &self.core, out, inp);
                    }
                }
                _ if now >= r.deadline => {
                    log::warn!(
                        "Giving up re-linking {}:{} -> {}:{}",
                        r.output.node_name,
                        r.output.port_name,
                        r.input.node_name,
                        r.input.port_name
                    );
                }
                _ => keep.push(r),
            }
        }
        self.relinks.borrow_mut().extend(keep);
    }

    /// K3: reload plugins whose filter runs at a different rate than the
    /// instance was created with.
    fn check_sample_rates(&self, now: Instant) {
        let mut mismatched: Vec<(u64, u32)> = Vec::new();
        let mut check = |id: u64, graph_rate: u32, instance_rate: f64| {
            if graph_rate == 0 {
                return;
            }
            self.live_rate.set(graph_rate);
            if instance_rate.round() as u32 != graph_rate {
                mismatched.push((id, graph_rate));
            }
        };
        for (id, f) in self.lv2_filters.borrow().iter() {
            if let Some(inst) = self.lv2_instances.borrow().get(id) {
                check(*id, f.graph_rate(), inst.borrow().sample_rate);
            }
        }
        for (id, f) in self.clap_filters.borrow().iter() {
            if let Some(inst) = self.clap_instances.borrow().get(id) {
                check(*id, f.graph_rate(), inst.borrow().sample_rate);
            }
        }
        for (id, f) in self.vst3_filters.borrow().iter() {
            if let Some(inst) = self.vst3_instances.borrow().get(id) {
                check(*id, f.graph_rate(), inst.borrow().sample_rate);
            }
        }

        for (id, rate) in mismatched {
            if let Some(last) = self.last_reload.borrow().get(&id)
                && now.duration_since(*last) < MIN_RELOAD_INTERVAL
            {
                continue;
            }
            self.last_reload.borrow_mut().insert(id, now);
            self.reload_plugin(id, rate, now);
        }
    }

    /// Re-create a plugin in place (same instance id / name / stable id) at
    /// `rate`, restoring state, parameters, bypass and links.
    fn reload_plugin(&self, instance_id: u64, rate: u32, now: Instant) {
        let Some(mut req) = self.snapshot_plugin(instance_id) else {
            return;
        };
        log::info!(
            "Reloading '{}' (instance {}) at {} Hz",
            req.display_name,
            instance_id,
            rate
        );
        req.sample_rate = Some(rate);
        let relinks = self.plugin_links(instance_id, true, now + RELINK_TIMEOUT_RELOAD);

        self.remove_plugin(instance_id);
        if self.add_plugin(&req) {
            self.relinks
                .borrow_mut()
                .extend(relinks.into_iter().map(|(_, r)| r));
        }
    }

    /// Tear everything down (connection lost). Returns what must be
    /// re-created on the next connection.
    fn teardown_all(&self) -> (Vec<AddRequest>, Vec<PendingRelink>) {
        let mut ids: Vec<u64> = self.meta.borrow().keys().copied().collect();
        ids.sort_unstable();

        // Queued ops: removals win over snapshots, unstarted adds carry over.
        let mut removed: HashSet<u64> = HashSet::new();
        let mut queued_adds: Vec<AddRequest> = Vec::new();
        for op in self.pending_ops.borrow_mut().drain(..) {
            match op {
                InternalOp::RemovePlugin { instance_id } => {
                    removed.insert(instance_id);
                    queued_adds.retain(|r| r.instance_id != instance_id);
                }
                InternalOp::AddPlugin(mut req) => {
                    if !matches!(req.probe, ProbeStatus::Done(_)) {
                        req.probe = ProbeStatus::Pending;
                    }
                    queued_adds.push(*req);
                }
                _ => {}
            }
        }

        let far = Instant::now() + RELINK_TIMEOUT_RECONNECT;
        let mut seen_links = HashSet::new();
        let mut relinks: Vec<PendingRelink> = Vec::new();
        for &id in &ids {
            if removed.contains(&id) {
                continue;
            }
            for (link_id, r) in self.plugin_links(id, false, far) {
                if seen_links.insert(link_id) {
                    relinks.push(r);
                }
            }
        }
        for mut r in self.relinks.borrow_mut().drain(..) {
            r.output.node_id = u32::MAX;
            r.input.node_id = u32::MAX;
            relinks.push(r);
        }

        let mut plugins: Vec<AddRequest> = ids
            .iter()
            .filter(|id| !removed.contains(id))
            .filter_map(|&id| self.snapshot_plugin(id))
            .collect();
        plugins.extend(queued_adds);

        for id in ids {
            self.remove_plugin(id);
            if removed.contains(&id) {
                let _ = self
                    .event_tx
                    .send(PwEvent::Plugin(PluginEvent::PluginRemoved { instance_id: id }));
            }
        }
        (plugins, relinks)
    }

    /// K2: one `PluginStateSaved` per live plugin, then `PluginStatesDone`.
    fn send_plugin_states(&self) {
        // A failed save sends nothing for that plugin, so the UI keeps the
        // state it already has.
        for (&instance_id, inst) in self.lv2_instances.borrow().iter() {
            let inst = inst.borrow();
            let lv2_state = if inst.has_state_interface() {
                match unsafe { inst.save_state() } {
                    Some(state) => state,
                    None => {
                        log::warn!("LV2 state save failed for instance {}", instance_id);
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
            let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::PluginStateSaved {
                instance_id,
                lv2_state,
                blob: None,
            }));
        }
        let blobs: Vec<(u64, Option<Vec<u8>>)> = self
            .clap_instances
            .borrow()
            .iter()
            .map(|(&id, inst)| (id, inst.borrow().get_state()))
            .chain(
                self.vst3_instances
                    .borrow()
                    .iter()
                    .map(|(&id, inst)| (id, inst.borrow().get_state())),
            )
            .collect();
        for (instance_id, blob) in blobs {
            if blob.is_none() {
                continue;
            }
            let _ = self.event_tx.send(PwEvent::Plugin(PluginEvent::PluginStateSaved {
                instance_id,
                lv2_state: Vec::new(),
                blob,
            }));
        }
        let _ = self
            .event_tx
            .send(PwEvent::Plugin(PluginEvent::PluginStatesDone));
    }

    fn open_plugin_ui(&self, instance_id: u64) {
        if let Some(instance) = self.lv2_instances.borrow().get(&instance_id) {
            let inst = instance.borrow();
            let plugin_uri = inst.plugin_uri.clone();
            let port_updates = inst.port_updates.clone();
            let control_values: Vec<(usize, f32)> = inst
                .get_parameters()
                .iter()
                .map(|p| (p.port_index, p.value))
                .collect();
            let lv2_handle = inst.lv2_handle_ptr();
            let extension_data_fn = inst.extension_data_fn();
            drop(inst);
            crate::lv2::ui::open_plugin_ui(
                &plugin_uri,
                instance_id,
                self.cmd_tx.clone(),
                self.event_tx.clone(),
                control_values,
                port_updates,
                self.urid_mapper.clone(),
                lv2_handle,
                extension_data_fn,
            );
        } else if let Some(instance) = self.clap_instances.borrow().get(&instance_id) {
            let inst = instance.borrow();
            let plugin_ptr = inst.plugin_ptr();
            let display_name = inst.display_name.clone();
            drop(inst);
            unsafe {
                crate::clap::ui::open_clap_gui(
                    plugin_ptr,
                    instance_id,
                    &display_name,
                    &self.event_tx,
                    &self.cmd_tx,
                );
            }
        } else if let Some(instance) = self.vst3_instances.borrow().get(&instance_id) {
            let inst = instance.borrow();
            let controller_ptr = inst.controller_ptr();
            let display_name = inst.display_name.clone();
            drop(inst);
            unsafe {
                crate::vst3::ui::open_vst3_gui(
                    controller_ptr,
                    instance_id,
                    &display_name,
                    &self.event_tx,
                    &self.cmd_tx,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // MIDI mappings
    // -----------------------------------------------------------------------

    fn add_midi_mapping(&self, mapping: crate::midi::MidiCcMapping) {
        let existing = self
            .midi_mapping_table
            .borrow()
            .get(&mapping.source)
            .cloned();
        if let Some(existing_mapping) = existing {
            if existing_mapping.target == mapping.target {
                return;
            }
            let _ = self
                .event_tx
                .send(PwEvent::Plugin(PluginEvent::MidiMappingConflict {
                    source: mapping.source.clone(),
                    existing_label: existing_mapping.label.clone(),
                }));
            return;
        }

        // A target has at most one source: replace (and report) the old one.
        let old_source = self
            .midi_mapping_table
            .borrow()
            .find_by_target(&mapping.target)
            .map(|m| m.source.clone());
        if let Some(old_source) = old_source {
            self.midi_mapping_table.borrow_mut().remove(&old_source);
            let _ = self
                .event_tx
                .send(PwEvent::Plugin(PluginEvent::MidiMappingRemoved(old_source)));
        }

        self.midi_mapping_table.borrow_mut().insert(mapping.clone());
        self.rebuild_resolved_mappings();
        let _ = self
            .event_tx
            .send(PwEvent::Plugin(PluginEvent::MidiMappingAdded(mapping)));
    }

    /// Remove matching mappings, emitting `MidiMappingRemoved` for each.
    fn remove_midi_mappings_where(
        &self,
        pred: impl Fn(&crate::midi::MidiCcMapping) -> bool,
    ) -> usize {
        let removed: Vec<crate::midi::MidiCcSource> = self
            .midi_mapping_table
            .borrow()
            .all_mappings()
            .into_iter()
            .filter(|m| pred(m))
            .map(|m| m.source.clone())
            .collect();
        if removed.is_empty() {
            return 0;
        }
        {
            let mut table = self.midi_mapping_table.borrow_mut();
            for source in &removed {
                table.remove(source);
            }
        }
        self.rebuild_resolved_mappings();
        let n = removed.len();
        for source in removed {
            let _ = self
                .event_tx
                .send(PwEvent::Plugin(PluginEvent::MidiMappingRemoved(source)));
        }
        n
    }

    fn rebuild_resolved_mappings(&self) {
        let table = self.midi_mapping_table.borrow();

        let mut per_instance: HashMap<u64, Vec<crate::midi::filter::ResolvedMappingEntry>> =
            HashMap::new();

        for mapping in table.all_mappings() {
            let instance_id = mapping.target.instance_id;
            if let Some(entry) = self.resolve_mapping_entry(mapping) {
                per_instance.entry(instance_id).or_default().push(entry);
            }
        }

        let mut resolved_for = |id: &u64| {
            Arc::new(crate::midi::filter::ResolvedMappings::new(
                per_instance.remove(id).unwrap_or_default(),
            ))
        };
        for (id, filter) in self.lv2_filters.borrow().iter() {
            filter.update_mappings(resolved_for(id));
        }
        for (id, filter) in self.clap_filters.borrow().iter() {
            filter.update_mappings(resolved_for(id));
        }
        for (id, filter) in self.vst3_filters.borrow().iter() {
            filter.update_mappings(resolved_for(id));
        }
    }

    fn resolve_mapping_entry(
        &self,
        mapping: &crate::midi::MidiCcMapping,
    ) -> Option<crate::midi::filter::ResolvedMappingEntry> {
        let instance_id = mapping.target.instance_id;
        let port_index = mapping.target.port_index;

        if let Some(inst_rc) = self.lv2_instances.borrow().get(&instance_id) {
            let inst = inst_rc.borrow();
            let cp = inst.control_inputs.iter().find(|cp| cp.index == port_index)?;
            return Some(crate::midi::filter::ResolvedMappingEntry {
                port_updates: inst.port_updates.clone(),
                port_index,
                instance_id,
                min: cp.min,
                max: cp.max,
                mode: mapping.mode,
                source: mapping.source.clone(),
                is_logarithmic: cp.is_logarithmic && cp.min > 0.0 && cp.max > cp.min,
                is_toggle: cp.is_toggle,
                step_count: cp.step_count,
            });
        }

        if let Some(inst_rc) = self.clap_instances.borrow().get(&instance_id) {
            let inst = inst_rc.borrow();
            let p = inst.params.iter().find(|p| p.port_index == port_index)?;
            return Some(crate::midi::filter::ResolvedMappingEntry {
                port_updates: inst.port_updates.clone(),
                port_index,
                instance_id,
                min: p.min as f32,
                max: p.max as f32,
                mode: mapping.mode,
                source: mapping.source.clone(),
                // CLAP exposes no scale hint; map linearly.
                is_logarithmic: false,
                is_toggle: p.is_toggle,
                step_count: p.step_count,
            });
        }

        if let Some(inst_rc) = self.vst3_instances.borrow().get(&instance_id) {
            let inst = inst_rc.borrow();
            let p = inst.params.iter().find(|p| p.port_index == port_index)?;
            return Some(crate::midi::filter::ResolvedMappingEntry {
                port_updates: inst.port_updates.clone(),
                port_index,
                instance_id,
                min: 0.0,
                max: 1.0,
                mode: mapping.mode,
                source: mapping.source.clone(),
                is_logarithmic: false,
                is_toggle: p.is_toggle,
                step_count: p.step_count,
            });
        }

        // Normal while restoring: mappings arrive before their plugins.
        log::debug!(
            "MIDI mapping target instance {} not loaded yet",
            instance_id
        );
        None
    }
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

/// Media type of a port from its `format.dsp` (authoritative when present),
/// then its name, then its node's media type.
fn port_media_type(
    format_dsp: Option<&str>,
    name: &str,
    node_media_type: impl FnOnce() -> Option<MediaType>,
) -> Option<MediaType> {
    match format_dsp.map(|s| s.to_ascii_lowercase()) {
        Some(dsp) if dsp.contains("midi") || dsp.contains("ump") => Some(MediaType::Midi),
        Some(dsp) if dsp.contains("audio") => Some(MediaType::Audio),
        _ if name.starts_with("midi_") => Some(MediaType::Midi),
        _ => node_media_type(),
    }
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

    let media_type = port_media_type(props.get("format.dsp"), &name, || {
        graph.get_node(node_id).and_then(|n| n.media_type)
    });
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

/// Current (port index, value) of every control input slot.
fn slot_values(updates: &crate::plugin::types::PortUpdates) -> Vec<(usize, f32)> {
    updates
        .control_inputs
        .iter()
        .map(|s| (s.port_index, s.value.load()))
        .collect()
}

fn slot_value(updates: &crate::plugin::types::PortUpdates, port_index: usize) -> Option<f32> {
    updates
        .control_inputs
        .iter()
        .find(|s| s.port_index == port_index)
        .map(|s| s.value.load())
}

/// Set `port_index` to `value` in a (port, value) list, adding it if absent.
fn upsert_param(params: &mut Vec<(usize, f32)>, port_index: usize, value: f32) {
    match params.iter_mut().find(|(p, _)| *p == port_index) {
        Some(entry) => entry.1 = value,
        None => params.push((port_index, value)),
    }
}

/// Values in `current` that differ from `reported`, which is updated to
/// `current`.  If the port list changed, `reported` is just reset.
fn diff_params(reported: &mut Vec<(usize, f32)>, current: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let same_ports = reported.len() == current.len()
        && reported.iter().zip(current).all(|(a, b)| a.0 == b.0);
    if !same_ports {
        *reported = current.to_vec();
        return Vec::new();
    }
    let mut changed = Vec::new();
    for (prev, &(port_index, value)) in reported.iter_mut().zip(current) {
        if prev.1.to_bits() != value.to_bits() {
            prev.1 = value;
            changed.push((port_index, value));
        }
    }
    changed
}

/// A link from a node to itself is only allowed between different port
/// groups (e.g. two devices behind one MIDI bridge node).
fn is_allowed_same_node_link(output: &Port, input: &Port) -> bool {
    matches!(
        (&output.port_group, &input.port_group),
        (Some(a), Some(b)) if a != b
    )
}

fn create_link(
    graph: &Arc<GraphState>,
    core: &CoreRc,
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

    if output_port.node_id == input_port.node_id
        && !is_allowed_same_node_link(&output_port, &input_port)
    {
        log::warn!(
            "Rejected self-loop: port {} and port {} belong to the same node {}",
            output_port_id,
            input_port_id,
            output_port.node_id
        );
        return;
    }

    // Reject cross-media-type links (e.g. Midi → Audio or Audio → Midi)
    // PipeWire will reject these anyway, but catching them here avoids
    // infinite retry loops from patchbay auto-rules.
    if let (Some(out_mt), Some(in_mt)) = (output_port.media_type, input_port.media_type)
        && out_mt != in_mt
    {
        log::warn!(
            "Rejected media type mismatch: port {} ({:?}, {:?}) -> port {} ({:?}, {:?})",
            output_port_id,
            output_port.name,
            out_mt,
            input_port_id,
            input_port.name,
            in_mt,
        );
        return;
    }

    log::debug!(
        "Creating link: port {} ({:?}, {:?}) -> port {} ({:?}, {:?})",
        output_port_id,
        output_port.name,
        output_port.media_type,
        input_port_id,
        input_port.name,
        input_port.media_type,
    );

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

#[cfg(test)]
mod tests {
    use super::*;

    fn port(group: Option<&str>) -> Port {
        Port {
            id: 1,
            node_id: 1,
            name: "p".into(),
            direction: PortDirection::Output,
            media_type: Some(MediaType::Midi),
            channel: None,
            physical_index: None,
            port_group: group.map(String::from),
            port_alias: None,
        }
    }

    #[test]
    fn restart_loop_guard_gives_up_on_rapid_repeats() {
        let t0 = Instant::now();
        let mut g = RestartLoopGuard::default();
        assert!(g.allow(t0));
        assert!(g.allow(t0 + Duration::from_secs(3)));
        assert!(g.allow(t0 + Duration::from_secs(6)));
        assert!(!g.allow(t0 + Duration::from_secs(9)));
    }

    #[test]
    fn restart_loop_guard_resets_after_a_quiet_window() {
        let t0 = Instant::now();
        let mut g = RestartLoopGuard::default();
        assert!(g.allow(t0));
        assert!(g.allow(t0 + Duration::from_secs(5)));
        assert!(g.allow(t0 + Duration::from_secs(9)));
        // Ran fine for longer than the window: a fresh streak.
        let later = t0 + Duration::from_secs(9) + RESTART_LOOP_WINDOW;
        assert!(g.allow(later));
        assert!(g.allow(later + Duration::from_secs(1)));
        assert!(g.allow(later + Duration::from_secs(2)));
        assert!(!g.allow(later + Duration::from_secs(3)));
    }

    #[test]
    fn diff_params_reports_only_changes() {
        let mut reported = vec![(0, 0.5), (1, 1.0)];
        assert!(diff_params(&mut reported, &[(0, 0.5), (1, 1.0)]).is_empty());
        assert_eq!(diff_params(&mut reported, &[(0, 0.25), (1, 1.0)]), vec![(0, 0.25)]);
        assert_eq!(reported, vec![(0, 0.25), (1, 1.0)]);
        assert!(diff_params(&mut reported, &[(0, 0.25), (1, 1.0)]).is_empty());
    }

    #[test]
    fn diff_params_resets_when_ports_change() {
        let mut reported = vec![(0, 0.5)];
        assert!(diff_params(&mut reported, &[(0, 0.1), (1, 0.2)]).is_empty());
        assert_eq!(reported, vec![(0, 0.1), (1, 0.2)]);
    }

    #[test]
    fn upsert_param_replaces_or_appends() {
        let mut params = vec![(3, 1.0)];
        upsert_param(&mut params, 3, 2.0);
        upsert_param(&mut params, 5, 0.5);
        assert_eq!(params, vec![(3, 2.0), (5, 0.5)]);
    }

    #[test]
    fn same_node_links_need_distinct_port_groups() {
        assert!(!is_allowed_same_node_link(&port(None), &port(None)));
        assert!(!is_allowed_same_node_link(&port(Some("a")), &port(Some("a"))));
        assert!(!is_allowed_same_node_link(&port(Some("a")), &port(None)));
        assert!(is_allowed_same_node_link(&port(Some("a")), &port(Some("b"))));
    }

    #[test]
    fn port_media_type_detection() {
        let none = || None;
        let audio = || Some(MediaType::Audio);
        assert_eq!(
            port_media_type(Some("8 bit raw midi"), "x", audio),
            Some(MediaType::Midi)
        );
        assert_eq!(
            port_media_type(Some("32 bit raw UMP"), "x", audio),
            Some(MediaType::Midi)
        );
        assert_eq!(
            port_media_type(Some("32 bit float mono audio"), "midi_in", none),
            Some(MediaType::Audio)
        );
        assert_eq!(port_media_type(None, "midi_in", audio), Some(MediaType::Midi));
        assert_eq!(port_media_type(None, "in_FL", audio), Some(MediaType::Audio));
        assert_eq!(port_media_type(None, "in_FL", none), None);
    }
}
