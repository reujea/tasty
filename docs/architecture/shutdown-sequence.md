# 종료 시퀀스 — 종료 cascade + Drop tail

사용자 종료(Cmd/Ctrl+Q, quit 모달, 창 닫기)는 부팅과 대칭으로 **상태 머신
(`ShutdownPhase`)이 프레임 단위로 전개**한다. 대기가 남아 있는 프레임마다 부팅과
같은 로딩 화면(워드마크 + 회전 스피너 + 단계 문구)을 present 하므로, 종료 대기
동안 창이 얼어붙지 않는다 — 근거는 [ADR-0077](../adr/0077-shutdown-loading-screen.md).

종료 비용은 두 구간으로 나뉜다. **`event_loop.exit()` 까지**(종료 cascade)와
**그 이후**(Drop tail)다. 후자는 창이 이미 사라진 뒤에 도는 destructor 구간이라
**어떤 종료 화면으로도 덮을 수 없다** — 종료 UX 를 논할 때 이 경계가 기준선이다.

## 시퀀스

```
진입 (3경로 — 모두 App::begin_shutdown 으로 수렴)
  ├─ AppEvent::Shutdown            (src/app/event_handler.rs)
  ├─ quit 모달 "종료" → 재진입      (src/app/modal/quit.rs — 모달이 이미 열린 상태)
  └─ close_behavior == "quit"       (src/app/modal/quit.rs)

begin_shutdown (src/app/shutdown_machine.rs)          [t0 확정]
  ShutdownPhase 상태 머신 설치 → 모든 MainView 의 native webview 숨김
  → about_to_wait 워치독이 16ms 케이던스로 구동
  (단계 본문은 src/app/shutdown_cascade.rs, 순서·대기는 상태 머신이 소유)

  SavingLayout
  └─ S1  flush_layout_persistence(true)
         main + parked engine 각각 SaveLayoutNow{force} → 자기 슬롯 파일
         (surface.closed 발화 전에 끝나야 한다 — layout 은 *살아있는* 상태를 기록)
  ReclaimingBootWorker                    (부팅 중 종료 전용 — 아니면 건너뛴다)
  └─ S2  try_recv 폴링 + deadline 5s → 회수한 PluginManager 를 장착
  ClosingSurfaces
  ├─ ―   system.shutdown_initiated 발화 (plugin cleanup hook 기회)
  ├─ S3  cascade_shutdown_close_all_surfaces + dispatch_pending_surface_lifecycle
  │      전 workspace→pane→tab→surface 순회 close 큐 push 후 plugin 으로 broadcast
  ├─ S3b observer_router.join_retired()  (창별 engine + parked engine)
  │      surface close 가 뒤로 미뤄둔 output observer sink 워커 회수
  └─ ―   begin_plugin_shutdown() — 전 plugin 에 shutdown 요청을 뿌리기만 한다
  StoppingPlugins
  └─ S4  poll_shutdown_all() 폴링 — 대기가 겹친다
         └─ S4a plugin 별 2s graceful deadline → 초과 시 force kill
  Done
  ├─ shutdown_total
  └─ event_loop.exit()

  ※ 대기가 있는 프레임마다 render_loading 으로 종료 화면 present (아래 절)

run_app 반환 (src/boot.rs) — 여기부터 Drop tail. 창은 이미 없다.
  drop_app_with_trace(app)
    ├─ S5d TcpIpcServer::drop        accept 스레드 stop + **port 파일 제거**
    ├─ S5a LuaEngine::drop           Shutdown send + 워커 join (블로킹)
    ├─ S5b PtyBackend::drop 합계     자식 셸 kill (surface 수만큼 반복)
    ├─ S5c SshTunnel::drop 합계      child.kill + wait (attach 세션 수만큼, 블로킹)
    ├─ S5  drop_tail                 run_app 반환 → App drop 완료 전체
    └─ shutdown_total_with_drop      **사용자 체감에 대응하는 값**
```

