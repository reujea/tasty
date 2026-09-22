#!/usr/bin/env bash
# plugin 매니페스트 version 이 **산출물이 달라질 때** 올랐는지 판정한다.
#
# 규칙의 목적은 라이브 반영이다 — `upgrade-builtins` 재sync 는 매니페스트 `version` 이
# 올랐을 때만 동작한다(same-version skip). 그래서 판정 기준도 "파일이 staged 되었는가"
# 가 아니라 **"그 plugin 의 빌드 산출물이 달라지는가"** 다. 근거·대안·재검토 조건은
# docs/adr/ 의 plugin 버전 채널 ADR.
#
# 판별식이 **문턱이 아닌 이유**: "큰 커밋은 전역 sweep 이니 봐준다" 는 식의 파일 수
# 문턱은 임의값이다. 실측(규칙 도입 이후 이탈 105 건)에서 문턱을 5→∞ 로 옮기면 국소
# 이탈이 56→105 로 약 2 배 흔들렸고, 같은 문턱에서도 세는 대상을 crates 파일/전체
# 파일 중 무엇으로 하느냐로 83→71 이 갈렸다. 그래서 여기서는 knob 을 쓰지 않는다.
#
# 대신 **rustfmt 로 정규화한 뒤 내용이 같은가**로 가른다. `git diff -w` 로는 부족하다 —
# rustfmt 재정렬은 줄바꿈을 바꾸는데 `-w` 는 줄 *안* 의 공백만 무시한다(실측으로 밟았다).
# 공백을 통째로 제거하는 방식도 쓰지 않는다 — 문자열 리터럴 안의 공백까지 지워서
# `"a b"` 와 `"ab"` 를 같다고 판정하는 **거짓 음성**의 구멍이 생긴다. rustfmt 는 진짜
# 파서라 리터럴 내용을 보존한다.
#
# 실측(가드 이후 창, 이탈 40 쌍)에서 세 정규화를 **양극성으로** 비교했다:
#
#     정규화              부채로 남음   오탐으로 배제   비고
#     exact                    39             0
#     공백 전부 제거            35             4         배제된 4 는 전부 `style:` 커밋
#     rustfmt                  33             6         배제된 6 도 전부 `style:` 커밋
#
# rustfmt 정규화가 **삼킨 6 쌍은 `style: cargo fmt` 커밋 두 개에서만** 나왔다. 남은
# 33 쌍은 fix 8 · docs 8 · feat 6 · refactor 5 · style 1 로, 그 style 1 이 커밋 type 을
# 판별식으로 쓸 수 없다는 증거다 — 한 `style:` 커밋이 plugin 별로, 나아가 파일별로
# 갈린다. 판별식은 커밋보다도 파일 종류보다도 잘게 자른다.
#
# **알려진 오탐: 주석만 바뀐 변경 8 쌍(41 중 20%)** — rustfmt 는 주석을 지우지 않으므로
# `docs:` 류가 여기 걸린다. 주석까지 제거하면 배제가 14 로 늘지만, 줄 단위 정규식
# 주석 제거는 raw string 안의 `//` 를 잘못 지워 **거짓 음성**을 만든다. 오탐(불필요한
# bump 하나)과 거짓 음성(라이브 반영이 조용히 깨짐)의 대가가 비대칭이므로 여기서는
# 오탐 쪽을 감수한다. 재검토 조건은 ADR-0137.
#
# ── 무엇을 그 plugin 의 산출물로 세는가 ────────────────────────────────
#
# **plugin 디렉토리만 보면 공유 크레이트를 통째로 놓친다.** 번들 plugin 은
# `tasty-plugin-agent-common`·`tasty-plugin-sdk`·`tasty-utils` 같은 워크스페이스 크레이트를
# 링크하고, 그것들이 바뀌면 plugin 바이너리가 달라진다. 그런데 그 크레이트들은 매니페스트가
# 없어 판정 대상에서 자연히 빠졌다 — 버전 그대로인 채 내용만 다른 산출물이 발행되고,
# `upgrade-builtins` 는 same-version skip 이라 **이미 그 버전을 받은 인스턴스는 재시작 없이
# 그 수정을 영영 못 받는다.** 빌드도 테스트도 초록인데 반영만 안 된다.
#
# 실측(가드 도입 이후 main 395 커밋): 공유 크레이트를 건드린 커밋 22 건, 그 22 건이 요구했어야
# 할 plugin bump 147 건 중 실제로 오른 것은 7 건. 영향 plugin 을 전부 올린 커밋은 1 건뿐이었다.
#
# 그래서 판정 대상 파일 집합을 **워크스페이스 내부 의존 폐포**로 넓힌다(`cargo tree`,
# normal+build 만 — dev-의존은 산출물에 안 들어간다). 그 값은 변경된 크레이트 중 매니페스트
# 없는 것이 하나라도 있을 때만 구한다(plugin 하나당 ~0.15 s).
#
# ── 그리고 **출하되는 내용**만 센다 ────────────────────────────────────
#
# 폐포로 넓히면 fan-out 이 생긴다 — `tasty-utils` 한 줄이 9 개 plugin 전부를 건드린다. 그래서
# 무엇을 "달라졌다" 로 셀지가 전보다 훨씬 크게 작동한다. 산출물에 안 들어가는 것을 세면 안 된다:
#
#   · 인라인 `#[cfg(test)]` 범위
#   · `#[cfg(test)] mod x;` 로만 선언된 **파일 전체**
#   · 술어가 `test` 를 **요구하는** `cfg_attr` 의 **속성 줄**(그 자리에 있으라고 요구되는
#     근거 주석 포함). 범위가 다르다 — `cfg_attr` 은 붙는 속성만 조건부라 **항목은 출하된다.**
#
# 셋 다 `strip-cfg-test --blank-test-only-files` 가 지운다 — 파일 SLOC 게이트가 이미 쓰는
# 판정기다(ADR-0165). 여기서 술어를 다시 구현하지 않는다. 실측에서 이 축이 147 요구 중
# 36 을 없앴고, 그 36 은 전부 전체-테스트 파일이었다.
#
# 셋째 형태는 처음에 빠져 있었고, 이 게이트의 실회차 첫 발화가 그 때문에 거짓 양성이었다
# (공유 크레이트 둘의 크레이트 루트 `#![cfg_attr(test, …)]` 한 줄씩이 plugin 6 개에 bump 를
# 요구했다 — 출하 산출물은 비트 단위로 동일). 경위와 재검토 조건은 ADR-0166.
#
# **빈 줄을 접는 이유**: 그 판정기는 줄 번호 보존용으로 지운 자리를 빈 줄로 남긴다. 안 접으면
# `#[cfg(test)] mod x;` 두 줄이 는 것이 "내용이 달라졌다" 로 읽힌다(실측으로 밟았다).
#
# 사용:
#   check-plugin-version-bump.sh --staged [--base <rev>]   # index 를 base 와 비교 (기본 base=HEAD)
#   check-plugin-version-bump.sh --range <before> <after>   # 두 커밋의 끝점 비교
#
# 끝점 비교인 이유: 목적이 "반영될 때 version 이 올라 있는가" 라, 한 lane 의 커밋마다
# 올릴 필요가 없다. 한 번 오르면 재sync 는 동작한다.
#
# exit: 0 통과 · 1 위반 · 2 판정 불가(인자·git 환경). **판정 불가를 통과로 내지 않는다.**

