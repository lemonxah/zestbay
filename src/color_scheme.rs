use std::sync::atomic::{AtomicI32, Ordering};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

static SCHEME: AtomicI32 = AtomicI32::new(0);

pub fn current() -> i32 {
    SCHEME.load(Ordering::Relaxed)
}

pub fn start() {
    let spawned = std::thread::Builder::new()
        .name("color-scheme".into())
        .spawn(|| {
            if let Err(e) = watch() {
                log::info!("Desktop colour scheme unavailable from the portal: {e}");
            }
        });
    if let Err(e) = spawned {
        log::warn!("Could not start the colour scheme watcher: {e}");
    }
}

fn scheme_of(value: &Value) -> Option<i32> {
    match value {
        Value::Value(inner) => scheme_of(inner),
        Value::U32(v) => i32::try_from(*v).ok(),
        _ => None,
    }
}

fn store(value: &Value) {
    if let Some(scheme) = scheme_of(value) {
        log::info!("Desktop colour scheme: {scheme}");
        SCHEME.store(scheme, Ordering::Relaxed);
    }
}

fn watch() -> zbus::Result<()> {
    let connection = Connection::session()?;
    let proxy = Proxy::new(&connection, DESTINATION, PATH, INTERFACE)?;
    let changes = proxy.receive_signal("SettingChanged")?;

    let initial = proxy
        .call::<_, _, OwnedValue>("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| proxy.call::<_, _, OwnedValue>("Read", &(NAMESPACE, KEY)))?;
    store(&initial);

    for message in changes {
        let Ok((namespace, key, value)) = message
            .body()
            .deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        if namespace == NAMESPACE && key == KEY {
            store(&value);
        }
    }
    Ok(())
}
