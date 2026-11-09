#!/usr/bin/env bash
# macOS 가상머신(Tart)에서 tasty 전체 시험을 돌린다 — 호스트의 도구·설정과 섞이지 않는
# 깨끗한 macOS 에서 `cargo test --workspace` · 셸 게이트 · GUI 시험을 잰다.
#
# 절차: docs/dev-guide/macos-vm-tests.md
#
# 쓰는 법:
#   bash scripts/macos-vm-test.sh prepare        # 이미지 → 가상머신 · toolchain 설치 (한 번)
#   bash scripts/macos-vm-test.sh test [범위]     # 빌드 캐시를 (필요하면) 지우고 작업 트리를 옮겨 시험
#                                                 (범위: all·unit·gates·gui·windows·pkg)
#   bash scripts/macos-vm-test.sh shell          # 가상머신에 들어간다 (ssh, exec 길이면 tart exec)
#   bash scripts/macos-vm-test.sh stop           # 가상머신을 끈다 (디스크·빌드 캐시는 남는다)
#   bash scripts/macos-vm-test.sh destroy        # 가상머신을 지운다
#
# 밖에서 덮는 값:
#   TASTY_VM_NAME   가상머신 이름        (기본 tasty-macos)
#   TASTY_VM_IMAGE  받을 이미지          (기본 ghcr.io/cirruslabs/macos-tahoe-base:latest)
#   TASTY_VM_OUT    호스트에 남길 로그 자리 (기본 target/macos-vm)
#   TASTY_VM_SSH_TRIES  부팅 뒤 ssh 를 2 초 간격으로 두드리는 횟수 (기본 60)
#   TASTY_VM_NET    가상머신 네트워크 — nat (기본) · bridged:<인터페이스> (예: bridged:en5).
#                   nat 은 호스트에 DNS 프록시를 띄워 Cloudflare WARP 를 끊는다 — WARP 를 쓰면 bridged.
#   TASTY_VM_TRANSPORT  명령·파일을 나르는 길 — ssh · exec (tart 게스트 에이전트). 기본은 nat 이면 ssh,
#                   bridged 면 exec (브리지된 가상머신은 호스트와 다른 대역을 받아 ssh 가 안 닿을 수 있다).
#   TASTY_VM_CLEAN  auto (기본) 이면 아래 문턱을 넘을 때만 빌드 캐시를 지운다 · 1 이면 늘 지운다 · 0 이면 늘 남긴다
#   TASTY_VM_CLEAN_MAX_TARGET_GB  auto 에서 가상머신의 target 이 이보다 크면 지운다 (기본 24)
#   TASTY_VM_CLEAN_DIRTY_PCT      auto 에서 지난 빌드 뒤 바뀐 크레이트가 전체의 이 비율(%)을 넘으면 지운다 (기본 50)
#   TASTY_VM_DISK_GB  prepare 가 새로 복제한 가상머신의 디스크 크기 (기본 100) — 이미지는 50 GB 다
#   TASTY_VM_MIN_FREE_GB  처음부터 빌드할 때 남아야 하는 디스크 여유 (기본 12) — 모자라면 옮기기 전에 멈춘다.
#                   캐시를 남기면 그 크기만큼 덜 요구한다(아래 하한까지)
#   TASTY_VM_KEEP_MIN_FREE_GB  캐시를 남길 때 요구하는 디스크 여유의 하한 (기본 3)
#   TASTY_VM_PKG    pkg 범위에서 시험할 패키지 (공백으로 나눈다)
#   TASTY_VM_BASE   pkg 범위에서 TASTY_VM_PKG 가 없을 때 — 이 커밋과의 분기점 뒤로 바뀐 파일의 패키지를 고른다
#
# 종료 코드: 0 전부 통과 · 1 시험 실패 · 2 준비가 안 됐다(도구·가상머신·권한).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VM="${TASTY_VM_NAME:-tasty-macos}"
IMAGE="${TASTY_VM_IMAGE:-ghcr.io/cirruslabs/macos-tahoe-base:latest}"
OUT="${TASTY_VM_OUT:-$ROOT/target/macos-vm}"
# 이미지의 기본 계정이다(cirruslabs 이미지 공통). 가상머신 밖으로 안 나가는 시험 전용 값이다.
VM_USER="admin"
VM_PASS="admin"
# 비밀번호는 `sshpass -e` 로 환경 변수에서 넘긴다. `-p` 는 sshpass 가 비밀번호를 숨기려 자기
# argv 를 고쳐 쓰는데, 그때 인자 영역이 당겨져 **첫 환경 변수가 `ps` 의 인자 목록에 드러난다**
# (실측 2026-10-21: Claude Code 의 세션 메시징 토큰이 보였다). 인자 목록은 모든 로컬 사용자가 읽는다.
export SSHPASS="$VM_PASS"
# 가상머신 안의 작업 트리. 빌드 캐시(`target/`)가 여기 남아 두 번째부터 빠르다.
GUEST_TREE=tasty
# 부팅 뒤 ssh 를 2 초 간격으로 몇 번 두드리나. 시험이 줄여 쓴다.
SSH_TRIES="${TASTY_VM_SSH_TRIES:-60}"
# 이미지 디스크는 50 GB 이고 빌드 캐시가 그 사이에 쌓인다 — 실측 2026-10-27: `~/tasty/target` 11 GB, 여유 2.2 GB.
# 그래서 prepare 가 새 가상머신의 디스크를 늘린다(실측 2026-11-02: 100 GB 로 늘린 뒤 빈 캐시 기준 여유 59 GB).
DISK_GB="${TASTY_VM_DISK_GB:-100}"
case "$DISK_GB" in ''|*[!0-9]*) echo "[macos-vm] TASTY_VM_DISK_GB 는 자연수다: $DISK_GB" >&2; exit 2 ;; esac
# 빌드 하나(CARGO_INCREMENTAL=0)가 10 GB 남짓을 쓰므로 그만큼 남지 않으면 빌드 도중 rustc 가 죽는다.
CLEAN="${TASTY_VM_CLEAN:-auto}"
MIN_FREE_GB="${TASTY_VM_MIN_FREE_GB:-12}"
# auto 문턱. 캐시는 cargo 가 안 거두는 옛 산출물로 커지고(실측 2026-10-27: 11 GB), 지울 때 드는 값은
# 외부 의존까지 새로 짓는 첫 빌드 시간이다(§2 실측 529 초) — 그래서 늘 지우지 않고 넘칠 때만 지운다.
CLEAN_MAX_TARGET_GB="${TASTY_VM_CLEAN_MAX_TARGET_GB:-24}"
CLEAN_DIRTY_PCT="${TASTY_VM_CLEAN_DIRTY_PCT:-50}"
KEEP_MIN_FREE_GB="${TASTY_VM_KEEP_MIN_FREE_GB:-3}"
case "$CLEAN" in 0|1|auto) ;; *) echo "[macos-vm] TASTY_VM_CLEAN 은 auto · 0 · 1 중 하나다: $CLEAN" >&2; exit 2 ;; esac
for v in MIN_FREE_GB KEEP_MIN_FREE_GB CLEAN_MAX_TARGET_GB CLEAN_DIRTY_PCT; do
    case "${!v}" in ''|*[!0-9]*) echo "[macos-vm] TASTY_VM_$v 는 자연수다: ${!v}" >&2; exit 2 ;; esac
