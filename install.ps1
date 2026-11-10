# =====================================================================
# GOLANI INSTALLER
# ---------------------------------------------------------------------
# [ENCODING SELF-HEAL] irm|iex 로 실행하면 Windows PowerShell 5.1 이 응답을
# ISO-8859-1 로 잘못 디코딩하여 한글/블록문자가 깨진다. 아래 부트스트랩이
# 이를 감지하면 파일을 바이트 그대로 다시 내려받아 UTF-8 로 해석해 재실행.
#
# 주의:
#  1. 이 부트스트랩 영역은 반드시 순수 ASCII 로 유지할 것!
#  2. 이 파일은 "BOM 없는 UTF-8" 로 저장/배포할 것. (BOM 을 넣으면
#     ISO-8859-1 로 디코딩된 경로에서 파서가 오류를 낼 수 있음)
#  3. $__Url 은 이 파일의 실제 배포 URL 과 일치해야 한다.
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

# 색상 설정
$White = "White"
$Gray = "DarkGray"
$Reset = "White"

# 콘솔 설정
$Host.UI.RawUI.WindowTitle = "GOLANI INSTALLER"

# 로고
$Logo = @'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
'@

# 메뉴
$MenuItems = @(
    "Install GOLANI",
    "Update GOLANI",
    "Uninstall GOLANI",
    "Exit"
)

$SelectedIndex = 0
$Running = $true

# 화면 출력
function Show-Menu {
    Clear-Host

    Write-Host ""
    Write-Host $Logo -ForegroundColor $White
    Write-Host ""
    Write-Host "  GOLANI INSTALLER" -ForegroundColor $White
    Write-Host "  Your personal installation manager" -ForegroundColor $Gray
    Write-Host ""
    Write-Host "  Select an action:" -ForegroundColor $Gray
    Write-Host ""

    for ($i = 0; $i -lt $MenuItems.Count; $i++) {
        if ($i -eq $SelectedIndex) {
            Write-Host "  >  $($MenuItems[$i])" -ForegroundColor $White
        }
        else {
            Write-Host "     $($MenuItems[$i])" -ForegroundColor $Gray
        }
    }

    Write-Host ""
    Write-Host "  UP / DOWN  Navigate     ENTER  Select     ESC  Exit" -ForegroundColor $Gray
    Write-Host ""
}

# 종료
function Exit-Installer {
    Clear-Host
    Write-Host ""
    Write-Host "  Goodbye!" -ForegroundColor $Gray
    Write-Host ""
    exit
}

# 설치
function Install-GOLANI {
    Clear-Host
    Write-Host ""
    Write-Host "  INSTALL GOLANI" -ForegroundColor $White
    Write-Host ""

    Write-Host "  Installation process is not configured yet." -ForegroundColor $Gray
    Write-Host ""
    Write-Host "  Press any key to return..." -ForegroundColor $Gray

    [Console]::ReadKey($true) | Out-Null
}

# 업데이트
function Update-GOLANI {
    Clear-Host
    Write-Host ""
    Write-Host "  UPDATE GOLANI" -ForegroundColor $White
    Write-Host ""

    Write-Host "  Update process is not configured yet." -ForegroundColor $Gray
    Write-Host ""
    Write-Host "  Press any key to return..." -ForegroundColor $Gray

    [Console]::ReadKey($true) | Out-Null
}

# 삭제
function Uninstall-GOLANI {
    Clear-Host
    Write-Host ""
    Write-Host "  UNINSTALL GOLANI" -ForegroundColor $White
    Write-Host ""

    Write-Host "  Uninstall process is not configured yet." -ForegroundColor $Gray
    Write-Host ""
    Write-Host "  Press any key to return..." -ForegroundColor $Gray

    [Console]::ReadKey($true) | Out-Null
}

# 메인 메뉴
try {
    while ($Running) {
        Show-Menu

        $Key = [Console]::ReadKey($true)

        switch ($Key.Key) {
            "UpArrow" {
                $SelectedIndex--

                if ($SelectedIndex -lt 0) {
                    $SelectedIndex = $MenuItems.Count - 1
                }
            }

            "DownArrow" {
                $SelectedIndex++

                if ($SelectedIndex -ge $MenuItems.Count) {
                    $SelectedIndex = 0
                }
            }

            "Enter" {
                switch ($SelectedIndex) {
                    0 { Install-GOLANI }
                    1 { Update-GOLANI }
                    2 { Uninstall-GOLANI }
                    3 { Exit-Installer }
                }
            }

            "Escape" {
                Exit-Installer
            }
        }
    }
}
catch {
    Clear-Host
    Write-Host ""
    Write-Host "  Installer stopped." -ForegroundColor $Gray
    Write-Host ""
}
finally {
    [Console]::OutputEncoding = $Utf8
}
