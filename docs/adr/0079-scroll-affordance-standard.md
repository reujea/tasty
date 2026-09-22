# ADR-0079: 스크롤 어포던스 표준은 "스크롤바 숨김 + 가장자리 페이드" — 폭 예약은 예외로만 남긴다

- **Status**: Accepted
- **Date**: 2026-08-23
- **Tags**: ui, scroll, egui, affordance, popup, table, remote-tool, port-scanner
- **Group**: ui-theme-gallery

## Context

egui 의 기본 스크롤 스타일은 `ScrollStyle::floating()` 이고 tasty 는 이를 덮어쓰지 않는다.
floating 스타일의 세로 스크롤바는 **레이아웃 폭을 전혀 할당하지 않고**(`floating_allocated_width = 0`)
콘텐츠 위에 오버레이로 그려진다. 즉 스크롤바가 뜨는 자리에 원래 있던 콘텐츠는 사라지지 않고
**가려질** 뿐이다.

tasty 의 목록 행은 우측 끝에 액션 아이콘(가져오기 / 편집 / 삭제 / 재감지 / 즐겨찾기)을 둔다.
그래서 커서를 그 아이콘으로 가져가는 순간 스크롤바가 같은 자리에 나타나 클릭을 먹는다 —
Remote connections 팝업에서 사용자가 직접 보고한 버그가 정확히 이것이다.

이 문제에 대해 코드베이스에 상반된 해법 둘이 이미 공존하고 있었다.

1. **폭 예약** — `src/adapters/ui/popup/port_scanner.rs` 의 `scrollbar_reserve`.
   스크롤바 폭(`bar_width + bar_inner_margin`)만큼 콘텐츠 가용폭을 미리 줄여, 오버레이 바가
   앉을 빈 홈통을 남긴다. 스크롤바는 계속 보인다.
2. **숨김 + 페이드** — `src/adapters/ui/popup/remote_tool.rs` 의 `scroll_list_with_fade`.
   `ScrollBarVisibility::AlwaysHidden` 으로 바를 아예 없애고, 스크롤 여지가 남은 쪽 가장자리에
   `bg-panel` → 투명 세로 그라디언트를 덮어 "더 있다"를 알린다.

1 번은 "커서가 스크롤바 위 = 클릭 불가" 라는 구조를 그대로 남긴다. 홈통 폭이 맞는 동안에만
증상이 안 보일 뿐, 다른 폭·해상도·컬럼 구성에서 재발할 수 있는 종류의 회피다.

## Decision

**tasty 의 스크롤 어포던스 표준은 "스크롤바 숨김 + 가장자리 페이드" 다.** 새로 만드는 스크롤
영역, 그리고 기존 영역을 손볼 때는 이 방식을 쓴다. 현재 참조 구현은 `remote_tool.rs` 의
`scroll_list_with_fade` / `fade_edges` / `fade_height` / `paint_edge_fade` 다.

**폭 예약은 표준이 아니라 예외다.** 예외로 남길 조건은 다음 둘을 **모두** 만족하는 경우로
한정한다.

- **(a) 스크롤바 폭이 콘텐츠 레이아웃 계산의 입력이다** — 스크롤바에 내주는 폭이 단순히
  "가리지 않게 비켜주는 여백"이 아니라, 콘텐츠 자체의 치수를 정하는 수식에 들어가서 다른
  레이아웃 판정(예: 가로 스크롤 발생 여부)까지 함께 바꾼다.
- **(b) 스크롤 뷰포트를 우리가 소유하지 않는다** — 스크롤 영역이 서드파티 컨테이너 안에 있어
  바 표시 여부를 우리가 지정할 수 없거나, 페이드를 그릴 뷰포트 `Rect` 를 돌려받지 못한다.

이 조건으로 판정하면 **`port_scanner.rs` 의 포트 테이블은 예외로 유지한다.** 근거는 추측이
아니라 코드다.

- (a) 성립: 이 테이블의 7 컬럼은 전부 `TableColumnWidth::Exact(w)` 이고, `w` 는
  `compute_column_widths(visible, item_spacing_x, available)` 이 `available` 을 컬럼들에
  **정확히 나눠 담아** 만든다(여유폭 `slack` 을 flex 컬럼에 분배해 남김없이 채운다). 그리고
  본문은 `horizontal_scroll(true)` 로 감싸여 컬럼 폭 합이 뷰포트를 넘으면 가로 스크롤로
  전환된다. 따라서 `available` 에서 빼는 `scrollbar_reserve` 는 여백 하나가 아니라
  **컬럼 폭 값과 가로 스크롤 발생 여부를 동시에 결정하는 수치**다. 실제로 이 값을 빼지 않으면
  컬럼 폭 합이 뷰포트 폭과 정확히 같아져(`total_w == 가용폭`) 경계에서 가짜 가로 스크롤이
  뜰 수 있다 — 코드 주석이 말하는 그 현상이다. 스크롤바를 숨기는 것만으로는 이 수식이
  사라지지 않는다.
- (b) 성립: 이 테이블의 세로 스크롤은 우리 `ScrollArea` 가 아니라 공용 `Table` 위젯
  (`crates/tasty-ui-widgets/src/table.rs`) 내부의 `egui_extras::TableBuilder` 가 소유한다.
  `Table` 은 `scroll_bar_visibility` 를 전달하는 API 가 없고, 반환값 `TableOutput` 에는
  클릭 결과만 있고 뷰포트 `Rect` 가 없다. 즉 지금 구조에서는 바를 숨길 수도, 페이드를 그릴
  좌표를 알 수도 없다 — 표준으로 옮기려면 위젯 API 를 먼저 넓혀야 한다.

