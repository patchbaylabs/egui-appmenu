//! Windows backend: `muda` attached to the eframe window's HWND with
//! [`muda::Menu::init_for_hwnd`].
//!
//! Unlike macOS, this needs two extra things macOS doesn't:
//!
//! 1. **The HWND itself.** eframe's `CreationContext` implements
//!    `raw_window_handle::HasWindowHandle`; `RawWindowHandle::Win32` carries
//!    the `HWND` as a `NonZeroIsize`, which is exactly what
//!    `Menu::init_for_hwnd` wants.
//! 2. **`TranslateAcceleratorW` in the Win32 message loop**, or `muda`
//!    accelerators never fire (`muda`'s own documented constraint, see
//!    [`Menu::init_for_hwnd`]'s doc comment and its example). eframe/winit
//!    don't call it for us, so [`install_event_loop_hook`] wires it in via
//!    winit's `EventLoopBuilderExtWindows::with_msg_hook` — call it on
//!    `NativeOptions` *before* `eframe::run_native`, i.e. before any
//!    `AppMenu` exists to supply the `HACCEL` the hook needs. That's the
//!    reason for `HACCEL_SLOT`: the hook closure and `Backend::new` run at
//!    different times (event-loop setup vs. app construction, both still on
//!    the main thread) and need a shared place to hand the table off.
//!
//! [`Menu::init_for_hwnd`]: muda::Menu::init_for_hwnd

use std::hash::Hash;
use std::sync::atomic::{AtomicIsize, Ordering};

use muda::MenuTheme;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::def::MenuBarDef;
use crate::native::{self, BuiltMenu};

/// The current menu's `HACCEL` (see [`muda::Menu::haccel`]), or `0` before
/// any `Backend` has been built. `install_event_loop_hook`'s closure reads
/// this on every Win32 message; `Backend::new` writes it once, right after
/// attaching the menu. An `isize` `HACCEL` is a plain handle value with no
/// destructor, so a bare atomic (no `Mutex`, no generic `A`) is enough —
/// this deliberately doesn't need to know anything about the app's action
/// type.
static HACCEL_SLOT: AtomicIsize = AtomicIsize::new(0);

/// Wires `TranslateAcceleratorW` into winit's Win32 message loop so `muda`
/// keyboard accelerators fire. Call this on `NativeOptions` before
/// `eframe::run_native` — see the module doc comment for why the ordering
/// matters. A no-op on every other target, so app code can call it
/// unconditionally (see [`crate::install_event_loop_hook`], which does
/// exactly that).
pub(crate) fn install_event_loop_hook(options: &mut eframe::NativeOptions) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, TranslateAcceleratorW};
    use winit::platform::windows::EventLoopBuilderExtWindows;

    options.event_loop_builder = Some(Box::new(|builder| {
        builder.with_msg_hook(|msg| {
            let haccel = HACCEL_SLOT.load(Ordering::Relaxed);
            if haccel == 0 {
                return false;
            }
            let msg = msg as *const MSG;
            // Safety: winit hands us a valid `*const MSG` for the duration
            // of this call, and `haccel` is either 0 (checked above) or a
            // `HACCEL` `Backend::new` obtained from a `muda::Menu` it kept
            // alive for the app's lifetime.
            unsafe { TranslateAcceleratorW((*msg).hwnd, haccel as _, msg) != 0 }
        });
    }));
}

pub(crate) struct Backend<A> {
    built: BuiltMenu<A>,
    hwnd: isize,
    theme: egui::Theme,
}

impl<A: Clone + Eq + Hash + Send + Sync + 'static> Backend<A> {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>, def: MenuBarDef<A>) -> Self {
        let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => panic!("egui-appmenu: eframe's CreationContext has no Win32 window handle"),
        };

        let built = native::build(&def, cc.egui_ctx.clone());
        let theme = cc.egui_ctx.theme();
        // Safety: `hwnd` came from eframe's own `CreationContext` for the
        // window this app is currently creating, which is exactly the
        // window `Menu::init_for_hwnd_with_theme` requires.
        unsafe {
            built
                .menu
                .init_for_hwnd_with_theme(hwnd, to_muda_theme(theme))
                .expect("attaching the menu to the window");
        }
        HACCEL_SLOT.store(built.menu.haccel(), Ordering::Relaxed);

        Self { built, hwnd, theme }
    }

    pub(crate) fn poll(&mut self) -> Option<A> {
        self.built.poll()
    }

    /// No-op: like macOS, Windows draws the menu bar itself. Present so app
    /// code can call `AppMenu::ui` unconditionally without a per-platform
    /// `cfg`.
    pub(crate) fn ui(&mut self, _ui: &mut egui::Ui) -> Vec<A> {
        Vec::new()
    }

    pub(crate) fn set_enabled(&mut self, action: &A, enabled: bool) {
        self.built.set_enabled(action, enabled);
    }

    pub(crate) fn set_checked(&mut self, action: &A, checked: bool) {
        self.built.set_checked(action, checked);
    }

    /// Re-themes the menu bar when egui's theme (light/dark) changes —
    /// `muda` only offers this on Windows (`Menu::set_theme_for_hwnd`);
    /// macOS's menu bar follows the system appearance on its own. Cheap to
    /// call every frame, but only does anything on an actual change.
    pub(crate) fn sync_theme(&mut self, ctx: &egui::Context) {
        let theme = ctx.theme();
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        // Safety: `self.hwnd` is the same window handle `new` validated.
        unsafe {
            let _ = self
                .built
                .menu
                .set_theme_for_hwnd(self.hwnd, to_muda_theme(theme));
        }
    }
}

fn to_muda_theme(theme: egui::Theme) -> MenuTheme {
    match theme {
        egui::Theme::Dark => MenuTheme::Dark,
        egui::Theme::Light => MenuTheme::Light,
    }
}
