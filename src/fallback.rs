//! Fallback backend: an in-window `egui::MenuBar`. Used automatically on
//! every OS `muda` doesn't have a backend for in this crate (i.e. not
//! macOS/Windows), and available explicitly everywhere else via
//! `AppMenu::new_in_window` — needing no platform API and no `muda` is
//! exactly what makes it safe to call from a test harness thread, which
//! isn't `muda`'s required real main thread. See `AppMenu::new_in_window`'s
//! and the crate root's doc comments for why an app has to ask for this
//! backend explicitly under its own `cfg(test)` rather than this crate
//! switching to it automatically.

use std::collections::HashMap;
use std::hash::Hash;

use crate::def::{Accelerator, EntryDef, ItemDef, Key, MenuBarDef, Predefined};

/// The live, mutable half of an item's state — everything [`Backend::new`]
/// seeds from the [`ItemDef`] and [`Backend::set_enabled`]/
/// [`Backend::set_checked`] update afterwards. The definition itself stays
/// untouched; this is what rendering actually reads.
#[derive(Debug, Clone, Copy)]
struct ItemState {
    enabled: bool,
    /// `None`: not checkable, same meaning as `ItemDef::checked`. `Some`:
    /// checkable, currently in this state.
    checked: Option<bool>,
}

pub(crate) struct Backend<A> {
    def: MenuBarDef<A>,
    state: HashMap<A, ItemState>,
}

impl<A: Clone + Eq + Hash> Backend<A> {
    /// `_cc` is unused here (no platform handle needed to draw an in-window
    /// menu) — kept as a parameter so every backend's constructor has the
    /// same signature.
    pub(crate) fn new(_cc: &eframe::CreationContext<'_>, def: MenuBarDef<A>) -> Self {
        let state = def
            .items()
            .into_iter()
            .map(|item| {
                (
                    item.action.clone(),
                    ItemState {
                        enabled: item.enabled,
                        checked: item.checked,
                    },
                )
            })
            .collect();
        Self { def, state }
    }

    /// Always `None`: this backend reports clicks through [`Self::ui`], the
    /// same "poll on macOS/Windows, `ui` everywhere else" split
    /// [`crate::AppMenu`]'s doc comment describes.
    pub(crate) fn poll(&mut self) -> Option<A> {
        None
    }

    /// Draws the in-window menu bar and returns every action clicked, or
    /// triggered by its keyboard shortcut, this frame.
    pub(crate) fn ui(&mut self, ui: &mut egui::Ui) -> Vec<A> {
        let mut clicked = Vec::new();
        let ctx = ui.ctx().clone();

        // Shortcuts fire whether or not the menu holding that item is open
        // — matching `muda` accelerators, which don't require the native
        // menu to be dropped down either. Checked ahead of drawing so a
        // shortcut consumed here doesn't also get "clicked" by the button
        // below (it can't: `consume_shortcut` removes the key event before
        // `MenuBar::ui` ever sees it, so there's no double-fire risk).
        for item in self.def.items() {
            let Some(accelerator) = item.accelerator else {
                continue;
            };
            let enabled = self
                .state
                .get(&item.action)
                .map_or(item.enabled, |s| s.enabled);
            if !enabled {
                continue;
            }
            let shortcut = to_egui_shortcut(accelerator);
            if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
                clicked.push(item.action.clone());
            }
        }

        egui::MenuBar::new().ui(ui, |ui| {
            for submenu in &self.def.menus {
                ui.menu_button(&submenu.label, |ui| {
                    render_entries(ui, &submenu.entries, &self.state, &mut clicked);
                });
            }
        });

