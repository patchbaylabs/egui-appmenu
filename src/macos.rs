//! macOS backend: `muda` attached with [`muda::Menu::init_for_nsapp`], which
//! needs no window handle at all because the macOS menu bar is
//! process-global rather than per-window.

use std::hash::Hash;

use crate::def::MenuBarDef;
use crate::native::{self, BuiltMenu};

pub(crate) struct Backend<A> {
    built: BuiltMenu<A>,
}

impl<A: Clone + Eq + Hash + Send + Sync + 'static> Backend<A> {
    /// `cc` is only used for its `egui_ctx` here (no window handle is
    /// needed to attach a process-global menu bar) — accepted anyway so
    /// every backend's constructor has the same signature and
    /// `AppMenu::new` needs no per-platform call site.
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, def: MenuBarDef<A>) -> Self {
        let built = native::build(&def, cc.egui_ctx.clone());
        built.menu.init_for_nsapp();
        Self { built }
    }

    pub(crate) fn poll(&mut self) -> Option<A> {
        self.built.poll()
    }

    /// No-op: macOS draws the menu bar itself. Present so app code can call
    /// `AppMenu::ui` unconditionally without a per-platform `cfg`.
    pub(crate) fn ui(&mut self, _ui: &mut egui::Ui) -> Vec<A> {
        Vec::new()
    }

    pub(crate) fn set_enabled(&mut self, action: &A, enabled: bool) {
        self.built.set_enabled(action, enabled);
    }

    pub(crate) fn set_checked(&mut self, action: &A, checked: bool) {
        self.built.set_checked(action, checked);
    }

    /// No-op: macOS's menu bar has no separate light/dark setting to sync —
    /// it always follows the system appearance on its own. Present for
    /// parity with the Windows backend, where it matters.
    pub(crate) fn sync_theme(&mut self, _ctx: &egui::Context) {}
}
