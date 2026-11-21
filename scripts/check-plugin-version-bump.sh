#!/usr/bin/env bash
# plugin 및 연결된 저장소 내 path 의존성의 내용을 비교해 버전 증가가 필요한지 판정한다.
# Rust는 test 전용 부분을 제외하고 rustfmt로 정규화한다. 주석 변경도 내용 차이로 볼 수 있다.
# --staged [--base rev]는 index와 base(기본 HEAD), --range before after는 두 커밋을 비교한다.
# --violations-out <파일>은 버전을 올려야 하는 플러그인 디렉터리를 한 줄에 하나씩 그 파일에 쓴다.
# 실제 바이너리를 빌드해 비교하는 검사는 아니다. 종료 코드: 통과 0, 위반 1, 판정 불가 2.

set -uo pipefail

die() { printf '%s\n' "$*" >&2; exit 2; }

git rev-parse --git-dir >/dev/null 2>&1 || die "판정 불가: git 저장소가 아니다 (배포 tarball 등)."
ROOT="$(git rev-parse --show-toplevel)" || die "판정 불가: 저장소 루트를 못 찾았다."
CALLER_DIR="$(pwd)"
cd "$ROOT" || die "판정 불가: 저장소 루트로 이동 실패."

command -v rustfmt >/dev/null 2>&1 \
    || die "판정 불가: rustfmt 가 없다 — 포맷 변경과 실변경을 가를 수 없다. rustup component add rustfmt"

# 합성 저장소에서도 사용할 수 있도록 스크립트 위치의 공용 도구 탐색 함수를 읽는다. 여기서 Cargo 빌드는 하지 않는다.
. "$(cd "$(dirname "$0")" && pwd)/lib/judge-bin.sh"
resolve_judge strip-cfg-test TASTY_STRIP_CFG_TEST_BIN "$ROOT"
STRIP_BIN="$JUDGE_BIN"
# 도구가 없거나 낡으면 test 전용 변경도 비교한다. 불필요한 버전 증가를 요구할 수 있다.
if [ -z "$STRIP_BIN" ]; then
    echo "[plugin-version] 출하 범위를 못 좁힌다 — 테스트 전용 변경까지 bump 를 요구한다." >&2
fi

RUST_EDITION=$(sed -n 's/^edition[[:space:]]*=[[:space:]]*"\([0-9]*\)".*/\1/p' Cargo.toml | sed -n '1p')
[ -n "$RUST_EDITION" ] || die "판정 불가: 루트 Cargo.toml 에서 edition 을 못 읽었다."

MODE=""; BASE="HEAD"; BEFORE=""; AFTER=""; VIOLATIONS_OUT=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --staged) MODE=staged; shift ;;
        --base)   BASE="${2:-}"; [ -n "$BASE" ] || die "판정 불가: --base 에 rev 가 없다."; shift 2 ;;
        --range)  MODE=range; BEFORE="${2:-}"; AFTER="${3:-}"
                  [ -n "$BEFORE" ] && [ -n "$AFTER" ] || die "판정 불가: --range 는 두 rev 를 요구한다."
                  shift 3 ;;
        --violations-out) VIOLATIONS_OUT="${2:-}"
                  [ -n "$VIOLATIONS_OUT" ] || die "판정 불가: --violations-out 에 파일 경로가 없다."
                  shift 2 ;;
        *) die "판정 불가: 알 수 없는 인자 '$1'" ;;
    esac
