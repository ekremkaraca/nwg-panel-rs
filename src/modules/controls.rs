use gtk4 as gtk;
use gdk4 as gdk;
use gtk::prelude::*;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossbeam_channel as cb;
use anyhow::{Context, Result};

use super::config::ControlsConfig;

#[derive(Debug, Clone)]
pub enum ControlsMsg {
    Brightness(i32),
    Volume(i32, bool),
    Battery(i32, String, bool),
}

#[derive(Clone)]
pub struct ControlsUi {
    root: gtk::Box,
    container: gtk::Box,
    menu_button: gtk::MenuButton,
    brightness_scale: Arc<Mutex<Option<gtk::Scale>>>,
    brightness_value: Arc<Mutex<Option<gtk::Label>>>,
    brightness_updating: Arc<Mutex<bool>>,
    volume_scale: Arc<Mutex<Option<gtk::Scale>>>,
    volume_value: Arc<Mutex<Option<gtk::Label>>>,
    volume_updating: Arc<Mutex<bool>>,
    battery_value: Arc<Mutex<Option<gtk::Label>>>,
    icons: Arc<Mutex<ControlsIcons>>,
    config: ControlsConfig,
}

#[derive(Debug, Clone)]
pub struct ControlsIcons {
    brightness: String,
    volume: String,
    battery: String,
}

impl ControlsUi {
    fn resolve_icon_name(preferred: &str, fallbacks: &[&str]) -> String {
        let display = match gdk::Display::default() {
            Some(d) => d,
            None => return preferred.to_string(),
        };
        let theme = gtk::IconTheme::for_display(&display);
        if theme.has_icon(preferred) {
            return preferred.to_string();
        }
        for name in fallbacks {
            if theme.has_icon(name) {
                return (*name).to_string();
            }
        }
        preferred.to_string()
    }

    pub fn new(config: ControlsConfig, sender: cb::Sender<ControlsMsg>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        if config.css_name.trim().is_empty() {
            root.set_widget_name("controls-button");
        } else {
            root.set_widget_name(&config.css_name);
        }

        let icons = Arc::new(Mutex::new(ControlsIcons {
            brightness: "display-brightness-medium-symbolic".to_string(),
            volume: "audio-volume-medium-symbolic".to_string(),
            battery: "battery-good-symbolic".to_string(),
        }));

        // Create horizontal box for icons
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.append(&container);

        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("pan-down-symbolic");
        menu_button.set_valign(gtk::Align::Center);

        let brightness_scale: Arc<Mutex<Option<gtk::Scale>>> = Arc::new(Mutex::new(None));
        let brightness_value: Arc<Mutex<Option<gtk::Label>>> = Arc::new(Mutex::new(None));
        let brightness_updating: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let volume_scale: Arc<Mutex<Option<gtk::Scale>>> = Arc::new(Mutex::new(None));
        let volume_value: Arc<Mutex<Option<gtk::Label>>> = Arc::new(Mutex::new(None));
        let volume_updating: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let battery_value: Arc<Mutex<Option<gtk::Label>>> = Arc::new(Mutex::new(None));

        let icons_clone = icons.clone();
        let config_clone = config.clone();

        let popover = Self::build_popover(
            &menu_button,
            &config_clone,
            &brightness_scale,
            &brightness_value,
            &brightness_updating,
            &volume_scale,
            &volume_value,
            &volume_updating,
            &battery_value,
        );
        menu_button.set_popover(Some(&popover));

        // Build initial UI
        Self::build_container(&container, &config_clone, &icons_clone, &menu_button);

        // Start refresh thread with better error handling
        let sender_clone = sender.clone();
        let config_refresh = config.clone();
        thread::spawn(move || {
            loop {
                // Add error handling to prevent panic
                if let Err(_) = std::panic::catch_unwind(|| {
                    Self::refresh_system_status(&sender_clone, &config_refresh);
                }) {
                    // Log error and continue
                    eprintln!("Controls refresh thread encountered an error, continuing...");
                }
                thread::sleep(Duration::from_secs(config_refresh.interval as u64));
            }
        });

        Self {
            root,
            container,
            menu_button,
            brightness_scale,
            brightness_value,
            brightness_updating,
            volume_scale,
            volume_value,
            volume_updating,
            battery_value,
            icons,
            config,
        }
    }

