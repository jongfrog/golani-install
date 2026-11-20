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
}

impl Config {
    pub fn default_for(paths: &AppPaths) -> Self {
        Self {
            servers_dir: paths.servers_dir(),
            java_path: None,
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