done
[ -n "$MODE" ] || die "판정 불가: --staged 또는 --range 중 하나를 줘야 한다."
# 저장소 루트로 이동하기 전에 받은 상대 경로도 같은 파일을 가리키도록 호출 위치 기준으로 고정한다.
if [ -n "$VIOLATIONS_OUT" ]; then
    case "$VIOLATIONS_OUT" in /*) ;; *) VIOLATIONS_OUT="$CALLER_DIR/$VIOLATIONS_OUT" ;; esac
    : > "$VIOLATIONS_OUT" || die "판정 불가: --violations-out 파일을 쓸 수 없다: $VIOLATIONS_OUT"
fi

if [ "$MODE" = staged ]; then
    if ! git rev-parse --verify --quiet "$BASE^{commit}" >/dev/null; then
        # HEAD가 아직 없는 첫 커밋만 비교 기준이 없는 정상 경우다. 그 밖의 rev는 오타일 수 있다.
        if [ "$BASE" = HEAD ] && ! git rev-parse --verify --quiet HEAD >/dev/null; then
            echo "[plugin-version] skip: 비교할 커밋이 없다(첫 커밋). 판정 대상 0."
            exit 0
        fi
        die "판정 불가: --base rev 를 못 찾았다: $BASE"
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

after_ref() { printf '%s:%s' "$AFTER_PREFIX" "$1"; }

exists_at() { git cat-file -e "$1" 2>/dev/null; }

# 외부 test 모듈 선언도 읽어야 하므로 관련 crate를 통째로 꺼내 사본을 만든다.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# normal/build 의존성 목록을 캐시한다. die가 상위 셸을 끝내도록 명령 치환 없이 호출한다.
# 빈·실패 결과를 캐시하지 않으며 오류 때 Cargo stderr도 표시한다.
closure_of() {
    local pname="$1" cache="$WORK/closure.$1"
    CLOSURE_FILE="$cache"
    CLOSURE_PATHS="$cache.paths"
    [ -f "$cache" ] && return 0
    # 플러그인은 여러 OS로 출하하므로 다른 target 전용 의존도 폐포에 넣는다.
    if ! cargo tree -p "$pname" -e normal,build --target all --prefix none --offline \
        2>"$cache.err" > "$cache.raw"; then
        die "판정 불가: cargo tree 가 실패했다 ($pname). 다른 target 의 크레이트를 받지 않았으면
    cargo fetch --locked 를 먼저 실행한다. cargo 의 stderr:
$(cat "$cache.err")"
    fi
    awk '{print $1}' "$cache.raw" | sort -u > "$cache.tmp"
    [ -s "$cache.tmp" ] || die "판정 불가: $pname 의 의존성 목록이 비어 비교 범위를 정할 수 없다."
    # 저장소 아래의 path 의존 경로만 추출한다. 루트 패키지와 저장소 밖 경로는 제외된다.
    sed -n 's|^[^ ]* v[^ ]* (\(/[^)]*\)).*$|\1|p' "$cache.raw" \
        | sed -n "s|^${ROOT_REAL}/||p" | sort -u > "$cache.paths"
    mv "$cache.tmp" "$cache"
}

# grep -q에 producer를 직접 연결하면 SIGPIPE가 성공 판정을 뒤집을 수 있어 캐시 파일을 읽는다.
links() {  # <pname> <crate-name>
    closure_of "$1"
    grep -qxF "$2" "$CLOSURE_FILE"
}

# workspace 밖 path 의존은 디렉터리명과 crate 이름이 다를 수 있어 경로로 비교한다.
links_crate() {  # <pname> <crate-dir>
    if is_extra_root "$2"; then
        closure_of "$1"
        grep -qxF "$2" "$CLOSURE_PATHS"
    else
        links "$1" "$(basename "$2")"
    fi
}

# Git 트리에서 관련 crate를 꺼내 test 전용 부분을 지운 사본을 만든다.
materialize() {
    local label="$1" tree="$2"; shift 2
    [ "$#" -gt 0 ] || return 0
    [ -n "$STRIP_BIN" ] || return 0
    local raw="$WORK/$label.raw" cooked="$WORK/$label.cooked"
    mkdir -p "$raw" "$cooked" || die "판정 불가: 작업 디렉토리를 못 만들었다."
    # 없는 경로를 git archive에 주면 전체가 실패하므로 존재하는 경로만 넘긴다.
    local present=() p
    for p in "$@"; do
        if git cat-file -e "$tree:$p" 2>/dev/null; then present+=("$p"); fi
    done
    [ "${#present[@]}" -gt 0 ] || return 0
    git archive --format=tar "$tree" -- "${present[@]}" | tar -x -C "$raw" \
        || die "판정 불가: 트리를 펼치지 못했다 ($label)."
    # Rust 파일이 없는 디렉터리는 사본 도구에 넘기지 않는다.
    local scan=() r
    # 이유: EXTRA_ROOTS는 공백으로 구분한 목록이므로 단어 분리가 필요하다.
    # shellcheck disable=SC2086
    for r in "$SCAN_ROOT" $EXTRA_ROOTS; do
        [ -d "$raw/$r" ] || continue
        [ -n "$(find "$raw/$r" -name '*.rs' -print -quit)" ] && scan+=("$r")
    done
    [ "${#scan[@]}" -gt 0 ] || return 0
    "$STRIP_BIN" --blank-test-only-files "$cooked" "$raw" "${scan[@]}" >/dev/null \
        || die "판정 불가: test 전용 코드 제외 도구가 실패했다 ($label)."
}

# 처음 나타나는 version 줄을 읽고 내용 비교에서도 같은 줄을 제외한다. TOML 절 구조는 파싱하지 않는다.
VERSION_LINE_RE='^version[[:space:]]*=[[:space:]]*"'
version_line_no() { awk -v re="$VERSION_LINE_RE" '$0 ~ re { print NR; exit }'; }

# Rust는 사본·rustfmt 결과의 빈 줄을 제외하고 비교한다. 도구 결과가 없으면 앞 단계 내용을 사용한다.
shipped_sum() {
    local label="$1" ref="$2" path="$3" out fmt cooked vline
    case "$path" in
        *.rs)
            # 없는 Rust 파일과 test 전용이라 내용이 사라진 사본은 같은 값으로 본다.
            if ! exists_at "$ref"; then printf '<no-ship>'; return; fi
            cooked="$WORK/$label.cooked/$path"
            if [ -f "$cooked" ]; then out=$(cat "$cooked"); else out=$(git show "$ref"); fi
            fmt=$(printf '%s' "$out" | rustfmt --edition "$RUST_EDITION" --emit stdout 2>/dev/null)
            [ -n "$fmt" ] && out="$fmt"
            out=$(printf '%s' "$out" | sed '/^[[:space:]]*$/d')
            if [ -z "$out" ]; then printf '<no-ship>'; return; fi
            ;;
        */Cargo.toml|*/tasty-plugin.toml)
            # 버전 줄 자체가 새 버전 증가를 다시 요구하지 않도록 내용 증거에서 제외한다.
            if ! exists_at "$ref"; then printf '<absent>'; return; fi
            out=$(git show "$ref")
            vline=$(printf '%s' "$out" | version_line_no)
            [ -n "$vline" ] && out=$(printf '%s' "$out" | sed "${vline}d")
            ;;
        *)
            # 자산은 없음과 빈 파일을 구별한다.
            if ! exists_at "$ref"; then printf '<absent>'; return; fi
            out=$(git show "$ref")
            ;;
    esac
    printf '%s' "$out" | cksum
}

