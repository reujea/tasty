# ADR-0063: Popup 닫힘 뒷정리는 `draw_popups` 가 아니라 `PopupDef.on_close` 훅 + `PopupManager::close()` 단일 관문으로 처리한다

- **Status**: Accepted
- **Date**: 2026-08-08
- **Tags**: ui, popup, lifecycle, close-hook, choke-point, refactor
- **Group**: ui-theme-gallery

## Context

Popup 이 닫힐 수 있는 경로는 6개다: draw_fn 이 `PopupAction::Close` 를 반환 / X 버튼·바깥 클릭(`PopupManager::draw` 내장 포인터 처리) / `UiIntent::ClosePopup` / 이미 열린 채로의 `UiIntent::TogglePopup` / App 계층의 직접 `close()` 호출 / debug IPC(`debug.host_popup.close`).

리팩터 전에는 draft 버퍼·대상 id 같은 팝업별 상태의 닫힘 뒷정리가 범용 렌더 루프(`draw_popups`) 안에 팝업마다 `if` 블록으로 인라인돼 있었고, 오직 두 경로(draw_fn Close, X 버튼/바깥 클릭)에만 붙어 있었다. 이 결합이 실제로 문제를 냈다:

- 범용 렌더 루프에 팝업별 지식이 117줄 응집 — 새 팝업을 추가할 때마다 이 루프를 건드려야 했다.
- 호출부 중복 — `image_upload.rs` 가 `transfer_progress` 정리를 popup close 호출과 별도로 직접 수행하고 있었다.
- 닫힘 시점 정리가 새는 걸 막으려고 **open 시점에 방어적으로 필드를 리셋**하는 코드가 6곳에 흩어졌다 — 원인이 아니라 증상을 지우는 패치였다.
- 실제 버그로 이어졌다: `preset_apply` 계열 3팝업이 X 버튼/바깥 클릭으로 닫히면(= draw_fn 을 거치지 않으면) `preset_apply_target_category` 가 정리되지 않고 다음 open 까지 살아남는 상태 누출이 있었다.

이 결정과 근거가 코드 어디에도 기록돼 있지 않았고, 그 사이 `docs/architecture/data-flows.md` 등 일부 문서는 렌더 루프의 실제 소재(`notification.rs`)에 대해서도 이미 부정확한 서술을 담고 있었다(별도 정정 — References 참고).

## Decision

**팝업별 닫힘 뒷정리는 `PopupDef.on_close: Option<fn(&egui::Context, &mut AppState, &mut CoreState)>` 훅으로 옮기고, `PopupManager::close()` 를 6개 close 경로 전부가 거치는 단일 관문으로 확정한다.**

`close()` 는 대상 popup 이 직전에 열려 있었을 때만(dedup) 그 id 를 `closed_queue` 에 쌓는다. 범용 렌더 루프(`popup::frame::draw_popup_layer`)가 매 프레임 이 큐를 drain 하며 등록된 훅을 정확히 한 번 호출한다. drain 은 재진입을 지원한다 — 훅이 다른 popup 을 닫으면 그 close 도 같은 drain 안에서 처리된다. 단, 훅끼리 서로를 계속 닫는 논리 오류를 막기 위해 라운드 상한(`ON_CLOSE_DRAIN_MAX_ROUNDS = 8`)을 둔다: 초과하면 경고 로그 후 그 라운드는 발화 없이 버린다.

상태가 없거나 남아도 무해한 팝업은 `on_close: None` 옆에 근거를 한 줄 남긴다(`src/adapters/ui/popup/defs.rs`).

## Consequences

- **얻은 것**: 팝업별 지식이 각자의 파일로 흩어져 범용 렌더 루프가 팝업 스키마를 몰라도 된다. 6개 close 경로 전부가 동일하게 커버돼 draw_fn 경로에만 붙던 뒷정리 누락이 구조적으로 불가능해진다(= `preset_apply` 버그의 재발 방지). open 시점 방어적 리셋 6곳이 전부 제거 가능해졌다 — 뒷정리가 close 시점에 정확히 1회 보장되므로 open 시점에 다시 지울 이유가 없다.
- **잃은 것**: 닫힘 처리가 그 프레임 안에서 동기적으로 끝나지 않고 다음 draw 호출의 drain 을 기다린다(1 draw 틱 지연). 대부분의 정리 작업(필드 리셋, 채널 정리)은 그 지연이 관측되지 않지만, "닫히는 즉시 동기적으로 끝나야 하는" 요구가 생기면 이 설계로는 부족하다.
- **운영 비용 / 유지 부담**: 새 팝업이 상태를 가지면 `on_close` 선언 여부를 판단해야 하는 체크리스트 항목이 하나 늘었다(`docs/dev-guide/popup-implementation.md` §닫힘 정리에 명문화). 재진입 상한은 정상 케이스에서 도달할 일이 없는 backstop 이라 상시 비용은 없다.

## Alternatives Considered

- **A — 뒷정리를 각 close 호출부에 복사**: 이미 발생한 방식(`image_upload.rs` 의 중복 정리)이고, close 경로가 6개라 팝업 하나당 최대 6곳에 같은 정리 코드를 반복해야 한다. 하나라도 누락하면 이번 `preset_apply` 와 같은 버그가 재발한다. 기각.
- **B — `draw_popups`(범용 렌더 루프)에 팝업별 `if` 블록을 계속 추가**: 리팩터 전 상태 그 자체. 범용 인프라가 모든 팝업의 스키마를 알아야 하고, 애초에 draw_fn Close/X버튼/바깥클릭 두 경로에만 붙어 있어 나머지 4개 경로(`ClosePopup`/`TogglePopup`/App 직접 호출/debug IPC)가 구조적으로 커버되지 않는다. 기각.
- **C — 정리 대상 상태를 팝업 열림 여부에서 파생(derive)**: dialog 상태(`DialogState`) 필드 다수가 팝업이 닫힌 뒤에도 다음 open 때까지 값을 들고 있어야 하거나(예: `port_scanner` 결과 유지 결정), 팝업 밖의 다른 코드에서도 읽힌다. "열려 있음" 하나로 파생 가능한 상태가 아니라 성립하지 않는다. 기각.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 프레임당 drain 으로는 부족해지는 경우 — 닫힘과 동기적으로(같은 프레임 안에서) 뒷정리가 끝나야 하는 요구가 실제로 생길 때.
- 재진입 상한(`ON_CLOSE_DRAIN_MAX_ROUNDS`)에 정상적인(버그가 아닌) 사용 패턴이 실제로 걸리는 사례가 나올 때 — 현재는 상호 재오픈처럼 논리 오류만 걸리도록 설계됐다.

## References

- [design/systems/popup.md §수명 계약](../design/systems/popup.md) — 동작 모델 정본
- [dev-guide/popup-implementation.md §닫힘 정리](../dev-guide/popup-implementation.md) — `on_close` 선언 규칙 + 절차
- [ADR-0024](0024-banner-fourth-overlay-concept.md) — 오버레이 개념 분리(같은 리팩터 체인에서 `draw_popups` 를 popup 루프/오버레이 체인으로 분리한 근거)
- `src/adapters/ui/popup.rs` — `PopupDef.on_close` 필드, `PopupManager::close()`(닫는 유일한 지점 — `grep "open = false"` 1건)
- `src/adapters/ui/popup/frame.rs` — `drain_on_close_hooks`/`drain_on_close_hooks_with_lookup`(drain 루프 + 라운드 상한)
