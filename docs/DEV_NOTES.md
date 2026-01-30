# Developer notes (nwg-panel-rs)

This folder tracks the ongoing Rust rewrite of **nwg-panel**. It's mostly prepared with the Windsurf AI assistant.

## Goals and scope (current)

- **Target compositor:** Hyprland only (for now). Other compositors will be added later.
- **UI toolkit:** GTK4.
- **Layering:** gtk4-layer-shell (Wayland layer-shell protocol).
- **Theming:** optional integration with Omarchy; keep nwg-panel config/style as compatible as practical.
- **Tray:** StatusNotifierWatcher over DBus (icons-only; DBusMenu not implemented).

## Repository layout

- `nwg-panel-rs/` — Rust rewrite lives here.
  - `Cargo.toml` — Rust dependencies
  - `src/main.rs` — current implementation
  - `src/modules/` — module implementations
    - `config.rs` — configuration parsing
    - `hyprland.rs` — Hyprland IPC integration
    - `tray.rs` — system tray integration
    - `ui.rs` — UI components and rendering
  - `docs/DEV_NOTES.md` — this document

## Build & run

From the repo root:

```bash
cargo run
```

### System dependencies

You need the GTK4 and layer-shell development/runtime libraries available on the system:

- `gtk4`
- `gtk4-layer-shell`

(Exact package names vary by distro.)

## Config compatibility

We intentionally read the existing nwg-panel config format:

- Config directory: `~/.config/nwg-panel/`
- Default panel config file: `~/.config/nwg-panel/config`
- Default style file: `~/.config/nwg-panel/style.css`

CLI flags:

- `-c/--config <filename>` (default: `config`)
- `-s/--style <filename>` (default: `style.css`)

### Parsed keys (currently)

Per panel entry (JSON object in top-level array), we currently parse:

- `name`
- `css-name`
- `layer` (`background|bottom|top|overlay`)
- `position` (`top|bottom|left|right`)
- `height`
- `margin-top`, `margin-bottom`, `margin-left`, `margin-right`
- `exclusive-zone` (default: `true`)
- `modules-left`, `modules-center`, `modules-right`
- `controls` (upstream-compatible): accepts `"left"|"right"|"off"` (string) or a legacy object form
- `controls-settings` block (see below)
- `clock` block (see below)

Other keys are ignored for now.

### Module instantiation (current)

Modules are referenced by string name in `modules-left/center/right`.

- `clock` is implemented as a real widget.
- `tray` is implemented (icons-only, best-effort).
- `hyprland-workspaces` is implemented.
- `hyprland-taskbar` is implemented (basic; focus/close actions are best-effort).
- `button-omarchy` is implemented.
- Other `button-*` modules currently become a placeholder `gtk::Button`.
- Everything else becomes a placeholder `gtk::Label` with its widget name set.

Controls:

- Upstream config uses `controls: "left|right|off"` and `controls-settings: { ... }`.
- If `controls` is `left`/`right`, we create the controls widget and place it in that box.
- If `controls` appears in `modules-*`, we still enable it, but avoid duplicating it.
- The current controls implementation includes a caret-triggered popover with controls details.

## Implemented: layer-shell window setup

Each panel entry spawns one GTK4 window.

We mirror the Python implementation’s key layer-shell settings:

- `init_layer_shell()`
- `set_namespace(Some("nwg-panel"))`
- `auto_exclusive_zone_enable()` if `exclusive-zone` is `true`
- `set_layer(...)` based on config `layer`
- anchors based on config `position`
- margins from config

## Implemented: theming (Omarchy + nwg-panel style)

The CSS loading order is:

1. Omarchy theme CSS (optional):
   - `~/.config/omarchy/current/theme/gtk.css`
2. User style.css (nwg-panel compatible):
   - `~/.config/nwg-panel/style.css` (or `-s` file)

### CSS loading details

We load both Omarchy and user CSS using `CssProvider::load_from_path(...)`.

Why:

