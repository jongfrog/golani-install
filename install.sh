#!/usr/bin/env bash
# golani Linux/macOS 설치 스크립트
# 사용법: curl -fsSL https://get.golani.kro.kr/install.sh | bash

set -euo pipefail

# 색상 정의
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

REPO="jongfrog/golani-install"
INSTALL_DIR="$HOME/.local/bin"
GOLANI_HOME="$HOME/.golani"

# 로그 함수
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# OS/Arch 감지
detect_platform() {
    local os arch
    case "$(uname -s)" in
        Linux*) os="linux" ;;
        Darwin*) os="macos" ;;
        *) log_error "지원하지 않는 OS입니다: $(uname -s)"; exit 1 ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) log_error "지원하지 않는 아키텍처입니다: $(uname -m)"; exit 1 ;;
    esac
    echo "${os}-${arch}"
}

# 최신 릴리스 정보 가져오기
get_latest_release() {
    local api_url="https://api.github.com/repos/${REPO}/releases/latest"
    curl -sSL -H "User-Agent: golani-installer" "$api_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

# 다운로드 URL 구성
get_download_url() {
    local platform="$1"
    local tag="$2"
    echo "https://github.com/${REPO}/releases/download/${tag}/golani-${platform}.tar.gz"
}

# 메인
main() {
    echo -e "${GREEN}======================================${NC}"
    echo -e "${GREEN}  golani 설치 프로그램${NC}"
    echo -e "${GREEN}======================================${NC}"
    echo

    # 플랫폼 감지
    PLATFORM=$(detect_platform)
    log_info "플랫폼: $PLATFORM"

    # 최신 버전 확인
    log_info "최신 버전 확인 중..."
    TAG=$(get_latest_release)
    if [ -z "$TAG" ]; then
        log_error "최신 버전을 가져올 수 없습니다."
        exit 1
    fi
    log_info "최신 버전: $TAG"

    # 다운로드 URL
    DOWNLOAD_URL=$(get_download_url "$PLATFORM" "$TAG")
    log_info "다운로드: $DOWNLOAD_URL"

    # 임시 디렉토리
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT

    # 다운로드
    log_info "다운로드 중..."
    if ! curl -sSL -o "$TMP_DIR/golani.tar.gz" "$DOWNLOAD_URL"; then
        log_error "다운로드 실패"
        exit 1
    fi

    # 압축 해제
    log_info "압축 해제 중..."
    tar -xzf "$TMP_DIR/golani.tar.gz" -C "$TMP_DIR"

    # 설치 디렉토리 생성
    mkdir -p "$INSTALL_DIR"
    mkdir -p "$GOLANI_HOME"

    # 설치
    log_info "설치 중: $INSTALL_DIR/golani"
    mv "$TMP_DIR/golani" "$INSTALL_DIR/golani"
    chmod +x "$INSTALL_DIR/golani"

    # PATH 확인
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        log_warn "$INSTALL_DIR이 PATH에 없습니다."
        log_info "다음 줄을 ~/.bashrc 또는 ~/.zshrc에 추가하세요:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    fi

    # 데이터 디렉토리 초기화
    log_info "데이터 디렉토리 초기화: $GOLANI_HOME"
    "$INSTALL_DIR/golani" >/dev/null 2>&1 || true

    log_success "설치 완료!"
    echo
    echo "사용법:"
    echo "  golani              # TUI 실행"
    echo "  golani new paper myserver 1.21.4  # 서버 생성"
    echo "  golani web          # Web UI 실행 (http://127.0.0.1:8788)"
    echo
    log_info "문서: https://golani.kro.kr"
}

main "$@"