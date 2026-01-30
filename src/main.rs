mod modules;

use anyhow::Context;
use clap::Parser;
use gdk4 as gdk;
use gdk::prelude::*;
use gtk4 as gtk;
use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;
use notify::{RecursiveMode, Watcher};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;
use std::time::Duration;
use crossbeam_channel as cb;
use glib;

use modules::config::{load_panels_from_path, ControlsCompat, PanelConfig};
use modules::hyprland::{send_hyprland_snapshot, spawn_hyprland_poller, AppMsg};
use modules::ui::{WorkspacesUi, TaskbarUi, TrayUi, instantiate_module};
use modules::tray::spawn_sni_watcher;
use modules::theme::load_user_css_if_exists;
use modules::controls::{ControlsUi, ControlsMsg};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "config")]
    config: String,

    #[arg(short, long, default_value = "style.css")]
    style: String,
}

fn select_gdk_monitor(display: &gdk::Display, panel: &PanelConfig) -> Option<gdk::Monitor> {
    let want_monitor = panel.monitor.trim();
    let want_output = panel.output.trim();

    if want_monitor.is_empty() && want_output.is_empty() {
        return None;
    }

    let monitors = display.monitors();
    let n = monitors.n_items();

    for idx in 0..n {
        let obj = monitors.item(idx)?;
        let mon = obj.downcast::<gdk::Monitor>().ok()?;

        let connector = mon.connector().unwrap_or_default();
        let manufacturer = mon.manufacturer().unwrap_or_default();
        let model = mon.model().unwrap_or_default();

        let haystack = format!(
            "{}\n{}\n{}",
            connector, manufacturer, model
        );

        if !want_monitor.is_empty() {
            // Upstream `monitor` is typically a human-readable monitor description.
            if haystack.contains(want_monitor) {
                return Some(mon);
            }
        }

        if !want_output.is_empty() {
            // Upstream `output` is typically a connector-like identifier (e.g. HDMI-A-1, DP-1).
            if connector.as_str() == want_output || haystack.contains(want_output) {
                return Some(mon);
            }
        }
    }

    None
}