- GTK parser warnings now reference real filenames/line numbers (e.g. `style.css:41`) instead of `<data>:...`.

Notes:

- If your GTK theme imports `libadwaita.css`/`libadwaita-tweaks.css` from `~/.config/gtk-4.0/`, you can satisfy it without sudo by creating empty placeholder files:
  - `~/.config/gtk-4.0/libadwaita.css`
  - `~/.config/gtk-4.0/libadwaita-tweaks.css`

## Implemented: config + CSS hot reload (debounced) + safe fallback

We watch the config and style files and trigger a debounced rebuild:

- Config: `~/.config/nwg-panel/config` (or `-c`)
- CSS: `~/.config/nwg-panel/style.css` (or `-s`)

Behavior:

- Rebuilds are debounced to avoid rapid rebuild loops while editors are saving.
- If a new config fails to parse, the app keeps the **last-known-good UI** and shows a visible warning.

## Implemented: visible config error indicator

When a config reload fails, we show a small warning indicator on the panel:

- Tooltip headline: `Config error`
- Full parser error on the next line

The indicator clears automatically once a valid config is loaded.

## Implemented: controls dropdown (popover)

The controls module contains a caret button that opens a small popover. The popover content respects `controls-settings.components` ordering and can include:

- Brightness slider
- Volume slider
- Battery info

Runtime command backends (best-effort fallbacks):

- Brightness: `light` (preferred) or `brightnessctl`
- Volume: `pamixer` (preferred) or `pactl`
- Battery: `upower`

## Implemented: clock module

A minimal clock widget is implemented using `chrono` + GLib timers.

Config keys used:

- `clock.format` (default: `%H:%M`)
- `clock.interval` (seconds, default: `1`)
- `clock.css-name` (applied to label widget name)
- `clock.root-css-name` (applied to container widget name)

Updates happen on the GTK thread via:

- `glib::timeout_add_seconds_local(interval, ...)`

## Implemented: Hyprland “backend pipeline” (minimal)

This is currently a proof-of-architecture.

- A background thread periodically runs:
  - `hyprctl -j activewindow` (extract `title` and `address`)
  - `hyprctl -j workspaces`
  - `hyprctl -j activeworkspace`
  - `hyprctl -j clients`
- It sends updates to the GTK thread via `crossbeam-channel`.
- The GTK thread drains messages via a periodic `glib::timeout_add_local` callback.

This validates the “poll in background, update on GTK thread” pattern.

## Implemented: tray (SNI over DBus, icons-only, best effort)

We implement a minimal **Status Notifier** host. The goal is “icons appear”, not full feature parity.

- We export a DBus service:
  - name: `org.kde.StatusNotifierWatcher`
  - path: `/StatusNotifierWatcher`
  - interface: `org.kde.StatusNotifierWatcher`

- The watcher collects registered items and forwards updates to GTK via `crossbeam-channel`.

- Items are stored as `(service, path)`:
  - Supports arguments like `"org.example.App"` (defaults path to `/StatusNotifierItem`)
  - Supports arguments like `"org.example.App/SomePath"` (path becomes `/SomePath`)
  - The path-only form `"/StatusNotifierItem"` **is not supported yet** (requires access to caller unique name).

- The `tray` module widget:
  - renders a horizontal container with widget name `tray`
  - creates a `gtk::Button` per item
  - fetches `org.kde.StatusNotifierItem.IconName` and renders `gtk::Image::from_icon_name(...)`
  - `IconPixmap` is not implemented yet
  - on click, calls `org.kde.StatusNotifierItem.Activate(0, 0)` (best effort)

- Placement:
  - if the config contains `"tray"` anywhere, it is **forced into the right box** and not duplicated elsewhere.

- Removal:
  - not implemented yet

- Icon refresh:
  - the watcher periodically re-fetches `IconName` and pushes updates to the GTK thread (best effort).

## Release builds / prebuilt binaries

### Warning-free builds

We keep `cargo build --release` warning-free.

Some structs include extra fields for future parity; these are intentionally kept and may use targeted `#[allow(dead_code)]`.