# 위 공용 조건이 고른 첫 version 줄을 읽는다.
version_at() {
    local ref="$1" text vline
    if ! exists_at "$ref"; then printf ''; return; fi
    text=$(git show "$ref")
    vline=$(printf '%s' "$text" | version_line_no)
    [ -n "$vline" ] || { printf ''; return; }
    printf '%s' "$text" | sed -n "${vline}s/${VERSION_LINE_RE}\([^\"]*\)\".*/\1/p"
}

# 숫자 세 부분을 읽는다. 뒤의 pre-release/build 표지는 비교에 쓰지 않는다.
ver_key() {
    printf '%s' "$1" | sed -n 's/^\([0-9][0-9]*\)\.\([0-9][0-9]*\)\.\([0-9][0-9]*\).*$/\1 \2 \3/p'
}

ver_gt() {
    local ka kb; ka=$(ver_key "$1"); kb=$(ver_key "$2")
    [ -n "$ka" ] && [ -n "$kb" ] || return 1
    # 이유: 버전의 정수 여섯 개를 위치 인자로 나누기 위해 단어 분리를 사용한다.
    # shellcheck disable=SC2086
    set -- $ka $kb
    [ "$1" -gt "$4" ] && return 0
    [ "$1" -lt "$4" ] && return 1
    [ "$2" -gt "$5" ] && return 0
    [ "$2" -lt "$5" ] && return 1
    [ "$3" -gt "$6" ]
}

# 변경 경로와 plugin 명부의 기준 디렉터리.
SCAN_ROOT=crates

if [ "$MODE" = staged ]; then
    # 비교 기준과 같은 BASE↔index 차이다. base 뒤 앞선 커밋의 변경도 여기에 들어온다.
    CHANGED=$(git diff --cached --name-only --diff-filter=ACMRD "$BASE" -- "$SCAN_ROOT/")
    OUTSIDE=$(git diff --cached --name-only --diff-filter=ACMRD "$BASE" -- . ":(exclude)$SCAN_ROOT/")
