use std::fs;

use golani::{config::Config, paths::AppPaths, Application};
use tempfile::tempdir;

#[test]
fn initialization_creates_required_data_and_configuration() {
    let temp = tempdir().unwrap();
    let root = temp.path().join(".golani");
    let application = Application::initialize_at(AppPaths::new(root.clone())).unwrap();

    assert!(root.join("config.toml").is_file());
    assert!(root.join("java").is_dir());
    assert!(root.join("servers").is_dir());
    assert!(root.join("logs").is_dir());
    assert_eq!(application.config.servers_dir, root.join("servers"));
}

#[test]
fn saved_server_directory_is_reused() {
    let temp = tempdir().unwrap();
    let root = temp.path().join(".golani");
    fs::create_dir_all(&root).unwrap();
    let external_servers = temp.path().join("minecraft-servers");
    let stored = Config {
        servers_dir: external_servers.clone(),
        java_path: None,
    };
    fs::write(root.join("config.toml"), toml::to_string(&stored).unwrap()).unwrap();

    let application = Application::initialize_at(AppPaths::new(root)).unwrap();
    assert_eq!(application.config.servers_dir, external_servers);
    assert!(application.config.servers_dir.is_dir());
}
