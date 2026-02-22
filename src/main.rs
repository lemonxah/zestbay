mod lv2;
mod patchbay;
mod pipewire;
mod tray;
mod ui;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QString, QUrl};
use std::pin::Pin;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting ZestBay");

    let mut app = QGuiApplication::new();

    QGuiApplication::set_desktop_file_name(&QString::from("zestbay"));

    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine
            .on_object_created(|_, obj, url| {
                if obj.is_null() {
                    log::error!("QML object creation FAILED for: {:?}", url.to_string());
                } else {
                    log::info!("QML object created successfully for: {:?}", url.to_string());
                }
            })
            .release();
    }

    if let Some(engine) = engine.as_mut() {
        engine
            .on_object_creation_failed(|_, url| {
                log::error!("QML creation failed signal for: {:?}", url.to_string());
            })
            .release();
    }

    let url = QUrl::from("qrc:/qt/qml/ZestBay/qml/main.qml");
    log::info!("Loading QML from: {:?}", url.to_string());
    if let Some(engine) = engine.as_mut() {
        engine.load(&url);
    }

    if let Some(engine) = engine.as_mut() {
        let engine: Pin<&mut QQmlEngine> = engine.upcast_pin();
        engine
            .on_quit(|_| {
                log::info!("QML engine quit signal received");
            })
            .release();
    }

    log::info!("Starting Qt event loop");

    if let Some(app) = app.as_mut() {
        app.exec();
    }

    log::info!("Qt event loop exited");

    lv2::ui::shutdown_gtk_thread();
}