### Release profile

`Cargo.toml` includes release profile settings to shrink binaries:

- LTO enabled
- `strip = true`
- `panic = abort`

### GitHub Actions + cargo-binstall

We publish GitHub Release artifacts that can be installed using `cargo-binstall`.

- Workflow: `.github/workflows/release.yml`
- Trigger: push tag `vX.Y.Z`
- Artifact format: `nwg-panel-rs-<version>-<target>.tar.gz`
- `Cargo.toml` includes `[package.metadata.binstall]` to point at release assets.

## Recent improvements (2025-01-27)

### Hyprland Taskbar Refinements
- **Performance**: Replaced complete rebuilds with efficient incremental updates
- **Visual Design**: 
  - Added proper application icon lookup from GTK icon theme
  - Improved spacing and layout with tighter 4px spacing
  - Enhanced fallback icons for applications without theme icons
- **Layout Improvements**:
  - Maintains workspace grouping with better visual separation
  - Smart button insertion and positioning
  - Active window highlighting with proper CSS class management
- **Functionality Enhancements**:
  - Enhanced window information display (title, class, workspace)
  - Better sorting (active windows first, then by title)
  - Robust error handling with defensive programming
- **Code Quality**: Modular design with focused helper methods and proper borrow management

### Button Implementation
- **Functional button-omarchy**: Implemented button that runs `omarchy-menu` command
- **Visual Design**: Icon + label with proper tooltip and theme-aware styling
- **Non-blocking execution**: Uses background threads to prevent UI freezing
- **Extensible framework**: Foundation for future button types

### Stability Improvements
- **Panic Prevention**: Added defensive borrowing with `try_borrow()` to prevent crashes
- **Error Handling**: Graceful handling of concurrent access and edge cases
- **Memory Management**: Reduced widget creation/destruction cycles

## Known issues / current rough edges

- Tray is **partial**:
  - DBusMenu is not implemented.
  - Some tray items may not appear/update correctly (no item signals yet; icons refreshed best-effort).
  - Some apps register tray items using the path-only form; those will currently be ignored.
- `style.css` still may produce GTK warnings depending on syntax; we reduce them best-effort.
- Output/monitor selection is not implemented yet (`output` / `monitor` are parsed but unused).
- Some button types remain placeholders (except `button-omarchy`).

## Codebase modularization (COMPLETED)

The monolithic `src/main.rs` has been successfully refactored into focused modules:

### New module structure:
```
src/
├── main.rs (minimal entry point with CLI parsing and app initialization)
└── modules/
    ├── mod.rs (module declarations)
    ├── config.rs (configuration structs and JSON parsing)
    ├── hyprland.rs (Hyprland IPC, workspace/client types, and polling)
    ├── ui.rs (UI components: WorkspacesUi, TaskbarUi, TrayUi, clock, module instantiation)
    ├── tray.rs (StatusNotifierItem DBus handling)
    └── theme.rs (CSS loading and theme management)
```

### Benefits achieved:
- **Separation of concerns**: Each module has a single responsibility
- **Maintainability**: Easier to locate and modify specific functionality
- **Testability**: Individual modules can be tested in isolation
- **Code organization**: Clear hierarchy following Rust conventions
- **Reduced complexity**: `main.rs` now focuses only on application wiring

### Updated imports:
- All imports now use the `modules::` prefix (e.g., `modules::config::PanelConfig`)
- Internal module imports use `super::` for sibling module access
- Module declarations centralized in `modules/mod.rs`

### Compatibility preserved:
- All existing functionality remains intact
- Configuration file format unchanged
- CLI interface unchanged
- All modules compile and run successfully

## Next planned steps

- Tray robustness:
  - implement signals from `StatusNotifierItem` (optional)
- Implement remaining button types:
  - Add support for configurable button commands
  - Extend button framework for custom icons and labels
- Output/monitor selection implementation:
  - Implement proper monitor filtering for workspace display
  - Add support for multi-monitor setups