done
# 네트워크. nat 은 Virtualization 의 공유 네트워크로, 호스트가 mDNSResponder 를 192.168.64.1:53 에
# DNS 프록시로 띄운다 — Cloudflare WARP 는 그것을 남의 DNS 가로채기로 보고 연결을 거부한다
# (CF_DNS_PROXY_FAILURE, 실측 2026-10-26). bridged 는 그 프록시를 안 띄운다.
NET="${TASTY_VM_NET:-nat}"
NET_ARGS=()
case "$NET" in
    nat) ;;
    bridged:?*) NET_ARGS=(--net-bridged="${NET#bridged:}") ;;
    *) echo "[macos-vm] TASTY_VM_NET 은 nat 또는 bridged:<인터페이스> 다: $NET" >&2; exit 2 ;;
esac
# 브리지된 가상머신은 그 랜의 DHCP 에서 주소를 받는다. 호스트가 그 대역에 없으면 ssh 가 안 닿는다
# (실측 2026-10-26: 호스트 192.0.2.10 고정, 가상머신 192.0.2.11 — ping 손실 100%). 그래서
# bridged 의 기본 길은 게스트 에이전트(`tart exec`)다 — 네트워크를 안 거친다.
TRANSPORT="${TASTY_VM_TRANSPORT:-}"
if [ -z "$TRANSPORT" ]; then
    if [ "$NET" = nat ]; then TRANSPORT="ssh"; else TRANSPORT="exec"; fi
fi
case "$TRANSPORT" in ssh|exec) ;; *) echo "[macos-vm] TASTY_VM_TRANSPORT 는 ssh 또는 exec 다: $TRANSPORT" >&2; exit 2 ;; esac

die() { echo "[macos-vm] $*" >&2; exit 2; }
say() { echo "[macos-vm] $*"; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "$1 이 없다 — 받는 법: $2"
}
need tart "docs/dev-guide/macos-vm-tests.md 의 설치 절을 따른다"
[ "$TRANSPORT" = exec ] || need sshpass "brew install sshpass"
need rsync "macOS 기본 도구다"

# 비밀번호로만 들어간다 — ssh 가 에이전트의 키를 먼저 여러 개 내밀면 가상머신의 sshd 가
# "Too many authentication failures" 로 끊는다(실측 2026-10-15).
SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR
          -o ConnectTimeout=5 -o PubkeyAuthentication=no -o PreferredAuthentications=password)

# 목록을 변수로 받아 넘긴다 — 파이프 오른쪽의 조기 종료 소비자(`grep -q`)는 producer 를
# SIGPIPE 로 죽여 `pipefail` 아래에서 찾았는데도 실패가 된다.
vm_exists() {
    local all
    all="$(tart list --quiet 2>/dev/null)" || return 1
    grep -qx "$VM" <<<"$all"
}
vm_running() {
    local all
    all="$(tart list 2>/dev/null)" || return 1
    awk -v n="$VM" '$2 == n && $NF == "running" {f=1} END {exit !f}' <<<"$all"
}

vm_ip() {
    tart ip --wait 120 "$VM" 2>/dev/null || die "$VM 의 IP 를 120 초 안에 못 받았다"
}

