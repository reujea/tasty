#!/usr/bin/env bash
# sign-bundle.sh --dir 의 계약을 일회용 폴더에서 확인한다.
# 정상(매니페스트마다 64바이트 서명), 빈 폴더, 키 없음, 이미 서명된 폴더를 나눈다.
# 정상 경우는 서명을 같은 키의 공개키로 다시 검증해 digest 서명 형식까지 본다.
# 사용: sign-bundle-dir.sh <저장소 루트> <비어 있는 임시 폴더>
set -eu -o pipefail
root="$(cd "$1" && pwd -P)"
work="$(cd "$2" && pwd -P)"
sign="$root/scripts/sign-bundle.sh"

failures=0
expect() {
    local want="$1" got="$2" what="$3"
    if [ "$want" = "$got" ]; then
        printf 'ok   %s (rc=%s)\n' "$what" "$got"
    else
        printf 'FAIL %s: rc=%s, 기대 %s\n' "$what" "$got" "$want" >&2
        failures=$((failures + 1))
    fi
}
expect_output() {
    local needle="$1" file="$2" what="$3"
    if grep -qF -- "$needle" "$file"; then
        printf 'ok   %s\n' "$what"
    else
        printf 'FAIL %s: 출력에 "%s"가 없다\n' "$what" "$needle" >&2
        sed 's/^/    /' "$file" >&2
        failures=$((failures + 1))
    fi
}

key="$work/key.pem"
openssl genpkey -algorithm ed25519 -out "$key" 2>/dev/null
openssl pkey -in "$key" -pubout -out "$work/pub.pem"

stage="$work/stage"
for id in com.example.a com.example.b; do
    mkdir -p "$stage/$id/lang"
    printf 'id = "%s"\n' "$id" > "$stage/$id/tasty-plugin.toml"
    printf 'x = "y"\n' > "$stage/$id/lang/en.toml"
done

# 1. 정상: 두 매니페스트에 64바이트 서명이 생기고, digest 서명으로 검증된다.
rc=0
bash "$sign" --key "$key" --dir "$stage" > "$work/out1" 2>&1 || rc=$?
expect 0 "$rc" "정상 폴더 서명"
for id in com.example.a com.example.b; do
    sig="$stage/$id/tasty-plugin.toml.sig"
    size=0
    [ -f "$sig" ] && size=$(wc -c < "$sig" | tr -d ' ')
    expect 64 "$size" "$id 서명 크기"
    openssl dgst -sha256 -binary "$stage/$id/tasty-plugin.toml" > "$work/digest"
    vrc=0
    openssl pkeyutl -verify -pubin -inkey "$work/pub.pem" -rawin \
        -in "$work/digest" -sigfile "$sig" > /dev/null 2>&1 || vrc=$?
    expect 0 "$vrc" "$id 서명 검증"
done
if [ -e "$stage/com.example.a/lang/en.toml.sig" ]; then
    echo "FAIL lang 파일까지 서명했다" >&2
    failures=$((failures + 1))
fi

# 2. 이미 서명이 있는 폴더: 아무것도 쓰지 않고 실패한다.
rm -f "$stage/com.example.b/tasty-plugin.toml.sig"
rc=0
bash "$sign" --key "$key" --dir "$stage" > "$work/out2" 2>&1 || rc=$?
expect 1 "$rc" "서명이 이미 있는 폴더"
expect_output "com.example.a/tasty-plugin.toml.sig" "$work/out2" "기존 서명 파일 이름 출력"
if [ -e "$stage/com.example.b/tasty-plugin.toml.sig" ]; then
    echo "FAIL 거절하면서 다른 매니페스트를 서명했다" >&2
    failures=$((failures + 1))
fi

# 3. 빈 폴더: 서명할 매니페스트가 없으면 실패한다.
mkdir -p "$work/empty"
rc=0
bash "$sign" --key "$key" --dir "$work/empty" > "$work/out3" 2>&1 || rc=$?
expect 1 "$rc" "빈 폴더"
expect_output "tasty-plugin.toml found" "$work/out3" "빈 폴더 안내"

# 4. 키 없음: 키 파일이 없으면 실패하고 서명을 만들지 않는다.
rm -f "$stage"/*/tasty-plugin.toml.sig
rc=0
bash "$sign" --key "$work/missing.pem" --dir "$stage" > "$work/out4" 2>&1 || rc=$?
expect 1 "$rc" "키 없음"
expect_output "private key not found" "$work/out4" "키 없음 안내"
if [ -n "$(find "$stage" -name '*.sig')" ]; then
    echo "FAIL 키 없이 서명 파일이 생겼다" >&2
    failures=$((failures + 1))
fi

# 5. 다른 대상 지정과 함께 쓰면 사용법 오류다.
rc=0
bash "$sign" --key "$key" --dir "$stage" --all-builtins > "$work/out5" 2>&1 || rc=$?
expect 2 "$rc" "--dir 와 --all-builtins 동시 지정"

if [ "$failures" -ne 0 ]; then
    echo "$failures 건 실패" >&2
    exit 1
fi
echo "sign-bundle --dir: 모든 경우 통과"
