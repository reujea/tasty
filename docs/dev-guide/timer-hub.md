# 중앙 타이머 허브 (TimerHub)

메인 루프의 **시간축** 주기 작업은 전부 `tasty-timer` 의 `TimerHub` 에 키로 등록된다.
전용 ticker 스레드를 새로 만들거나 매 프레임 `Instant` elapsed 게이트를 두는 방식은
쓰지 않는다.

- 크레이트: [`crates/tasty-timer`](../../crates/tasty-timer/src/lib.rs)
- 호스트측 키/등록: [`src/app/timers.rs`](../../src/app/timers.rs)
- gui 실행부: `src/app/event_handler.rs` `about_to_wait`
- headless 실행부: `src/boot.rs` `run_headless` 의 루프 — 대기는 `wait_for_event`(`recv_timeout`), 실행은 `run_due_timers`

## 두 축을 섞지 않는다

메인 루프에는 성격이 다른 두 축이 있다.

| 축 | 트리거 | 예 | 허브 대상 |
|---|---|---|---|
| 프레임축 | "이벤트가 있었으니 큐를 비운다" | `dispatch_pending_*`, `process_ipc` | ✘ |
| 시간축 | "N ms 마다 / N ms 뒤에 한 번" | busy 재평가, attach 뷰 갱신, 메뉴 트래킹 | ✔ |

프레임축에는 주기 개념이 없다. 허브는 시간축만 다룬다.

## API

```rust
hub.every(key, interval, precision, now);      // 반복
hub.once_after(key, delay, precision, now);    // 일회성(발화 후 자동 제거)
hub.cancel(key);
hub.is_registered(key);

let due: Vec<K> = hub.drain_due(now);          // due 한 키만. 실행은 호출자 몫
let at: Option<Instant> = hub.next_deadline();  // None = 깨울 이유 없음
hub.snapshot();                                 // 관측용
```

**허브는 콜백을 담지 않는다.** `Box<dyn FnMut(&mut App)>` 를 App 필드에 두면 drain 중
`&mut self` 를 다시 빌릴 수 없어 take-run-restore 우회가 강요되고 재진입에 취약해진다.
키만 반환하므로 실행부는 평범한 `match` 로 남는다.

**모든 API 가 `now` 를 인자로 받는다.** 내부에서 `Instant::now()` 를 부르지 않으므로
단위 테스트가 가짜 기준시각으로 완전히 결정론적이다. 호출부는 프레임 앞머리에서 읽은
`now` 를 재사용하되, 프레임 말미에 데드라인을 새로 잡을 때는 시각을 다시 읽는다
(긴 프레임 뒤에 이미 지난 데드라인을 등록하면 다음 프레임이 즉시 깨어나 spin 한다).

### 반복 타이머의 드리프트

다음 발화 시각은 `now` 가 아니라 **직전 데드라인**에 `interval` 배수를 더해 정한다.
1초 타이머를 2.5초 시점에 drain 하면 다음은 3.0초다(2.0 도 3.5 도 아니다) — 위상이
유지되므로 프레임 지연이 누적되지 않는다. 절전 복귀처럼 `u32` 배수로 표현할 수 없을
만큼 밀리면 `now` 기준으로 재정렬한다(밀린 만큼을 몰아서 발화시키지 않는다 — 허브는
발화를 큐잉하지 않는다).

## Strict vs Lax

| | `next_deadline()` 기여 | 쓰는 곳 |
|---|---|---|
| `Strict` | `next_due` — 그 시각에 반드시 깨운다 | 사용자가 지연을 체감하는 작업 |
| `Lax { slack }` | `next_due + slack` — slack 전까지 기여하지 않는다 | 늦어도 되는 정리/유지보수 |

`Lax` 는 due 해도 그 자체로 wakeup 을 만들지 않고, **다른 이유로 깨어난 프레임에
편승**해 실행된다(coalescing). `deadline + slack` 을 넘기면 hard deadline 으로 승격돼
반드시 깨운다 — 완전 idle 상태에서 영원히 안 도는 starvation 을 막는다.

판단 기준: *못 돌면 사용자가 즉시 알아채는가*. busy indicator(1s)·attach mirror(3s)·
메뉴 트래킹(8ms)은 `Strict`. "언젠가 치우면 되는" 캐시 정리류는 `Lax`.

### Lax 사용 사례 — TTL 정리를 lazy 에서 건져내기

TTL 기반 정리 3건이 `Lax` 를 쓴다. 셋 다 원래 **"누가 건드릴 때 같이 치우는"(접근
시점 lazy) 방식으로만** 돌았고, 그래서 접근이 멈추면 정리도 멈췄다 — 그 순간이
정확히 정리가 가장 필요한 순간이다.

