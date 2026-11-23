use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub servers_dir: PathBuf,
    #[serde(default)]
    pub java_path: Option<PathBuf>,
    #[serde(default = "default_ram")]
    pub default_ram_mb: u32,
    #[serde(default = "default_true")]
    pub web_ui_enabled: bool,
    #[serde(default = "default_true")]
    pub firewall_managed: bool,
    #[serde(default = "default_web_port")]
    pub web_ui_port: u16,
}

fn default_ram() -> u32 { 2048 }
fn default_true() -> bool { true }
fn default_web_port() -> u16 { 8788 }

impl Config {
    pub fn default_for(paths: &AppPaths) -> Self {
        Self {
            servers_dir: paths.servers_dir(),
            java_path: None,
            default_ram_mb: 2048,
            web_ui_enabled: true,
            firewall_managed: true,
            web_ui_port: 8788,
        }
    }

    pub fn load_or_create(paths: &AppPaths) -> Result<Self> {
        let file = paths.config_file();
        if file.exists() {
            return Ok(toml::from_str(&fs::read_to_string(file)?)?);
        }

        let config = Self::default_for(paths);
        config.save(paths)?;
        Ok(config)
    }

    pub fn save(&self, paths: &AppPaths) -> Result<()> {
        let data = toml::to_string_pretty(self)?;
        fs::write(paths.config_file(), data)?;
        Ok(())
    }
}
