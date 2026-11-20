use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("파일 시스템 오류: {0}")]
    Io(#[from] io::Error),
    #[error("설정 파일을 읽을 수 없습니다: {0}")]
    TomlDecode(#[from] toml::de::Error),
    #[error("설정 파일을 저장할 수 없습니다: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error("사용자 데이터 디렉터리를 찾을 수 없습니다")]
    DataDirectoryUnavailable,
    #[error("이미 golani가 실행 중입니다 (PID: {pid}, 잠금 파일: {path})")]
    AlreadyRunning { pid: u32, path: PathBuf },
    #[error("잠금 파일이 손상되었습니다: {0}")]
    InvalidLock(PathBuf),
    #[error("네트워크 오류: {0}")]
    Network(#[from] reqwest::Error),
    #[error("JSON 처리 오류: {0}")]
    Json(#[from] serde_json::Error),
    #[error("다운로드 검증 실패: {0}")]
    Verification(String),
    #[error("서버 생성 오류: {0}")]
    ServerCreation(String),
}

pub type Result<T> = std::result::Result<T, Error>;