- 진입 3경로가 `begin_shutdown` 하나로 수렴하는 것은 계측 요구이자 순서 요구다.
  t0 이 경로마다 어긋나면 `shutdown_total` 의 의미가 흔들리고, S1(layout 저장)이
  S3(close cascade) 앞에 온다는 제약도 호출자마다 재현해야 한다.
- **중복 진입은 무해하다.** 이미 종료 중이면 `begin_shutdown` 이 즉시 return 하므로,
  종료 화면이 뜬 상태에서 Cmd/Ctrl+Q 를 다시 눌러도 phase 가 되감기지 않는다.
- **스텝은 전부 논블로킹이어야 한다.** `shutdown_cascade.rs` 의 각 함수는 프레임
  안에서 끝나거나(1ms 미만) begin/poll 로 갈라져 있다. 블로킹 호출을 스텝 안에
  넣으면 그 구간만 다시 얼어붙는다 — 컴파일러가 잡아 주지 않는다.
- `PluginProcess::drop` 도 Drop tail 에서 블로킹(`child.wait()`)하지만, S4 의
  `shutdown_all` 이 이미 `processes` 를 drain 했다면 남은 대상이 없다.
- **정상 종료 경로에 `std::process::exit` 는 없다** — 위 Drop 들은 전부 실행된다.
  (`std::process::exit` 호출부는 초기화 실패/에러 경로 전용이다.)

## plugin 종료 대기의 겹침 (S4)

plugin 은 서로 독립 프로세스라 graceful 대기가 직렬일 이유가 없다. `shutdown_all`
은 두 단계로 나뉜다.

1. `begin_shutdown_all()` — 전 plugin 의 `req_tx` 에 shutdown 요청을 넣고 자식
   핸들만 회수한 뒤 **즉시 반환**한다. 요청을 먼저 전부 뿌리는 것이 대기 겹침의
   전제다.
2. `poll_shutdown_all()` — 논블로킹 폴링. 각 자식이 스스로 종료했는지
   `try_wait` 로 확인하고, 자기 deadline(2s)을 넘긴 자식만 force kill 한다.
   남은 대상이 없으면 `true`.

`shutdown_all()` 은 이 둘을 묶은 블로킹 형태다. 프레임을 계속 돌려야 하는
호출자는 두 함수를 직접 조합해 대기 중에도 렌더를 유지할 수 있다.

지켜야 하는 제약:

- **요청 순서 계약** — shutdown 요청은 `dispatch_pending_surface_lifecycle()` 이
  같은 `req_tx` 에 이미 넣어 둔 `surface.closed` 뒤에 놓인다. plugin 이 cleanup
  대상 surface 를 모르는 채 종료되면 안 되므로 S3 → S4 순서는 고정이다.
- **요청이 큐에 못 들어갈 수 있다** — 그 큐는 개수로도 바이트로도 유한하고(바이트는 큐마다와
  모든 plugin 채널의 합계로 — [ADR-0360](../adr/0360-plugin-channels-are-bounded-in-bytes-per-queue-and-in-total.md)),
  호스트→plugin 방향의 포화는
  대기가 아니라 **거절**이다([ADR-0315](../adr/0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md)).
  writer 스레드가 소켓에서 막혀 큐가 차 있으면 shutdown 요청이 거절되고, 그 plugin 은
  graceful 기회 없이 deadline 뒤 kill 로 회수된다. **그 사후 판별을 S4a 의 `killed` 로
  하지 마라** — `killed` 는 "요청이 거절됐다" 와 "요청은 갔는데 plugin 이 2s 안에 안
  빠졌다" 를 한 값으로 뭉갠다. 두 사건의 처방이 반대다(앞은 호스트 큐·writer 쪽, 뒤는
  plugin 쪽). 가르는 것은 거절 시점에 호스트 로그로 나가는
  `plugin '<id>' shutdown send failed: ...` 한 줄이고(사유는 `request queue full` ·
  `request queue over its byte budget` 둘 중 하나 — shutdown 은 제어 요청이라 합계 바이트
  판정을 면제받으므로 `plugin channels over their total byte budget` 로는 거절되지 않는다,
  ADR-0360 2026-09-21 보강), 그 줄이 있으면 거절이다. 그 줄은 `warn` 이라 기본 필터(stderr `warn` · 파일 dev `debug`/release `warn`)
  에 남는다.
