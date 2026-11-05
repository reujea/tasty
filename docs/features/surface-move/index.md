# Surface·Tab·Pane 위치 이동 (이동 / 이곳으로 이동)

- **Status**: Implemented
- **주체**: 로컬 사용자 (우클릭 컨텍스트 메뉴 — `서피스 이동` → `서피스를 이곳으로 이동`, 탭 헤더 `탭 이동` → `탭을 이곳으로 이동`, 탭 헤더 `페인 이동` → `페인을 이곳으로 이동`)
- **설계**: 없음
- **코드**: 명시 이동 요청 `src/app/command.rs`, journal 이동·교체 해석 `src/app/journal/commands/replacement.rs`, 순수 구조 교체 `crates/tasty-core/src/replacement.rs`, 원 자원 회수 `src/runtime/resource_retirement.rs`, 사용자 선택 후처리 `src/app/journal/commands/view_completion.rs`.
- **화면**: OS 네이티브 컨텍스트 메뉴 (`PendingNativeMenu::TerminalSurface`/`Surface`/`Tab`), 대기 대상의 점선 링과 move 글리프([이동 대기 표시](#이동-대기-표시)). 갤러리 Layouts › Move source highlight

## 목적

살아있는 surface 를 레이아웃 트리에서 **떼어내 다른 위치로 실제 이동**한다. 스냅샷 저장/복원이 아니라 surface 객체 자체를 옮기므로 terminal 의 PTY·실행 중 프로세스·scrollback 이 그대로 따라간다. 이동은 **replace** 의미다 — 목적지 surface 는 닫히고, 옮겨온 surface 가 그 자리를 차지한다.

## 내부 동작

### 사용자 트리거 (두 단계)

- 어떤 surface 든 "빈 공간"(특정 대상이 없는 영역)을 우클릭하면 `[surface 전용 항목] + 구분선 + [서피스 이동] + [서피스를 이곳으로 이동]` OS 메뉴가 뜬다. `서피스를 이곳으로 이동` 은 **대기 슬롯에 surface 가 있을 때만** 나타난다. 대기 중인 것이 없거나 다른 종류면 숨긴다.
- **서피스 이동**: 그 surface 의 id 를 단일 대기 슬롯 `pending_move` 에 `PendingMove::Surface(id)` 로 넣고, 그 surface 영역(`ToastScope::Surface`)에 "서피스를 잘라냈습니다…" Info 토스트(`toast.surface_cut`)를 띄운다. 도메인 변경이 아니라 UI 핸들러에서 슬롯만 설정 — 사용자 조작이므로 release 경로다.
- **대기 슬롯**: 종류(`PendingMove::Surface`/`Tab`/`Pane`)와 ID 를 함께 담는 슬롯 하나뿐이다. 새로 이동을 지정하면 종류와 관계없이 이전 대기를 덮어쓴다. 저장하지 않으며, 이동 요청을 실행하면(성공 여부와 무관) 비운다. 대상이 닫혀 어느 워크스페이스에서도 찾을 수 없으면 다음 GUI 프레임에서 비운다(`move_source::clear_if_target_closed`) — 닫힌 대상의 표시와 "이곳으로 이동" 항목이 남지 않는다. 대기 해제 조작은 없다. `CoreState` 가 윈도우마다 있으므로 슬롯도 윈도우별이며, 윈도우를 넘는 이동은 지원하지 않는다.
- **서피스를 이곳으로 이동**: 슬롯의 source(A) 를 우클릭한 위치의 target(B) 로 이동시키는 `DomainIntent::MoveSurface { source_surface_id, target_surface_id }` 를 `from_user_context_menu` origin 으로 발행한다.
- surface 종류에 따라 두 생산 경로가 있다 — 타입은 `PendingNativeMenu::TerminalSurface`(terminal, selection-copy 항목이 있어 별도 variant) / `Surface`(비-terminal)로 나뉘지만, "서피스 이동"/"서피스를 이곳으로 이동" 두 항목은 두 variant 모두에 동일하게 뜬다:
  - **terminal**(winit, `src/view/main/mouse.rs`) — winit 경로는 **terminal 전용**이다. mouse-tracking 위임(docs/surfaces/terminal/index.md) 미해당 시 terminal surface 메뉴를 낸다. 비-terminal 은 winit 이 메뉴를 만들지 않고 egui 프레임에 위임(`return`)한다.
  - **비-terminal surface**(explorer/empty/markdown/image/mesh/webview chrome/remote) — **egui 패널 단일 경로**(`emit_surface_menu_fallback`, `src/adapters/ui/egui_panels.rs`)가 release 시점 `secondary_clicked()` 를 패널 논리 rect 와 대조해 surface 를 식별하고 메뉴를 낸다. explorer 는 예외적으로 `apply_explorer_action` 이 위치별 `Explorer`/`ExplorerFavorite` 를 먼저 슬롯에 선점하며, fallback 은 `is_none()` 가드로 이를 존중한다(한 프레임 한 메뉴, 중복 메뉴 없음).

### replace 시맨틱과 cascade

`apply_move_surface` 는 두 가지를 한 번에 수행한다:

1. **A detach** (`detach_surface_for_move`) — A 를 트리에서 떼어 살아있는 `Box<dyn Surface>` 로 회수한다. **A 의 Terminal/store/scrollback 은 절대 만지지 않는다**(PTY 보존). A 가 split 안 leaf 면 형제를 끌어올리고, tab/pane/workspace 유일 surface 였으면 그 빈 자리를 `apply_close_surface` 와 동형으로 구조적 cascade(Tab/Pane/Workspace)한다 — 단 A 자신의 `cleanup_surface`/`terminals.remove` 는 일절 없다.
2. **B replace** — target 위치를 id 로 *재탐색*(detach 가 인덱스를 바꿨을 수 있음)한 뒤 B leaf 를 A 로 교체한다. B 의 옛 자리·구조 cascade 와 B 의 Terminal close(PTY kill, closed-item 히스토리 미기록)는 `MoveSurfaceApplied` 이벤트에 실려 `dispatch_surface_closed_cascade`(기존 close cascade 재사용)에서 처리된다.

이동 후 순 surface 수는 1 감소(A,B → A). 로컬에서는 tab·pane·workspace를 넘어 이동할 수 있다. mirror에서는 같은 mirror workspace 안의 이동만 원격에 전달하며 로컬과 원격 경계를 넘는 이동은 막는다([원격 구조 변경](../remote-attach/index.md#mirror-워크스페이스-내-구조-변경)).

### 불변식 / 가드

- **PTY 보존(R1)**: 이동 경로는 source 에 대해 `TerminalStore::remove`/`cleanup_surface` 를 절대 호출하지 않는다. surface_id 가 불변이라 store 가 자동 추종한다. 구조 이동은 `crates/tasty-core/src/replacement.rs`와 runtime collection의 같은 ID를 사용한다. 실제 자원 보존은 별도의 runtime 시험으로 확인한다.
- **포커스 독립성**: 모든 조회는 surface_id 기준(focused_* 미사용). 슬롯·이동은 사용자 우클릭 조작이라 포커스 부수효과는 사용자 맥락 안에서만 발생. release 에 포커스 변경 API 없음.
- **가드**: self-ref(source==target)·source 무효(이미 닫힘)·target 무효 → no-op(대기 슬롯만 비움). 구조 증명상 B 는 A detach 후에도 항상 생존하므로 missing-B 분기는 방어적 로깅(`tracing::error!`)만 둔다.

## 탭 이동

탭 헤더 우클릭 메뉴 끝에 구분선과 `탭 이동` 이 항상 붙는다. `탭을 이곳으로 이동` 은 대기 슬롯이 `PendingMove::Tab(id)` 이고 그 id 가 우클릭한 탭이 아닐 때만 붙는다. 메뉴를 여는 순간 우클릭한 탭의 `Tab::id` 를 잡아 두고, 메뉴가 닫힌 뒤에도 인덱스가 아니라 이 ID 로 대상을 찾는다. 그 사이 탭이 닫혔으면 아무것도 하지 않는다.

- **탭 이동**: 슬롯에 `PendingMove::Tab(tab_id)` 를 넣고 그 탭의 페인 영역(`ToastScope::Pane`)에 "탭을 잘라냈습니다…" Info 토스트(`toast.tab_cut`)를 띄운다.
- **탭을 이곳으로 이동**: 슬롯을 비우고 `DomainIntent::ReplaceTabWithTab { source_tab_id, target_tab_id }` 를 `from_user_context_menu` origin 으로 발행한다.

`apply_replace_tab_with_tab` 은 서피스 이동과 같은 replace 의미다.

1. **source detach** — source `Tab` 객체를 그대로 떼어 낸다(탭 ID·이름·surface ID·Terminal·scrollback 유지, Terminal store 미접촉). 같은 페인에 다른 탭이 있으면 탭만 빠지고(활성 탭은 같은 탭을 계속 가리킴), 유일 탭이면 페인을 닫고(`close_pane_preserving_focus`), 워크스페이스의 유일 페인이면 워크스페이스를 제거한다.
2. **target replace** — target 탭을 ID 로 다시 찾아(같은 페인에서 source 가 앞에 있었다면 인덱스가 줄어듦) 같은 인덱스에 source 탭을 넣는다. target 이 활성 탭이었다면 옮긴 탭이 활성 탭이 된다. 교체된 target 탭의 모든 surface(deferred 포함)는 `collect_close_targets` 로 모아 `ContainerMoveApplied.cleanup_targets` 에 싣고, 기존 close cascade 가 PTY 종료·자원 정리를 한다. 닫은 항목 히스토리에는 남기지 않는다.

이벤트의 닫힌 탭 목록에는 target 탭만 들어가고 source 탭은 들어가지 않는다. source 가 떠나 사라진 페인·워크스페이스는 닫힌 목록에 들어간다. `tab.closed` 알림은 target 탭이 있던 페인(`closed_tabs_pane`)으로 나가며, source 페인이 함께 사라져도 그 페인 소속으로 잘못 나가지 않는다. 옮긴 탭의 `tab.moved` 는 탭 위치 변화를 보는 lifecycle 감지가 한 번 낸다(같은 페인 안 교체면 나지 않는다).

source 또는 target 워크스페이스가 mirror 면 로컬 실행을 막는다(`mirror_workspace_index_for_structural`). 원격으로 전달하는 `StructuralOp` 는 없어 mirror 안의 탭 이동은 지원하지 않는다.

## 페인 이동

탭 헤더 우클릭 메뉴의 탭 이동 항목 뒤에 구분선과 `페인 이동` 이 붙는다. 대상은 우클릭한 탭이 지금 속한 페인이다(메뉴를 열 때 잡은 `Tab::id` 로 찾는다). `페인을 이곳으로 이동` 은 대기 슬롯이 `PendingMove::Pane(id)` 이고 그 id 가 우클릭한 탭의 페인이 아닐 때만 붙는다. 페인 이동 메뉴는 탭 헤더에서만 연다.

- **페인 이동**: 슬롯에 `PendingMove::Pane(pane_id)` 를 넣고 그 페인 영역에 "페인을 잘라냈습니다…" Info 토스트(`toast.pane_cut`)를 띄운다.
- **페인을 이곳으로 이동**: 슬롯을 비우고 `DomainIntent::ReplacePaneWithPane { source_pane_id, target_pane_id }` 를 `from_user_context_menu` origin 으로 발행한다.

`apply_replace_pane_with_pane` 도 replace 의미다.

1. **source detach** — 워크스페이스에 다른 페인이 있으면 `Workspace::detach_pane_preserving_focus` → `PaneNode::detach_pane` 이 source `Pane` 을 그대로 떼어 돌려주고 형제가 그 자리를 채운다(떠난 페인이 포커스 대상이었으면 남은 첫 페인으로 포커스 보정). 유일 페인이면 워크스페이스를 제거하고 그 안의 페인을 꺼낸다. 페인 ID·탭·surface·Terminal·scrollback 은 그대로다.
2. **target replace** — target 페인을 ID 로 다시 찾아 `PaneNode::replace_pane` 으로 같은 leaf 자리(분할 방향·비율 유지)에 source 페인을 넣는다. target 워크스페이스의 `focused_pane` 이 target 이었으면 옮긴 페인으로 바꾼다. 교체된 페인의 모든 탭 surface 를 탭마다 `collect_close_targets` 로 모아 정리 대상으로 싣는다. 닫은 항목 히스토리에는 남기지 않는다.

이벤트의 닫힌 페인 목록에는 target 페인만 들어가고 source 페인은 들어가지 않는다. 닫힌 탭 목록은 target 페인의 탭들이며 `tab.closed` 는 target 페인 소속으로 나간다. 옮긴 페인의 탭들은 pane ID 가 같아 `tab.moved` 를 내지 않고, 페인 이동 전용 호스트 이벤트는 없다. 레이아웃은 `mark_layout_dirty` 로 다음 패스에서 새 rect 를 받는다.

source 또는 target 워크스페이스가 mirror 면 로컬 실행을 막는다. 원격으로 전달하는 `StructuralOp` 는 없다.

## 이동 대기 표시

슬롯이 가리키는 대상을 화면에서 알아볼 수 있게 한다. 대상은 슬롯의 ID로 모든 워크스페이스에서 찾는다(`move_source::resolve`·`workspace_cue`). 포커스나 활성 탭으로 대상을 고르지 않는다.

- **링**: 대상 rect **안쪽**에 대시 링을 그린다. 2px(`move_source_ring_width` = `focus_ring_width`), 4px 선·4px 간격(`move_source_dash`·`move_source_dash_gap`), 색 `move_source_ring()` = `accent_move()`(pink). 선 전체를 rect 안에 두어 이웃과 공유하는 1px 구분선을 덮지 않는다. 변마다 대시 위상을 새로 시작한다. 정적 표시이며 입력을 받지 않는다.
- **rect**: 서피스는 서피스 영역, 탭은 탭 칸(오른쪽 구분선 포함), 페인은 탭 바와 콘텐츠를 합친 페인 rect다. 페인 링은 탭 바를 감싸고 서피스 링은 감싸지 않아 세 종류가 구별된다. 탭 링은 활성·비활성·hover 상태와 관계없이 칸 위에 그린다.
- **보이지 않는 대상**: 대상에서 위로 올라가 처음 보이는 컨테이너 하나에만 move 글리프(`move_source_glyph()`, 12px `move_source_glyph_size`)를 둔다. 텍스트는 없다.
  - 대상 서피스가 활성 워크스페이스의 비활성 탭 안에 있으면 그 탭 칸 오른쪽 상태 묶음의 move 칸(html 스크립트 표지와 busy 점 사이). 제목이 먼저 줄어든다([탭 칸 구성](../workspace-tabs/index.md#ui-요소-인벤토리)).
  - 대상(종류 무관)이 다른 워크스페이스에 있으면 사이드바의 그 워크스페이스 행(이름 뒤, 배지 묶음 앞). 접힌 레일에서는 아바타 왼쪽 아래 12px 칩(`move_source_chip_size`, 글리프 8px `move_source_chip_glyph_size`, 바탕 `bg_sidebar`). 오른쪽 위는 알림 점, 오른쪽 아래는 mirror 칩이 쓴다.
  - 탭 링이나 탭 칸 글리프를 둘 대상 탭 칸이 탭 바 스크롤에 가려지면, 가려진 쪽 스크롤 화살표가 `tab_scroll_arrow_move_fg()`(= `move_source_glyph()`) 잉크로 바뀐다. 칸이 viewport 안에 완전히 들어와야 보이는 것으로 치며, 일부만 보여도 화살표가 표시를 유지한다. 그쪽 화살표는 disabled가 되지 않고, 클릭은 평소의 한 칸 스크롤이다(대상으로 건너뛰지 않는다). 스크롤은 여전히 활성 탭만 따라간다. 판정은 `tab_bar/view.rs::hidden_move_side`다. 대상 서피스가 활성 탭 안에 있으면 탭 칸 표시가 아니라 서피스 링이 콘텐츠 영역에 보이므로, 그 탭 칸이 스크롤에 가려져도 화살표 표시는 하지 않는다.
  - 그 밖에 활성 워크스페이스에 있지만 보이지 않는 대상에는 대체 표시가 없다.
    - rect의 폭이나 높이가 링 두께보다 작으면 링을 그리지 않는다.
    - 대상 페인이 그 프레임의 페인 rect 목록에 없으면 아무것도 그리지 않는다. 활성 워크스페이스의 페인은 모두 그려지므로 지금은 이 경우가 생기지 않는다.
- **겹침**: 링은 대상 rect에서 가장 마지막에 그린다. 서피스·페인 링은 대상 페인의 탭 바 레이어(`tab_bar::pane_tab_bar_layer`, Foreground Area)에 탭 바·점유 테두리 뒤에 그리므로 포커스 배경·점유 오버레이·알림(Middle 레이어)·점유 테두리·활성 탭 표시 위에 온다. 대시가 덮고 간격으로 아래 테두리가 보인다(우선순위 판정 없음). 점유 테두리도 같은 레이어에 그리므로 두 표시 모두 팝업 아래에 남는다.
- **수명**: 지정하면 나타나고, 이동 실행·다른 대상 지정·대상 닫힘에 사라진다. 포커스·탭·워크스페이스 전환에는 유지한다.
- **대비**: Latte pink는 밝은 배경에서 약 2.5:1이다. [테마 규칙](../../design/systems/theme.md)의 4.5:1은 텍스트 대비 규칙이며, 링의 의미는 대시 패턴이 전달한다.

## 비-목표

- 복사(copy)·surface 스냅샷 이동·drag-and-drop UI.
- 이동 전 확인 다이얼로그.
- 대기 중인 대상을 흐리게 표시하기 — 대상은 대기 중에도 그대로 쓸 수 있고, 흐림은 비포커스 서피스의 흐림과 겹친다. 목적지 후보 표시 — 다른 모든 대상이 후보라 표시할 정보가 없다.
- 에이전트용 IPC/CLI — 이동 지정/이곳으로 이동은 사용자 우클릭 클립보드형 조작이라 GUI 전용이다([convert-surface](../convert-surface/index.md) 의 사용자 전용 팝업과 같은 원칙). 슬롯 `pending_move` 는 사용자 상태다.
- plugin 전용 컨텍스트 항목의 실제 선언 — 빈공간 판정 골격까지만. plugin 컨텍스트 메뉴 protocol 은 이 기능의 범위 밖이다 (UiNode DSL 제거로 선언 방식 재설계 필요).

## 관련

- [work-area](../work-area/index.md)(Surface/Tab/Pane/Workspace 계층) · [convert-surface](../convert-surface/index.md)(in-place replace 선례) · [closed-tab-restore](../closed-tab-restore/index.md)(이동은 closed-item 히스토리에 기록하지 않음) · [포커스 정책](../../design/policies/focus.md)(사용자↔에이전트 행동 분리)