# 가상머신의 sshd 가 맞는 비밀번호를 가끔 거절한다 — 실측 2026-10-21: 연달아 20 번 붙으면 2–4 번
# `Permission denied`, sshpass 의 `-e`·`-p`·`-f` 어느 쪽이든 같다. 그때 sshpass 는 5 로 끝나고 명령은 돌지
# 않았으므로 다시 해도 두 번 돌지 않는다. 5 가 아닌 실패(명령 자신의 종료 코드)는 그대로 돌려준다.
AUTH_TRIES=3
auth_retry() {
    local n=1 rc
    while :; do
        "$@" && return 0
        rc=$?
        [ "$rc" -eq 5 ] && [ "$n" -lt "$AUTH_TRIES" ] || return "$rc"
        n=$((n + 1))
        sleep 2
    done
}

# rsync 는 같은 것을 다시 옮겨도 결과가 같다 — 인증 거부가 rsync 의 다른 종료 코드로 감싸여 오므로
# 실패면 다시 한다.
sync_retry() {
    local n=1
    until "$@"; do
        [ "$n" -lt "$AUTH_TRIES" ] || return 1
        n=$((n + 1))
        sleep 2
    done
}

# 가상머신 안에서 **로그인 셸**로 돈다 — 이미지가 Homebrew · cargo 의 PATH 를 `~/.zprofile`
# 에 두므로, 비로그인 셸로 부르면 brew 도 cargo 도 안 보인다.
guest() {
    if [ "$TRANSPORT" = exec ]; then
        tart exec "$VM" zsh -lc "$1"
        return
    fi
    local ip
    ip="$(vm_ip)"
    auth_retry sshpass -e ssh "${SSH_OPTS[@]}" "$VM_USER@$ip" "zsh -lc $(printf '%q' "$1")"
}

# 가상머신에 닿는가 — 부팅 대기가 쓴다.
reachable() {
    if [ "$TRANSPORT" = exec ]; then
        tart exec "$VM" true 2>/dev/null
        return
    fi
    sshpass -e ssh "${SSH_OPTS[@]}" "$VM_USER@$1" true 2>/dev/null
}

# rsync 의 원격 셸과 목적지 앞부분 — 길마다 다르다.
rsync_rsh() {
    if [ "$TRANSPORT" = exec ]; then echo "tart exec -i"; else echo "sshpass -e ssh ${SSH_OPTS[*]}"; fi
}
rsync_dest() {
    if [ "$TRANSPORT" = exec ]; then echo "$VM:$1"; else echo "$VM_USER@$(vm_ip):$1"; fi
}

boot() {
    vm_exists || die "$VM 가 없다 — 먼저 prepare"
    if ! vm_running; then
        mkdir -p "$OUT"
        say "$VM 를 띄운다 (창 없이 — 가상 디스플레이는 그대로 있어 GUI 시험이 돈다)"
        nohup tart run --no-graphics ${NET_ARGS[@]+"${NET_ARGS[@]}"} "$VM" >"$OUT/tart-run.log" 2>&1 &
    fi
    local ip=''
    [ "$TRANSPORT" = exec ] || ip="$(vm_ip)"
    for _ in $(seq 1 "$SSH_TRIES"); do
        if reachable "$ip"; then
            return 0
        fi
        sleep 2
    done
    die "$VM ${ip:+($ip) }에 $TRANSPORT 가 $((SSH_TRIES * 2)) 초 안에 안 열렸다"
}

cmd_prepare() {
    if ! vm_exists; then
        say "이미지를 받아 $VM 로 복제한다: $IMAGE (처음이면 수십 GB 를 받는다)"
        tart clone "$IMAGE" "$VM"
        # 디스크는 끈 채로만 늘릴 수 있다 — 첫 부팅 전에 한다. tart 디스크는 sparse 라 쓴 만큼만 차지한다.
        say "디스크를 ${DISK_GB} GB 로 늘린다"
        tart set "$VM" --disk-size "$DISK_GB"
    fi
    boot
    # 늘린 디스크를 APFS 컨테이너가 차지하게 한다. 부팅이 이미 넓혔으면 크기가 같다는 오류(-69743)만 낸다.
    guest "echo $VM_PASS | sudo -S diskutil apfs resizeContainer disk0s2 0 >/dev/null 2>&1 || true"
    say "toolchain 을 깐다 (rustup · shellcheck · tokei · bash 5 · mingw-w64)"
    # 판은 레포의 rust-toolchain.toml 이 정한다 — 첫 cargo 호출이 그 판을 받는다.
    guest 'command -v rustup >/dev/null || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none'
    guest 'grep -q cargo/env ~/.zprofile || echo ". \"$HOME/.cargo/env\"" >> ~/.zprofile'
    # mingw-w64 는 windows 범위가 쓴다 — Windows 대상 cargo check 도 C 를 짓는 build.rs 가 그 컴파일러를 부른다.
    guest 'brew install shellcheck tokei bash mingw-w64'
    grant_screen_capture
    check_input_grant
    say "준비 끝."
}

