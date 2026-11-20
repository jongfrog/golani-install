use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use sha2::Digest;

use crate::{
    error::{Error, Result},
    minecraft::java_version_for,
    paths::AppPaths,
};

#[derive(Debug, Clone)]
pub struct JavaInstallation {
    pub executable: PathBuf,
    pub feature: u8,
}

pub fn required_version(minecraft_version: &str) -> u8 {
    java_version_for(minecraft_version)
}

pub fn find_system_java(required: u8) -> Option<JavaInstallation> {
    let output = Command::new("java").arg("-version").output().ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let feature = parse_java_feature(&text)?;
    (feature >= required).then(|| JavaInstallation {
        executable: PathBuf::from(if cfg!(windows) { "java.exe" } else { "java" }),
        feature,
    })
}

pub fn managed_java(paths: &AppPaths, required: u8) -> Option<JavaInstallation> {
    let executable = paths
        .java_dir()
        .join(format!("temurin-{required}"))
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" });
    executable.is_file().then(|| JavaInstallation {
        executable,
        feature: required,
    })
}

/// 수동 Java 경로(config.java_path)를 고려하여 Java를 찾습니다.
/// 1. 수동 경로가 설정되어 있으면 그것을 사용
/// 2. 시스템 java 명령어 확인
/// 3. 관리되는 Java(Temurin) 확인
pub fn find_java_with_config(
    config_java_path: Option<&std::path::Path>,
    paths: &AppPaths,
    required: u8,
) -> Option<JavaInstallation> {
    // 1. 수동 경로 우선
    if let Some(java_path) = config_java_path {
        let java_exe = java_path.join(if cfg!(windows) { "java.exe" } else { "java" });
        if java_exe.exists() {
            if let Ok(output) = Command::new(&java_exe).arg("-version").output() {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                if let Some(feature) = parse_java_feature(&text) {
                    if feature >= required {
                        return Some(JavaInstallation {
                            executable: java_exe,
                            feature,
                        });
                    }
                }
            }
        }
    }
    
    // 2. 시스템 java
    find_system_java(required)
        .or_else(|| managed_java(paths, required))
}

pub async fn latest_temurin_asset(feature: u8) -> Result<JavaAsset> {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let architecture = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };
    let url = format!("https://api.adoptium.net/v3/assets/latest/{feature}/hotspot?architecture={architecture}&image_type=jre&os={os}");
    let values: Vec<AdoptiumAsset> = reqwest::Client::new()
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let asset = values.into_iter().next().ok_or_else(|| {
        Error::ServerCreation(format!("Temurin {feature} JRE를 찾을 수 없습니다"))
    })?;
    Ok(JavaAsset {
        url: asset.binary.package.link,
        checksum: asset.binary.package.checksum,
        file_name: asset.binary.package.name,
    })
}

/// 다운로드와 검증이 끝날 때까지 기존 Java 설치를 유지합니다.
pub async fn install_managed_java(paths: &AppPaths, feature: u8) -> Result<JavaInstallation> {
    if let Some(java) = managed_java(paths, feature) {
        validate_java(&java.executable, feature)?;
        return Ok(java);
    }
    let asset = latest_temurin_asset(feature).await?;
    let bytes = reqwest::Client::new()
        .get(&asset.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if !hex::encode(sha2::Sha256::digest(&bytes)).eq_ignore_ascii_case(&asset.checksum) {
        return Err(Error::Verification(
            "Temurin SHA-256이 일치하지 않습니다".into(),
        ));
    }
    let root = paths.java_dir();
    std::fs::create_dir_all(&root)?;
    let staging = root.join(format!(".install-{feature}-{}", std::process::id()));
    if staging.exists() {
        return Err(Error::ServerCreation(
            "이전 Java 설치 작업이 남아 있습니다".into(),
        ));
    }
    std::fs::create_dir(&staging)?;
    let archive = staging.join(&asset.file_name);
    std::fs::write(&archive, &bytes)?;
    let output = if cfg!(windows) {
        Command::new("tar")
            .args(["-xf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&staging)
            .output()?
    } else {
        Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .args(["-C"])
            .arg(&staging)
            .output()?
    };
    if !output.status.success() {
        return Err(Error::ServerCreation(
            "Temurin 압축을 해제하지 못했습니다".into(),
        ));
    }
    let unpacked = std::fs::read_dir(&staging)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .ok_or_else(|| Error::ServerCreation("Temurin 압축 구조가 올바르지 않습니다".into()))?;
    std::fs::rename(unpacked, root.join(format!("temurin-{feature}")))?;
    let java = managed_java(paths, feature).ok_or_else(|| {
        Error::ServerCreation("설치한 Temurin 실행 파일을 찾을 수 없습니다".into())
    })?;
    validate_java(&java.executable, feature)?;
    let _ = std::fs::remove_file(archive);
    let _ = std::fs::remove_dir(staging);
    Ok(java)
}

#[derive(Debug, Clone)]
pub struct JavaAsset {
    pub url: String,
    pub checksum: String,
    pub file_name: String,
}
#[derive(Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}
#[derive(Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}
#[derive(Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
    name: String,
}

fn parse_java_feature(text: &str) -> Option<u8> {
    let quoted = text.split('"').nth(1)?;
    let mut parts = quoted.split(['.', '_']);
    let first: u8 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

pub fn validate_java(java: &Path, required: u8) -> Result<()> {
    let output = Command::new(java).arg("-version").output()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    match parse_java_feature(&text) {
        Some(feature) if feature >= required => Ok(()),
        Some(feature) => Err(Error::ServerCreation(format!(
            "Java {required} 이상이 필요하지만 Java {feature}이 감지되었습니다"
        ))),
        None => Err(Error::ServerCreation(
            "Java 버전을 확인할 수 없습니다".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_versions() {
        assert_eq!(parse_java_feature("openjdk version \"21.0.1\""), Some(21));
        assert_eq!(parse_java_feature("java version \"1.8.0_100\""), Some(8));
        assert_eq!(required_version("1.18.2"), 17);
    }
}
