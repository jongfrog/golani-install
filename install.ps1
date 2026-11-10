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

        Write-Host "+--------------------------------------------------+"
        Write-Host "|                                                  |"
        Write-Host "|                  GOLANI INSTALLER                |"
        Write-Host "|                                                  |"
        Write-Host "|               What would you like?               |"
        Write-Host "|                                                  |"

        for ($i = 0; $i -lt $options.Count; $i++) {
            if ($i -eq $selected) {
                Write-Host ("|   " + "> " + $options[$i].PadRight(42) + " |") -ForegroundColor Magenta
            }
            else {
                Write-Host ("|     " + $options[$i].PadRight(42) + " |")
            }
        }

        Write-Host "|                                                  |"
        Write-Host "|          UP / DOWN : Move    ENTER : Select       |"
        Write-Host "|                    ESC : Exit                    |"
        Write-Host "+--------------------------------------------------+"

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

$choice = Show-GolaniMenu

switch ($choice) {
    "Install GOLANI" {
        Write-Host "Starting GOLANI installation..." -ForegroundColor Green
    }
    "Update GOLANI" {
        Write-Host "Starting GOLANI update..." -ForegroundColor Green
    }
    "Uninstall GOLANI" {
        Write-Host "Starting GOLANI removal..." -ForegroundColor Yellow
    }
    "Exit" {
        Write-Host "Goodbye!"
        exit
    }
}