# GUI 시험은 enigo 로 실제 키·마우스 이벤트를 보낸다 — ssh 로 띄운 프로세스가 손쉬운 사용과
# 이벤트 보내기 권한을 가져야 한다. cirruslabs 이미지가 그 권한을 미리 넣어 두지만, 없으면
# 이벤트가 **조용히** 버려져 시험이 Timeout 으로만 죽으므로 돌리기 전에 **실제 프로세스에**
# 묻는다(TCC 표의 행을 세면 이름이 바뀐 클라이언트를 못 본다).
#
# 화면 기록은 다르다. macOS 26 은 ssh 세션을 `/usr/libexec/sshd-session` 으로 띄우고, 이미지는
# 옛 이름(`sshd-keygen-wrapper`)에만 그 권한을 넣어 두었다. 화면을 찍으면 권한 요청 창이
# **맨 앞에** 떠서 시험 창을 가린다(실측 2026-10-15). 그래서 그 항목은 SIP 가 꺼진 이미지의
# TCC 에 직접 넣는다.
TCC_DB="/Library/Application Support/com.apple.TCC/TCC.db"
SSH_SESSION=/usr/libexec/sshd-session

# SQL 을 표준입력으로 넘긴다 — 셸 인용을 한 겹도 안 거친다.
tcc_sql_once() {
    sshpass -e ssh "${SSH_OPTS[@]}" "$VM_USER@$2" "sudo sqlite3 '$TCC_DB'" <<<"$1"
}
tcc_sql() {
    if [ "$TRANSPORT" = exec ]; then
        tart exec -i "$VM" zsh -lc "sudo sqlite3 '$TCC_DB'" <<<"$1"
        return
    fi
    local ip
    ip="$(vm_ip)"
    # 시도마다 표준입력을 새로 넣는다 — 한 번 읽힌 here-string 은 다시 안 온다.
    auth_retry tcc_sql_once "$1" "$ip"
}

grant_screen_capture() {
    local have
    # exec 길의 프로세스는 게스트 에이전트가 띄운다 — 이미지가 그것에 세 권한(손쉬운 사용 · 이벤트
    # 보내기 · 화면 기록)을 미리 넣어 두었다(실측 2026-10-26). 경로에 판이 들어가 있어 넣지 않고 묻기만 한다.
    if [ "$TRANSPORT" = exec ]; then
        have="$(tcc_sql "select count(*) from access where client like '%/tart-guest-agent'
                         and service = 'kTCCServiceScreenCapture' and auth_value = 2;")"
        [ "$have" -ge 1 ] || die "게스트 에이전트에 화면 기록 권한이 없다 — docs/dev-guide/macos-vm-tests.md"
        grant_apple_events
        return 0
    fi
    have="$(tcc_sql "select count(*) from access where client = '$SSH_SESSION'
                     and service = 'kTCCServiceScreenCapture' and auth_value = 2;")"
    [ "$have" = 1 ] && return 0
    say "ssh 세션($SSH_SESSION)에 화면 기록 권한을 넣고 다시 띄운다"
    tcc_sql "INSERT OR REPLACE INTO access (service, client_type, client, auth_value, auth_reason,
             auth_version, indirect_object_identifier_type, indirect_object_identifier)
             VALUES ('kTCCServiceScreenCapture', 1, '$SSH_SESSION', 2, 0, 1, NULL, 'UNUSED');"
    # 권한 데몬이 새 항목을 읽게 다시 띄운다.
    tart stop "$VM"
    boot
}

# 클릭 시험은 osascript 로 System Events 에 창 위치를 묻는다(tests/gui_common/mod.rs 의 client_to_screen).
# 그 자동화 권한은 **사용자** TCC 가 띄운 주체(게스트 에이전트)로 판정한다. 이미지는 그 행을 안 넣어 두었고,
# 처음 물을 때 뜬 권한 창은 아무도 못 눌러 거부(auth_value 0)로 남는다 — 그 뒤로 osascript 가 -1743 으로 죽는다
# (실측 2026-10-27). 허용으로 쓰고 tccd 를 다시 띄우면 바로 듣는다. 다시 묻는 것으로 확인한다.
grant_apple_events() {
    local agent got
    agent="$(tcc_sql "select client from access where client like '%/tart-guest-agent'
                      and service = 'kTCCServiceScreenCapture' limit 1;")"
    # 경로를 못 찾으면 쓰지 않는다 — 바로 아래 확인이 그 경우도 멈춘다.
    if [ -n "$agent" ]; then
        # 경로의 $HOME 은 가상머신에서 풀린다.
        # shellcheck disable=SC2016
        tart exec -i "$VM" zsh -lc 'sudo sqlite3 "$HOME/Library/Application Support/com.apple.TCC/TCC.db"' <<<"INSERT OR REPLACE INTO access (service, client, client_type,
            auth_value, auth_reason, auth_version, indirect_object_identifier_type, indirect_object_identifier, flags)
            VALUES ('kTCCServiceAppleEvents', '$agent', 1, 2, 3, 1, 0, 'com.apple.systemevents', 0);"
    fi
    guest 'killall -u "$USER" tccd 2>/dev/null; true'
    got="$(guest 'osascript -e "tell application \"System Events\" to count processes" 2>&1')" || true
    case "$got" in
        ''|*[!0-9]*) die "게스트 에이전트가 System Events 에 Apple 이벤트를 못 보낸다(got: $got) — docs/dev-guide/macos-vm-tests.md" ;;
    esac
}

check_input_grant() {
    local got
    got="$(guest 'printf "%s\n" "import ApplicationServices" \
        "print(AXIsProcessTrusted() && CGPreflightPostEventAccess())" > ~/.tasty-ax.swift &&
        swiftc -o ~/.tasty-ax ~/.tasty-ax.swift && ~/.tasty-ax; rm -f ~/.tasty-ax ~/.tasty-ax.swift')"
    [ "$got" = true ] || die "$TRANSPORT 로 띄운 프로세스에 손쉬운 사용·이벤트 보내기 권한이 없다(got: $got) — docs/dev-guide/macos-vm-tests.md"
}

