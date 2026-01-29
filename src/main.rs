mod modules;

use anyhow::Context;
use clap::Parser;
use gdk4 as gdk;
use gtk4 as gtk;
use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::path::PathBuf;
use std::time::Duration;
use crossbeam_channel as cb;
use glib;

use modules::config::{load_panels_from_path, ControlsCompat, PanelConfig};
use modules::hyprland::{spawn_hyprland_poller, AppMsg};
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
    load_user_css_if_exists(&display, &style_path)
        .with_context(|| format!("Failed to load CSS {}", style_path.display()))?;

    let config_path = config_dir.join(&args.config);
    let panels = load_panels_from_path(&config_path)
        .with_context(|| format!("Failed loading config {}", config_path.display()))?;

    if panels.is_empty() {
        eprintln!("nwg-panel-rs: no panels found in config");
    }

    let (sender, receiver) = cb::unbounded::<AppMsg>();
    let (controls_sender, controls_receiver) = cb::unbounded::<ControlsMsg>();
    spawn_hyprland_poller(sender.clone());
    spawn_sni_watcher(sender);

    for panel in panels {
        let window = build_panel_window(
            app,
            &panel,
            receiver.clone(),
            controls_receiver.clone(),
            controls_sender.clone(),
        );
        window.present();
    }

    Ok(())
}

fn build_panel_window(
    app: &gtk::Application,
    panel: &PanelConfig,
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

    glib::timeout_add_local(Duration::from_millis(200), move || {
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
