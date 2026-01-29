use super::config::{PanelConfig, ClockConfig};
use super::hyprland::{HyprWorkspace, HyprClient, TrayItem, TrayIconPayload};
use super::hyprland::{hyprctl_dispatch_workspace, hyprctl_dispatch_focus_address, hyprctl_dispatch_close_address};
use super::hypr_config::HyprConfig;
use super::controls::ControlsUi;
use super::tray::activate_sni_item;
use gtk4 as gtk;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::thread;

#[derive(Clone)]
#[allow(dead_code)]
pub struct WorkspacesUi {
    root: gtk::Box,
    num_box: gtk::Box,
    buttons: Rc<RefCell<HashMap<i32, gtk::Button>>>,
    name_label: gtk::Label,
    icon: gtk::Image,
    floating_icon: gtk::Image,
    config: super::config::HyprlandWorkspacesConfig,
    ws_id2name: Rc<RefCell<HashMap<i32, String>>>,
    monitor_name: String,
    workspace_rules: Rc<RefCell<Vec<super::config::HyprWorkspaceRule>>>,
    ws_nums: Rc<RefCell<Vec<i32>>>,
    hypr_config: HyprConfig,
}

impl WorkspacesUi {
    pub fn new(config: super::config::HyprlandWorkspacesConfig, monitor_name: String) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        root.set_widget_name("hyprland-workspaces");
        
        let num_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        num_box.set_widget_name("hyprland-workspaces");
        
        let name_label = gtk::Label::new(None);
        name_label.set_widget_name("hyprland-workspaces-name");
        
        let icon = gtk::Image::new();
        icon.set_widget_name("hyprland-workspaces-icon");
        
        let floating_icon = gtk::Image::new();
        
        let mut hypr_config = HyprConfig::new();
        // Try to load Hyprland config, but don't fail if it doesn't exist
        let _ = hypr_config.parse_config("hyprland.conf");
        
