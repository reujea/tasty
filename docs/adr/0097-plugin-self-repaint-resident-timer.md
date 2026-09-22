# ADR-0097: plugin self-repaint 지연 알림은 프로세스 상주 타이머 스레드 1 개로 처리한다

- **Status**: Accepted
- **Date**: 2026-09-02
- **Tags**: plugin-sdk, egui-mesh, threading, self-repaint
- **Group**: plugin-render

## Context

egui-mesh surface/popup 은 egui 가 다음 pass 를 요청하면(`viewport_output[ROOT].repaint_delay`)
그 지연 뒤 host 에 `SurfaceInvalidated`/`PopupInvalidated` 를 보내 재-forward 를 받는다
([`egui-mesh-channel.md`](../dev-guide/egui-mesh-channel.md) "plugin self-repaint"). 이 신호가
하나라도 유실되면 유휴 상태에서 egui 내장 애니메이션이 끝까지 재생되지 못하고 방치되고
(스크롤 델타가 남은 채 정지), 중복되면 불필요한 `set_context` 왕복이 늘어난다.

plugin 프로세스의 메인 루프는 타임아웃 없는 blocking `read_line` 이라
([`runtime.rs`](../../crates/tasty-plugin-sdk/src/runtime.rs)) "지연 뒤 한 번" 을 걸 이벤트
루프 타이머가 없다. 그래서 지연 알림에는 별도 스레드가 필요하다는 전제 자체는 유효하다.

문제는 그 스레드를 **요청마다 새로 만들었다**는 점이다. `repaint_delay` 는 egui 가 즉시 다음
프레임을 원할 때 `0` 으로 오고(스크롤 스무딩의 `unprocessed_scroll_delta` drain 이 대표적),
그 경우 스레드는 `sleep` 도 없이 알림 한 번 보내고 죽는다. 애니메이션이 도는 동안 프레임마다
OS 스레드 생성·소멸이 반복된다.

## Decision

self-repaint 지연 알림은 **plugin 프로세스당 상주 타이머 스레드 1 개**(`SelfRepaintTimer`,
`crates/tasty-plugin-sdk/src/egui_surface.rs`)가 처리한다. 첫 요청 때 lazily 기동하고
(자체 repaint 를 한 번도 요청하지 않는 plugin 은 이 스레드를 갖지 않는다) 이후 프로세스
수명 동안 재사용한다. 요청은 `(마감시각, arm 가드, 발사 클로저)` 로 큐에 쌓이고, 스레드는
가장 이른 마감시각까지 `Condvar` 로 자다가 마감이 지난 요청을 락 밖에서 발사한다. 발사
순서는 **가드 해제 → 알림** 으로 기존과 동일하다. 인스턴스별 `AtomicBool` arm 가드는 그대로
유지해 대기 요청을 인스턴스당 최대 1 건으로 묶는다. `delay == 0` 도 예외 없이 같은 큐를
탄다(마감시각이 `now` 라 스레드가 즉시 깨어난다). 마감시각은 `checked_add` 로 계산해
단조 시계 범위를 넘기는 지연 요청은 (panic 대신) 버린다.

스레드가 하나뿐이라는 사실이 곧 단일 실패점이므로, **타이머 사망이 곧 self-repaint 영구
무음 정지**가 되지 않도록 세 겹으로 막는다: ① 발사 클로저 panic 은 요청 단위로 삼켜 루프를
유지하고, ② 그 밖의 이유로 루프가 풀리면 대기 요청의 arm 가드를 모두 풀고 `running` 을
되돌려 다음 요청이 스레드를 재기동하며, ③ 스레드 spawn 자체가 실패하면 그 요청만 1 회용
스레드(상주화 이전 경로)로 발사한다. 어느 경로든 `tracing::warn!`/`error!` 로 흔적을 남긴다.

## Consequences

- **얻은 것**: 애니메이션 중 프레임당 OS 스레드 생성·소멸이 사라진다. 스레드 수가 요청량과
  무관하게 일정해져 프로세스 상태 관측이 쉬워진다. 타이머가 `HostHandle` 을 모르는 순수
  스케줄러라(발사 클로저가 host 를 캡처) 유실 0 · 중복 0 을 host 없이 단위 테스트로 고정할 수
  있다.
