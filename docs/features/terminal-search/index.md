# 터미널 검색 (Terminal search)

- **Status**: Implemented
- **주체**: 로컬 사용자
- **ADR**: 없음
- **코드**: 엔진 `crates/tasty-terminal/src/search.rs` · 상태 `src/state/search.rs` · 검색 바 `src/adapters/ui/search_bar.rs` · 하이라이트 `src/gfx/`
- **화면**: 검색 바 popup (`PopupScope::Surface`, surface 상단 가로 중앙)

## 목적

터미널 **스크롤백 + 화면 전체**를 텍스트 검색. GPU 렌더러가 매치를 하이라이트하며 현재 매치(active)와 나머지(inactive)를 다른 색으로 구분한다. 두 색(테마 `search_match_active_bg` · `search_match_bg`)은 alpha 로 구분되고, 그 alpha 는 매치 칸의 배경 위에 합성된다([ADR-0460](../../adr/0460-cell-highlight-alpha-is-composited-on-the-cpu-over-the-cell-bg.md)).

## 내부 동작

### find 단축키 = 포커스 토글

`find` 바인딩([keybindings](../keybindings/index.md), Tasty 프리셋 `Ctrl+F`/`Alt+F`)은 **포커스 토글**이다:
- 터미널 포커스 + find → 검색 바 열림 + 검색 바로 포커스
- 검색 바 떠 있고 터미널 포커스 + find → 검색 바로 포커스만 이동
- 검색 바 포커스 + find → 포커스만 터미널로 복귀(검색 바는 유지)

**Escape**: 검색 바가 *포커스된 상태일 때만* 닫기 + 하이라이트 clear + 터미널 복귀. 터미널 포커스 상태의 Escape 는 그대로 PTY 로 전달(검색 바 안 닫음). **Enter**/**Shift+Enter**/**↑↓**: 다음/이전 매치(검색 바 포커스 시). 우측 끝 divider 뒤 **close(X) 버튼**을 마우스로 클릭해도 Escape 와 동일하게 닫힌다(하이라이트 clear 포함) — outside-click 으로는 닫히지 않으므로(입력 포커스 유지를 위해 의도적으로 막음) 마우스만 쓰는 사용자를 위한 유일한 클릭 close 수단이다.

### 검색 옵션 & 표시

우측 토글 3종: `Aa`(대소문자, 기본 off) · `.*`(정규식, Rust `regex`, 기본 off) · `ab`(단어 단위 `\b`, literal/regex 모두). 정규식 컴파일 실패는 매치 0 과 같이 카운터 `0/0` 을 danger 색으로 표시. 매치 카운터(`3/42`), 선택 시 해당 위치로 자동 스크롤.

### 키보드 라우팅 (headless popup, sticky 아님)

검색 바는 일반 headless `PopupDef`(sticky_focus 아님) — 포커스가 터미널이면 떠 있어도 키(Escape 포함)를 가로채지 않고 PTY 로 흘린다. 라우팅 결정자는 `PopupState.focused` 플래그이고 find 단축키가 이걸 토글. 검색 바 포커스 상태의 find 는 winit 단축키 경로(overlay 게이트)에 막히므로, 검색 바 draw fn 안에서 `KeybindingSettings.find` 바인딩을 egui 입력에 직접 매칭해 감지(하드코딩 없음). popup 시스템은 [design/systems/popup](../../design/systems/popup.md).

## 인터페이스

- **사용자**: `find` 단축키 토글, 검색 바 입력/탐색. (검색은 사용자 동작 — release IPC/CLI 비노출.)

## 관련

- [terminal](../terminal/index.md) · [keybindings](../keybindings/index.md) · [design/systems/popup](../../design/systems/popup.md)
