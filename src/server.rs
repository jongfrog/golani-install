use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::{
    error::{Error, Result},
    firewall,
    java::validate_java,
    lock::ServerLock,
    minecraft::{MinecraftApi, ServerKind, ServerMetadata},
    rcon,
};

pub struct RunningServer {
    _lock: ServerLock,
    pub pid: u32,
}

pub fn load_metadata(server_dir: &Path) -> Result<ServerMetadata> {
    Ok(serde_json::from_slice(&fs::read(
        server_dir.join(".golani.json"),
    )?)?)
}

pub fn rename(server_dir: &Path, new_name: &str) -> Result<PathBuf> {
    if recorded_running_pid(server_dir).is_some() {
        return Err(Error::ServerCreation(
            "실행 중인 서버는 이름을 변경할 수 없습니다".into(),
        ));
    }
    if new_name.is_empty()
        || !new_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::ServerCreation(
            "서버 이름은 영문, 숫자, 하이픈, 밑줄만 사용할 수 있습니다".into(),
        ));
    }
    let destination = server_dir
        .parent()
        .ok_or_else(|| Error::ServerCreation("서버 경로가 올바르지 않습니다".into()))?
        .join(new_name);
    if destination.exists() {
        return Err(Error::ServerCreation(
            "같은 이름의 서버가 이미 있습니다".into(),
        ));
    }
    let mut metadata: ServerMetadata = load_metadata(server_dir)?;
    metadata.name = new_name.into();
    fs::rename(server_dir, &destination)?;
    fs::write(
        destination.join(".golani.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(destination)
}

pub fn delete(server_dir: &Path, confirmation: &str) -> Result<()> {
    if recorded_running_pid(server_dir).is_some() {
        return Err(Error::ServerCreation(
            "실행 중인 서버는 삭제할 수 없습니다".into(),
        ));
    }
    if confirmation != "삭제" {
        return Err(Error::ServerCreation(
            "삭제하려면 확인어 `삭제`를 입력해야 합니다".into(),
        ));
    }
    if !server_dir.join(".golani.json").is_file() {
        return Err(Error::ServerCreation(
            "golani 서버 디렉터리가 아닙니다".into(),
        ));
    }
    // 방화벽 규칙 제거 (메타데이터 읽은 후)
    if let Ok(metadata) = load_metadata(server_dir) {
        let _ = firewall::remove_firewall_rule(&metadata.name, metadata.port);
    }
    fs::remove_dir_all(server_dir)?;
    Ok(())
}
pub fn pid_path(server_dir: &Path) -> PathBuf {
    server_dir.join(".golani.pid")
}

pub fn rcon_settings(server_dir: &Path) -> Option<(u16, String)> {
    let text = fs::read_to_string(server_dir.join("server.properties")).ok()?;
    let mut port = None;
    let mut password = None;
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "rcon.port" => port = value.parse().ok(),
                "rcon.password" => password = Some(value.to_owned()),
                _ => {}
            }
        }
    }
    Some((port?, password?))
}

pub fn start(server_dir: &Path, java: &Path) -> Result<RunningServer> {
    let metadata = load_metadata(server_dir)?;
    validate_java(java, metadata.java_version)?;
    if let Some(pid) = recorded_running_pid(server_dir) {
        return Err(Error::ServerCreation(format!(
            "서버가 이미 실행 중입니다 (PID: {pid})"
        )));
    }
    let lock = ServerLock::acquire(server_dir)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(server_dir.join("logs").join("latest.log"))?;
    let log_error = log.try_clone()?;

    let mut cmd = Command::new(java);
    cmd.current_dir(server_dir)
        .args([
            format!("-Xms{}M", metadata.ram_mb),
            format!("-Xmx{}M", metadata.ram_mb),
            "-jar".into(),
            "server.jar".into(),
            "nogui".into(),
        ])
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_error));

    // Windows에서 완전 분리된 프로세스로 실행 (CLI 종료 후에도 서버 유지)
    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    let child = cmd.spawn()?;
    let pid = child.id();
    fs::write(pid_path(server_dir), format!("{pid}\n"))?;
    Ok(RunningServer { _lock: lock, pid })
}

pub fn recorded_running_pid(server_dir: &Path) -> Option<u32> {
    let pid: u32 = fs::read_to_string(pid_path(server_dir))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    process_exists(pid).then_some(pid)
}

