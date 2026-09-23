# Surface 위치 이동 (잘라내기 / 여기로 이동)

- **Status**: Implemented
- **주체**: 로컬 사용자 (우클릭 컨텍스트 메뉴 — `잘라내기` → `여기로 이동`)
- **ADR**: 없음
- **코드**: `DomainIntent::MoveSurface` (`src/core/intent.rs`), `Core::apply_move_surface`/`detach_surface_for_move` (`src/core/impl_move.rs`), `SurfaceLayout::extract_surface` (`crates/tasty-model/src/surface_layout.rs`), 슬롯 `CoreState::pending_move_surface` (`src/core/state.rs`)
- **화면**: OS 네이티브 컨텍스트 메뉴 (`PendingNativeMenu::TerminalSurface`/`Surface`)

## 목적

살아있는 surface 를 레이아웃 트리에서 **떼어내 다른 위치로 실제 이동**한다. 스냅샷 저장/복원이 아니라 surface 객체 자체를 옮기므로 terminal 의 PTY·실행 중 프로세스·scrollback 이 그대로 따라간다. 이동은 **replace** 의미다 — 목적지 surface 는 닫히고, 옮겨온 surface 가 그 자리를 차지한다.

## 내부 동작

### 사용자 트리거 (두 단계)

- 어떤 surface 든 "빈 공간"(특정 대상이 없는 영역)을 우클릭하면 `[surface 전용 항목] + 구분선 + [잘라내기] + [여기로 이동]` OS 메뉴가 뜬다. `여기로 이동` 은 **잘라낸 대상이 대기 중일 때만** 나타난다.
- **잘라내기**: 그 surface 의 id 를 세션 단일 슬롯 `pending_move_surface` 에 마킹한다. 도메인 변경이 아니라 UI 핸들러에서 슬롯만 설정 — 사용자 조작이므로 release 경로다.
- **여기로 이동**: 슬롯의 source(A) 를 우클릭한 위치의 target(B) 로 이동시키는 `DomainIntent::MoveSurface { source_surface_id, target_surface_id }` 를 `from_user_context_menu` origin 으로 발행한다.
- surface 종류에 따라 두 생산 경로가 있다 — 타입은 `PendingNativeMenu::TerminalSurface`(terminal, selection-copy 항목이 있어 별도 variant) / `Surface`(비-terminal)로 나뉘지만, "잘라내기"/"여기로 이동" 두 항목은 두 variant 모두에 동일하게 뜬다:
  - **terminal**(winit, `src/view/main/mouse.rs`) — winit 경로는 **terminal 전용**이다. mouse-tracking 위임(ADR-0019/0022) 미해당 시 terminal surface 메뉴를 낸다. 비-terminal 은 winit 이 메뉴를 만들지 않고 egui 프레임에 위임(`return`)한다.
  - **비-terminal surface**(explorer/empty/markdown/image/mesh/webview chrome/remote) — **egui 패널 단일 경로**(`emit_surface_menu_fallback`, `src/adapters/ui/egui_panels.rs`)가 release 시점 `secondary_clicked()` 를 패널 논리 rect 와 대조해 surface 를 식별하고 메뉴를 낸다. explorer 는 예외적으로 `apply_explorer_action` 이 위치별 `Explorer`/`ExplorerFavorite` 를 먼저 슬롯에 선점하며, fallback 은 `is_none()` 가드로 이를 존중한다(한 프레임 한 메뉴, 이중 발화 없음).

### replace 시맨틱과 cascade

`apply_move_surface` 는 두 가지를 한 번에 수행한다:

1. **A detach** (`detach_surface_for_move`) — A 를 트리에서 떼어 살아있는 `Box<dyn Surface>` 로 회수한다. **A 의 Terminal/store/scrollback 은 절대 만지지 않는다**(PTY 보존). A 가 split 안 leaf 면 형제를 끌어올리고, tab/pane/workspace 유일 surface 였으면 그 빈 자리를 `apply_close_surface` 와 동형으로 구조적 cascade(Tab/Pane/Workspace)한다 — 단 A 자신의 `cleanup_surface`/`terminals.remove` 는 일절 없다.
2. **B replace** — target 위치를 id 로 *재탐색*(detach 가 인덱스를 바꿨을 수 있음)한 뒤 B leaf 를 A 로 교체한다. B 의 옛 자리·구조 cascade 와 B 의 Terminal close(PTY kill, closed-item 히스토리 미기록)는 `MoveSurfaceApplied` 이벤트에 실려 `dispatch_surface_closed_cascade`(기존 close cascade 재사용)에서 처리된다.

이동 후 순 surface 수는 1 감소(A,B → A). 범위 제한 없음 — cross-tab/pane/workspace 자유 이동.

### 불변식 / 가드

- **PTY 보존(R1)**: 이동 경로는 source 에 대해 `TerminalStore::remove`/`cleanup_surface` 를 절대 호출하지 않는다. surface_id 가 불변이라 store 가 자동 추종한다. 코어 테스트 `move_surface_tests`(`src/core/impl_move.rs`)가 이 불변식을 고정한다.
- **포커스 독립성**: 모든 조회는 surface_id 기준(focused_* 미사용). 슬롯·이동은 사용자 우클릭 조작이라 포커스 부수효과는 사용자 맥락 안에서만 발생. release 에 포커스 변경 API 없음.
- **가드**: self-ref(source==target)·source 무효(이미 닫힘)·target 무효 → no-op(슬롯만 소비). 구조 증명상 B 는 A detach 후에도 항상 생존하므로 missing-B 분기는 방어적 로깅(`tracing::error!`)만 둔다.

## 비-목표

- 복사(copy)·surface 스냅샷 이동·drag-and-drop UI.
- 잘라내기 시각 피드백(흐림 등) · 이동 전 확인 다이얼로그.
- 에이전트용 IPC/CLI — 잘라내기/이동은 사용자 우클릭 클립보드형 조작이라 GUI 전용이다([convert-surface](../convert-surface/index.md) 의 사용자 전용 팝업과 동궤). 슬롯 `pending_move_surface` 는 사용자 상태다.
- plugin 전용 컨텍스트 항목의 실제 선언 — 빈공간 판정 골격까지만. plugin 컨텍스트 메뉴 protocol 은 이 기능의 범위 밖이다 (UiNode DSL 제거로 선언 방식 재설계 필요).

## 관련

- [work-area](../work-area/index.md)(Surface/Tab/Pane/Workspace 계층) · [convert-surface](../convert-surface/index.md)(in-place replace 선례) · [closed-tab-restore](../closed-tab-restore/index.md)(이동은 closed-item 히스토리에 기록하지 않음) · `docs/identity.md`(불가침 원칙 1: 사용자↔에이전트 행동 분리)
