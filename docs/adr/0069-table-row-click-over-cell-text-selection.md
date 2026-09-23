# ADR-0069: 공용 `Table` 의 `selectable` 표는 셀 텍스트 선택을 포기하고 행 클릭을 보장한다

- **Status**: Accepted
- **Date**: 2026-08-13
- **Tags**: ui, shared-widgets, table, egui, hit-test, selectable-labels, explorer, port-scanner, gallery
- **Group**: ui-theme-gallery

## Context

공용 `Table`(`crates/tasty-ui-widgets/src/table.rs`)은 `selectable(true)` 일 때 행 전체가
클릭 타겟이라는 계약으로 설계됐다. 행 클릭·우클릭 신호는 egui_extras 가 셀 `Ui` 자체에
부여한 sense 에서 나온 `tr.response()` **하나**에서 파생되고, 소비처는 이 신호로
선택 / 컨텍스트 메뉴 / 더블클릭 열기를 모두 처리한다.

그런데 셀 내용은 호출자 클로저가 자유롭게 그리고, 소비처 4곳 전부 `ui.label(...)` 로
텍스트를 그린다(explorer detail 뷰, port scanner 팝업 표, 갤러리 specimen 2종).
egui 는 `Label` 에 `sense` 가 명시되지 않으면 `ui.style().interaction.selectable_labels`
(기본값 **true**)를 보고 `Sense::click_and_drag()` 를 라벨 rect 에 등록한다. 라벨은 셀 `Ui`
보다 **나중에** 등록되므로 hit-test 동률에서 앞서고(`egui::hit_test` — "in case of a tie,
take the last one"), 뒤쪽 위젯 우선 예외(`should_prioritize_hits_on_back`)는 뒤쪽이 훨씬
얇을 때만 걸리는데 여기서 뒤쪽은 행 전체라 해당하지 않는다.

결과: **글자 위에서는 행 클릭이 죽는다.** explorer detail 뷰에서 파일 이름 글자 위에
포인터를 올리면 커서가 I-beam 이 되고, 드래그는 텍스트 선택 하이라이트가 되며, 클릭해도
파일이 선택되지 않았다 — 사용자가 가장 자연스럽게 겨냥하는 위치에서 선택·컨텍스트 메뉴·
더블클릭 열기가 통째로 동작하지 않았다. grid/list 뷰와 사이드바 트리는 텍스트를
`painter.galley` 로 직접 그려(위젯 등록 없음) 같은 문제가 없다 — 즉 이 결함은
`Table` 셀에서 `ui.label` 을 쓰는 경로 고유의 것이고, 소비처마다 개별로 밟게 되어 있다.

레포에는 이미 같은 함정을 국소적으로 회피한 선례가 3곳 있다(`remote_tool`,
`port_scanner` 헤더, `remote_attach`). 모두 개별 팝업이 각자 `selectable_labels = false`
를 세운 형태로, 공용 위젯 계약으로는 승격돼 있지 않았다.

## Decision

`Table::show` 가 `selectable(true)` 일 때, **본문 셀 서브트리에서**
`interaction.selectable_labels = false` 를 강제한다. 즉 행 선택 모드의 표에서는 셀
텍스트의 드래그 선택/복사를 포기하고 행 클릭을 보장하는 것을 공용 위젯 레벨의 계약으로
못 박는다. 소비처가 셀에 무엇을 그리든 이 정합이 깨지지 않으며, 호출자 쪽 개별 회피는
필요 없다.

적용 범위는 본문 셀로 한정한다. 헤더 셀은 손대지 않는다 — 정렬 가능 헤더 라벨은 명시
`Sense::click()` 을 쓰고, 명시 sense 는 `selectable_labels` 와 무관하게 유지되므로 정렬
클릭은 그대로다. `selectable(false)` 인 표(행 클릭을 소비하지 않는 읽기 전용 표)는 egui
기본값을 유지해 셀 텍스트를 선택할 수 있다.

## Consequences

- **얻은 것**: `Table` 소비처 전체(explorer detail, port scanner, 갤러리 specimen 2종)가
  한 번에 정합된다. 행 선택 모드에서 "행 전체가 클릭 타겟"이 위젯 계약으로 성립하므로
  새 소비처가 같은 함정을 다시 밟지 않는다 — shared-widgets 의 단일 출처 원칙과 일치한다.
- **잃은 것**: `selectable` 표의 셀 텍스트를 드래그로 선택해 복사할 수 없다. 다만 두 소비처
  모두 대체 수단이 이미 있다 — explorer 는 우클릭 "경로 복사", port scanner 는 행 컨텍스트의
  주소 복사. 대체 수단이 없는 표라면 그 표는 애초에 행 클릭을 소비하지 않는 게 맞고,
  그때는 `selectable(false)` 로 두면 기본 동작이 유지된다.
- **운영 비용 / 유지 부담**: 없음에 가깝다. 강제 지점은 셀 렌더 호출부 한 줄이고,
  계약은 `crates/tasty-ui-widgets/tests/table_row_click.rs` 가 headless egui 프레임 구동으로
  고정한다(셀 글자 위 클릭 → `clicked_row`, 헤더 클릭 → `clicked_sort` 회귀 포함).

## Alternatives Considered

- **A: 소비처(explorer `detail_view`)에서 국소 해제** — 같은 결함을 공유하는 port_scanner·
  갤러리가 그대로 남고, 앞으로 추가되는 소비처도 각자 밟는다. 선례 3곳이 이미 개별 회피의
  누적 비용을 보여준다.
- **B: 셀 텍스트를 `painter.galley` 로 직접 그리기(grid/list 방식)** — 위젯 등록이 없어
  문제는 사라지지만, 셀 렌더를 호출자에게 위임한다는 `Table` 의 범용 API 를 포기해야 하고
  줄바꿈·말줄임·정렬을 소비처마다 다시 구현하게 된다.
- **C: 앱 전역 style 에서 `selectable_labels = false`** — 표와 무관한 곳(로그·설명 텍스트
  등)의 텍스트 복사까지 일괄로 죽인다. 결함의 범위보다 훨씬 넓다.
- **D: 셀 라벨에 `Sense::hover()` 를 명시하도록 소비처에 요구** — A 와 같은 문제(소비처마다
  규율 필요)에 더해, 규율을 어겨도 컴파일이 통과하므로 계약이 강제되지 않는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- `selectable` 표에서 셀 텍스트를 드래그 선택해 복사해야 한다는 요구가 실제로 생기고,
  행 컨텍스트 메뉴의 복사 항목으로 대체되지 않는 경우(그때는 셀 단위 opt-in 이 필요하다).
- egui 가 hit-test 우선순위 또는 `Label` 의 sense 결정 방식을 바꿔, 자식 라벨이 있어도
  컨테이너 sense 가 클릭을 받을 수 있게 되는 경우.

## References

- [`docs/architecture/ui-widgets-crate.md`](../architecture/ui-widgets-crate.md#무엇을-공용-위젯으로) — 보편 컴포넌트 단일 출처
- [`docs/features/explorer/index.md`](../features/explorer/index.md) — detail 행 상호작용
- `crates/tasty-ui-widgets/src/table.rs`, `crates/tasty-ui-widgets/tests/table_row_click.rs`
- egui 0.31.1 `egui/src/widgets/label.rs`(sense 결정), `egui/src/hit_test.rs`(동률 시 나중 등록 우선),
  `egui/src/text_selection/label_text_selection.rs`(hover 시 `CursorIcon::Text`),
  egui_extras 0.31 `egui_extras/src/layout.rs`(셀 `Ui` sense 부여 시점)
