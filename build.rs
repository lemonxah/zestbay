use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
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
            .qml_file("qml/MidiMappings.qml")
            .qml_file("qml/Preferences.qml")
            .qml_file("qml/CpuOverlay.qml")
            .qml_file("qml/About.qml")
            .qml_file("qml/ZbBadge.qml")
            .qml_file("qml/ZbButton.qml")
            .qml_file("qml/ZbCard.qml")
            .qml_file("qml/ZbCountPill.qml")
            .qml_file("qml/ZbDialogHeader.qml")
            .qml_file("qml/ZbDialog.qml")
            .qml_file("qml/ZbDivider.qml")
            .qml_file("qml/ZbIcon.qml")
            .qml_file("qml/ZbLogo.qml")
            .qml_file("qml/ZbMenu.qml")
            .qml_file("qml/ZbMenuItem.qml")
            .qml_file("qml/ZbMenuSeparator.qml")
            .qml_file("qml/ZbSearchField.qml")
            .qml_file("qml/ZbSectionLabel.qml")
            .qml_file("qml/ZbSegmented.qml")
            .qml_file("qml/ZbScrollBar.qml")
            .qml_file("qml/ZbSlider.qml")
            .qml_file("qml/ZbSwitch.qml")
            .qml_file("qml/ZbTextField.qml")
            .qml_file("qml/ZbTooltip.qml")
            .qml_file("qml/ZbValueField.qml")
            .qml_file("qml/ZbWheelScroll.qml")
            .qml_file("qml/ZbWindow.qml")
            .qml_file(QmlFile::from("qml/Theme.qml").singleton(true))
            .qml_file(QmlFile::from("qml/Glyphs.qml").singleton(true)),
    )
    // Declares the singletons for the qml/ directory import (Qt < 6.5).
    .qrc_resources(["qml/qmldir"])
    // Bundled IBM Plex (OFL, fonts/OFL.txt), loaded by Theme.qml.
    .qrc_resources([
        "fonts/IBMPlexMono-Medium.ttf",
        "fonts/IBMPlexMono-Regular.ttf",
        "fonts/IBMPlexSans-Medium.ttf",
        "fonts/IBMPlexSans-Regular.ttf",
        "fonts/IBMPlexSans-SemiBold.ttf",
    ])
    .qt_module("Network")
    .files(["src/ui/qobject_bridge.rs"])
    .build();
}
