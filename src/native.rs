//! Shared `muda` tree-building logic for the macOS (`macos.rs`) and Windows
//! (`windows.rs`) backends — everything that's identical once you already
//! have a `muda::Menu` main thread to build on, i.e. everything except how
//! that `Menu` gets attached to the running app (`init_for_nsapp()` vs
//! `init_for_hwnd(hwnd)`).
//!
//! Not compiled under `cfg(test)`: `muda` is main-thread-only and mostly
//! panics off it (its own documented constraint), which the test harness
//! isn't, so tests exercise `fallback.rs` instead — see the crate root doc
//! comment.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use muda::accelerator::{Accelerator as MudaAccelerator, Code, Modifiers as MudaModifiers};
use muda::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::def::{Accelerator, EntryDef, Key, MenuBarDef, Predefined, SubmenuDef};

/// A leaf item's live handle, kept so [`BuiltMenu::set_enabled`]/
/// [`BuiltMenu::set_checked`] can act on it later. `muda`'s `MenuItem`/
/// `CheckMenuItem` are `Rc`-backed `Clone`s of the same underlying native
/// item (see their doc comments), so cloning one into this map costs
/// nothing beyond a refcount bump — it does not duplicate the menu item.
enum Handle {
    Plain(MenuItem),
    Check(CheckMenuItem),
}

impl Handle {
    fn set_enabled(&self, enabled: bool) {
        match self {
            Self::Plain(item) => item.set_enabled(enabled),
            Self::Check(item) => item.set_enabled(enabled),
        }
    }

    fn set_checked(&self, checked: bool) {
        // Silently ignored for a plain item, same as `fallback.rs`'s
        // equivalent: an app calling `set_checked` on an item it never
        // marked `checkable()` is a caller bug we don't want to panic over.
        if let Self::Check(item) = self {
            item.set_checked(checked);
        }
    }
}

/// Everything a `muda`-backed `Menu` needs kept alive for the app's whole
/// lifetime, plus the click-dispatch state built while constructing it.
///
/// Holding `all` is load-bearing, not bookkeeping: `muda` on macOS (and,
/// per its own docs, Windows too) ties native click dispatch to these Rust
/// wrapper objects. Dropping them once the tree is built would leave the
/// still-attached native items pointing at freed state and abort inside
/// `muda`'s click handler the first time an operator picks one — confirmed
/// against a real macOS crash report in the `native_menu.rs` modules this
/// crate replaces, not just a hunch. `muda`'s own examples hold every item
/// as a struct field for the app's lifetime; boxing the whole tree into one
/// `Vec` here does the same thing without hand-naming a field per item.
pub(crate) struct BuiltMenu<A> {
    pub(crate) menu: Menu,
    #[allow(dead_code)] // kept alive only, never read again
    all: Vec<Box<dyn IsMenuItem>>,
    handles: HashMap<A, Handle>,
    pending: Arc<Mutex<VecDeque<A>>>,
}

impl<A: Clone + Eq + Hash> BuiltMenu<A> {
    /// Drains at most one pending native menu click — call in a loop once
    /// per frame, same "drain a queue each frame" shape the `poll()`s in
    /// the `native_menu.rs` modules this crate replaces used.
    pub(crate) fn poll(&mut self) -> Option<A> {
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .pop_front()
    }

    pub(crate) fn set_enabled(&self, action: &A, enabled: bool) {
        if let Some(handle) = self.handles.get(action) {
            handle.set_enabled(enabled);
        }
    }

    pub(crate) fn set_checked(&self, action: &A, checked: bool) {
        if let Some(handle) = self.handles.get(action) {
            handle.set_checked(checked);
        }
    }
}

/// Builds the `muda::Menu` tree for `def` (not yet attached to anything —
/// the caller still has to call `init_for_nsapp()`/`init_for_hwnd()`) and
/// registers the click handler that feeds [`BuiltMenu::poll`].
///
/// `ctx` wakes a sleeping egui on a click the same way the `native_menu.rs`
/// modules this crate replaces do: a native menu click happens with the
/// pointer up in the *system* menu bar, generating no egui input, so an
/// on-demand repainter like egui/eframe would otherwise sit on the click
/// until something unrelated scheduled a frame.
///
/// Registers the handler *after* the tree is built and attached, matching
/// those modules — `MenuEvent::set_event_handler`'s `OnceCell` latches
/// permanently on whichever handler (or lack of one) is in place the first
/// time any event is sent, so this only has to happen before the menu can
/// possibly be clicked, which "before the first frame runs" already
/// guarantees.
pub(crate) fn build<A: Clone + Eq + Hash + Send + Sync + 'static>(
    def: &MenuBarDef<A>,
    ctx: egui::Context,
) -> BuiltMenu<A> {
    let mut all: Vec<Box<dyn IsMenuItem>> = Vec::new();
    let mut ids: HashMap<MenuId, A> = HashMap::new();
    let mut handles: HashMap<A, Handle> = HashMap::new();

    let menu = Menu::new();
    for submenu_def in &def.menus {
        let submenu = build_submenu(submenu_def, &mut all, &mut ids, &mut handles);
        menu.append(&submenu)
            .expect("attaching a top-level submenu");
        all.push(Box::new(submenu));
    }

    let ids = Arc::new(ids);
    let pending: Arc<Mutex<VecDeque<A>>> = Arc::new(Mutex::new(VecDeque::new()));
    MenuEvent::set_event_handler(Some({
        let ids = Arc::clone(&ids);
        let pending = Arc::clone(&pending);
        move |event: MenuEvent| {
            if let Some(action) = ids.get(event.id()) {
                pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .push_back(action.clone());
                ctx.request_repaint();
            }
        }
    }));

    BuiltMenu {
        menu,
        all,
        handles,
        pending,
    }
}