sync_tree() {
    local ignored
    # `--delete` 로 미는 자리다 — 원본이 이 레포가 아니면 남의 트리를 통째로 옮긴다. 실측
    # 2026-10-15: 이 파일을 다른 셸에서 source 하자 `$0` 이 셸 이름이 되어 ROOT 가 레포의
    # **부모**로 잡혔고, 옆 프로젝트 31 개가 가상머신으로 넘어가 디스크를 채웠다.
    # 레포의 꼭대기인지는 git 에게 묻는다 — worktree 는 `.git` 이 디렉토리가 아니라 파일이다.
    local top
    top="$(git -C "$ROOT" rev-parse --show-toplevel 2>/dev/null)" || top=''
    [ -f "$ROOT/Cargo.toml" ] && [ -n "$top" ] && [ "$(cd "$top" && pwd -P)" = "$(cd "$ROOT" && pwd -P)" ] \
        && [ -f "$ROOT/scripts/macos-vm-test.sh" ] \
        || die "옮길 원본이 tasty 레포가 아니다: $ROOT"
    say "작업 트리를 옮긴다 (커밋 안 한 변경 포함 · git 이 무시하는 것 제외)"
    # 무시 목록은 `.gitignore` 파일이 아니라 **git 에게** 묻는다 — `.git/info/exclude` 와 전역
    # 설정까지 들어가야 호스트 git 이 보는 트리와 같아진다.
    ignored="$(cd "$ROOT" && git ls-files --others --ignored --exclude-standard --directory)"
    # 제외 목록은 파일로 넘긴다 — 프로세스 치환(`<(…)`)은 한 번 읽으면 끝이라, 인증 거부로 다시 하는
    # 두 번째 시도가 빈 목록을 읽고 무시된 파일까지 옮긴다.
    local excl
    excl="$(mktemp)"
    sed 's|^|/|' <<<"$ignored" >"$excl"
    local rc=0
    sync_retry rsync -a --delete \
        --exclude '/target/' \
        --exclude '/.git' \
        --exclude-from="$excl" \
        -e "$(rsync_rsh)" \
        "$ROOT/" "$(rsync_dest "$GUEST_TREE/")" || rc=$?
    rm -f "$excl"
    [ "$rc" -eq 0 ] || return "$rc"
    sync_git
}

# git 저장소도 함께 간다 — 셸 게이트가 `git ls-files` 로 좌변을 정한다. worktree 의 `.git` 은 호스트 경로를
# 가리키는 파일이라 그대로 옮기면 가상머신에서 git 이 깨진다. 그래서 공용 저장소(objects · refs)를 옮기고,
# 그 worktree 몫인 HEAD 와 index 를 그 위에 덮는다 — 가상머신에서는 그 브랜치를 체크아웃한 보통 저장소가 된다.
# 본 체크아웃이면 두 자리가 같아 덮기가 아무것도 안 바꾼다.
sync_git() {
    local gitdir common
    gitdir="$(git -C "$ROOT" rev-parse --absolute-git-dir)" || return 1
    common="$(cd "$ROOT" && cd "$(git rev-parse --git-common-dir)" && pwd)" || return 1
    sync_retry rsync -a --delete --exclude '/worktrees/' \
        -e "$(rsync_rsh)" \
        "$common/" "$(rsync_dest "$GUEST_TREE/.git/")" || return 1
    [ "$gitdir" = "$common" ] && return 0
    say "worktree 를 옮긴다 ($(git -C "$ROOT" rev-parse --abbrev-ref HEAD))"
    sync_retry rsync -a \
        -e "$(rsync_rsh)" \
        "$gitdir/HEAD" "$gitdir/index" "$(rsync_dest "$GUEST_TREE/.git/")"
}

# 지난 빌드의 커밋 표지. target 안에 둔다 — 누가 cargo clean 을 하든 표지도 같이 사라져, 출처 모르는
# 캐시를 "같은 트리"로 잘못 읽지 않는다.
BUILT_REV="$GUEST_TREE/target/.tasty-vm-built-rev"

# 가상머신의 디스크 여유(KB). 숫자가 아니면 빈 값.
guest_free_kb() {
    local kb
    kb="$(guest 'df -k ~ | awk "NR==2 {print \$4}"  # tasty-free-kb')"
    case "$kb" in ''|*[!0-9]*) echo '' ;; *) echo "$kb" ;; esac
}

# 지난 빌드 뒤 바뀐 크레이트를 센다 — "바뀐 수 전체 수", 작업 공간 설정이 바뀌었으면 "all".
# 루트 패키지(src·tests·benches·build.rs·lang)는 하나로 센다. 문서 같은 비 Rust 자리는 안 센다.
# 지난 실행에서 커밋 안 하고 옮긴 변경은 표지에 안 남는다 — 그 몫은 cargo 가 어차피 다시 짓는다.
dirty_crates() {
    local rev="$1" files total
    files="$(cd "$ROOT" && { git diff --name-only "$rev" -- && git ls-files --others --exclude-standard; })" || return 1
    total=$(( $(find "$ROOT/crates" -mindepth 2 -maxdepth 2 -name Cargo.toml 2>/dev/null | wc -l) + 1 ))
    awk -v total="$total" '
        /^(Cargo\.toml|Cargo\.lock|rust-toolchain(\.toml)?|\.cargo\/.*)$/ { all = 1 }
        /^crates\/[^\/]+\// { split($0, a, "/"); seen[a[2]] = 1 }
        /^(src|tests|benches|lang)\// || /^build\.rs$/ { seen["(root)"] = 1 }
        END { if (all) { print "all"; exit } n = 0; for (k in seen) n++; print n, total }
    ' <<<"$files"
}

