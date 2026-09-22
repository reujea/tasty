# ADR-0049: 강한 점유(hard occupy)의 readonly 는 PTY 상호작용만 차단한다 — 로컬 selection 은 예외, 휠/링크클릭은 계속 차단

- **Status**: Accepted
- **Date**: 2026-07-13
- **Tags**: occupation, hard-occupy, readonly, selection, mouse, wheel, link-click, mirror, attach, adr-0040
- **Group**: remote-attach

## Context

[ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md)은 강한 점유(hard occupy)를 "로컬 사용자에게 readonly(입력 차단, mirror 관찰)"로 정의했지만, "readonly"가 정확히 무엇을 차단하는지는 명문화하지 않았다. 실제 구현(`render_pass.rs`)은 이를 "PTY/TUI 조작뿐 아니라 selection/vi-cursor/IME/링크 하이라이트/검색까지 포함한 사용자 상호작용 오버레이 전체 억제"로 암묵적으로 해석했다.

이 암묵적 해석과는 별개로, 점유된 surface(soft/hard 공통) 위에 surface 전체 크기의 interactable `egui::Area`(1px 테두리 렌더용)가 얹혀 있어 `egui::Context::wants_pointer_input()`이 항상 `true`가 되고, 그 결과 마우스 클릭/드래그 이벤트가 `handle_mouse_input`에서 `egui_consumed`로 조기 소비되는 버그가 있었다. 이 버그 때문에:

- **soft 점유**(write 제한 없음이 원칙)에서도 마우스 드래그/휠이 전혀 시작되지 못했다 — ADR-0040의 soft 정의("표시만, write 제한 없음")를 위반하는 회귀였다.
- **hard 점유**에서는 이 버그가 위 암묵적 해석("readonly = 상호작용 전면 차단")을 사실상 강제하고 있었다 — selection 자체가 hard 에서 지금까지 한 번도 동작한 적이 없었다.

이 Area 버그를 고치면(순수 페인트로 전환) surface 위 마우스 이벤트가 다시 정상적으로 도달하게 되는데, 이때 hard 점유에서 "PTY로 아무것도 보내지 않는 순수 로컬 UI 동작"인 드래그 선택·클립보드 복사까지 계속 막을지, 아니면 열어줄지를 명시적으로 결정해야 했다. 사용자 요구("강한점유는 tui 조작은 못하지만 tasty 자체적인 드래그(내용 복사 등을 위한)는 돼야해")도 이 지점을 정확히 짚고 있다.

## Decision

강한 점유(hard occupy)의 readonly 는 **PTY/TUI 상호작용만 차단**하며, **tasty 로컬 텍스트 선택·복사는 예외적으로 허용**한다. 드래그 선택·selection 렌더·클립보드 복사는 PTY 에 아무것도 보내지 않는 순수 로컬 UI 동작이라 readonly 원칙(입력 차단)과 상충하지 않는다.

- **selection 렌더**(`render_pass.rs`)는 hard 점유에서도 그린다. IME preedit·vi-cursor·링크 하이라이트·검색 하이라이트는 여전히 PTY 상태와 직결되므로 계속 억제한다.
- **좌표 변환·복사 텍스트 추출**(`mouse_to_grid`/`mouse_cell_for_report`/`copy_selection_*`)은 hard 점유 시 live terminal 대신 **mirror(`readonly_view`, 3초 cadence)** 를 참조한다 — 실제 렌더되는 것과 같은 대상을 봐야 사용자가 드래그한 영역과 복사되는 텍스트가 화면과 일치한다. 공용 헬퍼 `CoreState::visible_terminal(surface_id)`가 이 판정을 캡슐화한다.
- **마우스 트래킹**(`effective_click_tracking`)은 hard 점유 시 실제 트래킹 모드와 무관하게 항상 `None`으로 격하한다 — hard 점유는 사용자가 그 live 앱과 상호작용할 수 없는 상태이므로, 트래킹이 켜진 채였다면 "앱에 보고" 분기로 빠져 조용히 무동작하는 대신 항상 로컬 선택으로 떨어져야 한다.
- **휠 스크롤**은 selection 과 같은 논리(순수 로컬 동작)가 성립함에도 **계속 차단**한다. 이유: 트래킹 조회·트래킹 OFF 시의 로컬 스크롤백 mutate 가 모두 live terminal 을 직접 건드리는데, hard 점유가 렌더하는 것은 mirror 이므로 이 mutate 는 화면에 반영되지 않으면서 live 의 `scroll_offset` 만 조용히 어긋난다. 점유 해제 직후 그 어긋난 만큼 스크롤이 튀어 보이는 새 회귀가 생긴다. hard 점유의 목표 범위에 휠이 애초에 포함되지 않았으므로, mirror 용 `&mut Terminal` 접근자를 새로 만들어 이 mutate 를 mirror 로 리다이렉트하는 대신 `handle_mouse_wheel` 진입부(surface_id 확정 직후, live 조회/mutate 이전)에서 hard 점유면 조기 차단하는 더 단순한 쪽을 택했다.
- **링크 클릭**(Ctrl+click 파일 열기·외부 URL 오픈)도 **계속 차단**한다. `try_handle_link_click`이 실행하는 동작 자체는 PTY로 아무것도 보내지 않는 순수 로컬 동작이라는 점에서 selection 과 같은 성격이지만, selection 과 달리 파일 열기·외부 앱 실행은 **되돌릴 수 없는 부수효과**를 일으킨다. hard 점유 화면은 최대 3초 지연된 mirror 스냅샷이므로, 그 시점에 보이는 링크가 실제 현재 PTY 상태와 다를 수 있다 — 신뢰할 수 없는 스냅샷을 근거로 파일/URL 을 여는 것은 위험하므로 억제를 유지한다.