- **타임아웃 의미론** — 겹치는 것은 대기 구간뿐이고, plugin 하나가 받는 graceful
  기회는 여전히 2s 다. S4a 는 개별 소요와 `graceful|killed` 사유를 그대로 남긴다.
- **잔존 프로세스 없음** — `poll_shutdown_all()` 이 `true` 를 반환한 시점에 모든
  자식이 회수(exit 관측 또는 kill+wait 완료)돼 있다. 폴링을 끝내지 않고 매니저가
  drop 되면 남은 자식은 그 자리에서 kill 된다.
- **단건 경로는 메인 스레드가 기다리지 않는다** — `plugin disable` 과 헬스체크
  재시작은 shutdown 요청을 보낸 뒤 회수 대기(최대 2s, 넘으면 kill)를 전용 스레드
  (`plugin-retire-<id>`)에 맡긴다. 메인 스레드는 50 ms 주기 타이머(`PluginRetire`,
  회수 중인 것이 있을 때만 등록)로 끝난 것만 거두고, 로그에 `plugin process retired`
  한 줄(`ms` · `reason`)을 남긴다. **새 프로세스는 옛 것이 회수된 뒤에 뜬다** — 재시작은
  기동을 미뤘다가 회수가 끝난 tick 에 한다(겹치면 옛 프로세스가 쥔 포트·파일을 새 것이
  못 잡는다). 회수 중에 온 `disable` 은 그 예약을 거둔다. 회수 중에 온 명시적 `enable`
  은 미루지 않고 그 id 의 회수를 기다린 뒤 그 자리에서 띄운다 — `enable` 이 돌아오면
  plugin 이 떠 있다. 그 대가로 이 조작에 한해 메인 스레드가 최대 2s 선다. 호스트 종료가
  시작되면 회수 중인 것도 `poll_shutdown_all()` 이 끝날 때까지 보고 다시 띄우지 않으며, 끝난 것마다 S4a 를 `retiring before exit` 문구로 남긴다.
  **기다리는 자리는 넷이다** — `plugin remove` · swap · `upgrade-builtins` 의 쓰기 갈래 ·
  명시적 `enable`. 헬스체크 재시작과 `disable` 은 기다리지 않는다. 앞의 셋은 옛
  프로세스가 반드시 사라져 있어야 하는 자리다 — `plugin remove`
  (디렉토리를 지운다) · swap(`upgrade-builtins --restart-running` · auto-reload,
  디렉토리를 덮어쓴다) · `upgrade-builtins` 의 **쓰기 갈래**(버전이 달라 덮어쓰거나,
  같은 버전인데 바뀐 내용이 있을 때). 설치본이 더 높아 건너뛰거나 같은 버전에 바뀐 것이
  없으면 기다리지 않는다 — 회수와 재기동 예약은 뒤에서 그대로 이어진다. 기다린 자리는
  회수 뒤의 재기동 예약을 함께 가져오므로 쓰기를 마친 뒤 다시 띄운다. 근거·대안은
  [ADR-0457](../adr/0457-a-single-plugin-shutdown-is-reaped-off-the-main-thread.md).

## 종료 화면

종료가 확정되면 첫 종료 프레임 전에 모든 MainView 가 소유한 native webview 를
숨기고 표시 여부 캐시도 비운다. 활성 창·탭뿐 아니라 비활성 탭의 인스턴스도 대상이다.
이 처리는 일반 redraw 가 끊기기 전의 마지막 프레임에 의존하지 않고
`App::begin_shutdown` 에서 수행한다. native 자식 뷰는 GPU 표면보다 위에 있으므로
로딩 프레임만 다시 그려서는 웹뷰를 가릴 수 없다.

