# 토스트 시스템

**Toast** 는 View 내부에 잠깐 떴다가 자동으로 사라지는 휘발성 피드백 UI 다 — "복사됨", "저장됨" 같은 사용자 동작의 즉각 결과. `PopupManager` 가 아니라 별도 `ToastManager`(`src/adapters/ui/toast.rs`)로 관리된다. 용어 구분은 [concepts/ubiquitous-language](../../concepts/ubiquitous-language.md).

## Popup 과의 차이

| 항목 | Popup | Toast |
|------|-------|-------|
| 수명 | 사용자가 닫을 때까지 | 일정 시간 후 자동 소멸(기본 2s, Settings › General › Overlay 의 Toast duration 으로 1~10s 조절) |
| 포커스 | 클릭 시 보유 가능 | **절대 받지 않음** |
| 입력 | 클릭/드래그/X | **소비 안 함**(마우스 통과) |
| 타이틀바 | 있음 | 없음(본문만) |
| 위치 | 자유 이동 | 스코프별 고정 스택 |
| 트리거 | 사용자 또는(debug)에이전트 | **사용자 행동만** (예외: 원격 연결 상태 사건 — 아래 허용 부류) |

Toast 는 Popup 의 변종이 *아니다* — 8대 규칙(타이틀바·X·드래그·z-order 승격·외부클릭닫기)이 토스트와 정면 충돌하므로 별도 매니저로 둔다. 단 스코프 정의와 스코프-rect 계산은 `LayoutContext` 를 재사용해 일관성을 유지한다.

## 트리거 정책 (CRITICAL)

**Toast 는 사용자 행동(키보드 단축키 / 마우스)에서만 발사된다** — 예외는 아래 허용 부류 하나다. CLI/IPC 를 통한 에이전트 동작은 사용자 시각 상태에 영향을 주면 안 되므로 토스트를 띄우지 않는다([identity](../../identity.md) 원칙 1, [popup.md](popup.md) 발화 정책과 동일).

복사 예시:
- 터미널 선택 후 `Ctrl+C` → ✅ · Explorer 경로 복사 → ✅ · 클립보드 뷰어에서 항목 클릭 복사 → ✅
- IPC `clipboard.*` 쓰기 → ❌
- OSC 52 쓰기(터미널 프로그램이 보낸 클립보드 시퀀스) → ✅ `toast.copied_osc52` — 프로그램이 **보이지 않게**
  시스템 클립보드를 덮어쓰는 것을 사용자에게 보이게 하려는 것이다(`src/app/dispatch_domain.rs`
  `cascade_terminal_clipboard_set`).

**OSC 52 는 이 정책의 알려진 예외다.** OSC 52 는 PTY 출력이라 그 바이트를 누가 나오게 했는지
(origin)를 가를 수 없다 — 사용자가 친 명령이든, 에이전트가 `send text` 로 셸에 찍게 한
명령이든 같은 바이트다. 그래서 에이전트가 셸에 OSC 52 를 찍게 해도 토스트가 뜬다. 원칙 1 과
긴장이 있는 자리이고, 가시화(클립보드 무단 덮어쓰기 알림)를 택해 둔 상태다.

**재는 법** — 이 정책을 보는 자동 채널(시험·가드)은 없다. 대신 IPC 가 들어오는 경로에서
토스트 매니저로 닿는 이름을 센다. 결과가 **0 줄**이어야 한다.

```bash
grep -rnE --exclude='*_tests.rs' 'toasts|report_apply_error|push_toast' \
  src/adapters/ipc/ src/app/ipc/ src/app/ipc.rs \
  src/boot/headless_dispatch.rs src/boot/headless_plugins.rs
```

- 이 명령이 잡는다는 것은 변이로 확인했다: IPC 핸들러 파일에 `state.toasts.push_info(...)`
  한 줄을 넣으면 그 줄이 나온다(2026-09-21).
- **못 보는 경로가 넷이다.** ① IPC 가 창 생성 같은 일을 winit 이벤트로 넘기고, 그 이벤트
  핸들러가 실패를 토스트로 알리는 경로(예전 `window.create` 가 그랬다 — 지금은 완료
  채널로 응답한다). ② IPC 가 만든 도메인 이벤트가 `src/app/dispatch_domain.rs` cascade
  에서 토스트를 내는 경로. ③ 에이전트가 터미널에 보낸 텍스트가 프로그램을 거쳐 토스트를
  내는 경로(OSC 52 등). ④ IPC 엔진 핸들러가 `out.push(... .from_agent_ipc())` 로
  요청의 intent 출구에 넣고(진입점이 요청 끝에 창 큐로 옮긴다), 메인 루프가 그것을
  `src/intent/*` 에서 처리하면서 토스트를 내는 경로. 이 넷은 호출 이름이 IPC 파일에 안 나타난다.