| 키 | 대상 | 주기 / slack | lazy 경로가 멈추는 조건 |
|---|---|---|---|
| `PtySweep` | headless PTY 좀비(TTL 5분) | 30s / 60s | 에이전트가 `pty.spawn`/`pty.list` 를 다시 부르지 않음 |
| `CaptureSweep` | 캡처 업로드 partial(TTL 5분) | 30s / 60s | 다음 청크가 안 옴(연결은 살아 있고 업로드만 중단) |
| `LogPrune` | IPC 관측 로그 3종(집행 게이트 1시간) | 600s / 600s | 세 로그 모두 append 가 희소 + 재시작 없이 장수명 |

`Lax` 인 이유는 하나다 — 회수가 몇십 초 늦는 것은 무해하지만, **그것 때문에 idle
인스턴스를 깨우는 것은 낭비다.** 사용자가 뭐라도 하는 프레임에 공짜로 실행되고,
완전 idle 이면 slack 경계에서 한 번만 깨운다. 회수 지연 상한은
`TTL + interval + slack` 이다(PTY 기준 최대 6.5분). TTL 값 자체는 바꾸지 않았다 —
바뀐 것은 **언제 회수하는가** 뿐이다.

**주기 타이머가 lazy 를 대체하지 않는다.** 특히 `pty` 쪽 lazy 는 `pty.spawn`
**직전에** 도는 덕분에 동시 개수 상한 판정을 정확하게 만든다(죽은 항목을 먼저 치우고
상한을 본다). 대체하면 "실제로는 idle 인 PTY 때문에 spawn 이 상한 초과로 실패" 하는
회귀가 된다 — 주기 타이머는 최대 90초 뒤에나 도는데 spawn 은 지금 성공해야 한다.
두 경로는 보완 관계다(`docs/adr/0050-headless-pty-primitive.md` "좀비 회수 시점").

두 경로가 공존하면 **후처리가 갈라지는 것**이 다음 위험이다. headless PTY 회수는
registry 제거 + `TerminalStore` 제거 + waker 게이트 해제 셋을 한 묶음으로 해야 하는데
(어느 하나만 하면 좀비나 누수), 경로마다 따로 쓰면 언젠가 어긋난다. 그래서 후처리를
`CoreState::sweep_idle_ptys` 한 곳으로 묶고 두 경로가 그것만 부르게 했다. 로그 prune
도 같은 이유로 진입점(`log_retention::maybe_prune`)과 게이트(`LAST_PRUNE_MS`)를
공유한다 — 새 게이트를 만들면 두 드라이버가 각자 주기를 세게 된다.

### `every` 는 바닥치기(`arm_derived`) 대상이 아니다

아래 "파생 데드라인은 반드시 바닥친다" 규칙은 **`once_at` 계열**(외부 상태에서
파생한 절대 시각을 매 프레임 재등록하는 형태)에만 적용된다. 위 정리 tick 들처럼
`every(주기)` 로 등록하는 키는 그 실패 클래스에 해당하지 않는다:

- 등록이 **부팅 1회**뿐이라 매 프레임 재등록되지 않는다 — 스핀은 "과거 값을 계속 다시
  넣는" 데서 오는데 그 동작 자체가 없다.
