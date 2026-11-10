
# ==========================================
# GOLANI INSTALLER
# ==========================================

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
