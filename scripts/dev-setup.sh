#!/usr/bin/env bash
# 이 checkout의 Git 훅 경로를 설정하고 ShellCheck와 모든 target의 Cargo 의존을 준비한다.
# 사용: bash scripts/dev-setup.sh. ShellCheck 설치와 의존 받기의 실패는 안내만 하고 계속한다.

set -e

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
    echo "✘ git 저장소 안에서 실행해주세요." >&2
    exit 1
}

cd "$REPO_ROOT"

red()    { printf '\033[31m%s\033[0m\n' "$*" >&2; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }

yellow "▸ Tasty 개발 환경 셋업"
echo

yellow "[1/3] git hooks 설정 (core.hooksPath = .githooks)"

CURRENT="$(git config --local --get core.hooksPath || echo '')"
if [ "$CURRENT" = ".githooks" ]; then
    green "  이미 설정됨 — skip"
else
    git config --local core.hooksPath .githooks
    green "  설정 완료"
fi

if [ ! -d .githooks ]; then
    red "  ✘ .githooks 디렉토리가 없습니다. repo 가 손상되었는지 확인하세요."
    exit 1
fi

chmod +x .githooks/* 2>/dev/null || true

echo
yellow "[2/3] 셸 정적 검사기 (shellcheck)"
if bash "$(dirname "$0")/install-shellcheck.sh"; then
    green "  준비됨"
else
    yellow "  ▸ 못 받았다. 배포판 패키지로 받아도 된다 (apt/brew/pacman 의 shellcheck)."
    yellow "    그때까지 pre-commit 의 A.3 이 판정 불가로 멈춘다 — 통과가 아니라 멈춤이다."
fi

echo
yellow "[3/3] 모든 target의 Cargo 의존 (cargo fetch --locked)"
# 플러그인 버전 검사(P.1·B.9)는 cargo tree --target all --offline 으로 다른 OS 전용 의존까지 읽는다.
if ! command -v cargo >/dev/null 2>&1; then
    yellow "  ▸ cargo 가 없다. Rust 를 설치한 뒤 cargo fetch --locked 를 한 번 실행한다."
elif cargo fetch --locked; then
    green "  준비됨"
else
    yellow "  ▸ 못 받았다. 네트워크를 확인하고 cargo fetch --locked 를 다시 실행한다."
    yellow "    그때까지 플러그인 크레이트를 바꾼 커밋의 P.1 이 판정 불가로 멈출 수 있다."
fi

echo
green "✓ 셋업 완료"
echo
echo "이제 git commit / git push 시 자동으로 검사가 실행됩니다."
echo "검사 규칙: docs/dev-guide/git-hooks.md"
echo "긴급 우회: git commit --no-verify / git push --no-verify"
