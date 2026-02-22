use lilv::World;

use super::types::*;

/// LV2 feature URIs that ZestBay provides to plugins.
/// Plugins requiring only these features (or a subset) are considered compatible.
const PROVIDED_FEATURES: &[&str] = &["http://lv2plug.in/ns/ext/urid#map"];

/// Scan all installed LV2 plugins and return their metadata
pub fn scan_plugins() -> Vec<Lv2PluginInfo> {
    let world = World::with_load_all();
    scan_plugins_with_world(&world)
}

/// Scan plugins using an existing lilv World
pub fn scan_plugins_with_world(world: &World) -> Vec<Lv2PluginInfo> {
    let input_class = world.new_uri("http://lv2plug.in/ns/lv2core#InputPort");
    let output_class = world.new_uri("http://lv2plug.in/ns/lv2core#OutputPort");
    let audio_class = world.new_uri("http://lv2plug.in/ns/lv2core#AudioPort");
    let control_class = world.new_uri("http://lv2plug.in/ns/lv2core#ControlPort");
    let atom_class = world.new_uri("http://lv2plug.in/ns/ext/atom#AtomPort");

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

        // Parse ports
        let mut ports = Vec::new();
        let mut audio_inputs = 0usize;
        let mut audio_outputs = 0usize;
        let mut control_inputs = 0usize;
        let mut control_outputs = 0usize;

        let port_ranges = plugin.port_ranges_float();

        for (i, port_range) in port_ranges.iter().enumerate() {
            let port = match plugin.port_by_index(i) {
                Some(p) => p,
                None => continue,
            };

            let port_symbol = match port.symbol() {
                Some(s) => s.as_str().unwrap_or("").to_string(),
                None => format!("port_{}", i),
            };

            let port_name = match port.name() {
                Some(n) => n.as_str().unwrap_or("").to_string(),
                None => port_symbol.clone(),
            };

            let is_input = port.is_a(&input_class);
            let is_output = port.is_a(&output_class);
            let is_audio = port.is_a(&audio_class);
            let is_control = port.is_a(&control_class);
            let is_atom = port.is_a(&atom_class);

            let port_type = if is_audio && is_input {
                audio_inputs += 1;
                Lv2PortType::AudioInput
            } else if is_audio && is_output {
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
                continue; // Skip unknown port types
            };

            let default_value = port_range.default;
            let min_value = port_range.min;
            let max_value = port_range.max;

            ports.push(Lv2PortInfo {
                index: i,
                symbol: port_symbol,
                name: port_name,
                port_type,
                default_value,
                min_value,
                max_value,
            });
        }

        // Extract required features and check compatibility
        let required_features: Vec<String> = plugin
            .required_features()
            .iter()
            .filter_map(|n| n.as_uri().map(String::from))
            .collect();

        let compatible = required_features
            .iter()
            .all(|req| PROVIDED_FEATURES.iter().any(|provided| provided == req));

        plugins.push(Lv2PluginInfo {
            uri,
            name,
            category,
            author,
            ports,
            audio_inputs,
            audio_outputs,
            control_inputs,
            control_outputs,
            required_features,
            compatible,
        });
    }

    // Sort by name for predictable ordering
    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    plugins
}
