# Action Dispatch (Intent 큐)

호스트 내부에서 모듈 경계를 넘는 동작(popup open, preset apply, surface split 등)을 **Intent 큐**로 통일 발화·처리하는 모델. plugin↔host 경계의 read 채널인 [Event Bus](../../reference/event-catalog.md)와는 **다른 layer**다. 코드: `src/intent.rs` + `src/intent/<domain>.rs`.

## 왜 큐인가

같은 동작(예: command palette popup 열기)을 단축키·도구 메뉴·IPC·plugin·우클릭 등 여러 진입점에서 발화한다. 직접 호출하면 `mem::take` 로 비워진 매니저에 호출돼 **noop 버그**가 나거나, draw 콜백 안에서 자기 매니저를 mutate 못 한다. **모든 진입점이 Intent 를 큐에 던지고, 메인 루프 단계에서 일관 처리**하면 발화 위치와 무관해진다.

## 용어

| 용어 | 정의 |
|------|------|
| **Intent** | 호스트 내부 명령 — "다음 메인 루프 단계에 무엇을 할지". 큐 통과. Event Bus 키와 다른 네임스페이스 |
| **Event** | [Event Bus](../../reference/event-catalog.md)의 사건(이미 일어난 일의 알림). plugin 구독. Intent 와 다른 layer |
| **Origin** | Intent 발화 주체 — `User { source }` / `Agent { source }`. 핸들러가 정책 분기에 사용 |
| **Cascade** | Intent A 핸들러가 발화하는 파생 Intent B. 별 origin 종류 없이 A 의 origin 전파 |
| **Bridge** | Intent 처리 결과 → Event Bus 변환 단일 지점(`meta.origin`/`trace_id` 채움) |

## 핵심 원칙 — Intent 의 위치

> **Intent 는 "의도" 다 — 흐름의 *시작점* 에만 존재한다. 끝에서 결과로 나오거나 중간에서 정보 전달용으로 쓰이면 안 된다.**

흐름은 한 방향: `이벤트 → 해석된 의도(Intent→큐) → 처리(drain) → 결과(state mutate / cascade)`.

- **이벤트→Intent 변환 함수 리턴은 OK**(시작점): `fn parse(e) -> Option<Intent>`.
- **처리 핸들러가 Intent 를 *리턴 경로로* 흘려보내면 금지** — cascade 는 핸들러 *본문 안에서* `state.dispatch_intent(...)` 로 큐에 enqueue. 반환 경로로 흘리면 호출 트리가 재귀가 되어 큐 모델이 깨진다.
- **Intent 는 응답 데이터를 갖지 않는다**(`Result<Intent>`, 응답 필드 금지). 응답이 필요한 mutate(새 ID 발급, 분기용 status)는 Intent 가 아니라 **Core method**(sync 리턴) 또는 **Query(read)** 로.

| 유형 | 메커니즘 | 응답 |
|------|----------|------|
| Query | `&CoreState` 직접 read | 데이터 |
| **Intent** (fire-and-forget) | enqueue → cascade | 없음 |
| Core method | `core.create_workspace(...) -> WorkspaceCreated` | sync 리턴 |

### 사용자 입력 대기 = 반드시 2 Intent 분리

"확인 popup → 응답 → 후속" 같은 *사용자 입력 대기* 는 **한 Intent 안에서 wait 하지 않는다.** 1차 Intent(popup 띄움 + 컨텍스트를 state 에 저장 + 종료) / 2차 Intent(응답 시 state 에서 읽어 진행 또는 폐기)로 분리한다.

```rust
Intent::PresetApplyRequest{kind,name} → handler: state.dialogs.pending_preset_apply = Some(ctx);
                                                  state.dispatch_intent(OpenPopup{"confirm_preset_apply"});
Intent::PresetApplyConfirmed → handler: let Some(ctx)=…take() else {return}; apply(ctx);
Intent::PresetApplyCancelled → handler: …pending_preset_apply = None;
```

이유: 사용자 응답은 *무한정* — 한 Intent 가 메모리에 lock 되면 그동안의 state 변경(window close, settings change)에 취약하고 큐가 정체된다. 명시적 분리는 *진행 중 컨텍스트* 가 state 에 보여 디버깅도 쉽다. (시스템 내부의 유한-시간 multi-step cascade 는 핸들러 본문 cascade enqueue 로 충분.)

## Intent 자료형