soft 점유는 이번 결정의 영향을 받지 않는다 — ADR-0040 원칙대로 write 제한이 없으므로 마우스 클릭/드래그/휠이 점유 없는 surface 와 동일하게 전부 정상 동작한다(이번 작업은 위 Area 버그를 고쳐 그 원칙을 실제로 성립시켰을 뿐, 새 예외를 도입하지 않는다).

## Consequences

- **얻은 것**:
  - hard 점유 surface 에서도 tasty 자체 기능인 "드래그로 텍스트 선택 → 복사"가 동작한다 — ADR-0040 이 강한 점유를 "관찰·종료 권한은 유지"라 규정한 취지와 정합.
  - soft 점유의 마우스 드래그/휠 회귀(Area 버그)도 함께 해소된다.
  - `CoreState::visible_terminal` 신설로 "hard 점유면 mirror, 아니면 live"라는 분기가 `gpu.rs`(스크린샷)/`render_pass.rs`(렌더)/`mouse.rs`/`selection.rs`(좌표·복사) 전역에서 하나로 통일된다.
- **잃은 것 / 트레이드오프**:
  - hard 점유 mirror 는 3초 주기 갱신이라, 사용자가 드래그하는 순간과 mirror 스냅샷 시점 사이에 최대 3초의 시차가 있을 수 있다. 이 제약 자체는 ADR-0040 의 "3초 polling" 설계가 이미 내포한 것이라 이번 결정이 새로 만드는 문제는 아니지만, selection 이 hard 에서 처음 동작하게 되면서 **비로소 사용자에게 노출되는 UX**다.
  - "readonly = 상호작용 전면 차단"이라는 기존 암묵적 해석이 selection 한정으로 깨진다 — 향후 hard 점유에 새 상호작용(예: 검색)을 추가할 때마다 "PTY 상호작용인가, 순수 로컬 동작인가"를 매번 이 ADR 의 기준으로 재판단해야 한다.
- **운영 비용 / 유지 부담**:
  - hard 전용 조기 차단 지점(`try_handle_link_click`, `handle_mouse_wheel`)이 늘어 hard 점유 분기가 파일 여러 곳(mouse.rs 3곳 + render_pass.rs + terminal_finders.rs)에 흩어진다 — `visible_terminal` 헬퍼로 좌표/복사 경로는 통일했으나, 완전 차단 분기(휠/링크)는 그 헬퍼로 커버되지 않으므로 별도로 유지해야 한다.

## Alternatives Considered

- **A: selection 도 hard 에서 계속 차단(현행 암묵적 해석 유지)** — 사용자가 명시적으로 요구한 "강한 점유에서도 복사는 되어야 한다"를 충족하지 못한다. 기각.
- **B: hard 점유에서 휠도 mirror 기준으로 허용** — 트래킹 조회를 `effective_click_tracking`으로 통일하고 로컬 스크롤백 mutate 를 위한 별도 `&mut Terminal` mirror 접근자를 신설해야 한다. hard 점유의 목표 범위에 휠이 없어 이 확장 비용을 정당화하지 못했다. 채택 안 함(재검토 조건 참고).
- **C: 링크 클릭도 selection 과 같은 논리로 허용** — 파일 열기·외부 URL 오픈은 되돌릴 수 없고, hard 점유 화면은 최대 3초 지연된 mirror 스냅샷이라 그 시점 링크가 실제 PTY 상태와 다를 위험이 있다. 기각.
- **D: mirror 참조 대신 selection 좌표도 live 기준 유지** — live 는 점유와 무관하게 계속 갱신되므로(입력만 차단, 출력 수신은 차단 안 됨) 화면(mirror)과 스크롤 위치·scrollback 길이가 어긋나 사용자가 본 것과 다른 텍스트가 복사될 수 있다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- hard 점유에서도 휠 스크롤을 mirror 기준으로 지원해야 할 요구가 생길 때 — 로컬 스크롤백 mutate 용 mirror `&mut Terminal` 접근자 신설이 필요해진다(위 대안 B).
- mirror 갱신 주기가 3초보다 훨씬 짧아지거나 실시간에 가까워져, 링크 클릭의 "스냅샷 신뢰성 문제"(위 대안 C 기각 사유)가 해소될 때.
- mirror 3초 지연 경계에서의 selection 좌표 드리프트가 실사용에서 반복적으로 문제로 보고될 때 — polling 주기 단축 또는 드래그 중 즉시 재동기화 등 별도 대응이 필요해진다.

## References

- 강한/약한 점유 모델 정의: [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md)
- 강한 점유의 mirror/lock/readonly/polling 메커니즘: [`dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md)
- mirror geometry 클라이언트 구동: [ADR-0045](0045-mirror-geometry-client-driven.md)
