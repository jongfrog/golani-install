# golani (고라니)

Minecraft Java Edition 서버를 간편하게 생성하고 관리할 수 있는 크로스 플랫폼 서버 관리 도구입니다.

![golani](https://img.shields.io/badge/rust-1.75+-orange.svg)
![platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)
![license](https://img.shields.io/badge/license-MIT-green.svg)

## 주요 기능

- **서버 생성 마법사** - 7단계 대화형 마법사로 Vanilla/Paper 서버 손쉽게 생성
- **다중 서버 관리** - 여러 서버 동시 관리 (시작/중지/재시작/상태 확인)
- **TUI (터미널 UI)** - 키보드 중심의 직관적인 텍스트 인터페이스
- **Web UI** - 브라우저 기반 관리 대시보드 (http://127.0.0.1:8788)
- **RCON 콘솔** - 실시간 서버 콘솔 접속 및 명령 히스토리 지원
- **자동 Java 관리** - 버전별 Java(Temurin) 자동 설치 및 버전 매핑
- **서버 업데이트** - JAR 다운로드/검증/롤백 자동화
- **방화벽 자동 설정** - Windows Defender / UFW / firewalld 규칙 자동 관리
- **설정 마이그레이션** - 서버 저장 위치 변경 시 데이터 자동 이동
- **크로스 플랫폼** - Windows / Linux / macOS 지원

## 설치

### Windows (PowerShell)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://get.golani.kro.kr/install.ps1 | iex"
```

### Linux / macOS (Bash)

```bash
curl -fsSL https://get.golani.kro.kr/install.sh | bash
```

### 수동 설치

1. [Releases](https://github.com/nousresearch/golani/releases)에서 플랫폼에 맞는 바이너리 다운로드
2. 실행 권한 부여 (`chmod +x golani` 또는 Windows에서는 그대로 사용)
3. PATH에 포함된 디렉토리에 배치 (예: `~/.local/bin`, `%LOCALAPPDATA%\golani`)

## 사용법

### TUI 실행 (기본)

```bash
golani
```

![TUI 스크린샷](docs/tui.png)

### 서버 생성

```bash
# 대화형 마법사 (TUI에서 'n' 키)
golani

# 또는 CLI로 직접 생성
golani new paper myserver 1.21.4 2048 25565
# golani new <종류> <이름> <버전> [RAM_MB] [포트]
```

### 서버 제어

```bash
golani start myserver      # 시작
golani stop myserver       # 중지
golani restart myserver    # 재시작
golani status myserver     # 상태 확인
golani logs myserver       # 로그 보기 (최근 100줄)
golani logs myserver -f    # 실시간 로그 추적
```

### RCON 콘솔 접속

```bash
golani attach myserver
# ESC로 분리 (서버는 계속 실행)
# ↑/↓로 명령 히스토리 탐색
```

### 서버 업데이트

```bash
golani update myserver
# 최신 빌드 다운로드 → 검증 → 교체 (실패 시 롤백)
```

### 서버 삭제

```bash
golani delete myserver 삭제
# 2단계 확인: y/N → '삭제' 입력
```

### Web UI

```bash
golani web
# http://127.0.0.1:8788 접속
```

![Web UI 스크린샷](docs/webui.png)

### 설정 관리

```bash
golani settings                    # 현재 설정 조회
golani settings path /new/path     # 서버 저장 위치 변경 (마이그레이션 포함)
golani settings java /path/to/java # 수동 Java 경로 지정
golani settings java --clear       # 수동 Java 경로 해제
```

## 지원 서버 종류

| 종류 | 설명 |
|------|------|
| **Vanilla** | 공식 Mojang 서버 |
| **Paper** | 고성능 최적화 서버 (권장) - Fill API v3 사용 |

## Java 버전 매핑

| Minecraft 버전 | Java 버전 |
|----------------|-----------|
| 1.20.5+        | 21        |
| 1.18 ~ 1.20.4  | 17        |
| 1.17           | 16        |
| 1.16.5 이하    | 8         |

Eclipse Temurin(Adoptium) 자동 설치 지원

## 데이터 디렉토리 구조

```
~/.golani/
├── config.toml          # 설정 파일
├── golani.lock          # 전역 락
├── java/
│   └── temurin-21/      # 자동 설치된 Java
├── servers/
│   └── myserver/
│       ├── .golani.json      # 서버 메타데이터
│       ├── server.jar        # 서버 JAR
│       ├── server.properties # 서버 설정
│       ├── eula.txt          # EULA 동의
│       ├── world/            # 월드 데이터
│       ├── plugins/          # 플러그인 (Paper)
│       └── logs/
│           └── latest.log    # 서버 로그
└── logs/                  # golani 자체 로그
```

## 방화벽 자동 관리

서버 생성/삭제/포트 변경 시 자동으로 방화벽 규칙 관리:

- **Windows**: `netsh advfirewall` (관리자 권한 필요 시 안내)
- **Linux (UFW)**: `ufw allow <port>/tcp`
- **Linux (firewalld)**: `firewall-cmd --add-port=<port>/tcp`

규칙 이름 형식: `golani-<서버명>-<포트>`

## 외부 접속 진단

```bash
golani diagnose myserver
```

확인 항목:
1. 서버 프로세스 실행 여부
2. 포트 리스닝 상태
3. 방화벽 규칙 존재
4. 로컬 IP 확인
5. 공유기 포트포워딩/ISP CGNAT 안내

## 빌드 (개발자용)

```bash
# 의존성 설치
rustup default stable

# 빌드
cargo build --release

# 테스트
cargo test

# 단일 바이너리 (정적 링크)
cargo build --release --target x86_64-pc-windows-gnu  # Windows
cargo build --release --target x86_64-unknown-linux-musl  # Linux
```

## 설정 파일 (config.toml)

```toml
servers_dir = "C:/Users/User/.golani/servers"
java_path = "C:/Program Files/Java/jdk-21"  # 선택사항
```

## 라이선스

MIT License - 자세한 내용은 [LICENSE](LICENSE) 참조

## 기여하기

1. Fork the repository
2. Create feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open Pull Request

## 문의

- Issues: [GitHub Issues](https://github.com/nousresearch/golani/issues)
- Documentation: [golani.kro.kr](https://golani.kro.kr)

---

**golani** - Minecraft 서버 관리를 더 간편하게 🦌