확인 모달을 열거나 취소하는 경로와 최소화는 이 진입점을 거치지 않는다.
숨김은 포커스를 요청하거나 웹뷰를 조기 파괴하지 않으며, layout 저장·lifecycle 통지·
plugin 종료 및 backend 최종 정리 순서는 그대로 유지한다.
웹뷰가 없거나 부팅 중인 경우에는 숨길 인스턴스가 없어 추가 대기 없이 진행한다.

상태 머신이 대기하는 프레임마다 `GpuState::render_loading` 으로 로딩 화면을
present 한다. **부팅과 같은 렌더 경로·같은 락업**이고 다른 것은 phase 문구뿐이다 —
`render_loading` 은 phase 타입이 아니라 i18n 키를 받고, 두 상태 머신이 각자의
매핑을 소유한다(`boot_phase_text_key` / `ShutdownPhase::text_key`).

| phase | 문구 키 | 프레임을 넘기는가 |
|-------|---------|-------------------|
| `SavingLayout` | `shutdown.phase_saving_layout` | 아니오 (실측 <1ms) |
| `ReclaimingBootWorker` | `shutdown.phase_finishing_startup` | 예 (부팅 중 종료 전용) |
| `ClosingSurfaces` | `shutdown.phase_closing_surfaces` | 아니오 (실측 <1ms) |
| `StoppingPlugins` | `shutdown.phase_stopping_plugins` | 예 (S4 — 가장 늦게 빠지는 plugin 의 소요, 상한 2s) |

동작 규칙 넷:

- **빠른 종료는 화면을 띄우지 않는다.** `drive_shutdown_frame` 은 한 호출 안에서
  더 진행할 수 없을 때까지 스텝을 반복하므로, 대기가 없는 종료(plugin 0 개 — 실측
  0.63ms)는 첫 구동에서 완료에 도달해 **한 프레임도 그리지 않는다.** 최소 표시
  시간이나 지연 표시 타이머가 필요 없다.
- **창이 없으면 프레임을 돌리지 않는다.** macOS 최소화(창 파괴 + park)나 트레이
  hide 상태의 종료는 상태 머신을 설치하지 않고 그 자리에서 끝까지 블로킹으로
  돈다(= 이 구조 이전과 동일한 동작).
- **창이 여럿이면 전부 종료 화면으로 바꾼다.** 하나만 그리고 나머지를 먼저 닫으면
  창이 하나씩 사라지는 것으로 보여 크래시와 구분되지 않는다.
- **종료 가드** — 종료 진행 중에는 steady-state 파이프라인(IPC 처리 / intent drain /
  plugin pump)을 태우지 않고 `AppEvent` 는 폐기한다(부팅 가드는 지연 후 재생하지만
  종료에는 재생할 미래가 없다). 키/마우스도 core 에 닿지 않는다.
  가드는 `event_loop.exit()` **이후에도 유지된다** — winit 은 exit 요청 즉시 루프를
  끊지 않고 콜백을 한 번 더 돌리며(Linux/X11 실측), 그 패스에서 가드가 풀려 있으면
  이미 정리가 끝난 상태로 파이프라인이 돈다. 그래서 `finish_shutdown` 은
  `App.shutdown` 을 비우지 않고 phase 를 `Exited` 로 옮긴다.
- **IPC 요청은 무시하지 않고 거절한다** — 가드가 `process_ipc()` 를 막으므로 이
  구간의 요청은 아무도 읽지 않는다. 그냥 드롭하면 클라이언트(우리 자신의 `tasty`
  CLI 포함)는 무한정 기다린다. 그래서 매 프레임과 `exit()` 직전에 큐를 drain 해
  핸들러를 태우지 않고 `-32000 "host is shutting down"` 으로 회신한다
  ([ADR-0078](../adr/0078-shutdown-rejects-pending-ipc.md)). 창 없는 블로킹 경로도
  같은 루프를 쓰므로 함께 덮인다.

갤러리 specimen 은 Chrome 카테고리의 "Shutdown loading screen" — 부팅 specimen 과
같은 `draw_frame` 을 공유해 두 화면의 동일성을 눈으로 확인하는 자리다.

