//! VST2 X11 editors. All window and dispatcher calls run on the plugin main
//! thread. Resize callbacks only publish atomic requests, avoiding reentry
//! into a borrowed GUI registry from inside the plugin's dispatcher.

use super::abi::{eff, *};
use crate::clap::x11::HostWindow;
use crate::pipewire::{PluginEvent, PwEvent};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

struct Editor {
    effect: *mut Effect,
    window: HostWindow,
    events: Sender<PwEvent>,
}

thread_local! {
    static EDITORS: RefCell<HashMap<u64, Editor>> = RefCell::new(HashMap::new());
}

unsafe fn size(effect: *mut Effect) -> Option<(u32, u32)> {
    let mut rect: *const Rect = std::ptr::null();
    unsafe {
        dispatch(
            effect,
            eff::EDIT_RECT,
            0,
            0,
            (&mut rect as *mut *const Rect).cast(),
            0.0,
        );
        let rect = rect.as_ref()?;
        let width = i32::from(rect.right) - i32::from(rect.left);
        let height = i32::from(rect.bottom) - i32::from(rect.top);
        ((1..=16384).contains(&width) && (1..=16384).contains(&height))
            .then_some((width as u32, height as u32))
    }
}

/// `effect` remains live until its editor is closed; main thread only.
pub unsafe fn open_vst2_gui(
    effect: *mut Effect,
    instance_id: u64,
    name: &str,
    event_tx: &Sender<PwEvent>,
    _cmd_tx: &Sender<crate::pipewire::PwCommand>,
) {
    if EDITORS.with(|m| m.borrow().contains_key(&instance_id)) {
        return;
    }
    unsafe {
        if (*effect).flags & HAS_EDITOR == 0 {
            return;
        }
        let (width, height) = size(effect).unwrap_or((640, 480));
        let Some(window) = HostWindow::open(name, width, height) else {
            return;
        };
        // Linux VST2 uses the X11 Window ID encoded as a pointer-sized value.
        dispatch(effect, eff::EDIT_OPEN, 0, 0, window.window as *mut _, 0.0);
        if let Some((w, h)) = size(effect) {
            window.resize(w, h);
        }
        window.flush();
        EDITORS.with(|m| {
            m.borrow_mut().insert(
                instance_id,
                Editor {
                    effect,
                    window,
                    events: event_tx.clone(),
                },
            );
        });
    }
    let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiOpened { instance_id }));
}

pub fn idle() {
    // Take ownership so no RefCell borrow crosses a call into foreign code.
    let mut editors = EDITORS.with(|m| std::mem::take(&mut *m.borrow_mut()));
    editors.retain(|id, editor| unsafe {
        if editor.window.pump() {
            command(editor.effect, eff::EDIT_CLOSE, 0);
            let _ = editor
                .events
                .send(PwEvent::Plugin(PluginEvent::PluginUiClosed {
                    instance_id: *id,
                }));
            return false;
        }
        command(editor.effect, eff::EDIT_IDLE, 0);
        let resize = super::host::host_data(editor.effect)
            .resize
            .swap(0, Ordering::AcqRel);
        if resize != 0 {
            editor.window.resize((resize >> 32) as u32, resize as u32);
        }
        editor.window.flush();
        true
    });
    EDITORS.with(|m| m.borrow_mut().extend(editors));
}

pub fn close_vst2_gui(instance_id: u64, event_tx: &Sender<PwEvent>) {
    let editor = EDITORS.with(|m| m.borrow_mut().remove(&instance_id));
    if let Some(editor) = editor {
        unsafe {
            command(editor.effect, eff::EDIT_CLOSE, 0);
        }
        drop(editor);
        let _ = event_tx.send(PwEvent::Plugin(PluginEvent::PluginUiClosed { instance_id }));
    }
}

pub(super) fn force_close_for_plugin(effect: *mut Effect) {
    let entry = EDITORS.with(|m| {
        m.borrow()
            .iter()
            .find(|(_, e)| e.effect == effect)
            .map(|(id, e)| (*id, e.events.clone()))
    });
    if let Some((id, tx)) = entry {
        close_vst2_gui(id, &tx);
    }
}
