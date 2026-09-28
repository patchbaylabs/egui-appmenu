<div align="center">

# egui-appmenu

**Describe your menu bar once — native on macOS and Windows, in-window everywhere else.**

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue)](LICENSE-MIT)
[![eframe/egui](https://img.shields.io/badge/eframe%2Fegui-0.35-orange)](https://crates.io/crates/eframe)
[![Rust edition](https://img.shields.io/badge/rust-edition_2024-orange)](https://doc.rust-lang.org/edition-guide/)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-green)](#platform-support)

[Features](#features) · [Installation](#installation) · [Usage](#usage) · [Platform notes](#platform-notes) · [Testing](#testing)

</div>

---

Describe an [eframe]/[egui] app's menu bar once, as plain data, and get:

- a real **native** menu bar on **macOS** ([`muda`]'s `init_for_nsapp`, process-global, no window handle needed) and **Windows** (`init_for_hwnd`, plus the `TranslateAcceleratorW` wiring keyboard accelerators need to actually fire), or
- an **in-window `egui::MenuBar`** everywhere else, with the same shortcut hints and the same keys handled through egui's own input.

One `AppMenu<A>` wraps whichever backend is active behind the same small API, where `A` is whatever action enum your app already matches menu clicks on: native backends deliver clicks through `poll()`, the in-window one through `ui()`, and keyboard accelerators fire on both.

## Features

- **Native menu bars** on macOS and Windows, via [`muda`]
- **In-window fallback** elsewhere — same definition, same shortcut hints, keys handled through egui's own input
- **Accelerators that fire everywhere** — one `Accelerator` value per item works on every backend; on Windows, `TranslateAcceleratorW` is wired into the Win32 message loop for you
- **Predefined OS items** — About, Services, Hide, Quit, and the standard Edit items (Undo/Redo/Cut/Copy/Paste/Select All), the same set `muda`'s `PredefinedMenuItem` builds
- **Typed actions** — items bind to your own action enum, not string IDs; clicks and shortcuts come back as that type
- **Runtime updates** — `set_enabled` / `set_checked`, keyed by action
- **Theme sync** — the Windows menu bar follows egui's light/dark theme; macOS follows the system appearance on its own

## Platform support

| Platform | Backend | Menu bar |
| --- | --- | --- |
| macOS | [`muda`] (`init_for_nsapp`) | Native, process-global |
| Windows | [`muda`] (`init_for_hwnd` + accelerator hook) | Native, per-window |
| Linux and others | `egui::MenuBar` | In-window |

`AppMenu::new_in_window` forces the in-window backend on any platform — which is also how tests get around `muda`'s main-thread requirement (see [Testing](#testing)).

## Installation

```bash
cargo add egui-appmenu
```

Or track `main` from git:

```toml
[dependencies]
egui-appmenu = { git = "https://github.com/patchbaylabs/egui-appmenu" }
```

> [!NOTE]
> Pinned to eframe/egui 0.35 and muda 0.19, the versions this crate was extracted from three existing apps' `native_menu.rs` modules to share.

## Usage

```rust
use eframe::egui;
use egui_appmenu::{Accelerator, AppMenu, EntryDef, ItemDef, Key, MenuBarDef, Predefined, SubmenuDef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Action {
    About,
    Settings,
}

fn menu_def() -> MenuBarDef<Action> {
    // The *first* submenu becomes the macOS app menu automatically.
    MenuBarDef::new([SubmenuDef::new(
        "MyApp",
        [
            ItemDef::new("About MyApp", Action::About).into(),
            EntryDef::Separator,
            ItemDef::new("Settings…", Action::Settings)
                .accelerator(Accelerator::primary(Key::Comma))
                .into(),
            EntryDef::Predefined(Predefined::Quit),
        ],
    )])
}

struct MyApp {
    menu: AppMenu<Action>,
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // `muda` is main-thread-only, and a `cargo test` harness thread
        // isn't the real main thread `eframe::run_native` uses in
        // production — so tests construct the in-window backend instead.
        // See "Testing" below.
        #[cfg(test)]
        let menu = AppMenu::new_in_window(cc, menu_def());
        #[cfg(not(test))]
        let menu = AppMenu::new(cc, menu_def());
        Self { menu }
    }

    fn handle(&mut self, action: Action) {
        match action {
            Action::About => { /* … */ }
            Action::Settings => { /* … */ }
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.menu.sync_theme(ui.ctx()); // no-op except on Windows
        while let Some(action) = self.menu.poll() {
            self.handle(action);
        }
        // On macOS/Windows the OS already drew the menu bar; `ui`
        // returns nothing there, so it's simplest to only call it where
        // there's an in-window bar to draw.
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        for action in self.menu.ui(ui) {
            self.handle(action);
        }
        ui.label("Hello from MyApp!");
    }
}

fn main() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    egui_appmenu::install_event_loop_hook(&mut options); // no-op except on Windows
    eframe::run_native("MyApp", options, Box::new(|cc| Ok(Box::new(MyApp::new(cc)))))
}
```

The three moving parts, in order:

1. `install_event_loop_hook(&mut options)` in `main`, before `eframe::run_native` — wires Windows accelerators in; a no-op everywhere else.
2. `AppMenu::new(cc, menu_def())` in your app's constructor — builds and attaches the bar.
3. Once per frame in `eframe::App::ui`: `sync_theme(ui.ctx())` (no-op except on Windows), then drain clicks — `poll()` in a loop on macOS/Windows, or `ui(ui)` where the bar is drawn in-window.

## Defining a menu

- `MenuBarDef::new([...])` — the whole menu bar, an ordered list of `SubmenuDef`s.
- `SubmenuDef::new(label, [...])` — one top-level or nested menu.
- `EntryDef` — one entry in a submenu: `Item`, `Separator`, a nested `Submenu`, or a `Predefined` OS item.
- `ItemDef::new(label, action)` — a clickable item bound to your own action type, with `.accelerator(...)`, `.disabled()`, and `.checkable(initial)` builder methods.

Accelerators:

- `Accelerator::primary(Key::S)` — the platform's conventional app-shortcut modifier plus a key: ⌘S on macOS, Ctrl+S elsewhere.
- `Accelerator::primary_shift(Key::S)` — the same plus Shift, the usual second tier (Save As…, Redo…).
- `Accelerator::new(Modifiers, Key)` — full control (`primary`/`shift`/`alt`). The `Key` set is deliberately small (letters, digits, a few punctuation keys) and `#[non_exhaustive]`, so it can grow without a breaking change.

Every `ItemDef` needs `A: Clone + Eq + Hash + Send + Sync + 'static` — an app's existing menu-action enum already satisfies this in practice. `A` is both the value handed back from a click and the key `AppMenu::set_enabled`/`set_checked` use to update an item later:

```rust
menu.set_enabled(&Action::Save, false); // gray an item out
menu.set_checked(&Action::DarkMode, true); // tick a `.checkable()` item
```

> [!TIP]
> On macOS, `muda` renders the **first** submenu with the running app's own name regardless of its label — the standard "first menu becomes the app menu" convention. Put your app menu (About, Settings…, Quit) first; every other backend uses the label as given.

## Platform notes

- **macOS**: `muda` + `init_for_nsapp()`. The menu bar follows the system light/dark appearance on its own; `sync_theme` is a no-op.
- **Windows**: `muda` + `init_for_hwnd()` (the HWND comes from eframe's `CreationContext`), themed via `set_theme_for_hwnd` — kept in sync by `sync_theme`, called once per frame. Accelerators need `TranslateAcceleratorW` in the Win32 message loop, wired in by `install_event_loop_hook` through winit's `with_msg_hook`: call it on `NativeOptions` before `eframe::run_native`, on every platform (a no-op except on Windows).
- **Everywhere else** (Linux, and anything built with `AppMenu::new_in_window`): an in-window `egui::MenuBar`, drawn and polled through `AppMenu::ui`. Only `Predefined::Quit` has an in-window equivalent (it closes the viewport); the other predefined items are macOS/window-manager concepts or already work in a focused egui text field without a menu item, so nothing is drawn for them there.

> [!IMPORTANT]
> `muda` menus and their click events are main-thread-only. Build the menu on the same thread `eframe::run_native`'s builder closure runs on — which is exactly where `AppMenu::new` is meant to be called: your app's `YourApp::new(cc: &eframe::CreationContext<'_>)`.

## Testing

`AppMenu::new` picks a backend from `target_os` alone — a build-time property this crate can see. Whether the *calling* code is on the real main thread or a `cargo test` harness thread is a runtime fact about your crate, not this one, so this crate can't switch to the fallback backend under your `cfg(test)` the way it switches on the OS.

> [!WARNING]
> Any test that constructs your app type directly must call `AppMenu::new_in_window` instead of `AppMenu::new` — a test harness thread isn't `muda`'s required real main thread, and on macOS/Windows `AppMenu::new` panics with `muda`'s own "can only be created on the main thread" message.

`new_in_window` always builds the in-window backend, regardless of platform, and never touches `muda` — safe to call from any thread, and also a reasonable way to force the in-window bar on macOS/Windows outside tests. The `#[cfg(test)]` branch in the example above is the exact shape the three apps this crate was extracted from already used for their own native-menu construction.

[eframe]: https://crates.io/crates/eframe
[egui]: https://crates.io/crates/egui
[muda]: https://crates.io/crates/muda
