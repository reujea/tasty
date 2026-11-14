# 저장소 루트를 cwd로 두고 source한다. 고지 파일 목록은 LICENSES 디렉터리에서 읽는다.
# 배포 기준: docs/dev-guide/release.md#배포물의-고지-파일.
# shellcheck shell=bash

# LICENSES가 비면 고지 파일이 누락된 checkout으로 보고 거부한다.
# LICENSES는 일반 파일만 담는다. RPM은 본문을 한 폴더에 펼쳐 놓으므로 하위 폴더를 둘 수 없다.
# 폴더·링크를 건너뛰면 그 본문이 배포물에서 조용히 빠지므로 실패한다.
# build-windows.ps1의 Get-NoticeSetFiles도 같은 규칙이다.
notice_set_files() {
    local f found=0 out
    out="LICENSE"$'\n'"THIRD_PARTY_LICENSES.md"
    for f in LICENSES/*; do
        [[ -e "$f" || -L "$f" ]] || continue
        if [[ -L "$f" || ! -f "$f" ]]; then
            echo "Error: $f is not a regular file; LICENSES/ holds licence texts only" >&2
            return 1
        fi
        out+=$'\n'"$f"
        found=1
    done
    if [[ "$found" -eq 0 ]]; then
        echo "Error: LICENSES/ holds no licence text" >&2
        return 1
    fi
    echo "$out"
}

# 고지 문서의 상대 링크를 유지하도록 LICENSES 하위 경로를 보존한다.
stage_notice() {
    local dest="$1" files f
    files=$(notice_set_files) || return 1
    mkdir -p "$dest/LICENSES"
    while IFS= read -r f; do
        cp "$f" "$dest/$f"
    done <<<"$files"
}

# 저장소 원본과 스테이징된 파일의 내용을 비교한다. 아직 패키징하지 않은 트리도 받을 수 있다.
verify_notice_tree() {
    local root="$1" label="$2" files f
    files=$(notice_set_files) || return 1
    while IFS= read -r f; do
        if ! cmp -s "$f" "$root/$f"; then
            echo "Error: $f missing from $label (or differs from the repo copy)" >&2
            return 1
        fi
    done <<<"$files"
}

# 목록의 한 줄이 prefix+경로와 같은지 확인한다. 줄마다 경로 하나여야 하며(tar -t, rpm -qpl,
# dpkg-deb --fsys-tarfile | tar -t), 앞의 "./" 하나만 떼고 줄 전체를 비교한다.
# 그래서 다른 폴더 아래에 같은 이름이 있어도 통과하지 않는다. 파일 내용 검사는 아니다.
# flat 모드는 RPM 배치처럼 LICENSES/ 접두를 뺀 이름을 찾는다.
# producer의 SIGPIPE를 피하도록 목록 전체를 문자열로 받는다.
verify_notice_listing() {
    local listing="$1" prefix="$2" label="$3" flat="${4:-}" files f want
    files=$(notice_set_files) || return 1
    while IFS= read -r f; do
        want="$f"
        if [[ -n "$flat" ]]; then
            want="${f#LICENSES/}"
        fi
        if ! awk -v t="$prefix$want" '
                { p = $0; sub(/^\.\//, "", p); if (p == t) hit = 1 }
                END { exit !hit }' <<<"$listing"; then
            echo "Error: $prefix$want not in $label" >&2
            return 1
        fi
    done <<<"$files"
}
