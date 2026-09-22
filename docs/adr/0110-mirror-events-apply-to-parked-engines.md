# ADR-0110: 창 없는 parked engine 에도 mirror 이벤트를 즉시 적용한다 — 버퍼링·흐름제어 대신 로컬 PTY 출력과 같은 대칭

- **Status**: Accepted
- **Date**: 2026-09-04
- **Tags**: remote-attach, mirror, parked-engine, multi-window, data-loss, performance
- **Group**: remote-attach

## Context

attach 세션의 수명은 창이 아니라 engine 에 매인다 — 마지막 창을 닫거나 macOS 에서 최소화하면 창은 사라지지만 engine 은 `App.parked_states` 에 `(AppState, CoreState)` 로 살아 있고, mirror 워크스페이스·mirror 터미널·remote→local 매핑도 그 안에 남는다([remote-attach — 창 없는 상태(parked)에서의 세션 수명](../features/remote-attach/index.md#창-없는-상태parked에서의-세션-수명), [ADR-0087](0087-layout-slot-occupancy-model.md)). 고아 판정(`mirror_workspace_engine_alive`)과 정리(`cleanup_mirror_workspace`)는 그래서 창 있는 engine 과 parked engine 을 **함께** 순회한다.

그런데 mirror 이벤트를 적용하는 `apply_attach_client_output`(`src/app/attach_client.rs`)은 두 가지가 어긋나 있었다.

1. **적용 대상을 창에서만 찾았다.** `find_main_with_workspace` 는 창(`MainView`)만 본다. parked engine 은 후보가 아니었다.
2. **drain 이 탐색보다 먼저였다.** reader thread 가 쌓은 `Vec<MirrorEvent>` 를 먼저 `mem::take` 한 뒤 대상을 찾았으므로, 대상이 없으면 이미 꺼낸 이벤트를 되돌릴 수 없어 그대로 버려졌다.

결과, 세션은 살아 있는데 창이 없는 구간에 도착한 출력이 전부 폐기됐다 — `MirrorEvent::Data` 는 복원 뒤 mirror 화면의 영구 결손으로, `MirrorEvent::StructuralDelta` 는 매핑 desync(그 뒤 새 원격 surface 의 Data 가 라우팅되지 않고 정리 시 그 터미널이 누락)로 이어진다. 이 함수는 창 유무와 무관하게 `AttachClientData` wake 와 `Tick::AttachView` 3초 tick 으로 계속 호출되므로 유실은 구간 내내 지속된다.

도달 경로는 현재 macOS 최소화뿐이다 — `handle_minimize` 가 창을 파괴하고 engine 을 park 한다. Linux/Windows 의 최소화는 창을 유지(hide 또는 taskbar minimize)하고, 마지막 창 닫기는 quit 흐름(`handle_quit_requested`)이라 mirror 를 든 parked engine 이 만들어지지 않는다. 그럼에도 판정·정리가 parked 를 포함하는 이상 **적용도 포함해야 구조가 닫힌다** — 세 순회 중 하나만 범위가 다르면 그 차이만큼 조용한 유실이 생긴다.

제약:

- `StructuralDelta` 는 버릴 수 없다(매핑이 깨진다). 어떤 방향이든 구조 delta 는 반드시 적용되거나 보존돼야 한다.
- 에이전트/원격 행동의 부수효과가 사용자 상태에 닿지 않는다([identity 2.1](../identity.md)) — 적용은 engine 내부 상태(터미널 grid · 트리 · 매핑)만 바꾸고 포커스·창·활성 워크스페이스는 건드리지 않는다.
- 로컬 PTY 출력은 **이미** parked engine 에서도 파싱된다(`handle_terminal_output` 의 `parked_states` 순회, `src/app/event_handler.rs`). 창 없는 동안의 파싱 비용은 로컬 터미널에 대해 이미 수용된 정책이다.

## Decision

**방향 1 — parked engine 에도 즉시 적용한다.** `apply_attach_client_output` 은 적용 대상을 `mirror_output_host` 로 **창 있는 engine → parked engine** 순으로 고르고, **대상을 찾은 뒤에만** 버퍼를 비운다(`MirrorHost::drain_and_apply`). 대상이 어느 쪽이든 같은 `(AppState, CoreState)` 쌍을 `MirrorHost` 로 감싸 같은 본문(`apply_mirror_events` → `apply_one_mirror_event`)을 탄다 — 창 있는 경로와 parked 경로의 적용 규칙은 한 지점에서만 정의된다. 세 순회(고아 판정 · 정리 · 적용)의 범위는 같다.

세부:

- **대상이 없으면 drain 하지 않는다.** 어느 engine 에도 그 워크스페이스가 없으면 세션은 고아이고 같은 프레임의 `detach_orphaned_mirror_sessions` 가 세션째 정리한다(버퍼도 함께 drop). 적용 대상이 없는데 꺼내는 일이 구조적으로 사라진다.
- **창 유무는 부수효과만 게이트한다.** parked engine 에는 toast 를 쌓지 않고 로그로 남긴다 — 창이 없어 표면이 없고 토스트 수명이 wall-clock 이라 복원 시점엔 이미 만료돼 보이지도 않는다(정리 경로의 기존 방침과 동일). repaint 요청도 창이 없으니 생략한다 — 복원 시 새 창이 그 engine 의 터미널 grid 를 그대로 그린다.
- **적용 순서는 도착 순서다.** Data/Resize/StructuralDelta 가 한 큐에 담겨 있으므로 resize 앞뒤 출력이 올바른 grid 에서 재생되고, delta 로 갱신된 매핑을 같은 drain 안의 후속 Data 가 즉시 쓴다.

## Consequences

- **얻은 것**: 창 없는 구간의 출력·구조 변경이 유실되지 않는다. 복원 순간 mirror 는 이미 최신 상태다(일괄 재생 지연 없음). 판정·정리·적용의 순회 범위가 일치해 "살아 있다고 판정된 engine 에 적용이 닿지 않는" 상태가 존재하지 않는다.
- **잃은 것**: 창이 없는 동안에도 mirror 출력의 VT 파싱이 메인 스레드에서 돈다. 실측(아래) — 다만 이 비용은 창이 있을 때도 **똑같이** 드는 비용이다(`Terminal::feed_bytes` 는 창 유무와 무관하게 같은 호출). parked 가 새로 얹는 비용은 0 이고, 렌더 비용만 빠진다.
- **파싱 비용 실측**(release 빌드, `Terminal::new_detached` 에 16 MiB 를 4 KiB 청크로 `feed_bytes`, 20 코어 머신에서 loadavg 74 의 부하 중 측정 — 그래서 **CPU 시간** 기준으로 적는다. 벤치 소스는 저장소 밖 임시 프로젝트):

  | 출력 종류 | 80×24 | 200×50 |
  |---|---|---|
  | plain(80자 줄) | 0.80 ms / 4 KiB · 4.9 MiB/s | 0.72 ms · 5.4 MiB/s |
  | SGR 색상 다수(빌드 로그류) | 0.37 ms · 10.5 MiB/s | 0.47 ms · 8.3 MiB/s |
  | 커서 제어 + 진행바(`\x1b[2K\r`) | 0.84 ms · 4.7 MiB/s | 1.18 ms · 3.3 MiB/s |
  | `yes`(2 바이트 줄 — 줄마다 스크롤, 최악) | 3.2 ms · 1.2 MiB/s | 6.1 ms · 0.6 MiB/s |

  해석: 원격이 1 MiB/s 를 계속 쏟아내는 극단에서도 일반 출력은 코어 한 개의 10–20 %, `yes` 류 최악 출력은 한 코어를 다 쓴다. 실제 원격 터미널 출력(빌드 로그·에이전트 대화)은 초당 수십 KiB 수준이라 1 % 미만이다. 위 수치는 부하 중 측정이라 상한에 가깝다.
- **e2e 실측**(loopback attach, 창 있는 client 로 측정 — parked 도 같은 파싱 호출): 아래 "e2e 실측" 절.
- **운영 비용 / 유지 부담**: 세 순회(판정 `mirror_workspace_engine_alive` · 정리 `cleanup_mirror_workspace` · 적용 `mirror_output_host`)의 범위를 함께 유지해야 한다. 각 함수 doc 가 서로를 가리키고, 적용 경로는 parked 단위 테스트(`parked_engine_receives_mirror_data_and_structural_delta`, `mirror_output_host_prefers_window_then_parked_then_none`)가 고정한다. `apply_one_mirror_event` 에 창 표면이 반드시 필요한 부수효과를 추가할 때는 `MirrorHost::windowed` 게이트를 거쳐야 한다.

  부수효과 게이트와 적용 경로는 다음 두 테스트가 고정한다(각각 해당 배선을 되돌리는 변이에서 실패하는 것을 확인했다):

  | 테스트 | 고정하는 성질 |
  |---|---|
  | `a_host_drains_and_applies_the_mirror_buffer` | host 가 있으면 버퍼를 비우고 실제로 적용한다 |
  | `parked_host_does_not_stack_toasts_but_windowed_does` | 창 유무가 부수효과만 게이트한다 |

  `no_host_leaves_the_mirror_buffer_untouched` 는 여기 없다 — **집행 지점이 아니라 의도 기록**이다. "host 가 없으면 꺼내지 않는다" 는 아래 타입 경계가 지탱하므로 위반하는 코드가 컴파일되지 않고, 이 테스트가 잡을 수 있는 것은 `None` 분기가 `true` 를 보고하는 정도다. 위 `a_host_…` 의 대칭축(그 테스트만 있으면 "아무것도 안 하는 함수" 가 통과한다)으로 남겨둔다.

  **무엇이 무엇을 지탱하는가** — 위 Decision 의 "적용 대상이 없는데 꺼내는 일이 구조적으로 사라진다" 를 지탱하는 것은 호출 순서 약속도, 테스트도 아니고 **모듈 경계 + 시그니처** 둘의 조합이다.

  - **모듈 경계**: mirror 버퍼는 `mod outbox` 안의 `MirrorOutbox` 가 들고, 내부 `Mutex` 필드는 그 모듈 밖에서 보이지 않는다. Rust 의 private 은 **모듈 범위**라 이 경계가 없으면 같은 파일(4,600 줄, 결함이 살던 바로 그 파일)의 다른 함수가 필드에 그대로 닿는다 — 실제로 리뷰에서 `sess.output.events.lock()` 여섯 글자로 결함이 되살아났다.
  - **시그니처**: 밖으로 열린 유일한 비우기 경로 `MirrorOutbox::take_for(&self, host: &MirrorHost)` 가 적용 대상을 인자로 요구한다.

  둘 중 하나만으로는 부족하다. 그래서 "꺼냈는데 적용 대상이 없다" 는 상태를 쓸 수가 없고, 호출부의 2차 조회(`as_main_mut()`)가 실패해도 그 시점엔 아직 꺼내지 않았다. 꺼내는 일과 적용하는 일은 `MirrorHost::drain_and_apply` 가 함께 쥔다.

  이 봉인의 범위는 **프로덕션 빌드**다 — 테스트 빌드에는 버퍼를 직접 채우고 들여다보는 `#[cfg(test)]` 접근자(`MirrorOutbox::peek`)가 있고, 그것을 읽기 전용으로 좁히려면 `MirrorEvent` 에 `Clone` 요구가 새로 생긴다(테스트 관심사 때문에 프로덕션 타입에 트레잇 요구를 얹는 것이라 비용 방향이 반대다). 보장하는 것보다 많이 주장하지 않기 위해 적어둔다.

  이 형태가 되기 전에는 같은 단언을 **소스 문자열 스캔**(호출부 본문에 drain 호출이 없는지 보는 테스트)이 지탱했다. 그 가드는 함수 이름 하나만 봤으므로 우회 형태 어느 것도 잡지 못했고(인라인 `mem::take` · 필드 직접 접근 — Gate4 리뷰가 둘 다 실측했다, 결함을 부활시켰는데 테스트 전부 통과), 반대로 무관한 함수의 doc 한 줄 편집에는 거짓 실패를 냈다.

  위 두 경계가 그 우회 형태를 컴파일 단계에서 막으면서 그 가드의 술어는 공허해졌고, 그래서 지웠다 — 없어도 되는 가드가 남아 있으면 다음 사람이 무엇이 진짜 집행 지점인지 헷갈린다. (그 가드가 내던 거짓 실패도 함께 사라졌지만, 그건 앵커를 고쳐서가 아니라 **테스트를 지워서**다.)

### e2e 실측 (loopback attach, 2026-09-04)

동일 머신 loopback attach(서버·client 모두 GUI debug 인스턴스, 창 있음, client mirror terminal 158×52, loadavg 35~50 / 20 코어)로 client mirror surface 에서 명령을 보내 서버 surface 가 16 MiB 를 출력하게 하고, 마커 줄이 client mirror 화면에 나타날 때까지 client 프로세스의 CPU 시간(`/proc/<pid>/stat` utime+stime)·RSS 증가분을 쟀다. client 수치는 mirror 파싱에 더해 창 repaint(`AttachMirror` dirty)와 폴링 IPC 응답까지 포함한 **창 있는 경로의 총량**이다 — parked 는 그중 repaint 가 빠지므로 이 값을 parked 의 상한으로 읽는다.

| payload (16 MiB) | client CPU 증가 | MiB 당 | client RSS 증가 | 서버 CPU 증가 |
|---|---|---|---|---|
| `yes` (2 byte 줄, 줄마다 scroll) | 38.57 s | 2.41 s/MiB (0.41 MiB/s) | +11.3 MB | 39.55 s |
| `base64 /dev/urandom` (76 열 줄) | 5.21 s | 0.33 s/MiB (3.1 MiB/s) | +0.7 MB | 5.31 s |

client 와 서버(자기 terminal 파싱 + 렌더 + PTY/stream 처리)의 CPU 증가분이 같은 자릿수다 — mirror 측 파싱이 원본 측보다 비싸지 않다. RSS 증가는 mirror terminal 의 scrollback 이 차는 몫이다.

parked 상태 자체는 이 머신(Linux)에서 production 경로로 도달할 수 없다(macOS minimize 전용) — parked 적용은 단위 테스트(`parked_engine_receives_mirror_data_and_structural_delta`)로 확인했고, 위 e2e 는 창 있는 경로(Data · StructuralDelta 왕복)가 host 추상화 이후에도 동일하게 동작함과 파싱 비용의 상한을 준다.

## Alternatives Considered

- **A. 버퍼링 후 복원 시 일괄 적용** — 창이 없으면 drain 하지 않거나 별도 큐에 쌓고 복원 시 순서대로 적용. 상한이 필요한데 어떤 정책도 손해다: 오래된 것을 버리면 Data 유실이 재발하고, `StructuralDelta` 는 버릴 수 없어 Data 와 분리해야 하지만 분리하면 도착 순서(resize/delta 와 Data 의 상대 순서)가 깨진다. 상한 없이 두면 메모리가 원격 출력량에 비례해 무한히 자란다. 복원 시점에 수십 MiB 를 한 번에 파싱하면 메인 스레드가 수 초 멈춘다(위 표 기준 16 MiB ≈ 3–25 초). 로컬 PTY 출력이 parked 에서 즉시 파싱되는 것과도 비대칭이다. 기각.
- **B. 원격에 흐름 제어 요청** — 창이 없는 동안 서버에 전송 보류를 알린다. 스트림 프로토콜 변경(양방향 control 추가)이 필요하고, 서버가 그 client 를 위해 출력을 버퍼링해야 하므로 문제가 서버 메모리로 옮겨갈 뿐 사라지지 않는다(headless 서버 포함). 원격 PTY 자체를 막을 수는 없다(다른 소비자·서버 자신의 화면이 있다). 범위가 가장 크고, 얻는 것은 client 파싱 CPU 절약뿐인데 그 비용이 위 실측대로 작다. 기각.
- **C. 창 유무로 세션을 끊는다(선행 수정 이전 동작)** — 사용자가 창을 최소화했을 뿐인데 원격 점유가 풀리는 더 큰 결함이라 검토 대상이 아니다.
- **D. drain 순서만 고치고 parked 는 미적용(버퍼에 계속 누적)** — 유실은 없지만 A 의 무제한 버퍼와 같아지고, 복원 시 일괄 파싱 스파이크가 남는다. 기각.

## Reconsideration Triggers

- parked 구간의 파싱이 실사용에서 체감 문제(배터리·팬·응답성)로 보고될 때 — B(흐름 제어)를 다시 검토한다.
- 로컬 PTY 출력이 parked engine 에서 파싱을 중단하는 정책으로 바뀔 때 — 이 결정의 대칭 근거가 사라진다.
- `apply_one_mirror_event` 가 창 표면이 필수인 부수효과(toast 이상의 것)를 갖게 될 때 — `MirrorHost` 게이트만으로 충분한지 재검토한다.
- mirror 워크스페이스가 `App.core_state`(첫 창 등록 전 임시 engine)에도 만들어질 수 있게 바뀔 때 — 판정·정리·적용 세 순회의 범위를 함께 넓혀야 한다.

## References

- [remote-attach 기능 문서 — 창 없는 상태(parked)에서의 세션 수명](../features/remote-attach/index.md#창-없는-상태parked에서의-세션-수명)
- [dev-guide/attach-behavior.md](../dev-guide/attach-behavior.md) — 세션 종료·정리 경로
- [architecture/multi-window.md — parked states](../architecture/multi-window.md#parked-states--pty-생존)
- [ADR-0087](0087-layout-slot-occupancy-model.md) — parked engine 은 레이아웃 슬롯 점유를 유지한다
- [identity.md](../identity.md) — 2.1 사용자 행동 ↔ 에이전트 행동 분리
