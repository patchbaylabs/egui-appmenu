# AGENTS.md

## Project Overview

`egui-appmenu` is a Rust **library crate** (edition 2024) — not an app. It lets an
eframe/egui app describe its menu bar once, as plain data (`MenuBarDef<A>`), and get:

- a **native** menu bar on macOS (`muda` + `Menu::init_for_nsapp`) and Windows
  (`muda` + `Menu::init_for_hwnd`, plus the `TranslateAcceleratorW` wiring that
  makes keyboard accelerators fire), or
- an **in-window `egui::MenuBar`** everywhere else (Linux, and any platform forced
  onto it for tests).

One `AppMenu<A>` wraps whichever backend is active behind the same small API. `A` is
the app's own action enum (`Clone + Eq + Hash + Send + Sync + 'static`): clicks and
shortcuts come back as `A`, and `set_enabled`/`set_checked` key off `A`.

Pinned versions — do not bump independently:

- eframe/egui `0.35`
- muda `0.19`
- winit `=0.30.13` (exact pin to match the winit eframe 0.35 vendors — the
  `with_msg_hook` hook only works if this crate and eframe share one winit)

### Module map

| File | Role |
| --- | --- |
| `src/def.rs` | The menu bar as pure data (`MenuBarDef`, `SubmenuDef`, `EntryDef`, `ItemDef`, `Accelerator`, `Key`, `Modifiers`, `Predefined`). Touches no `muda`, draws nothing. |
| `src/menu.rs` | `AppMenu<A>` — the public wrapper. Holds a `Backend` enum (not a `cfg` type alias, because `new_in_window` can force the fallback at runtime on any OS). |
| `src/fallback.rs` | In-window `egui::MenuBar` backend. Compiled on every platform (it's always usable via `AppMenu::new_in_window`). |
| `src/native.rs` | Shared `muda` tree-building for macOS/Windows: `BuiltMenu`, the click-event handler, `set_enabled`/`set_checked` dispatch. `#[cfg]`'d out on other targets. |
| `src/macos.rs` | macOS backend: `init_for_nsapp` (process-global, no window handle). |
| `src/windows.rs` | Windows backend: `init_for_hwnd` + `install_event_loop_hook` (accelerator wiring via `HACCEL_SLOT`). |
| `src/lib.rs` | Crate root: public re-exports, `install_event_loop_hook`, the doctested usage example, and the "Lessons" notes. |

## Setup Commands

- Requires Rust 1.85+ (edition 2024). No rust-toolchain file, no env vars, no config
  files, no system dependencies beyond a normal Rust install.
- Build: `cargo build`
- `Cargo.lock` is gitignored — this is a library; don't commit lockfile churn.

## Development Workflow

There is no dev server; this is a library crate. Fast iteration loop:

- `cargo check` — quickest type-check of the host platform's code paths.
- `cargo doc --no-deps --open` — preview the docs; `src/lib.rs`'s doc comment is the
  canonical usage example and doubles as the crate's front page.

Every backend compiles conditionally, so from macOS you only see `def.rs`,
`fallback.rs`, `menu.rs`, `macos.rs`, and `native.rs` by default. To type-check the
Windows-only code (`src/windows.rs`, the `windows-sys`/`winit` hook) without a
Windows machine:

```bash
cargo check --target x86_64-pc-windows-msvc   # check only — no linking needed
```

`fallback.rs` needs no special target: it compiles on every platform.
`windows-sys`/`raw-window-handle`/`winit` deps are `cfg`-gated to Windows in
`Cargo.toml`, so other targets never pull them in.

## Testing Instructions

- Run all tests: `cargo test` (12 total today)
- Unit tests only: `cargo test --lib` (11)
- Doc tests only: `cargo test --doc` (1 — the `no_run` example in `src/lib.rs`)
- One test by name: `cargo test find_looks_up_by_action`
- Test file locations: inline `#[cfg(test)]` modules at the bottom of
  `src/def.rs` (4), `src/fallback.rs` (6), and `src/menu.rs` (1). No `tests/`
  directory.

Patterns that matter here:

- **Never construct the native backends in a test.** `muda` is main-thread-only and
  panics off the real process main thread — which a `cargo test` harness thread
  isn't. Use `AppMenu::new_in_window` (or `fallback::Backend::new` directly) in
  tests; `native.rs` is `#[cfg]`-compiled out under test for the same reason, so it
  has no test coverage by design.
- Tests build an `eframe::CreationContext` without a real window via
  `eframe::CreationContext::_new_kittest(egui::Context::default())` — copy that
  helper, don't invent a new one.
- Add or update tests for anything you change in `def.rs`, `fallback.rs`, or
  `menu.rs`. The public API (`AppMenu`) has a runtime test that exercises
  `new_in_window` end-to-end; keep it passing.

## Code Style

- Formatting is standard rustfmt, default settings (no `rustfmt.toml`): run
  `cargo fmt` after editing.
- Lint: `cargo clippy --all-targets` — must pass with zero warnings.
- Doc comments in this crate explain **why**, not just what: platform constraints,
  `muda` requirements, the crash that motivated a design, pinning rationale. When
  you touch a load-bearing decision, update the module doc that records it
  (`src/lib.rs` has a "Lessons this crate encodes" section; `native.rs`'s
  `BuiltMenu` doc is the canonical one for item-lifetime). New public items get a
  doc comment; keep the `no_run` example in `src/lib.rs` compiling.
- Error handling conventions: `muda` calls that can't fail in practice use
  `.expect("...")` with a message saying what was attached/built; poisoned locks
  are recovered with `.unwrap_or_else(|p| p.into_inner())` (see `native.rs`).
- `Key` is `#[non_exhaustive]` by design — adding a variant is fine, but extend
  **both** conversions alongside it: `native.rs::to_muda_accelerator` and
  `fallback.rs::to_egui_key`. Same for `Modifiers`: only `primary`/`shift`/`alt`
  exist on purpose (no raw Ctrl/Cmd); keep both conversions exhaustive.
- `unsafe` lives only in `src/windows.rs` (HWND/`HACCEL` FFI, theme setting) and
  every block carries a `// Safety:` comment. Keep it that way.

## Build and Deployment

- `cargo build --release` — artifacts land in `target/` (gitignored).
- No CI pipeline yet (no `.github/workflows`); the checks below are the de facto
  gate.
- Publishing is standard `cargo publish` territory; version is `0.1.0`, not yet on
  crates.io.

## Pull Request Guidelines

The repo is young — no commit history to pattern-match yet. Before declaring any
change done, all of these must pass:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo doc --no-deps
```

If you touched Windows-only code, also run the cross-check from the Development
Workflow section; you cannot verify it on a mac any other way.

## Critical Constraints (read before touching backends)

1. **`muda` is main-thread-only.** `AppMenu::new` must run on the real main thread
   (where `eframe::run_native`'s builder closure runs). It panics from test
   threads — that's why `new_in_window` exists, and why this crate can't auto-switch
   under a downstream crate's `cfg(test)`.
2. **`BuiltMenu`'s `all: Vec<Box<dyn Any>>` is load-bearing.** Every `muda` item
   must be kept alive for the menu's whole lifetime; dropping one after attaching
   crashes inside `muda`'s macOS click handler on the next click (confirmed against
   a real crash report, not a hunch). Do not "simplify" it away.
3. **Clicks go through `MenuEvent::set_event_handler`, not `receiver()`**, and the
   handler must call `ctx.request_repaint()` — a native click happens with the
   pointer outside the window, so no egui input would otherwise schedule a frame
   and the click would sit unseen.
4. **Windows accelerators need `TranslateAcceleratorW` in the Win32 message loop**,
   wired by `install_event_loop_hook` (via `HACCEL_SLOT` and winit's
   `with_msg_hook`). Apps call it on `NativeOptions` *before* `eframe::run_native`,
   on every platform — it's a no-op except on Windows, so no `cfg` at call sites.
5. **The first submenu becomes the macOS app menu**, rendered with the running
   app's name regardless of its label. Docs and examples assume the app menu
   (About/Settings/Quit) comes first.
6. **Backend choice is an enum, not a `cfg` type alias** (`src/menu.rs`): the
   fallback must be constructible on macOS/Windows too, so `new_in_window` and
   tests can force it. Keep all `AppMenu` methods matching over the full enum with
   per-variant `#[cfg]`s.