else
    CHANGED=$(git diff --name-only --diff-filter=ACMRD "$BEFORE" "$AFTER" -- "$SCAN_ROOT/")
    OUTSIDE=$(git diff --name-only --diff-filter=ACMRD "$BEFORE" "$AFTER" -- . ":(exclude)$SCAN_ROOT/")
fi

# src/lang/assets는 확장자에 관계없이 비교한다. 이 범위를 바꾸면 CI 경로 필터도 함께 대조한다.
build_affecting() {
    case "$1" in
        */src/*|*/lang/*|*/assets/*|*/Cargo.toml|*/tasty-plugin.toml|*/build.rs) return 0 ;;
        *) return 1 ;;
    esac
}

# SCAN_ROOT 밖의 변경이 있으면 연결된 path 의존성도 조사한다.
# workspace 멤버가 아닌 저장소 내부 경로만 추가하며 현재 작업 트리의 Cargo 그래프를 사용한다.
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
# 공식 플러그인은 tasty-official-plugins 레포가 버전을 관리하고 tasty는 잠금 파일의 릴리스 산출물만 싣는다.
# 잠금 파일 id와 같은 매니페스트를 가진 크레이트는 판정 대상에서 뺀다. 둘 다 비교 대상 트리(index 또는 AFTER)에서 읽는다.
SCAN_AWK="$(cd "$(dirname "$0")" && pwd)/lib/toml-scan.awk"
LOCK_FILE=official-plugins.lock.toml
LOCKED_DIRS=""
# toml-scan.awk 출력에서 잠금 파일 [[plugin]] 표의 id를 뽑는다. 배포 도구가 받는 꼴(맨 키, 기본 문자열)만 받는다.
read_lock_ids() {  # <scan 파일>
    awk -F'\t' '
        function fail(m) { if (err == "") err = m }
        function close_table() { if (inp && ids != 1) fail("[[plugin]] 표의 id 가 " ids " 개다") }
        $1 == "H" {
            close_table()
            # generate 가 쓰는 세 표의 맨 이름만 받는다. 다른 표 머리는 id 를 담은 표를 놓칠 수 있다.
            if ($3 != "bare" || ($2 != "[release]" && $2 != "[[plugin]]" && $2 != "[[plugin.artifact]]")) fail("잠금 파일에 모르는 표가 있다: " $2)
            inp = ($2 == "[[plugin]]"); if (inp) { tables++; ids = 0 }
            # 배포 파서는 [[plugin.artifact]] 가 앞선 [[plugin]] 에 붙는다고 읽는다.
            if ($2 == "[[plugin.artifact]]" && tables == 0) fail("[[plugin.artifact]] 앞에 [[plugin]] 표가 없다")
            next
        }
        $1 == "K" && inp {
            if ($2 == "unknown") { fail("[[plugin]] 표에 읽을 수 없는 키가 있다"); next }
            if ($3 != "id") next
            ids++
            if ($2 != "bare" || $4 != "basic") { fail("id 가 맨 키·기본 문자열이 아니다"); next }
            if ($5 !~ /^com[.]tasty[.][a-z0-9-]+$/) { fail("id 가 공식 플러그인 id 꼴이 아니다: " $5); next }
            if (seen[$5]++) { fail("id 가 중복됐다: " $5); next }
            print $5
        }
        END {
            close_table()
            if (tables == 0) fail("[[plugin]] 표가 없다")
            if (err != "") { print "E\t" err; exit 3 }
        }' "$1"
}
# 매니페스트 최상위(첫 표 머리 앞)의 id 하나를 뽑는다. 확신할 수 없는 표기는 판정 불가다.
read_manifest_id() {  # <scan 파일>
    awk -F'\t' '
        function fail(m) { if (err == "") err = m }
        $1 == "H" { top_done = 1; next }
        $1 == "K" && !top_done {
            if ($2 == "unknown") { fail("최상위에 읽을 수 없는 키가 있다"); next }
            if ($3 != "id") next
            n++
            if ($4 != "basic" && $4 != "literal") { fail("id 가 단순 문자열이 아니다"); next }
            id = $5
        }
        END {
            if (err == "" && n != 1) fail("최상위 id 가 " n + 0 " 개다")
            if (err != "") { print "E\t" err; exit 3 }
            print id
        }' "$1"
}
# 비교 대상 트리(index 또는 AFTER)에서 pathspec 아래 경로를 $WORK/tree.paths 에 나열한다.
# 없는 경로는 빈 목록이고, 조회 자체가 실패하면 판정 불가다.
target_tree_paths() {  # <pathspec>
    local rc
    if [ "$MODE" = staged ]; then
        git ls-files -- "$1" > "$WORK/tree.paths" 2> "$WORK/tree.err"
        rc=$?
    else
        git ls-tree -r --name-only "$AFTER" -- "$1" > "$WORK/tree.paths" 2> "$WORK/tree.err"
        rc=$?
    fi
    [ "$rc" -eq 0 ] || die "판정 불가: 비교 대상 트리를 나열하지 못했다 ($1): $(cat "$WORK/tree.err")"
}
compute_locked_dirs() {
    local out ids rel mid
    target_tree_paths "$LOCK_FILE"
    grep -qxF "$LOCK_FILE" "$WORK/tree.paths" || return 0
    [ -f "$SCAN_AWK" ] || die "판정 불가: TOML 판독기가 없다: $SCAN_AWK"
    git show "$(after_ref "$LOCK_FILE")" > "$WORK/lock.toml" \
        || die "판정 불가: 잠금 파일을 꺼내지 못했다: $(after_ref "$LOCK_FILE")"
    awk -f "$SCAN_AWK" "$WORK/lock.toml" > "$WORK/lock.scan" \
        || die "판정 불가: 잠금 파일을 읽지 못했다: $(sed -n '$p' "$WORK/lock.scan")"
    out=$(read_lock_ids "$WORK/lock.scan") \
        || die "판정 불가: 잠금 파일 $LOCK_FILE: $(printf '%s\n' "$out" | sed -n '$p' | cut -f2)"
    ids=" $(printf '%s\n' "$out" | tr '\n' ' ')"
    # 매니페스트 목록도 비교 대상 트리에서 읽는다. 작업 트리에서만 지운 매니페스트도 분류한다.
    target_tree_paths "$SCAN_ROOT"
    grep -E "^${SCAN_ROOT}/[^/]+/tasty-plugin[.]toml\$" "$WORK/tree.paths" > "$WORK/manifests.paths"
    while IFS= read -r man; do
        [ -n "$man" ] || continue
        rel="${man%/tasty-plugin.toml}"
        git show "$(after_ref "$man")" > "$WORK/manifest.toml" \
            || die "판정 불가: 매니페스트를 꺼내지 못했다: $man"
        awk -f "$SCAN_AWK" "$WORK/manifest.toml" > "$WORK/manifest.scan" \
            || die "판정 불가: $man 을 읽지 못했다: $(sed -n '$p' "$WORK/manifest.scan")"
        mid=$(read_manifest_id "$WORK/manifest.scan") \
            || die "판정 불가: $man: $(printf '%s\n' "$mid" | sed -n '$p' | cut -f2)"
        case "$ids" in *" $mid "*) LOCKED_DIRS="$LOCKED_DIRS $rel" ;; esac
    done < "$WORK/manifests.paths"
}
is_locked_dir() {
    case " $LOCKED_DIRS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}
SKIPPED_LOCKED=""
note_locked() {
    case " $SKIPPED_LOCKED " in *" $1 "*) ;; *) SKIPPED_LOCKED="$SKIPPED_LOCKED $1" ;; esac
}
if [ -n "$CHANGED$OUTSIDE_BUILD" ]; then
    compute_locked_dirs