- **경로 ④ 는 `src/intent/*` 가 origin 을 보아 막는다.** IPC 핸들러가 intent 를 agent origin
  으로 넘기면, `src/intent/*` 의 적용 실패 신호는 **사용자 origin 에서만** 토스트가 된다 —
  에이전트 origin 의 실패는 `warn` 로그로 끝난다. 두 자리가 있다.
  - 동기 차단: `core.apply` 가 mirror 구조 op 를 forward 하지 못하면(`forwarded: false`)
    `report_apply_error`(`src/intent.rs`)가 `attach.toast.mirror_structural_blocked` 를 사용자
    origin 에서만 낸다. preset 적용·저장 실패 토스트(`src/intent/preset.rs`)도 같다(저장 성공
    토스트는 origin 과 무관하다).
  - 비동기 실패: forward 된 op 가 원격에서 실패하면 `attach.toast.mirror_structural_forward_failed`
    가 난다. `report_apply_error` 가 에이전트 origin 의 forward op 에 `silent_failure` 를
    표시하고, attach client 는 그 op_id 의 실패 회신을 토스트 대신 로그로 보낸다
    (`src/app/attach_client/agent_origin.rs` `AgentRequests`).
  이 경로의 실례: `markdown.navigate` 의 convert 와 `file_handler.dispatch`(`origin_surface_id`
  없이)가 새 탭으로 떨어지는 경우(`Intent::NewTab`)는 mirror 워크스페이스에서 forward 되고, 원격이 그 op 를 적용하지
  못하면(예: 원격에 없는 surface kind) 실패 회신이 온다. 뒤쪽이 에이전트 origin 인 것은 호출이
  사용자가 만진 plugin popup 을 대지 않았을 때다 — 대면 사용자 origin 이라 표시가 없고 실패는
  토스트가 된다(markdown 파일열기 팝업, [ADR-0526](../../adr/0526-a-plugin-popup-the-user-touched-makes-its-file-dispatch-a-user-action.md)). 시험:
  `src/intent/apply_error_tests.rs` · `src/app/attach_client/agent_origin.rs` 의
  `an_agent_forward_failure_does_not_toast`. 두 끝을 세는 명령(경로의 폭을 보는 것이다):
  `grep -rn 'from_agent_ipc()' src --include='*.rs'` 로 agent origin intent 의 발화 자리 ·
  `grep -rln 'report_apply_error\|toasts\.push' src/intent.rs src/intent/`(7 파일 — 시험 `apply_error_tests.rs` 포함).
  intent 를 안 거치고 `Core::apply` 를 직접 부르는 IPC 구조 핸들러(split · tab.create/close/move ·
  pane.close · surface.close · `image.open`)도 같은 표시를 붙인다 — `core::structural_exec` 의
  `apply_as_agent` 와 `image::handle_open` 이 붙이고, 시험은 같은 파일의
  `an_ipc_direct_structural_forward_is_marked_for_a_silent_failure` 다. 표시를 안 받는 것은 origin 을
  가를 수 없는 자리(`src/file/dispatch.rs` 의 `Some(pane)` 갈래)와 사용자 전용 경로
  (`AppState::forward_mirror_structural`)뿐이고, 그 forward op 의 원격 실패는 토스트가 난다.
- 판정기를 짓지 않은 이유: 위 명령의 좌변(IPC 파일)에서는 결함이 난 적이 없다. 결함은 늘 그
  밖에서 났다 — ① 과 ④ 둘 다 IPC 파일을 스캔하는 판정기로는 안 잡힌다. ④ 는 판정기가 아니라
  `src/intent/*` 가 origin 을 보게 고친 것으로 막았고, 위 두 시험이 그 분기를 고정한다.

**허용 부류 — 원격 연결 상태 사건.** attach mirror 의 연결 상태 사건(끊김 · 재연결 · 손실 · 구조 전달 실패)은 사용자 행동 없이도 토스트를 띄운다. 원인이 에이전트 IPC 가 아니라 네트워크·원격 처리·소비 속도이고, 알리지 않으면 사용자가 원격의 사본인 mirror 의 낡은 화면을 최신으로 읽는다. 현재 구성원은 `attach.toast.mirror_reconnecting` · `mirror_reconnected` · `mirror_reconnect_giveup` · `mirror_disconnected` · `mirror_desynced` · `mirror_structural_forward_failed` 여섯이다. `mirror_markdown_truncated`(원격 문서가 잘렸다)는 사용자 행동 없이 나지만 연결 사건이 아니라 이 부류 밖이다 — 알려진 예외로 ADR-0401 에 적혀 있다. 그 예외에서 에이전트의 `markdown.reload` 가 건 재조회는 빠졌다 — 그 회신의 잘림은 toast 없이 로그다([ADR-0503](../../adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md)). 창 없는(parked) engine 에서는 띄우지 않는다. 에이전트 IPC 호출이 직접 일으킨 결과는 이 부류가 아니다(위 원칙대로 ❌) — 에이전트 origin intent 나 IPC 구조 요청이 forward 한 op 의 원격 실패도 그렇다(`mirror_structural_forward_failed` 대신 로그, 위 경로 ④). 근거·대안은 [ADR-0401](../../adr/0401-remote-connection-events-may-raise-a-toast-without-a-user-action.md) · 개정 [ADR-0503](../../adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md).