set -uo pipefail

die() { printf '%s\n' "$*" >&2; exit 2; }

git rev-parse --git-dir >/dev/null 2>&1 || die "판정 불가: git 저장소가 아니다 (배포 tarball 등)."
ROOT="$(git rev-parse --show-toplevel)" || die "판정 불가: 저장소 루트를 못 찾았다."
cd "$ROOT" || die "판정 불가: 저장소 루트로 이동 실패."

# 정규화기가 없으면 **판정 불가**다. 없는 채로 통과시키면 0 을 통과로 세는 형태가 된다.
command -v rustfmt >/dev/null 2>&1 \
    || die "판정 불가: rustfmt 가 없다 — 포맷 변경과 실변경을 가를 수 없다. rustup component add rustfmt"

# 출하 판정기. **여기서 빌드하지 않는다** — pre-commit 에서 cargo 를 부르면 바깥 cargo 와
# 락을 다툰다(파일 SLOC 게이트가 같은 이유로 같은 형태를 쓴다). 없으면 판정 불가다.
# 판정기를 찾는 것과 **낡았는지 보는 것**은 공용이다 — 이 계열 게이트 셋이 같은 크레이트의
# CLI 판정기를 부르고, 규칙을 게이트마다 따로 쓰면 같은 물음에 답이 셋이 된다.
# 스크립트 자신의 위치에서 읽는다 — `$ROOT` 는 **판정 대상 저장소**라, 합성 픽스처
# 저장소에서 부르면 거기엔 이 파일이 없다.
. "$(cd "$(dirname "$0")" && pwd)/lib/judge-bin.sh"
resolve_judge strip-cfg-test TASTY_STRIP_CFG_TEST_BIN "$ROOT"
STRIP_BIN="$JUDGE_BIN"
# 없거나 낡았으면 **더 넓게** 본다. 여기서 판정 불가로 죽이지 않는 이유는, 이 스크립트가 갓
# 클론한 트리의 pre-commit 에서도 불리기 때문이다. 넓게 보는 방향은 조용한 통과를 안 만든다 —
# 출하 밖 변경(테스트 전용)이 bump 를 요구하는 오탐이 될 뿐이고, ADR-0137 이 적은 비대칭
# (오탐 하나 vs 라이브 반영이 조용히 깨짐)에서 감수하는 쪽이다. 사유는 헬퍼가 말한다.
if [ -z "$STRIP_BIN" ]; then
    echo "[plugin-version] 출하 범위를 못 좁힌다 — 테스트 전용 변경까지 bump 를 요구한다." >&2
fi

# edition 을 루트 Cargo.toml 에서 읽는다. 여기 박아두면 워크스페이스가 올릴 때 만료된다.
RUST_EDITION=$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' Cargo.toml | sed -n '1p')
[ -n "$RUST_EDITION" ] || die "판정 불가: 루트 Cargo.toml 에서 edition 을 못 읽었다."

MODE=""; BASE="HEAD"; BEFORE=""; AFTER=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --staged) MODE=staged; shift ;;
        --base)   BASE="${2:-}"; [ -n "$BASE" ] || die "판정 불가: --base 에 rev 가 없다."; shift 2 ;;
        --range)  MODE=range; BEFORE="${2:-}"; AFTER="${3:-}"
                  [ -n "$BEFORE" ] && [ -n "$AFTER" ] || die "판정 불가: --range 는 두 rev 를 요구한다."
                  shift 3 ;;
        *) die "판정 불가: 알 수 없는 인자 '$1'" ;;
    esac
done
[ -n "$MODE" ] || die "판정 불가: --staged 또는 --range 중 하나를 줘야 한다."

# ── 비교 양끝 확정 ────────────────────────────────────────────
# staged 모드에서 after 쪽은 **인덱스**다. git 은 인덱스 블롭을 `:<경로>` 로 준다.
if [ "$MODE" = staged ]; then
    if ! git rev-parse --verify --quiet "$BASE^{commit}" >/dev/null; then
        # 첫 커밋 — 비교 대상이 없다. 의무도 없다(새 plugin 의 최초 버전).
        echo "[plugin-version] skip: 비교할 커밋이 없다(첫 커밋). 판정 대상 0."
        exit 0
    fi
    BEFORE_REV="$BASE"
    AFTER_PREFIX=""          # `:경로`
