# ADR-0380: 토스트 카드의 치수·색은 도출이 하나다 — 갤러리는 본체의 alpha 곱 순서를 따른다

- **Status**: Accepted
- **Date**: 2026-09-21
- **Tags**: toast, gallery, shared-widgets, color, alpha, identity, measurement, adr-0510, adr-0122

## Context

토스트 카드의 **그리기**(chrome)는 이미 `crates/tasty-ui-widgets/src/toast.rs` 하나였다.
본체 스택(`draw_toast_scopes`)과 갤러리 specimen 이 같은 `draw_card` 를 불렀다. 그러나
갤러리의 단일 카드 specimen(Toast · Toast stack)은 `draw_card` 에 넘길 값 — 본문 galley,
카드 폭·높이, alpha 를 곱한 fill·border·accent — 을 **자기 파일에서 다시 계산**했다.

두 계산은 같아 보였지만 alpha 를 곱하는 순서가 달랐다.

- 본체: 테마 색(`HexColor`, straight alpha)에 alpha 를 곱한 뒤 `Color32` 로 바꾼다.
  `Color32::from_rgba_unmultiplied` 가 **선형 공간**에서 premultiply 한다.
- 갤러리: `Color32`(premultiplied)로 먼저 바꾼 뒤 `Color32::gamma_multiply` 로 곱한다.
  네 채널을 **감마 공간**에서 곱한다.

alpha 가 1 이면 두 길이 같다. alpha 가 1 이 아니면 갈린다 — 갤러리 Toast stack specimen 의
alpha 0.85·0.6 카드에서 fill·border 가 채널 델타 최대 20 만큼 달랐다(실측 2026-09-21, 아래
Consequences). 즉 갤러리는 페이드 중인 카드를 **본체와 다른 색으로** 보여주고 있었다.

같은 티켓의 다른 요구는 "Agent IPC 가 사용자 토스트를 발화시키지 않는다"(identity 원칙 1)를
재는 채널을 값 자리에 두는 것이었다. 그 정책을 보는 시험·가드는 레포에 없었다.

## Decision

**치수·색의 도출을 위젯 크레이트 하나로 올린다.** `layout_card`(galley · 폭 · 높이),
`card_colors`(alpha 를 곱한 네 색), `draw_single_card`(한 장을 `ui` 에 자리 잡아 그림)를
두고, 본체 스택은 앞의 둘을, 갤러리 단일 카드는 셋째를 `(kind, message, alpha)` 만 넘겨
부른다. **alpha 곱 순서는 본체 쪽으로 고정한다** — 사용자가 보는 것이 정본이고, 갤러리는
그것을 보여주는 자리다.

발화 금지 정책에는 **판정기를 짓지 않고 재는 법을 적는다.** `docs/design/systems/toast.md`
"트리거 정책" 절에 IPC 진입 경로에서 토스트 매니저로 닿는 이름을 세는 명령과, 그것이 잡는다는
변이 확인, 못 보는 경로 넷을 적는다. 그중 ④(IPC → `dispatch_intent(.. from_agent_ipc())` →
`src/intent/*` 의 `report_apply_error`)는 **형태는 있으나 지금 확인된 실례는 없다.**
`report_apply_error` 는 origin 을 안 보고 사용자 토스트를 낼 수 있지만, mirror 에서 그
토스트가 나는 forward 불가 op 로 끝나는 IPC intent 를 찾지 못했다 — convert 는 항상 forward
된다(`docs/dev-guide/attach-behavior.md` 의 convert/move-surface 항목). 이 결정은 그 함수를
고치지 않는다. 실례가 생기면 처방은 판정기가 아니라 그 함수가 origin 을 보게 하는 것이다.

## Consequences

- **얻은 것**: 갤러리가 페이드 중인 카드를 본체와 같은 색으로 보인다. 카드 치수 규칙
  (줄바꿈 폭 · 패딩 · accent 바 · 높이)이 한 곳에만 있다.
