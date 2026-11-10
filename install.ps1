# =====================================================================
# GOLANI INSTALLER  (Windows PowerShell 5.1 / PowerShell 7)
# ---------------------------------------------------------------------
#  실행 방법:
#    powershell -ExecutionPolicy Bypass -File install.ps1
#      -> 대화형 메뉴 (UP/DOWN + ENTER)
#    install.ps1 install|update|uninstall|help   -> 바로 실행
#    irm http://get-golani.kro.kr/install.ps1 | iex
#      -> 아래 부트스트랩이 인코딩 문제를 자동 복구
#
#  종료 안전:
#    - 메뉴에서는 Ctrl+C 를 입력 키로 받아 "Goodbye!" 화면으로 깔끔 종료
#    - 작업 실행 중 Ctrl+C 는 즉시 중단 + finally 에서 콘솔 상태 복구
#
#  저장 형식: 반드시 "BOM 없는 UTF-8" 로 저장/배포할 것!
#  부트스트랩 영역(아래 self-heal 부분)은 반드시 순수 ASCII 로 유지할 것!
# =====================================================================

# ---- encoding self-heal bootstrap (ASCII only) ----
$__Url = 'http://get-golani.kro.kr/install.ps1'
$__Enc = '█'   # must be exactly 1 char; 2~3 chars => mis-decoded download
if ($__Enc.Length -ne 1) {
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        $__wc = New-Object Net.WebClient
        $__wc.Proxy.Credentials = [Net.CredentialCache]::DefaultCredentials
        iex ([Text.Encoding]::UTF8.GetString($__wc.DownloadData($__Url)))
    }
    catch {
        Write-Host '  [!] Encoding auto-repair failed (offline?).' -ForegroundColor Yellow
        Write-Host '      Download install.ps1 and run it directly:' -ForegroundColor Yellow
        Write-Host '      powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1' -ForegroundColor Yellow
    }
    return
}

# UTF-8 출력 설정
$Utf8 = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = $Utf8
$OutputEncoding = $Utf8

# ------------------------------------------------------------ 설정 (필요하면 수정)
$Script:UserHome = if ($env:USERPROFILE) { $env:USERPROFILE } elseif ($env:HOME) { $env:HOME } else { $PWD.Path }
$Script:GolaniHome = Join-Path $Script:UserHome '.golani'    # 설치 루트
$Script:GolaniBin  = Join-Path $Script:GolaniHome 'bin'      # 실행파일 위치
$Script:MenuItems  = @('Install GOLANI', 'Update GOLANI', 'Uninstall GOLANI', 'Exit')

# ------------------------------------------------------------ 로고
$Logo = @'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
'@

# ------------------------------------------------------------ 출력 헬퍼
function Write-Logo { Write-Host $Logo -ForegroundColor White }