## 헤드리스와의 비대칭

헤드리스 빌드(`--no-default-features`)의 `AppEvent::Shutdown` 은 `rx.recv()` 루프를
`break` 할 뿐 `shutdown_all` 을 호출하지 않는다(`src/boot.rs` 의 `run_headless`).
plugin 정리는 `PluginProcess::drop` 의 즉시 kill 로만 이뤄지므로 graceful 2s 대기가
없고, 본 문서의 종료 cascade 계측(S1~S4)도 발화하지 않는다. 이 비대칭은 의도된
것이다 — 헤드리스는 GUI 종료 UX 가 없어 graceful 대기의 값이 다르다.

## 종료 계측 (target: `tasty::shutdown`)

부팅 계측(`target: "tasty::boot"`, [boot-sequence](boot-sequence.md))과 같은 관례를
따른다: 상시 발화, 레벨 `info!`, 소요는 `ms` 필드(f64 밀리초), 분기 사유는 `reason`
필드. debug 빌드는 `$TASTY_HOME/debug-dev.log`(debug 레벨 file layer)에 수집되고,
stderr 기본 필터가 warn 이라 콘솔 노이즈는 없다. release 검증은 `TASTY_LOG=info`.

| 마커 | 구간 | 추가 필드 |
|------|------|-----------|
| S1 layout_flush | `flush_layout_persistence(true)` | — |
| S2 boot_worker_reclaim | `ReclaimingBootWorker` phase (부팅 중 종료 전용, timeout 5s) | `reason = reclaimed\|unreclaimed` |
| S3 surface_close_cascade | close 큐 push + plugin broadcast | `surfaces` = 큐에 push 한 surface 수 |
| S3b observer_sink_join | close 경로가 미뤄둔 observer sink 워커 join | — |
| S4 plugin_shutdown | `StoppingPlugins` phase 전체 (겹친 대기의 합계 = 최댓값) | `plugins` = 종료 대상 plugin 수 |
| S4a plugin_shutdown_one | plugin 1개 종료 (graceful deadline 2s) | `plugin_id`, `reason = graceful\|killed\|no_child` |
| shutdown_total | 종료 진입 → `event_loop.exit()` 직전 | — |
| S5d ipc_server_drop | `TcpIpcServer::drop` (accept stop + port 파일 제거) | — |
| S5a lua_join | `LuaEngine::drop` (Shutdown send + 워커 join) | — |
| S5b pty_drop | `PtyBackend::drop` 합계 (자식 종료 **대기는 포함하지 않는다** — [ADR-0076](../adr/0076-close-path-per-surface-blocking-removal.md)) | `ptys` = drop 된 PTY 수 |
| S5c ssh_tunnel_drop | `SshTunnel::drop` 합계 | `tunnels` = drop 된 터널 수 |
| S5 drop_tail | `run_app` 반환 → `App` drop 완료 | — |
| shutdown_total_with_drop | 종료 진입 → Drop tail 완료 (**체감 종료 시간**) | — |

읽는 법:

- **S2 는 마커가 없는 것이 정상이다.** steady-state 종료에서는 회수할 부팅 워커가
  없어 상태 머신이 이 phase 를 건너뛴다. 마커가 보였다면 부팅 로딩 화면 상태에서
  창을 닫은 것이다. 이 경로에서 회수한 `PluginManager` 는 그 자리에서
  `shutdown_all` 되지 않고 `self.plugin_manager` 에 장착되므로, **S4/S4a 는 S2 에
  중첩되지 않고 그 뒤에 나란히** 나온다(대기 중에도 종료 화면이 돈다).
- **S4 는 plugin 이 0개여도 `plugins=0` 으로 발화한다.** "안 걸렸다" 와 "계측이 안
  붙었다" 를 로그만으로 구분할 수 있어야 하기 때문이다. plugin manager 자체가
  없으면 `S4 plugin_shutdown (no plugin manager)` 로 구분된다.