`Intent` 는 **flat enum**(nested 안 함). variant 는 `src/intent.rs`, 도메인 핸들러는 `src/intent/<domain>.rs`(popup/preset/surface/tab/pane/workspace). 발화 시 envelope 으로 감싼다:

```rust
pub struct DispatchedIntent { pub body: Intent, pub origin: IntentOrigin, pub trace_id: Option<String> }
pub enum IntentOrigin { User { source: UserSource }, Agent { source: AgentSource } }
//   UserSource: Shortcut(&str) / Menu(&str) / ContextMenu
//   AgentSource: Ipc / Plugin(String) / Cli
```

**Cascade origin**: popup A→B cascade 에서 B 의 origin 은 A 의 origin 을 그대로 명시 전달(`별 Cascade variant 없음`). "왜 이 popup 이 열렸나" 를 origin 이 정확히 가리켜 audit 에서 시작점 추적 가능.

## 발화 / 처리

- **발화**: `state.dispatch_intent(dispatched)` = `Vec::push` 한 줄(fire-and-forget). ergonomics 빌더 `.from_user_shortcut("…")` / `.from_user_menu("…")` / `.cascaded_from(intent)`.
- **발화(IPC 엔진 핸들러)**: 핸들러는 창을 받지 않으므로 요청 하나의 `IntentOutbox` 에 `out.push(dispatched)` 한다. 진입점(게이트 · 엔진 라우터 `dispatch_routed` · `record_plugin_rss_samples`)이 요청이 끝날 때 출구를 그 창 큐 끝으로 옮긴다 — 반환 경로가 아니라 본문 안의 enqueue 이고, 한 요청 안의 순서는 넣은 순서 그대로(게이트가 낸 것이 먼저)다. 근거: [ADR-0471](../../adr/0471-ipc-engine-handlers-reach-the-window-through-a-port.md).
- **처리**: 메인 루프의 `App::dispatch_pending_intents` 가 모든 window/parked_state 를 순회 drain. 처리 순서는 발화 순서 전체가 아니라 **클래스별 부분순서**(`classify_intent` 이 단일 정의점): 같은 state 내 non-Domain(`Immediate`) 은 FIFO 즉시 처리, `Intent::Domain` 은 전부 뒤로 밀려 단계 C(`run_domain_cascade`)에서 FIFO, `AppearanceChanged` 는 프레임 끝 1회로 축약. 따라서 같은 state 에서 Domain↔non-Domain 클래스 간 재정렬은 설계상 의도(borrow 분리 제약). drain 중 새로 발화한 Intent 는 **다음 프레임**(재진입 방지, `mem::take` 후 별 Vec 순회).
- **처리(headless)**: window 가 없어 위 drain 이 통째로 `gui` feature 게이트이므로, headless 는 `crate::intent::headless::drain_pending_intents` 가 engine 하나짜리로 같은 계약을 수행한다 — IPC 요청 처리 직후(**응답 송신 전**) · plugin 호출 결과 회신 전 · 메인 루프 블로킹 대기 직전. cascade 는 gui `handle_core_event` 중 engine 상태로 완결되는 것만(attention · notification 적재 · terminal mark · settings 적용/저장) 수행하고(OSC 7 cwd 의 탭 이름 갱신은 intent 가 아니라 PTY drain 이 `intent::headless::apply_terminal_cwd_changed` 로 직접 한다), view redraw/toast/theme 재설치·알림음은 소비처가 없어 제외한다. 근거·범위: [ADR-0111](../../adr/0111-headless-drains-the-intent-queue.md).
- **host event(headless)**: 같은 세 지점과 PTY 출력 처리 끝(`src/boot.rs`)에서 `crate::intent::headless::drain_pending_host_events` 가 `pending_host_events` 를 통째로 비우되, 소비자가 plugin event bus 뿐이 아닌 종류만 적용한다 — 오늘 그것은 `HookFired` 하나이고 push 완료 전략으로 대기 중인 agent task 를 마감한다. 나머지 종류는 버린다(headless 에 bus 발화 배선이 없다). 근거·대안·재검토 트리거: [ADR-0134](../../adr/0134-headless-drains-host-events-but-applies-only-hook-fired.md).
- **핸들러 분기**: trait dispatch 아니라 **도메인 함수 분기**(`match &intent.body`). `&mut AppState` 를 trait object 가 통째 잡으면 partial mutation 이 borrow checker 를 못 통과하기 때문.
- **에러**: 핸들러 안에서 `tracing::warn!`(패닉 금지, `let _=` 금지). 사용자에게 보여야 하면 `state.toasts.push`.

