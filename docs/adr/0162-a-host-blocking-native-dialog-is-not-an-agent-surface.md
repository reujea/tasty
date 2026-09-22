# ADR-0162: 호스트를 막는 네이티브 다이얼로그는 에이전트 표면이 아니다 — `fs.pick_file` 을 뺀다

- **Status**: Accepted
- **Date**: 2026-09-05
- **Tags**: ipc, agent-surface, gui, blocking, rfd, portal, identity-principle, adr-0042, adr-0058, adr-0091

## Context

[ADR-0042](0042-fs-pick-file-native-dialog-host-delegation.md) 는 "파일 열기" 를 host `fs.pick_file` IPC 로 위임했다.
그 결정의 전제는 본문에 그대로 적혀 있었다 — 모달 블로킹인 `rfd::FileDialog::pick_file()` 을
**메인 스레드에서 동기로 열어도 안전하다**. 이 ADR 은 그 전제를 실측으로 반증하고 메서드를 뺀다.

### 이 메서드를 지금 재는 이유

호출자가 이미 없다. ADR-0042 의 유일한 in-tree 소비자였던 markdown plugin 의 browse 버튼은
[ADR-0058](0058-plugin-triggered-host-popup-async-ack-push.md) 의 `file_picker.trigger`
(즉시 ack + `event.dispatch` 유니캐스트 푸시)로 옮겨 갔고, 원격 attach 쪽 파일 선택은
[ADR-0053](0053-native-file-picker-remote-attach-channel.md) 이 in-tree 피커 popup 위에 따로 세웠다.
제거 직전 트리에서 `fs.pick_file` 이라는 이름이 남아 있던 자리는 **핸들러 자신 · 라우터 arm ·
메서드 표 한 줄, 그리고 낡은 비교 주석들**뿐이었다 — 실제로 이 메서드를 부르는 코드는 0 개다.
ADR-0042 의 Status 도 자기 용례를 이미 그 하나로 한정하고 있었다.

### 실측 — 전제는 거짓이다

Linux(XDG desktop portal 없음) 호스트에서 debug 인스턴스를 띄우고 잰 값이다.
비교군으로 같은 인스턴스의 평시 IPC 왕복은 **0.01–0.28 s**.

| 잰 것 | 결과 |
|-------|------|
| `fs.pick_file` 이 미결인 동안 `system.info` / `surface.list` / `window.list` | 셋 다 타임아웃 |
| 그동안 생성된 다이얼로그 창 | 0 개 (창 census) |
| t = 15 / 30 / 45 / 60 / 90 s 회복 | 다섯 시점 모두 회복 없음 |
| 타임아웃 120 s 로 참고 기다린 클라이언트 | 끝내 응답 없음 |

원인은 백엔드에 있다. rfd 0.15.4 의 Linux 백엔드는 D-Bus 위의 XDG desktop portal(ashpd)이고
**완료에 상한이 없다**. 포털 서비스가 없는 호스트에서는 완료 이벤트 자체가 발생하지 않으므로,
"조금 느리다" 가 아니라 **끝나지 않는다**. 그리고 이 상태를 되돌릴 것이 트리에 없다 —
[ADR-0091](0091-render-stall-watchdog-observation-only.md) 의 stall 워치독은 관측 전용이라
멈춘 메인 스레드를 회복시키지 않는다(그 ADR 의 적용 지점 목록에 이 호출이 있었다).

즉 한 번의 에이전트 호출이 **인스턴스 전체를 무기한 무응답으로 만든다.** 로컬 사용자도,
다른 에이전트도, attach 한 원격 사용자도 같이 멈춘다.

### 원칙 1 은 이 메서드를 어디에 두라고 말하는가

원칙 1 의 판단 기준은 *에이전트가 자기 작업을 하기 위해 필요한가(→ release) vs 사용자가 직접
하는 조작을 재현하는가(→ debug)* 다([identity](../identity.md) 2.1,
[debug-ipc](../dev-guide/debug-ipc.md)). `fs.pick_file` 은 **둘 다 아니다.**

- 에이전트가 자기 작업을 하는 것이 아니다 — 이 호출의 답은 에이전트가 만들지 않는다.
  **키보드 앞의 사람**이 고른 뒤에야 값이 생긴다.
