# shellcheck shell=bash
# 같은 자리의 동시 실행을 잠금 파일 하나로 막는다. 기다리지 않는다.
# 잠금은 bash noclobber 리다이렉션(없는 파일을 O_EXCL로 연다)으로 만든다. 외부 mkdir에 기대지 않으므로
# coreutils 구현과 무관하다(일부 uutils mkdir은 경합에서 진 쪽도 0으로 끝난다). flock은 macOS에 기본으로 없다.
# 한 셸에서 잠금 하나만 잡는다. bash 3.2에서도 돈다.
#
# 호출자 계약:
#   - INT·TERM trap은 run_lock_signal <종료 코드>를 부른다(trap 'run_lock_signal 130' INT 등).
#   - EXIT trap(cleanup)의 첫 명령은 run_lock_defer_signals "$?"다(종료 상태는 run_lock_status에 남는다).
#     그 뒤 정리를 하고 run_lock_release가 실패하면 경고를 낸 다음 run_lock_finish로 끝낸다.
#     정리 동안 온 INT·TERM은 기록만 하므로 셸은 남은 정리를 계속하고 마지막에 그 코드로 끝난다.
#     정리 중인 외부 명령(rm·mv 등)은 같은 그룹 신호로 끝날 수 있고 그 실패는 호출자의 기존 처리를 따른다.
#
# 잠금 파일 내용은 이 실행이 먼저 만든 표지 파일(<잠금>.<pid>.<난수>)의 경로다. 표지 파일이 있는 동안 같은 이름은
# 다시 생기지 않으므로 내용으로 자기 잠금을 알아보고, 자기 잠금일 때만 지운다.
# kill -9처럼 trap이 돌지 않는 종료는 잠금과 표지를 남긴다. 진행 중인 실행이 없는 것을 확인하고 둘 다(<잠금>, <잠금>.*) 지운다.
# 한계:
#   - 존재 검사와 noclobber 열기 사이에 같은 경로에 FIFO·장치가 생기면 bash가 O_EXCL 없이 연다.
#   - 파일을 연 뒤 리다이렉션 단계(dup2 등)가 실패하면 파일은 생겼지만 열지 못한 것으로 보인다. 다른 실행이 막 연
#     빈 잠금과 구별할 수 없으므로 그 빈 잠금·표지는 남고 다음 실행은 "이미 있음"으로 거부된다.
#   - 잠금에 표지를 쓰다 실패한 뒤 그 잠금을 지우지 못하면 소유를 확인할 수 없어 남긴다.
#   - cleanup이 시작해 첫 명령을 돌리기 전에 온 신호는 정리를 건너뛰고 끝낸다(셸에서 닫을 수 없는 구간).
#   - bash 5.2.21에서 SIGINT를 계속 받으며 자식을 거듭 만드는 반복(잡기·놓기 반복)에서 셸이 끝난 자식을 거두지 못하고
#     도는 경우를 드물게 관측했다(원인 미확인). 미루는 구간은 잡기·놓기·정리뿐이라 그 밖에서는 지금처럼 바로 끝난다.
#   - bash 3.2는 전경 외부 명령을 기다리는 동안 받은 SIGINT를 버리는 경우가 있다(측정). 잠금 상태는 어긋나지 않고
#     실행이 계속된다. `&`로 띄운 비대화형 bash는 SIGINT가 무시된 채 시작해 INT trap이 걸리지 않는다.
#   - 잠금을 지웠는지(rm rc 0 또는 부모 폴더 나열로 부재 확정) 또는 남의 것인지 확인하지 못하면(조회·읽기·나열 실패)
#     경고하고 잠금과 표지를 남긴다.

run_lock_path=""
run_lock_token=""
run_lock_token_owned=0
run_lock_held=0
run_lock_deferring=0
run_lock_exit=""
run_lock_status=0
run_lock_opened=0

# 호출자의 INT·TERM trap에서 부른다. 미루는 구간이면 기록만 한다. 아니면 미룸을 켠 채 끝내 cleanup이
# 첫 명령부터 신호를 기록하게 한다.
run_lock_signal() {
    if [ "$run_lock_deferring" -eq 1 ]; then
        run_lock_exit=$1
        return 0
    fi
    run_lock_deferring=1
    exit "$1"
}

# cleanup의 첫 명령. 종료 상태를 run_lock_status에 두고 이후 INT·TERM을 기록만 한다.
run_lock_defer_signals() {
    run_lock_deferring=1
    run_lock_status=$1
    trap 'run_lock_signal 130' INT
    trap 'run_lock_signal 143' TERM
}