## Intent → Event Bus Bridge

핸들러가 mutation 성공 후 `state.pending_host_events` 에 host event 를 push → 단일 bridge(`src/app/dispatch/host_events.rs` → `PluginManager::emit_host_event`)가 [Event Bus](../../reference/event-catalog.md) envelope 으로 변환:

| envelope | 규칙 |
|----------|------|
| `meta.origin` | 항상 `{kind:host}`(`EventOrigin::Host`) — intent origin 을 반영하지 않는다 |
| `meta.trace_id` | 이벤트마다 새 발급(`h<seq 16진>`) — intent 값을 이어받지 않는다 |
| `meta.scope` | 핸들러가 mutation 결과 보고 채움 |
| `meta.hop` | 호스트 발화이므로 `0` |

## User vs Agent 정책

dispatcher 가 강제 거부하지 않고 **핸들러 작성자가 `origin.is_user()` 로 분기**(PR 리뷰 강제):

| 동작 | User | Agent |
|------|------|-------|
| popup open(UI Intent 발화) | 가능 | **금지**(debug 사용자 입력 재현 IPC 제외) |
| closed-tab restore push · OS 윈도우 focus · Workspace activate · Window 생성 | 가능 | **금지**(focus 독립) |

> **UI Intent vs Domain Intent**(제1 분류축): UI(popup open/close/toggle) 또는 Domain(나머지). release 의 Domain 처리 흐름은 UI Intent 를 발화할 수 없고 **타입 차원에서 강제**된다(Core 가 `UiIntent` 를 모름). 자동 popup/toast/dialog 는 release 에 없다. 자매 정책: [popup](../systems/popup.md) · [toast](../systems/toast.md) 의 "발화 정책 (CRITICAL)".

## intent-discipline 강제

도메인 직접 **변이** 호출은 `scripts/check-intent-discipline.sh` 가 금지한다 — popup 만이 아니라 preset · surface · tab · pane · workspace 여섯 도메인이다. 채널은 `script-gates.yml`(main push · PR).

술어는 **변이 API 를 열거**한다(`open`/`open_centered`/`open_centered_focused`/`open_with_scope`/`open_at_top_of_scope`/`open_at_focused`/`close`/`toggle*`). 질의(`is_open`·`get_mut`·`open_geometry`)는 대상이 아니다 — 접두어로 뭉뚱그리면 질의가 변이로 잡힌다.

**대상이 아닌 자리 셋**: ① 주석·문자열 리터럴(코드가 아니다) ② `#[cfg(test)] mod` 본문과 `*_tests.rs`(도메인이 **피험자**다 — 규율은 도메인을 *도구로* 쓸 때의 규칙이다) ③ 이름만 같은 다른 타입의 메서드(패턴별 면제 경로).

예외는 `// intent-exempt: <사유>` 주석으로 suppress — **같은 줄 · 바로 위 · 바로 아래** 어디든 인정한다. 현재 예외는 popup 자기-close cleanup(on_close 훅에서 큐의 다음 항목), 처리 핸들러 본문의 cascade, 응답이 필요한 mutate(Core method sync 리턴), 그리고 focus 보존을 위해 큐를 우회한 자리 하나다. 호스트 모든 Intent 가시화는 debug 전용 `intent::watch`(`src/intent/watch.rs`)가 `tracing::debug!` 로(release 제거). 부르는 자리가 GUI 메인 루프의 drain 이라 gui 빌드에만 있다 — headless 의 drain(`intent::headless`)은 이 로그를 안 남긴다.

## dedup

같은 popup id 의 OpenPopup 이 동일 사이클에 중복 들어오면 핸들러가 `state.popups.is_open(id)` 로 두 번째를 무시(큐 push 시점엔 dedup 안 함 — origin/trace_id 다른 둘을 같다 볼 근거 없음).

## 관련

- [popup](../systems/popup.md) · [toast](../systems/toast.md) — 발화 정책 · [focus](../policies/focus.md) — 독립성
- [reference/event-catalog](../../reference/event-catalog.md) — Event Bus envelope · [dev-guide/popup-implementation](../../dev-guide/popup-implementation.md)