else
    for r in "$BEFORE" "$AFTER"; do
        git rev-parse --verify --quiet "$r^{commit}" >/dev/null \
            || die "판정 불가: rev 를 못 찾았다: $r (shallow clone 이면 fetch-depth 를 늘려라)"
    done
    BEFORE_REV="$BEFORE"
    AFTER_PREFIX="$AFTER"
fi

# 경로의 after 쪽 좌표. staged 는 `:경로`, range 는 `<after>:경로`.
after_ref() { printf '%s:%s' "$AFTER_PREFIX" "$1"; }

exists_at() { git cat-file -e "$1" 2>/dev/null; }

# ── 한쪽 끝의 크레이트 트리를 펼친다 ─────────────────────────────────
# 전체-테스트 파일 판정은 **다른 파일의 선언**(`#[cfg(test)] mod x;`)을 봐야 하므로
# 파일 하나만 꺼내면 답이 안 나온다. 그래서 관련 크레이트를 통째로 펼친 뒤 판정기를 한 번
# 돌린다. 펼치는 대상은 변경에 걸린 크레이트뿐이라 워크스페이스 규모와 무관하다.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# 이 plugin 이 링크하는 **워크스페이스 내부** 크레이트 이름들(한 줄에 하나)을 캐시 파일에
# 쓰고 그 경로를 `CLOSURE_FILE` 에 둔다. 후보 선정과 파일 집합 계산에서 두 번 묻는다.
#
# 이름 **정확 일치**로 본다. 부분 문자열로 보면 `tasty-utils` 가 `tasty-utils-extra` 에도
# 걸려 엉뚱한 plugin 을 판정 대상으로 끌어들인다.
#
# **값을 표준출력으로 돌려주지 않는다 — 반드시 본 셸에서 부른다.** 한때 `$(closure_of …)`
# 로 불렸고, 그러면 안의 `die` 가 **서브셸만** 끝낸다. 게이트는 빈 폐포를 "아무 plugin 도
# 링크하지 않는다" 로 읽고 계속 가, 링크된 크레이트가 바뀌었는데 `통과` · rc 0 으로 끝났다
# (실패 문장은 stderr 에 한 줄 남지만 판정은 rc 가 한다). 그 서브셸은 빈 캐시 파일까지
# 남겨, 다음 호출은 비었는지 묻지도 않고 빈 폐포를 돌려줬다. 그래서 캐시는 성공한 뒤에만
# 제자리로 옮긴다.
#
# cargo 의 stderr 는 버리지 않고 받아 둔다 — 성공할 때는 경고라 안 찍고, 실패할 때는 그것이
# 판정 불가의 **사유**다. 버리면 "cargo tree 가 실패했다" 한 줄만 남아 다시 돌려야 안다.
# 시험: tests/plugin_version_bump_channel.rs 의 an_unreadable_dependency_closure_is_undecidable_and_says_why
closure_of() {
    local pname="$1" cache="$WORK/closure.$1"
    CLOSURE_FILE="$cache"
    CLOSURE_PATHS="$cache.paths"
    [ -f "$cache" ] && return 0
    if ! cargo tree -p "$pname" -e normal,build --prefix none --offline 2>"$cache.err" \
        > "$cache.raw"; then
        die "판정 불가: cargo tree 가 실패했다 ($pname). cargo 의 stderr:
$(cat "$cache.err")"
    fi
    awk '{print $1}' "$cache.raw" | sort -u > "$cache.tmp"
    [ -s "$cache.tmp" ] || die "판정 불가: $pname 의 의존 폐포가 비었다 — 빈 모수는 측정 실패다."
    # path 의존의 디렉토리(저장소 루트 기준 상대 경로). cargo tree 는 path 의존에만
    # `(/절대/경로)` 를 붙인다 — 레지스트리·git 의존은 이 모양으로 안 나온다. 저장소 밖
    # 경로와 루트 패키지 자신은 접두 치환에서 자연히 빠진다(git 이 담지 않거나, 루트는
    # `$ROOT_REAL/` 접두를 안 가진다).
    sed -n 's|^[^ ]* v[^ ]* (\(/[^)]*\)).*$|\1|p' "$cache.raw" \
        | sed -n "s|^${ROOT_REAL}/||p" | sort -u > "$cache.paths"
    mv "$cache.tmp" "$cache"
}

# 이 plugin 의 폐포에 그 크레이트가 있는가.
#
# **파이프로 쓰지 않는다.** `grep -q` 는 첫 일치에서 입력을 닫고, 그러면 왼쪽 단이
# SIGPIPE 로 죽는다 — `pipefail` 이 켜져 있으므로 파이프라인 rc 가 141 이 되어 **찾았는데
# 못 찾은 것이 된다.** 이 함수의 반환값은 판정 대상 집합을 정하므로, 뒤집히면 게이트가
# 조용히 반대로 판정한다. 그래서 캐시 파일을 직접 읽는다.
# 가드: crates/tasty-doc-guards/tests/no_early_exit_consumer_in_shell_pipes.rs
links() {  # <pname> <crate-name>
    closure_of "$1"
    grep -qxF "$2" "$CLOSURE_FILE"
}