function Step($msg) { Write-Host "  [*] $msg" -ForegroundColor Green }
function Info($msg) { Write-Host "  [i] $msg" -ForegroundColor DarkGray }
function Warn($msg) { Write-Host "  [!] $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "  [x] $msg" -ForegroundColor Red }

function Show-Header([string]$Title) {
    Clear-Host
    Write-Logo
    Write-Host ''
    Write-Host "  $Title" -ForegroundColor White
    Write-Host ''
}

function Wait-AnyKey {
    try {
        Write-Host '  아무 키나 누르면 메뉴로 돌아갑니다...' -ForegroundColor DarkGray
        [Console]::ReadKey($true) | Out-Null
    }
    catch { }   # 비대화형 환경이면 건너뜀
}

# ------------------------------------------------------------ 설치 액션
# 실제 설치 로직은 Invoke-InstallPayload / Invoke-UpdatePayload 에 작성하세요.
function Invoke-InstallPayload {
    # 예)
    #   $url = 'https://example.com/golani.zip'
    #   Invoke-WebRequest $url -OutFile "$env:TEMP\golani.zip"
    #   Expand-Archive "$env:TEMP\golani.zip" $Script:GolaniHome -Force
    Warn '설치 로직이 아직 구성되지 않았습니다. (Invoke-InstallPayload 함수를 편집하세요)'
}

function Invoke-UpdatePayload {
    Warn '업데이트 로직이 아직 구성되지 않았습니다. (Invoke-UpdatePayload 함수를 편집하세요)'
}

function Invoke-Install {
    Show-Header 'INSTALL GOLANI'
    Step "설치 경로 확인: $Script:GolaniBin"
    try {
        New-Item -ItemType Directory -Force -Path $Script:GolaniBin | Out-Null
        Info '디렉터리 준비 완료'
    }
    catch {
        Fail "디렉터리 생성 실패: $_"
        Wait-AnyKey
        return
    }
    Invoke-InstallPayload
    Wait-AnyKey
}

function Invoke-Update {
    Show-Header 'UPDATE GOLANI'
    Step "설치 경로 확인: $Script:GolaniHome"
    if (-not (Test-Path -LiteralPath $Script:GolaniHome)) {
        Info '설치된 GOLANI 가 없습니다. 먼저 Install 을 실행하세요.'
        Wait-AnyKey
        return
    }
    Invoke-UpdatePayload
    Wait-AnyKey
}

function Invoke-Uninstall {
    Show-Header 'UNINSTALL GOLANI'
    if (-not (Test-Path -LiteralPath $Script:GolaniHome)) {
        Info "제거할 항목이 없습니다. ($Script:GolaniHome 없음)"
        Wait-AnyKey
        return
    }
    # 안전장치: 사용자 홈 자체나 드라이브 루트는 제거 금지
    $homeFull = [IO.Path]::GetFullPath($Script:UserHome)
    $target   = [IO.Path]::GetFullPath($Script:GolaniHome)
    if ($target -eq $homeFull -or $target.TrimEnd('\').Length -le 3) {
        Fail "안전상 제거할 수 없는 경로입니다: $Script:GolaniHome"
        Wait-AnyKey
        return
    }
    Write-Host "  $Script:GolaniHome 을(를) 완전히 제거할까요? [y/N]: " -NoNewline -ForegroundColor Yellow
    $ans = 'n'
    try { $ans = [Console]::ReadKey($true).Key.ToString() }
    catch {
        Write-Host ''
        Info '대화형 콘솔이 아니어서 취소합니다.'
        return
    }
    Write-Host $ans
    if ($ans -eq 'y' -or $ans -eq 'Y') {
        try {
            Remove-Item -LiteralPath $Script:GolaniHome -Recurse -Force
            Step "제거 완료: $Script:GolaniHome"
        }
        catch {
            Fail "제거 실패: $_"
        }
    }
    else {
        Info '취소했습니다.'
    }
    Wait-AnyKey
}

# ------------------------------------------------------------ 메뉴
$Script:SelectedIndex = 0

function Show-Menu {
    $Host.UI.RawUI.WindowTitle = 'GOLANI INSTALLER'
    Clear-Host
    Write-Logo
    Write-Host ''
    Write-Host '  GOLANI INSTALLER' -ForegroundColor White
    Write-Host '  Your personal installation manager' -ForegroundColor DarkGray
    Write-Host ''
    Write-Host '  UP/DOWN Move · ENTER Select · ESC / Ctrl+C Exit' -ForegroundColor DarkGray
    Write-Host ''
    for ($i = 0; $i -lt $Script:MenuItems.Count; $i++) {
        if ($i -eq $Script:SelectedIndex) {
            Write-Host "  >  $($Script:MenuItems[$i])" -ForegroundColor White
        }
        else {
            Write-Host "     $($Script:MenuItems[$i])" -ForegroundColor DarkGray
        }
    }
    Write-Host ''
}

function Exit-Installer {
    Clear-Host
    Write-Host ''
    Write-Host '  Goodbye!' -ForegroundColor DarkGray
    Write-Host ''
    exit 0
}

function Show-Usage {
    Write-Logo
    Write-Host ''
    Write-Host '  Usage: install.ps1 [install|update|uninstall|help]' -ForegroundColor White
    Write-Host ''
    Write-Host '  인자 없이 실행하면 대화형 메뉴가 열립니다.' -ForegroundColor DarkGray
    Write-Host ''
}

# ------------------------------------------------------------ 메인
try {
    if ($args.Count -gt 0) {                 # CLI 직접 실행 모드
        switch ($args[0].ToString().ToLower()) {
            'install'   { Invoke-Install }
            'update'    { Invoke-Update }
            'uninstall' { Invoke-Uninstall }
            'help'      { Show-Usage }
            default     {
                Show-Usage
                Fail "알 수 없는 명령: $($args[0])"
                exit 1
            }
        }
        return
    }

    [Console]::TreatControlCAsInput = $true   # 메뉴에서 Ctrl+C 를 키로 받아 깔끔 종료
    while ($true) {
        Show-Menu
        $key = $null
        try {
            $key = [Console]::ReadKey($true)
        }
        catch {
            Write-Host ''
            Warn '대화형 콘솔을 사용할 수 없습니다. 사용법: install.ps1 install|update|uninstall|help'
            exit 1
        }

        $isCtrlC = ($key.Modifiers -band [ConsoleModifiers]::Control) -and ($key.Key -eq [ConsoleKey]::C)

        switch ($key.Key) {
            'UpArrow' {
                $Script:SelectedIndex = ($Script:SelectedIndex + $Script:MenuItems.Count - 1) % $Script:MenuItems.Count
            }
            'DownArrow' {
                $Script:SelectedIndex = ($Script:SelectedIndex + 1) % $Script:MenuItems.Count
            }
            'Enter' {
                # 작업 중에는 Ctrl+C 가 즉시 중단되도록 복구 (finally 에서 재설정)
                [Console]::TreatControlCAsInput = $false
                try {
                    switch ($Script:SelectedIndex) {
                        0 { Invoke-Install }
                        1 { Invoke-Update }
                        2 { Invoke-Uninstall }
                        3 { Exit-Installer }
                    }
                }
                finally {
                    [Console]::TreatControlCAsInput = $true
                }
            }
            'Escape' { Exit-Installer }
            default  { if ($isCtrlC) { Exit-Installer } }
        }
    }
}
catch {
    Clear-Host
    Write-Host ''
    Fail "Installer stopped: $_"
    Write-Host ''
    exit 1
}
finally {
    [Console]::TreatControlCAsInput = $false          # 콘솔 상태 복구
    try { [Console]::OutputEncoding = $Utf8 } catch { }
}