- 사용자 입력의 *재현*도 아니다 — 에이전트가 사용자의 제스처를 대신 흉내 내는 것이 아니라,
  사용자에게 제스처를 **요구**한다. debug 격리의 목적은 자기검증(사용자 전용 동작을 IPC 로
  구동해 에이전트가 스스로 확인하는 것)인데, 사람이 골라줘야 끝나는 호출은 그 목적을 못 채운다.
  게다가 위 실측은 빌드 프로필과 무관하다 — debug 로 옮겨도 debug 인스턴스가 똑같이 멈춘다.

원칙 1 이 이 축을 판정하지 못하는 것은 이 메서드가 그 이분법 **밖**에 있기 때문이다. 판정하는
것은 다른 두 항이다.

- **2.1 ①** — 에이전트 행동의 부수효과가 사용자 상태에 닿지 않는다. 네이티브 **모달**
  다이얼로그는 정의상 사용자의 입력 포커스를 가져가고 그 창을 막는다. 에이전트 호출이 그것을
  연다는 것 자체가 ① 위반이다.
- **2.3** — 포커스는 사용자의 것이고 release 에는 포커스를 바꾸는 API 가 없다. 이 메서드는
  그 API 였다.

그리고 **2.2 의 "파일 열기"** 는 이 메서드가 아니어도 충족된다 — 그것이 `file_picker.trigger`
(ADR-0058)가 하는 일이고, 그쪽은 즉시 ack 한 뒤 사람의 답을 이벤트로 뒤늦게 밀어준다.
사람의 개입이 필요한 호출을 **즉답 + 나중 푸시**로 쪼개는 것이 이 저장소의 답이며,
`approval.await`(워커 스레드에서 지연 응답)도 같은 형태다.

## Decision

`fs.pick_file` 을 **제거한다.** 전용 핸들러 모듈 · 라우터 arm · `METHOD_TABLE` 등재를
함께 뺀다. ADR-0042 는 이 ADR 로 Superseded 다.

`fs` prefix 는 **예약된 채로 남긴다** — 그 아래 호스트 메서드가 0 개가 됐지만, 이름이 비었다고
plugin 에 내주면 `fs.*` 가 호스트 파일시스템 표면처럼 읽히는 자리를 남의 것으로 만든다.
예약을 미리 막아 두는 쪽이 나중에 뺏는 것보다 싸다는 [ADR-0140](0140-host-ipc-prefixes-are-reserved-where-they-can-be-enforced.md)
의 판단을 그대로 따른다.

일반화한 규약: **호출 하나가 호스트 전체를 무응답으로 만들 수 있고 그 대기에 상한이 없으면,
그것은 에이전트 표면이 아니다.** 사람의 개입이 필요한 동작은 트리거 + 비동기 결과 푸시로
쪼개서 노출한다(ADR-0058). 네이티브 모달을 여는 일은 **사용자가 버튼을 눌러서** 시작한다 —
남아 있는 `rfd::FileDialog` 호출 지점 넷(설정 scripts / remote-transfer · 프리셋 필드 ·
플러그인 추가)이 전부 그 형태다.

## Consequences

- **얻은 것**: 에이전트 호출로 인스턴스를 무기한 멈추는 경로가 사라졌다. `METHOD_TABLE.len()`
  이 277 → 276(제거 직전·직후 실행 측정), 호스트 prefix 는 46 → 45 — `fs.*` 가 통째로 비었다.
  release 표면에서 사용자의 입력 포커스를 가져가는 모달을 여는 메서드가 없어졌다.
- **잃은 것**: "에이전트가 요청하면 OS 네이티브 다이얼로그가 뜨고 그 경로를 돌려준다" 는
  동기 형태는 이제 없다. 대신 `file_picker.trigger` 가 있고, 이쪽은 즉답 후 푸시라 호출 규약이
  다르다(응답을 기다리는 대신 이벤트를 구독해야 한다).
- **운영 비용 / 유지 부담**: 없어진 코드라 유지 부담은 음수다. 문서 락스텝만 남는다 —
  api-conventions 의 gui-only 표에서 한 행이 빠지고 그 수가 30 → 29 가 됐다.

## Alternatives Considered

