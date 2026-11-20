#!/usr/bin/env bash
# 체크섬 생성 스크립트
# 사용법: ./generate_checksums.sh <version>

set -euo pipefail

VERSION="${1:-dev}"
DIST_DIR="dist/${VERSION}"

echo "체크섬 생성: ${VERSION}"

mkdir -p "${DIST_DIR}"

# 플랫폼별 빌드
echo "빌드 중..."

# Windows x64
echo "Windows x64 빌드..."
cargo build --release --target x86_64-pc-windows-msvc
cp target/x86_64-pc-windows-msvc/release/golani.exe "${DIST_DIR}/golani-windows-x64.exe"

# Windows ARM64
echo "Windows ARM64 빌드..."
cargo build --release --target aarch64-pc-windows-msvc
cp target/aarch64-pc-windows-msvc/release/golani.exe "${DIST_DIR}/golani-windows-arm64.exe"

# Linux x64 (musl)
echo "Linux x64 (musl) 빌드..."
cargo build --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/golani "${DIST_DIR}/golani-linux-x64"

# Linux ARM64 (musl)
echo "Linux ARM64 (musl) 빌드..."
cargo build --release --target aarch64-unknown-linux-musl
cp target/aarch64-unknown-linux-musl/release/golani "${DIST_DIR}/golani-linux-arm64"

# macOS x64
echo "macOS x64 빌드..."
cargo build --release --target x86_64-apple-darwin
cp target/x86_64-apple-darwin/release/golani "${DIST_DIR}/golani-macos-x64"

# macOS ARM64
echo "macOS ARM64 빌드..."
cargo build --release --target aarch64-apple-darwin
cp target/aarch64-apple-darwin/release/golani "${DIST_DIR}/golani-macos-arm64"

# 체크섬 생성
echo "체크섬 생성..."
cd "${DIST_DIR}"
sha256sum * > CHECKSUMS.txt

echo "완료: ${DIST_DIR}/CHECKSUMS.txt"
cat CHECKSUMS.txt