<# 
.SYNOPSIS
    릴리스 체크섬 생성 스크립트 (Windows PowerShell)

.DESCRIPTION
    크로스 플랫폼 빌드 및 SHA-256 체크섬 생성

.USAGE
    .\generate_checksums.ps1 -Version "v1.0.0"
#>

param(
    [Parameter(Mandatory=$true)]
    [string]$Version = "dev"
)

$ErrorActionPreference = "Stop"

Write-Host "체크섬 생성: $Version" -ForegroundColor Green

$DistDir = "dist/$Version"
New-Item -ItemType Directory -Path $DistDir -Force | Out-Null

# Rust 타겟 설치 확인
$Targets = @(
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc", 
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin"
)

foreach ($target in $Targets) {
    if (-not (rustup target list --installed | Select-String $target)) {
        Write-Host "타겟 설치: $target" -ForegroundColor Yellow
        rustup target add $target
    }
}

Write-Host "빌드 시작..." -ForegroundColor Cyan

# Windows x64
Write-Host "Windows x64 빌드..." -ForegroundColor Yellow
cargo build --release --target x86_64-pc-windows-msvc
Copy-Item "target\x86_64-pc-windows-msvc\release\golani.exe" "$DistDir\golani-windows-x64.exe"

# Windows ARM64
Write-Host "Windows ARM64 빌드..." -ForegroundColor Yellow
cargo build --release --target aarch64-pc-windows-msvc
Copy-Item "target\aarch64-pc-windows-msvc\release\golani.exe" "$DistDir\golani-windows-arm64.exe"

# Linux x64 (musl)
Write-Host "Linux x64 (musl) 빌드..." -ForegroundColor Yellow
cargo build --release --target x86_64-unknown-linux-musl
Copy-Item "target\x86_64-unknown-linux-musl\release\golani" "$DistDir\golani-linux-x64"

# Linux ARM64 (musl)
Write-Host "Linux ARM64 (musl) 빌드..." -ForegroundColor Yellow
cargo build --release --target aarch64-unknown-linux-musl
Copy-Item "target\aarch64-unknown-linux-musl\release\golani" "$DistDir\golani-linux-arm64"

# macOS x64
Write-Host "macOS x64 빌드..." -ForegroundColor Yellow
cargo build --release --target x86_64-apple-darwin
Copy-Item "target\x86_64-apple-darwin\release\golani" "$DistDir\golani-macos-x64"

# macOS ARM64
Write-Host "macOS ARM64 빌드..." -ForegroundColor Yellow
cargo build --release --target aarch64-apple-darwin
Copy-Item "target\aarch64-apple-darwin\release\golani" "$DistDir\golani-macos-arm64"

# 체크섬 생성
Write-Host "체크섬 생성..." -ForegroundColor Cyan
Set-Location $DistDir
$files = Get-ChildItem -File
$checksums = @()
foreach ($file in $files) {
    $hash = Get-FileHash -Algorithm SHA256 -Path $file.FullName
    $checksums += "$($hash.Hash)  $($file.Name)"
}
$checksums -join "`n" | Out-File -Encoding UTF8 "CHECKSUMS.txt"

Write-Host "완료: $DistDir\CHECKSUMS.txt" -ForegroundColor Green
Get-Content "CHECKSUMS.txt"