바꿔 말하면 `port_scanner` 의 폭 예약은 "스크롤바를 보여주고 싶다"는 스타일 선택이 아니라
**Exact 폭 테이블 + 가로 스크롤 모델에 얽힌 계산**이라서 예외다. 스타일 선택이었다면 표준을
따라야 한다.

## Consequences

- **얻은 것**: 목록 행 우측 끝 아이콘이 스크롤바에 먹히는 버그가 구조적으로 사라진다(표준을
  따르는 영역에서는 바가 존재하지 않는다). "이 아래 더 있다"는 정보는 페이드로 보존된다.
  같은 문제에 두 해법이 경쟁하던 상태가 정리돼, 다음 사람이 어느 쪽을 베낄지 고민하지 않는다.
- **잃은 것**: 표준을 따르는 영역에는 스크롤 **위치**(전체 중 어디쯤인지)와 **분량**(스크롤바
  핸들 길이) 정보가 없다. 페이드는 "더 있다/없다" 이진 신호만 준다. 목록이 아주 길어져 위치
  감각이 필요해지는 화면이 생기면 그때 별도 인디케이터를 논의한다.
- **운영 비용 / 유지 부담**: 현재 헬퍼는 `remote_tool.rs` 안에 산다. 다른 팝업이 이 패턴을
  실제로 필요로 할 때 `tasty-ui-widgets` 로 승격하고 갤러리 specimen 을 붙인다 — 지금은
  소비자가 하나뿐이라 승격하지 않는다. 그때까지 다른 파일이 같은 코드를 복사하는 상황이
  생기면 그것이 승격 신호다.
- 기존 스크롤 영역을 일괄 마이그레이션하지는 않는다. 이 ADR 은 **새로 만들거나 손대는 곳**에
  적용되는 기본값이고, 손대지 않는 화면의 기존 동작을 바꾸라고 요구하지 않는다.

## Alternatives Considered

- **폭 예약을 표준으로 승격**: 스크롤 위치·분량 정보를 유지한다는 장점이 있지만, 커서가 바
  위에 있으면 그 아래 콘텐츠를 못 누른다는 구조가 남는다. 홈통 폭과 아이콘 배치가 어긋나는
  순간(다른 팝업 폭, 다른 DPI, 컬럼 구성 변경) 같은 버그가 되돌아온다. 사용자가 이 방향을
  명시적으로 물렸다.
- **스크롤바를 보이되 클릭을 관통시키기**: 바 위 입력을 콘텐츠로 흘려보내면 이번엔 스크롤바를
  드래그할 수 없다. 보이는데 못 잡는 컨트롤은 더 나쁘다.
- **`ScrollStyle::solid()` 로 전역 전환**(바가 레이아웃 폭을 실제로 점유): 겹침 자체는
  사라지지만 모든 스크롤 영역의 콘텐츠 폭이 줄고, 스크롤이 없는 동안에도 홈통이 남거나
  스크롤 발생 시 콘텐츠가 밀린다. 앱 전역 시각에 미치는 영향이 이 버그의 범위보다 훨씬 크다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 두 번째 팝업이 `scroll_list_with_fade` 를 필요로 한다 → 헬퍼를 `tasty-ui-widgets` 로 승격하고
  갤러리 specimen 을 추가한다(위치만 바뀌고 결정은 유지되므로, 이 항목은 ADR 교체가 아니라
  후속 작업 트리거다).
- 공용 `Table` 이 바 표시 여부와 뷰포트 `Rect` 를 노출하도록 API 가 넓어진다 → 위 예외 조건
  (b) 가 깨지므로 `port_scanner` 를 표준으로 옮길지 다시 판단한다.
- 목록 길이가 커져 "더 있다"만으로는 부족하고 스크롤 **위치**를 알려야 한다는 요구가 생긴다.
- egui 의 기본 `ScrollStyle` 이 non-floating 으로 바뀌거나 tasty 가 전역 스크롤 스타일을
  덮어쓰기로 한다 → 오버레이 전제 자체가 달라진다.

## References

- [`docs/features/remote-profiles/screens/remote-tool.md`](../features/remote-profiles/screens/remote-tool.md) — 표준 적용 화면("목록 스크롤" 절)
- [`docs/design/systems/theme.md`](../design/systems/theme.md) — 페이드 색·높이는 `Theme` 토큰(`bg-panel`, `space-xl`)에서
- `src/adapters/ui/popup/remote_tool.rs` — 참조 구현(`scroll_list_with_fade` / `fade_edges` / `fade_height` / `paint_edge_fade`)
- `src/adapters/ui/popup/port_scanner.rs` — 예외(`scrollbar_reserve` / `compute_column_widths`)
- `crates/tasty-ui-widgets/src/table.rs` — 예외 조건 (b) 의 근거(`TableOutput` 에 뷰포트 `Rect` 없음, 바 표시 여부 전달 API 없음)
- [ADR-0069](0069-table-row-click-over-cell-text-selection.md) — 같은 표에서 발생한 다른 히트테스트 결정
