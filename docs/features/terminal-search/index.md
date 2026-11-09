# 터미널 검색 (Terminal search)

- **Status**: Implemented
- **주체**: 로컬 사용자
- **설계**: 없음
- **코드**: 엔진 `crates/tasty-terminal/src/search.rs` · 상태 `src/state/search.rs` · 검색 바 `src/adapters/ui/search_bar.rs` · 하이라이트 `src/gfx/`
- **화면**: 검색 바 popup (`PopupScope::Surface`, surface 상단 가로 중앙)

## 목적

일반 화면에서는 **스크롤백 + 화면 전체**를 텍스트 검색한다. vim·less 같은 프로그램의
대체 화면에서는 그 화면의 현재 grid만 검색하며, 숨겨진 기본 화면의 기록은 결과에 섞지 않는다. GPU 렌더러가 매치를 하이라이트하며 현재 매치(active)와 나머지(inactive)를 다른 색으로 구분한다. 일반 매치 안의 기본 글자는 `text-primary`로, 프로그램이 색을 고른 글자는 그 색으로 그린다([터미널 색상](../../surfaces/terminal/index.md#색상--폰트)). 두 색(테마 `search_match_active_bg` · `search_match_bg`)은 alpha 로 구분되고, 그 alpha 는 매치 칸의 배경 위에 합성된다([테마 시스템 (운영 상세)](../../design/systems/theme.md)). 현재 매치 칸의 글리프는 셀 자신의 글자색(SGR 포함) 대신 테마 `search_match_active_fg` 로 그린다. 일반 매치에서 자기 글자색을 유지하는 셀은 프로그램이 색을 고른 셀이다. 내장 값은 Mocha 가 노랑 70% + base 잉크, Latte 가 노랑 50% + text 잉크다(bg-panel 위 대비 Mocha 6.95 · Latte 4.68). 테마 파일에 이 키가 없으면 Mocha 기본값 `#1e1e2e` 를 쓴다. 설정 › 외관의 색 조정 목록에는 아직 이 키가 없다. 선택과 겹친 매치의 글자와 바탕은 [터미널 색상](../../surfaces/terminal/index.md#색상--폰트)에 따로 있다.

## 내부 동작

검색 결과는 단일 `TerminalReadView`에서 읽은 content epoch와 안정된 행 좌표로
보관한다. 결과로 이동할 때 현재 내용 cut에서 살아 있는지 확인하고 View의 viewport만
변경한다. renderer는 다른 epoch의 결과를 강조하지 않는다.


### find 단축키 = 포커스 토글

`find` 바인딩([keybindings](../keybindings/index.md), Tasty 프리셋 `Ctrl+F`/`Alt+F`)은 **포커스 토글**이다:
- 터미널 포커스 + find → 포커스된 surface에 검색 바 열림 + 검색 바로 포커스
- 같은 surface에 검색 바가 떠 있고 터미널 포커스 + find → 검색 바로 포커스만 이동(검색어·결과 유지)
- 검색 바가 다른 surface에 열려 있음(다른 탭이라 숨은 경우, 같은 탭의 다른 분할 surface) + find → 그 검색을 비우고 검색 바를 현재 surface로 옮겨 포커스. 닫았다 다시 열지 않고 범위만 옮긴다.
- 검색 바 포커스 + find → 포커스만 터미널로 복귀(검색 바는 유지)
- explorer 포커스 + find → 터미널 검색 대신 그 explorer 의 Find 바를 열고 입력에 포커스([Explorer 찾기](../../surfaces/explorer/index.md#찾기--하위-폴더-검색))

OS 메뉴의 찾기와 탭 스트립의 검색 버튼도 같은 규칙을 따른다.

**Escape**: 검색 바가 *포커스된 상태일 때만* 닫기 + 하이라이트 clear + 터미널 복귀. 터미널 포커스 상태의 Escape 는 그대로 PTY 로 전달(검색 바 안 닫음). **Enter**/**Shift+Enter**/**↑↓**: 다음/이전 매치(검색 바 포커스 시). 우측 끝 divider 뒤 **close(X) 버튼**을 마우스로 클릭해도 Escape 와 동일하게 닫힌다(하이라이트 clear 포함) — outside-click 으로는 닫히지 않으므로(입력 포커스 유지를 위해 의도적으로 막음) 마우스만 쓰는 사용자를 위한 유일한 클릭 close 수단이다.

### 검색 옵션 & 표시

우측 토글 3종: `Aa`(대소문자, 기본 off) · `.*`(정규식, Rust `regex`, 기본 off) · `ab`(단어 단위 `\b`, literal/regex 모두). 정규식 컴파일 실패는 매치 0 과 같이 카운터 `0/0` 을 danger 색으로 표시. 입력 칸은 `input-height`(배율 1에서 28px)를 그대로 쓴다. 셸 sizer는 입력 높이와 상하 `popup-content-margin` 패딩을 더해 매 프레임 할당 높이를 정한다. 배율 1에서 본문은 28px, 관리자 할당은 36px, 바깥에 그리는 1px 테두리를 포함한 외곽은 38px다. 포커스 링은 셸 패딩 안에 그리며 본문 높이로 자르지 않는다. 색은 공용 `Input`의 `Ground::Raised`로 `input-raised-bg` 채움과 `input-raised-border` 휴지·호버 테두리를 쓴다. 정상 포커스는 `border-focus` 테두리와 1px 링이다. 정규식 오류가 있으면 포커스 유무와 관계없이 `accent-danger` 테두리를 유지하고, 포커스가 있으면 같은 색 링을 더한다. 매치 카운터(`3/42`), 선택 시 해당 위치로 자동 스크롤.

### 키보드 라우팅 (headless popup, sticky 아님)

검색 바는 일반 headless `PopupDef`(sticky_focus 아님) — 포커스가 터미널이면 떠 있어도 키(Escape 포함)를 가로채지 않고 PTY 로 흘린다. 라우팅 결정자는 `PopupState.focused` 플래그이고 find 단축키가 이걸 토글. 검색 바 포커스 상태의 find 는 winit 단축키 경로(overlay 게이트)에 막히므로, 검색 바 draw fn은 그리기 전의 `popup_keyboard_focus` 스냅샷으로 포커스를 읽는다. 그리기 중 팝업 관리자는 별도 대여되므로, 터미널 복귀는 `PopupAction::ReleaseFocus`를 반환하고 관리자를 복원한 뒤 적용한다. 검색 바 draw fn 안에서 `KeybindingSettings.find` 바인딩을 입력칸보다 먼저 egui 입력에서 소비(`consume_binding_egui`)해 감지한다(하드코딩 없음). 그 키 누름이 만든 글자도 함께 지우므로 `Alt+F` 같은 조합이 검색어에 `f` 를 넣지 않는다. popup 시스템은 [design/systems/popup](../../design/systems/popup.md).

## 인터페이스

- **사용자**: `find` 단축키 토글, 검색 바 입력/탐색. (검색은 사용자 동작 — release IPC/CLI 비노출.)

## 관련

- [terminal](../../surfaces/terminal/index.md) · [keybindings](../keybindings/index.md) · [design/systems/popup](../../design/systems/popup.md)
