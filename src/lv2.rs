pub mod filter;
pub mod host;
pub mod log;
pub mod options;
pub mod scanner;
pub mod state;
pub mod types;
pub mod ui;
pub mod ui_bridge_client;
pub mod urid;
pub mod worker;

pub use host::Lv2Manager;
pub use types::*;

thread_local! {
    static THREAD_WORLD: std::cell::RefCell<Option<std::rc::Rc<lilv::World>>> =
        const { std::cell::RefCell::new(None) };
}

/// A fully loaded lilv world for the calling thread, created on first use.
/// lilv worlds are not shared between threads: each thread that needs one
/// (the PipeWire thread, the GTK UI thread) gets its own.
pub fn thread_world() -> std::rc::Rc<lilv::World> {
    THREAD_WORLD.with(|w| {
        w.borrow_mut()
            .get_or_insert_with(|| std::rc::Rc::new(lilv::World::with_load_all()))
            .clone()
    })
}

/// Reload the calling thread's world (e.g. a plugin installed since it was
/// loaded is not found).  Instances keep the old world alive until dropped.
pub fn reload_thread_world() -> std::rc::Rc<lilv::World> {
    let world = std::rc::Rc::new(lilv::World::with_load_all());
    THREAD_WORLD.with(|w| *w.borrow_mut() = Some(world.clone()));
    world
}

/// Find `uri` in the calling thread's world, reloading the world once if it
/// is not there.
pub fn find_in_thread_world(
    uri: &str,
) -> Option<(std::rc::Rc<lilv::World>, lilv::plugin::Plugin)> {
    let find = |world: &lilv::World| {
        let uri_node = world.new_uri(uri);
        world
            .plugins()
            .iter()
            .find(|p| p.uri().as_uri() == uri_node.as_uri())
    };
    let world = thread_world();
    if let Some(p) = find(&world) {
        return Some((world, p));
    }
    let world = reload_thread_world();
    let p = find(&world)?;
    Some((world, p))
}