- **S4 는 개별 S4a 의 합이 아니라 최댓값에 수렴한다.** 대기가 겹치므로
  plugin 이 6개면 S4a 가 각각 ≈2000ms 여도 S4 는 ≈2000ms 다. S4 가 plugin 수에
  비례해 커졌다면 대기가 다시 직렬화된 것이다.
- **S3b 는 정상 경로에서 0 에 가깝다.** surface close 는 sink 워커를 join 하지 않고
  모아두기만 하므로([ADR-0076](../adr/0076-close-path-per-surface-blocking-removal.md)),
  여기서 한 번에 회수한다. 워커들이 그동안 병렬로 이미 배수를 끝냈기 때문에 실제
  대기는 거의 없다. observer 를 쓰지 않으면 항상 0 이다.
  **이 단계는 정확성 요건이 아니라 최적화다** — `ObserverRouter::drop` 이 같은
  `join_retired` 를 한 번 더 부르므로, S3b 가 빠져도 sink 파일이 잘리지는 않는다.
  다만 그 회수가 Drop tail(S5) 까지 밀려 종료가 그만큼 늦어진다.
- **S3 의 `surfaces` 와 S5b 의 `ptys` 는 세는 대상이 다르다.** 전자는 layout 상의
  surface, 후자는 PTY 를 실제로 가진 backend 다 — child terminal / 헤드리스 PTY 는
  layout 밖에도 있고, PTY 없는 surface(webview 등)도 있어 두 값은 일치하지 않는다.
- **S5b/S5c 는 크레이트 전역 누적기의 전후 델타**다(`tasty_terminal::pty_drop_totals`
  / `tasty_ssh::tunnel_drop_totals`). destructor 가 개수만큼 반복돼 개별 로그로는
  읽기 어렵고, 평시(surface 닫기·attach 해제)의 drop 도 같은 누적기에 쌓이므로
  **절대값이 아니라 델타로만** 의미가 있다.
- `shutdown_total` ≥ S1+S2+S3+S4 이며, 차이가 크면 계측이 덮지 않은 구간이 있다는
  뜻이다. 마찬가지로 `shutdown_total_with_drop` − `shutdown_total` = S5 다.
- **마지막 줄은 유실되지 않는다.** file layer 는 host 프로세스가 연 `Mutex<File>` 을
  writer 로 직접 쓰고 (`crates/tasty-platform/src/crash_report.rs`) BufWriter 를 끼우지 않아,
  프로세스가 곧 죽는 Drop tail 구간에서도 이벤트마다 write 가 완료된다.

## 실측 기준치

Linux(X11) / debug 빌드 / 번들 plugin 전부 활성, `TASTY_LOG=info`, `system.shutdown`
IPC 로 종료. 단위 ms.

| | plugins | S1 | S3 | S4 | shutdown_total | S5a | S5b | S5 | **with_drop** |
|---|---|---|---|---|---|---|---|---|---|
| 겹친 대기, 정지 0개 | 6 (전부 killed) | — | 0.4 | 2008 | 2009 | — | — | 115 | **2124** |
| 겹친 대기, SIGSTOP 2개 | 7 (전부 killed) | 0.7 | 0.4 | 2073 | 2074 | — | — | 262 | **2336** |
| 겹친 대기, SIGSTOP 1개 | 5 (전부 killed) | — | — | 2050 | 2051 | — | — | 286 | **2337** |
| plugin 0개 | 0 | 0.5 | 0.05 | 0.001 | 0.63 | 0.18 | 101 | 153 | **158** |

프레임 구동 상태 머신 도입 후 같은 환경 실측 (창 닫기 / Ctrl+Q 로 종료):

| | plugins | S1 | S2 | S3 | S4 | shutdown_total | S5 | **with_drop** |
|---|---|---|---|---|---|---|---|---|
| 창 1개, SIGSTOP 2개 | 8 (전부 killed) | 0.6 | — | 0.2 | 2004 | 2005 | 36 | **2043** |
| 창 2개, SIGSTOP 2개 | 7 (전부 killed) | 1.2 | — | 0.9 | 2012 | 2015 | 154 | **2179** |
| 부팅 중 종료 (GpuInit) | 0 (manager 없음) | 0.009 | — | 0.003 | 0 | 0.11 | 37 | **52** |
| 부팅 중 종료 (WaitingEngine) | 8 (전부 killed) | 0.008 | 594 | 0.005 | 2001 | 2596 | — | — |

