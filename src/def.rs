//! The menu bar described as plain data, independent of any backend.
//!
//! Nothing in this module touches `muda` or draws anything — that's
//! deliberate. The macOS/Windows backends (`native.rs` plus `macos.rs`/
//! `windows.rs`) turn a [`MenuBarDef`] into a real `muda::Menu`; the
//! fallback backend (`fallback.rs`) turns the same [`MenuBarDef`] into an
//! in-window `egui::MenuBar`. Neither backend, nor the app that builds a
//! `MenuBarDef`, needs to know which one is actually running.

use std::hash::Hash;

/// One keyboard shortcut, described independently of both `muda`'s
/// `accelerator::Accelerator` and egui's `KeyboardShortcut` — each backend
/// converts it to whichever of those it needs (`native.rs::to_muda_accelerator`,
/// `fallback.rs::to_egui_shortcut`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Accelerator {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Accelerator {
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// The conventional "app shortcut" modifier alone (⌘ on macOS, Ctrl
    /// elsewhere) — most menu accelerators are just this plus a key.
    pub const fn primary(key: Key) -> Self {
        Self::new(Modifiers::PRIMARY, key)
    }

    /// The conventional "app shortcut" modifier plus Shift — the usual
    /// second-tier accelerator (Save As…, Redo, …).
    pub const fn primary_shift(key: Key) -> Self {
        Self::new(Modifiers::PRIMARY_SHIFT, key)
    }
}

/// Modifier keys held down for an [`Accelerator`].
///
/// `primary` is the platform's conventional "this is an app shortcut"
/// modifier — ⌘ on macOS, Ctrl on Windows/Linux — matching `muda`'s own
/// `accelerator::CMD_OR_CTRL` and egui's `Modifiers::COMMAND`. Plain
/// `Ctrl`/`Cmd` as distinct, non-interchangeable modifiers aren't exposed
/// here: none of the menus this crate was built for need them, and adding
/// them only when a real use case shows up keeps the conversion in
/// `native.rs`/`fallback.rs` exhaustive instead of guessing at a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub primary: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        primary: false,
        shift: false,
        alt: false,
    };
    pub const PRIMARY: Self = Self {
        primary: true,
        shift: false,
        alt: false,
    };
    pub const PRIMARY_SHIFT: Self = Self {
        primary: true,
        shift: true,
        alt: false,
    };
}

/// The non-modifier half of an [`Accelerator`].
///
/// Deliberately small: only the keys the menus this crate was built for
/// (Sidestage, mictrace, RCPMix) actually bind. `#[non_exhaustive]` so more
/// can be added later without a breaking change; extend the two `match`es
/// in `native.rs::to_muda_accelerator` and `fallback.rs::to_egui_key`
/// alongside any new variant.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Comma,
    Period,
    Minus,
    Equals,
    Slash,
}

/// A clickable menu entry bound to an app-defined action `A`.
///
/// `A` is whatever enum the app already matches native-menu clicks on today
/// (e.g. the existing `MenuAction` in each of the three apps this crate
/// replaces `native_menu.rs` in) — `Clone + Eq + Hash` so it can be both a
/// click-dispatch value and a `HashMap` key for [`crate::AppMenu::set_enabled`]/
/// [`crate::AppMenu::set_checked`].
#[derive(Debug, Clone)]
pub struct ItemDef<A> {
    pub label: String,
    pub action: A,
    pub accelerator: Option<Accelerator>,
    pub enabled: bool,
    /// `None`: a plain item. `Some(checked)`: a checkable item (`muda`'s
    /// `CheckMenuItem` / an egui checkbox-style button), initially in this
    /// state.
    pub checked: Option<bool>,
}

impl<A> ItemDef<A> {
    /// A plain, enabled, unchecked item with no accelerator — the common
    /// case. Chain `accelerator`/`disabled`/`checkable` to change any of
    /// that.
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Self {
            label: label.into(),
            action,
            accelerator: None,
            enabled: true,
            checked: None,
        }
    }

    #[must_use]
    pub fn accelerator(mut self, accelerator: Accelerator) -> Self {
        self.accelerator = Some(accelerator);
        self
    }

    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Makes this a checkable item (`muda`'s `CheckMenuItem`, an egui
    /// checkbox-styled button), initially `checked`.
    #[must_use]
    pub fn checkable(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }
}

/// The OS-provided items `muda::PredefinedMenuItem` knows how to build —
/// the standard macOS app-menu contents (About/Services/Hide/Quit) and the
/// standard Edit menu (Undo/Redo/Cut/Copy/Paste/Select All), pulled out as
/// their own [`EntryDef`] variant because they have no app-defined action
/// `A`: macOS wires them straight to a native selector (`terminate:`,
/// `copy:`, …) and no `MenuEvent` for them ever reaches this crate, exactly
/// like the `muda`-only code this crate replaces.
///
/// On the fallback (in-window egui) backend only [`Predefined::Quit`] does
/// anything — it closes the viewport. The rest have no cross-platform
/// in-window equivalent (Hide/Services/Minimize are window-manager/macOS
/// concepts, and Copy/Cut/Paste/Undo/Redo/Select All already work in a
/// focused egui text field without a menu item) and are simply not drawn
/// there — see `fallback.rs`'s render function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Predefined {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Minimize,
    Maximize,
    Fullscreen,
    CloseWindow,
    BringAllToFront,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// One entry in a [`SubmenuDef`]'s list — a leaf item, a separator, a
/// nested submenu, or one of `muda`'s OS-predefined items.
#[derive(Debug, Clone)]
pub enum EntryDef<A> {
    Item(ItemDef<A>),
    Separator,
    Submenu(SubmenuDef<A>),
    Predefined(Predefined),
}

