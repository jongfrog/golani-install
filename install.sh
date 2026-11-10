
#!/bin/bash

# ==========================================
# GOLANI INSTALLER
# ==========================================

WHITE='\033[97m'
GRAY='\033[90m'
BOLD='\033[1m'
RESET='\033[0m'

# ==========================================
# 터미널 상태 복구
# ==========================================

cleanup() {
    stty echo 2>/dev/null
    printf '\033[?25h'
    printf '\033[0m'
}

handle_interrupt() {
    printf '\n'
    echo "GOLANI installer closed."
    exit 130
}

trap cleanup EXIT
trap handle_interrupt INT
trap 'exit 143' TERM

# ==========================================
# 로고
# ==========================================

print_banner() {
    printf "${WHITE}${BOLD}"

    cat <<'EOF'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
EOF

    printf "${RESET}\n"
}

# ==========================================
# 메뉴
# ==========================================

show_menu() {
    local selected=0
    local key
    local options=(
        "Install GOLANI"
        "Update GOLANI"
        "Uninstall GOLANI"
        "Exit"
    )

    while true; do
        clear

        print_banner

        printf "${WHITE}${BOLD}"
        echo "  GOLANI INSTALLER"
        printf "${RESET}"

        printf "${GRAY}"
        echo "  Your personal installation manager"
        printf "${RESET}\n"

        echo "  Select an action:"
        echo ""

        for i in "${!options[@]}"; do
            if [[ "$i" -eq "$selected" ]]; then
                printf "${WHITE}${BOLD}  ❯  %-30s${RESET}\n" "${options[$i]}"
            else
                printf "${GRAY}     %-30s${RESET}\n" "${options[$i]}"
            fi
        done

        echo ""
        printf "${GRAY}"
        echo "  ↑ ↓  Navigate     ENTER  Select     ESC  Exit"
        printf "${RESET}"

        key=""

        IFS= read -rsn1 key

        # 방향키 및 ESC
        if [[ "$key" == $'\x1b' ]]; then
            key=""
            IFS= read -rsn2 -t 0.1 key || true

            case "$key" in
                "[A")
                    if (( selected > 0 )); then
                        ((selected--))
                    fi
                    ;;
                "[B")
                    if (( selected < ${#options[@]} - 1 )); then
                        ((selected++))
                    fi
                    ;;
                "")
                    return 0
                    ;;
            esac

        # Enter
        elif [[ -z "$key" ]]; then
            return $((selected + 1))
        fi
    done
}

# ==========================================
# 실행
# ==========================================

while true; do
    show_menu
    choice=$?

    case "$choice" in

        1)
            clear
            print_banner

            echo "Starting GOLANI installation..."

            # 설치 코드 연결 위치

            break
            ;;

        2)
            clear
            print_banner

            echo "Starting GOLANI update..."

            # 업데이트 코드 연결 위치

            break
            ;;

        3)
            clear
            print_banner

            echo "Starting GOLANI uninstall..."

            # 삭제 코드 연결 위치

            break
            ;;

        4|0)
            echo "Goodbye!"
            exit 0
            ;;

    esac
done