- **잃은 것**: self-repaint 를 한 번이라도 쓰는 plugin 프로세스에 상주 스레드가 1 개 남는다
  (대부분 `Condvar` 대기 상태라 CPU 를 쓰지 않는다). 요청 등록에 `Mutex` 한 번이 붙는다.
  알림이 한 스레드에서 직렬 발사되지만 실제 병목인 host writer 뮤텍스는 상주화 이전에도
  공유였다. 스레드가 하나라는 점 자체가 단일 실패점이라 위 3 중 방어와 그만큼의 코드를
  떠안는다 — 그 방어가 없으면 실패 모양이 "원인 표시 없는 영구 정지" 라 수용할 수 없다.
- **운영 비용 / 유지 부담**: 대기 요청이 인스턴스당 1 건이라 큐는 `Vec` 선형 탐색으로 충분하다.
  surface/popup 수가 크게 늘어 선형 탐색이 문제되면 그때 우선순위 큐로 바꾼다.

## Alternatives Considered

- **A: `EguiMeshCore` 인스턴스마다 상주 스레드** — 마감시각 관리가 필요 없어 단순하지만,
  popup/surface 를 여러 개 쓰는 plugin 이 그 수만큼 상주 스레드를 갖는다. 전역 1 개로도
  마감시각 관리가 `Vec` + `Condvar` 수준이라 복잡도 이득이 비용을 넘지 않는다.
- **B: `delay == 0` 은 호출 스레드에서 곧바로 notify** — 스레드 경유를 완전히 없앨 수 있으나,
  `schedule_self_repaint` 는 `paint()`/`repaint_last()` 에서 **frame 송신보다 먼저** 불린다
  (frame 이 dedup 으로 생략돼도 self-repaint 는 걸려야 하므로 그 위치가 맞다). 인라인으로
  바꾸면 `*Invalidated` 가 `PaintFrame` 보다 **앞서** 나가 wire 상 이벤트 순서가 바뀐다.
  상주 타이머만으로 프레임당 스레드 비용은 이미 사라지므로, 관측 가능한 순서까지 바꾸는
  추가 위험을 지지 않는다.
- **D: 기존 `crates/tasty-timer` 재사용** — host 에 이미 타이머 허브가 있지만 부적합하다.
  `TimerHub` 는 콜백을 들지 않고 호출자가 매 프레임 `drain_due` 를 도는 **host 메인루프 전용**
  설계라, blocking `read_line` 이라 주기적으로 돌 지점이 없는 plugin 프로세스에는 그대로
  얹히지 않는다. 게다가 SDK 가 host 크레이트에 의존하게 되면 "plugin 은 무거운 host 의존
  없이 컴파일 가능해야 한다" 는 SDK 불변식이 깨진다.
- **C: 메인 루프를 타임아웃 있는 이벤트 루프로 교체** — 스레드 자체를 없앨 수 있지만 plugin
  런타임의 blocking `read_line` 구조를 갈아엎는 변경이라 이 문제의 크기에 비해 과하다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin 런타임 메인 루프가 타임아웃 기반 이벤트 루프로 바뀌어 별도 스레드 없이 지연
  알림을 걸 수 있게 될 때(대안 C 가 성립하게 됨).
- 한 plugin 프로세스가 동시에 다루는 egui-mesh surface/popup 수가 커져 `Vec` 선형 탐색이
  프로파일에 잡힐 때.
- self-repaint 요청 자체를 없애거나 크게 줄이는 상위 변경(예: host 가 애니메이션 구동을
  주도하는 구조)이 들어와 타이머 경로가 사실상 죽을 때.

## References

- [`docs/dev-guide/egui-mesh-channel.md`](../dev-guide/egui-mesh-channel.md) — "plugin self-repaint" 절 (채널 규약과 유실 시 재발하는 증상)
- [`docs/adr/0056-git-viewer-remote-attach-git-query-channel.md`](0056-git-viewer-remote-attach-git-query-channel.md) — popup 이 편승하는 `plugin_mesh_popup_pending_repaint` 예약 경로
- `crates/tasty-plugin-sdk/src/egui_surface.rs` — `SelfRepaintTimer` / `arm_self_repaint_timer`
- `crates/tasty-plugin-sdk/src/runtime.rs` — plugin 메인 루프(blocking `read_line`) 및 상주 스레드 선례(parent-death watchdog)
- `crates/tasty-timer/src/lib.rs` — host 메인루프 전용 `TimerHub`(대안 D 가 기각된 근거)
- [`docs/dev-guide/error-handling.md`](../dev-guide/error-handling.md) — 타이머 사망·spawn 실패를 삼키지 않고 남기는 기준