- **(a) `AsyncFileDialog` 로 바꿔 비동기화한다** — 기각. rfd 0.15.4 에 `AsyncFileDialog` 는
  있지만, 이 메서드를 부르는 자리가 **plugin host-call 경로**다. 그 경로는 host 가 구현한
  메서드에 대해 **같은 tick 안에서 인라인으로 답한다** — `src/app/dispatch/plugin_ipc.rs` 6 곳,
  헤드리스 `src/boot/headless_plugins.rs` 3 곳이 전부 그 형태고, 지연 응답 슬롯
  (`PendingRequestKind`)은 **plugin→plugin 포워딩용**이라 "host 가 나중에 답한다" 를 담지
  않는다. 그래서 (a) 의 선행 작업은 다이얼로그 교체가 아니라 **host-call 경로에 host-측 지연
  완료 종류를 새로 만들고 두 조합(gui dispatch + headless pump)에 각각 배선하는 것** —
  최소 9 개 인라인 응답 지점이 걸린 축이다. 외부 IPC 경로에는 이미 지연 응답이 있지만
  (`spawn_*` 3 종 — `src/core/app_surface.rs` 의 `spawn_remote_workspaces` 와, 대기 본문
  옆으로 옮겨 간 `ipc::handler::agent::task::spawn_task_await` ·
  `ipc::handler::approval::spawn_approval_await`) plugin caller 는 그 채널을 안 탄다.
  그 선행 작업의 크기가 이 축(호출자 0 개인 메서드 하나)보다 크고, 다 만들어도 결과는
  ADR-0058 이 이미 제공하는 것과 같은 형태다 — 즉 (a) 는 ADR-0058 을 다시 여는 일이다.
- **(b) 타임아웃을 걸어 유한 시간에 실패시킨다** — 기각. 메인 스레드가 막힌 동안은 타이머도
  못 돈다. 워커로 옮기면 그건 (a) 이고, macOS 백엔드는 `dispatch2::run_on_main` 으로 메인
  스레드에 마셜하므로 "워커에서 열면 된다" 가 세 OS 에서 성립하지도 않는다(원칙 4:
  한 OS 에서 되는 것이 다른 OS 에서 되는 근거가 아니다).
- **(c) debug 로 옮긴다** — 기각. 위 "원칙 1 은 어디에 두라고 말하는가" 참조: 자기검증이라는
  debug 표면의 목적을 못 채우고, 멈춤은 프로필과 무관하다.
- **(d) 그대로 둔다** — 기각. 호출자가 0 개인데 인스턴스를 멈출 수 있는 표면을 남기는 것은
  비용만 있다. 남겨두면 다음 사람이 "표에 있으니 써도 되는 것" 으로 읽는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin host-call 경로에 **host-측 지연 완료** 채널이 (다른 축의 필요로) 실제로 생겼을 때 —
  (a) 의 선행 작업이 이미 지불된 상태가 되므로 동기 규약을 원하는 소비자가 있으면 다시 잰다.
- 세 OS 전부에서 **상한 있는** 네이티브 파일 다이얼로그가 가능해졌을 때(rfd 또는 그 대체가
  타임아웃/취소 토큰을 제공).
- `file_picker.trigger` 의 즉답+푸시 규약으로 표현할 수 없는 파일 선택 요구가 나왔을 때.

## References

- [ADR-0042](0042-fs-pick-file-native-dialog-host-delegation.md) — 이 ADR 이 대체하는 결정
- [ADR-0058](0058-plugin-triggered-host-popup-async-ack-push.md) — 트리거 + 비동기 ack + 푸시
- [ADR-0504](0504-the-file-picker-trigger-answers-only-a-plugin-caller.md) — `file_picker.trigger` 를 plugin 호출자 전용으로 닫은 후속 결정(에이전트의 파일 열기는 경로 직접 지정)
- [ADR-0053](0053-native-file-picker-remote-attach-channel.md) — 원격 attach 파일 선택 채널
- [ADR-0091](0091-render-stall-watchdog-observation-only.md) — stall 워치독은 관측 전용
- [identity](../identity.md) 2.1 · 2.2 · 2.3 · 2.4 — 불가침 원칙
- [debug-ipc](../dev-guide/debug-ipc.md) — 원칙 1 의 release/debug 판단 기준
- [features/native-file-picker](../features/native-file-picker/index.md) — 현재 파일 선택 표면
