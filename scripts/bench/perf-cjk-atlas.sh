#!/usr/bin/env bash
# 세 코드포인트 범위의 출력을 발생시켜 마지막 perf 로그를 보여준다.
# tasty CLI·jq·python3가 필요하다. 특정 폰트 경고 문자열만 검사하므로 글리프 표시 전체를 검증하지는 않는다.

set -euo pipefail

PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
PROFILE="${PERF_PROFILE:-release}"
case "$PROFILE" in
    release) CARGO_FLAGS=(--release) ;;
    dist)    CARGO_FLAGS=(--profile dist) ;;
    *) echo "error: PERF_PROFILE must be release|dist (got '$PROFILE')" >&2; exit 2 ;;
esac
LOG_DIR="${PERF_LOG_DIR:-${TMPDIR:-/tmp}/tasty-bench}"
LOG="${LOG_DIR}/perf-cjk-${PLATFORM}-${PROFILE}.log"
DURATION="${PERF_DURATION_SECS:-60}"

mkdir -p "$LOG_DIR"

if ! command -v tasty >/dev/null 2>&1; then
    echo "error: tasty CLI not on PATH" >&2
    exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
    echo "error: jq required (used to parse 'tasty list surfaces')" >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    echo "error: python3 required (used to emit CJK code points)" >&2
    exit 2
fi

echo "[perf-cjk-atlas] platform=$PLATFORM profile=$PROFILE duration=${DURATION}s log=$LOG"

TASTY_LOG="warn,tasty::gfx::perf=info" \
    cargo run "${CARGO_FLAGS[@]}" > "$LOG" 2>&1 &
TASTY_PID=$!
cleanup() {
    kill "$TASTY_PID" 2>/dev/null || true
    wait "$TASTY_PID" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 60); do
    if tasty list info >/dev/null 2>&1; then break; fi
    sleep 0.5
done
tasty list info >/dev/null

FIRST_SID="$(tasty list surfaces | jq -r '.[0].id')"
if [ -z "$FIRST_SID" ] || [ "$FIRST_SID" = "null" ]; then
    echo "error: could not determine first surface id" >&2
    exit 3
fi

CR="$(printf '\r')"

# 짧은 샘플 출력 뒤 지정한 폰트 경고가 있는지 확인한다.
tasty send text "한국어 中文 日本語${CR}" --surface "$FIRST_SID" >/dev/null
sleep 2
if grep -qE "font fallback missing|no glyph for codepoint" "$LOG"; then
    echo "error: CJK fallback font missing — abort" >&2
    grep -m5 -E "font fallback missing|no glyph for codepoint" "$LOG" >&2
    exit 3
fi

for _ in 1 2 3; do
    tasty split --level surface --target-surface "$FIRST_SID" --direction vertical >/dev/null
done

for sid in $(tasty list surfaces | jq -r '.[].id'); do
    tasty send text "python3 -c \"print(''.join(chr(0x4E00 + i % 3000) for i in range(50000)))\"${CR}" \
        --surface "$sid" >/dev/null
    tasty send text "python3 -c \"print(''.join(chr(0x3040 + i % 3000) for i in range(50000)))\"${CR}" \
        --surface "$sid" >/dev/null
    tasty send text "python3 -c \"print(''.join(chr(0xAC00 + i % 3000) for i in range(50000)))\"${CR}" \
        --surface "$sid" >/dev/null
done

sleep "$DURATION"

cleanup
trap - EXIT

echo "--- last 12 perf samples ---"
grep "tasty::gfx::perf" "$LOG" | tail -12