- 재발화 시각은 허브가 직전 데드라인에 주기를 더해 **스스로 전진**시킨다(위 "반복
  타이머의 드리프트"). 외부 상태에 의존하지 않으므로 과거에 고정될 수 없다.
- 0 주기는 `TimerHub` 의 `normalize()` 가 1ns 로 올려 막는다.

그래서 `crates/tasty-doc-guards/tests/timer_deadline_hygiene.rs` 도 `hub.once_at` 만 금지하고 `hub.every` 는
건드리지 않는다.

## 대기 전략 — 왜 waker 스레드인가 (대안 B 채택)

두 안이 있었다.

- **대안 A (순수 `WaitUntil`)** — ticker 스레드를 전부 없애고 `ControlFlow::WaitUntil`
  로만 깨운다.
- **대안 B (단일 waker 스레드)** — 고정 주기 ticker 3개를 **데드라인 기반 waker 1개**로
  합치고 `EventLoopProxy::send_event` 로 깨운다. `WaitUntil` 은 보조로만 쓴다.

**대안 B 를 채택했다.** 근거: tasty 에는 창이 없거나 사실상 없는 상태가 실재한다 —
macOS 최소화는 window 를 파괴하고 `CoreState` 만 `parked_states` 에 남기며
(`src/app/window_lifecycle.rs`), tray 상주는 모든 창을 닫는다
([system-tray](../design/policies/system-tray.md)). 그 상태에서도 busy forward(원격
attach client 로의 `StreamControl::Activity`)와 글로벌 훅은 계속 돌아야 한다 —
`poll_busy_states` 가 `parked_states` 를 순회하는 이유가 정확히 그것이다.

그런데 **창 없이도 이벤트 루프가 계속 깨어난다는 보장은 플랫폼마다 다르다.**
`src/app/shutdown_machine.rs` 의 창 없는 종료 경로가 같은 이유로 이미 구동을 이벤트
루프에 맡기지 않는다. 대안 A 는 그 보장을 플랫폼 구현에 넘기므로, 세 OS 에서 최소화·
tray 상주 상태의 1Hz busy forward 를 실증하기 전에는 채택할 수 없다.

대안 B 는 절전 이득을 그대로 얻으면서 그 위험이 없다:

- 스레드 수 3 → 1 (`busy_tick` 1s + `attach_tick` 3s + headless `spawn_busy_ticker` 1s
  → `tasty-timer-waker` 1개)
- 깨우는 시점이 **고정 주기가 아니라 허브가 정한 데드라인**이다. 등록이 없으면 무기한
  park(idle wakeup 0), 등록되면 `Condvar` 로 즉시 재무장한다.
- 정확성은 waker 스레드가 책임지고, `WaitUntil` 은 창이 있을 때 wake 지연을 줄이는
  보조다. 둘 다 같은 데드라인(`sync_timer_control_flow`)을 받는다.

**headless 는 waker 스레드조차 없다.** 메인 루프가 `rx.recv_timeout(deadline - now)` 로
직접 데드라인을 지키므로 wake 신호가 필요 없다(`AppEvent::TimerTick` 은 gui 전용).

### waker 상태 락의 poison 복구

waker가 보호하는 값은 다음 데드라인과 정지 플래그다. 임계구역은 이 값만 교체하고
사용자 콜백은 락 밖에서 실행하므로, poison 이후에도 상태를 복구해 다음 데드라인
발화와 정지를 처리한다. 이것은 죽은 waker 스레드를 재시작한다는 보장은 아니다.

최초 `Mutex::lock` 과 `Condvar::wait`·`wait_timeout` 재획득은 모두
`tasty_utils::poison` 헬퍼를 거친다. **waker 인스턴스마다 최초 한 번** error 로그에
`timer waker state lock poisoned` 를 남기며, 세 경로가 같은 보고 플래그를 공유한다.
대기 시작 후 다른 스레드가 poison을 만들 수도 있어 재획득의 보고를 생략하지 않는다.
로그 규약은 [error-handling](error-handling.md)의 락 poison 절을 따른다.

`crates/tasty-timer/src/waker_poison_tests.rs` 는 독립된 상태 락을 실제로 poison시키고
최초 획득·무기한 대기·시간제한 대기 각각의 error 로그 한 번, 데드라인 발화,
정지 처리를 검증한다. 사용자 환경이나 GUI 인스턴스를 사용하지 않는다.

## 실행부

### gui — `about_to_wait`

```rust
for key in self.timers.drain_due(Instant::now()) {
    match key {
        Tick::Busy => { poll_busy_states(); poll_global_hooks(); poll_idle_timeout_hooks(); }
        Tick::AttachView => poll_attach_views(),
        Tick::NativeMenu => {}   // 깨어나는 것 자체가 목적
    }
}
// ... 파이프라인 ...
self.sync_timer_control_flow(event_loop);   // waker + ControlFlow 를 한 번에
```

`set_control_flow` 는 **말미 1회**만 호출한다. 기본값을 앞에서 깔고 뒤에서 덮어쓰면
합성 순서에 따라 데드라인이 유실된다.

### headless — `run_headless`

```rust
// wait_for_event: 데드라인까지만 블로킹. 세 결과를 `Wait` 로 이름 붙인다.
let pending = match wait_for_event(&rx, app.timers.next_deadline()) {
    Wait::Event(ev) => Some(ev),
    Wait::Deadline => None,          // 타이머만 돌린다
    Wait::Disconnected => break,
};
run_due_timers(&mut app, &mut state, &mut engine);  // drain_due + headless 실행부 + plugin 허브
let Some(event) = pending else { continue };
```

`run_due_timers` 는 앱 허브를 비운 뒤 plugin 허브의 데드라인이 지났으면 그쪽도 pump 한다
(`pump_plugins_if_due`). 대기가 두 허브의 `min` 이라 plugin 데드라인에 깨어난 바퀴가 그것을 안
거두면 데드라인이 과거에 남고 다음 대기가 0 이 된다 — 루프가 다음 `Tick::Busy` 까지 헛돈다(실측:
`PluginTick::Ping` 15 s 마다 약 1 s 동안 160 만 회). `src/source_guards/headless_loop_reaps_both_hubs.rs`
가 그 호출이 시간축에 있는지 문다.

`Wait` 가 필요한 이유는 `Option<AppEvent>` 하나로는 "타이머만 돌린다"(계속)와
"송신단이 사라졌다"(종료)가 구분되지 않기 때문이다 — 대기를 함수로 빼려면 그 셋이
값이어야 한다.

gui/headless 는 **같은 키 집합을 같은 주기로** 굴린다. `Tick::Busy` 가 headless 에서
하는 일은 넷이다 — busy 재평가 + attach forward / 글로벌 훅 / idle-timeout 훅(바인딩
실행 + `HookFired` enqueue 포함) / plugin pump 안전망. 하나라도 빠지면 headless 원격
attach 가 조용히 멈춘다. `Tick::AttachView` 는 렌더가 없는 headless 에선 등록하지
않는다(키 자체가 `#[cfg(feature = "gui")]`).

## 계층을 넘는 허브 합성

본체 타입을 모르는 크레이트(`crates/tasty-host-plugin` 등)는 본체 허브에 직접 등록할 수
없다. 대신 **자기 허브를 소유하고 `next_deadline()` 만 노출**한다.

```rust
// crates/tasty-host-plugin
pub struct PluginManager { timers: TimerHub<PluginTick>, … }
impl PluginManager {
    pub fn pump(&mut self, now: Instant) -> Vec<(String, String)> {
        …
        for key in self.timers.drain_due(now) { /* 실행 */ }
    }
    pub fn next_deadline(&self) -> Option<Instant> { self.timers.next_deadline() }
}
```

호스트는 프레임 말미에 `min_deadline(self.timers.next_deadline(), mgr.next_deadline())`
으로 접는다(`src/app/timers.rs`). **허브가 여러 개여도 대기 계산은 하나다** — 이것이
계층을 넘을 때의 표준 패턴이다. gui(`sync_timer_control_flow`)와
headless(`recv_timeout` 앞) 양쪽이 같은 합성을 한다.

`pump` 이 `now` 를 인자로 받는 이유도 같다 — 매니저가 내부에서 `Instant::now()` 를
부르지 않으므로 단위 테스트가 시간을 주입해 "15초 뒤" 를 sleep 없이 재현한다.

### plugin 주기 작업과 healthcheck 검출 상한

| 키 | 주기 | Precision | 비고 |
|---|---|---|---|
| `PluginTick::Ping` | 15s | Strict | ping 송신 + **비응답 재시작 판정** |
| `PluginTick::Rss` | 30s | Lax(slack 15s) | 관측용 — 스스로 wakeup 을 만들지 않고 ping 에 편승 |
| `PluginTick::AutoReload` | 2s | Strict | **flag on 일 때만 등록** — 꺼진 기능은 데드라인에 기여하지 않는다 |

`HEALTHCHECK_TIMEOUT`(60s)은 인터벌이 아니라 "마지막 pong 이후 경과" **데드라인 비교**라
아무 tick 에서나 판정할 수 있다. 별도 tick 을 만들지 않고 `Ping` tick 에 합승시켰다 —
ping 을 보내는 tick 이 곧 응답을 기대하는 tick 이라 판정 시점으로 자연스럽고, 검사 전용
wakeup 이 늘지 않는다. 그 결과 **비응답 검출 상한은 `60s + 15s = 75s`** 다.

프로세스가 실제로 죽는 경우는 이 경로가 아니다 — plugin 이벤트 채널이 Disconnected 가
되는 즉시 잡히므로(`collect_plugin_events`) 크래시 검출은 상한의 영향을 받지 않는다.
이 상한이 적용되는 것은 "프로세스는 살아 있는데 ping 에 응답하지 않는" 행 상태뿐이다.

## 디바운스는 `every` 가 아니라 `once_at` 이다

레이아웃 저장(`Tick::LayoutFlush`)은 "변경이 있고 나서 한 번" 이지 "주기적으로" 가
아니다. `every` 로 옮기면 변경이 없어도 500ms 마다 깨어나는 회귀가 된다.

호스트는 매 프레임 살아있는 engine 들의 **처음 dirty 가 된 시각**(`dirty_since`)
중 가장 이른 값을 모아 `once_at(Tick::LayoutFlush, since + 500ms, Lax{500ms})` 로
동기화하고, 저장할 변경이 없으면 `cancel` 한다(`sync_layout_flush_timer`).
`dirty_since` 는 뒤이은 변경으로 리셋되지 않으므로 같은 절대 시각으로 매 프레임
재등록해도 위상이 밀리지 않는다 — `once_after`(상대 지연) 대신 `once_at`(절대 시각)을
쓰는 이유다. 연속 변경 중에도 **첫 변경으로부터 debounce 안에 반드시 한 번** 저장된다.

`Lax` 인 이유: 저장은 사용자가 즉시 체감하는 작업이 아니라 자기 힘으로 호스트를 깨울
이유가 없다. slack 을 넘기면 hard deadline 이 되어 변경이 영영 디스크에 못 닿는 일은
없다. 종료·창 은퇴 경로는 이 타이머와 무관하게 `force=true` 로 즉시 저장한다.

## 파라미터화된 키는 수명을 반드시 동기화한다

`Tick::DagGraph(surface_id)` 처럼 **키가 대상마다 하나씩 생기는** 타이머는 등록보다
해제가 어렵다. egui `request_repaint_after` 는 뷰가 그려질 때만 예약이 갱신돼 뷰가
닫히면 자동 소멸했지만, 허브 등록은 저절로 사라지지 않는다 — 닫힌 뷰 하나가 500ms
마다 영원히 호스트를 깨우는 누수가 된다.

그래서 매 프레임 **"지금 실제로 보이는 대상" 집합으로 전체를 맞춘다**(선언적 동기화):

```rust
hub.cancel_if(|key| matches!(key, Tick::DagGraph(sid) if !active.contains(sid)));
for (sid, at) in active { hub.once_at(Tick::DagGraph(*sid), *at, Strict); }
```

**정리를 보장하는 것은 `drop_view` 가 아니라, 매 프레임 `poll()` 이 `visible` 을
통째로 교체한다는 사실이다.** `drop_view` 는 surface 가 **닫히는** 경로에만 걸린다 —
DAG 탭을 닫지 않고 배경 탭으로 밀어두는 흔한 조작은 `drop_view` 를 부르지 않는다.
그 경로에서 예약이 걷히는 유일한 이유는 그 프레임의 요청 목록에 그 surface 가 없어
`visible` 에서 빠지기 때문이다.

따라서 호출부는 **요청 목록이 비어 있어도 반드시 `poll()` 을 호출해야 한다.** 빈
목록은 "이 창에 보이는 DAG 뷰가 없다" 는 정보이고, 그걸 건너뛰면 `visible` 이 옛
surface id 를 그대로 들고 있게 된다(실제로 `if !requests.is_empty()` 가드 때문에
배경 탭 전환 시 코어 하나가 100% 로 스핀하는 회귀가 있었다). `drop_view` 도 여전히
`visible` 에서 빼지만, 그건 닫힘 경로의 보조 수단이지 정리의 주체가 아니다.

DAG 목록 popup 은 surface 에 매이지 않으므로 `Tick::DagListPopup` 로 키가 따로다
(같은 `DagGraphView` 를 쓰지만 수명 주체가 다르다). popup 이 닫히면 `None` 을
넘겨 취소한다.

### 파생 데드라인은 반드시 바닥친다 — 누수보다 스핀이 비싸다

파생 데드라인은 전부 "외부 상태의 어떤 시각 + 주기" 꼴이다 — 마지막으로 읽은 시각,
처음 dirty 가 된 시각, 다음 재시도 시각. 그 외부 상태가 어떤 이유로든 갱신을 멈추면
값이 **영원히 과거**에 머물고, 그걸 그대로 등록하면 `next_deadline()` 이 과거가 되어
`WaitUntil(과거)` = 즉시 wake 가 무한 반복된다. 결과는 코어 하나 100% 스핀이다.

등록을 잊는 누수(= 쓸데없이 한 번 더 깨어남)와 stale 데드라인(= 아예 쉬지 못함)은
실패 비용의 차원이 다르다. 그래서 바닥치기는 개별 키의 선택이 아니라 **모듈 규칙**
이다: `src/app/timers.rs` 의 절대시각 등록은 전부 `arm_derived` 한 곳을 통과하고,
그 안에서 `not_before_next_period(at, now, period)` 가 이미 지난 값을 `now + 주기` 로
올린다. 상류가 무슨 실수를 하든 최악이 **주기당 1회 wakeup** 으로 묶이고, 정상
(미래) 데드라인은 손대지 않으므로 위상은 그대로다.

적용 범위 — 절대시각으로 등록하는 키 **전부**다:

| 키 | 파생식 | 바닥값 |
|---|---|---|
| `LayoutFlush` | `dirty_since + 디바운스` | `LAYOUT_FLUSH_DEBOUNCE` |
| `DagGraph(sid)` | `last_poll + 폴링주기` | `POLL_INTERVAL` |
| `DagListPopup` | `last_list_poll + 폴링주기` | `POLL_INTERVAL` |
| `Reconnect(anchor)` | `slot.next_attempt` | `RECONNECT_MIN_BACKOFF` |

`NativeMenu` · `WebviewKeyPoll` 은 예외인데, 그건 파생이 아니라 `once_after(주기)` = **상대 지연**이라
정의상 과거가 될 수 없기 때문이다. 새 키가 절대시각을 쓴다면 예외가 아니다.

이 규칙은 `crates/tasty-doc-guards/tests/timer_deadline_hygiene.rs` 가 소스 수준에서 강제한다 — `timers.rs`
에서 `hub.once_at` 을 직접 부르면 그 테스트가 fail 한다(채널은 [ci-gates](ci-gates.md)). 실패 지점이 단위 테스트가 닿지
않는 **호출부 한 줄**이라 같은 클래스가 두 번 재발했기 때문이다.

### 바닥치기는 2차 방어다 — 스케줄 대상 자체를 좁혀라

바닥치기는 스핀을 막을 뿐 **누수는 남긴다.** 해소되지 않을 상태로 데드라인을 만들면
주기당 1회씩 영원히 깨어난다. 그래서 애초에 "그 데드라인이 해소될 수 있는가" 를
등록 전에 판정한다.

레이아웃 flush 가 그 사례다. `restore_layout=false` 이거나 슬롯이 없는 engine 은
`apply_save_layout_now` 가 저장을 건너뛰면서 `layout_dirty` 를 **clear 하지 않는다**
— dirty 가 영원히 남는다. 그래서 `App::earliest_layout_dirty_since` 는 dirty 여부만
보지 않고 `schedulable_dirty_since(restore_layout, has_slot, dirty_since)` 로
"실제로 저장될 dirty" 만 골라 넘긴다(판정식은 `apply_save_layout_now` 의 저장 조건과
같은 것이어야 한다 — 어긋나면 그 차이가 그대로 누수/스핀이 된다).

dirty 자체는 지우지 않는다. 사용자가 세션 중에 `restore_layout` 을 다시 켜면 그때까지
쌓인 변경이 그대로 flush 되어야 하기 때문이다.

> **참고**: 이 두 경로는 원래 egui `request_repaint_after(500ms)` 로 자기 wakeup 을
> 예약했지만, 호스트는 idle frame loop 를 막으려고 `delay > 0` 인 repaint 요청을
> repaint 콜백 단계에서 **drop** 한다(`src/gfx/gpu.rs`). 즉 그 예약은 실제로 루프를
> 깨우지 못했고, 다른 이유로 프레임이 돌 때만 갱신됐다. 허브로 옮기면서 의도대로
> 500ms cadence 가 실제로 동작한다.

## 타이머 취소 ≠ 상태 삭제

원격 attach 재연결(`Tick::Reconnect(anchor)`)이 그 구분이 실제로 문제가 되는 사례다.
`ReconnectSlot` 에는 give-up 플래그가 있고, **슬롯을 지우면** `reconnect_due(None, _)`
가 "아직 한 번도 실패하지 않음" 으로 오해해 즉시 재시도가 재개되고 시도 횟수가 0부터
다시 쌓여 무한히 give-up→재개를 반복하는 회귀가 있었다.

그래서 give-up 한 anchor 는 **타이머만 걷고 슬롯은 남긴다.** 동기화 함수
(`sync_reconnect_timers`)는 허브만 만지고 슬롯 맵에는 접근하지 않는다 — 취소가
"스케줄 삭제" 로 새는 경로 자체를 없앤다.

트리거가 둘(시각 `due` / 워크스페이스 재활성화 `edge`)인 것도 그대로다. **타이머는
`due` 쪽만 담당**한다 — edge 는 시각과 무관해 예약할 것이 없고, 두 트리거의 합류
판정은 기존대로 프레임에서 한다. 타이머는 "그 시각에 프레임이 돌게 하는" 역할이라
실행부가 no-op 이다(`Tick::NativeMenu` 와 같은 형태).

## 가드 중 타이머는 정지한다 (계약)

`about_to_wait` 의 shutdown / boot 가드는 조기 return 한다. **그 동안 `drain_due` 에
닿지 않으므로 steady-state 타이머는 통째로 멈춘다.** 의도된 계약이다 — 종료·부팅 중에
주기 작업이 정리 중이거나 아직 구성되지 않은 상태를 건드리지 않는다. 두 가드는 각자
자기 `ControlFlow`(프레임 워치독)를 직접 설정하고 빠져나가므로 말미의
`sync_timer_control_flow` 와 합성되지 않는다.

가드가 풀리면 밀린 타이머는 **한 번만** 발화하고 위상이 재정렬된다(허브는 발화를
큐잉하지 않는다).

## gui 는 IPC 부하 중에도 `about_to_wait` 에 닿아야 한다 (계약)

gui 의 타이머는 `about_to_wait` 앞머리에서만 돈다. winit 은 한 iteration 안에서 사용자 이벤트를
**큐가 빌 때까지** 처리한 뒤에야 `about_to_wait` 을 부르므로, 사용자 이벤트 처리가 새 사용자 이벤트를
계속 부르면 타이머가 통째로 멈춘다. IPC 가 그 형태였다 — 회차가 답을 주면 호출자가 곧바로 다음
명령과 wake 를 보낸다. 그래서 `IpcReady` 처리기는 직전 회차가 끝난 뒤 한 회차 예산이 지났을 때만
회차를 연다(`src/app/ipc.rs` 의 `IpcPacer`,
[ADR-0413](../adr/0413-in-gui-an-ipc-wake-yields-to-the-rest-of-the-loop-and-a-cut-round-wakes-it-again.md)).
예산에서 잘린 회차가 보내는 재깨움은 그 규칙을 건너뛰지만 `about_to_wait` 한 번 사이에
한 번뿐이다 — 한도를 풀면 잘림과 재깨움이 사슬을 이뤄 같은 기아가 돌아온다(실측으로 확인됨).
사용자 이벤트 처리기에 새로 무거운 일을 넣을 때도 같은 물음을 한다 — 그 일이 끝나면서 같은
이벤트를 다시 부르는가.

headless 는 이벤트 하나마다 `run_due_timers` 를 부르므로 타이머에는 이 형태가 없다. 대신 같은 이벤트
채널을 쓰는 다른 wake 가 굶을 수 있어서, IPC waker 는 채널에 `IpcReady` 를 하나만 두고 예산에서 잘린
회차가 루프를 다시 깨운다([ADR-0465](../adr/0465-headless-keeps-one-ipc-wake-in-its-channel-and-a-cut-round-wakes-it-again.md)).

## 새 주기 작업을 추가할 때

1. `src/app/timers.rs` 의 `Tick` 에 키를 추가한다. gui 전용이면
   `#[cfg(feature = "gui")]` 로 게이트한다(워크스페이스 `dead_code = "deny"`).
2. 주기 상수를 같은 파일에 두고 `register_steady_state` 에 등록한다. 조건부로만
   도는 작업이면 등록 대신 발생 시점에 `once_after` / 해제 시점에 `cancel` 한다
   (`reschedule_pending_menu_poll` 이 그 형태다).
3. gui/headless 실행부 `match` 에 arm 을 추가한다. 한쪽만 도는 키면 다른 쪽 `match`
   에서 `cfg` 로 사라지므로 컴파일러가 누락을 잡아준다.
4. `Strict` / `Lax` 를 위 기준으로 고르고, 이유를 키의 doc-comment 에 남긴다.
5. 절대 시각(외부 상태에서 파생한 데드라인)을 등록한다면 `arm_derived` 를 통과시킨다
   — `hub.once_at` 직접 호출은 `crates/tasty-doc-guards/tests/timer_deadline_hygiene.rs` 가 막는다. 고정
   주기(`every`)면 해당 없다(위 "`every` 는 바닥치기 대상이 아니다").
6. 기존 lazy 경로를 **보완**하는 tick 이면 그 lazy 경로를 지우지 말고, 두 경로가 같은
   함수를 부르게 한다(위 "Lax 사용 사례").
7. 실행 중 인스턴스에 `tasty list timers` 를 걸어 새 키가 예상 주기 / precision 으로
   보이는지, 조건부 키면 해제 후 사라지는지 확인한다(아래 "관측").

## 관측 — `timer.list` / `tasty list timers`

허브 상태는 메모리 안에만 있어서, 관측 표면이 없으면 "이 인스턴스가 idle 인데 왜
계속 깨어나는가" 를 로그를 심어 재빌드하는 방법으로만 답할 수 있다. 그 질문 셋
(파생 데드라인 스핀 · 파라미터화된 키 누수 · flag off 후 잔존)이 전부 회귀 검증
항목이라, 조회를 IPC + CLI 양면으로 노출한다.

```
$ tasty list timers
key           interval  next_due  precision       last_fired
Busy          1s        +400ms    strict          600ms ago
AttachView    3s        +2.1s     strict          900ms ago
DagGraph(41)  500ms     +200ms    strict          300ms ago
PluginPing    15s       +7s       strict          8s ago       [plugin hub]
─ hard deadline: +200ms (DagGraph(41))
```

**마지막 줄이 요점이다** — 지금 무엇이 이 인스턴스를 깨우고 있는지에 직접 답한다.
`next_deadline()` 이 min 을 취하는 것과 같은 정의(Strict = `next_due`,
Lax = `next_due + slack`)를 쓰므로, 여기 지목된 항목이 곧 실제 wakeup 원인이다.
등록된 타이머가 없으면 `none` 이고, 그건 "무기한 자도 된다" 를 뜻한다.

읽는 법:

| 관측 | 의미 |
|---|---|
| `next_due` 가 음수 | 데드라인이 이미 지났다. 매 프레임 재등록되는 파생 데드라인이면 스핀이다(위 "파생 데드라인은 반드시 바닥친다") |
| 닫은 뷰의 `DagGraph(<sid>)` 가 남아 있음 | 파라미터화된 키 누수(위 "파라미터화된 키는 수명을 반드시 동기화한다") |
| 요약 라인이 `lax` 항목을 지목 | slack 까지 넘겨 hard deadline 으로 승격됐다는 뜻 — 기아 상태이거나 slack 설정이 틀렸다 |
| 껐는데도 남아 있는 항목 | flag off 경로가 `cancel` 을 빠뜨렸다(`set_auto_reload_enabled` 가 반례) |

### 조회 전용이다

등록 / 취소 / 강제발화 API 는 만들지 않는다. 외부가 내부 스케줄을 흔들면 회수 지연
상한 같은 계약이 무너지고, 그건 에이전트가 자기 작업에 필요한 일도 아니다.
권한은 `local_only()` — plugin 이 호스트 내부 스케줄을 알아야 할 이유가 없다.

### 허브가 여러 개인 것은 `hub` 필드로만 드러난다

대기 계산이 `min_deadline` 으로 하나로 접히는 것과 같은 이유로(위 "계층을 넘는 허브
합성"), 관측도 하나의 목록으로 합친다. 본체 허브 항목은 표시가 없고, plugin manager
처럼 자기 허브를 가진 계층의 항목만 `[plugin hub]` 꼬리표가 붙는다.
`PluginTick` 은 그 크레이트 내부 어휘라 밖으로 나오지 않는다 —
`PluginManager::timer_snapshot()` 이 표시용 라벨로 옮겨 넘긴다.

### 구현 위치가 일반 IPC 핸들러가 아닌 이유

허브는 `App` 필드다. `CoreState` 만 받는 `src/adapters/ipc/handler/` 계열에서는
본체 허브에도 plugin manager 허브에도 닿지 못해 "무엇이 깨우고 있는가" 에 답할 수
없다. 그래서 조립은 `src/app/timer_report.rs` 에 두고, gui 는 `app_methods` step
(`src/app/ipc/app_methods.rs`), headless 는 dispatch pump
(`src/boot/headless_dispatch.rs`)에서 같은 함수를 부른다. headless 를 빼면 창 없는
인스턴스의 wakeup 원인을 물어볼 방법이 사라지므로 양쪽 다 배선한다.

## 허브 대상이 아닌 것

시간을 다루지만 주기 작업이 아니라 허브에 넣지 않는 코드:

| 위치 | 성격 |
|---|---|
| `src/app/window_lifecycle.rs` 부팅 deadline 대기 | 1회성 동기 대기 |
| `src/app/shutdown_machine.rs` `HEADLESS_POLL_INTERVAL` | 창 없는 종료의 블로킹 스텝 — 의도적으로 이벤트 루프에 안 맡긴 코드 |
| `src/adapters/production/tcp_ipc_server.rs` accept 재시도 | IO 재시도 |
| `src/webhook/abuse.rs` | 시각 비교(순수 함수). 실행 스케줄 없음 |
| `src/adapters/ui/` toast/banner TTL | 렌더 프레임 종속 애니메이션(프레임축) |
| `crates/tasty-terminal` `ALIVE_CHECK_INTERVAL` | syscall 레이트 리밋 |
| `src/core/agent/runner_thread.rs` `TICK_INTERVAL` | 워커 스레드 소유. 메인 루프와 실행·소유권 경계가 다르다 |
| `crates/tasty-plugin-*/` 자체 폴링 | 별도 **프로세스** — 본체 허브가 닿을 수 없다 |

## 참고

- [data-flows](../architecture/data-flows.md) — 메인 루프의 두 축
- [shutdown-sequence](../architecture/shutdown-sequence.md) — 가드 중 정지 계약의 배경
- [multi-window](../architecture/multi-window.md) — 단일 프로세스·메인 스레드 단일 루프 전제