fn build_submenu<A: Clone + Eq + Hash>(
    def: &SubmenuDef<A>,
    all: &mut Vec<Box<dyn IsMenuItem>>,
    ids: &mut HashMap<MenuId, A>,
    handles: &mut HashMap<A, Handle>,
) -> Submenu {
    let mut children: Vec<Box<dyn IsMenuItem>> = Vec::new();
    for entry in &def.entries {
        match entry {
            EntryDef::Separator => children.push(Box::new(PredefinedMenuItem::separator())),
            EntryDef::Predefined(predefined) => children.push(build_predefined(*predefined)),
            EntryDef::Item(item) => {
                let accelerator = item.accelerator.map(|a| to_muda_accelerator(&a));
                if let Some(checked) = item.checked {
                    let native =
                        CheckMenuItem::new(&item.label, item.enabled, checked, accelerator);
                    ids.insert(native.id().clone(), item.action.clone());
                    handles.insert(item.action.clone(), Handle::Check(native.clone()));
                    children.push(Box::new(native));
                } else {
                    let native = MenuItem::new(&item.label, item.enabled, accelerator);
                    ids.insert(native.id().clone(), item.action.clone());
                    handles.insert(item.action.clone(), Handle::Plain(native.clone()));
                    children.push(Box::new(native));
                }
            }
            EntryDef::Submenu(nested) => {
                let submenu = build_submenu(nested, all, ids, handles);
                children.push(Box::new(submenu));
            }
        }
    }
    let refs: Vec<&dyn IsMenuItem> = children.iter().map(AsRef::as_ref).collect();
    let submenu = Submenu::with_items(&def.label, def.enabled, &refs).expect("building a submenu");
    all.extend(children);
    submenu
}

fn build_predefined(predefined: Predefined) -> Box<dyn IsMenuItem> {
    use Predefined::*;
    Box::new(match predefined {
        About => PredefinedMenuItem::about(None, None),
        Services => PredefinedMenuItem::services(None),
        Hide => PredefinedMenuItem::hide(None),
        HideOthers => PredefinedMenuItem::hide_others(None),
        ShowAll => PredefinedMenuItem::show_all(None),
        Minimize => PredefinedMenuItem::minimize(None),
        Maximize => PredefinedMenuItem::maximize(None),
        Fullscreen => PredefinedMenuItem::fullscreen(None),
        CloseWindow => PredefinedMenuItem::close_window(None),
        BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
        Quit => PredefinedMenuItem::quit(None),
        Undo => PredefinedMenuItem::undo(None),
        Redo => PredefinedMenuItem::redo(None),
        Cut => PredefinedMenuItem::cut(None),
        Copy => PredefinedMenuItem::copy(None),
        Paste => PredefinedMenuItem::paste(None),
        SelectAll => PredefinedMenuItem::select_all(None),
    })
}

fn to_muda_accelerator(accelerator: &Accelerator) -> MudaAccelerator {
    let mut mods = MudaModifiers::empty();
    if accelerator.modifiers.primary {
        mods |= muda::accelerator::CMD_OR_CTRL;
    }
    if accelerator.modifiers.shift {
        mods |= MudaModifiers::SHIFT;
    }
    if accelerator.modifiers.alt {
        mods |= MudaModifiers::ALT;
    }
    let code = match accelerator.key {
        Key::A => Code::KeyA,
        Key::B => Code::KeyB,
        Key::C => Code::KeyC,
        Key::D => Code::KeyD,
        Key::E => Code::KeyE,
        Key::F => Code::KeyF,
        Key::G => Code::KeyG,
        Key::H => Code::KeyH,
        Key::I => Code::KeyI,
        Key::J => Code::KeyJ,
        Key::K => Code::KeyK,
        Key::L => Code::KeyL,
        Key::M => Code::KeyM,
        Key::N => Code::KeyN,
        Key::O => Code::KeyO,
        Key::P => Code::KeyP,
        Key::Q => Code::KeyQ,
        Key::R => Code::KeyR,
        Key::S => Code::KeyS,
        Key::T => Code::KeyT,
        Key::U => Code::KeyU,
        Key::V => Code::KeyV,
        Key::W => Code::KeyW,
        Key::X => Code::KeyX,
        Key::Y => Code::KeyY,
        Key::Z => Code::KeyZ,
        Key::Digit0 => Code::Digit0,
        Key::Digit1 => Code::Digit1,
        Key::Digit2 => Code::Digit2,
        Key::Digit3 => Code::Digit3,
        Key::Digit4 => Code::Digit4,
        Key::Digit5 => Code::Digit5,
        Key::Digit6 => Code::Digit6,
        Key::Digit7 => Code::Digit7,
        Key::Digit8 => Code::Digit8,
        Key::Digit9 => Code::Digit9,
        Key::Comma => Code::Comma,
        Key::Period => Code::Period,
        Key::Minus => Code::Minus,
        Key::Equals => Code::Equal,
        Key::Slash => Code::Slash,
    };
    MudaAccelerator::new(Some(mods), code)
}
