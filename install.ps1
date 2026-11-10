# ==========================================
# GOLANI INSTALLER
# ==========================================

$White = "White"
$Gray = "DarkGray"

# ------------------------------------------
# 로고 출력
# ------------------------------------------

function Show-Banner {

    Write-Host ""

    $logo = @'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██╗██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
'@

    Write-Host $logo -ForegroundColor $White
}

# ------------------------------------------
# 메뉴 출력 및 조작
# ------------------------------------------

function Show-GolaniMenu {

    $options = @(
        "Install GOLANI"
        "Update GOLANI"
        "Uninstall GOLANI"
        "Exit"
    )

    $selected = 0

    while ($true) {

        Clear-Host

        Show-Banner

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

        $key = [Console]::ReadKey($true)

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

# ------------------------------------------
# 프로그램 실행
# ------------------------------------------

$choice = Show-GolaniMenu

Clear-Host

Show-Banner

switch ($choice) {

    "Install GOLANI" {

        Write-Host "Starting GOLANI installation..." -ForegroundColor $White

        # TODO: 기존 설치 코드 연결

    }

    "Update GOLANI" {

        Write-Host "Starting GOLANI update..." -ForegroundColor $White

        # TODO: 기존 업데이트 코드 연결

    }

    "Uninstall GOLANI" {

        Write-Host "Starting GOLANI uninstall..." -ForegroundColor $White

        # TODO: 기존 삭제 코드 연결

    }

    "Exit" {

        Write-Host "Goodbye!" -ForegroundColor $Gray

        exit 0

    }
}
