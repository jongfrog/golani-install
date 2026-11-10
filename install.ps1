
# ==========================================
# GOLANI INSTALLER
# ==========================================

$White = "White"
$Gray = "DarkGray"

# ==========================================
# 콘솔 상태 복구
# ==========================================

$OriginalForeground = [Console]::ForegroundColor
$OriginalBackground = [Console]::BackgroundColor

function Restore-Console {
    try {
        [Console]::ForegroundColor = $OriginalForeground
        [Console]::BackgroundColor = $OriginalBackground
        [Console]::CursorVisible = $true
    }
    catch {
    }
}

# Ctrl+C 처리
$global:GolaniInterrupted = $false

$CancelHandler = {
    $global:GolaniInterrupted = $true
    [Console]::CancelKeyPressEventArgs
}

$ConsoleCancelHandler = [ConsoleCancelEventHandler] {
    param($sender, $eventArgs)

    $eventArgs.Cancel = $true
    $global:GolaniInterrupted = $true
}

[Console]::add_CancelKeyPress($ConsoleCancelHandler)

# ==========================================
# 로고
# ==========================================

function Show-Banner {

    $logo = @'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
'@

    Write-Host $logo -ForegroundColor $White
}

# ==========================================
# 메뉴
# ==========================================

function Show-GolaniMenu {

    $options = @(
        "Install GOLANI"
        "Update GOLANI"
        "Uninstall GOLANI"
        "Exit"
    )

    $selected = 0

    while ($true) {

        if ($global:GolaniInterrupted) {
            return "Exit"
        }

        Clear-Host

        Show-Banner

        Write-Host ""
        Write-Host "  GOLANI INSTALLER" -ForegroundColor $White
        Write-Host "  Your personal installation manager" -ForegroundColor $Gray

        Write-Host ""
        Write-Host "  Select an action:"
        Write-Host ""

        for ($i = 0; $i -lt $options.Count; $i++) {

            if ($i -eq $selected) {
                Write-Host ("  ❯  " + $options[$i]) -ForegroundColor $White
            }
            else {
                Write-Host ("     " + $options[$i]) -ForegroundColor $Gray
            }
        }

        Write-Host ""
        Write-Host "  ↑ ↓  Navigate     ENTER  Select     ESC  Exit" -ForegroundColor $Gray

        try {
            $key = [Console]::ReadKey($true)
        }
        catch {
            return "Exit"
        }

        if ($global:GolaniInterrupted) {
            return "Exit"
        }

        switch ($key.Key) {

            "UpArrow" {
                if ($selected -gt 0) {
                    $selected--
                }
            }

            "DownArrow" {
                if ($selected -lt ($options.Count - 1)) {
                    $selected++
                }
            }

            "Enter" {
                return $options[$selected]
            }

            "Escape" {
                return "Exit"
            }
        }
    }
}

# ==========================================
# 실행
# ==========================================

try {

    $choice = Show-GolaniMenu

    if ($global:GolaniInterrupted) {
        Write-Host ""
        Write-Host "GOLANI installer closed." -ForegroundColor $Gray
        exit 130
    }

    Clear-Host

    Show-Banner

    switch ($choice) {

        "Install GOLANI" {

            Write-Host ""
            Write-Host "Starting GOLANI installation..." -ForegroundColor $White

            # 설치 코드 연결 위치

        }

        "Update GOLANI" {

            Write-Host ""
            Write-Host "Starting GOLANI update..." -ForegroundColor $White

            # 업데이트 코드 연결 위치

        }

        "Uninstall GOLANI" {

            Write-Host ""
            Write-Host "Starting GOLANI uninstall..." -ForegroundColor $White

            # 삭제 코드 연결 위치

        }

        "Exit" {

            Write-Host ""
            Write-Host "Goodbye!" -ForegroundColor $Gray

        }
    }

}
finally {

    [Console]::remove_CancelKeyPress($ConsoleCancelHandler)
    Restore-Console

}