# 공유 크레이트를 **디렉토리**로 묻는다. `$SCAN_ROOT` 아래는 디렉토리 이름이 곧 크레이트
# 이름이라 위 `links` 로 묻고, 워크스페이스 밖 path 의존(아래 EXTRA_ROOTS)은 디렉토리
# 이름과 크레이트 이름이 같다는 보장이 없어 폐포의 **경로**로 묻는다. 파이프를 안 쓰는
# 이유는 위 `links` 와 같다.
links_crate() {  # <pname> <crate-dir>
    if is_extra_root "$2"; then
        closure_of "$1"
        grep -qxF "$2" "$CLOSURE_PATHS"
    else
        links "$1" "$(basename "$2")"
    fi
}

# 사용: materialize <라벨> <tree-ish> <crate 경로>...
# 라벨 디렉토리에 `crates/<...>` 를 펼치고 출하 밖 줄을 지운 사본을 남긴다.
materialize() {
    local label="$1" tree="$2"; shift 2
    [ "$#" -gt 0 ] || return 0
    [ -n "$STRIP_BIN" ] || return 0
    local raw="$WORK/$label.raw" cooked="$WORK/$label.cooked"
    mkdir -p "$raw" "$cooked" || die "판정 불가: 작업 디렉토리를 못 만들었다."
    # 없는 경로가 섞이면 git archive 가 통째로 실패한다 — 있는 것만 넘긴다.
    local present=() p
    for p in "$@"; do
        if git cat-file -e "$tree:$p" 2>/dev/null; then present+=("$p"); fi
    done
    [ "${#present[@]}" -gt 0 ] || return 0
    git archive --format=tar "$tree" -- "${present[@]}" | tar -x -C "$raw" \
        || die "판정 불가: 트리를 펼치지 못했다 ($label)."
    # 판정기는 `.rs` 가 0 개인 뿌리를 판정 불가(exit 2)로 돌려준다 — `.rs` 를 가진
    # 뿌리만 넘긴다. 워크스페이스 밖 path 의존(EXTRA_ROOTS)도 같은 판정기를 거친다.
    local scan=() r
    # 이유: EXTRA_ROOTS 는 공백으로 이은 디렉토리 목록이다 — 단어 분리가 목적이다.
    # shellcheck disable=SC2086
    for r in "$SCAN_ROOT" $EXTRA_ROOTS; do
        [ -d "$raw/$r" ] || continue
        [ -n "$(find "$raw/$r" -name '*.rs' -print -quit)" ] && scan+=("$r")
    done
    [ "${#scan[@]}" -gt 0 ] || return 0
    "$STRIP_BIN" --blank-test-only-files "$cooked" "$raw" "${scan[@]}" >/dev/null \
        || die "판정 불가: 출하 판정기가 실패했다 ($label)."
}

# 이 게이트가 "version" 으로 읽는 줄 = 최상위 `version = "..."` 중 **첫 줄**.
#
# 술어를 여기 한 번만 둔다. 값을 **읽는** 쪽(version_at)과 그 줄을 내용 증거에서
# **빼는** 쪽(shipped_sum)이 다른 줄을 가리키면, 게이트가 자기가 판정하는 값을 증거로
# 도로 세거나 반대로 진짜 변경을 지운다 — 둘 다 조용하다.
VERSION_LINE_RE='^version[[:space:]]*=[[:space:]]*"'
version_line_no() { awk -v re="$VERSION_LINE_RE" '$0 ~ re { print NR; exit }'; }

# 출하 내용의 체크섬. 파일이 없으면 고정 토큰을 낸다 — "없음" 과 "빈 파일" 을 같게 보면
# 파일 추가·삭제를 무변경으로 오판한다.
#
# `.rs` 는 판정기를 거친 사본을 쓰고 rustfmt 로 포맷을 지운다. rustfmt 가 실패하면(옛
# edition 등) **그 앞 단계 내용을 그대로 쓴다** — 정규화 실패를 "같음" 으로 흘리지 않는다.
# 그 방향(오탐)이 조용한 통과보다 낫다. 마지막에 **빈 줄을 접는다**(위 헤더의 이유).
shipped_sum() {
    local label="$1" ref="$2" path="$3" out fmt cooked vline
    case "$path" in
        *.rs)
            # **출하가 0 이면 없는 것과 같다.** `.rs` 에서는 "파일이 없다" 와 "전부
            # 출하 밖이다" 가 산출물에 미치는 영향이 똑같이 0 이므로 같은 토큰을 낸다.
            # 안 그러면 테스트 전용 파일을 **새로 추가**하는 것이 "없음 → 빈 사본" 으로
            # 읽혀 발화한다(실측으로 밟았다). 진짜 파일을 지우는 쪽은 여전히 갈린다 —
            # 그쪽은 before 가 내용 있는 체크섬이다.
            if ! exists_at "$ref"; then printf '<no-ship>'; return; fi
            cooked="$WORK/$label.cooked/$path"
            if [ -f "$cooked" ]; then out=$(cat "$cooked"); else out=$(git show "$ref"); fi
            fmt=$(printf '%s' "$out" | rustfmt --edition "$RUST_EDITION" --emit stdout 2>/dev/null)
            [ -n "$fmt" ] && out="$fmt"
            out=$(printf '%s' "$out" | sed '/^[[:space:]]*$/d')
            if [ -z "$out" ]; then printf '<no-ship>'; return; fi
            ;;
        */Cargo.toml|*/tasty-plugin.toml)
            # **판정하는 값 자신을 증거로 쓰지 않는다.** version 줄이 내용에 남아 있으면
            # 그 줄 하나가 "산출물이 달라졌다" 를 만들고, 게이트는 값을 **내리는** 커밋에
            # 또 한 번의 bump 를 요구한다 — 올리면 되돌림이 아니라 순환이다. 값을 되
            # 정하는 것은 예외가 아니라 규칙이 정상으로 규정한 흐름이다(분할 착지 때
            # 병합하는 쪽이 최종 값을 정한다).
            #
            # 산출물이 이 값을 담는 것은 맞다(`CARGO_PKG_VERSION`). 그래서 더더욱 증거가
            # 될 수 없다 — 담기니까 늘 달라지고, 늘 다르면 아무것도 못 가른다.
            #
            # 빼는 줄은 version_at 이 **읽는 그 줄**이다(위 공용 술어). 나머지 줄
            # (feature·의존·bin 선언)은 그대로 본다.
            if ! exists_at "$ref"; then printf '<absent>'; return; fi
            out=$(git show "$ref")
            vline=$(printf '%s' "$out" | version_line_no)
            [ -n "$vline" ] && out=$(printf '%s' "$out" | sed "${vline}d")
            ;;
        *)
            # 소스가 아닌 것(`lang/*.toml`·`assets/*`)은 없음과 빈 파일을 가른다 —
            # 삭제를 무변경으로 오판하지 않기 위해서다.
            if ! exists_at "$ref"; then printf '<absent>'; return; fi
            out=$(git show "$ref")
            ;;
    esac
    printf '%s' "$out" | cksum
}

