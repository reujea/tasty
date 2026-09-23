#!/usr/bin/env bash
#
# CJK 글리프 집약 시나리오. 4 surface × 3 batch (CJK 한자 / 히라가나 / 한글 음절) 출력.
# 목적: atlas eviction 빈도 (atlas_evictions delta) + 페이지 사용량 평균 측정.
#
# 사전: tasty 빌드 + `tasty` CLI 가 PATH, jq + python3.
# 출력: ${PERF_LOG_DIR:-${TMPDIR:-/tmp}/tasty-bench}/perf-cjk-{platform}-{profile}.log 의 마지막 12 `perf` 라인.
#
# CJK 폰트 fallback 부재 시 자동 abort (visual check 의존 X). 측정 segment / 임계값은
# docs/dev-guide/gpu-rendering.md#성능-측정 참조.

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

# 1) tasty 실행 — perf info + font warn 모두 캡처해서 fallback 부재 시 grep 가능.
RUST_LOG="tasty::gfx::perf=info,tasty::font=warn,tasty=warn" \
    cargo run "${CARGO_FLAGS[@]}" > "$LOG" 2>&1 &
TASTY_PID=$!
cleanup() {
    kill "$TASTY_PID" 2>/dev/null || true
    wait "$TASTY_PID" 2>/dev/null || true
}
trap cleanup EXIT

# 2) tasty ready 대기
for _ in $(seq 1 60); do
    if tasty list info >/dev/null 2>&1; then break; fi
    sleep 0.5
done
tasty list info >/dev/null

# 3) 첫 surface 의 ID 확보
FIRST_SID="$(tasty list surfaces | jq -r '.[0].id')"
if [ -z "$FIRST_SID" ] || [ "$FIRST_SID" = "null" ]; then
    echo "error: could not determine first surface id" >&2
    exit 3
fi

CR="$(printf '\r')"

# 4) CJK fallback 폰트 사전 점검 — 부재 시 abort.
#    한자 / 히라가나 / 한글을 한 줄 흘려보내고, tasty::font warning 이 잡히면 abort.
tasty send text "한국어 中文 日本語${CR}" --surface "$FIRST_SID" >/dev/null
sleep 2
if grep -qE "font fallback missing|no glyph for codepoint" "$LOG"; then
    echo "error: CJK fallback font missing — abort" >&2
    grep -m5 -E "font fallback missing|no glyph for codepoint" "$LOG" >&2
    exit 3
fi

# 5) surface 3 개 추가 (총 4 개) — 기존 perf-10-surfaces.sh 와 동일한 split 패턴.
for _ in 1 2 3; do
    tasty split --level surface --target-surface "$FIRST_SID" --direction vertical >/dev/null
done

# 6) 각 surface 에 CJK 3 batch (한자 / 히라가나 / 한글 음절) 출력.
#    한 batch = 3000 unique 코드포인트 × ~16 회 반복 = 50000 글리프 호출.
for sid in $(tasty list surfaces | jq -r '.[].id'); do
    tasty send text "python3 -c \"print(''.join(chr(0x4E00 + i % 3000) for i in range(50000)))\"${CR}" \
        --surface "$sid" >/dev/null
    tasty send text "python3 -c \"print(''.join(chr(0x3040 + i % 3000) for i in range(50000)))\"${CR}" \
        --surface "$sid" >/dev/null
    tasty send text "python3 -c \"print(''.join(chr(0xAC00 + i % 3000) for i in range(50000)))\"${CR}" \
        --surface "$sid" >/dev/null
done

# 7) 측정 (DURATION 초).
sleep "$DURATION"

# 8) 종료.
cleanup
trap - EXIT

# 9) 마지막 12 perf 라인 추출 (≈60s @ 5s/dump).
echo "--- last 12 perf samples ---"
grep "tasty::gfx::perf" "$LOG" | tail -12