    fn build_container(
        container: &gtk::Box,
        config: &ControlsConfig,
        icons: &Arc<Mutex<ControlsIcons>>,
        menu_button: &gtk::MenuButton,
    ) {
        // Remove everything except the menu button to avoid reparenting warnings while the
        // menu button is active.
        let mut to_remove = Vec::new();
        let mut child_opt = container.first_child();
        while let Some(child) = child_opt {
            child_opt = child.next_sibling();
            if let Some(btn) = child.downcast_ref::<gtk::MenuButton>() {
                if btn == menu_button {
                    continue;
                }
            }
            to_remove.push(child);
        }
        for child in to_remove {
            container.remove(&child);
        }

        let icons_guard = icons.lock().unwrap();

        for component in &config.components {
            match component.as_str() {
                "brightness" => {
                    let name = Self::resolve_icon_name(
                        &icons_guard.brightness,
                        &[
                            "display-brightness-symbolic",
                            "weather-clear-symbolic",
                            "display-symbolic",
                        ],
                    );
                    let img = gtk::Image::from_icon_name(&name);
                    img.set_pixel_size(config.icon_size);
                    container.append(&img);
                }
                "volume" => {
                    let name = Self::resolve_icon_name(
                        &icons_guard.volume,
                        &["audio-volume-medium-symbolic", "audio-volume-high-symbolic"],
                    );
                    let img = gtk::Image::from_icon_name(&name);
                    img.set_pixel_size(config.icon_size);
                    container.append(&img);
                }
                "battery" => {
                    let name = Self::resolve_icon_name(
                        &icons_guard.battery,
                        &["battery-good-symbolic", "battery-full-symbolic"],
                    );
                    let img = gtk::Image::from_icon_name(&name);
                    img.set_pixel_size(config.icon_size);
                    container.append(&img);
                }
                _ => {}
            }
        }

        menu_button.set_icon_name("pan-down-symbolic");
        if menu_button.parent().is_none() {
            container.append(menu_button);
        }
    }

    fn build_popover(
        _menu_button: &gtk::MenuButton,
        config: &ControlsConfig,
        brightness_scale: &Arc<Mutex<Option<gtk::Scale>>>,
        brightness_value: &Arc<Mutex<Option<gtk::Label>>>,
        brightness_updating: &Arc<Mutex<bool>>,
        volume_scale: &Arc<Mutex<Option<gtk::Scale>>>,
        volume_value: &Arc<Mutex<Option<gtk::Label>>>,
        volume_updating: &Arc<Mutex<bool>>,
        battery_value: &Arc<Mutex<Option<gtk::Label>>>,
    ) -> gtk::Popover {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_top(8);
        root.set_margin_bottom(8);
        root.set_margin_start(8);
        root.set_margin_end(8);

        for component in &config.components {
            match component.as_str() {
                "brightness" => {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let label = gtk::Label::new(Some("Brightness"));
                    label.set_xalign(0.0);
                    label.set_hexpand(true);
                    let value = gtk::Label::new(Some(""));
                    value.set_xalign(1.0);

                    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
                    scale.set_hexpand(true);
                    scale.set_draw_value(false);
                    scale.set_height_request(24);

                    let value_for_update = value.clone();
                    let brightness_updating = brightness_updating.clone();
                    scale.connect_value_changed(move |s| {
                        if let Ok(flag) = brightness_updating.lock() {
                            if *flag {
                                return;
                            }
                        }
                        let v = s.value().round() as i32;
                        value_for_update.set_text(&format!("{}%", v));
                        let _ = Self::set_brightness(v);
                    });

                    row.append(&label);
                    row.append(&value);
                    root.append(&row);
                    root.append(&scale);

                    if let Ok(mut slot) = brightness_scale.lock() {
                        *slot = Some(scale);
                    }
                    if let Ok(mut slot) = brightness_value.lock() {
                        *slot = Some(value);
                    }
                }
                "volume" => {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let label = gtk::Label::new(Some("Volume"));
                    label.set_xalign(0.0);
                    label.set_hexpand(true);
                    let value = gtk::Label::new(Some(""));
                    value.set_xalign(1.0);

                    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
                    scale.set_hexpand(true);
                    scale.set_draw_value(false);
                    scale.set_height_request(24);

                    let value_for_update = value.clone();
                    let volume_updating = volume_updating.clone();
                    scale.connect_value_changed(move |s| {
                        if let Ok(flag) = volume_updating.lock() {
                            if *flag {
                                return;
                            }
                        }
                        let v = s.value().round() as i32;
                        value_for_update.set_text(&format!("{}%", v));
                        let _ = Self::set_volume(v);
                    });

                    row.append(&label);
                    row.append(&value);
                    root.append(&row);
                    root.append(&scale);

                    if let Ok(mut slot) = volume_scale.lock() {
                        *slot = Some(scale);
                    }
                    if let Ok(mut slot) = volume_value.lock() {
                        *slot = Some(value);
                    }
                }
                "battery" => {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let label = gtk::Label::new(Some("Battery"));
                    label.set_xalign(0.0);
                    label.set_hexpand(true);
                    let value = gtk::Label::new(Some(""));
                    value.set_xalign(1.0);
                    row.append(&label);
                    row.append(&value);
                    root.append(&row);

                    if let Ok(mut slot) = battery_value.lock() {
                        *slot = Some(value);
                    }
                }
                _ => {}
            }
        }

        popover.set_child(Some(&root));
        popover
    }

