use anyhow::Context;
use hyprlang::Hyprland;
use std::path::Path;
use crate::modules::config::HyprWorkspaceRule;

pub struct HyprConfig {
    hyprland: Hyprland,
}

impl Clone for HyprConfig {
    fn clone(&self) -> Self {
        Self {
            hyprland: Hyprland::new(),
        }
    }
}

impl HyprConfig {
    pub fn new() -> Self {
        Self {
            hyprland: Hyprland::new(),
        }
    }

    pub fn parse_config<P: AsRef<Path>>(&mut self, config_path: P) -> anyhow::Result<()> {
        let path = config_path.as_ref();
        
        // Try common Hyprland config locations
        let home_config = dirs::home_dir()
            .unwrap_or_default()
            .join(".config/hypr/hyprland.conf");
        
        let config_paths = [
            path,
            home_config.as_path(),
            Path::new("/etc/hypr/hyprland.conf"),
        ];

        for config_path in &config_paths {
            if config_path.exists() {
                self.hyprland.parse_file(config_path)
                    .with_context(|| format!("Failed to parse Hyprland config: {}", config_path.display()))?;
                return Ok(());
            }
        }

        anyhow::bail!("No Hyprland config file found");
    }

    pub fn get_workspace_rules(&self) -> Vec<HyprWorkspaceRule> {
        let mut rules = Vec::new();
        
        // Get workspace rules from Hyprland config
        for workspace in self.hyprland.all_monitors() {
            // Parse monitor workspace rules like "monitor =,preferred,auto,1"
            if let Some(parts) = workspace.split(',').collect::<Vec<&str>>().get(1..=3) {
                if parts.len() >= 3 {
                    let rule = HyprWorkspaceRule {
                        workspace_string: parts.get(2).unwrap_or(&"1").to_string(),
                        monitor: workspace.split(',').next().unwrap_or("").to_string(),
                    };
                    rules.push(rule);
                }
            }
        }

        rules
    }

    #[allow(dead_code)]
    pub fn get_variable(&self, name: &str) -> Option<String> {
        self.hyprland.get_variable(name).cloned()
    }

    #[allow(dead_code)]
    pub fn get_general_setting(&self, key: &str) -> anyhow::Result<String> {
        match key {
            "border_size" => Ok(self.hyprland.general_border_size()?.to_string()),
            "gaps_in" => Ok(self.hyprland.general_gaps_in()?.to_string()),
            "gaps_out" => Ok(self.hyprland.general_gaps_out()?.to_string()),
            "layout" => Ok(self.hyprland.general_layout()?.to_string()),
            _ => anyhow::bail!("Unknown general setting: {}", key),
        }
    }

    #[allow(dead_code)]
    pub fn is_animations_enabled(&self) -> bool {
        self.hyprland.animations_enabled().unwrap_or(false)
    }
}

impl Default for HyprConfig {
    fn default() -> Self {
        Self::new()
    }
}