- **S2 가 실제로 발화하고, 그 뒤에 S4 가 나란히 온다.** 부팅 로딩 화면 상태에서
  창을 닫으면 워커 회수(414~626ms)가 끝난 뒤 S4 가 시작된다. 회수 구간 동안 화면
  문구는 `Finishing startup…`, 그 뒤는 `Stopping plugins…` 로 바뀐다.
- **종료 화면이 덮는 범위는 `shutdown_total` 까지다.** Drop tail(위 표에서
  36~154ms)은 창이 사라진 뒤라 덮이지 않는다.

참고 — 대기를 겹치기 전(plugin 하나씩 순차 대기)의 같은 환경 실측:

| | plugins | S1 | S3 | S4 | shutdown_total | S5a | S5b | S5 | **with_drop** |
|---|---|---|---|---|---|---|---|---|---|
| run 1 | 8 (전부 killed) | 1.8 | 0.07 | 16130 | 16132 | 0.11 | 55 | 518 | **16664** |
| run 2 | 6 (전부 killed) | 0.6 | 0.06 | 12094 | 12095 | 0.19 | 122 | 335 | **12444** |
| run 3 | 8 (전부 killed) | 0.6 | 0.04 | 16052 | 16053 | 0.14 | 100 | 149 | **16208** |

SDK 가 shutdown 요청에 스스로 빠지게 된 뒤의 같은 경로 실측 (2026-09-23, Linux Xvfb /
debug GUI 빌드 / 격리 `TASTY_HOME` / 번들 plugin 9 개 전부 running 확인 후 `system.shutdown`
IPC, 정지시킨 plugin 없음, 부하 평균 36~56):

| | plugins | S1 | S3 | S4 | shutdown_total | S5 | **with_drop** |
|---|---|---|---|---|---|---|---|
| run 1 | 9 (전부 graceful) | 0.89 | 0.06 | 3.44 | 4.88 | 60 | **66** |
| run 2 | 9 (전부 graceful) | 0.83 | 0.05 | 3.78 | 5.00 | 64 | **69** |
| run 3 | 9 (전부 graceful) | 0.85 | 0.05 | 3.70 | 4.92 | 64 | **70** |

S4a 는 세 회 모두 plugin 마다 1.6~3.8 ms 였다.

이 값들이 말하는 것:

- **위쪽 표들의 "전부 killed" 와 S4 ≈2s 는 SDK 결함의 값이다.** plugin 로그에는 `plugin
  received shutdown` 이 찍혀 요청 자체는 도달했지만, SDK `run()` 이 worker 스레드 join 에서
  서서 프로세스가 안 끝났다 — worker 는 큐가 닫히기를 기다렸는데 그 큐의 sender 를
  `HostHandle` 이 쥐고 있었다. 그래서 그 시절 S4 는 2s deadline 에 붙어 plugin 수와 무관하게
  2.0~2.1s 로 평탄했고, 일부 plugin 을 `SIGSTOP` 으로 정지시켜도 값이 안 움직였다.
  순차 대기 시절의 같은 plugin 수(6개=12.1s)와 비교하면 6배였다.
- **지금 정상 종료의 S4 는 수 ms 다.** 9 개 전부 graceful 로 빠져 S4 3.4~3.8 ms,
  `shutdown_total` 5 ms 안쪽이다. 2s deadline 은 응답하지 않는 plugin 이 있을 때만 S4 의
  상한으로 드러난다.
- **체감 종료는 이제 Drop tail 이 대부분이다.** with_drop 66~70 ms 중 S5 가 60~64 ms 다.
  헤드리스 경로의 같은 수정 실측(번들 9 개를 `plugin enable` 로 띄우고 하나씩
  `plugin disable`)은 9/9 `reason="graceful"`, 개별 55~160 ms 였다(`plugin process retired` 줄).