fi

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
        if is_locked_dir "$SCAN_ROOT/$(basename "$(dirname "$man")")"; then
            note_locked "$SCAN_ROOT/$(basename "$(dirname "$man")")"
            continue
        fi
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

crate_dir_of() {
    local r
    for r in $EXTRA_ROOTS; do
        case "$1" in "$r"/*) printf '%s' "$r"; return ;; esac
    done
    printf '%s' "$1" | sed -n "s|^\\(${SCAN_ROOT}/[^/]*\\)/.*\$|\\1|p"
}

VIOLATIONS=0
CONSIDERED=0
BEHIND=0

CHANGED_CRATES=""
while IFS= read -r f; do
    [ -z "$f" ] && continue
    build_affecting "$f" || continue
    # 호스트 시험 픽스처는 어떤 플러그인의 빌드 입력도 아니다.
    case "$f" in "$SCAN_ROOT"/tasty-test-support/fixtures/official-plugins/*) continue ;; esac
    CHANGED_CRATES="$CHANGED_CRATES
$(crate_dir_of "$f")"
done <<EOF
$CHANGED
EOF
CHANGED_CRATES=$(printf '%s\n' "$CHANGED_CRATES" | sed -n '/./p' | sort -u)

SHARED_CHANGED=""
for c in $CHANGED_CRATES; do
    [ -f "$ROOT/$c/tasty-plugin.toml" ] || SHARED_CHANGED="$SHARED_CHANGED $c"
done

PLUGINS=""
for c in $CHANGED_CRATES; do
    [ -f "$ROOT/$c/tasty-plugin.toml" ] || continue
    if is_locked_dir "$c"; then note_locked "$c"; continue; fi
    PLUGINS="$PLUGINS $c"
done

# cargo tree는 range의 각 과거 그래프가 아니라 현재 작업 트리를 읽는다.
# 의존성 추가·삭제에 따라 더 넓게 또는 좁게 판단할 수 있으므로 과거 범위 조사에 한계가 있다.
if [ -n "${SHARED_CHANGED// /}" ]; then
    for man in "$ROOT/$SCAN_ROOT"/*/tasty-plugin.toml; do
        [ -f "$man" ] || continue
        pdir=$(dirname "$man"); pname=$(basename "$pdir"); prel="$SCAN_ROOT/$pname"
        case " $PLUGINS " in *" $prel "*) continue ;; esac
        if is_locked_dir "$prel"; then note_locked "$prel"; continue; fi
        for c in $SHARED_CHANGED; do
            if links_crate "$pname" "$c"; then PLUGINS="$PLUGINS $prel"; break; fi
        done
    done
