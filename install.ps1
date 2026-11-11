# =====================================================================
# GOLANI INSTALLER  (Windows PowerShell 5.1 / PowerShell 7)
# ---------------------------------------------------------------------
#  irm http://get-golani.kro.kr/install.ps1 | iex      대화형 메뉴
#  powershell -ExecutionPolicy Bypass -File install.ps1 [명령]
#  명령: install [경로] | update | uninstall | help
#
#  저장 형식: BOM 없는 UTF-8  (부트스트랩 영역은 순수 ASCII 유지)
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
try { [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false) } catch { }

# ------------------------------------------------------------ 설정
 $Script:BaseUrl     = 'http://get-golani.kro.kr'
 $Script:UserHome    = if ($env:USERPROFILE) { $env:USERPROFILE } elseif ($env:HOME) { $env:HOME } else { $PWD.Path }
 $Script:DefaultRoot = Join-Path $Script:UserHome '.golani'                  # 기본 설치 경로
 $Script:MarkerFile  = Join-Path $Script:DefaultRoot 'install-path.txt'      # 실제 설치 경로 기록
 $Script:MenuItems   = @('Install GOLANI', 'Update GOLANI', 'Uninstall GOLANI', 'Exit')

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
        Write-Host '  아무 키나 누르면 계속...' -ForegroundColor DarkGray
        [Console]::ReadKey($true) | Out-Null
    }
    catch { }   # 비대화형 환경이면 건너뜀
}

# ------------------------------------------------------------ 네트워크
function Get-RemoteFile([string]$Url, [string]$OutFile) {
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }
    $wc = New-Object Net.WebClient
    $wc.Proxy.Credentials = [Net.CredentialCache]::DefaultCredentials
    $wc.DownloadFile($Url, $OutFile)
}