- **Drop tail 은 위쪽 표들에서 36~520ms, 지금 실측에서 60~64ms 다.** 위쪽 표들의 내역은
  S5b(PTY) 55~122ms, S5a(Lua join) 0.2ms 미만이고, 나머지 수십~수백 ms 는
  GPU/egui/View 등 그 밖의 destructor 다. **이 구간은 종료 화면으로 덮을 수 없다** —
  창이 이미 사라진 뒤이므로, 종료 화면을 도입해도 체감 시간의 하한으로 남는다. plugin
  종료가 graceful 로 빠진 지금은 S4 보다 이 구간이 길다.
- **`~/.tasty/tasty.port` 는 Drop tail 초반(S5d, `event_loop.exit()` 후 ~14ms)에
  사라진다.** 제거는 `TcpIpcServer::drop` 에서만 일어나지만 Drop tail 의 앞쪽이라,
  Drop tail 이 길어도 stale 포트 파일이 남는 창은 짧다.

실측 시 유의:

- plugin 수가 run 마다 다른 것은 이 환경에서 일부 plugin 이 spawn 되지 않기
  때문이다. 순차 대기 시절에는 plugin 수가 S4 를 그대로 좌우했으므로 비교 시
  `plugins` 필드를 함께 봐야 했다. 겹친 대기에서는 S4 가 plugin 수에 둔감해지는
  것 자체가 판정 기준이다 — 비례해서 늘어난다면 병렬화가 깨진 것이다.
- 정지시킨 plugin 개수(0/1/2)를 바꿔도 S4 가 평탄한지가 회귀 확인의 핵심이다.
  재현은 `pgrep -f tasty-plugin- | head -N | xargs -r kill -STOP` 후 종료.
- 위 값은 Linux/debug 기준이다. 부팅 계측의 기준치(Windows/7950X3D)와는 환경이
  달라 직접 비교하지 않는다.
- **S2(부팅 중 종료)는 X11 에서 `wmctrl -i -c <win>` 로 재현된다.** 창이 뜨자마자
  닫으면 `GpuInit` 단계라 워커가 아직 없어 S2 가 발화하지 않고, ~0.1s 뒤에 닫으면
  `WaitingEngine` 단계에 걸려 S2 가 발화한다. quit 모달 경로는 키 입력이 필요한데,
  `debug.settings.apply` 로 `keybindings.quit` 을 라이브 지정한 뒤 `xdotool key`
  로 실제 키 이벤트를 보내면 된다(합성 이벤트가 아니라 XTEST 경로여야 한다).
- **종료 중 IPC 는 즉시 오류로 끝나야 한다.** 종료 대기 중에 아무 메서드나 던져
  `{"error":{"code":-32000,"message":"host is shutting down"}}` 가 곧바로 오는지
  본다. 응답이 없거나 클라이언트가 매달리면 회귀다.
- **화면이 실제로 도는지는 창 캡처로 판정한다.** `xwd -id <win>` 로 0.25~0.3s 간격
  3장 이상을 찍어 스피너 각도가 서로 다른지 본다 — 각도가 같으면 프레임이 안 도는
  것이다(정지 프레임). tasty 자체 `ui.screenshot` IPC 는 종료 중에 처리되지 않으므로
  (종료 가드) 이 판정에는 쓸 수 없다.

## 관련

- [boot-sequence](boot-sequence.md) — 대칭 구조인 부팅 상태 머신 + 부팅 계측(T1~T7)
- [`docs/dev-guide/error-handling.md`](../dev-guide/error-handling.md) — 로그 레벨 선택 기준
- [`docs/dev-guide/self-verification.md`](../dev-guide/self-verification.md) — debug 인스턴스로 시나리오 재현
- [ADR-0077](../adr/0077-shutdown-loading-screen.md) — 종료를 프레임 구동으로 전개하고 로딩 화면을 씌운 결정