impl<A> From<ItemDef<A>> for EntryDef<A> {
    fn from(item: ItemDef<A>) -> Self {
        Self::Item(item)
    }
}

impl<A> From<SubmenuDef<A>> for EntryDef<A> {
    fn from(submenu: SubmenuDef<A>) -> Self {
        Self::Submenu(submenu)
    }
}

impl<A> From<Predefined> for EntryDef<A> {
    fn from(predefined: Predefined) -> Self {
        Self::Predefined(predefined)
    }
}

/// One top-level or nested menu — a label plus its entries.
#[derive(Debug, Clone)]
pub struct SubmenuDef<A> {
    pub label: String,
    pub enabled: bool,
    pub entries: Vec<EntryDef<A>>,
}

impl<A> SubmenuDef<A> {
    pub fn new(label: impl Into<String>, entries: impl IntoIterator<Item = EntryDef<A>>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            entries: entries.into_iter().collect(),
        }
    }
}

/// A whole menu bar: an ordered list of top-level [`SubmenuDef`]s.
///
/// On macOS, `muda` renders the *first* submenu with the running app's own
/// name regardless of the label given here — the same "first submenu
/// appended becomes the app menu" convention the `native_menu.rs` this
/// crate replaces already relied on. Put the app menu (About/Settings…/
/// Quit, or `Predefined::About`/`Predefined::Quit`) first for that to look
/// right; every other backend just uses the label as given.
#[derive(Debug, Clone)]
pub struct MenuBarDef<A> {
    pub menus: Vec<SubmenuDef<A>>,
}

impl<A> MenuBarDef<A> {
    pub fn new(menus: impl IntoIterator<Item = SubmenuDef<A>>) -> Self {
        Self {
            menus: menus.into_iter().collect(),
        }
    }
}

// Written by hand rather than `#[derive(Default)]`: the derive would add an
// `A: Default` bound that an empty `Vec<SubmenuDef<A>>` never actually
// needs, forcing every action enum using `MenuBarDef::default()` (e.g. in
// tests) to implement `Default` too.
impl<A> Default for MenuBarDef<A> {
    fn default() -> Self {
        Self { menus: Vec::new() }
    }
}

impl<A: Clone + Eq + Hash> MenuBarDef<A> {
    /// Every [`ItemDef`] in this definition, depth-first — what every
    /// backend walks once to build its click-dispatch table (`native.rs`'s
    /// `ids`/`handles` maps, `fallback.rs`'s `state` map), and what tests
    /// use to check a definition was assembled as intended without needing
    /// a backend at all.
    pub fn items(&self) -> Vec<&ItemDef<A>> {
        let mut out = Vec::new();
        for submenu in &self.menus {
            collect_items(&submenu.entries, &mut out);
        }
        out
    }

    /// The [`ItemDef`] bound to `action`, if this definition has one —
    /// `Eq` on `A`, so the first match wins if an app accidentally reuses
    /// an action for two items.
    pub fn find(&self, action: &A) -> Option<&ItemDef<A>> {
        self.items().into_iter().find(|item| &item.action == action)
    }
}

fn collect_items<'a, A>(entries: &'a [EntryDef<A>], out: &mut Vec<&'a ItemDef<A>>) {
    for entry in entries {
        match entry {
            EntryDef::Item(item) => out.push(item),
            EntryDef::Submenu(submenu) => collect_items(&submenu.entries, out),
            EntryDef::Separator | EntryDef::Predefined(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Action {
        About,
        Settings,
        New,
    }

    fn sample() -> MenuBarDef<Action> {
        MenuBarDef::new([
            SubmenuDef::new(
                "App",
                [
                    ItemDef::new("About", Action::About).into(),
                    EntryDef::Separator,
                    ItemDef::new("Settings…", Action::Settings)
                        .accelerator(Accelerator::primary(Key::Comma))
                        .into(),
                    EntryDef::Predefined(Predefined::Quit),
                ],
            ),
            SubmenuDef::new(
                "File",
                [ItemDef::new("New", Action::New)
                    .accelerator(Accelerator::primary(Key::N))
                    .into()],
            ),
        ])
    }

    #[test]
    fn items_walks_every_submenu_depth_first() {
        let def = sample();
        let labels: Vec<&str> = def.items().iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["About", "Settings…", "New"]);
    }

    #[test]
    fn find_looks_up_by_action() {
        let def = sample();
        let settings = def.find(&Action::Settings).expect("Settings item exists");
        assert_eq!(settings.label, "Settings…");
        assert_eq!(settings.accelerator, Some(Accelerator::primary(Key::Comma)));
        assert!(def.find(&Action::New).is_some());
    }

    #[test]
    fn nested_submenus_are_walked_too() {
        let def: MenuBarDef<Action> = MenuBarDef::new([SubmenuDef::new(
            "Top",
            [SubmenuDef::new("Nested", [ItemDef::new("New", Action::New).into()]).into()],
        )]);
        assert_eq!(def.items().len(), 1);
        assert_eq!(def.find(&Action::New).unwrap().label, "New");
    }

    #[test]
    fn item_builder_defaults() {
        let item = ItemDef::new("Groups", Action::New);
        assert!(item.enabled);
        assert_eq!(item.checked, None);
        assert_eq!(item.accelerator, None);

        let item = ItemDef::new("Groups", Action::New)
            .disabled()
            .checkable(true);
        assert!(!item.enabled);
        assert_eq!(item.checked, Some(true));
    }
}
