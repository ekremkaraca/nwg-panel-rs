use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct PanelConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub output: String,
    #[serde(default)]
    pub monitor: String,

    #[serde(default = "default_layer")]
    pub layer: String,
    #[serde(default = "default_position")]
    pub position: String,

    #[serde(default)]
    pub height: i32,

    #[serde(default, rename = "margin-top")]
    pub margin_top: i32,
    #[serde(default, rename = "margin-bottom")]
    pub margin_bottom: i32,
    #[serde(default, rename = "margin-left")]
    pub margin_left: i32,
    #[serde(default, rename = "margin-right")]
    pub margin_right: i32,

    #[serde(default, rename = "css-name")]
    pub css_name: String,

    #[serde(default)]
    pub controls: ControlsCompat,

    #[serde(default, rename = "controls-settings")]
    pub controls_settings: ControlsConfig,

    #[serde(default, rename = "modules-left")]
    pub modules_left: Vec<String>,
    #[serde(default, rename = "modules-center")]
    pub modules_center: Vec<String>,
    #[serde(default, rename = "modules-right")]
    pub modules_right: Vec<String>,

    #[serde(default = "default_exclusive_zone", rename = "exclusive-zone")]
    pub exclusive_zone: bool,

    #[serde(default, rename = "hyprland-workspaces")]
    pub hyprland_workspaces: HyprlandWorkspacesConfig,

    #[serde(default)]
    pub clock: ClockConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(untagged)]
pub enum ControlsCompat {
    #[default]
    None,
    Position(String),
    Settings(ControlsConfig),
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ClockConfig {
    #[serde(default = "default_clock_format")]
    pub format: String,

    #[serde(default = "default_clock_interval")]
    pub interval: u32,

    #[serde(default, rename = "css-name")]
    pub css_name: String,

    #[serde(default, rename = "root-css-name")]
    pub root_css_name: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[allow(dead_code)]
pub struct HyprlandWorkspacesConfig {
    #[serde(default = "default_num_ws")]
    pub num_ws: usize,
    
    #[serde(default = "default_true")]
    pub show_icon: bool,
    
    #[serde(default = "default_true")]
    pub show_inactive_workspaces: bool,
    
    #[serde(default = "default_true")]
    pub show_workspaces_from_all_outputs: bool,
    
    #[serde(default = "default_image_size")]
    pub image_size: i32,
    
    #[serde(default = "default_true")]
    pub show_workspaces: bool,
    
    #[serde(default = "default_true")]
    pub show_name: bool,
    
    #[serde(default = "default_name_length")]
    pub name_length: usize,
    
    #[serde(default = "default_true")]
    pub show_empty: bool,
    
    #[serde(default = "default_true")]
    pub mark_content: bool,
    
    #[serde(default = "default_true")]
    pub show_names: bool,
    
    #[serde(default = "default_true")]
    pub mark_floating: bool,
    
    #[serde(default = "default_true")]
    pub mark_xwayland: bool,
    
    #[serde(default)]
    pub angle: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct HyprWorkspaceRule {
    pub workspace_string: String,
    pub monitor: String,
}

fn default_true() -> bool { true }
fn default_num_ws() -> usize { 10 }
fn default_image_size() -> i32 { 16 }
fn default_name_length() -> usize { 40 }
fn default_clock_format() -> String { "%H:%M".to_string() }
fn default_clock_interval() -> u32 { 1 }
fn default_layer() -> String { "bottom".to_string() }
fn default_position() -> String { "top".to_string() }
fn default_exclusive_zone() -> bool { true }

pub fn load_panels_from_path(path: &Path) -> anyhow::Result<Vec<PanelConfig>> {
    let text = fs::read_to_string(path)?;
    let panels: Vec<PanelConfig> = serde_json::from_str(&text)?;
    Ok(panels)
}

// Controls configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ControlsConfig {
    #[serde(default = "default_controls_components")]
    pub components: Vec<String>,
    
    #[serde(default = "default_icon_size")]
    pub icon_size: i32,
    
    #[serde(default = "default_interval")]
    pub interval: u32,
    
    #[serde(default)]
    pub css_name: String,
}

fn default_controls_components() -> Vec<String> {
    vec!["brightness".to_string(), "volume".to_string(), "battery".to_string()]
}

fn default_icon_size() -> i32 { 16 }

fn default_interval() -> u32 { 1 }
