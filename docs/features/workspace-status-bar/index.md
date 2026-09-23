# 상태바 (Workspace status bar)

- **Status**: Implemented
- **주체**: 로컬 사용자 (GUI 전용)
- **ADR**: 없음
- **코드**: `crates/tasty-ui-widgets/src/status_bar.rs`(view) · `src/adapters/ui/status_bar.rs`(wrapper)
- **화면**: [아래 절](#화면)

## 목적

[작업 영역](../work-area/index.md) 하단의 24px 바. 작업 컬럼 아래 strip 을 항상 차지한다(타이틀바 `top_inset` 과 대칭인 `bottom_inset`). **포커스 surface 의 읽기 전용 요약 + 키보드 리마인더 하나**이고, 어느 항목도 목적지가 아니다 — 포커스를 옮기는 항목이 없다.

표시 항목과 좁은 윈도우에서 접는 순서는 **확정이다**. 항목은 좌측이 git 브랜치 · surface id · shell · grid, 우측이 팔레트 단축키 키캡 · 테마 글리프이고, **값이 없는 항목은 자리째 없다**(dash 를 그리지 않는다). 접는 순서는 아래 "좁은 윈도우" 절에 있다.

## 내부 동작 (headless-valid)

순수 view(`tasty_ui_widgets::draw_status_bar_view`)가 `StatusBarData` + `Theme` 만 받아 주어진 `egui::Ui` 안에 바를 그리고 클릭을 `StatusBarAction` 으로 보고 → wrapper(`draw_status_bar`)가 부유 레이어(`egui::Area`) 생성 · state/engine 데이터 추출 · i18n 라벨 주입 · 액션 적용을 맡는다.

view 는 본체 binary 가 아니라 공용 crate `tasty-ui-widgets` 에 있어 **갤러리 specimen 이 같은 함수를 호출**한다(시각 복제 없음 — [gallery-completeness](../../design/policies/gallery-completeness.md)). Area 와 z-order(`Order::Foreground`, Area Id `workspace_status_bar`)는 본체 정책이라 wrapper 가 소유하며, `gfx/gpu/egui_bridge.rs` 의 배너 z-order 강제가 그 Id 상수를 참조한다.

### 표시 데이터 (focus surface read)

좌측 클러스터는 **현재 focus surface 를 read** 해 표시한다 — 표시 *대상 결정* 이 focus 에 의존하지만 *동작이 아니라 표시* 라 [포커스 독립성](../../identity.md) 에 위배되지 않는다(허용된 조회 read).

- **브랜치 글리프 + 이름** — `git-branch` 글리프(`statusbar-glyph` 잉크)와 브랜치명. focus surface 의 cwd 기준 git 브랜치(`.git` 을 상위로 탐색해 `HEAD` 를 std::fs 로 파싱, git 바이너리/libgit2 비의존·크로스플랫폼). `.git` 이 디렉토리인 일반 repo 와 `gitdir:` 경로를 담은 **파일**인 worktree/submodule 을 모두 지원한다. repo 가 아니면 **항목 자체가 없고**, detached HEAD 면 그 자리에 short sha(`@ 4af6ac9`)가 온다. **`@ ` 표지를 붙이는 것은 그리는 쪽이다** — 파싱은 브랜치인지 detached 인지를 값으로만 돌려주고(`HeadState`), 표지는 상태바 wrapper 한 자리에서 입는다. 그래서 `@4af6ac9` 라는 **이름의 브랜치**는 표지가 덧붙지 않고 그 이름 그대로 나온다. 셸 프로세스명과 마찬가지로 매 프레임 파일 IO 가 아니라 **1Hz busy-poll 캐시**(`CoreState::status_bar_branch`)에서 읽는다 — `git checkout`/`cd` 반영이 최대 1초 늦고(캐시 변화는 redraw 를 유발하므로 터미널을 건드리지 않아도 반영된다), 캐시는 focus surface 한 칸만 갱신한다(headless 는 상태바를 렌더하지 않아 갱신하지 않음).
- **surface id** — focus surface 의 숫자 ID("Copy Terminal ID" 와 동일 값)를 그 surface 를 담은 pane 과 함께 mono 로 찍는다(`s3·p1`). pane 을 못 찾으면 앞마디만(`s3`).
- **shell** 과 **grid** — terminal 한정이고 **서로 독립한 두 항목**이다(포그라운드 프로세스명 · `120×32`). 하나만 있으면 그 하나만 나온다. 프로세스명은 매 프레임 OS 조회가 아니라 1Hz busy-poll 캐시(`CoreState::foreground_name`)에서 읽는다(최대 1초 지연). Windows 에선 셸의 *가장 얕은 non-shell 자손*을 표시 — 선택·플랫폼 메커니즘은 [busy-indicator](../../design/policies/busy-indicator.md). 그리드는 lock-free 핸들 캐시 read.

### 우측 액션 (clickable)

- **팔레트 단축키** — 라벨 단어 없이 **Kbd 키캡**으로만 그린다. 값은 `KeybindingSettings`(`toggle_command_palette`) 에서 오고, 바인딩이 없으면 값이 없는 항목이라 **자리째 없다**. 누르면 명령 팔레트가 토글된다.
- **테마 글리프** — 이름 텍스트 없이 글리프 하나. 누르면 latte ↔ mocha 전환(그 외 테마에서 누르면 latte). 테마 종류는 색이 아니라 글리프가 든다 — light 는 해 모양, dark 는 테마 글리프이고 색은 두 쪽 다 물러난 chrome 잉크(`statusbar-theme-glyph`)다.

### 좁은 윈도우 — 접는 순서

바가 좁아지면 **들어가는 가장 작은 단계**를 고른다. 단계는 다섯이다.

1. grid
2. shell
3. surface id
4. 팔레트 키캡
5. 브랜치 **텍스트**(글리프는 남는다)

**테마 글리프는 어느 단계에서도 안 빠진다.** 5 단계보다 좁으면 거기서 멈춘다 — 더 접을 것이 없다. 브랜치 이름은 접히기 전에 먼저 말줄임된다(항목 전체가 160 을 넘지 않는다).

## 인터페이스

- **사용자 트리거**: 팔레트 키캡 클릭(→ [command-palette](../command-palette/index.md)), 테마 글리프 클릭(→ 테마 설정). 좌측 클러스터는 표시 전용(비클릭). 어느 쪽도 포커스를 옮기지 않는다.
- **AI Agent**: 없음 — 표시 위젯. 표시되는 값(브랜치/그리드 등)은 각 도메인 조회(`tasty list surfaces` 등)로 별도 접근.

## 비-목표 (Out of scope)

- **명령 팔레트 내용** — [command-palette](../command-palette/index.md).
- **테마 정의/적용 규칙** — 테마 시스템(설정).
- **표시 값의 도메인 동작**(터미널 그리드·cwd 추적 등) — 각 기능.

## Acceptance Criteria

- Given focus surface 가 git repo 안 터미널 Then 브랜치 글리프+이름 / `s<sid>·p<pane>` / shell / `<cols>×<rows>` 가 표시된다.
- Given repo 아님 Then 브랜치 항목이 자리째 사라진다(dash 가 남지 않는다).
- Given detached HEAD Then 브랜치 자리에 `@ <short sha>` 가 온다.
- Given 팔레트 단축키 바인딩 없음 Then 키캡 항목이 자리째 사라진다.
- Given 바가 좁아짐 Then grid → shell → surface id → 팔레트 키캡 → 브랜치 텍스트 순으로 빠지고 테마 글리프는 남는다.
- Given 팔레트 키캡 클릭 Then 명령 팔레트가 토글된다.
- Given 테마 글리프 클릭 Then latte ↔ mocha 가 전환되고 글리프가 해 ↔ 테마로 바뀐다.
- Given 팔레트 단축키 설정 변경 Then 키캡 표시가 따라간다.

> GUI 위젯이라 시각은 스크린샷. 표시 값은 `tasty list surfaces` 등 도메인 조회와 대조, 액션 결과(팔레트/테마)는 해당 기능으로 확인.

## 구현

- view(공용 crate): `crates/tasty-ui-widgets/src/status_bar.rs` `draw_status_bar_view(ui, &Theme, width, &StatusBarData) -> StatusBarDrawResult`. 셀 프리미티브(text / branch / kbd / glyph-button)와 축소 판정(`ItemWidths`·`items_at`·`drop_level`)은 이 모듈 private 이고, 판정은 같은 파일의 단위 테스트가 든다 — 순서·"테마는 안 빠진다"·"없는 항목은 gap 도 안 든다". 히트박스까지 이어지는지는 계약 테스트 `crates/tasty-ui-widgets/tests/status_bar_view.rs` 가 본다.
- wrapper(본체): `src/adapters/ui/status_bar.rs` `draw_status_bar`(Area 생성 + focus surface read → 데이터 추출 + i18n 라벨 주입, 액션 적용: 팔레트 intent / 테마 settings), `STATUS_BAR_AREA_ID`/`status_bar_layer_id`(z-order 배선의 단일 진실원), `status_bar_bottom_inset`(= `status_bar_height` 토큰).
- 브랜치 캐시: `src/core/state/branch.rs`(`git_branch` 상위 탐색 + worktree `gitdir:` 추적, `refresh_status_bar_branch`/`status_bar_branch`). 갱신 배선은 `src/app/busy.rs` 의 1Hz `poll_busy_states`(focus surface 만, 변화 시 `mark_dirty`).
- 갤러리 specimen: `crates/tasty-gallery/src/catalog/components/status_bar.rs`(Layouts → Status bar) — 위 view 를 그대로 호출한다. 축소 변종은 단계를 박지 않고 **폭만** 넘긴다(단계는 view 가 정한다).

## 화면

화면정의서 — **상태바 화면**.

- **시각 소스**: Claude Design `gallery/layouts.jsx` 의 **Workspace status bar** 섹션 — 항목·축소 순서·변종 여섯의 정본

[작업 영역](../work-area/index.md#화면) 하단의 24px 바. 동작은 부모 기획, 여기선 시각.

### 트리거

작업 영역이 있으면 항상 하단에 표시(`bottom_inset`).

### UI 요소 인벤토리

```
┌ 상태바 (높이 status_bar_height, 상단 1px separator) ─────────────────┐
│ ⑂ main   s3·p1   zsh   120×32                     [Ctrl]+[K]    ☀ │
│ └브랜치  └surface id └shell └grid                  └팔레트 키캡  └테마 │
└────────────────────────────────────────────────────────────────┘
```

- **바깥 여백 10 · 항목 사이 gap 10**. 셀마다 패딩을 주는 형태가 아니라, 인접한 두 항목 사이는 10 이다.
- **좌측 클러스터**(표시 전용, 순서 고정): `git-branch` 글리프(잉크 `statusbar-glyph` · 크기 `statusbar-glyph-size` → `icon-size-xs` 12) + 이름 · surface id · shell · grid. 바의 인라인 글리프는 **한 자리**에서 크기를 받는다 — 글리프마다·테마마다 갈리지 않는다.
  - **mono 는 둘뿐이다** — surface id(`s3·p1`)와 grid(`120×32`). 브랜치 이름과 shell 은 UI 글꼴이다. 크기는 넷 다 `font-size-caption`.
  - 브랜치 항목은 **전체 폭 160 상한**이고, 넘으면 이름을 말줄임(`…`)한다.
  - `<shell>` 셀은 정확히는 해당 surface 의 **foreground 프로세스명**이다(셸 idle 시엔 셸 이름). Windows 에선 *가장 얕은 non-shell 자손*(사용자가 띄운 바깥쪽 앱, 예: `node`)을 표시한다 — 안쪽 단명 helper 가 아니라. 매 프레임 OS 조회가 아니라 1Hz busy-poll 캐시(`CoreState::foreground_name`)에서 읽으므로 표시는 최대 1초 지연될 수 있다. 판정·플랫폼별 메커니즘은 [busy-indicator](../../design/policies/busy-indicator.md).
- **우측 클러스터**(clickable): 팔레트 단축키 **Kbd 키캡**(라벨 단어 없음) · 테마 **글리프**(이름 없음, 잉크 `statusbar-theme-glyph` · 크기는 좌측과 같은 `statusbar-glyph-size`).
- **상단 1px separator** + `bg_app` 배경.

### 상태별 시각

- **repo / 비-repo** — 브랜치 항목이 통째로 있거나 없다. 없을 때 dash 를 그리지 않는다.
- **detached HEAD** — 브랜치 자리에 `@ <short sha>`.
- **terminal / 비-terminal** — shell 과 grid 는 terminal 한정이고 서로 독립이다.
- **팔레트 바인딩 없음** — 키캡 항목이 자리째 없다.
- **테마 light / dark** — 글리프(해 / 테마). 색은 두 쪽이 같다.
- **hover** — 우측 두 항목만 반응한다(테마 글리프는 잉크가 밝아진다).
- **좁은 윈도우** — grid → shell → surface id → 팔레트 키캡 → 브랜치 텍스트 순으로 빠지고, **테마 글리프는 안 빠진다**. 갤러리 `statusbar` specimen 이 변종 여섯으로 전시한다.

### 시각 소스

Claude Design `gallery/layouts.jsx` 의 **Workspace status bar** 섹션 — 항목 목록·축소 순서·여백·글꼴 구분·색 토큰의 단일 출처.
