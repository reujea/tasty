# Model + Host View 분리 패턴

`tasty-model` 의 surface 모델은 **GUI-free** 다 (egui/wgpu 직접 사용 금지). 휘발성 GUI 상태(텍스처·캐시·편집 세션·스크롤·팝업 버퍼)는 호스트 측 **View** 구조체에 둔다. host 가 egui 로 직접 그리는 내장 surface 에 뷰 상태를 더할 때 이 패턴을 따른다.

`tasty-model` 이 의존하는 type-\* crate 는 `tasty-type-geometry`(LogicalPx/PhysicalPx)와 `tasty-type-appearance` 다 — 둘 다 GUI 타입을 들이지 않는다.

## 왜 분리하나

- **플러그인 호환성** — 모델은 직렬화 가능한 식별 정보만 보유 → plugin 프로세스가 같은 모델을 그대로 쓸 수 있다.
- **테스트 용이성** — 모델 단위 테스트가 GUI 컨텍스트 없이 가능.
- **정리 일관성** — View 가 store 에 모이면 surface 닫힘 시 한 곳에서 일괄 해제.

> 적용 대상은 **host 내장 surface**(host 가 egui 로 그리는 surface, 현재 explorer·dag_graph·empty)다. `image` 같은 **egui-mesh plugin surface** 와 `html`/`markdown`([ADR-0065](../adr/0065-markdown-webview-render-channel.md)) 같은 **webview plugin surface** 는 plugin 프로세스가 자기 상태를 들고 그리므로 이 패턴 밖이다 (→ [concepts/plugins](../concepts/plugins.md)).

## 어디에 무엇을 두나

| 종류 | 위치 | 예 |
|------|------|-----|
| 식별 정보 | model | `id`, `tabs`, `active`, `dag_id` |
| 직렬화 영속 상태 | model | mtime, 트리 구조 |
| egui 타입 | view | `egui::ColorImage`, `TextureHandle` |
| 편집 세션 머신 | view | `EditState`, `DragState`, `ActionHistory` |
| 휘발성 UI 버퍼 | view | popup 텍스트 버퍼, scroll offset, brush 설정 |

판단 기준: **"이 상태를 plugin 프로세스가 들고 있을 이유가 있는가?"** 없으면 view, 있으면 model.

## 패턴

### 1. Model — 슬림 식별·탐색 정보 (`crates/tasty-model/src/`)

```rust
pub struct FooPanel { pub id: u32, pub file_path: String, last_mtime: Option<SystemTime> }
impl FooPanel {
    pub fn poll_reload(&mut self) -> Option<String> { /* 외부 변경 감지 시 새 콘텐츠 */ }
}
impl Surface for FooPanel {        // crates/tasty-model/src/surface_trait.rs
    fn kind(&self) -> &'static str { "foo" }
    /* ... */
}
```

### 2. View + Store — 호스트 측 (`src/adapters/ui/surface/<foo>/view.rs`)

```rust
pub struct FooView { pub content: String, pub texture: Option<egui::TextureHandle> }

#[derive(Default)]
pub struct FooViewStore { views: HashMap<SurfaceId, FooView> }
impl FooViewStore {
    pub fn get_or_init(&mut self, panel: &mut FooPanel) -> &mut FooView {
        let view = self.views.entry(panel.id).or_insert_with(|| FooView::new(panel));
        if let Some(c) = panel.poll_reload() { view.replace_content(c); } // 첫 접근/변경 자동 반영
        view
    }
    pub fn drop_view(&mut self, sid: SurfaceId) { self.views.remove(&sid); }
}
```

### 3. AppState 등록 + 정리 (`src/state.rs`)

```rust
pub struct AppState { /* ... */ pub(crate) foo_views: FooViewStore }

pub(crate) fn cleanup_surface(&mut self, surface_id: u32) {
    /* ... */
    self.foo_views.drop_view(surface_id);  // ← 누락하면 close/reopen 시 텍스처/캐시 누수
}
```

### 4. 렌더 호출 — `mem::take` 패턴

디스패치 루프에서 `&mut FooPanel`(engine.workspaces 경로)와 `&mut FooView`(state.foo_views 경로)를 동시에 mutable 보유하려면 borrow checker 가 막는다. 루프 진입 직전 store 를 일시 추출:

```rust
let mut foo_views = std::mem::take(&mut state.foo_views);
for info in &infos {
    if let Some(panel) = surface.as_foo_mut() {
        let view = foo_views.get_or_init(panel);
        draw_foo(ui, panel, view);
    }
}
state.foo_views = foo_views;   // 반드시 복원 (이후 state 접근 전에)
```

이 패턴은 `src/adapters/ui/egui_panels.rs`(메인 디스패치)에서 쓰인다.

## 안티패턴

- **Model 에 `egui::*` 필드** — plugin 호환성을 깬다. View 로.
- **store `drop_view` 누락** — close/reopen 누수. `cleanup_surface` 로 강제.
- **`mem::take` 후 복원 누락** — 다음 프레임 빈 store → 전 view 재생성 → flicker/상태 손실.
- **panel↔view 양방향 의존** — view 는 panel 을 읽지만 panel 은 view 를 모른다.

## 현재 적용된 host surface

| Model | View | Store |
|-------|------|-------|
| `ExplorerPanel` (id, tabs, active) | `ExplorerView` (entries, …) | `AppState::explorer_views` |
| `DagGraphSurface` (id, dag_id, workspace_id, direction) | `DagGraphView` (data, layout cache, …) | `AppState::dag_graph_views` |
| `TerminalSurface` / `EmptySurface` | (없음 — GPU 렌더 또는 id-only) | — |

신규 host surface 추가 시 이 표에 줄을 더한다. plugin surface(`image`/`html`/`markdown`)는 여기 들어오지 않는다.
