use std::path::{Path, PathBuf};

use rand::{distr::Alphanumeric, Rng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::Sha256;
use tokio::io::AsyncWriteExt;

use crate::error::{Error, Result};
use crate::firewall;

const VANILLA_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const PAPER_FILL: &str = "https://fill.papermc.io/v3";
const USER_AGENT: &str = "golani/0.1.0 (https://golani.kro.kr)";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServerKind {
    Vanilla,
    Paper,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetadata {
    pub name: String,
    pub kind: ServerKind,
    pub minecraft_version: String,
    pub paper_build: Option<String>,
    pub ram_mb: u32,
    pub port: u16,
    pub created_at_unix: u64,
    pub last_started_at_unix: Option<u64>,
    pub last_exit_status: Option<String>,
    pub java_version: u8,
}

#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub size: Option<u64>,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub paper_build: Option<String>,
}

#[derive(Clone)]
pub struct MinecraftApi {
    client: Client,
}

impl MinecraftApi {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder().user_agent(USER_AGENT).build()?,
        })
    }

    pub async fn vanilla_versions(&self) -> Result<Vec<String>> {
        let manifest: VanillaManifest = self
            .client
            .get(VANILLA_MANIFEST)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(manifest
            .versions
            .into_iter()
            .filter(|v| v.kind == "release")
            .map(|v| v.id)
            .collect())
    }

    pub async fn vanilla_download(&self, version: &str) -> Result<Download> {
        let manifest: VanillaManifest = self
            .client
            .get(VANILLA_MANIFEST)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let entry = manifest
            .versions
            .into_iter()
            .find(|v| v.id == version)
            .ok_or_else(|| {
                Error::ServerCreation(format!("존재하지 않는 Vanilla 버전입니다: {version}"))
            })?;
        let detail: VanillaDetail = self
            .client
            .get(entry.url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let server = detail.downloads.server.ok_or_else(|| {
            Error::ServerCreation("이 버전은 서버 다운로드를 제공하지 않습니다".into())
        })?;
        Ok(Download {
            url: server.url,
            size: None,
            sha1: Some(server.sha1),
            sha256: None,
            paper_build: None,
        })
    }

    pub async fn paper_versions(&self) -> Result<Vec<String>> {
        let value: serde_json::Value = self
            .client
            .get(format!("{PAPER_FILL}/projects/paper"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let mut versions = value
            .get("versions")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                Error::ServerCreation("Paper 버전 응답 형식이 올바르지 않습니다".into())
            })?
            .values()
            .flat_map(|v| v.as_array().into_iter().flatten())
            .filter_map(|v| v.as_str())
            .filter(|v| !v.contains('-'))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        versions.sort_by(|left, right| version_key(right).cmp(&version_key(left)));
        versions.dedup();
        Ok(versions)
    }

    pub async fn paper_download(&self, version: &str) -> Result<Download> {
        let value: serde_json::Value = self
            .client
            .get(format!(
                "{PAPER_FILL}/projects/paper/versions/{version}/builds"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let builds = value
            .as_array()
            .or_else(|| value.get("builds").and_then(|v| v.as_array()))
            .ok_or_else(|| {
                Error::ServerCreation("Paper 빌드 응답 형식이 올바르지 않습니다".into())
            })?;
        let build = builds
            .iter()
            .rev()
            .find(|b| b.get("channel").and_then(|c| c.as_str()) == Some("STABLE"))
            .ok_or_else(|| {
                Error::ServerCreation("사용 가능한 안정 Paper 빌드가 없습니다".into())
            })?;
        let download = build["downloads"]
            .get("server:default")
            .or_else(|| {
                build["downloads"]
                    .get("server")
                    .and_then(|server| server.get("default"))
            })
            .ok_or_else(|| Error::ServerCreation("Paper 서버 다운로드 정보가 없습니다".into()))?;
        let url = download["url"]
            .as_str()
            .ok_or_else(|| Error::ServerCreation("Paper 다운로드 URL이 없습니다".into()))?
            .to_owned();
        let sha256 = download["checksums"]["sha256"]
            .as_str()
            .ok_or_else(|| Error::Verification("Paper SHA-256이 제공되지 않았습니다".into()))?
            .to_owned();
        Ok(Download {
            url,
            size: download["size"].as_u64(),
            sha1: None,
            sha256: Some(sha256),
            paper_build: build
                .get("build")
                .or_else(|| build.get("id"))
                .and_then(|v| v.as_i64())
                .map(|v| v.to_string()),
        })
    }

    pub async fn download_verified(&self, download: &Download, destination: &Path) -> Result<()> {
        let temporary = destination.with_extension("jar.part");
        let response = self
            .client
            .get(&download.url)
            .send()
            .await?
            .error_for_status()?;
        let bytes = response.bytes().await?;
        if let Some(size) = download.size {
            if bytes.len() as u64 != size {
                return Err(Error::Verification(
                    "다운로드 파일 크기가 일치하지 않습니다".into(),
                ));
            }
        }
        if let Some(expected) = &download.sha256 {
            let actual = hex::encode(Sha256::digest(&bytes));
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(Error::Verification("SHA-256이 일치하지 않습니다".into()));
            }
        }
        if let Some(expected) = &download.sha1 {
            let actual = hex::encode(Sha1::digest(&bytes));
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(Error::Verification("공식 SHA-1이 일치하지 않습니다".into()));
            }
        }
        let mut file = tokio::fs::File::create(&temporary).await?;
        file.write_all(&bytes).await?;
        file.flush().await?;
        drop(file);
        tokio::fs::rename(temporary, destination).await?;
        Ok(())
    }
}

fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| part.parse().ok())
        .collect()
}

pub fn java_version_for(version: &str) -> u8 {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    match parts.as_slice() {
        [1, minor, patch, ..] if *minor > 20 || (*minor == 20 && *patch >= 5) => 21,
        [1, minor, ..] if *minor >= 18 => 17,
        [1, 17, ..] => 16,
        _ => 8,
    }
}

pub fn write_server_properties(
    path: &Path,
    port: u16,
    motd: &str,
    game_mode: &str,
    difficulty: &str,
    seed: Option<&str>,
    rcon_password: &str,
) -> Result<()> {
    let seed = seed.unwrap_or("");
    std::fs::write(path, format!("server-port={port}\nmotd={motd}\ngamemode={game_mode}\ndifficulty={difficulty}\nlevel-seed={seed}\nenable-rcon=true\nrcon.port={}\nrcon.password={rcon_password}\n", port.saturating_add(10)))?;
    Ok(())
}

pub async fn create_server(
    api: &MinecraftApi,
    base: &Path,
    name: &str,
    kind: ServerKind,
    version: &str,
    ram_mb: u32,
    port: u16,
    motd: &str,
    game_mode: &str,
    difficulty: &str,
    seed: Option<&str>,
) -> Result<ServerMetadata> {
    let directory = server_directory(base, name)?;
    if directory.exists() {
        return Err(Error::ServerCreation(
            "같은 이름의 서버가 이미 있습니다".into(),
        ));
    }
    let download = match kind {
        ServerKind::Vanilla => api.vanilla_download(version).await?,
        ServerKind::Paper => api.paper_download(version).await?,
    };
    std::fs::create_dir_all(directory.join("logs"))?;
    let result = async {
        api.download_verified(&download, &directory.join("server.jar"))
            .await?;
        let password: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        write_server_properties(
            &directory.join("server.properties"),
            port,
            motd,
            game_mode,
            difficulty,
            seed,
            &password,
        )?;
        std::fs::write(directory.join("eula.txt"), "eula=true\n")?;
        std::fs::write(
            directory.join("logs").join("golani.log"),
            "eula.txt auto-accepted\n",
        )?;
        let metadata = ServerMetadata {
            name: name.into(),
            kind,
            minecraft_version: version.into(),
            paper_build: download.paper_build,
            ram_mb,
            port,
            created_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            last_started_at_unix: None,
            last_exit_status: None,
            java_version: java_version_for(version),
        };
        std::fs::write(
            directory.join(".golani.json"),
            serde_json::to_vec_pretty(&metadata)?,
        )?;
        // 방화벽 규칙 추가 시도 (실패해도 서버 생성은 계속)
        let _ = firewall::add_firewall_rule(name, port);
        Ok(metadata)
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&directory);
    }
    result
}

pub fn server_directory(base: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::ServerCreation(
            "서버 이름은 영문, 숫자, 하이픈, 밑줄만 사용할 수 있습니다".into(),
        ));
    }
    Ok(base.join(name))
}

#[derive(Deserialize)]
struct VanillaManifest {
    versions: Vec<VanillaVersion>,
}
#[derive(Deserialize)]
struct VanillaVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
}
#[derive(Deserialize)]
struct VanillaDetail {
    downloads: VanillaDownloads,
}
#[derive(Deserialize)]
struct VanillaDownloads {
    server: Option<VanillaServer>,
}
#[derive(Deserialize)]
struct VanillaServer {
    url: String,
    sha1: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_mapping() {
        assert_eq!(java_version_for("1.20.4"), 17);
        assert_eq!(java_version_for("1.20.5"), 21);
        assert_eq!(java_version_for("1.17.1"), 16);
        assert_eq!(java_version_for("1.16.5"), 8);
    }
    #[test]
    fn rejects_unsafe_server_name() {
        assert!(server_directory(Path::new("x"), "../bad").is_err());
    }
}
