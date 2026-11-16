#!/usr/bin/env bash
# GUI feature를 끈 workspace를 별도 target에 빌드하고 번들 플러그인을 준비한 뒤 검사에 사용할 바이너리 경로를 출력한다.
# target-e2e-headless/debug 위치는 개발용 plugin 탐색의 저장소 경로 계산에 맞춘다.
# 실패하면 stdout에 경로를 내지 않는다. 호출자는 TASTY_E2E_BIN을 설정하지 않고 기본 하네스를 사용할 수 있다.
# 사용: BIN=$(scripts/build-e2e-headless.sh) && export TASTY_E2E_BIN=$BIN
set -uo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
HL_DIR="$ROOT/target-e2e-headless"     # .gitignore 에 등재돼 있다
BIN="$HL_DIR/debug/tasty"

CARGO_TARGET_DIR="$HL_DIR" cargo build --workspace --no-default-features >&2
rc=$?
if [ $rc -ne 0 ]; then
  echo "헤드리스 빌드 실패(rc=$rc) — TASTY_E2E_BIN 을 넘기지 마라." >&2
  exit "$rc"
fi

# 번들 플러그인이 필요한 스위트를 위해 같은 target 옆에 공식 플러그인과 mesh-demo를 준비한다.
# stdout은 바이너리 경로 하나만 내야 하므로 준비 출력은 stderr로 보낸다.
CARGO_TARGET_DIR="$HL_DIR" bash "$ROOT/scripts/stage-dev-plugins.sh" --profile debug >&2
rc=$?
if [ $rc -ne 0 ]; then
  echo "번들 플러그인 준비 실패(rc=$rc) — TASTY_E2E_BIN 을 넘기지 마라." >&2
  exit "$rc"
fi

if [ ! -x "$BIN" ]; then
  echo "빌드는 성공했는데 $BIN 이 없다 — 경로 규약이 바뀌었는지 확인해라." >&2
  exit 1
fi

# Rust 소스 mtime이 바이너리와 같거나 새로우면 거부한다. 내용 차이나 전체 빌드 입력의 최신성을 확인하는 검사는 아니다.
# 하네스도 mtime을 쓰므로 읽지 못한 파일을 서로 보완해 준다고 볼 수 없다.
bin_mtime=$(find "$BIN" -maxdepth 0 -printf '%T@' 2>/dev/null)
if [ -z "$bin_mtime" ]; then
  echo "$BIN 의 mtime 을 못 읽었다 — 낡음 판정을 할 수 없다." >&2
  exit 1
fi
# awk의 조기 종료가 find에 SIGPIPE를 일으키지 않도록 출력을 먼저 받는다.
all_src_mtimes=$(find "$ROOT/src" "$ROOT/crates" -name '*.rs' -printf '%T@ %p\n' 2>/dev/null)
stale=$(awk -v b="$bin_mtime" '$1 >= b { print $2; exit }' <<<"$all_src_mtimes")
if [ -n "$stale" ]; then
  echo "$stale 의 mtime이 바이너리와 같거나 새롭다. 내용 차이는 확인하지 못했으므로 다시 빌드해라." >&2
  exit 1
fi

echo "$BIN"
