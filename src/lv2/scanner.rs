use lilv::World;

use super::types::*;

const PROVIDED_FEATURES: &[&str] = &[
    "http://lv2plug.in/ns/ext/urid#map",
    "http://lv2plug.in/ns/ext/urid#unmap",
    "http://lv2plug.in/ns/ext/worker#schedule",
    "http://lv2plug.in/ns/ext/log#log",
    "http://lv2plug.in/ns/ext/options#options",
    "http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
    "http://lv2plug.in/ns/ext/buf-size#fixedBlockLength",
    "http://lv2plug.in/ns/ext/buf-size#powerOf2BlockLength",
    "http://lv2plug.in/ns/ext/state#interface",
    "http://lv2plug.in/ns/ext/state#makePath",
    "http://lv2plug.in/ns/ext/state#freePath",
    "http://lv2plug.in/ns/ext/state#mapPath",
    "http://lv2plug.in/ns/ext/data-access",
    "http://lv2plug.in/ns/ext/instance-access",
    "http://lv2plug.in/ns/ext/resize-port#resize",
    "http://lv2plug.in/ns/ext/uri-map",
    "http://lv2plug.in/ns/lv2core#hardRTCapable",
    "http://lv2plug.in/ns/lv2core#isLive",
    "http://lv2plug.in/ns/lv2core#inPlaceBroken",
];

pub struct Lv2PortClassification {
    pub ports: Vec<Lv2PortInfo>,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub control_inputs: usize,
    pub control_outputs: usize,
}

pub fn classify_lv2_ports(
    world: &World,
    plugin: &lilv::plugin::Plugin,
) -> Option<Lv2PortClassification> {
    let input_class = world.new_uri("http://lv2plug.in/ns/lv2core#InputPort");
    let output_class = world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
    let audio_class = world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
    let control_class = world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort");
    let atom_class = world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort");
    let cv_class = world.new_uri("http://lv2plug.in/ns/lv2core#CVPort");
    let toggled_prop = world.new_uri("http://lv2plug.in/ns/lv2core#toggled");

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
        let is_cv = port.is_a(&cv_class);

        let port_type = if (is_audio || is_cv) && is_input {
            audio_inputs += 1;
            Lv2PortType::AudioInput
        } else if (is_audio || is_cv) && is_output {
            audio_outputs += 1;
            Lv2PortType::AudioOutput
        } else if is_control && is_input {
            control_inputs += 1;
            Lv2PortType::ControlInput
        } else if is_control && is_output {
            control_outputs += 1;
            Lv2PortType::ControlOutput
        } else if is_atom && is_input {
            Lv2PortType::AtomInput
        } else if is_atom && is_output {
            Lv2PortType::AtomOutput
        } else {
            continue;
        };

        let is_toggle = is_control && is_input && port.has_property(&toggled_prop);

        ports.push(Lv2PortInfo {
            index: i,
            symbol: port_symbol,
            name: port_name,
            port_type,
            default_value: port_range.default,
            min_value: port_range.min,
            max_value: port_range.max,
            is_toggle,
        });
    }

    Some(Lv2PortClassification {
        ports,
        audio_inputs,
        audio_outputs,
        control_inputs,
        control_outputs,
    })
}

pub fn scan_plugins() -> Vec<Lv2PluginInfo> {
    let world = World::with_load_all();
    scan_plugins_with_world(&world)
}

pub fn scan_plugins_with_world(world: &World) -> Vec<Lv2PluginInfo> {
    let ui_gtk3 = world.new_uri("http://lv2plug.in/ns/extensions/ui#Gtk3UI");
    let ui_gtk2 = world.new_uri("http://lv2plug.in/ns/extensions/ui#GtkUI");
    let ui_gtk4 = world.new_uri("http://lv2plug.in/ns/extensions/ui#Gtk4UI");
    let ui_x11 = world.new_uri("http://lv2plug.in/ns/extensions/ui#X11UI");
    let ui_qt5 = world.new_uri("http://lv2plug.in/ns/extensions/ui#Qt5UI");
    let ui_classes = [&ui_gtk3, &ui_x11, &ui_qt5, &ui_gtk2, &ui_gtk4];

    let mut plugins = Vec::new();

    for plugin in world.plugins().iter() {
        if !plugin.verify() {
            continue;
        }

        let uri = match plugin.uri().as_uri() {
            Some(u) => u.to_string(),
            None => continue,
        };

        let name = match plugin.name().as_str() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let category = Lv2PluginCategory::from_class_label(
            plugin.class().label().as_str().unwrap_or("Plugin"),
        );

        let author = plugin
            .author_name()
            .and_then(|n| n.as_str().map(String::from));

        let classification = match classify_lv2_ports(world, &plugin) {
            Some(c) => c,
            None => continue,
        };

        let required_features: Vec<String> = plugin
            .required_features()
            .iter()
            .filter_map(|n| n.as_uri().map(String::from))
            .collect();

        let compatible = required_features
            .iter()
            .all(|req| PROVIDED_FEATURES.iter().any(|provided| provided == req));

        let has_ui = plugin
            .uis()
            .map(|uis| {
                uis.iter()
                    .any(|ui| ui_classes.iter().any(|cls| ui.is_a(cls)))
            })
            .unwrap_or(false);

        plugins.push(Lv2PluginInfo {
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
            compatible,
            has_ui,
            format: PluginFormat::Lv2,
            library_path: String::new(),
        });
    }

    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    plugins
}
