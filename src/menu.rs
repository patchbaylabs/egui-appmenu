use std::hash::Hash;

use crate::def::MenuBarDef;
use crate::fallback;
#[cfg(target_os = "macos")]
use crate::macos;
#[cfg(target_os = "windows")]
use crate::windows;

/// Which backend an [`AppMenu`] is actually running — chosen once, in
/// [`AppMenu::new`]/[`AppMenu::new_in_window`], and never changed after.
/// An enum rather than a `cfg`-selected type alias because the choice isn't
/// purely a build-time property of `target_os`: [`AppMenu::new_in_window`]
/// can force the fallback variant on any platform (see its doc comment for
/// why that's needed under `cfg(test)`), so the type has to be able to hold
/// either on macOS/Windows.
enum Backend<A> {
    #[cfg(target_os = "macos")]
    Macos(macos::Backend<A>),
    #[cfg(target_os = "windows")]
    Windows(windows::Backend<A>),
    Fallback(fallback::Backend<A>),
}

/// A running app menu: built once from a [`MenuBarDef`], then polled/drawn
/// every frame.
///
/// The same type and the same methods exist on every platform. macOS and
/// Windows render natively via `muda` and answer [`AppMenu::poll`] from a
/// queue `muda`'s click events fill; every other OS (and anything built with
/// [`AppMenu::new_in_window`]) render an in-window `egui::MenuBar` from
/// [`AppMenu::ui`], which both draws the bar and returns the actions clicked
/// — or triggered by a keyboard shortcut — this frame. An app picks which of
/// the two it calls with the same `cfg(target_os = "macos")`-style split its
/// own code already needs for other platform differences; see the crate
/// root doc comment for the exact pattern.
pub struct AppMenu<A: Clone + Eq + Hash + Send + Sync + 'static> {
    backend: Backend<A>,
}