# [package] 절의 version. 매니페스트(tasty-plugin.toml)는 최상위 version 을 쓴다.
# 어느 줄인지는 위 공용 술어가 정한다 — shipped_sum 이 빼는 줄과 반드시 같은 줄이다.
version_at() {
    local ref="$1" text vline
    if ! exists_at "$ref"; then printf ''; return; fi
    text=$(git show "$ref")
    vline=$(printf '%s' "$text" | version_line_no)
    [ -n "$vline" ] || { printf ''; return; }
    printf '%s' "$text" | sed -n "${vline}s/${VERSION_LINE_RE}\([^\"]*\)\".*/\1/p"
}

# semver 를 정수 셋으로. 비교 가능한 형태가 아니면 빈 값.
ver_key() {
    printf '%s' "$1" | sed -n 's/^\([0-9][0-9]*\)\.\([0-9][0-9]*\)\.\([0-9][0-9]*\).*$/\1 \2 \3/p'
}

# a 가 b 보다 큰가.
ver_gt() {
    local ka kb; ka=$(ver_key "$1"); kb=$(ver_key "$2")
    [ -n "$ka" ] && [ -n "$kb" ] || return 1
    # 이유: 단어 분리가 목적이다. ver_key 가 세 마디를 공백으로 이어 돌려주고
    # 여기서 그것을 위치 인자 여섯으로 펼친다 — 따옴표를 치면 한 덩어리가 되어
    # 아래 자리 비교($1..$6)가 통째로 무너진다.
    # shellcheck disable=SC2086
    set -- $ka $kb
    [ "$1" -gt "$4" ] && return 0
    [ "$1" -lt "$4" ] && return 1
    [ "$2" -gt "$5" ] && return 0
    [ "$2" -lt "$5" ] && return 1
    [ "$3" -gt "$6" ]
}

# ── 좌변 (**한 값이다 — 소비처가 다섯이다**) ──────────────────────────
#
# 이 게이트가 훑는 뿌리. 전에는 같은 글자가 다섯 자리에 따로 적혀 있었다:
# 두 모드의 pathspec, 크레이트 뿌리를 뽑는 sed, 매니페스트 명부 glob, 그 명부가
# 만드는 상대 경로. 하나만 고치면 나머지는 옛 뿌리를 계속 보고, **그 어긋남은
# 조용하다** — 안 보는 자리가 늘 뿐 실패가 안 난다.
#
# 두 모드가 갈리는 것은 이 값이 아니라 **모수**다(무엇을 훑는가는 같고, 어느 두
# 끝점을 견주는가가 다르다). `--staged` 가 발행된 값을 못 보는 이유가 그것이고,
# 그것은 여기서 고칠 수 있는 종류가 아니다 — CLAUDE.md 의 "판정의 올바른 범위는
# 직전 push 지점 → 현재" 참조.
SCAN_ROOT=crates

# ── 변경된 파일 목록 ─────────────────────────────────────────
if [ "$MODE" = staged ]; then
    CHANGED=$(git diff --cached --name-only --diff-filter=ACMRD -- "$SCAN_ROOT/")
    OUTSIDE=$(git diff --cached --name-only --diff-filter=ACMRD -- . ":(exclude)$SCAN_ROOT/")
else
    CHANGED=$(git diff --name-only --diff-filter=ACMRD "$BEFORE" "$AFTER" -- "$SCAN_ROOT/")
    OUTSIDE=$(git diff --name-only --diff-filter=ACMRD "$BEFORE" "$AFTER" -- . ":(exclude)$SCAN_ROOT/")
fi

# 산출물에 닿는 경로인가. `.sig`·러너 스크립트, 그리고 아래 세 디렉토리 밖의 문서(.md)는 여기 없다 —
# `src/`·`lang/`·`assets/` 아래는 확장자와 무관하게 산출물로 본다(`crates/tasty-plugin-markdown/assets/NOTICE.md` 포함).
# ★ 이 디렉토리 모양 목록은 `.github/workflows/plugin-version-check.yml` 의 `paths` 가 복제한다
# (`.md` 를 빼고 되살리는 줄들). 여기에 모양을 더하면 그쪽도 더해야 필터가 판정 대상의
# 상위집합으로 남는다(ADR-0537).
build_affecting() {
    case "$1" in
        */src/*|*/lang/*|*/assets/*|*/Cargo.toml|*/tasty-plugin.toml|*/build.rs) return 0 ;;
        *) return 1 ;;
    esac
}

