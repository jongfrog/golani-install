#!/bin/bash

# ==========================================
# GOLANI INSTALLER
# ==========================================

WHITE='\033[97m'
GRAY='\033[90m'
BOLD='\033[1m'
RESET='\033[0m'

# ------------------------------------------
# 로고 출력
# ------------------------------------------

print_banner() {
    printf "${WHITE}${BOLD}"
    cat <<'EOF'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
EOF
    printf "${RESET}\n"
}

# ------------------------------------------
# 메뉴
# ------------------------------------------

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
            if [ "$i" -eq "$selected" ]; then
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

        # ESC 또는 방향키
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

# ------------------------------------------
# 프로그램 실행
# ------------------------------------------

while true; do
    show_menu
    choice=$?

    case "$choice" in
        1)
            clear
            print_banner

            echo "Starting GOLANI installation..."

            # TODO: 기존 설치 코드 연결

            break
            ;;

        2)
            clear
            print_banner

            echo "Starting GOLANI update..."

            # TODO: 기존 업데이트 코드 연결

            break
            ;;

        3)
            clear
            print_banner

            echo "Starting GOLANI uninstall..."

            # TODO: 기존 삭제 코드 연결

            break
            ;;

        4|0)
            clear
            echo "Goodbye!"
            exit 0
            ;;
    esac
done