fi
PLUGINS=$(printf '%s\n' $PLUGINS | sed -n '/./p' | sort -u)
if [ -n "${SKIPPED_LOCKED// /}" ]; then
    echo "[plugin-version] 잠금 파일의 공식 플러그인 id라 대상에서 뺐다:$SKIPPED_LOCKED"
fi

INVOLVED=$(printf '%s\n' $CHANGED_CRATES | sed -n '/./p' | sort -u)
if [ "$MODE" = staged ]; then
    AFTER_TREE=$(git write-tree) || die "판정 불가: 인덱스 트리를 못 만들었다."
else
    AFTER_TREE="$AFTER"
fi
# 이유: INVOLVED의 줄별 경로를 materialize의 개별 인자로 전달한다.
# shellcheck disable=SC2086
materialize before "$BEFORE_REV" $INVOLVED
# shellcheck disable=SC2086
materialize after "$AFTER_TREE" $INVOLVED

for base in $PLUGINS; do
    man="$base/tasty-plugin.toml"
    exists_at "$(after_ref "$man")" || continue
    exists_at "$BEFORE_REV:$man" || continue

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
    # 버전 감소는 비교 기준 오류와 의도한 변경을 구분할 수 없어 별도 판정 불가로 집계한다.
    if ver_gt "$vb" "$va"; then
        printf '▲ [plugin-version] %s: 이 범위의 앞 끝이 뒤 끝보다 새것이다 (%s → %s)\n' \
            "$base" "$vb" "$va" >&2
        printf '    비교 기준 오류인지 의도한 버전 감소인지 이 검사만으로는 구별할 수 없다:\n' >&2
        printf '      (가) 네가 뒤처졌다: 비교 대상(%s)이 작업 시작점보다 앞선 경우다.\n' \
            "$BEFORE_REV" >&2
        printf '           다른 변경이 반영된 공유 ref를 비교 대상으로 삼았는지 확인해라.\n' >&2
        printf '           작업 시작점의 정확한 커밋을 기준으로 다시 실행해라:\n' >&2
        printf '             %s --range <작업 시작 커밋> HEAD\n' \
            "scripts/check-plugin-version-bump.sh" >&2
        printf '      (나) 의도적으로 내렸다: 버전 감소의 필요성은 별도로 검토해야 한다.\n' >&2
        printf '    비교 기준을 확인하기 전에 버전만 올려 해결하지 마라.\n' >&2
        printf '      이미 배포된 버전과 작업 중인 변경을 먼저 대조해라.\n' >&2
        BEHIND=$((BEHIND + 1))
    elif ! ver_gt "$va" "$vb"; then
        printf '✘ [plugin-version] %s: 배포 대상 내용이 달라지는데 version 이 안 올랐다 (%s → %s)\n' \
            "$base" "$vb" "$va" >&2
        printf '    내용이 바뀐 파일:%s\n' "$changed_list" >&2
        printf '    고쳐라: %s 와 %s 의 version 을 같은 값으로 patch +1.\n' \
            "$base/Cargo.toml" "$man" >&2
        [ -z "$VIOLATIONS_OUT" ] || printf '%s\n' "$base" >> "$VIOLATIONS_OUT"
        if [ "$MODE" = staged ]; then
            printf '    이 검사는 index와 %s를 비교한다. 배포할 때는\n' \
                "$BEFORE_REV" >&2
            printf '           직전 push 지점부터의 변경 전체도 확인해야 한다.\n' >&2
            printf '           같은 작업을 여러 커밋으로 나누면 앞 커밋에서 버전을 올렸어도\n' >&2
            printf '           추가 내용 변경에 대해 다시 버전 증가를 요구할 수 있다.\n' >&2
            printf '           프로젝트의 커밋·버전 정책에 따라 처리해라:\n' >&2
            printf '             (가) 같은 작업의 앞 버전 변경 커밋에 이 변경을 합친다.\n' >&2
            printf '             (나) 한 번 더 올리고 통합 담당자에게 최종 버전 확인을 요청한다.\n' >&2
            printf '           배포 범위를 확인하는 명령: %s --range <직전 push> HEAD\n' \
                "scripts/check-plugin-version-bump.sh" >&2
            printf '           커밋 전에는 staged 내용, push 전에는 원격 이후 범위를\n' >&2
            printf '             각각 확인한다. 두 검사의 기준 커밋이 다름에 주의해라.\n' >&2
            printf '             pre-push 훅의 B.9는 원격 tip을 기준으로 다시 검사한다.\n' >&2
        fi
        VIOLATIONS=$((VIOLATIONS + 1))
    fi