# ── 워크스페이스 밖 path 의존 (EXTRA_ROOTS) ──────────────────────────
#
# `[patch]` 로 끼운 상류 사본(`vendor/tiny_http`)처럼 워크스페이스 `exclude` 에 있으면서
# plugin 이 링크하는 path 의존은 `$SCAN_ROOT` 밖에 산다. 좌변을 `$SCAN_ROOT` 로만 두면
# 그 사본만 고친 커밋이 세 채널(P.1 · B.9 · CI) 모두에서 판정 대상 0 건으로 통과한다 —
# plugin 산출물은 달라졌는데. 근거·대안·재검토 조건:
# docs/adr/0537-the-plugin-version-gate-follows-path-dependencies-outside-the-workspace.md
#
# 좌변은 **plugin 폐포에 실제로 든 path 의존** 중 워크스페이스 멤버가 아니고 `$SCAN_ROOT`
# 밖인 디렉토리다. 멤버를 빼는 이유: `$SCAN_ROOT` 밖의 멤버는 이 게이트의 좌변이 아니다 —
# 좌변을 옮기는 것은 `SCAN_ROOT` 한 값의 일이다. 폐포에 안 든 사본(아무 plugin 도
# 링크하지 않는 것)은 여기 안 들어오므로 bump 를 요구하지 않는다.
#
# 값은 `$SCAN_ROOT` 밖에서 산출물 경로 모양의 변경이 **있을 때만** 구한다(plugin 마다
# cargo tree 한 번, 폐포에 `$SCAN_ROOT` 밖 경로가 나오면 cargo metadata 한 번). 없으면
# cargo 를 안 부른다.
#
# 경로는 cargo 가 찍는 정규 경로를 `pwd -P` 와 견준다 — 심볼릭 링크로 들어온 트리에서도
# 같은 문자열이 되도록.
ROOT_REAL=$(pwd -P)
EXTRA_ROOTS=""
is_extra_root() {
    case " $EXTRA_ROOTS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}
OUTSIDE_BUILD=""
while IFS= read -r f; do
    [ -z "$f" ] && continue
    if build_affecting "$f"; then OUTSIDE_BUILD="$OUTSIDE_BUILD
$f"; fi
done <<EOF
$OUTSIDE
EOF
# 멤버 명부는 후보(폐포 안의 `$SCAN_ROOT` 밖 경로)가 처음 나올 때 한 번 구한다.
MEMBERS=""; MEMBERS_READ=0
read_members() {
    [ "$MEMBERS_READ" = 1 ] && return 0
    MEMBERS=$(cargo metadata --no-deps --offline --format-version 1 2>"$WORK/metadata.err") \
        || die "판정 불가: cargo metadata 가 실패했다. cargo 의 stderr:
$(cat "$WORK/metadata.err")"
    MEMBERS=$(printf '%s' "$MEMBERS" | grep -o '"manifest_path":"[^"]*"' \
        | sed -n "s|^\"manifest_path\":\"${ROOT_REAL}/\(.*\)/Cargo.toml\"\$|\1|p")
    MEMBERS_READ=1
}
if [ -n "$OUTSIDE_BUILD" ]; then
    for man in "$ROOT/$SCAN_ROOT"/*/tasty-plugin.toml; do
        [ -f "$man" ] || continue
        closure_of "$(basename "$(dirname "$man")")"
        while IFS= read -r p; do
            [ -z "$p" ] && continue
            case "$p" in "$SCAN_ROOT"/*) continue ;; esac
            read_members
            case "
$MEMBERS
" in *"
$p
"*) continue ;; esac
            is_extra_root "$p" || EXTRA_ROOTS="$EXTRA_ROOTS $p"
        done < "$CLOSURE_PATHS"
    done
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        for r in $EXTRA_ROOTS; do
            case "$f" in "$r"/*) CHANGED="$CHANGED
$f"; break ;; esac
        done
    done <<EOF
$OUTSIDE_BUILD
EOF
fi

# 변경 파일이 속한 크레이트 디렉토리.
crate_dir_of() {
    local r
    for r in $EXTRA_ROOTS; do
        case "$1" in "$r"/*) printf '%s' "$r"; return ;; esac
    done
    printf '%s' "$1" | sed -n "s|^\\(${SCAN_ROOT}/[^/]*\\)/.*\$|\\1|p"
}

VIOLATIONS=0
CONSIDERED=0
# 현재 쪽 값이 BEFORE 쪽보다 **낮은** 항. 위반과 따로 센다 — 그 둘은 다른 물음이다.
BEHIND=0

# 변경에 걸린 크레이트(산출물 경로만). 트리 전체를 훑지 않으므로 규모와 무관하다.
CHANGED_CRATES=""
while IFS= read -r f; do
    [ -z "$f" ] && continue
    build_affecting "$f" || continue
    CHANGED_CRATES="$CHANGED_CRATES
$(crate_dir_of "$f")"
done <<EOF
$CHANGED
EOF
CHANGED_CRATES=$(printf '%s\n' "$CHANGED_CRATES" | sed -n '/./p' | sort -u)

# 매니페스트를 가진 것 = 번들 plugin. 나머지가 **공유 크레이트**이고, 그것이 이 게이트가
# 여태 못 보던 자리다.
SHARED_CHANGED=""
for c in $CHANGED_CRATES; do
    [ -f "$ROOT/$c/tasty-plugin.toml" ] || SHARED_CHANGED="$SHARED_CHANGED $c"
done

# plugin 후보 ① 변경된 크레이트 중 매니페스트를 가진 것
PLUGINS=""
for c in $CHANGED_CRATES; do
    [ -f "$ROOT/$c/tasty-plugin.toml" ] && PLUGINS="$PLUGINS $c"
done

# plugin 후보 ② 바뀐 공유 크레이트를 폐포에 포함하는 번들 plugin.
#
# **`cargo tree` 는 작업 트리의 의존 그래프를 읽는다** — `--range` 로 과거 구간을 볼 때도
# 그렇다. CI 는 after 쪽을 체크아웃한 상태로 부르므로 그 자리에서는 after 그래프가 맞고,
# 로컬 `--staged` 도 마찬가지다. 의존 자체가 바뀐 구간을 소급해서 볼 때만 어긋나는데,
# 그 방향은 **더 넓게 보는 쪽**이라 조용한 통과를 만들지 않는다.
if [ -n "${SHARED_CHANGED// /}" ]; then
    for man in "$ROOT/$SCAN_ROOT"/*/tasty-plugin.toml; do
        [ -f "$man" ] || continue
        pdir=$(dirname "$man"); pname=$(basename "$pdir"); prel="$SCAN_ROOT/$pname"
        case " $PLUGINS " in *" $prel "*) continue ;; esac
        for c in $SHARED_CHANGED; do
            if links_crate "$pname" "$c"; then PLUGINS="$PLUGINS $prel"; break; fi
        done
    done