        Self {
            root,
            num_box,
            buttons: Rc::new(RefCell::new(HashMap::new())),
            name_label,
            icon,
            floating_icon,
            config,
            ws_id2name: Rc::new(RefCell::new(HashMap::new())),
            monitor_name,
            workspace_rules: Rc::new(RefCell::new(hypr_config.get_workspace_rules())),
            ws_nums: Rc::new(RefCell::new(Vec::new())),
            hypr_config,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_workspaces(&self, workspaces: Vec<HyprWorkspace>, active_id: i32) {
        // Don't show workspaces if disabled in config
        if !self.config.show_workspaces {
            return;
        }

        let mut buttons = match self.buttons.try_borrow_mut() {
            Ok(b) => b,
            Err(_) => return,
        };

        let mut to_remove = Vec::new();
        for (id, btn) in buttons.iter() {
            if !workspaces.iter().any(|w| w.id == *id) {
                self.root.remove(btn);
                to_remove.push(*id);
            }
        }
        for id in to_remove {
            buttons.remove(&id);
        }

        // Limit number of workspaces if configured
        let mut workspaces = workspaces;
        if workspaces.len() > self.config.num_ws {
            workspaces.sort_by_key(|w| w.id);
            workspaces.truncate(self.config.num_ws);
        }

        for ws in workspaces {
            // Filter workspaces by monitor if configured
            if !self.config.show_workspaces_from_all_outputs && ws.monitor != self.monitor_name {
                continue;
            }

            // Skip empty workspaces if config says so
            if !self.config.show_empty && ws.windows == 0 {
                continue;
            }

            // Skip inactive workspaces if config says so
            if !self.config.show_inactive_workspaces && ws.id != active_id {
                continue;
            }

            let btn = if let Some(btn) = buttons.get(&ws.id) {
                btn.clone()
            } else {
                let btn = gtk::Button::new();
                btn.set_widget_name("hyprland-workspace");

                // Create button content based on config
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                
                // Add icon if enabled
                if self.config.show_icon {
                    let icon = gtk::Image::new();
                    icon.set_widget_name("hyprland-workspace-icon");
                    icon.set_pixel_size(self.config.image_size);
                    icon.set_icon_size(gtk::IconSize::Normal);
                    // TODO: Set actual workspace icon when available
                    content.append(&icon);
                }

                // Add name/number label
                let label_text = if self.config.show_name {
                    if ws.name.trim().is_empty() {
                        ws.id.to_string()
                    } else {
                        // Truncate name if it exceeds configured length
                        let name = ws.name.clone();
                        if name.len() > self.config.name_length {
                            name.chars().take(self.config.name_length).collect::<String>() + "..."
                        } else {
                            name
                        }
                    }
                } else {
                    ws.id.to_string()
                };

                let label = gtk::Label::new(Some(&label_text));
                label.set_widget_name("hyprland-workspace-name");
                content.append(&label);

                btn.set_child(Some(&content));

                let id_for_click = ws.id;
                btn.connect_clicked(move |_| {
                    thread::spawn(move || {
                        let _ = hyprctl_dispatch_workspace(id_for_click);
                    });
                });

                self.root.append(&btn);
                buttons.insert(ws.id, btn.clone());
                btn
            };

            if ws.id == active_id {
                btn.set_widget_name("task-box-focused");
                btn.add_css_class("active");
            } else {
                btn.set_widget_name("task-box");
                btn.remove_css_class("active");
            }
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct TaskbarUi {
    root: gtk::Box,
    buttons: Rc<RefCell<HashMap<String, gtk::Button>>>,
    workspace_labels: Rc<RefCell<HashMap<i32, gtk::Label>>>,
    clients: Rc<RefCell<Vec<HyprClient>>>,
    active_address: Rc<RefCell<String>>,
    last_workspace_count: Rc<RefCell<i32>>,
}

impl TaskbarUi {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.set_widget_name("hyprland-taskbar");
        Self {
            root,
            buttons: Rc::new(RefCell::new(HashMap::new())),
            workspace_labels: Rc::new(RefCell::new(HashMap::new())),
            clients: Rc::new(RefCell::new(Vec::new())),
            active_address: Rc::new(RefCell::new(String::new())),
            last_workspace_count: Rc::new(RefCell::new(0)),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_clients(&self, clients: Vec<HyprClient>) {
        let mut clients_ref = match self.clients.try_borrow_mut() {
            Ok(c) => c,
            Err(_) => return,
        };
        
        *clients_ref = clients;
        self.update_taskbar();
    }

    pub fn set_active_address(&self, address: String) {
        let mut active_ref = match self.active_address.try_borrow_mut() {
            Ok(a) => a,
            Err(_) => return,
        };
        
        if *active_ref == address {
            return;
        }
        
        *active_ref = address.clone();
        self.update_taskbar();
    }

    fn update_taskbar(&self) {
        let clients = match self.clients.try_borrow() {
            Ok(c) => c,
            Err(_) => return,
        };

        let active_address = match self.active_address.try_borrow() {
            Ok(a) => a,
            Err(_) => return,
        };

        // Group clients by workspace
        let mut workspace_groups: HashMap<i32, Vec<HyprClient>> = HashMap::new();
        for client in clients.iter() {
            workspace_groups.entry(client.workspace.id).or_insert_with(Vec::new).push(client.clone());
        }

        // Sort workspaces
        let mut workspace_ids: Vec<i32> = workspace_groups.keys().copied().collect();
        workspace_ids.sort();

        self.update_workspace_labels(&workspace_ids, &workspace_groups);
        self.update_client_buttons(&workspace_groups, &active_address);
    }

    fn update_workspace_labels(&self, workspace_ids: &[i32], groups: &HashMap<i32, Vec<HyprClient>>) {
        let mut labels = match self.workspace_labels.try_borrow_mut() {
            Ok(l) => l,
            Err(_) => return,
        };

        // Clear existing labels
        for label in labels.values() {
            self.root.remove(label);
        }
        labels.clear();

        for &ws_id in workspace_ids {
            let label = gtk::Label::new(Some(&ws_id.to_string()));
            label.set_widget_name("hyprland-task-workspace");
            
            // Add styling for workspace with clients
            if let Some(tasks) = groups.get(&ws_id) {
                if !tasks.is_empty() {
                    label.add_css_class("has-clients");
                }
            }
            
            self.root.append(&label);
            labels.insert(ws_id, label);
        }
    }

    fn update_client_buttons(&self, groups: &HashMap<i32, Vec<HyprClient>>, active_address: &str) {
        let mut buttons = match self.buttons.try_borrow_mut() {
            Ok(b) => b,
            Err(_) => return,
        };
        
        for (ws_id, tasks) in groups {
            for c in tasks {
                let _btn = if let Some(btn) = buttons.get(&c.address) {
                    // Update existing button content
                    self.update_button_content(btn, c, active_address);
                    btn.clone()
                } else {
                    // Create new button
                    let btn = self.create_client_button(c, active_address);
                    buttons.insert(c.address.clone(), btn.clone());
                    
                    // Find the right position to insert this button
                    self.insert_button_at_position(*ws_id, &c.address, &btn);
                    btn
                };
            }
        }
    }

    fn insert_button_at_position(&self, _workspace_id: i32, _client_address: &str, button: &gtk::Button) {
        // Simplified insertion - just append at the end for now
        // TODO: Implement proper positioning when needed
        self.root.append(button);
    }

    fn create_client_button(&self, client: &HyprClient, active_address: &str) -> gtk::Button {
        let btn = gtk::Button::new();
        btn.set_widget_name("hyprland-task");
        
        // Set up click handlers
        let address_for_click = client.address.clone();
        btn.connect_clicked(move |_| {
            let address_for_click = address_for_click.clone();
            thread::spawn(move || {
                let _ = hyprctl_dispatch_focus_address(&address_for_click);
            });
        });

        // Middle-click closes the window
        let address_for_close = client.address.clone();
        let middle = gtk::GestureClick::new();
        middle.set_button(2);
        middle.connect_released(move |_, _, _, _| {
            let address_for_close = address_for_close.clone();
            thread::spawn(move || {
                let _ = hyprctl_dispatch_close_address(&address_for_close);
            });
        });
        btn.add_controller(middle);
        
        // Set initial content and state
        self.update_button_content(&btn, client, active_address);
        
        btn
    }

    fn update_button_content(&self, btn: &gtk::Button, client: &HyprClient, active_address: &str) {
        // Update tooltip
        let tooltip = if client.class.trim().is_empty() {
            client.title.clone()
        } else if client.title.trim().is_empty() {
            client.class.clone()
        } else {
            format!("{} - {}", client.class, client.title)
        };
        btn.set_tooltip_text(Some(&tooltip));

        // Update active state
        if client.address == active_address {
            btn.add_css_class("active");
        } else {
            btn.remove_css_class("active");
        }

        // Create button content
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        
        // Add application icon
        let icon = gtk::Image::new();
        icon.set_widget_name("hyprland-task-icon");
        icon.set_pixel_size(16);
        icon.set_icon_size(gtk::IconSize::Normal);
        
        // Try to get icon from theme
        if gtk::IconTheme::default().has_icon(&client.class) {
            icon.set_icon_name(Some(&client.class));
        } else {
            // Fallback to a generic icon
            icon.set_icon_name(Some("application-x-executable"));
        }
        
        content.append(&icon);

        // Add title label
        let label = gtk::Label::new(Some(&client.title));
        label.set_widget_name("hyprland-task-title");
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(20);
        content.append(&label);

        btn.set_child(Some(&content));
    }
}

#[derive(Clone)]
pub struct TrayUi {
    root: gtk::Box,
    items: Rc<RefCell<Vec<TrayItem>>>,
    buttons: Rc<RefCell<HashMap<String, gtk::Button>>>,
}

impl TrayUi {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.set_widget_name("tray");
        Self {
            root,
            items: Rc::new(RefCell::new(Vec::new())),
            buttons: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_items(&self, items: Vec<TrayItem>) {
        let mut items_ref = match self.items.try_borrow_mut() {
            Ok(i) => i,
            Err(_) => return,
        };
        
        *items_ref = items.clone();
        self.rebuild_buttons(&items);
    }

    pub fn update_item_icon(&self, item: &TrayItem, icon: &TrayIconPayload) {
        let buttons = match self.buttons.try_borrow_mut() {
            Ok(b) => b,
            Err(_) => return,
        };

        if let Some(btn) = buttons.get(&item.as_registration_string()) {
            match icon {
                TrayIconPayload::IconName(name) => {
                    if gtk::IconTheme::default().has_icon(name) {
                        let img = gtk::Image::from_icon_name(name);
                        img.set_pixel_size(16);
                        img.set_icon_size(gtk::IconSize::Normal);
                        btn.set_child(Some(&img));
                    }
                }
                TrayIconPayload::Pixmap(_pixmaps) => {
                    // For now, just use a fallback icon for pixmap data
                    // TODO: Implement proper ARGB32 to texture conversion
                    if gtk::IconTheme::default().has_icon("image-x-generic") {
                        let img = gtk::Image::from_icon_name("image-x-generic");
                        img.set_pixel_size(16);
                        img.set_icon_size(gtk::IconSize::Normal);
                        btn.set_child(Some(&img));
                    }
                }
                TrayIconPayload::None => {}
            }
        }
    }

    fn rebuild_buttons(&self, _items: &[TrayItem]) {
        let mut buttons = match self.buttons.try_borrow_mut() {
            Ok(b) => b,
            Err(_) => return,
        };

        // Clear existing buttons
        for btn in buttons.values() {
            self.root.remove(btn);
        }
        buttons.clear();

        let items = match self.items.try_borrow() {
            Ok(i) => i,
            Err(_) => return,
        };

        for item in items.iter() {
            let key = item.as_registration_string();
            let btn = gtk::Button::new();
            btn.set_widget_name("tray-item");

            let img = gtk::Image::from_icon_name("image-x-generic");
            img.set_pixel_size(16);
            img.set_icon_size(gtk::IconSize::Normal);
            btn.set_child(Some(&img));

            let item_for_click = item.clone();
            btn.connect_clicked(move |_| {
                let item_for_click = item_for_click.clone();
                thread::spawn(move || {
                    let _ = activate_sni_item(&item_for_click);
                });
            });

            self.root.append(&btn);
            buttons.insert(key, btn);
        }
    }
}

pub fn build_clock(cfg: &ClockConfig) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    if !cfg.root_css_name.is_empty() {
        root.set_widget_name(&cfg.root_css_name);
    }
    
    // Make root expand and center content
    root.set_hexpand(true);
    root.set_halign(gtk::Align::Center);

    let label = gtk::Label::new(None);
    if !cfg.css_name.is_empty() {
        label.set_widget_name(&cfg.css_name);
    }
    
    // Center the label itself
    label.set_halign(gtk::Align::Center);

    let format = cfg.format.clone();
    let interval = cfg.interval;

    let update_label = move |lbl: &gtk::Label| {
        let now = chrono::Local::now();
        lbl.set_text(&now.format(&format).to_string());
    };

    update_label(&label);
    let label_for_timer = label.clone();

    glib::timeout_add_seconds_local(interval, move || {
        update_label(&label_for_timer);
        glib::ControlFlow::Continue
    });

    root.append(&label);
    root
}

pub fn instantiate_module(
    panel: &PanelConfig,
    name: &str,
    tray: Option<&TrayUi>,
    workspaces: Option<&WorkspacesUi>,
    taskbar: Option<&TaskbarUi>,
    controls: Option<&ControlsUi>,
) -> gtk::Widget {
    if name == "clock" {
        return build_clock(&panel.clock).upcast();
    }

    if name == "hyprland-workspaces" {
        if let Some(workspaces) = workspaces {
            return workspaces.widget();
        }
    }

    if name == "hyprland-taskbar" {
        if let Some(taskbar) = taskbar {
            return taskbar.widget();
        }
        if let Some(workspaces) = workspaces {
            return workspaces.widget();
        }
    }

    if name == "tray" {
        if let Some(tray) = tray {
            return tray.widget();
        }
    }

    if name == "controls" {
        if let Some(controls) = controls {
            return controls.widget();
        }
    }

    if name.starts_with("button-") {
        // Handle specific button types
        if name == "button-omarchy" {
            let button = gtk::Button::new();
            button.set_widget_name("button-omarchy");
            
            // Create button content with icon
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            
            // Add icon if available
            if gtk::IconTheme::default().has_icon("view-grid") {
                let icon = gtk::Image::from_icon_name("view-grid");
                icon.set_pixel_size(16);
                icon.set_icon_size(gtk::IconSize::Normal);
                content.append(&icon);
            }
            
            // Add label
            let label = gtk::Label::new(Some("Menu"));
            content.append(&label);
            
            button.set_child(Some(&content));
            button.set_tooltip_text(Some("omarchy-menu"));
            
            button.connect_clicked(move |_| {
                thread::spawn(|| {
                    let _ = std::process::Command::new("omarchy-menu").spawn();
                });
            });
            
            return button.upcast();
        }
        
        // Generic button for other button-* types
        let button = gtk::Button::new();
        button.set_widget_name(name);
        button.set_label(name);
        return button.upcast();
    }

    // Fallback: create a label with the module name
    let label = gtk::Label::new(Some(name));
    label.set_widget_name(name);
    label.upcast()
}
