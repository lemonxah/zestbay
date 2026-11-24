import QtQuick
import QtTest
import "../../qml"

TestCase {
    id: testCase
    name: "Graph"
    when: windowShown
    width: 1000
    height: 700
    visible: true

    property var graph

    QtObject {
        id: backend
        signal rules_changed()
        signal session_changing()
        signal session_changed()
        signal subgraphs_changed()
        property bool meter: false
        property string layout: "{}"
        property string hidden: "[]"
        property string pinned: "[]"
        property string viewport: "{}"
        property var commands: []
        function get_nodes_json() {
            return JSON.stringify([
                {id: 1, name: "Source", type: "Source", layoutKey: "Source:source"},
                {id: 2, name: "Effect", type: "Plugin", layoutKey: "Plugin:effect", enabled: true, meterChannels: meter ? 1 : 0},
                {id: 3, name: "Sink", type: "Sink", layoutKey: "Sink:sink"}
            ]);
        }
        function get_ports_json(id) {
            var ports = {
                1: [{id: 11, name: "output_1", direction: "Output", mediaType: "Audio"}],
                2: [{id: 21, name: "input_1", direction: "Input", mediaType: "Audio"},
                    {id: 22, name: "output_1", direction: "Output", mediaType: "Audio"}],
                3: [{id: 31, name: "input_1", direction: "Input", mediaType: "Audio"}]
            };
            return JSON.stringify(ports[id] || []);
        }
        function get_links_json() {
            return JSON.stringify([{id: 100, outputNodeId: 1, outputPortId: 11, inputNodeId: 2, inputPortId: 21}]);
        }
        function get_layout_json() { return layout; }
        function save_layout(value) { layout = value; }
        function get_hidden_json() { return hidden; }
        function save_hidden(value) { hidden = value; }
        function get_pinned_json() { return pinned; }
        function save_pinned(value) { pinned = value; }
        function get_viewport_json() { return viewport; }
        function save_viewport(value) { viewport = value; }
        function meter_levels_json() { return JSON.stringify({2: [0.5]}); }
        function get_docked_meters_json() { return "{}"; }
        function get_node_styles_json() { return "{}"; }
        function get_plugin_presets_all_json() { return "{}"; }
        function get_blocked_pairs_json() { return "[]"; }
        function get_default_node() { return ""; }
        function get_subgraph_view() { return ""; }
        function blocked_reason(outPort, inPort) { return ""; }
        function connect_ports(outPort, inPort) { commands.push(["connect", outPort, inPort]); }
        function disconnect_link(id) { commands.push(["disconnect", id]); }
    }

    Component {
        id: graphComponent
        GraphView {
            controller: backend
            width: testCase.width
            height: testCase.height
        }
    }

    function init() {
        failOnWarning(/.*/);
        backend.meter = false;
        backend.layout = JSON.stringify({"Source:source": [60, 120], "Plugin:effect": [380, 120], "Sink:sink": [720, 120]});
        backend.hidden = "[]";
        backend.pinned = "[]";
        backend.viewport = "{}";
        backend.commands = [];
        graph = createTemporaryObject(graphComponent, testCase);
        verify(graph !== null);
        graph.refreshData();
        tryVerify(function() { return Object.keys(graph.portPositions).length === 4; });
    }

    function test_render_and_hit_testing() {
        compare(graph.nodes.length, 3);
        var node = graph.nodePositions[2];
        var screen = graph.toScreen(node.x + 20, node.y + 10);
        compare(graph.findNodeAt(screen.x, screen.y), 2);
        var pin = graph.portPositions[11];
        screen = graph.toScreen(pin.cx, pin.cy);
        compare(graph.findPortAt(screen.x, screen.y), 11);
        graph.updateHover(screen.x, screen.y);
        compare(graph.hoveredPortId, 11);
        verify(graph.elements.hScrollBar !== null);
        verify(graph.elements.vScrollBar !== null);
    }

    function test_move_undo_redo() {
        var from = {2: {x: graph.nodePositions[2].x, y: graph.nodePositions[2].y}};
        graph.nodePositions[2] = {x: 420, y: 260};
        verify(graph.recordMove("move", from) !== null);
        verify(graph.canUndo);
        graph.undo();
        compare(graph.nodePositions[2].x, from[2].x);
        compare(graph.nodePositions[2].y, from[2].y);
        verify(graph.canRedo);
        graph.redo();
        compare(graph.nodePositions[2].x, 420);
        compare(graph.nodePositions[2].y, 260);
        compare(JSON.parse(backend.layout)[graph.uniqueKeyById[2]], [420, 260]);
    }

    function test_routing_avoids_obstacles() {
        var obstacles = [{l: 100, t: 20, r: 180, b: 180}];
        var path = graph.routeOrtho(0, 100, 300, 100, obstacles);
        verify(path !== null);
        compare(path[0], [0, 100]);
        compare(path[path.length - 1], [300, 100]);
        for (var i = 1; i < path.length; i++) {
            verify(path[i - 1][0] === path[i][0] || path[i - 1][1] === path[i][1]);
            verify(!graph.segmentBlocked(path[i - 1][0], path[i - 1][1], path[i][0], path[i][1], obstacles, 0));
        }
    }

    function test_session_reset_clears_transient_state() {
        graph.selectedNodes = {2: graph.nodeSig(graph.nodes[1])};
        graph.pushUndo({kind: "hide", label: "hide", keys: ["Plugin:effect"]});
        graph.pendingMeterAttach = [{id: 99}];
        backend.session_changed();
        compare(graph.undoStack.length, 0);
        compare(graph.redoStack.length, 0);
        compare(Object.keys(graph.selectedNodes).length, 0);
        compare(graph.pendingMeterAttach.length, 0);
        compare(graph.nodes.length, 3);
    }

    function test_zoom_keeps_cursor_anchor() {
        var before = graph.toCanvas(400, 250);
        graph.zoomAt(400, 250, 1.5);
        var after = graph.toCanvas(400, 250);
        fuzzyCompare(after.x, before.x, 0.001);
        fuzzyCompare(after.y, before.y, 0.001);
    }

    function test_pointer_drag_and_keyboard_undo() {
        graph.snapToGrid = false;
        var node = graph.nodePositions[2];
        var x = node.x;
        var y = node.y;
        var screen = graph.toScreen(x + 20, y + 10);
        mouseDrag(graph, screen.x, screen.y, 30, 70, Qt.LeftButton);
        verify(graph.nodePositions[2].y > y + 30);
        verify(graph.canUndo);
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        compare(graph.nodePositions[2].x, x);
        compare(graph.nodePositions[2].y, y);
    }

    function test_meter_overlay() {
        backend.meter = true;
        graph.refreshData();
        compare(graph.meterNodeIds, [2]);
        tryVerify(function() { return graph.meterFrames[2] !== undefined; });
        compare(graph.meterFrames[2].channels, 1);
        tryCompare(graph, "meterLevels", {2: [0.5]});
        graph.elements.canvas.requestPaint();
        verify(waitForRendering(graph));
    }

    function test_wire_styles_data() {
        return [
            {tag: "bezier", style: "bezier"},
            {tag: "straight", style: "straight"},
            {tag: "stepped", style: "stepped"},
            {tag: "circuit", style: "circuit"}
        ];
    }

    function test_wire_styles(data) {
        graph.wireStyle = data.style;
        graph.gradientWires = true;
        graph.selectAll();
        graph.elements.canvas.requestPaint();
        verify(waitForRendering(graph));
        var start = graph.portPositions[11];
        var end = graph.portPositions[21];
        var midpoint = graph.wireMidpoint(start.cx, start.cy, end.cx, end.cy, 100);
        verify(isFinite(midpoint.x));
        verify(isFinite(midpoint.y));
    }

    function test_context_menu_and_selection() {
        graph.selectAll();
        compare(graph.selectedNodeIds().length, 3);
        var screen = graph.toScreen(graph.nodePositions[2].x + 30, graph.nodePositions[2].y + 10);
        mouseClick(graph, screen.x, screen.y, Qt.RightButton);
        tryCompare(graph.elements.nodeContextMenu, "opened", true);
        compare(graph.contextNodeId, 2);
        graph.elements.nodeContextMenu.close();
        graph.clearSelection();
        verify(!graph.hasSelection());
    }
}
