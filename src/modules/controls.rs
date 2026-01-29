use gtk4 as gtk;
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
    root: gtk::Button,
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
    pub fn new(config: ControlsConfig, sender: cb::Sender<ControlsMsg>) -> Self {
        let root = gtk::Button::new();
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
        root.set_child(Some(&container));

        let icons_clone = icons.clone();
        let config_clone = config.clone();

        // Build initial UI
        Self::build_container(&container, &config_clone, &icons_clone);

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
            icons,
            config,
        }
    }

    fn build_container(container: &gtk::Box, config: &ControlsConfig, icons: &Arc<Mutex<ControlsIcons>>) {
        // Clear existing children
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }

        let icons_guard = icons.lock().unwrap();

        if config.components.contains(&"brightness".to_string()) {
            let bri_img = gtk::Image::from_icon_name(&icons_guard.brightness);
            bri_img.set_pixel_size(config.icon_size);
            container.append(&bri_img);
        }

        if config.components.contains(&"volume".to_string()) {
            let vol_img = gtk::Image::from_icon_name(&icons_guard.volume);
            vol_img.set_pixel_size(config.icon_size);
            container.append(&vol_img);
        }

        if config.components.contains(&"battery".to_string()) {
            let bat_img = gtk::Image::from_icon_name(&icons_guard.battery);
            bat_img.set_pixel_size(config.icon_size);
            container.append(&bat_img);
        }

        // Add dropdown arrow
        let arrow_img = gtk::Image::from_icon_name("pan-down-symbolic");
        arrow_img.set_pixel_size(config.icon_size);
        container.append(&arrow_img);
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
                    return Ok((brightness * 100.0) as i32);
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
            if let Some(child) = self.root.child() {
                if let Some(container) = child.downcast_ref::<gtk::Box>() {
                    Self::build_container(container, &self.config, &self.icons);
                }
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
            if let Some(child) = self.root.child() {
                if let Some(container) = child.downcast_ref::<gtk::Box>() {
                    Self::build_container(container, &self.config, &self.icons);
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
            if let Some(child) = self.root.child() {
                if let Some(container) = child.downcast_ref::<gtk::Box>() {
                    Self::build_container(container, &self.config, &self.icons);
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