done

# 모든 후보가 실패했다는 사실만으로 원인을 확정할 수 없어 비교 범위도 확인하도록 안내한다.
flagged=$((VIOLATIONS + BEHIND))
if [ "$CONSIDERED" -ge 2 ] && [ "$flagged" -eq "$CONSIDERED" ]; then
    printf '[plugin-version] 판정 대상 %d 건이 모두 실패했다. 여러 실제 변경일 수도 있고\n' \
        "$CONSIDERED" >&2
    printf '    비교 범위가 잘못됐을 수도 있으므로 기준을 확인해라(BEFORE=%s).\n' "$BEFORE_REV" >&2
fi

if [ "$VIOLATIONS" -gt 0 ]; then
    printf '[plugin-version] 위반 %d 건 / 판정 대상 %d 건' "$VIOLATIONS" "$CONSIDERED" >&2
    [ "$BEHIND" -gt 0 ] && printf ' (그 밖에 범위가 뒤집힌 항 %d 건)' "$BEHIND" >&2
    printf '\n' >&2
    exit 1
fi
if [ "$BEHIND" -gt 0 ]; then
    printf '[plugin-version] 판정 불가 — 범위가 뒤집힌 항 %d 건 / 판정 대상 %d 건.\n' \
        "$BEHIND" "$CONSIDERED" >&2
    printf '    측정이 안 됐으므로 게이트를 통과로 읽지 않는다.\n' >&2
    exit 2
fi
# workspace 밖 path 의존 파일은 crates 파일 수와 구분해 출력한다.
n_all=$(printf '%s\n' "$CHANGED" | sed -n '/./p' | wc -l)
n_extra=$(printf '%s\n' "$CHANGED" | sed -n "/^${SCAN_ROOT}\//!{/./p;}" | wc -l)
# 판정 대상이 없는 변경은 정상이라 rc 는 0 이지만, 아무것도 검사하지 않았으므로 통과로 표기하지 않는다.
if [ "$CONSIDERED" -eq 0 ]; then
    printf '[plugin-version] 판정 대상 없음 — 판정 대상 0 건 (변경된 crates 파일 %d 개 중). 버전을 확인할 플러그인 내용 변경이 없다.\n' \
        "$n_all"
elif [ "$n_extra" -gt 0 ]; then
    printf '[plugin-version] 통과 — 판정 대상 %d 건 (변경된 crates 파일 %d 개 · 워크스페이스 밖 path 의존 파일 %d 개 중)\n' \
        "$CONSIDERED" "$((n_all - n_extra))" "$n_extra"
else
    printf '[plugin-version] 통과 — 판정 대상 %d 건 (변경된 crates 파일 %d 개 중)\n' \
        "$CONSIDERED" "$n_all"
fi
exit 0