    fn set_brightness(value: i32) -> Result<()> {
        let v = value.clamp(0, 100);
        // Prefer `light` if available
        if Command::new("light")
            .args(["-S", &format!("{}", v)])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }

        // Fallback to brightnessctl
        let _ = Command::new("brightnessctl")
            .args(["set", &format!("{}%", v)])
            .status();
        Ok(())
    }

    fn set_volume(value: i32) -> Result<()> {
        let v = value.clamp(0, 100);
        if Command::new("pamixer")
            .args(["--set-volume", &format!("{}", v)])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(());
        }

        let _ = Command::new("pactl")
            .args(["set-sink-volume", "@DEFAULT_SINK@", &format!("{}%", v)])
            .status();
        Ok(())
    }

    fn refresh_system_status(sender: &cb::Sender<ControlsMsg>, config: &ControlsConfig) {
        // Brightness
        if config.components.contains(&"brightness".to_string()) {
            match Self::get_brightness() {
                Ok(brightness) => {
                    let _ = sender.send(ControlsMsg::Brightness(brightness));
                }
                Err(_) => {
                    // Send default value on error
                    let _ = sender.send(ControlsMsg::Brightness(50));
                }
            }
        }

        // Volume
        if config.components.contains(&"volume".to_string()) {
            match Self::get_volume() {
                Ok((volume, muted)) => {
                    let _ = sender.send(ControlsMsg::Volume(volume, muted));
                }
                Err(_) => {
                    // Send default value on error
                    let _ = sender.send(ControlsMsg::Volume(50, false));
                }
            }
        }

        // Battery (check less frequently)
        if config.components.contains(&"battery".to_string()) {
            match Self::get_battery() {
                Ok((capacity, time, charging)) => {
                    let _ = sender.send(ControlsMsg::Battery(capacity, time, charging));
                }
                Err(_) => {
                    // Send default value on error
                    let _ = sender.send(ControlsMsg::Battery(50, "Unknown".to_string(), false));
                }
            }
        }
    }

    fn get_brightness() -> Result<i32> {
        // Try light command first
        if let Ok(output) = Command::new("light")
            .arg("-G")
            .output() 
        {
            if output.status.success() {
                let brightness_str = String::from_utf8(output.stdout)
                    .context("Invalid UTF-8 from light command")?;
                
                if let Ok(brightness) = brightness_str.trim().parse::<f64>() {
                    // `light -G` usually returns a percentage in the range 0..=100
                    // (often with decimals). Some setups may return 0.0..=1.0.
                    let pct = if brightness <= 1.0 { brightness * 100.0 } else { brightness };
                    return Ok(pct.round().clamp(0.0, 100.0) as i32);
                }
            }
        }

        // Fallback to brightnessctl
        if let Ok(cur) = Command::new("brightnessctl").arg("g").output() {
            if cur.status.success() {
                if let Ok(max) = Command::new("brightnessctl").arg("m").output() {
                    if max.status.success() {
                        let cur_s = String::from_utf8(cur.stdout)
                            .context("Invalid UTF-8 from brightnessctl g")?;
                        let max_s = String::from_utf8(max.stdout)
                            .context("Invalid UTF-8 from brightnessctl m")?;
                        if let (Ok(cur_v), Ok(max_v)) = (cur_s.trim().parse::<f64>(), max_s.trim().parse::<f64>()) {
                            if max_v > 0.0 {
                                let pct = (cur_v / max_v) * 100.0;
                                return Ok(pct.round().clamp(0.0, 100.0) as i32);
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback to a default value
        Ok(50)
    }

    fn get_volume() -> Result<(i32, bool)> {
        // Try pamixer first
        if let Ok(output) = Command::new("pamixer")
            .arg("--get-volume")
            .output() 
        {
            if output.status.success() {
                if let Ok(volume_str) = String::from_utf8(output.stdout) {
                    if let Ok(volume) = volume_str.trim().parse::<i32>() {
                        let muted_output = Command::new("pamixer")
                            .arg("--get-mute")
                            .output();
                        
                        let muted = if muted_output.is_ok() && muted_output.as_ref().unwrap().status.success() {
                            if let Ok(muted_str) = String::from_utf8(muted_output.unwrap().stdout) {
                                muted_str.trim() == "true"
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        
                        return Ok((volume, muted));
                    }
                }
            }
        }

        // Fallback to pactl (PulseAudio / PipeWire)
        if let Ok(output) = Command::new("pactl")
            .args(["get-sink-volume", "@DEFAULT_SINK@"]) 
            .output()
        {
            if output.status.success() {
                let out = String::from_utf8(output.stdout)
                    .context("Invalid UTF-8 from pactl get-sink-volume")?;
                if let Ok(volume) = Self::parse_pactl_volume(&out) {
                    let muted = Command::new("pactl")
                        .args(["get-sink-mute", "@DEFAULT_SINK@"])
                        .output()
                        .ok()
                        .and_then(|o| {
                            if o.status.success() {
                                String::from_utf8(o.stdout).ok()
                            } else {
                                None
                            }
                        })
                        .map(|s| s.contains("yes"))
                        .unwrap_or(false);

                    return Ok((volume, muted));
                }
            }
        }

        // Fallback to default values
        Ok((50, false))
    }

    fn parse_pactl_volume(output: &str) -> Result<i32> {
        // Look for volume percentage in pactl output
        for line in output.lines() {
            if line.contains("Volume:") {
                if let Some(start) = line.find('(') {
                    if let Some(end) = line.find('%') {
                        let volume_str = &line[start + 1..end];
                        return volume_str.trim().parse::<i32>()
                            .context("Failed to parse volume percentage");
                    }
                }
            }
        }
        anyhow::bail!("Could not find volume in pactl output")
    }

    fn get_battery() -> Result<(i32, String, bool)> {
        // Try upower command
        if let Ok(output) = Command::new("upower")
            .args(&["-i", "/org/freedesktop/UPower/devices/battery_BAT0"])
            .output() 
        {
            if output.status.success() {
                if let Ok(battery_info) = String::from_utf8(output.stdout) {
                    let mut capacity = 50;
                    let mut time_to_empty = String::new();
                    let mut charging = false;

                    for line in battery_info.lines() {
                        if line.contains("percentage:") {
                            if let Some(percent_str) = line.split(':').nth(1) {
                                if let Ok(cap) = percent_str.trim().trim_end_matches('%').parse::<i32>() {
                                    capacity = cap;
                                }
                            }
                        } else if line.contains("time to empty:") {
                            if let Some(time_str) = line.split(':').nth(1) {
                                time_to_empty = time_str.trim().to_string();
                            }
                        } else if line.contains("state:") {
                            if let Some(state_str) = line.split(':').nth(1) {
                                charging = state_str.trim().contains("charging");
                            }
                        }
                    }

                    return Ok((capacity, time_to_empty, charging));
                }
            }
        }

        // Fallback to default values
        Ok((50, "Unknown".to_string(), false))
    }

    pub fn update_brightness(&self, value: i32) {
        let icon_name = Self::brightness_icon_name(value);
        
        let mut icons = self.icons.lock().unwrap();
        if icons.brightness != icon_name {
            icons.brightness = icon_name;
            drop(icons);
            
            // Rebuild container with updated icons
            Self::build_container(&self.container, &self.config, &self.icons, &self.menu_button);
        }

        if let Ok(slot) = self.brightness_scale.lock() {
            if let Some(scale) = slot.as_ref() {
                if let Ok(mut flag) = self.brightness_updating.lock() {
                    *flag = true;
                }
                scale.set_value(value.clamp(0, 100) as f64);
                if let Ok(mut flag) = self.brightness_updating.lock() {
                    *flag = false;
                }
            }
        }
        if let Ok(slot) = self.brightness_value.lock() {
            if let Some(lbl) = slot.as_ref() {
                lbl.set_text(&format!("{}%", value.clamp(0, 100)));
            }
        }
    }

    pub fn update_volume(&self, value: i32, muted: bool) {
        let icon_name = Self::volume_icon_name(value, muted);
        
        let mut icons = self.icons.lock().unwrap();
        if icons.volume != icon_name {
            icons.volume = icon_name;
            drop(icons);
            
            // Rebuild container with updated icons
            Self::build_container(&self.container, &self.config, &self.icons, &self.menu_button);
        }

        if let Ok(slot) = self.volume_scale.lock() {
            if let Some(scale) = slot.as_ref() {
                if let Ok(mut flag) = self.volume_updating.lock() {
                    *flag = true;
                }
                scale.set_value(value.clamp(0, 100) as f64);
                if let Ok(mut flag) = self.volume_updating.lock() {
                    *flag = false;
                }
            }
        }
        if let Ok(slot) = self.volume_value.lock() {
            if let Some(lbl) = slot.as_ref() {
                if muted {
                    lbl.set_text("Muted");
                } else {
                    lbl.set_text(&format!("{}%", value.clamp(0, 100)));
                }
            }
        }
    }

    pub fn update_battery(&self, capacity: i32, charging: bool) {
        let icon_name = Self::battery_icon_name(capacity, charging);
        
        let mut icons = self.icons.lock().unwrap();
        if icons.battery != icon_name {
            icons.battery = icon_name;
            drop(icons);
            
            // Rebuild container with updated icons
            Self::build_container(&self.container, &self.config, &self.icons, &self.menu_button);
        }

        if let Ok(slot) = self.battery_value.lock() {
            if let Some(lbl) = slot.as_ref() {
                let cap = capacity.clamp(0, 100);
                let status = if charging { "Charging" } else { "" };
                if status.is_empty() {
                    lbl.set_text(&format!("{}%", cap));
                } else {
                    lbl.set_text(&format!("{}% {}", cap, status));
                }
            }
        }
    }

    fn brightness_icon_name(value: i32) -> String {
        if value > 70 {
            "display-brightness-high-symbolic".to_string()
        } else if value > 30 {
            "display-brightness-medium-symbolic".to_string()
        } else {
            "display-brightness-low-symbolic".to_string()
        }
    }

    fn volume_icon_name(value: i32, muted: bool) -> String {
        if muted {
            "audio-volume-muted-symbolic".to_string()
        } else if value > 70 {
            "audio-volume-high-symbolic".to_string()
        } else if value > 30 {
            "audio-volume-medium-symbolic".to_string()
        } else {
            "audio-volume-low-symbolic".to_string()
        }
    }

    fn battery_icon_name(value: i32, charging: bool) -> String {
        if charging {
            if value > 90 {
                "battery-full-charging-symbolic".to_string()
            } else if value > 40 {
                "battery-good-charging-symbolic".to_string()
            } else if value > 19 {
                "battery-low-charging-symbolic".to_string()
            } else {
                "battery-empty-charging-symbolic".to_string()
            }
        } else {
            if value > 90 {
                "battery-full-symbolic".to_string()
            } else if value > 40 {
                "battery-good-symbolic".to_string()
            } else if value > 19 {
                "battery-low-symbolic".to_string()
            } else {
                "battery-empty-symbolic".to_string()
            }
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