pub async fn stop(server_dir: &Path) -> Result<()> {
    let pid = recorded_running_pid(server_dir)
        .ok_or_else(|| Error::ServerCreation("실행 중인 서버를 찾을 수 없습니다".into()))?;

    // 1. RCON으로 정상 종료 시도
    if let Some((port, password)) = rcon_settings(server_dir) {
        let _ = rcon::command(&format!("127.0.0.1:{port}"), &password, "stop").await;
    }

    // 2. 최대 30초 대기하며 정상 종료 확인
    let until = SystemTime::now() + Duration::from_secs(30);
    while SystemTime::now() < until {
        if !process_exists(pid) {
            let _ = fs::remove_file(pid_path(server_dir));
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    // 3. 강제 종료
    let status = if cfg!(windows) {
        Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()?
    } else {
        Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()?
    };

    if !status.success() {
        return Err(Error::ServerCreation(
            "서버 프로세스를 강제 종료하지 못했습니다".into(),
        ));
    }

    let _ = fs::remove_file(pid_path(server_dir));
    Ok(())
}

pub async fn restart(server_dir: &Path, java: &Path) -> Result<RunningServer> {
    if recorded_running_pid(server_dir).is_some() {
        stop(server_dir).await?;
    }
    start(server_dir, java)
}

pub fn latest_log_tail(server_dir: &Path, lines: usize) -> Result<String> {
    let text = fs::read_to_string(server_dir.join("logs").join("latest.log"))?;
    let mut items: Vec<&str> = text.lines().collect();
    let start = items.len().saturating_sub(lines);
    Ok(items.drain(start..).collect::<Vec<_>>().join("\n"))
}

/// 실시간 로그 스트리밍 (`tail -f` 스타일).
/// `on_line` 콜백으로 각 줄을 전달하며, `should_stop` 클로저가 true를 반환하면 중단합니다.
pub fn follow_log<F, S>(server_dir: &Path, mut on_line: F, mut should_stop: S) -> Result<()>
where
    F: FnMut(&str),
    S: FnMut() -> bool,
{
    let log_path = server_dir.join("logs").join("latest.log");
    let file = OpenOptions::new().read(true).open(&log_path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    // 기존 내용 모두 출력
    while reader.read_line(&mut line)? > 0 {
        on_line(line.trim_end_matches('\n'));
        line.clear();
    }

    // 새 줄 대기하며 출력
    loop {
        if should_stop() {
            break;
        }
        match reader.read_line(&mut line) {
            Ok(0) => {
                std::thread::sleep(Duration::from_millis(200));
                line.clear();
            }
            Ok(_) => {
                on_line(line.trim_end_matches('\n'));
                line.clear();
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub fn set_property(server_dir: &Path, key: &str, value: &str) -> Result<()> {
    if key.trim().is_empty() || key.contains(['\n', '\r', '=']) || value.contains(['\n', '\r']) {
        return Err(Error::ServerCreation("서버 속성 형식이 올바르지 않습니다".into()));
    }
    let path = server_dir.join("server.properties");
    let original = fs::read_to_string(&path)?;
    let mut replaced = false;
    let mut result = Vec::new();
    for line in original.lines() {
        if line.split_once('=').map(|(current, _)| current == key).unwrap_or(false) {
            result.push(format!("{key}={value}")); replaced = true;
        } else { result.push(line.to_owned()); }
    }
    if !replaced { result.push(format!("{key}={value}")); }
    fs::write(path, result.join("\n") + "\n")?;
    Ok(())
}

fn process_exists(pid: u32) -> bool {
    if cfg!(windows) {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    } else {
        Path::new("/proc").join(pid.to_string()).exists()
    }
}

/// 서버 JAR 업데이트 (Paper/Vanilla 모두 지원).
/// 실행 중인 서버는 업데이트 불가. 기존 JAR을 보존하지 않고 교체.
/// 다운로드/검증 실패 시 기존 JAR 유지.
pub async fn update_server(server_dir: &Path) -> Result<()> {
    if recorded_running_pid(server_dir).is_some() {
        return Err(Error::ServerCreation(
            "실행 중인 서버는 업데이트할 수 없습니다. 먼저 중지하세요.".into(),
        ));
    }

    let metadata = load_metadata(server_dir)?;
    let api = MinecraftApi::new()?;

    // 최신 버전 다운로드 정보 가져오기
    let download = match metadata.kind {
        ServerKind::Vanilla => api.vanilla_download(&metadata.minecraft_version).await?,
        ServerKind::Paper => api.paper_download(&metadata.minecraft_version).await?,
    };

    let jar_path = server_dir.join("server.jar");
    let backup_jar = server_dir.join("server.jar.backup");

    // 기존 JAR 백업 (롤백용)
    if jar_path.exists() {
        fs::copy(&jar_path, &backup_jar)?;
    }

    // 새 JAR 다운로드 및 검증
    let result = api.download_verified(&download, &jar_path).await;

    // 실패 시 롤백
    if result.is_err() {
        if backup_jar.exists() {
            let _ = fs::copy(&backup_jar, &jar_path);
            let _ = fs::remove_file(&backup_jar);
        }
        return Err(Error::ServerCreation(
            "업데이트 실패: 기존 JAR을 유지합니다.".into(),
        ));
    }

    // 성공 시 백업 파일 삭제
    if backup_jar.exists() {
        let _ = fs::remove_file(&backup_jar);
    }

    // 메타데이터 업데이트 (Paper 빌드 번호 등)
    let mut updated_metadata = metadata;
    updated_metadata.paper_build = download.paper_build;
    updated_metadata.last_exit_status = Some("updated".into());
    fs::write(
        server_dir.join(".golani.json"),
        serde_json::to_vec_pretty(&updated_metadata)?,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_pid_is_not_running() {
        assert!(recorded_running_pid(Path::new("no-such-golani-server")).is_none());
    }
}
