# 저장소 루트를 cwd로 두고 source한다. rpm이 자기 폴더를 소유하게 하는 assets 표를 만들고,
# 만든 rpm의 헤더 파일 목록에서 폴더 소유와 항목 종류를 확인한다.
# cargo-generate-rpm은 폴더 항목을 만들지 않으므로 scripts/lib/rpm_dir_assets.py가 Cargo.toml의 assets에
# 폴더 항목을 덧붙이고, build-linux.sh가 그 표를 --metadata-overwrite로 넘긴다.
# shellcheck shell=bash

# 패키지가 소유하는 뿌리. 이 아래에 놓이는 파일의 조상 폴더(뿌리 포함)를 모두 소유한다.
# /usr/bin, /usr/share/applications, /usr/share/icons/hicolor 같은 폴더는 배포판 패키지가 소유하므로 넣지 않는다.
RPM_OWNED_ROOTS=(/usr/lib/tasty /usr/share/licenses/tasty)

# 생성기에 필요한 Python(3.11 이상, tomllib)이 있는지 본다. release·dist 빌드가 본체를 빌드하기 전에 부른다.
check_rpm_dir_tools() {
    if ! python3 -I -c 'import tomllib' 2>/dev/null; then
        echo "Error: python3 3.11 or later (tomllib) is required to build the rpm directory entries" >&2
        return 1
    fi
}

# $1 = Cargo.toml, $2 = 출력 TOML. 폴더 항목의 source로 쓸 빈 파일을 출력 옆에 만든다.
write_rpm_assets_overwrite() {
    local manifest=$1 out=$2 empty
    empty="${out%.toml}.dir-entry"
    mkdir -p "$(dirname "$out")" || return 1
    : >"$empty" || { echo "Error: cannot create $empty" >&2; return 1; }
    python3 -I scripts/lib/rpm_dir_assets.py "$manifest" "$empty" "$out" "${RPM_OWNED_ROOTS[@]}"
}

# $1 = rpm 파일. 헤더의 파일 목록을 "권한<TAB>경로"로 한 줄씩 낸다.
rpm_header_entries() {
    LC_ALL=C rpm -qp --qf '[%{FILEMODES:perms}\t%{FILENAMES}\n]' "$1" 2>/dev/null
}

# $1 = rpm_header_entries 출력, $2 = 진단용 이름. 일반 파일 경로만 낸다.
# 일반 파일과 폴더 밖의 항목(링크 등)이 있으면 실패한다.
rpm_regular_files() {
    awk -F '\t' -v label="$2" '
        NF < 2 { printf "Error: malformed rpm file entry in %s: %s\n", label, $0 > "/dev/stderr"; bad = 1; next }
        substr($1, 1, 1) == "-" { print $2; next }
        substr($1, 1, 1) == "d" { next }
        { printf "Error: unexpected rpm file entry type in %s: %s %s\n", label, $1, $2 > "/dev/stderr"; bad = 1 }
        END { exit bad }' <<<"$1"
}

# $1 = rpm_header_entries 출력, $2 = 진단용 이름, 나머지 = 소유 뿌리.
# 소유 뿌리 아래 일반 파일의 조상 폴더(뿌리 포함) 집합이 헤더의 폴더 항목 집합과 같고,
# 폴더 항목의 권한이 모두 drwxr-xr-x인지 본다.
verify_rpm_dir_ownership() {
    local entries=$1 label=$2
    shift 2
    awk -F '\t' -v label="$label" -v roots="$*" '
        BEGIN { n = split(roots, root, " ") }
        function owned_root(p,    i) {
            for (i = 1; i <= n; i++) if (p == root[i] || index(p, root[i] "/") == 1) return root[i]
            return ""
        }
        function parent(p) { sub(/\/[^\/]*$/, "", p); return p }
        {
            kind = substr($1, 1, 1); p = $2
            if (kind == "d") {
                have[p] = 1
                if ($1 != "drwxr-xr-x") { printf "Error: %s: directory %s has mode %s, want drwxr-xr-x\n", label, p, $1 > "/dev/stderr"; bad = 1 }
                if (owned_root(p) == "") { printf "Error: %s: directory entry outside the owned roots: %s\n", label, p > "/dev/stderr"; bad = 1 }
                next
            }
            r = owned_root(p)
            if (r == "" || p == r) next
            d = parent(p)
            while (1) { want[d] = 1; if (d == r) break; d = parent(d) }
        }
        END {
            for (d in want) if (!(d in have)) { printf "Error: %s: directory not owned by the package: %s\n", label, d > "/dev/stderr"; bad = 1 }
            for (d in have) if (!(d in want) && owned_root(d) != "") { printf "Error: %s: owned directory with no packaged file: %s\n", label, d > "/dev/stderr"; bad = 1 }
            exit bad
        }' <<<"$entries"
}
