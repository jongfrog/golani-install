#!/usr/bin/env bash
# =====================================================================
# GOLANI INSTALLER  (Linux / macOS, bash)
# ---------------------------------------------------------------------
#  실행 방법:
#    bash install.sh                 대화형 메뉴 (↑/↓ + Enter)
#    bash install.sh install         바로 실행 (install|update|uninstall|help)
#    curl -fsSL <URL>/install.sh | bash
#      -> 파이프 실행이어도 키 입력은 /dev/tty 에서 읽어 메뉴 조작 가능
#
#  종료 안전:
#    Ctrl+C / Ctrl+D / ESC / TERM 시그널로 종료해도
#    커서·색상 등 터미널 상태를 항상 복구합니다 (EXIT 트랩).
#
#  저장 형식: UTF-8 (LF). 메뉴 조작은 bash 3.2 이상에서 동작합니다.
# =====================================================================

# sh 로 실행된 경우 bash 로 재실행
if [ -z "${BASH_VERSION:-}" ]; then
    exec bash "$0" "$@"
fi

set -u

# ------------------------------------------------------------ 색상
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_RESET=$'\e[0m';  C_WHITE=$'\e[1;37m'; C_GRAY=$'\e[90m'
    C_GREEN=$'\e[1;92m'; C_YELLOW=$'\e[93m'; C_RED=$'\e[91m'
else
    C_RESET=""; C_WHITE=""; C_GRAY=""; C_GREEN=""; C_YELLOW=""; C_RED=""
fi

# ------------------------------------------------------------ 설정 (필요하면 수정)
GOLANI_HOME="${GOLANI_HOME:-$HOME/.golani}"     # 설치 루트 디렉터리
GOLANI_BIN="$GOLANI_HOME/bin"                   # 실행파일 위치

# ------------------------------------------------------------ 터미널 감지
STDIN_TTY=0; [ -t 0 ] && STDIN_TTY=1
OUT_TTY=0;  [ -t 1 ] && OUT_TTY=1
TTY_FD=0
NO_TTY=0
# curl | bash 처럼 stdin 이 파이프면 키 입력을 /dev/tty 에서 읽는다
if [ "$STDIN_TTY" -eq 0 ]; then
    if (exec 3</dev/tty) 2>/dev/null; then
        exec 3</dev/tty
        TTY_FD=3
    else
        NO_TTY=1
    fi
fi

# ------------------------------------------------------------ 로고
print_logo() {
    cat <<'LOGO'
░██████╗░░█████╗░██╗░░░░░░█████╗░███╗░░██╗██╗
██╔════╝░██╔══██╗██║░░░░░██╔══██╗████╗░██║██║
██║░░██╗░██║░░██║██║░░░░░███████║██╔██╗██║██║
██║░░╚██╗██║░░██║██║░░░░░██╔══██║██║╚████║██║
╚██████╔╝╚█████╔╝███████╗██║░░██║██║░╚███║██║
░╚═════╝░░╚════╝░╚══════╝╚═╝░░╚═╝╚═╝░░╚══╝╚═╝
LOGO
}

# ------------------------------------------------------------ 출력 헬퍼
cls()      { command -v clear >/dev/null 2>&1 && clear || printf '\e[2J\e[H'; }
print_line() { printf '%s%s%s\n' "$2" "$1" "$C_RESET"; }
step()     { printf '  %s[*]%s %s\n' "$C_GREEN" "$C_RESET" "$1"; }
info()     { printf '  %s[i]%s %s\n' "$C_GRAY"  "$C_RESET" "$1"; }
warn()     { printf '  %s[!]%s %s\n' "$C_YELLOW" "$C_RESET" "$1"; }
err()      { printf '  %s[x]%s %s\n' "$C_RED"   "$C_RESET" "$1"; }

show_header() {          # $1 = 화면 제목
    cls
    print_logo
    printf '\n'
    print_line "  $1" "$C_WHITE"
    printf '\n'
}

pause_key() {            # 아무 키나 누르면 돌아가기 (비대화형이면 건너뜀)
    if [ "$OUT_TTY" -eq 1 ] && [ "$NO_TTY" -eq 0 ]; then
        printf '  %s아무 키나 누르면 메뉴로 돌아갑니다...%s' "$C_GRAY" "$C_RESET"
        IFS= read -rsn1 -u "$TTY_FD" _ || true
        printf '\n'
    fi
}

# ------------------------------------------------------------ 종료 안전장치
cleanup() {
    printf '\e[?25h\e[0m'          # 커서 표시 + 색상 초기화
    [ "$TTY_FD" -eq 3 ] && exec 3<&-
    return 0
}
trap cleanup EXIT

on_interrupt() {
    printf '\n\n'
    print_line '  중단되었습니다 (Ctrl+C) — Goodbye!' "$C_YELLOW"
    exit 130
}
trap on_interrupt INT

on_term() {
    printf '\n\n'
    print_line '  종료 신호 수신 — Goodbye!' "$C_YELLOW"
    exit 143
}
trap on_term TERM

# ------------------------------------------------------------ 메뉴
ITEMS=("Install GOLANI" "Update GOLANI" "Uninstall GOLANI" "Exit")
SEL=0

show_menu() {
    printf '\e]0;GOLANI INSTALLER\a'   # 창 제목
    cls
    print_logo
    printf '\n'
    print_line "  GOLANI INSTALLER" "$C_WHITE"
    print_line "  Your personal installation manager" "$C_GRAY"
    printf '\n'
    print_line "  UP/DOWN 이동  ·  ENTER 선택  ·  ESC / Ctrl+C 종료" "$C_GRAY"
    printf '\n'
    local i
    for i in "${!ITEMS[@]}"; do
        if [ "$i" -eq "$SEL" ]; then
            printf '  %s>%s  %s%s%s\n' "$C_GREEN" "$C_RESET" "$C_WHITE" "${ITEMS[$i]}" "$C_RESET"
        else
            printf '     %s%s%s\n' "$C_GRAY" "${ITEMS[$i]}" "$C_RESET"
        fi
    done
    printf '\n'
}