## 스코프

Popup 과 같은 enum 을 쓰지만 **위치 앵커 용도** 다(가시성 필터 역할은 거의 없음 — 어차피 짧게 떴다 사라짐). `ToastScope`: `Window` / `Workspace(usize)` / `Pane(u32)` / `Surface(u32)`. 기본은 `Surface`(어디서 일어난 일인지 모르면 `Window`). 같은 스코프 내 여럿이면 아래에서 위로 쌓고, 스코프가 화면에서 사라지면 즉시 제거.

## 시각 / 레이아웃

모든 색·치수는 Theme 토큰([theme.md](theme.md)). 배경 `surface0` + 1px `surface1` 보더 + `corner_radius`, 본문 `font_size_body`, 스코프 우측 하단 정렬·스택. 종류 강조는 좌측 4px 컬러 바:

| 종류 | 바 색 | 용도 |
|------|-------|------|
| Info | `blue` | 일반(기본) |
| Success | `green` | 완료 |
| Warning | `yellow` | 주의 |
| Error | `red` | 실패 |

> 페이드(등장/소멸 알파만, 위치 이동 없음)는 적용된다 — theme.md 의 "터미널 콘텐츠 애니메이션 0ms" 규칙은 **터미널 콘텐츠** 한정이라 비-터미널 알림 UI 에는 적용되지 않는다.

**경계 처리**: 토스트의 모든 변은 자기 스코프 `scope_rect` 안에 머문다. ① `max_width` 는 surface 안쪽 폭(`width - 2*margin`)으로 클램프해 좁은 surface 에서 좌측 누출을 막고(정상 폭 surface 에선 0.8 폭이 그대로라 시각 무변경), ② 스택이 스코프 상단을 넘으면 더 오래된 토스트는 그리지 않으며, ③ scope 경계로 painter 를 클립해 1px 단위 누출까지 차단한다. 우측 하단 정렬 자체는 유지된다.

## 입력 — 소비하지 않음

Toast 위에서 마우스 클릭/드래그해도 토스트는 무시하고 이벤트가 아래 레이어(터미널/popup/divider)로 통과한다. `popup_hovered` 도 토스트 영역에선 false. 키보드 포커스도 받지 않아 `has_focused()` 와 무관.

## 합치기 / 제한

같은 스코프에서 같은 메시지가 짧은 시간(기본 500ms) 내 재발사되면 새로 만들지 않고 **기존 토스트 수명만 갱신**(연속 Ctrl+C 깜빡임 방지). 스코프당 최대 동시 5개, 초과 시 가장 오래된 것 즉시 제거.

본문은 **200자(유니코드 문자 기준)** 로 제한한다. 초과 시 앞 200자만 남기고 줄바꿈 + 안내 접미(`toast.char_limit_notice`)를 붙여(`<앞 200자>\n(200자 제한)`) 비정상적으로 긴 입력이 토스트를 세로로 폭주시키는 것을 막는다. 길이/자르기는 char 경계로 처리해 멀티바이트에서 안전하며, coalesce 비교 이전(`push` 진입부 `truncate_message`)에 적용된다.

캡 값의 출처는 `tasty-i18n` 의 `TOAST_MAX_CHARS` **하나**다. 캡에 맞춰 문구를 만드는 쪽(`fit_fragment` · `t_fmt_fit`)과 캡을 집행하는 쪽(`truncate_message`)이 같은 상수를 읽고, 접미가 말하는 숫자도 번역문의 `{}` 에 그 값이 들어가 만들어진다. 셋 중 하나만 바뀌어 "맞췄는데 잘린다" 거나 "200자 제한이라고 적혀 있는데 실제는 아니다" 가 되는 상태를 만들 수 없다. 번역문에 숫자가 다시 박히는 것은 `tests/toast_message_fits_cap.rs` 가 막는다.

> **길이를 모르는 조각(경로·실패 사유)을 실을 때는 `push` 전에 미리 줄인다** — `tasty_i18n::t_fmt_fit` / `fit_fragment` 가 번역된 틀은 그대로 두고 **조각의 가운데만** 생략해 캡 안에 맞춘다. 호스트의 기본 잘림은 **꼬리를 버리므로**, 경로면 어느 파일인지가, 실패 사유면 OS 에러가, 문장이면 "어떻게 하라" 는 지시가 사라진다. 소비자: 언어팩 폴백 경고(`LoadReport::user_warning`), 설정의 bashrc 저장 실패(`toast.bashrc_save_failed`).