fi
PLUGINS=$(printf '%s\n' $PLUGINS | sed -n '/./p' | sort -u)

# 양끝 트리를 한 번씩만 펼친다.
INVOLVED=$(printf '%s\n' $CHANGED_CRATES | sed -n '/./p' | sort -u)
if [ "$MODE" = staged ]; then
    AFTER_TREE=$(git write-tree) || die "판정 불가: 인덱스 트리를 못 만들었다."
else
    AFTER_TREE="$AFTER"
fi
# 이유: 아래 둘 다 단어 분리가 목적이다. INVOLVED 는 줄바꿈으로 이은 크레이트
# 이름 목록이고 materialize 는 그것을 가변 인자로 받는다 — 따옴표를 치면 이름
# 전부가 한 인자가 되어 그런 이름의 크레이트를 찾다가 빈손으로 끝난다.
# shellcheck disable=SC2086
materialize before "$BEFORE_REV" $INVOLVED
# shellcheck disable=SC2086
materialize after "$AFTER_TREE" $INVOLVED

for base in $PLUGINS; do
    man="$base/tasty-plugin.toml"
    # bin plugin 인가 — after 쪽에 매니페스트가 있어야 한다. 라이브러리 크레이트
    # (protocol/sdk 등)는 매니페스트가 없어 여기서 자연히 빠진다.
    exists_at "$(after_ref "$man")" || continue
    # before 에 매니페스트가 없으면 새로 추가된 plugin 이다 — 올릴 이전 값이 없다.
    exists_at "$BEFORE_REV:$man" || continue

    # 이 plugin 의 산출물에 닿는 변경 = 자기 디렉토리 + 워크스페이스 내부 의존 폐포.
    scope="$base"
    if [ -n "${SHARED_CHANGED// /}" ]; then
        for c in $SHARED_CHANGED; do
            if links_crate "$(basename "$base")" "$c"; then scope="$scope $c"; fi
        done
    fi
    files=""
    for d in $scope; do
        files="$files
$(printf '%s\n' "$CHANGED" | sed -n "s|^\($d/.*\)$|\1|p")"
    done
    files=$(printf '%s\n' "$files" | sed -n '/./p' | sort -u)
    content_changed=0
    changed_list=""
    for f in $files; do
        build_affecting "$f" || continue
        a=$(shipped_sum before "$BEFORE_REV:$f" "$f")
        b=$(shipped_sum after "$(after_ref "$f")" "$f")
        if [ "$a" != "$b" ]; then
            content_changed=1
            changed_list="$changed_list
      $f"
        fi
    done
    [ "$content_changed" = 1 ] || continue

    CONSIDERED=$((CONSIDERED + 1))
    vb=$(version_at "$BEFORE_REV:$man")
    va=$(version_at "$(after_ref "$man")")
    if [ -z "$vb" ] || [ -z "$va" ]; then
        printf '✘ [plugin-version] %s: 매니페스트에서 version 을 못 읽었다 (before=%s after=%s)\n' \
            "$base" "${vb:-<없음>}" "${va:-<없음>}" >&2
        VIOLATIONS=$((VIOLATIONS + 1))
        continue
    fi
    # ── 값이 **내려가는** 항을 먼저 가른다 ────────────────────────────────────
    # `vb > va` 면 BEFORE 쪽이 더 새것이다. 그건 "안 올렸다" 가 아니라 **이 판정의 범위가
    # 뒤집힌 것**이고, 원인이 둘이라 이 게이트가 어느 쪽인지 못 고른다 — 그래서 판정 불가다.
    # 비교는 형제 술어 `ver_gt` 를 그대로 부른다(같은 물음에 답을 둘로 만들지 않는다).
    # ★ 값이 읽히지 않는 경우 `ver_gt` 는 양방향 모두 거짓이라 아래 위반 갈래로 떨어진다 —
    #   더 시끄러운 쪽이라 조용한 통과가 안 된다.
    if ver_gt "$vb" "$va"; then
        printf '▲ [plugin-version] %s: 이 범위의 **앞 끝이 뒤 끝보다 새것**이다 (%s → %s)\n' \
            "$base" "$vb" "$va" >&2
        printf '    원인이 둘이고 이 게이트는 어느 쪽인지 못 고른다:\n' >&2
        printf '      (가) **네가 뒤처졌다** — 비교 대상(%s)이 네 base 보다 앞서 있다.\n' \
            "$BEFORE_REV" >&2
        printf '           조립이 도는 동안 `main` 같은 공유 ref 를 범위 끝으로 주면 이렇게 된다.\n' >&2
        printf '           고치는 법: base 를 맞춰라. 회차 중이면 회차 base 를 **손으로 박아라** —\n' >&2
        printf '             %s --range <회차 base> HEAD\n' \
            "scripts/check-plugin-version-bump.sh" >&2
        printf '      (나) **의도적으로 내렸다** — 그건 이 게이트가 판정할 일이 아니다.\n' >&2
        printf '    ★ 어느 쪽이든 **bump 로 풀지 마라.** 여기서 값을 올리면 남이 이미 발행한\n' >&2
        printf '      값을 덮는다 — 이 게이트가 막으려는 바로 그 상태를 만든다.\n' >&2
        BEHIND=$((BEHIND + 1))
    elif ! ver_gt "$va" "$vb"; then
        printf '✘ [plugin-version] %s: 산출물이 달라지는데 version 이 안 올랐다 (%s → %s)\n' \
            "$base" "$vb" "$va" >&2
        printf '    내용이 바뀐 파일:%s\n' "$changed_list" >&2
        printf '    고쳐라: %s 와 %s 의 version 을 **같은 값**으로 patch +1.\n' \
            "$base/Cargo.toml" "$man" >&2
        if [ "$MODE" = staged ]; then
            printf '    [모수] 이 판정이 본 범위는 **이 커밋 하나**(index vs %s)다. 발행이\n' \
                "$BEFORE_REV" >&2
            printf '           묻는 범위는 다르다 — **직전 push 지점부터 지금까지**다.\n' >&2
            printf '           그래서 lane 이 여러 커밋으로 나눠 착지하면 이 검사는 앞 커밋이\n' >&2
            printf '           이미 올린 값에 대해 **또 한 번의 bump 를 요구한다**(값이 부푼다).\n' >&2
            printf '           그때 옳은 처리는 둘 중 하나다:\n' >&2
            printf '             (가) 이 변경을 그 앞 bump 커밋에 합친다(같은 lane 안이면 보통 이쪽).\n' >&2
            printf '             (나) 여기서 한 번 더 올리고 **최종 값은 병합하는 쪽이 정한다**고 보고한다.\n' >&2
            printf '           발행 기준으로 다시 재는 명령: %s --range <직전 push> HEAD\n' \
                "scripts/check-plugin-version-bump.sh" >&2
            printf '           ★ 두 모수는 둘 다 필요하다 — 이 검사는 **커밋 전에** 답할 수\n' >&2
            printf '             있는 유일한 채널이고, 발행 판정은 push 지점을 아는 쪽이 한다\n' >&2
            printf '             (pre-push 훅의 B.9 가 원격 tip 을 모수로 같은 판정을 한 번 더 한다).\n' >&2
        fi
        VIOLATIONS=$((VIOLATIONS + 1))
    fi
