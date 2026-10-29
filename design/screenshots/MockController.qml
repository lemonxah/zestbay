import QtQuick

// Stand-in for the Rust AppController when rendering screenshots
// (design/screenshots/render.sh). Sample data only; nothing touches PipeWire.
QtObject {
    id: mock
    readonly property bool light: Qt.application.arguments.indexOf("light") >= 0

    property bool patchbay_enabled: true
    property bool patchbay_exclusive: true
    property int active_plugin_count: 3
    property int node_count: nodes.length
    property int link_count: links.length
    property string cpu_usage: "1.4%"
    signal graph_changed()
    signal error_occurred(string message)
    signal show_window_requested()
    signal hide_window_requested()
    signal tray_action_requested(string action)
    signal midi_mapping_added(string mapping_json)
    signal midi_mapping_removed(var instance_id, var port_index)
    signal midi_learn_started(var instance_id, var port_index)
    signal midi_learn_cancelled()
    signal midi_mapping_conflict(string source_json, string existing_label)
    signal crash_recovery_available(string crashed_uris)
    signal plugin_parameter_changed(var instance_id, var port_index, real value)

    // ---- Graph ----------------------------------------------------------
    function stereoIn(base) {
        return [{ id: base + 1, name: "playback_FL", direction: "Input", mediaType: "Audio" },
                { id: base + 2, name: "playback_FR", direction: "Input", mediaType: "Audio" }]
    }
    function stereoOut(base, prefix) {
        return [{ id: base + 3, name: prefix + "_FL", direction: "Output", mediaType: "Audio" },
                { id: base + 4, name: prefix + "_FR", direction: "Output", mediaType: "Audio" }]
    }
    property var nodes: [
        { id: 1, name: "Roland Digital Piano", type: "Source", mediaType: "Midi", layoutKey: "MidiBridge:Roland", ready: true },
        { id: 2, name: "Microphone", type: "Source", isVirtual: true, layoutKey: "Source:Microphone", ready: true },
        { id: 3, name: "Headset Microphone", type: "Source", layoutKey: "Source:Headset", ready: true },
        { id: 4, name: "C270 HD Webcam", type: "Source", mediaType: "Video", layoutKey: "Source:Webcam", ready: true },
        { id: 10, name: "Pianoteq 9", type: "Plugin", layoutKey: "LV2:Pianoteq 9", pluginFormat: "LV2", pluginHasUi: true, ready: true },
        { id: 11, name: "LSP Gate Mono", type: "Plugin", layoutKey: "LV2:LSP Gate Mono", pluginFormat: "LV2", pluginHasUi: true, ready: true },
        { id: 12, name: "Firefox", type: "StreamOutput", layoutKey: "StreamOut:Firefox", ready: true },
        { id: 13, name: "Spotify", type: "StreamOutput", layoutKey: "StreamOut:Spotify", ready: true },
        { id: 20, name: "Daw Sink", type: "Sink", isVirtual: true, layoutKey: "Sink:Daw Sink", ready: true },
        { id: 21, name: "Media Sink", type: "Sink", isVirtual: true, layoutKey: "Sink:Media Sink", ready: true },
        { id: 22, name: "Comms Sink", type: "Sink", isVirtual: true, layoutKey: "Sink:Comms Sink", ready: true },
        { id: 30, name: "LSP Mixer x4 Stereo", type: "Plugin", layoutKey: "LV2:LSP Mixer", pluginFormat: "LV2", pluginHasUi: true, ready: true },
        { id: 40, name: "Speakers", type: "Sink", layoutKey: "Sink:Speakers", ready: true },
        { id: 41, name: "Headphones", type: "Sink", layoutKey: "Sink:Headphones", ready: true }
    ]
    property var ports: ({
        1: [{ id: 101, name: "MIDI 1", direction: "Output", mediaType: "Midi" }],
        2: [{ id: 201, name: "capture_MONO", direction: "Output", mediaType: "Audio" }],
        3: [{ id: 301, name: "capture_MONO", direction: "Output", mediaType: "Audio" }],
        4: [{ id: 401, name: "capture_1", direction: "Output", mediaType: "Video" }],
        10: (function() {
            var a = [{ id: 1001, name: "midi_in", direction: "Input", mediaType: "Midi" },
                     { id: 1002, name: "input_0", direction: "Input", mediaType: "Audio" },
                     { id: 1003, name: "midi_out", direction: "Output", mediaType: "Midi" }]
            for (var i = 0; i < 16; i++) a.push({ id: 1010 + i, name: "output_" + i, direction: "Output", mediaType: "Audio" })
            return a })(),
        11: [{ id: 1101, name: "input_0", direction: "Input", mediaType: "Audio" },
             { id: 1102, name: "output_0", direction: "Output", mediaType: "Audio" }],
        12: stereoOut(1200, "output"),
        13: stereoOut(1300, "output"),
        20: stereoIn(2000).concat(stereoOut(2000, "monitor")),
        21: stereoIn(2100).concat(stereoOut(2100, "monitor")),
        22: stereoIn(2200).concat(stereoOut(2200, "monitor")),
        30: (function() {
            var a = [{ id: 3001, name: "midi_in", direction: "Input", mediaType: "Midi" },
                     { id: 3002, name: "midi_out", direction: "Output", mediaType: "Midi" },
                     { id: 3003, name: "output_0", direction: "Output", mediaType: "Audio" },
                     { id: 3004, name: "output_1", direction: "Output", mediaType: "Audio" }]
            for (var i = 0; i < 8; i++) a.push({ id: 3010 + i, name: "input_" + i, direction: "Input", mediaType: "Audio" })
            return a })(),
        40: stereoIn(4000),
        41: stereoIn(4100)
    })
    property var links: (function() {
        var l = [[101, 1001], [1010, 2001], [1011, 2002], [1203, 2101], [1204, 2102], [1303, 2101], [1304, 2102],
                 [201, 1101], [1102, 2201], [1102, 2202], [2003, 3010], [2004, 3011], [2103, 3012], [2104, 3013],
                 [2203, 3014], [2204, 3015], [3003, 4001], [3004, 4002], [301, 3016]]
        var out = []
        for (var i = 0; i < l.length; i++) out.push({ id: 9000 + i, outputPortId: l[i][0], inputPortId: l[i][1], active: true })
        return out })()
    property var layout: ({
        "MidiBridge:Roland": [48, 48], "Source:Microphone": [48, 168], "Source:Headset": [48, 288], "Source:Webcam": [48, 408],
        "LV2:Pianoteq 9": [312, 48], "LV2:LSP Gate Mono": [312, 264], "StreamOut:Firefox": [312, 408], "StreamOut:Spotify": [312, 528],
        "Sink:Daw Sink": [576, 48], "Sink:Media Sink": [576, 192], "Sink:Comms Sink": [576, 336],
        "LV2:LSP Mixer": [840, 96], "Sink:Speakers": [1104, 120], "Sink:Headphones": [1104, 264]
    })

    // ---- Dialog data ----------------------------------------------------
    property var rules: [
        { id: "r1", sourcePattern: "Firefox", sourceType: "App Out", targetPattern: "Media Sink", targetType: "Sink", sourceLabel: "Firefox", targetLabel: "Media Sink", enabled: true, portMappings: [{ outputPort: "output_FL", inputPort: "playback_FL" }, { outputPort: "output_FR", inputPort: "playback_FR" }] },
        { id: "r2", sourcePattern: "Spotify", sourceType: "App Out", targetPattern: "Media Sink", targetType: "Sink", sourceLabel: "Spotify", targetLabel: "Media Sink", enabled: true, portMappings: [{ outputPort: "output_FL", inputPort: "playback_FL" }, { outputPort: "output_FR", inputPort: "playback_FR" }] },
        { id: "r3", sourcePattern: "Pianoteq 9", sourceType: "Plugin", targetPattern: "Daw Sink", targetType: "Sink", sourceLabel: "Pianoteq 9", targetLabel: "Daw Sink", enabled: true, portMappings: [{ outputPort: "output_0", inputPort: "playback_FL" }, { outputPort: "output_1", inputPort: "playback_FR" }] },
        { id: "r4", sourcePattern: "Microphone", sourceType: "Source", targetPattern: "LSP Gate Mono", targetType: "Plugin", sourceLabel: "Microphone", targetLabel: "LSP Gate Mono", enabled: true, portMappings: [{ outputPort: "capture_MONO", inputPort: "input_0" }] },
        { id: "r5", sourcePattern: "Daw Sink", sourceType: "Sink", targetPattern: "LSP Mixer x4 Stereo", targetType: "Plugin", sourceLabel: "Daw Sink", targetLabel: "LSP Mixer x4 Stereo", enabled: true, portMappings: [{ outputPort: "monitor_FL", inputPort: "input_0" }, { outputPort: "monitor_FR", inputPort: "input_1" }] },
        { id: "r6", sourcePattern: "LSP Mixer x4 Stereo", sourceType: "Plugin", targetPattern: "Speakers", targetType: "Sink", sourceLabel: "LSP Mixer x4 Stereo", targetLabel: "Speakers", enabled: true, portMappings: [] },
        { id: "r7", sourcePattern: "Headset Microphone", sourceType: "Source", targetPattern: "Comms Sink", targetType: "Sink", sourceLabel: "Headset Microphone", targetLabel: "Comms Sink", enabled: false, portMappings: [] }
    ]
    property var plugins: [
        { uri: "http://lsp-plug.in/plugins/lv2/mixer_x4_stereo", name: "LSP Mixer x4 Stereo", category: "Mixer", author: "LSP", audioIn: 8, audioOut: 2, controlIn: 34, controlOut: 12, compatible: true, requiredFeatures: [], hasUi: true, format: "LV2" },
        { uri: "http://lsp-plug.in/plugins/lv2/gate_mono", name: "LSP Gate Mono", category: "Dynamics", author: "LSP", audioIn: 1, audioOut: 1, controlIn: 39, controlOut: 8, compatible: true, requiredFeatures: [], hasUi: true, format: "LV2" },
        { uri: "urn:pianoteq9", name: "Pianoteq 9", category: "Instrument", author: "Modartt", audioIn: 0, audioOut: 2, controlIn: 2, controlOut: 0, compatible: true, requiredFeatures: [], hasUi: true, format: "VST3" },
        { uri: "urn:vital", name: "Vital", category: "Synth", author: "Matt Tytel", audioIn: 0, audioOut: 2, controlIn: 0, controlOut: 0, compatible: true, requiredFeatures: [], hasUi: true, format: "CLAP" },
        { uri: "urn:surge", name: "Surge XT Effects", category: "Effects", author: "Surge Synth Team", audioIn: 2, audioOut: 2, controlIn: 0, controlOut: 0, compatible: true, requiredFeatures: [], hasUi: true, format: "CLAP" },
        { uri: "urn:dragonfly", name: "Dragonfly Hall Reverb", category: "Reverb", author: "Michael Willis", audioIn: 2, audioOut: 2, controlIn: 17, controlOut: 0, compatible: true, requiredFeatures: [], hasUi: true, format: "LV2" },
        { uri: "urn:calfexciter", name: "Calf Exciter", category: "Distortion", author: "Calf Studio Gear", audioIn: 2, audioOut: 2, controlIn: 9, controlOut: 3, compatible: true, requiredFeatures: [], hasUi: false, format: "LV2" },
        { uri: "urn:x42eq", name: "x42 Parametric EQ", category: "EQ", author: "Robin Gareus", audioIn: 2, audioOut: 2, controlIn: 30, controlOut: 1, compatible: true, requiredFeatures: [], hasUi: true, format: "LV2" }
    ]
    property var params: ({ instanceId: 30, displayName: "LSP Mixer x4 Stereo", pluginUri: "http://lsp-plug.in/plugins/lv2/mixer_x4_stereo", bypassed: false, parameters: [
        { portIndex: 0, symbol: "mono", name: "Mono output", min: 0, max: 1, default: 0, value: 0, isToggle: true },
        { portIndex: 1, symbol: "g_in", name: "Input gain", min: 0, max: 10, default: 1, value: 1.25, isToggle: false },
        { portIndex: 2, symbol: "bal", name: "Balance", min: -1, max: 1, default: 0, value: -0.2, isToggle: false },
        { portIndex: 3, symbol: "mute_1", name: "Mute channel 1", min: 0, max: 1, default: 0, value: 1, isToggle: true },
        { portIndex: 4, symbol: "g_1", name: "Channel 1 gain", min: 0, max: 10, default: 1, value: 0.8, isToggle: false },
        { portIndex: 5, symbol: "pan_1", name: "Channel 1 pan", min: -1, max: 1, default: 0, value: 0.3, isToggle: false },
        { portIndex: 6, symbol: "g_out", name: "Output gain", min: 0, max: 10, default: 1, value: 1.0, isToggle: false }
    ] })
    property var cpuHist: (function() {
        var a = []
        // 600 samples at the 100 ms poll = the CPU chart's full 60 s.
        for (var i = 0; i < 600; i++) a.push(0.9 + Math.sin(i / 7) * 0.35 + Math.sin(i / 2.3) * 0.12 + (i % 37 === 0 ? 4 : 0))
        return a })()

    // ---- API ------------------------------------------------------------
    property Timer _kick: Timer { interval: 200; running: true; onTriggered: mock.graph_changed() }
    function init() {}
    function poll_events() {}
    function request_quit() { Qt.quit() }
    function get_window_geometry_json() { return JSON.stringify({ width: 1440, height: 860 }) }
    function save_window_geometry(j) {}
    function get_preferences_json() {
        return JSON.stringify({ rule_settle_ms: 500, params_persist_ms: 1000, links_persist_ms: 2000, poll_interval_ms: 100,
                                auto_learn_rules: true, start_minimized: false, close_to_tray: false, pw_tick_interval_ms: 10,
                                pw_operation_cooldown_ms: 50, patchbay_exclusive: true, snap_gap_px: 10, snap_to_grid: true,
                                grid_size_px: 24, theme: light ? "light" : "dark" })
    }
    function get_poll_interval_ms() { return 100 }
    function get_viewport_json() { return JSON.stringify({ panX: 16, panY: 8, zoom: 1 }) }
    function get_layout_json() { return JSON.stringify(layout) }
    function get_default_node() { return "Sink:Speakers" }
    function get_hidden_json() { return "[]" }
    function get_pinned_json() { return JSON.stringify(["LV2:Pianoteq 9", "LV2:LSP Mixer", "Sink:Speakers"]) }
    function get_nodes_json() { return JSON.stringify(nodes) }
    function get_links_json() { return JSON.stringify(links) }
    function get_ports_json(id) { return JSON.stringify(ports[id] || []) }
    function get_rules_json() { return JSON.stringify(rules) }
    function get_node_names_json() { return JSON.stringify(nodes.map(function(n) { return { name: n.name, type: n.type, mediaType: n.mediaType || "Audio" } })) }
    function get_available_plugins_json() { return JSON.stringify(plugins) }
    function get_active_plugins_json() {
        return JSON.stringify([{ instanceId: 30, stableId: "s30", displayName: params.displayName, pluginUri: params.pluginUri, format: "LV2", bypassed: false, parameters: params.parameters }])
    }
    function get_plugin_params_json(nodeId) { return JSON.stringify(params) }
    function get_midi_mappings_json() {
        return JSON.stringify([{ instanceId: 30, portIndex: 1, label: "Input gain", deviceName: "APC MINI", channel: 0, cc: 48, mode: "continuous", pluginName: "LSP Mixer x4 Stereo", paramName: "Input gain" }])
    }
    function get_midi_mapping_for_param_json(a, b) { return a === 30 && b === 1 ? JSON.stringify({ cc: 48, channel: 0, deviceName: "APC MINI" }) : "" }
    function list_rule_backups_json() { return "[]" }
    function get_cpu_history() { return JSON.stringify(cpuHist) }
    function get_plugin_cpu_json() {
        return JSON.stringify([{ id: 10, name: "Pianoteq 9", dspPercent: 1.8 + Math.random() * 0.4, avgUs: 410, lastUs: 380, calls: 100 },
                               { id: 30, name: "LSP Mixer x4 Stereo", dspPercent: 0.09, avgUs: 12, lastUs: 11, calls: 100 },
                               { id: 11, name: "LSP Gate Mono", dspPercent: 0.03, avgUs: 4, lastUs: 4, calls: 100 }])
    }
    function get_app_version() { return "0.8.8" }
    function get_qt_version() { return "6.12.0" }
    function auto_layout(a, b, c) { return "{}" }
    function save_layout(j) {}
    function save_viewport(j) {}
    function save_hidden(j) {}
    function save_pinned(j) {}
    function set_preference(k, v) {}
    function reset_preferences() {}
    function toggle_patchbay(e) { patchbay_enabled = e }
    function toggle_patchbay_exclusive(e) { patchbay_exclusive = e }
    function apply_rules() {}
    function snapshot_rules() {}
    function toggle_rule(id) {}
    function remove_rule(id) {}
    function add_rule() {}
    function backup_rules() {}
    function set_window_visible(v) {}
    function set_default_node(k) {}
}
