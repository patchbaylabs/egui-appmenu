//! Describe an eframe/egui app's menu bar once, as data ([`MenuBarDef`]),
//! and get:
//!
//! - a real native menu bar on **macOS** (`muda` + `Menu::init_for_nsapp`,
//!   process-global, no window handle needed) and **Windows** (`muda` +
//!   `Menu::init_for_hwnd`, plus [`install_event_loop_hook`] wiring
//!   `TranslateAcceleratorW` into the Win32 message loop so accelerators
//!   fire), or
//! - an in-window `egui::MenuBar` everywhere else, with the same shortcut
//!   hints and the same keys handled through egui's own input.
//!
//! One [`AppMenu`] wraps whichever of those is active behind the same API —
//! see its doc comment for the two ways an app drives it per frame.
//!
//! **Tests**: build with [`AppMenu::new_in_window`] instead of
//! [`AppMenu::new`] in any test that constructs your app type directly.
//! `muda` requires the real process main thread (its own documented
//! constraint) and panics off it, which a `cargo test` harness thread isn't
//! — unlike `target_os`, that isn't something this crate can detect on your
//! behalf from inside a dependency, so it can't switch automatically the way
//! it does for the OS. `new_in_window` always builds the in-window fallback
//! backend, regardless of platform, and is just as safe to call from a test
//! thread as any other egui code. See the example below and the "Lessons"
//! section for the exact `#[cfg(test)]` shape the three apps this crate was
//! extracted from all already used for this.
//!
//! # Example
//!
//! ```no_run
//! use eframe::egui;
//! use egui_appmenu::{Accelerator, AppMenu, EntryDef, ItemDef, Key, MenuBarDef, Predefined, SubmenuDef};
//!
//! #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
//! enum Action {
//!     About,
//!     Settings,
//! }
//!
//! fn menu_def() -> MenuBarDef<Action> {
//!     // The *first* submenu becomes the macOS app menu automatically.
//!     MenuBarDef::new([SubmenuDef::new(
//!         "MyApp",
//!         [
//!             ItemDef::new("About MyApp", Action::About).into(),
//!             EntryDef::Separator,
//!             ItemDef::new("Settings…", Action::Settings)
//!                 .accelerator(Accelerator::primary(Key::Comma))
//!                 .into(),
//!             EntryDef::Predefined(Predefined::Quit),
//!         ],
//!     )])
//! }
//!
//! struct MyApp {
//!     menu: AppMenu<Action>,
//! }
//!
//! impl MyApp {
//!     fn new(cc: &eframe::CreationContext<'_>) -> Self {
//!         // `muda` needs the real main thread eframe's builder closure
//!         // (and so this function) runs on in production — which a test
//!         // harness thread isn't, so tests that call `MyApp::new` directly
//!         // need the in-window backend instead. See the crate root doc
//!         // comment.
//!         #[cfg(test)]
//!         let menu = AppMenu::new_in_window(cc, menu_def());
//!         #[cfg(not(test))]
//!         let menu = AppMenu::new(cc, menu_def());
//!         Self { menu }
//!     }
//!
//!     fn handle(&mut self, action: Action) {
//!         match action {
//!             Action::About => { /* … */ }
//!             Action::Settings => { /* … */ }
//!         }
//!     }
//! }
//!
//! impl eframe::App for MyApp {
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         self.menu.sync_theme(ui.ctx()); // no-op except on Windows
//!         while let Some(action) = self.menu.poll() {
//!             self.handle(action);
//!         }
//!         // On macOS/Windows the OS already drew the menu bar; `ui`
//!         // returns nothing there, so it's simplest to only call it where
//!         // there's an in-window bar to draw.
//!         #[cfg(not(any(target_os = "macos", target_os = "windows")))]
//!         for action in self.menu.ui(ui) {
//!             self.handle(action);
//!         }
//!         ui.label("Hello from MyApp!");
//!     }
//! }
//!
//! fn main() -> eframe::Result<()> {
//!     let mut options = eframe::NativeOptions::default();
//!     egui_appmenu::install_event_loop_hook(&mut options); // no-op except on Windows
//!     eframe::run_native("MyApp", options, Box::new(|cc| Ok(Box::new(MyApp::new(cc)))))
//! }
//! ```
//!
//! # Lessons this crate encodes
//!
//! - Every `muda` item must be kept alive for the menu's whole lifetime —
//!   dropping one after attaching it crashes inside `muda`'s macOS click
//!   handler the next time it's clicked, confirmed against a real macOS
//!   crash report in the code this crate was extracted from, not just a
//!   hunch (see `native.rs`'s `BuiltMenu` doc comment).
//! - Clicks go through `MenuEvent::set_event_handler`, not
//!   `MenuEvent::receiver()`, and the handler must call
//!   `ctx.request_repaint()`: a native menu click happens with the pointer
//!   outside the app's window, so it generates no egui input, and an
//!   on-demand repainter like egui/eframe would otherwise sit on the click
//!   until something unrelated scheduled a frame.
//! - `muda` is main-thread-only. This crate can pick a backend from
//!   `target_os` alone (that's a property of the build), but "is this
//!   thread the real main thread `eframe::run_native` set up" is a runtime
//!   fact about the *calling* crate's own test harness that a dependency
//!   has no way to see — so unlike the `native_menu.rs` modules this crate
//!   replaces, it can't skip `muda` under `cfg(test)` automatically. Call
//!   [`AppMenu::new_in_window`] instead of [`AppMenu::new`] under your own
//!   crate's `cfg(test)` to get the same effect (see the example above).

mod def;
mod fallback;
#[cfg(target_os = "macos")]
mod macos;
mod menu;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native;
#[cfg(target_os = "windows")]
mod windows;

pub use def::{Accelerator, EntryDef, ItemDef, Key, MenuBarDef, Modifiers, Predefined, SubmenuDef};
pub use menu::AppMenu;

/// Wires `TranslateAcceleratorW` into winit's Win32 message loop so `muda`
/// keyboard accelerators fire on Windows (see `windows.rs`'s module doc
/// comment for why that's needed and how). Call this on `NativeOptions`
/// *before* `eframe::run_native`, on every platform — it does nothing
/// except on Windows, so app code needs no `cfg` around the call.
pub fn install_event_loop_hook(options: &mut eframe::NativeOptions) {
    #[cfg(target_os = "windows")]
    windows::install_event_loop_hook(options);
    #[cfg(not(target_os = "windows"))]
    let _ = options;
}