- **잃은 것**: 갤러리 Toast stack specimen 의 픽셀이 바뀐다. 실측 2026-09-21(Xvfb ·
  llvmpipe · 1360×1000, `TASTY_GALLERY_SHOT`): 같은 바이너리 2 회 캡처의 잡음 바닥은 해당 컷에서
  bbox `None`, 전후 diff 는 12171px · 채널 델타 최대 20 · 중앙값 8 이고 bbox 가 alpha < 1 카드
  두 장 자리에만 있다. 단일 카드(alpha 1) specimen 은 전후 diff 가 없다 — 그 자리에 대상이
  있다는 것은 단일 카드 alpha 를 0.5 로 바꾼 양성 대조가 그 bbox 에서 달라지는 것으로 확인했다.
- **운영 비용 / 유지 부담**: 발화 금지 정책은 여전히 자동 채널이 없다. 재는 법은 사람이
  돌려야 하고, 호출 이름이 IPC 파일에 안 나타나는 경로 넷을 원리적으로 못 본다. 그중 ④ 는
  형태는 있으나 지금 확인된 실례는 없다(convert 는 항상 forward 되므로 `markdown.navigate` 는
  그 실례가 아니다 — `docs/dev-guide/attach-behavior.md` 의 convert/move-surface 항목).

## Alternatives Considered

- **갤러리의 곱 순서를 정본으로** — 본체를 갤러리에 맞추면 사용자에게 보이는 페이드 색이
  바뀐다. 이 회차는 값을 옮기기만 하고 새 디자인 값을 정하지 않는다. 기각.
- **곱 순서를 인자로 받기** — 두 길을 다 살려 두는 것은 "도출이 둘" 을 함수 안으로 옮긴
  것뿐이다. 기각.
- **발화 금지 소스 스캔 가드를 짓기** — 스캔 좌변(IPC 파일)에서 난 결함을 댈 수 없다. 결함은
  그 밖에서 난다: 실제로 있었던 결함(예전 `window.create` 가 창 생성 실패를 사용자 토스트로
  알리던 것, [ADR-0122](0122-winit-scheduled-fallible-ipc-returns-outcome.md) 가 고쳤다)은 winit
  이벤트 핸들러에서 났고, 형태만 확인된 경로 ④ 는 `src/intent/*` 에서 날 수 있다. 둘 다 IPC 파일 스캔으로는
  안 잡힌다. ④ 를 IPC 파일에서 intent 파일까지 따라가는 가드는 호출 그래프 추적이라 문자열 스캔으로
  만들 수 없고, 고치면(origin 을 보게 하면) 필요가 사라진다. 기각.

## Reconsideration Triggers

**채널이 붙는 것**

- `crates/tasty-ui-widgets/src/toast.rs` 밖에서 `surface_raised().gamma_multiply` 또는
  `toast_border().gamma_multiply` 로 카드 색을 다시 도출하는 자리가 생기면(도출이 다시 둘이
  된다).

**원리적으로 안 붙는 것**

- 디자인이 페이드 중 카드 색을 감마 공간 곱으로 정하면 — 그때는 `card_colors` 한 곳을 바꾼다.
  재는 법: 디자인 request 의 확정 값.
- 경로 ④(`report_apply_error` 의 origin 무시)를 고치는 작업이 착지할 때 — 그 수정이 origin 을
  보는 시험을 함께 두는지 보고, 재는 법 절의 경로 목록을 갱신한다. 재는 법: 그 작업의 커밋.
- IPC 요청이 사용자 토스트를 띄운 결함이 경로 ④ 밖에서 실제로 보고되면 — 그 결함의 경로를 보는
  판정기를 그 수정과 함께 짓는다. 재는 법: 결함 보고.

## References

- 후속: [ADR-0503](0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md) — 경로 ④ 를 origin 으로 막았다(위 재검토 트리거의 착지)
- `docs/design/systems/toast.md` — 구조 절(함수 목록), 트리거 정책 절(재는 법)
- 코드 근거(결정이 실현된 현재 위치): `crates/tasty-ui-widgets/src/toast.rs` 의 `card_colors` ·
  `layout_card` · `draw_single_card` · `draw_toast_scopes`, `crates/tasty-gallery/src/catalog/widgets/toast.rs`
  의 `draw_toast_card`
- [ADR-0510](0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md) — 갤러리 완전성
