use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(
        QmlModule::new("ZestBay")
            .qml_file("qml/main.qml")
            .qml_file("qml/GraphView.qml")
            .qml_file("qml/PluginBrowser.qml")
            .qml_file("qml/PluginParams.qml")
            .qml_file("qml/RuleEditor.qml")
            .qml_file("qml/Preferences.qml"),
    )
    // Link Qt modules we need beyond the defaults
    // Qt Core is always linked; Qt Gui and Qt Qml are linked by cxx-qt-lib features
    .qt_module("Network") // Required by Qt Qml on some platforms
    // Our bridge file(s)
    .files(["src/ui/qobject_bridge.rs"])
    .build();
}