        clicked
    }

    pub(crate) fn set_enabled(&mut self, action: &A, enabled: bool) {
        if let Some(state) = self.state.get_mut(action) {
            state.enabled = enabled;
        }
    }

    /// A no-op if `action` names an item that isn't checkable — same
    /// "caller bug, don't panic" choice `native.rs::Handle::set_checked`
    /// makes.
    pub(crate) fn set_checked(&mut self, action: &A, checked: bool) {
        // Two different questions, kept as nested `if`s rather than joined
        // with `&&` for readability: "is there such an item" vs. "is it
        // checkable".
        #[allow(clippy::collapsible_if)]
        if let Some(state) = self.state.get_mut(action) {
            if state.checked.is_some() {
                state.checked = Some(checked);
            }
        }
    }

    /// No-op: no OS-level menu theme to sync on this backend — the in-window
    /// bar already follows whatever `egui::Visuals` the app's own theme
    /// applies. Present for parity with the Windows backend, where it
    /// matters.
    pub(crate) fn sync_theme(&mut self, _ctx: &egui::Context) {}
}

fn render_entries<A: Clone + Eq + Hash>(
    ui: &mut egui::Ui,
    entries: &[EntryDef<A>],
    state: &HashMap<A, ItemState>,
    clicked: &mut Vec<A>,
) {
    for entry in entries {
        match entry {
            EntryDef::Separator => {
                ui.separator();
            }
            // Quit is the one predefined item with an obvious, always-safe
            // in-window equivalent. Every other variant is a macOS/
            // window-manager concept (Hide, Services, Minimize, …) or
            // already works in a focused egui text field without a menu
            // item at all (Copy/Cut/Paste/Undo/Redo/Select All) — see
            // `Predefined`'s doc comment — so there's nothing to draw for
            // them here.
            EntryDef::Predefined(Predefined::Quit) => {
                if ui.button("Quit").clicked() {
                    ui.close();
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            EntryDef::Predefined(_) => {}
            EntryDef::Item(item) => render_item(ui, item, state, clicked),
            EntryDef::Submenu(submenu) => {
                ui.menu_button(&submenu.label, |ui| {
                    render_entries(ui, &submenu.entries, state, clicked);
                });
            }
        }
    }
}

fn render_item<A: Clone + Eq + Hash>(
    ui: &mut egui::Ui,
    item: &ItemDef<A>,
    state: &HashMap<A, ItemState>,
    clicked: &mut Vec<A>,
) {
    let live = state.get(&item.action);
    let enabled = live.map_or(item.enabled, |s| s.enabled);
    let checked = live
        .and_then(|s| s.checked)
        .or(item.checked)
        .unwrap_or(false);

    let mut button = egui::Button::new(&item.label).selected(checked);
    if let Some(accelerator) = item.accelerator {
        button = button.shortcut_text(ui.ctx().format_shortcut(&to_egui_shortcut(accelerator)));
    }
    if ui.add_enabled(enabled, button).clicked() {
        clicked.push(item.action.clone());
        ui.close();
    }
}

fn to_egui_shortcut(accelerator: Accelerator) -> egui::KeyboardShortcut {
    let mut modifiers = egui::Modifiers::NONE;
    if accelerator.modifiers.primary {
        modifiers |= egui::Modifiers::COMMAND;
    }
    if accelerator.modifiers.shift {
        modifiers |= egui::Modifiers::SHIFT;
    }
    if accelerator.modifiers.alt {
        modifiers |= egui::Modifiers::ALT;
    }
    egui::KeyboardShortcut::new(modifiers, to_egui_key(accelerator.key))
}

fn to_egui_key(key: Key) -> egui::Key {
    match key {
        Key::A => egui::Key::A,
        Key::B => egui::Key::B,
        Key::C => egui::Key::C,
        Key::D => egui::Key::D,
        Key::E => egui::Key::E,
        Key::F => egui::Key::F,
        Key::G => egui::Key::G,
        Key::H => egui::Key::H,
        Key::I => egui::Key::I,
        Key::J => egui::Key::J,
        Key::K => egui::Key::K,
        Key::L => egui::Key::L,
        Key::M => egui::Key::M,
        Key::N => egui::Key::N,
        Key::O => egui::Key::O,
        Key::P => egui::Key::P,
        Key::Q => egui::Key::Q,
        Key::R => egui::Key::R,
        Key::S => egui::Key::S,
        Key::T => egui::Key::T,
        Key::U => egui::Key::U,
        Key::V => egui::Key::V,
        Key::W => egui::Key::W,
        Key::X => egui::Key::X,
        Key::Y => egui::Key::Y,
        Key::Z => egui::Key::Z,
        Key::Digit0 => egui::Key::Num0,
        Key::Digit1 => egui::Key::Num1,
        Key::Digit2 => egui::Key::Num2,
        Key::Digit3 => egui::Key::Num3,
        Key::Digit4 => egui::Key::Num4,
        Key::Digit5 => egui::Key::Num5,
        Key::Digit6 => egui::Key::Num6,
        Key::Digit7 => egui::Key::Num7,
        Key::Digit8 => egui::Key::Num8,
        Key::Digit9 => egui::Key::Num9,
        Key::Comma => egui::Key::Comma,
        Key::Period => egui::Key::Period,
        Key::Minus => egui::Key::Minus,
        Key::Equals => egui::Key::Equals,
        Key::Slash => egui::Key::Slash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::SubmenuDef;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Action {
        One,
        Two,
    }

    fn cc() -> eframe::CreationContext<'static> {
        eframe::CreationContext::_new_kittest(egui::Context::default())
    }

    #[test]
    fn seeds_state_from_definition() {
        let def = MenuBarDef::new([SubmenuDef::new(
            "Menu",
            [
                ItemDef::new("One", Action::One).into(),
                ItemDef::new("Two", Action::Two).checkable(true).into(),
            ],
        )]);
        let backend = Backend::new(&cc(), def);
        assert!(backend.state.get(&Action::One).unwrap().enabled);
        assert_eq!(backend.state.get(&Action::One).unwrap().checked, None);
        assert_eq!(backend.state.get(&Action::Two).unwrap().checked, Some(true));
    }

    #[test]
    fn set_enabled_updates_state_without_touching_the_definition() {
        let def = MenuBarDef::new([SubmenuDef::new(
            "Menu",
            [ItemDef::new("One", Action::One).into()],
        )]);
        let mut backend = Backend::new(&cc(), def);
        backend.set_enabled(&Action::One, false);
        assert!(!backend.state.get(&Action::One).unwrap().enabled);
        assert!(backend.def.find(&Action::One).unwrap().enabled);
    }

    #[test]
    fn set_checked_is_a_noop_for_non_checkable_items() {
        let def = MenuBarDef::new([SubmenuDef::new(
            "Menu",
            [ItemDef::new("One", Action::One).into()],
        )]);
        let mut backend = Backend::new(&cc(), def);
        backend.set_checked(&Action::One, true);
        assert_eq!(backend.state.get(&Action::One).unwrap().checked, None);
    }

    #[test]
    fn set_checked_updates_checkable_items() {
        let def = MenuBarDef::new([SubmenuDef::new(
            "Menu",
            [ItemDef::new("Two", Action::Two).checkable(false).into()],
        )]);
        let mut backend = Backend::new(&cc(), def);
        backend.set_checked(&Action::Two, true);
        assert_eq!(backend.state.get(&Action::Two).unwrap().checked, Some(true));
    }

    #[test]
    fn poll_never_yields_anything() {
        let def: MenuBarDef<Action> = MenuBarDef::default();
        let mut backend = Backend::new(&cc(), def);
        assert!(backend.poll().is_none());
    }

    #[test]
    fn unknown_action_updates_are_ignored() {
        let def: MenuBarDef<Action> = MenuBarDef::default();
        let mut backend = Backend::new(&cc(), def);
        // Neither call should panic even though no item uses `Action::One`.
        backend.set_enabled(&Action::One, false);
        backend.set_checked(&Action::One, true);
    }
}
