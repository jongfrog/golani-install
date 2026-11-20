//! golani의 UI와 플랫폼 기능이 함께 사용하는 Core 모듈입니다.

pub mod config;
pub mod error;
pub mod firewall;
pub mod java;
pub mod lock;
pub mod minecraft;
pub mod paths;
pub mod rcon;
pub mod server;
pub mod tui;
pub mod web;

use std::fs;

use config::Config;
use error::Result;
use lock::GlobalLock;
use paths::AppPaths;

/// 프로그램이 사용하는 디렉터리와 설정을 준비하고, 전역 잠금을 획득합니다.
pub struct Application {
    pub paths: AppPaths,
    pub config: Config,
    _global_lock: GlobalLock,
}

impl Application {
    pub fn initialize() -> Result<Self> {
        let paths = AppPaths::discover()?;
        Self::initialize_at(paths)
    }

    pub fn initialize_at(paths: AppPaths) -> Result<Self> {
        paths.ensure_directories()?;
        let config = Config::load_or_create(&paths)?;
        fs::create_dir_all(&config.servers_dir)?;
        let global_lock = GlobalLock::acquire(&paths.global_lock_file())?;
        Ok(Self {
            paths,
            config,
            _global_lock: global_lock,
        })
    }
}