fn expand_panels_for_all_outputs(display: &gdk::Display, panels: Vec<PanelConfig>) -> Vec<PanelConfig> {
    let mut out = Vec::new();

    let monitors = display.monitors();
    let n = monitors.n_items();

    for panel in panels {
        if panel.output.trim() == "All" && panel.monitor.trim().is_empty() {
            for idx in 0..n {
                let obj = match monitors.item(idx) {
                    Some(o) => o,
                    None => continue,
                };

                let mon = match obj.downcast::<gdk::Monitor>() {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let connector = mon.connector().unwrap_or_default();
                let connector = connector.to_string();
                if connector.trim().is_empty() {
                    continue;
                }

                let mut clone = panel.clone();
                clone.output = connector.clone();
                if !clone.name.trim().is_empty() {
                    clone.name = format!("{}-{}", clone.name.trim(), connector);
                } else {
                    clone.name = connector;
                }
                out.push(clone);
            }
        } else {
            out.push(panel);
        }
    }

    out
}

fn main() -> anyhow::Result<()> {
    let app = gtk::Application::new(Some("com.github.nwg-panel-rs"), gtk::gio::ApplicationFlags::empty());
    app.connect_activate(build_ui);
    app.run();
    Ok(())
}

fn build_ui(app: &gtk::Application) {
    if let Err(err) = try_build_ui(app) {
        eprintln!("nwg-panel-rs failed to start: {err:#}");
        app.quit();
    }
}

fn try_build_ui(app: &gtk::Application) -> anyhow::Result<()> {
    let args = Args::parse();

    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nwg-panel");

    let display = gdk::Display::default().context("Could not connect to a display")?;

    let style_path = config_dir.join(&args.style);
    let config_path = config_dir.join(&args.config);

    let (app_sender, app_receiver) = cb::unbounded::<AppMsg>();
    let (controls_sender, controls_receiver) = cb::unbounded::<ControlsMsg>();
    spawn_hyprland_poller(app_sender.clone());
    spawn_sni_watcher(app_sender.clone());

    let hypr_snapshot_sender = app_sender.clone();

    let next_sub_id: Rc<Cell<usize>> = Rc::new(Cell::new(1));
    let app_subs: Rc<RefCell<Vec<(usize, cb::Sender<AppMsg>)>>> = Rc::new(RefCell::new(Vec::new()));
    let controls_subs: Rc<RefCell<Vec<(usize, cb::Sender<ControlsMsg>)>>> = Rc::new(RefCell::new(Vec::new()));
    let error_indicators: Rc<RefCell<Vec<(usize, gtk::Widget)>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let app_subs = app_subs.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            while let Ok(msg) = app_receiver.try_recv() {
                if let Ok(mut subs) = app_subs.try_borrow_mut() {
                    subs.retain(|(_, tx)| tx.send(msg.clone()).is_ok());
                }
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let controls_subs = controls_subs.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            while let Ok(msg) = controls_receiver.try_recv() {
                if let Ok(mut subs) = controls_subs.try_borrow_mut() {
                    subs.retain(|(_, tx)| tx.send(msg.clone()).is_ok());
                }
            }
            glib::ControlFlow::Continue
        });
    }

    let windows: Rc<RefCell<Vec<gtk::ApplicationWindow>>> = Rc::new(RefCell::new(Vec::new()));

    let rebuild = {
        let app = app.clone();
        let display = display.clone();
        let style_path = style_path.clone();
        let config_path = config_path.clone();
        let controls_sender = controls_sender.clone();
        let windows = windows.clone();
        let hypr_snapshot_sender = hypr_snapshot_sender.clone();
        let next_sub_id = next_sub_id.clone();
        let app_subs = app_subs.clone();
        let controls_subs = controls_subs.clone();
        let error_indicators = error_indicators.clone();

        move || {
            let set_error = |msg: String| {
                let tooltip = if msg.trim().is_empty() {
                    "Config error".to_string()
                } else {
                    format!("Config error\n{}", msg)
                };
                if let Ok(indicators) = error_indicators.try_borrow() {
                    for (_, w) in indicators.iter() {
                        w.set_tooltip_text(Some(&tooltip));
                        w.set_visible(true);
                    }
                }
            };

            let clear_error = || {
                if let Ok(indicators) = error_indicators.try_borrow() {
                    for (_, w) in indicators.iter() {
                        w.set_visible(false);
                        w.set_tooltip_text(None);
                    }
                }
            };

            if let Err(err) = load_user_css_if_exists(&display, &style_path)
                .with_context(|| format!("Failed to load CSS {}", style_path.display()))
            {
                eprintln!("nwg-panel-rs: reload: {err:#}");
            }

            let panels = match load_panels_from_path(&config_path)
                .with_context(|| format!("Failed loading config {}", config_path.display()))
            {
                Ok(p) => p,
                Err(err) => {
                    // Safe reload: keep last known-good UI alive.
                    eprintln!("nwg-panel-rs: reload: {err:#}");
                    set_error(format!("Config error: {err}"));
                    return;
                }
            };

            let panels = expand_panels_for_all_outputs(&display, panels);

            if panels.is_empty() {
                // Safe reload: don't tear down the existing UI if config is empty.
                eprintln!("nwg-panel-rs: reload: no panels found in config; keeping existing UI");
                set_error("Config error: no panels found".to_string());
                return;
            }

            // Successful config load: now perform the teardown + rebuild.
            clear_error();
            if let Ok(mut subs) = app_subs.try_borrow_mut() {
                subs.clear();
            }
            if let Ok(mut subs) = controls_subs.try_borrow_mut() {
                subs.clear();
            }
            if let Ok(mut indicators) = error_indicators.try_borrow_mut() {
                indicators.clear();
            }
            next_sub_id.set(1);

            // Close existing windows.
            if let Ok(mut ws) = windows.try_borrow_mut() {
                for w in ws.iter() {
                    w.close();
                }
                ws.clear();
            }

            for panel in panels {
                let sub_id = next_sub_id.get();
                next_sub_id.set(sub_id + 1);

                let (win_app_tx, win_app_rx) = cb::unbounded::<AppMsg>();
                let (win_controls_tx, win_controls_rx) = cb::unbounded::<ControlsMsg>();

                if let Ok(mut subs) = app_subs.try_borrow_mut() {
                    subs.push((sub_id, win_app_tx));
                }
                if let Ok(mut subs) = controls_subs.try_borrow_mut() {
                    subs.push((sub_id, win_controls_tx));
                }

                let window = build_panel_window(
                    &app,
                    &display,
                    &panel,
                    sub_id,
                    app_subs.clone(),
                    controls_subs.clone(),
                    error_indicators.clone(),
                    win_app_rx,
                    win_controls_rx,
                    controls_sender.clone(),
                );
                window.present();
                if let Ok(mut ws) = windows.try_borrow_mut() {
                    ws.push(window);
                }
            }

            send_hyprland_snapshot(&hypr_snapshot_sender);
        }
    };

    // Initial build.
    rebuild();

    // Watch config + CSS and rebuild on change (debounced).
    let (reload_tx, reload_rx) = cb::unbounded::<()>();
    let last_reload_event: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));

    {
        let last_reload_event = last_reload_event.clone();
        let rebuild = Rc::new(rebuild);
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut got_event = false;
            while reload_rx.try_recv().is_ok() {
                got_event = true;
            }

            if got_event {
                last_reload_event.set(Some(Instant::now()));
            }

            if let Some(t0) = last_reload_event.get() {
                if t0.elapsed() >= Duration::from_millis(250) {
                    rebuild();
                    last_reload_event.set(None);
                }
            }

            glib::ControlFlow::Continue
        });
    }

    let mut watcher = notify::recommended_watcher({
        let reload_tx = reload_tx.clone();
        let config_path = config_path.clone();
        let style_path = style_path.clone();
        move |res: notify::Result<notify::Event>| {
            let event = match res {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("nwg-panel-rs: watcher error: {err}");
                    return;
                }
            };

            // Most editors write via rename/temp files; we accept any event that touches the
            // target files (or their parent directory reports them).
            let mut matched = false;
            for p in &event.paths {
                if *p == config_path || *p == style_path {
                    matched = true;
                    break;
                }
            }
            if matched {
                let _ = reload_tx.send(());
            }
        }
    })?;

    if let Err(err) = watcher.watch(&config_path, RecursiveMode::NonRecursive) {
        eprintln!("nwg-panel-rs: failed to watch config {}: {err}", config_path.display());
    }
    if let Err(err) = watcher.watch(&style_path, RecursiveMode::NonRecursive) {
        eprintln!("nwg-panel-rs: failed to watch style {}: {err}", style_path.display());
    }

    // Keep watcher alive for the entire process lifetime.
    std::mem::forget(watcher);

    Ok(())
}

