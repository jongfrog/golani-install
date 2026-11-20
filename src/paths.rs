use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        if let Some(root) = std::env::var_os("GOLANI_HOME") {
            return Ok(Self::new(PathBuf::from(root)));
        }
        let home = dirs::home_dir().ok_or(Error::DataDirectoryUnavailable)?;
        Ok(Self::new(home.join(".golani")))
    }

    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn global_lock_file(&self) -> PathBuf {
        self.root.join("golani.lock")
    }

    pub fn java_dir(&self) -> PathBuf {
        self.root.join("java")
    }

    pub fn servers_dir(&self) -> PathBuf {
        self.root.join("servers")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn ensure_directories(&self) -> Result<()> {
        for directory in [
            self.root(),
            self.java_dir().as_path(),
            self.servers_dir().as_path(),
            self.logs_dir().as_path(),
        ] {
            fs::create_dir_all(directory)?;
        }
        Ok(())
    }
}