# ------------------------------------------------------------ 사용자 PATH (REG_EXPAND_SZ 보존)
function Send-EnvChange {
    # 열려 있는 프로그램에 PATH 변경을 즉시 통지 (재로그온 불필요)
    try {
        if (-not $Script:EnvChangeType) {
            $sig = @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
            $Script:EnvChangeType = Add-Type -MemberDefinition $sig -Name EnvBroadcast -Namespace Golani -PassThru
        }
        [UIntPtr]$result = [UIntPtr]::Zero
        $null = $Script:EnvChangeType::SendMessageTimeout([IntPtr]0xFFFF, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
    }
    catch { }
}

function Edit-UserPath {
    param([string]$AddDir = '', [string]$RemoveDir = '')
    try {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        if (-not $key) { return }
        $old   = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $parts = @($old -split ';' | Where-Object { $_.Trim() })
        $changed = $false
        if ($RemoveDir) {
            $rm   = $RemoveDir.Trim().TrimEnd('\')
            $kept = @($parts | Where-Object { $_.Trim().TrimEnd('\') -ne $rm })
            if ($kept.Count -ne $parts.Count) { $changed = $true; $parts = $kept }
        }
        if ($AddDir) {
            $ad = $AddDir.Trim().TrimEnd('\')
            if (-not ($parts | Where-Object { $_.Trim().TrimEnd('\') -eq $ad })) {
                $parts = @($parts) + $ad
                $changed = $true
            }
        }
        if ($changed) {
            $key.SetValue('Path', ($parts -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
            Send-EnvChange
        }
        $key.Close()
    }
    catch {
        Warn "사용자 PATH 갱신 실패: $_"
    }
}

# ------------------------------------------------------------ 설치 경로 유틸
function Convert-ToRoot([string]$Raw) {
    try {
        $p = $Raw.Trim().Trim('"').Trim()
        $p = [Environment]::ExpandEnvironmentVariables($p)
        if (-not $p) { return $null }
        return [IO.Path]::GetFullPath($p)
    }
    catch { return $null }
}

function Get-InstallRoot {
    try {
        if (Test-Path -LiteralPath $Script:MarkerFile) {
            $line = [string]([IO.File]::ReadAllLines($Script:MarkerFile) | Select-Object -First 1)
            if ($line.Trim()) { return $line.Trim() }
        }
    }
    catch { }
    return $Script:DefaultRoot
}

function Write-InstallMarker([string]$Root) {
    try {
        New-Item -ItemType Directory -Force -Path $Script:DefaultRoot | Out-Null
        [IO.File]::WriteAllText($Script:MarkerFile, $Root, [Text.UTF8Encoding]::new($true))   # BOM: PS5.1 판독용
    }
    catch {
        Warn '설치 경로 기록 실패 (Update/Uninstall 이 기본 경로를 사용합니다)'
    }
}

# ------------------------------------------------------------ 설치 코어
function Install-At {
    param([string]$Root, [string]$Title = 'INSTALL GOLANI')
    if (-not $Root) { Fail '올바른 경로가 아닙니다.'; return $false }
    $bin = Join-Path $Root 'bin'
    Show-Header $Title
    Step "설치 경로: $Root"
    try { New-Item -ItemType Directory -Force -Path $bin | Out-Null }
    catch { Fail "폴더 생성 실패: $_"; return $false }

    Step '배너 파일 다운로드 중...'
    try { Get-RemoteFile "$($Script:BaseUrl)/gorani_banner.py" (Join-Path $bin 'gorani_banner.py') }
    catch {
        Fail "다운로드 실패: $_"
        Info "서버 확인: $($Script:BaseUrl)/gorani_banner.py"
        return $false
    }

    # golani.cmd 실행 shim (ASCII 저장, 경로는 실행 시 %~dp0 로 해결)
    $shim = "@echo off`r`npython `"%~dp0gorani_banner.py`" %*`r`n"
    [IO.File]::WriteAllText((Join-Path $bin 'golani.cmd'), $shim, [Text.Encoding]::ASCII)

    if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
        Warn 'Python 이 없으면 golani 명령이 동작하지 않습니다. (python.org 에서 설치)'
    }
    Edit-UserPath -AddDir $bin
    Write-InstallMarker $Root

    Write-Host ''
    Step '설치 완료!  새 터미널을 열고  golani  를 입력해 보세요.'
    return $true
}

# ------------------------------------------------------------ 액션
function Invoke-Install {
    while ($true) {
        $items = @(
            "기본 경로에 설치   ($Script:DefaultRoot)",
            '경로를 지정해서 설치'
        )
        $i = Read-MenuChoice -Title 'INSTALL GOLANI' -Subtitle '설치 방법을 선택하세요.' -Items $items
        if ($i -lt 0) { Info '설치를 취소했습니다.'; Wait-AnyKey; return }      # ESC / Ctrl+C
        if ($i -eq 0) { Install-At $Script:DefaultRoot | Out-Null; Wait-AnyKey; return }

        # 경로 직접 지정
        Show-Header 'INSTALL GOLANI'
        Write-Host '  설치할 경로를 입력하세요.  예) D:\tools\golani' -ForegroundColor White
        Write-Host '  환경 변수 사용 가능 (%USERPROFILE% 등) · 따옴표 자동 제거 · 빈 입력 = 취소' -ForegroundColor DarkGray
        Write-Host ''
        $raw = Read-Host '  경로'
        if ([string]::IsNullOrWhiteSpace($raw)) { Info '설치를 취소했습니다.'; Wait-AnyKey; return }
        $root = Convert-ToRoot $raw
        if (-not $root) { Fail '올바른 경로가 아닙니다.'; Wait-AnyKey; continue }
        Install-At $root | Out-Null
        Wait-AnyKey
        return
    }
}

function Invoke-Update {
    $root = Get-InstallRoot
    Show-Header 'UPDATE GOLANI'
    if (-not (Test-Path -LiteralPath (Join-Path (Join-Path $root 'bin') 'gorani_banner.py'))) {
        Info '설치된 GOLANI 가 없습니다. 먼저 Install 을 실행하세요.'
        Wait-AnyKey
        return
    }
    Install-At -Root $root -Title 'UPDATE GOLANI' | Out-Null
    Wait-AnyKey
}

function Invoke-Uninstall {
    $root = Get-InstallRoot
    Show-Header 'UNINSTALL GOLANI'
    if (-not (Test-Path -LiteralPath $root)) {
        Info "제거할 항목이 없습니다. ($root 없음)"
        Wait-AnyKey
        return
    }
    Info "설치 경로: $root"
    Write-Host ''
    Write-Host "  $root 을(를) 완전히 제거할까요? [y/N]: " -NoNewline -ForegroundColor Yellow
    try {
        $ans = [Console]::ReadKey($true).Key.ToString()
        Write-Host $ans
    }
    catch {
        Write-Host ''
        Info '대화형 콘솔이 아니어서 취소합니다.'
        return
    }
    if ($ans -ne 'y' -and $ans -ne 'Y') { Info '취소했습니다.'; Wait-AnyKey; return }
    try {
        Remove-Item -LiteralPath $root -Recurse -Force
        Step "제거 완료: $root"
    }
    catch {
        Fail "제거 실패: $_"
        Wait-AnyKey
        return
    }
    Edit-UserPath -RemoveDir (Join-Path $root 'bin')
    try {   # 설치 경로가 기본 경로가 아니면 마커도 정리
        if ($root -ne $Script:DefaultRoot -and (Test-Path -LiteralPath $Script:MarkerFile)) {
            Remove-Item -LiteralPath $Script:MarkerFile -Force
            if (@(Get-ChildItem -LiteralPath $Script:DefaultRoot -Force).Count -eq 0) {
                Remove-Item -LiteralPath $Script:DefaultRoot -Force
            }
        }
    }
    catch { }
    Wait-AnyKey
}

# ------------------------------------------------------------ 메뉴 공용
function Read-MenuChoice {
    param([string]$Title, [string]$Subtitle = '', [string[]]$Items, [string]$KeyHint = 'UP/DOWN 이동 · ENTER 선택 · ESC 취소')
    $sel = 0
    $prevCtrlC = $false
    try { $prevCtrlC = [Console]::TreatControlCAsInput } catch { }
    try {
        [Console]::TreatControlCAsInput = $true
        while ($true) {
            Show-Header $Title
            if ($Subtitle) { Write-Host "  $Subtitle" -ForegroundColor DarkGray; Write-Host '' }
            for ($i = 0; $i -lt $Items.Count; $i++) {
                if ($i -eq $sel) { Write-Host "  >  $($Items[$i])" -ForegroundColor White }
                else             { Write-Host "     $($Items[$i])" -ForegroundColor DarkGray }
            }
            Write-Host ''
            Write-Host "  $KeyHint" -ForegroundColor DarkGray
            Write-Host ''
            try { $key = [Console]::ReadKey($true) } catch { return -2 }   # 비대화형
            if (($key.Modifiers -band [ConsoleModifiers]::Control) -and ($key.Key -eq [ConsoleKey]::C)) { return -1 }
            switch ($key.Key) {
                'UpArrow'   { $sel = ($sel + $Items.Count - 1) % $Items.Count }
                'DownArrow' { $sel = ($sel + 1) % $Items.Count }
                'Enter'     { return $sel }
                'Escape'    { return -1 }
            }
        }
    }
    finally {
        try { [Console]::TreatControlCAsInput = $prevCtrlC } catch { }
    }
}

function Show-Usage {
    Write-Logo
    Write-Host ''
    Write-Host '  Usage: install.ps1 [install [경로] | update | uninstall | help]' -ForegroundColor White
    Write-Host ''
    Write-Host '  install 뒤에 경로를 주면 그 경로에, 없으면 대화형으로 설치합니다.' -ForegroundColor DarkGray
    Write-Host ''
}

function Exit-Installer {
    Clear-Host
    Write-Host ''
    Write-Host '  Goodbye!' -ForegroundColor DarkGray
    Write-Host ''
    exit 0
}

# ------------------------------------------------------------ 메인
try {
    try { $Host.UI.RawUI.WindowTitle = 'GOLANI INSTALLER' } catch { }

    if ($args.Count -gt 0) {                          # CLI 직접 실행 모드
        switch ($args[0].ToString().ToLower()) {
            'install' {
                if ($args.Count -ge 2) {
                    $root = Convert-ToRoot ([string]$args[1])
                    if (-not $root) { Fail "올바른 경로가 아닙니다: $($args[1])"; exit 1 }
                    if (-not (Install-At $root)) { exit 1 }
                }
                else { Invoke-Install }
            }
            'update'    { Invoke-Update }
            'uninstall' { Invoke-Uninstall }
            'help'      { Show-Usage }
            default     { Show-Usage; Fail "알 수 없는 명령: $($args[0])"; exit 1 }
        }
        return
    }

    while ($true) {                                   # 대화형 메뉴
        $i = Read-MenuChoice -Title 'GOLANI INSTALLER' -Items $Script:MenuItems -KeyHint 'UP/DOWN 이동 · ENTER 선택 · ESC 종료'
        switch ($i) {
            0 { Invoke-Install }
            1 { Invoke-Update }
            2 { Invoke-Uninstall }
            default { Exit-Installer }                # Exit / ESC / Ctrl+C / 비대화형
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
    try { [Console]::TreatControlCAsInput = $false } catch { }
}
