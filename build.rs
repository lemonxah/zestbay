use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("ZestBay")
            .qml_file("qml/main.qml")
            .qml_file("qml/GraphView.qml")
            .qml_file("qml/PluginBrowser.qml")
            .qml_file("qml/PluginParams.qml")
            .qml_file("qml/RuleEditor.qml")
            .qml_file("qml/PluginManager.qml")
            .qml_file("qml/Preferences.qml")
            .qml_file("qml/CpuOverlay.qml"),
    )
    .qt_module("Network")
    .files(["src/ui/qobject_bridge.rs"])
    .build();
}
