use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::process::Command;

fn main() {
    // Detect Qt version at compile time via pkg-config or qmake6
    let qt_version = Command::new("pkg-config")
        .args(["--modversion", "Qt6Core"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            Command::new("qmake6")
                .args(["-query", "QT_VERSION"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=QT_VERSION={}", qt_version);

    CxxQtBuilder::new_qml_module(
        QmlModule::new("ZestBay")
            .qml_file("qml/main.qml")
            .qml_file("qml/GraphView.qml")
            .qml_file("qml/PluginBrowser.qml")
            .qml_file("qml/PluginParams.qml")
            .qml_file("qml/RuleEditor.qml")
            .qml_file("qml/PluginManager.qml")
            .qml_file("qml/Preferences.qml")
            .qml_file("qml/CpuOverlay.qml")
            .qml_file("qml/About.qml"),
    )
    .qt_module("Network")
    .files(["src/ui/qobject_bridge.rs"])
    .build();
}