done

# 판정 대상이 **전부** 걸렸으면 그 사실을 말한다. 진짜 누락은 부분집합이다 — 폐포를 건드린
# 것 중 **일부**만 값이 안 따라온 것이니까. 전량은 lane 하나가 만들 수 있는 모양이 아니라
# 보통 **범위가 틀린 것**이다. 이 수 둘은 원래 같은 줄에 찍히고 있었고 읽는 규칙만 없었다.
# 대상이 1 건이면 100% 가 정상이라 세지 않는다.
flagged=$((VIOLATIONS + BEHIND))
if [ "$CONSIDERED" -ge 2 ] && [ "$flagged" -eq "$CONSIDERED" ]; then
    printf '[plugin-version] ★ 판정 대상 %d 건이 **전부** 걸렸다 — 한 lane 이 만들 수 있는 모양이\n' \
        "$CONSIDERED" >&2
    printf '    아니다. 값을 고치기 전에 **범위**를 의심해라(BEFORE=%s).\n' "$BEFORE_REV" >&2
fi

if [ "$VIOLATIONS" -gt 0 ]; then
    # 진짜 위반이 하나라도 있으면 그쪽이 이긴다 — 뒤처짐 때문에 누락이 판정 불가로 가려지면 안 된다.
    printf '[plugin-version] 위반 %d 건 / 판정 대상 %d 건' "$VIOLATIONS" "$CONSIDERED" >&2
    [ "$BEHIND" -gt 0 ] && printf ' (그 밖에 범위가 뒤집힌 항 %d 건)' "$BEHIND" >&2
    printf '\n' >&2
    exit 1
fi
if [ "$BEHIND" -gt 0 ]; then
    # 위반은 0 인데 뒤집힌 항만 있다 — 이 게이트는 **판정을 못 한 것**이지 통과가 아니다.
    # 위반(1)과 갈리도록 2 로 낸다. 비영이라 train·pre-commit 은 그대로 막힌다.
    printf '[plugin-version] 판정 불가 — 범위가 뒤집힌 항 %d 건 / 판정 대상 %d 건.\n' \
        "$BEHIND" "$CONSIDERED" >&2
    printf '    측정이 안 됐으므로 게이트를 통과로 읽지 않는다.\n' >&2
    exit 2
fi
# 워크스페이스 밖 path 의존(EXTRA_ROOTS)의 파일은 따로 센다 — "crates 파일" 이라는 이름
# 아래 섞으면 그 수가 무엇을 셌는지 안 보인다. 0 이면 꼬리를 안 붙여 기존 줄 형태를 지킨다.
n_all=$(printf '%s\n' "$CHANGED" | sed -n '/./p' | wc -l)
n_extra=$(printf '%s\n' "$CHANGED" | sed -n "/^${SCAN_ROOT}\//!{/./p}" | wc -l)
if [ "$n_extra" -gt 0 ]; then
    printf '[plugin-version] 통과 — 판정 대상 %d 건 (변경된 crates 파일 %d 개 · 워크스페이스 밖 path 의존 파일 %d 개 중)\n' \
        "$CONSIDERED" "$((n_all - n_extra))" "$n_extra"
else
    printf '[plugin-version] 통과 — 판정 대상 %d 건 (변경된 crates 파일 %d 개 중)\n' \
        "$CONSIDERED" "$n_all"
fi
exit 0