impl<A: Clone + Eq + Hash + Send + Sync + 'static> AppMenu<A> {
    /// Builds the menu bar described by `def` and attaches it (macOS/
    /// Windows) or prepares it for drawing (every other OS). Call once, from
    /// the main thread, before the first frame — `muda` is main-thread-only,
    /// and that's also where `eframe::run_native`'s builder closure (and so
    /// this call) runs in production.
    ///
    /// **Don't call this from a test that constructs your app type
    /// directly** — use [`Self::new_in_window`] there instead. See the
    /// crate root doc comment.
    ///
    /// `cc` is eframe's `CreationContext`. Windows needs it for the
    /// window's HWND; every other backend only reads `cc.egui_ctx` from it,
    /// but all take one so `AppMenu::new` has a single call site regardless
    /// of platform.
    pub fn new(cc: &eframe::CreationContext<'_>, def: MenuBarDef<A>) -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                backend: Backend::Macos(macos::Backend::new(cc, def)),
            }
        }
        #[cfg(target_os = "windows")]
        {
            Self {
                backend: Backend::Windows(windows::Backend::new(cc, def)),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Self {
                backend: Backend::Fallback(fallback::Backend::new(cc, def)),
            }
        }
    }

    /// Builds the menu bar using the in-window fallback backend, regardless
    /// of platform — safe to call from any thread, since it never touches
    /// `muda`. Use this instead of [`Self::new`] in any test that
    /// constructs your app type directly (`muda` requires the real process
    /// main thread, which a `cargo test` harness thread isn't); it's also a
    /// reasonable way to force the in-window bar on macOS/Windows outside of
    /// tests, if an app ever wants that.
    pub fn new_in_window(cc: &eframe::CreationContext<'_>, def: MenuBarDef<A>) -> Self {
        Self {
            backend: Backend::Fallback(fallback::Backend::new(cc, def)),
        }
    }

    /// Drains at most one pending native menu click — call in a loop once
    /// per frame on macOS/Windows, the same "drain a queue each frame"
    /// shape the `native_menu.rs` modules this crate replaces used. Always
    /// returns `None` on the in-window fallback backend, which reports
    /// clicks through [`Self::ui`] instead.
    pub fn poll(&mut self) -> Option<A> {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            Backend::Macos(backend) => backend.poll(),
            #[cfg(target_os = "windows")]
            Backend::Windows(backend) => backend.poll(),
            Backend::Fallback(backend) => backend.poll(),
        }
    }

    /// Draws the in-window menu bar and returns the actions clicked — or
    /// triggered by a keyboard shortcut — this frame. A no-op that returns
    /// an empty `Vec` on the native macOS/Windows backends, where the OS
    /// draws the menu bar itself; call it unconditionally if that's simpler
    /// than a `cfg` at the call site, or skip the call there the way the
    /// three apps this crate was extracted from skip their old egui
    /// fallback menu bar on macOS.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<A> {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            Backend::Macos(backend) => backend.ui(ui),
            #[cfg(target_os = "windows")]
            Backend::Windows(backend) => backend.ui(ui),
            Backend::Fallback(backend) => backend.ui(ui),
        }
    }

    /// Enables or disables the item bound to `action`. A no-op if no item
    /// in the definition uses that action.
    pub fn set_enabled(&mut self, action: &A, enabled: bool) {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            Backend::Macos(backend) => backend.set_enabled(action, enabled),
            #[cfg(target_os = "windows")]
            Backend::Windows(backend) => backend.set_enabled(action, enabled),
            Backend::Fallback(backend) => backend.set_enabled(action, enabled),
        }
    }

    /// Checks or unchecks the item bound to `action`. A no-op if that item
    /// isn't checkable (wasn't built with [`crate::ItemDef::checkable`]) or
    /// no item uses that action.
    pub fn set_checked(&mut self, action: &A, checked: bool) {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            Backend::Macos(backend) => backend.set_checked(action, checked),
            #[cfg(target_os = "windows")]
            Backend::Windows(backend) => backend.set_checked(action, checked),
            Backend::Fallback(backend) => backend.set_checked(action, checked),
        }
    }

    /// Re-themes the native menu bar if egui's light/dark theme changed
    /// since the last call (or since `new`). Only the Windows backend needs
    /// this — `muda` only exposes a menu-bar theme API there; macOS's menu
    /// bar follows the system appearance on its own, and the fallback
    /// backend's in-window bar already follows the app's own `egui::Visuals`.
    /// A no-op everywhere else, so it's safe to call every frame
    /// unconditionally.
    pub fn sync_theme(&mut self, ctx: &egui::Context) {
        match &mut self.backend {
            #[cfg(target_os = "macos")]
            Backend::Macos(backend) => backend.sync_theme(ctx),
            #[cfg(target_os = "windows")]
            Backend::Windows(backend) => backend.sync_theme(ctx),
            Backend::Fallback(backend) => backend.sync_theme(ctx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::{ItemDef, SubmenuDef};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Action {
        One,
    }

    fn cc() -> eframe::CreationContext<'static> {
        eframe::CreationContext::_new_kittest(egui::Context::default())
    }

    /// `new_in_window` must be safe to call for real (not just compile) on
    /// every platform and from any thread — this is the constructor every
    /// downstream app's tests are expected to use, so it's the one thing in
    /// this module that actually needs runtime coverage rather than just a
    /// `no_run` doc example.
    #[test]
    fn new_in_window_builds_and_responds_through_the_public_api() {
        let def = MenuBarDef::new([SubmenuDef::new(
            "Menu",
            [ItemDef::new("One", Action::One).checkable(false).into()],
        )]);
        let mut menu = AppMenu::new_in_window(&cc(), def);
        assert_eq!(menu.poll(), None);
        menu.set_enabled(&Action::One, false);
        menu.set_checked(&Action::One, true);
        menu.sync_theme(&egui::Context::default());
    }
}
