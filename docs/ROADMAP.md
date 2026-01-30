 # nwg-panel-rs roadmap
 
 This roadmap is based on upstream **nwg-panel** (Python/GTK3) feature set and the current **nwg-panel-rs** (Rust/GTK4) implementation.
 
 Guiding principles:
 
 - **Hyprland-first** until the basics are solid.
 - Keep **config + CSS compatibility** with upstream where practical (`~/.config/nwg-panel/config`, `style.css`, `-c/-s`).
 - Prefer **stability** over rushing feature parity.
 
 ## Snapshot: upstream vs nwg-panel-rs
 
 Upstream modules include (non-exhaustive):
 
 - `clock`, `playerctl`, `tray` (SNI), `controls`, `menu-start`
 - compositor modules: `sway-taskbar`, `sway-workspaces`, `scratchpad`, `sway-mode`, `hyprland-taskbar`, `hyprland-workspaces`, `hyprland-submap`, `niri-taskbar`
 - misc: `openweather`, `brightness-slider`, `random-wallpaper`, `keyboard-layout`, `dwl-tags`, `pinned`, `button-*`, `executor-*`
 
 nwg-panel-rs currently has:
 
 - **Core**: GTK4 + `gtk4-layer-shell`, one window per panel entry, CSS loading.
 - **Config**: parses a subset of upstream keys (layer/position/margins/exclusive-zone/modules lists; partial module configs).
 - **Hyprland**: `hyprland-workspaces`, `hyprland-taskbar` (basic).
 - **Modules**: `clock`, `controls` (icons + popover sliders/info), `tray` (SNI watcher/host, icons-only, best-effort), `button-omarchy`.
 - **Reload**: config + CSS watch with debounced rebuilds; safe reload keeps last-known-good UI on config parse errors.
 
 ## Milestones
 
 ### M0: Hyprland “daily-usable baseline”
 
 Goal: a panel that can run continuously on Hyprland without breaking user sessions.
 
 - **Outputs / monitors**
   - Actually honor `output` / `monitor` (currently parsed but not applied).
   - Basic multi-monitor behavior and predictable placement.
 - **Config compatibility hardening**
   - Accept and ignore unknown keys without crashing.
   - Keep widget names/CSS names aligned with upstream so common themes work.
 - **Tray robustness (still icons-only)**
   - Support the SNI path-only registration form (requires caller unique name).
   - Handle item removal/unregistration.
   - Reduce polling where possible (prefer signals; keep polling as fallback).
 - **Hyprland workspaces/taskbar correctness**
   - Better filtering by monitor (align with upstream knobs: all-outputs / per-output behavior).
   - More reliable active window tracking and updates.
 - **Controls stability**
   - Make refresh logic more deterministic and configurable (intervals; avoid command spam).
   - The caret-triggered popover is implemented; remaining work is parity/polish.
 - **Reload stability**
   - Keep debounced hot reload robust across multiple windows.
   - Keep last-known-good UI visible on config errors (implemented); add more diagnostics as needed.

 Exit criteria:
 
 - Runs for hours without crashing.
 - Correct monitor placement works for common multi-monitor setups.
 - Tray items appear/disappear reliably for common apps.
 
 ### M1: “Upstream-compatible module surface (Hyprland subset)”
 
 Goal: reduce the number of placeholders by implementing the high-impact upstream modules.
 
 - **button-* (real)**
   - Implement `button-*` config parsing: label, icon, command, click actions.
 - **executor-* (real)**
   - Implement `executor-*` (periodic command execution + text/icon output).
   - Optional signal-triggered refresh (upstream uses real-time signals).
 - **hyprland-submap**
   - Implement `hyprland-submap` display (best-effort parity).
 - **keyboard-layout**
   - Implement keyboard layout indicator for Hyprland (scope: indicator first).
 
 ### M2: Controls UI parity (popups)
 
 Goal: match upstream “controls” UX.
 
 - **Popover / popup behavior parity**
   - Close behavior, focus behavior, and consistent styling.
 - Optional interaction:
   - Scroll to change volume/brightness (configurable).
   - Click actions.
 - Battery details (time remaining/charging state) best-effort.
 
 Notes:
 
 - A caret-triggered popover with brightness/volume sliders and battery info is already implemented.
 - Remaining work here is parity/polish (matching upstream styling/behavior).
 
 ### M3: “Nice-to-have” modules (opt-in)
 
 Goal: implement popular extra modules without locking into complex dependencies.
 
 - `playerctl` (media status + buttons).
 - `brightness-slider` (separate from controls).
 - `openweather` (behind feature flag; avoid hard dependencies when possible).
 - `pinned` (launcher shortcuts).
 
 ### P0: UI polish / GTK warnings (low priority)
 
 Goal: eliminate remaining GTK runtime warnings that may be theme/CSS dependent.
 
 - Investigate `GtkGizmo slider` min-height warnings (likely theme/CSS interactions).
 - Investigate occasional `GtkToggleButton` active-state accounting warnings.
 
 ### M4: Other compositors
 
 Goal: expand beyond Hyprland once the architecture is proven.
 
 - **Sway**: `sway-workspaces`, `sway-taskbar`, `sway-mode`, `scratchpad`.
 - **Niri**: `niri-taskbar`.
 - Decide whether to keep a shared abstraction or separate backends per compositor.
 
 ## Open design questions
 
 - **Backend strategy**: event-driven (socket/listener) vs polling fallback (likely both).
 - **Tray scope**: when to implement `IconPixmap` properly and whether to implement DBusMenu.
 - **Compatibility target**: strict upstream config parity vs “best-effort + documented differences”.

