use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::error::{Error, Result};

#[derive(Debug)]
pub struct GlobalLock {
    file: File,
    path: PathBuf,
}

/// 개별 Minecraft 서버의 중복 실행을 막는 잠금입니다.
/// 서버 디렉터리 안에 잠금을 두어 서버 이동·삭제 시 함께 관리할 수 있습니다.
#[derive(Debug)]
pub struct ServerLock {
    inner: GlobalLock,
}

impl ServerLock {
    pub fn acquire(server_directory: &Path) -> Result<Self> {
        fs::create_dir_all(server_directory)?;
        Ok(Self {
            inner: GlobalLock::acquire(&server_directory.join(".golani.lock"))?,
        })
    }

    pub fn path(&self) -> &Path {
        self.inner.path()
    }
}

impl GlobalLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;

        // Windows의 파일 잠금 충돌은 `WouldBlock` 대신 OS 오류 33으로 보고된다.
        // 잠금을 시도하기 전에 PID를 읽어 두면, Windows의 필수 잠금 상태에서도
        // 사용자에게 실행 중인 프로세스 정보를 안내할 수 있다.
        let recorded_pid = read_pid(&mut file).unwrap_or(0);
        if let Err(error) = file.try_lock_exclusive() {
            if error.kind() == std::io::ErrorKind::WouldBlock || error.raw_os_error() == Some(33) {
                return Err(Error::AlreadyRunning {
                    pid: recorded_pid,
                    path: path.to_path_buf(),
                });
            }
            return Err(error.into());
        }

        // 파일 잠금이 해제되어 남아 있는 PID는 비정상 종료의 흔적(STALE)입니다.
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "{}", std::process::id())?;
        file.sync_data()?;

        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for GlobalLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let _ = fs::remove_file(&self.path);
    }
}

fn read_pid(file: &mut File) -> Option<u32> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    contents.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn stale_lock_file_is_replaced() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("golani.lock");
        fs::write(&path, "999999\n").unwrap();

        let lock = GlobalLock::acquire(&path).unwrap();
        assert!(lock.path().is_file());
        drop(lock);
        assert!(!path.exists());
    }

    #[test]
    fn second_lock_is_rejected() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("golani.lock");
        let _first = GlobalLock::acquire(&path).unwrap();
        let error = GlobalLock::acquire(&path).unwrap_err();
        assert!(matches!(error, Error::AlreadyRunning { .. }));
    }

    #[test]
    fn server_lock_uses_its_own_server_directory() {
        let temp = tempdir().unwrap();
        let server = temp.path().join("survival");
        let lock = ServerLock::acquire(&server).unwrap();
        assert_eq!(lock.path(), server.join(".golani.lock"));
    }
}