read_key() {             # 결과는 전역 KEY (UP/DOWN/ENTER/ESC/EOF/기타 문자)
    KEY=""
    local c rest
    if ! IFS= read -rsn1 -u "$TTY_FD" c; then
        KEY="EOF"
        return 0
    fi
    case "$c" in
        $'\x1b')
            if IFS= read -rsn2 -u "$TTY_FD" -t 1 rest; then
                case "$rest" in
                    '[A') KEY="UP" ;;
                    '[B') KEY="DOWN" ;;
                    *)    KEY="ESC" ;;
                esac
            else
                KEY="ESC"                      # ESC 단독 입력 (1초 무입력)
            fi
            ;;
        ''|"$'\r'"|"$'\n'") KEY="ENTER" ;;
        *) KEY="$c" ;;
    esac
    return 0
}

do_exit() {
    cls
    printf '\n'
    print_line '  Goodbye!' "$C_GRAY"
    printf '\n'
    exit 0
}

# ------------------------------------------------------------ 설치 액션
# 실제 설치 로직은 install_payload() / update_payload() 에 작성하세요.
install_payload() {
    # 예)
    #   local url="https://example.com/golani.tar.gz"
    #   curl -fsSL "$url" | tar -xz -C "$GOLANI_HOME"
    #   ln -sf "$GOLANI_HOME/golani" "$GOLANI_BIN/golani"
    warn '설치 로직이 아직 구성되지 않았습니다. (install_payload 함수를 편집하세요)'
    return 1
}

update_payload() {
    # 예)
    #   do_uninstall_silent && do_install_silent 등
    warn '업데이트 로직이 아직 구성되지 않았습니다. (update_payload 함수를 편집하세요)'
    return 1
}

do_install() {
    show_header 'INSTALL GOLANI'
    step "설치 경로 확인: $GOLANI_BIN"
    if mkdir -p "$GOLANI_BIN" 2>/dev/null; then
        info '디렉터리 준비 완료'
    else
        err "디렉터리 생성 실패: $GOLANI_BIN"
        pause_key
        return 1
    fi
    install_payload || true
    pause_key
    return 0
}

do_update() {
    show_header 'UPDATE GOLANI'
    step "설치 경로 확인: $GOLANI_HOME"
    if [ ! -d "$GOLANI_HOME" ]; then
        info '설치된 GOLANI 가 없습니다. 먼저 Install 을 실행하세요.'
        pause_key
        return 0
    fi
    update_payload || true
    pause_key
    return 0
}

do_uninstall() {
    show_header 'UNINSTALL GOLANI'
    if [ ! -d "$GOLANI_HOME" ]; then
        info "제거할 항목이 없습니다. ($GOLANI_HOME 없음)"
        pause_key
        return 0
    fi
    if [ "$NO_TTY" -eq 1 ]; then
        warn '대화형 터미널이 아니어서 확인을 받을 수 없습니다. (취소)'
        return 1
    fi
    local ans
    printf '  %s을(를) 완전히 제거할까요? [y/N]: %s' "$GOLANI_HOME" "$C_RESET"
    IFS= read -r -u "$TTY_FD" ans || ans=""
    case "$ans" in
        y|Y|yes|YES)
            case "$GOLANI_HOME" in
                "/"|"$HOME"|"$HOME"/*/.golani)
                    rm -rf "$GOLANI_HOME" && step "제거 완료: $GOLANI_HOME" \
                        || err '제거 실패 (권한 확인 필요)'
                    ;;
                *)
                    err "안전상 제거할 수 없는 경로입니다: $GOLANI_HOME"
                    ;;
            esac
            ;;
        *)
            info '취소했습니다.'
            ;;
    esac
    pause_key
    return 0
}

usage() {
    print_logo
    printf '\n'
    print_line '  Usage: install.sh {install|update|uninstall|help}' "$C_WHITE"
    printf '\n'
    print_line '  인자 없이 실행하면 대화형 메뉴가 열립니다.' "$C_GRAY"
    printf '\n'
}

# ------------------------------------------------------------ 메인
main() {
    if [ "$#" -gt 0 ]; then                 # CLI 직접 실행 모드
        case "$1" in
            install)   do_install ;;
            update)    do_update ;;
            uninstall) do_uninstall ;;
            help|-h|--help) usage ;;
            *)
                usage
                err "알 수 없는 명령: $1"
                exit 1
                ;;
        esac
        return 0
    fi

    if [ "$NO_TTY" -eq 1 ] || [ "$OUT_TTY" -eq 0 ]; then
        print_logo
        printf '\n'
        warn '대화형 터미널이 아니어서 메뉴를 표시할 수 없습니다.'
        info  '사용법: install.sh {install|update|uninstall|help}'
        printf '\n'
        exit 1
    fi

    printf '\e[?25l'                        # 커서 숨김 (cleanup 에서 복구)
    while :; do
        show_menu
        read_key
        case "$KEY" in
            UP)   SEL=$(( (SEL + ${#ITEMS[@]} - 1) % ${#ITEMS[@]} )) ;;
            DOWN) SEL=$(( (SEL + 1) % ${#ITEMS[@]} )) ;;
            ENTER)
                case "$SEL" in
                    0) do_install ;;
                    1) do_update ;;
                    2) do_uninstall ;;
                    3) do_exit ;;
                esac
                ;;
            ESC|EOF) do_exit ;;
        esac
    done
}

main "$@"