fn build_panel_window(
    app: &gtk::Application,
    display: &gdk::Display,
    panel: &PanelConfig,
    subscriber_id: usize,
    app_subs: Rc<RefCell<Vec<(usize, cb::Sender<AppMsg>)>>>,
    controls_subs: Rc<RefCell<Vec<(usize, cb::Sender<ControlsMsg>)>>>,
    error_indicators: Rc<RefCell<Vec<(usize, gtk::Widget)>>>,
    receiver: cb::Receiver<AppMsg>,
    controls_receiver: cb::Receiver<ControlsMsg>,
    controls_sender: cb::Sender<ControlsMsg>,
) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(if panel.name.is_empty() {
            "nwg-panel-rs"
        } else {
            &panel.name
        })
        .build();

    if !panel.css_name.is_empty() {
        window.set_widget_name(&panel.css_name);
    }

    window.init_layer_shell();
    window.set_namespace(Some("nwg-panel"));

    if let Some(mon) = select_gdk_monitor(display, panel) {
        window.set_monitor(Some(&mon));
    } else if !panel.monitor.trim().is_empty() || !panel.output.trim().is_empty() {
        eprintln!(
            "nwg-panel-rs: could not match monitor for panel '{}' (monitor='{}', output='{}')",
            panel.name,
            panel.monitor,
            panel.output
        );
    }

    // Set layer
    match panel.layer.as_str() {
        "background" => window.set_layer(gtk4_layer_shell::Layer::Background),
        "bottom" => window.set_layer(gtk4_layer_shell::Layer::Bottom),
        "top" => window.set_layer(gtk4_layer_shell::Layer::Top),
        "overlay" => window.set_layer(gtk4_layer_shell::Layer::Overlay),
        _ => window.set_layer(gtk4_layer_shell::Layer::Top),
    }

    // Set anchors based on position
    match panel.position.as_str() {
        "top" => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        "bottom" => {
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        "left" => {
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
        "right" => {
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
        _ => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
    }

    // Margins (best-effort; config mostly uses top/bottom).
    window.set_margin(gtk4_layer_shell::Edge::Top, panel.margin_top);
    window.set_margin(gtk4_layer_shell::Edge::Bottom, panel.margin_bottom);
    window.set_margin(gtk4_layer_shell::Edge::Left, panel.margin_left);
    window.set_margin(gtk4_layer_shell::Edge::Right, panel.margin_right);

    // Size: only set height for top/bottom panels for now.
    if panel.height > 0 {
        match panel.position.as_str() {
            "top" | "bottom" => window.set_default_size(-1, panel.height),
            "left" | "right" => window.set_default_size(panel.height, -1),
            _ => window.set_default_size(-1, panel.height),
        }
    }

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    root.set_widget_name("nwg-panel");
    window.set_child(Some(&root));

    // Auto exclusive zone if enabled
    if panel.exclusive_zone {
        window.auto_exclusive_zone_enable();
    }

    // Create layout containers
    let left = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    left.set_widget_name("modules-left");

    let center = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    center.set_hexpand(true);
    center.set_halign(gtk::Align::Center);
    center.set_margin_start(20);
    center.set_margin_end(20);
    center.set_widget_name("modules-center");

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    right.set_halign(gtk::Align::End);
    right.set_hexpand(true);
    right.set_widget_name("modules-right");

    let config_error_icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
    config_error_icon.set_widget_name("config-error");
    config_error_icon.set_pixel_size(16);
    config_error_icon.set_visible(false);
    right.append(&config_error_icon);

    if let Ok(mut indicators) = error_indicators.try_borrow_mut() {
        indicators.push((subscriber_id, config_error_icon.upcast::<gtk::Widget>()));
    }

    let active_title_label = gtk::Label::new(None);
    active_title_label.set_widget_name("active-window-title");
    active_title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let active_title_label_for_update = active_title_label.clone();

    let tray_ui = TrayUi::new();
    let tray_ui_for_update = tray_ui.clone();

    let workspaces_monitor_name = if panel.monitor.trim().is_empty() {
        panel.output.clone()
    } else {
        panel.monitor.clone()
    };
    let workspaces_ui = WorkspacesUi::new(panel.hyprland_workspaces.clone(), workspaces_monitor_name);
    let workspaces_ui_for_update = workspaces_ui.clone();

    let has_tray = panel
        .modules_left
        .iter()
        .chain(panel.modules_center.iter())
        .chain(panel.modules_right.iter())
        .any(|m| m == "tray");

    let has_taskbar = panel
        .modules_left
        .iter()
        .chain(panel.modules_center.iter())
        .chain(panel.modules_right.iter())
        .any(|m| m == "hyprland-taskbar");

    let taskbar_ui = if has_taskbar {
        Some(TaskbarUi::new())
    } else {
        None
    };

    let taskbar_ui_for_update = taskbar_ui.clone();

    // Create controls UI if needed.
    // Upstream config uses `controls: "left|right|off"` and `controls-settings: {...}`.
    let controls_position: Option<String> = match &panel.controls {
        ControlsCompat::Position(pos) => Some(pos.clone()),
        // Legacy/alternative form: allow `controls` to be the settings object.
        ControlsCompat::Settings(_cfg) => None,
        ControlsCompat::None => None,
    };

    let controls_enabled_by_modules = panel
        .modules_left
        .iter()
        .chain(panel.modules_center.iter())
        .chain(panel.modules_right.iter())
        .any(|m| m == "controls");

    let controls_enabled = controls_position
        .as_deref()
        .is_some_and(|p| p == "left" || p == "right")
        || controls_enabled_by_modules
        || matches!(panel.controls, ControlsCompat::Settings(_));

    let controls_cfg = match &panel.controls {
        ControlsCompat::Settings(cfg) => cfg.clone(),
        _ => panel.controls_settings.clone(),
    };

    let controls_ui: Option<ControlsUi> = if controls_enabled {
        Some(ControlsUi::new(controls_cfg, controls_sender))
    } else {
        None
    };

    let controls_ui_for_update = controls_ui.clone();

    let update_source_id = {
        let id = glib::timeout_add_local(Duration::from_millis(200), move || {
        // Process messages safely
        while let Ok(msg) = receiver.try_recv() {
            match msg {
                AppMsg::HyprActiveWindow(title) => {
                    if !title.is_empty() {
                        active_title_label_for_update.set_text(&title);
                    }
                }
                AppMsg::HyprWorkspaces {
                    workspaces,
                    active_id,
                } => {
                    workspaces_ui_for_update.set_workspaces(workspaces, active_id);
                }
                AppMsg::HyprActiveWindowAddress(addr) => {
                    if let Some(taskbar) = taskbar_ui_for_update.as_ref() {
                        taskbar.set_active_address(addr);
                    }
                }
                AppMsg::HyprClients { clients } => {
                    if let Some(taskbar) = taskbar_ui_for_update.as_ref() {
                        taskbar.set_clients(clients);
                    }
                }
                AppMsg::TrayItemsChanged(items) => {
                    tray_ui_for_update.set_items(items);
                }
                AppMsg::TrayIconUpdated { item, icon } => {
                    tray_ui_for_update.update_item_icon(&item, &icon);
                }
            }
        }
        
        // Process controls messages with error handling
        if let Ok(msg) = controls_receiver.try_recv() {
            if let Some(controls) = controls_ui_for_update.as_ref() {
                match msg {
                    ControlsMsg::Brightness(value) => {
                        controls.update_brightness(value);
                    }
                    ControlsMsg::Volume(value, muted) => {
                        controls.update_volume(value, muted);
                    }
                    ControlsMsg::Battery(capacity, _time, charging) => {
                        controls.update_battery(capacity, charging);
                    }
                }
            }
        }
        
        glib::ControlFlow::Continue
        });
        Rc::new(RefCell::new(Some(id)))
    };

    window.connect_close_request({
        let update_source_id = update_source_id.clone();
        let app_subs = app_subs.clone();
        let controls_subs = controls_subs.clone();
        let error_indicators = error_indicators.clone();
        move |_| {
            if let Some(id) = update_source_id.borrow_mut().take() {
                id.remove();
            }

            if let Ok(mut subs) = app_subs.try_borrow_mut() {
                subs.retain(|(id, _)| *id != subscriber_id);
            }
            if let Ok(mut subs) = controls_subs.try_borrow_mut() {
                subs.retain(|(id, _)| *id != subscriber_id);
            }
            if let Ok(mut indicators) = error_indicators.try_borrow_mut() {
                indicators.retain(|(id, _)| *id != subscriber_id);
            }
            glib::Propagation::Proceed
        }
    });

    for m in &panel.modules_left {
        if m == "tray" {
            continue;
        }
        if m == "controls" {
            // Upstream-style controls are placed by `panel.controls`; avoid duplicating.
            continue;
        }
        left.append(&instantiate_module(
            panel,
            m,
            Some(&tray_ui),
            Some(&workspaces_ui),
            taskbar_ui.as_ref(),
            controls_ui.as_ref(),
        ));
    }
    for m in &panel.modules_center {
        if m == "tray" {
            continue;
        }
        if m == "controls" {
            continue;
        }
        center.append(&instantiate_module(
            panel,
            m,
            Some(&tray_ui),
            Some(&workspaces_ui),
            taskbar_ui.as_ref(),
            controls_ui.as_ref(),
        ));
    }
    for m in &panel.modules_right {
        if m == "tray" {
            continue;
        }
        if m == "controls" {
            continue;
        }
        right.append(&instantiate_module(
            panel,
            m,
            Some(&tray_ui),
            Some(&workspaces_ui),
            taskbar_ui.as_ref(),
            controls_ui.as_ref(),
        ));
    }

    if has_tray {
        right.append(&tray_ui.widget());
    }

    // Place controls widget based on upstream-style `controls` value.
    if let Some(controls) = controls_ui.as_ref() {
        match controls_position.as_deref() {
            Some("left") => left.append(&controls.widget()),
            Some("right") => right.append(&controls.widget()),
            _ => {
                // If enabled via module list, default to right.
                if controls_enabled_by_modules {
                    right.append(&controls.widget());
                }
            }
        }
    }

    root.append(&left);
    root.append(&center);
    root.append(&right);

    window
}
