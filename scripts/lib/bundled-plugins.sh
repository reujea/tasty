# 저장소 루트를 cwd로 두고 source한다. 배포 패키지에 넣을 공식 플러그인을 잠금 파일
# (official-plugins.lock.toml)의 bundle = true 산출물로 준비하고, 패키지 안의 사본을 대조한다.
# 플러그인은 여기서 빌드하지 않는다. 기준: docs/dev-guide/plugin-packaging.md의 "배포 패키지의 번들 플러그인" 절.
# build-windows.ps1의 Get-BundledPlugins·Test-BundledPluginTree도 같은 규칙이다.
# bash 3.2(macOS 기본)에서도 돈다.
# shellcheck shell=bash

# 받은 산출물을 푸는 폴더. deb·rpm assets(Cargo.toml)가 이 경로를 원본으로 읽는다.
BUNDLED_PLUGINS_DIR="target/bundled-plugins"

# 받기·SHA-256 대조·매니페스트 검증은 fetch-official-plugins.sh가 한다. 산출물이 없거나 해시가 다르면
# 실패하고 폴더를 남기지 않는다. 이전 실행의 폴더가 있으면 unpack이 거부하므로 먼저 지운다.
# 매니페스트 서명은 이 빌드의 키로만 만든다. 받은 산출물에 .sig가 있으면 서명을 생략하는 debug를
# 포함해 어느 프로필에서든 실패한다(sign-bundle.sh --dir과 같은 계약).
fetch_bundled_plugins() {
    local platform="$1" presigned
    rm -rf "${BUNDLED_PLUGINS_DIR:?}"
    if ! ./scripts/fetch-official-plugins.sh --platform "$platform" --out "$BUNDLED_PLUGINS_DIR"; then
        rm -rf "${BUNDLED_PLUGINS_DIR:?}"
        echo "Error: could not fetch the locked official plugins for $platform" >&2
        return 1
    fi
    presigned=$(find "$BUNDLED_PLUGINS_DIR" -name '*.sig' | LC_ALL=C sort)
    if [[ -n "$presigned" ]]; then
        echo "Error: $BUNDLED_PLUGINS_DIR already contains signatures; fetched plugins must arrive unsigned:" >&2
        printf '  %s\n' "$presigned" >&2
        rm -rf "${BUNDLED_PLUGINS_DIR:?}"
        return 1
    fi
}

# deb·rpm은 assets에 mode를 적지 않고 파일 권한을 그대로 싣는다(rpm 폴더 항목은 rpm_dir_assets.py가 755를 명시). umask에 따라 달라지지 않도록
# 실행 권한이 있는 파일은 755, 나머지 파일은 644, 폴더는 755로 맞춘다.
normalize_bundled_plugin_modes() {
    find "$BUNDLED_PLUGINS_DIR" -type d -exec chmod 755 {} +
    find "$BUNDLED_PLUGINS_DIR" -type f -perm -u+x -exec chmod 755 {} +
    find "$BUNDLED_PLUGINS_DIR" -type f ! -perm -u+x -exec chmod 644 {} +
}

# 스테이징 폴더 아래 파일의 상대 경로를 정렬해 한 줄씩 낸다.
bundled_plugin_files() {
    (cd "$BUNDLED_PLUGINS_DIR" && find . -type f | sed 's|^\./||' | LC_ALL=C sort)
}

# 받은 폴더를 그대로 패키지의 plugins 폴더에 복사한다. 고지 파일(LICENSE 등)도 플러그인과 함께 간다.
stage_bundled_plugins() {
    local dest="$1"
    mkdir -p "$dest"
    cp -R "$BUNDLED_PLUGINS_DIR/." "$dest/"
}

# 패키지 목록(tar -t, rpm 헤더 목록의 일반 파일, dpkg-deb --fsys-tarfile | tar -t)에서 prefix 아래 파일 집합이
# 스테이징 폴더와 같은지 본다. 앞의 "./"를 떼고, "/"로 끝나는 폴더 줄은 뺀다. 내용은 비교하지 않는다.
verify_bundled_plugin_listing() {
    local listing="$1" prefix="$2" label="$3" want got diff
    want=$(bundled_plugin_files) || return 1
    got=$(awk -v p="$prefix" '
            { s = $0; sub(/^\.\//, "", s)
              if (index(s, p) == 1 && substr(s, length(s)) != "/") print substr(s, length(p) + 1) }' \
            <<<"$listing" | LC_ALL=C sort)
    diff=$(diff <(printf '%s\n' "$want") <(printf '%s\n' "$got") || true)
    if [[ -n "$diff" ]]; then
        echo "Error: bundled plugins in $label (under $prefix) differ from $BUNDLED_PLUGINS_DIR (< missing, > extra):" >&2
        printf '%s\n' "$diff" >&2
        return 1
    fi
}

# 풀린 트리의 plugins 폴더가 스테이징 폴더와 파일 집합·내용 모두 같은지 본다.
verify_bundled_plugin_tree() {
    local root="$1" label="$2" want got diff f
    want=$(bundled_plugin_files) || return 1
    if [[ ! -d "$root" ]]; then
        echo "Error: $label has no plugins folder ($root)" >&2
        return 1
    fi
    got=$(cd "$root" && find . -type f | sed 's|^\./||' | LC_ALL=C sort)
    diff=$(diff <(printf '%s\n' "$want") <(printf '%s\n' "$got") || true)
    if [[ -n "$diff" ]]; then
        echo "Error: bundled plugins in $label differ from $BUNDLED_PLUGINS_DIR (< missing, > extra):" >&2
        printf '%s\n' "$diff" >&2
        return 1
    fi
    while IFS= read -r f; do
        if ! cmp -s "$BUNDLED_PLUGINS_DIR/$f" "$root/$f"; then
            echo "Error: $f in $label differs from $BUNDLED_PLUGINS_DIR" >&2
            return 1
        fi
    done <<<"$want"
}