# cleanup의 끝. 정리 동안 신호가 왔으면 그 코드로, 아니면 <rc>(없으면 run_lock_status)로 끝낸다.
run_lock_finish() {
    exit "${run_lock_exit:-${1:-$run_lock_status}}"
}

# 미루는 구간을 닫는다. 기록된 신호가 있으면 값을 비우고 그 코드로 끝낸다(EXIT trap이 정리한다).
_run_lock_resume() {
    local code
    run_lock_deferring=0
    if [ -n "$run_lock_exit" ]; then
        code=$run_lock_exit
        run_lock_exit=""
        run_lock_deferring=1
        exit "$code"
    fi
}

_run_lock_unlink() {
    rm -- "$1"
}

# 경로가 없음을 확정한다. 부모 폴더에서 그 이름만 고르는 find(-name에는 패턴 문자를 이스케이프한 이름)가 종료 0이고
# 아무것도 출력하지 않을 때만 참이다(종료 0이면 나열을 끝까지 읽었다). stat·나열 오류는 부재로 보지 않는다.
_run_lock_absent() {
    local path=$1 dir base pat out
    case $path in
        */*) dir=${path%/*} base=${path##*/} ;;
        *) dir=. base=$path ;;
    esac
    [ -n "$dir" ] || dir=/
    case $dir in -*) dir=./$dir ;; esac
    pat=${base//\\/\\\\}
    pat=${pat//\*/\\*}
    pat=${pat//\?/\\?}
    pat=${pat//\[/\\[}
    out=$(find "$dir/." ! -name . -prune -name "$pat" -print 2>/dev/null) || return 1
    [ -z "$out" ]
}

# 지운다(-f 없이). 쓰기 금지 파일에서 확인을 묻지 않게 stdin을 /dev/null로 둔다. rm이 0이거나, 실패했어도 경로가
# 없음이 확정되면(같은 그룹 신호가 unlink 뒤의 rm을 죽이면 실패로 보고된다) 지운 것으로 본다.
_run_lock_rm() {
    _run_lock_unlink "$1" < /dev/null 2>/dev/null || _run_lock_absent "$1"
}

# 잠금이 지금 이 실행의 표지를 가리키는지 본다. 0 이 실행의 것(내용 전체가 표지 경로와 줄바꿈 하나),
# 1 아니다(링크, 폴더·FIFO·소켓·장치, NUL에서 멈춤, 다른 내용을 읽음), 2 확인할 수 없다(없거나 stat 실패, 일반
# 파일인지 모름, 읽기 권한 없음, 읽은 내용이 이 실행 내용의 엄밀한 앞부분뿐). 이 실행의 잠금을 읽다 오류·신호로 멈추면
# 앞부분만 읽히므로 "아니다"로 보지 않는다. 다른 실행의 잠금은 그 실행의 표지 경로를 담아 앞부분이 되지 않는다.
_run_lock_is_mine() {
    local content="" want="$run_lock_token"$'\n'
    if [ -L "$run_lock_path" ]; then return 1; fi
    [ -e "$run_lock_path" ] || return 2
    if [ ! -f "$run_lock_path" ]; then
        if [ -d "$run_lock_path" ] || [ -p "$run_lock_path" ] || [ -S "$run_lock_path" ] \
            || [ -b "$run_lock_path" ] || [ -c "$run_lock_path" ]; then
            return 1
        fi
        return 2
    fi
    [ -r "$run_lock_path" ] || return 2
    if IFS= read -r -d '' content < "$run_lock_path"; then return 1; fi
    if [ "$content" = "$want" ]; then return 0; fi
    case $want in "$content"*) return 2 ;; esac
    return 1
}

# 표지를 지운다. 지우지 못하면 경고한다(남은 표지는 다음 실행을 막지 않는다).
_run_lock_drop_token() {
    [ "$run_lock_token_owned" -eq 1 ] || return 0
    if _run_lock_rm "$run_lock_token" || _run_lock_rm "$run_lock_token"; then
        run_lock_token_owned=0
        run_lock_token=""
    else
        echo "Warning: could not remove $run_lock_token" >&2
    fi
}

# 리다이렉션을 건 채 부른다. 본문에 들어왔으면 열기는 성공한 것이다(쓰기 실패와 구분한다).
_run_lock_fill() {
    run_lock_opened=1
    printf '%s\n' "$run_lock_token"
}

# 결과: 0 잡음, 1 이미 있음(다른 실행 또는 남은 잠금), 2 만들 수 없음(부모 폴더 없음·권한·쓰기 실패).
# noclobber는 여는 단순 명령 동안만 켜고, 신호를 미룬 채 진입 상태로 되돌린 뒤 미룸을 푼다.
run_lock_acquire() {
    local lock=$1 had_c=0 cand tries=0 rc=0 ok
    if [ "$run_lock_held" != 0 ] || [ "$run_lock_token_owned" -eq 1 ]; then
        return 2
    fi
    case $- in *C*) had_c=1 ;; esac
    # noclobber는 기존 경로가 일반 파일이 아니면 O_EXCL 없이 연다(FIFO는 열기에서 멈춘다). 열기 전에 거부한다.
    if [ -e "$lock" ] || [ -L "$lock" ]; then return 1; fi
    while :; do
        if [ "$tries" -ge 100 ]; then return 2; fi
        tries=$((tries + 1))
        cand="$lock.$$.$RANDOM$RANDOM"
        if [ -e "$cand" ] || [ -L "$cand" ]; then continue; fi
        run_lock_deferring=1
        set -C
        if { : > "$cand"; } 2>/dev/null; then ok=1; else ok=0; fi
        if [ "$had_c" -eq 0 ]; then set +C; fi
        if [ "$ok" -eq 1 ]; then
            run_lock_token=$cand
            run_lock_token_owned=1
        fi
        _run_lock_resume
        if [ "$ok" -eq 1 ]; then break; fi
        if [ -e "$cand" ] || [ -L "$cand" ]; then continue; fi
        return 2
    done
    if [ -e "$lock" ] || [ -L "$lock" ]; then
        rc=1
    else
        run_lock_path=$lock
        run_lock_opened=0
        run_lock_deferring=1
        run_lock_held=pending
        set -C
        if { _run_lock_fill > "$lock"; } 2>/dev/null; then ok=1; else ok=0; fi
        if [ "$had_c" -eq 0 ]; then set +C; fi
        if [ "$ok" -eq 1 ]; then
            run_lock_held=1
        elif [ "$run_lock_opened" -eq 1 ]; then
            # 열었지만 쓰지 못했다. 만든 잠금을 한 번 지운다. 내용이 온전하지 않아 소유를 다시 확인할 수 없으므로
            # 지우지 못했으면 남기고 다시 지우지 않는다. 그때 표지도 소유를 놓아 남긴다.
            run_lock_held=0
            _run_lock_rm "$lock" || run_lock_token_owned=0
            rc=2
        else
            run_lock_held=0
            if [ -e "$lock" ] || [ -L "$lock" ]; then rc=1; else rc=2; fi
        fi
        _run_lock_resume
    fi
    if [ "$rc" -ne 0 ]; then _run_lock_drop_token; fi
    return "$rc"
}

# 잡은 잠금을 놓는다. 표지는 잠금을 지운 것(rm rc 0 또는 부재 확정)을 확인했거나 잠금이 다른 실행의 것임을 읽어 확인했을 때만 지운다.
# 그 밖(조회·읽기 실패, 확인할 수 없는 내용)은 1을 돌려주고 표지를 남긴다(지우면 다음 실행이 같은 표지 이름을 다시
# 만들어 남은 잠금을 자기 것으로 볼 수 있다).
#   - held=1(잡기가 끝났고 아직 지우지 않았다): held=pending으로 내린 뒤 첫 삭제를 내용 확인 없이 한다. 읽기 권한을
#     빼는 umask에서도 지운다. rc 0이면 끝낸다.
#   - held=pending이거나 첫 삭제가 실패하면: 부재가 확정되면 끝, is_mine 1이면 끝(남의 잠금을 지우지 않는다), 2면 한 번 더 읽고 그래도
#     2면 1, 0이면 다시 지운다(최대 두 번, rc 0이면 끝).
# 놓는 동안 온 INT·TERM은 run_lock_exit에 남고, 스스로 끝내지 않는다. trap은 건드리지 않는다.
run_lock_release() {
    local rc=0 tries=0 own=0 was=$run_lock_deferring
    run_lock_deferring=1
    if [ "$run_lock_held" = 1 ]; then
        run_lock_held=pending
        if _run_lock_rm "$run_lock_path"; then run_lock_held=0; fi
    fi
    while [ "$run_lock_held" != 0 ]; do
        if _run_lock_absent "$run_lock_path"; then
            run_lock_held=0
            break
        fi
        own=0
        _run_lock_is_mine || own=$?
        if [ "$own" -eq 2 ]; then
            own=0
            _run_lock_is_mine || own=$?
        fi
        if [ "$own" -eq 1 ]; then
            run_lock_held=0
            break
        fi
        if [ "$own" -eq 2 ] || [ "$tries" -ge 2 ]; then
            rc=1
            break
        fi
        tries=$((tries + 1))
        if _run_lock_rm "$run_lock_path"; then run_lock_held=0; fi
    done
    if [ "$run_lock_held" = 0 ]; then _run_lock_drop_token; fi
    run_lock_deferring=$was
    return "$rc"
}
