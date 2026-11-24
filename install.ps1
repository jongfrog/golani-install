<# 
.SYNOPSIS
    golani Windows 설치 스크립트

.DESCRIPTION
    golani Minecraft 서버 관리 도구를 Windows에 설치합니다.

.USAGE
    powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://get.golani.kro.kr/install.ps1 | iex"
#>

[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12

$ErrorActionPreference = "Stop"

# 색상 함수
function Write-Info { param($msg) Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Write-Success { param($msg) Write-Host "[SUCCESS] $msg" -ForegroundColor Green }
function Write-Warn { param($msg) Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Write-Error { param($msg) Write-Host "[ERROR] $msg" -ForegroundColor Red }

# 설정
$Repo = "jongfrog/golani-install"
$InstallDir = "$env:LOCALAPPDATA\golani"
$GolaniHome = "$env:USERPROFILE\.golani"

Write-Host "`n======================================" -ForegroundColor Green
Write-Host "  golani Windows 설치 프로그램" -ForegroundColor Green
Write-Host "======================================`n" -ForegroundColor Green

# 아키텍처 감지
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x64" }
$Platform = "windows-$Arch"
Write-Info "플랫폼: $Platform"

# 최신 릴리스 조회
Write-Info "최신 버전 확인 중..."
$ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
$Headers = @{ "User-Agent" = "golani-installer" }
try {
    $Release = Invoke-RestMethod -Uri $ApiUrl -Headers $Headers -ErrorAction Stop
    $Tag = $Release.tag_name
    if (-not $Tag) { throw "태그를 찾을 수 없음" }
    Write-Info "최신 버전: $Tag"
} catch {
    Write-Error "최신 버전 조회 실패: $_"
    exit 1
}

# 다운로드 URL
$DownloadUrl = "https://github.com/$Repo/releases/download/$Tag/golani-$Platform.zip"
Write-Info "다운로드: $DownloadUrl"

# 임시 디렉토리
$TempDir = [System.IO.Path]::GetTempPath() + "golani-install-" + [Guid]::NewGuid()
New-Item -ItemType Directory -Path $TempDir | Out-Null

# 다운로드
Write-Info "다운로드 중..."
try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile "$TempDir\golani.zip" -Headers $Headers -ErrorAction Stop
} catch {
    Write-Error "다운로드 실패: $_"
    exit 1
}

# 압축 해제
Write-Info "압축 해제 중..."
Expand-Archive -Path "$TempDir\golani.zip" -DestinationPath $TempDir -Force

# 압축 해제된 exe 찾기 (Expand-Archive가 서브디렉토리 생성할 수 있음)
$ExePath = Get-ChildItem -Path $TempDir -Filter "golani.exe" -Recurse | Select-Object -First 1 -ExpandProperty FullName
if (-not $ExePath) {
    Write-Error "압축 해제 후 golani.exe를 찾을 수 없음"
    exit 1
}

# 설치 디렉토리 생성
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
New-Item -ItemType Directory -Path $GolaniHome -Force | Out-Null

# 설치
Write-Info "설치 중: $InstallDir\golani.exe"
# 기존 프로세스 종료
$Processes = Get-Process -Name "golani" -ErrorAction SilentlyContinue
if ($Processes) {
    Write-Warn "기존 golani 프로세스 종료 중..."
    Stop-Process -Name "golani" -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
}
if (Test-Path "$InstallDir\golani.exe") {
    Remove-Item -Path "$InstallDir\golani.exe" -Force -ErrorAction SilentlyContinue
}
Move-Item -Path $ExePath -Destination "$InstallDir\golani.exe" -Force

# PATH 등록
$CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($CurrentPath -notlike "*$InstallDir*") {
    Write-Warn "$InstallDir이 사용자 PATH에 없습니다."
    Write-Info "PATH에 추가 중..."
    [Environment]::SetEnvironmentVariable("Path", "$CurrentPath;$InstallDir", "User")
    Write-Success "PATH에 추가됨 (새 터미널에서 적용됨)"
}

# 데이터 디렉토리 초기화 (버전 확인만으로 설정 디렉토리 생성)
Write-Info "데이터 디렉토리 초기화: $GolaniHome"
& "$InstallDir\golani.exe" --version 2>$null | Out-Null

# 정리
Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "`n======================================" -ForegroundColor Green
Write-Success "설치 완료!"
Write-Host "======================================`n"

Write-Host "사용법:"
Write-Host "  golani              # TUI 실행"
Write-Host "  golani new paper myserver 1.21.4  # 서버 생성"
Write-Host "  golani web          # Web UI 실행 (http://127.0.0.1:8788)`n"
Write-Info "문서: https://golani.kro.kr"

# 새 PowerShell 세션에서 PATH 적용 안내
Write-Warn "변경된 PATH를 적용하려면 새 PowerShell 창을 여세요."