## 구조 — 그리기와 상태가 다른 크레이트에 있다

**그리기**는 `crates/tasty-ui-widgets/src/toast.rs` 가 소유한다. 본체와 갤러리 specimen 이
같은 함수를 부르므로 카드 모양이 두 벌이 될 수 없다.

- `ToastEntryView` / `ToastScopeView` / `ToastViewProps` — 그릴 준비가 끝난 입력. 시간도
  상태도 안 들어 있고 `alpha` 는 이미 계산돼 있다.
- `draw_toast_scopes(painter, props)` — 스택 배치 + 카드 chrome. `Context` 가 아니라
  `Painter` 를 받는 이유는 **떠오르는 자리가 부르는 쪽마다 다르기 때문**이다: 본체는
  `Order::Tooltip` 레이어 painter 를, 갤러리는 무대 frame 의 painter 를 넘긴다.
- `toast_layout_card(ctx, theme, message, max_width)` — 본문 galley 와 카드 크기(폭 × 높이).
  폭 상한을 **어디서 얻는가**(본체는 스코프 폭, 단일 카드는 `toast_max_width`)만 부르는 쪽이
  정하고, 줄바꿈 폭·패딩·accent 바·높이는 여기서 한 번만 정한다.
- `toast_card_colors(theme, kind, alpha)` — alpha 를 반영한 fill · border · accent · 글자 색.
  alpha 를 곱하는 순서가 여기 고정돼 있다 — 테마 색(straight)에 곱한 뒤 `Color32` 로 바꾼다.
  `Color32` 로 바꾼 뒤 곱하면 감마 공간 곱이 돼 alpha < 1 에서 색이 달라진다.
- `draw_toast_single_card(ui, theme, kind, message, alpha)` — 스택 없이 카드 한 장만
  그려야 하는 자리용(갤러리의 Toast · Toast stack specimen). 치수와 색은 위 두 함수에서 온다.
- `draw_toast_card` / `ToastCardColors` — 카드 chrome 만 그리는 하위 함수. 치수·색을
  부르는 쪽이 직접 채운다.
- `toast_accent_color(kind, theme)` — kind → 좌측 바 색.
- `toast_fade_alpha(age, lifetime, reduced_motion)` — 페이드 곡선. `Duration` 둘만 받아
  어떤 상태 타입도 안 본다.

**상태**는 `src/adapters/ui/toast.rs` 에 남는다 — host 를 봐야 하는 것들이다.

- `ToastKind` — Info / Success / Warning / Error. 정본은
  `crates/tasty-type-appearance/src/toast_kind.rs` 이고 여기는 재수출이다.
- `ToastScope` — 위 enum. 정본은 `crates/tasty-model/src/toast_kind.rs`.
- `ToastState` — id, message, kind, scope, spawned_at, lifetime.
- `ToastManager` — `push(message, kind, scope)` / `push_info(...)` / `draw(ctx, &LayoutContext, reduced_motion)`(만료 제거 + 렌더). `AppState::toasts` 로 통합, draw 는 popup draw 직후(= 위 레이어)에서.
- `compute_alpha` — `ToastState` 에서 곡선이 읽는 두 값을 꺼내는 어댑터. `ToastState` 가
  `ToastScope`(→ `tasty-model` → termwiz)를 품어 위젯 크레이트로 넘어가지 못한다.
- `truncate_message` — 캡 집행. `push` 진입부라 그리기 경로가 아니다.

> **본문 글자는 페이드하지 않는다.** `Fonts::layout` 에 색을 명시하면 그 색이 galley 에
> 박히고 `Painter::galley` 의 fallback 은 `Color32::PLACEHOLDER` 구간에만 쓰이므로,
> 지금 페이드하는 것은 카드 배경·보더·accent 바뿐이다. 갤러리 specimen 이 한때 글자까지
> 흐리게 그려 본체와 갈려 있었고 지금은 본체에 맞췄다.

모든 토스트 문자열은 `t("toast.*")` 키 — `lang/{en,ko,ja}.toml` 세 파일 동시 추가([i18n](../../dev-guide/i18n.md)).

## 관련

- [popup.md](popup.md) — 내부 팝업 시스템
- [banner.md](banner.md) — parent 상단 info+action 오버레이 (내용 적으면 Toast 권장)
- [identity](../../identity.md) — 사용자/에이전트 행동 분리
- [ADR-0380](../../adr/0380-the-toast-card-geometry-and-color-have-one-derivation.md) — 카드 치수·색 도출이 하나인 이유, 발화 금지 정책에 판정기를 안 지은 이유