# pkg 범위가 시험할 패키지 — base 와 HEAD 의 분기점 뒤로 바뀐 파일(커밋 안 한 것 포함)이 속한 패키지.
# crates/<dir>/ 은 그 Cargo.toml 의 이름, 루트 소스(src·tests·lang·build.rs)는 tasty, 문서는
# 문서 검사(tasty-doc-guards)로 보낸다. 루트 Cargo.toml·Cargo.lock 은 안 센다 — 앞의 --locked 빌드가 본다.
changed_packages() {
    local base="$1" mb files d n ex
    mb="$(git -C "$ROOT" merge-base "$base" HEAD 2>/dev/null)" || return 1
    ex="$(grep '^exclude *=' "$ROOT/Cargo.toml" 2>/dev/null || true)"
    files="$(cd "$ROOT" && { git diff --name-only "$mb" -- && git ls-files --others --exclude-standard; })" || return 1
    while IFS= read -r f; do
        case "$f" in
            crates/*/*)
                d="${f#crates/}"; d="${d%%/*}"
                case "$ex" in *"\"crates/$d\""*) continue ;; esac
                n="$(sed -n '/^name *= */{s/^name *= *"\(.*\)"/\1/p;q;}' "$ROOT/crates/$d/Cargo.toml" 2>/dev/null)"
                [ -z "$n" ] || echo "$n" ;;
            src/*|tests/*|lang/*|build.rs) echo tasty ;;
            docs/*|site/*|*.md) echo tasty-doc-guards ;;
        esac
    done <<<"$files" | sort -u | tr '\n' ' ' | sed 's/ $//'
}

# 가상머신의 빌드 캐시 크기(KB). 못 읽으면 0.
guest_target_kb() {
    local kb
    kb="$(guest "du -sk $GUEST_TREE/target 2>/dev/null | awk '{print \$1}'  # tasty-target-kb")"
    case "$kb" in ''|*[!0-9]*) echo 0 ;; *) echo "$kb" ;; esac
}

# 빌드에 필요한 디스크 여유(KB). 남긴 캐시는 이미 빌드 몫을 차지하고 있어, 다시 지을 때는 바뀐 크레이트의
# 산출물만 덮어쓴다(CARGO_INCREMENTAL=0) — 그래서 처음 빌드 몫에서 캐시 크기를 빼고, 하한을 둔다.
# 고정 문턱이면 unit 한 번 뒤(캐시 10 GB 남짓 · 여유 2 GB 남짓, 실측 2026-11-02) 늘 지우게 된다.
need_free_kb() {
    local need=$((MIN_FREE_GB * 1048576 - $1)) floor=$((KEEP_MIN_FREE_GB * 1048576))
    [ "$need" -ge "$floor" ] || need="$floor"
    echo "$need"
}

# auto 에서 캐시를 지울 까닭 — 지울 일이 없으면 빈 값.
clean_reason() {
    local free kb need rev counts dirty total
    kb="$(guest_target_kb)"
    free="$(guest_free_kb)"
    need="$(need_free_kb "$kb")"
    if [ -n "$free" ] && [ "$free" -lt "$need" ]; then
        echo "디스크 여유 $((free / 1048576)) GB 가 필요한 $((need / 1048576)) GB 보다 적다"; return
    fi
    [ "$kb" -gt 0 ] || return 0
    if [ "$kb" -gt $((CLEAN_MAX_TARGET_GB * 1048576)) ]; then
        echo "빌드 캐시 $((kb / 1048576)) GB 가 ${CLEAN_MAX_TARGET_GB} GB 를 넘는다"; return
    fi
    rev="$(guest "cat $BUILT_REV 2>/dev/null  # tasty-built-rev")"
    case "$rev" in
        ''|*[!0-9a-f]*) echo "지난 빌드의 커밋 표지가 없다"; return ;;
    esac
    git -C "$ROOT" cat-file -e "$rev^{commit}" 2>/dev/null \
        || { echo "지난 빌드의 커밋 $rev 를 이 저장소가 모른다"; return; }
    counts="$(dirty_crates "$rev")" || { echo "지난 빌드와 비교하지 못했다"; return; }
    if [ "$counts" = all ]; then
        echo "작업 공간 설정(Cargo.toml · Cargo.lock · 툴체인)이 바뀌었다"; return
    fi
    read -r dirty total <<<"$counts"
    if [ $((dirty * 100)) -gt $((total * CLEAN_DIRTY_PCT)) ]; then
        echo "바뀐 크레이트 $dirty/$total 가 ${CLEAN_DIRTY_PCT}% 를 넘는다"; return
    fi
    echo "keep:바뀐 크레이트 $dirty/$total"
}

# 시험 전에 빌드 캐시를 (필요하면) 지우고 디스크 여유를 잰다. 옮기기 **전에** 한다 — 모자란 채로
# 옮기고 빌드하면 수십 분 뒤 rustc 가 디스크 부족으로 죽고, 그 실패는 시험 실패처럼 보인다.
# 지우는 것은 이 스크립트가 만든 자리뿐이다(트리의 target · 단계 로그).
clean_guest() {
    local reason='' keep='TASTY_VM_CLEAN=0'
    case "$CLEAN" in
        1) reason='TASTY_VM_CLEAN=1' ;;
        auto)
            reason="$(clean_reason)"
            case "$reason" in keep:*) keep="${reason#keep:}"; reason='' ;; '') keep='캐시 없음' ;; esac ;;
    esac
    if [ -n "$reason" ]; then
        say "빌드 캐시를 지운다 — $reason"
        guest "{ [ ! -f $GUEST_TREE/Cargo.toml ] || (cd $GUEST_TREE && cargo clean -q) || rm -rf $GUEST_TREE/target; } &&
            find ~ -maxdepth 1 -name 'vm-*.log' -delete"
    else
        say "빌드 캐시를 남긴다 ($keep)"
    fi
    local kb need=$((MIN_FREE_GB * 1048576))
    [ -n "$reason" ] || need="$(need_free_kb "$(guest_target_kb)")"
    kb="$(guest_free_kb)"
    [ -n "$kb" ] || die "가상머신의 디스크 여유를 못 읽었다"
    say "디스크 여유 $((kb / 1048576)) GB (필요 $((need / 1048576)) GB)"
    [ "$kb" -ge "$need" ] \
        || die "디스크 여유가 $((kb / 1048576)) GB 로 필요한 $((need / 1048576)) GB 보다 적다 — shell 로 들어가 홈의 큰 자리(du -sh ~/*)를 지우거나 디스크를 늘린다(docs/dev-guide/macos-vm-tests.md §3)"
}

# 옮긴 트리의 커밋을 표지로 남긴다 — 다음 auto 판정이 그 커밋과 비교한다.
stamp_guest() {
    local rev
    rev="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null)" || return 0
    guest "mkdir -p $GUEST_TREE/target && echo $rev > $BUILT_REV"
}

# 구간마다 걸린 시간을 남긴다 — 실행끼리 비교해 빌드·시험·옮기기 가운데 어디가 느려졌는지 본다.
# `$OUT/timings.tsv` 에 `구간<TAB>초<TAB>rc` 를 끝날 때마다 한 줄씩 더하므로, 도중에 멈춰도 끝난 구간은 남는다.
# 구간 함수를 `|| rc=$?` 로 감싸 재지 않는다 — 그 문맥에서는 함수 안의 `set -e` 가 꺼진다.
TIMINGS=()
PHASE_T0=0
phase_start() { PHASE_T0=$(date +%s); }
phase_end() {
    local sec=$(($(date +%s) - PHASE_T0))
    TIMINGS+=("$1 $sec ${2:-0}")
    printf '%s\t%s\t%s\n' "$1" "$sec" "${2:-0}" >>"$OUT/timings.tsv"
}
print_timings() {
    [ ${#TIMINGS[@]} -gt 0 ] || return 0
    local row name sec rc
    say "구간별 시간 — $OUT/timings.tsv"
    for row in "${TIMINGS[@]}"; do
        read -r name sec rc <<<"$row"
        say "$(printf '  %-12s %3dm %02ds' "$name" $((sec / 60)) $((sec % 60)))$([ "$rc" = 0 ] || echo " rc=$rc")"
    done
}

# 한 단계를 돌리고 로그를 호스트로 가져온다. 실패해도 다음 단계로 간다 — 한 번에 전부 본다.
# 결과는 반환값이 아니라 LAST_RC 에 둔다 — 0 이 아닌 반환은 `set -e` 로 다음 단계를 막는다.
FAILED=()
LAST_RC=0
step() {
    local name="$1" script="$2"
    say "▶ $name"
    local rc=0
    phase_start
    # 증분 컴파일 캐시를 안 만든다 — 늘리기 전 이미지 디스크(50 GB)에서 첫 빌드 캐시 16 GB 중 9.4 GB 가
    # 그것이었고, 두 번째 빌드에서 디스크가 차 rustc 가 죽었다(실측 2026-10-15). CI 와 같은 설정이다.
    guest "cd $GUEST_TREE && export CARGO_INCREMENTAL=0 && { $script ; } > ~/vm-$name.log 2>&1" || rc=$?
    guest "cat ~/vm-$name.log" > "$OUT/$name.log" || true
    phase_end "$name" "$rc"
    LAST_RC=$rc
    if [ "$rc" -eq 0 ]; then
        say "  통과 — $OUT/$name.log"
    else
        say "  실패 rc=$rc — $OUT/$name.log"
        FAILED+=("$name")
    fi
}

cmd_test() {
    local scope="${1:-all}"
    case "$scope" in all|unit|gates|gui|windows|pkg) ;; *) die "범위는 all·unit·gates·gui·windows·pkg 중 하나다: $scope" ;; esac
    if [ "$scope" = pkg ] && [ -z "${TASTY_VM_PKG:-}" ]; then
        [ -n "${TASTY_VM_BASE:-}" ] || die "pkg 범위는 TASTY_VM_PKG 나 TASTY_VM_BASE 가 필요하다"
        TASTY_VM_PKG="$(changed_packages "$TASTY_VM_BASE")" || die "TASTY_VM_BASE 와의 분기점을 못 찾는다: $TASTY_VM_BASE"
        say "바뀐 패키지 — ${TASTY_VM_PKG:-(없음)}"
    fi
    mkdir -p "$OUT"
    printf 'phase\tseconds\trc\n' >"$OUT/timings.tsv"
    phase_start; boot; phase_end boot
    phase_start; clean_guest; phase_end clean
    phase_start; sync_tree; stamp_guest; phase_end sync
    guest "sw_vers -productVersion; cd $GUEST_TREE && rustc --version" | tee "$OUT/env.txt"

    if [ "$scope" = all ] || [ "$scope" = gates ]; then
        step gates 'cargo build -q -p tasty-doc-guards --bin mask-source --bin strip-cfg-test &&
            for s in check-intent-discipline check-allow-reason check-shared-walk-ratchet check-shell-assets; do
                echo "== $s"; bash scripts/$s.sh || exit 1; done &&
            for s in check-file-size check-frozen-sum-ratchet; do
                echo "== $s"; /opt/homebrew/bin/bash scripts/$s.sh || exit 1; done'
    fi
    if [ "$scope" = all ] || [ "$scope" = unit ]; then
        # plugin 실행 파일을 먼저 짓는다 — `cargo test` 는 그것을 `target/debug/` 에 안 만들고, e2e
        # 하네스는 그 자리의 plugin 을 띄운다(없으면 `unknown surface kind: …`).
        # 빌드와 시험을 단계로 나눠 각자의 시간과 로그를 남긴다. 빌드가 실패하면 시험은 돌리지 않는다.
        local rf="${TASTY_VM_RUSTFLAGS:+export RUSTFLAGS='$TASTY_VM_RUSTFLAGS'; }"
        step unit-build "$rf"'cargo build --workspace --locked'
        if [ "$LAST_RC" -eq 0 ]; then
            step unit "$rf"'cargo test --workspace --locked --no-fail-fast'
        else
            say "  unit 을 건너뛴다 — 빌드가 실패했다"
            FAILED+=(unit)
        fi
    fi
    if [ "$scope" = pkg ]; then
        # all 에 안 넣는다 — PR 처럼 바꾼 패키지만 빨리 잴 때 쓰고, 전체 unit 은 통합 브랜치에서 돈다.
        # 워크스페이스는 먼저 다 짓는다 — 바꾼 API 를 쓰는 다른 패키지의 컴파일 깨짐은 여기서 잡는다.
        local rf="${TASTY_VM_RUSTFLAGS:+export RUSTFLAGS='$TASTY_VM_RUSTFLAGS'; }"
        step unit-build "$rf"'cargo build --workspace --locked'
        if [ "$LAST_RC" -ne 0 ]; then
            say "  pkg 를 건너뛴다 — 빌드가 실패했다"
            FAILED+=(pkg)
        elif [ -z "$TASTY_VM_PKG" ]; then
            say "  시험할 패키지가 없다 — 빌드만 했다"
        else
            local pk=''; for p in $TASTY_VM_PKG; do pk="$pk -p $p"; done
            step pkg "${rf}cargo test --locked --no-fail-fast$pk"
        fi
    fi
    if [ "$scope" = all ] || [ "$scope" = gui ]; then
        # 입력 소스가 영문이어야 키 입력 시험이 산다(tests/gui_tests.rs 머리말).
        # exec 길의 권한은 prepare 를 다시 안 돌린 가상머신에도 있어야 한다 — 여기서 확인하고 없으면 넣는다.
        [ "$TRANSPORT" != exec ] || grant_screen_capture
        check_input_grant
        step gui 'cargo test --locked --test gui_tests -- --ignored --test-threads=1'
    fi
    # Windows 대상 컴파일·clippy 를 가상머신에서 잰다 — 실행은 안 한다. all 에 안 넣는다: Windows 대상의
    # 의존을 따로 지어 시간과 디스크가 두 배 가까이 든다. 절차: docs/dev-guide/macos-vm-tests.md §2.
    if [ "$scope" = windows ]; then
        guest 'command -v x86_64-w64-mingw32-gcc >/dev/null' \
            || die "가상머신에 mingw-w64 를 깔지 않았다 — prepare 를 다시 돌린다"
        step windows 'rustup target add x86_64-pc-windows-gnu &&
            cargo check --locked --target x86_64-pc-windows-gnu --workspace --all-targets --keep-going &&
            cargo clippy --locked --target x86_64-pc-windows-gnu --workspace --all-targets -- -D clippy::correctness'
    fi

    print_timings
    if [ ${#FAILED[@]} -eq 0 ]; then
        say "전부 통과 ($scope)"
    else
        say "실패한 단계: ${FAILED[*]} — 로그는 $OUT"
        exit 1
    fi
}

cmd_shell() {
    boot
    [ "$TRANSPORT" = exec ] && exec tart exec -i -t "$VM" zsh -l
    local ip
    ip="$(vm_ip)"
    exec sshpass -e ssh "${SSH_OPTS[@]}" "$VM_USER@$ip"
}

case "${1:-}" in
    prepare) cmd_prepare ;;
    test) shift; cmd_test "${1:-all}" ;;
    shell) cmd_shell ;;
    stop) tart stop "$VM" ;;
    destroy) tart delete "$VM" ;;
    *) sed -n '2,20p' "$0"; exit 2 ;;
esac
