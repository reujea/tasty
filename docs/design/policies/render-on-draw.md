# 그릴 때 적용 (Render on Draw)

렌더링에 쓰이는 값이 바뀌면 화면에 그려지지 않는 대상은 새 값과 dirty 표시만 기록한다. 그 값에 따른 재계산은 대상을 실제로 그릴 때 한 번 한다. 보이지 않는 대상의 재계산은 사용자가 볼 결과를 만들지 않으면서 CPU와 자식 프로세스 신호만 쓴다. 보이게 될 때까지 값이 여러 번 바뀌어도 마지막 값으로 한 번만 계산하면 된다. 사용자 포커스와 대상 선택은 [포커스 정책](focus.md)을 따른다.

## 적용 대상 값

화면 표현을 정하는 값과 그 값에서 파생되는 계산이 대상이다.

| 바뀌는 값 | 그릴 때로 미루는 계산 |
|---|---|
| 창·pane·surface 크기 | terminal grid reflow, OS PTY 크기 통지(Unix SIGWINCH, Windows ConPTY resize), 레이아웃 재계산 |
| 배율(scale factor) | 위 크기 계산, 글리프 래스터 |
| 글꼴·글꼴 크기 | 셀 크기 재계산과 그에 따른 grid reflow·PTY 크기 통지, 글리프 래스터 |
| 테마 | 색 재계산과 다시 그릴 자료 생성 |

값 자체의 기록(설정 저장, 목표 크기 보관)은 미루지 않는다. 미루는 것은 기록한 값을 대상에 반영하는 계산이다.

## 그리지 않는 대상

다음 대상은 화면에 그려지지 않는다.

- 활성 탭이 아닌 탭의 surface
- 활성 workspace가 아닌 workspace의 surface
- 위 조건에 해당하는 attach client의 mirror surface
- 최소화·트레이로 숨긴 창의 surface

## 그릴 때의 정의

대상이 화면에 보이게 되는 순간을 "그릴 때"로 본다. 탭 전환, workspace 전환, 창 복원처럼 대상이 보이게 되는 첫 프레임에 기록해 둔 마지막 값을 한 번에 적용한다. dirty 표시가 없으면 아무것도 하지 않는다.

에이전트의 입력·읽기 요청은 그릴 때가 아니다. 숨은 대상에 `send`나 `read`가 와도 대기 중인 값을 강제로 적용하지 않는다.

## 숨은 동안 보고하는 값

IPC·CLI는 대상의 실제 상태를 보고한다. 숨은 터미널이라면 `read screen`, `tree`, `surface.list`가 지금 grid의 크기와 내용을 돌려준다. `surface.list`의 `cols`·`rows`는 터미널의 현재 grid 값이다(`src/adapters/ipc/handler/surface/list.rs`). 기록만 해 둔 대기 값을 실제 값처럼 섞어 보고하지 않는다. 대기 값을 보여 줘야 하는 소비자가 생기면 실제 값과 구분되는 별도 필드로 낸다. 현재 대기 값을 보고하는 필드는 없다.

## 적용 사례: 숨은 터미널의 창 크기

창 크기가 바뀌었을 때 숨은 터미널은 다음 규칙을 따른다. 이 규칙은 아직 구현되지 않았다.

- 숨은 터미널은 새 목표 크기를 기록하고 dirty 표시를 켠다. grid reflow와 OS PTY resize는 하지 않는다.
- 화면에 보이게 되는 순간 grid reflow와 PTY 크기 통지를 한 번에 적용한다.
- 숨은 동안 `read screen`·`tree`·`surface.list`는 옛 크기의 실제 grid를 보고한다.
- attach client의 숨은 mirror도 같다. 목표 크기를 기록만 하고 `StreamControl::ClientResize`를 보내지 않는다. 보이게 될 때 한 번 보낸다.

현재 구현은 이 규칙과 다르다. GUI의 레이아웃 리사이즈 스윕 `MainViewState::resize_all`(`src/state/layout.rs`)은 모든 workspace의 초기화된 모든 탭을 돌며 surface마다 목표 grid를 모아 `GeometryEffect::Resize`로 넘긴다. 그래서 숨은 터미널도 창 크기가 바뀌면 바로 reflow되고 PTY 크기 통지가 예약된다. 숨은 mirror도 목표 grid를 forward 큐에 넣어 원격에 크기를 요청한다([Attach 메커니즘](../../dev-guide/attach-behavior.md#리사이즈-전파-mirror-geometry)).

## 적용이 일으킨 출력과 실행 중 표시

dirty를 적용하는 시점(보이게 될 때의 grid reflow와 PTY 크기 통지)은 Tasty가 정한다. 그 적용을 받은 앱이 화면을 다시 그리며 내는 출력은 [실행 중 표시](busy-indicator.md)를 새로 켜지 않는다. 키 입력 에코를 다루는 규칙과 같은 모양이다.

- 적용 전에 이미 busy였던 터미널은 그대로 busy다. 적용이 busy를 끄거나 줄이지 않는다.
- 적용 전에 idle이었던 터미널은 이 출력 때문에 busy가 되지 않는다.
- 실행 중 신호는 두 경로로 나온다. 하나는 busy 판정(`last_activity_at` 기반)으로, 사이드바의 실행 중 점과 개수, 탭의 점, 자식 상태의 `pty_busy` 근거가 읽는다. 다른 하나는 자식 Claude·Codex 상태의 출력 침묵 판정(`derive_child_state`, `src/core/state/child_liveness.rs`)이다. 이 판정은 busy가 아니어도 모든 출력에 갱신되는 `last_output_at`의 침묵 시간이 `CHILD_OUTPUT_SILENCE`보다 짧으면 `active`(근거 `recent_output`)를 낸다. 규칙은 두 경로 모두에 적용해야 한다.
- idle-timeout 훅은 이 규칙 밖이다. 훅은 실행 중 신호가 아니라 출력 침묵을 보므로 적용이 일으킨 출력도 출력으로 센다.

이 규칙은 아직 구현되지 않았다. 현재 busy 판정(`Terminal`의 busy 조회, `crates/tasty-terminal/src/accessors.rs`)은 resize 시각을 기록하지 않으므로, 창 크기 변경 뒤 앱이 다시 그리는 출력은 일반 활동 출력으로 세어 idle이던 터미널을 busy로 바꾼다. 같은 출력이 `last_output_at`도 갱신하므로 자식 상태는 `recent_output` 근거의 `active`가 된다. 화면에 보이는 터미널의 resize도 같다.

## 예외

지금 값이 필요한 계약은 그릴 때까지 미루지 않는다.

- **attach 서버의 크기 요청 적용**: 서버는 holder client의 `StreamControl::ClientResize`를 받으면 서버 화면에 그 surface가 보이는지와 관계없이 바로 grid를 그 크기로 바꾸거나 같은 크기임을 확인하고 응답한다(`EngineMut::apply_attached_workspace_resize`, `src/remote/server/attach_resize.rs` → `TerminalStore::resize_for_attach`, `src/runtime/terminal_store.rs`). OS PTY 크기 통지는 이때 `Pty::schedule_resize`로 예약만 하고, 실제 통지는 `crates/tasty-terminal/src/pty.rs`의 flush와 throttle을 거친다. 응답은 grid 확정을 뜻하며 OS 적용 확인이 아니다. 그 surface를 그리는 쪽은 client이고, 요청은 client가 그리는 크기에서 나온다. client는 `ipc.stream.resize-ack` 계약에 따라 `Resize`·`ResizeRejected` 응답을 기다리고, 마감이 지나면 다시 보낸 뒤 실패 배너를 띄운다(`src/app/attach_client/resize_sync.rs`). 서버가 적용을 미루면 이 응답이 늦어 거짓 실패가 난다. 협상 규칙은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#크기-요청의-응답과-재시도)에 있다.
- **headless**: 화면이 없어 그릴 때가 오지 않는다. 레이아웃 리사이즈 스윕 `MainViewState::resize_all`은 `gui` 기능에서만 컴파일되므로 headless에는 미룰 창 크기 계산이 없다. headless 터미널의 크기는 만들 때의 기본 크기(`EngineRuntime`의 `default_cols`·`default_rows`)이거나 위 attach 크기 요청으로 정해진 값이다.

에이전트가 surface나 터미널 크기를 직접 지정하는 IPC는 없다. surface 없이 셸을 만드는 `pty.spawn`은 엔진 기본 크기로 시작한다(`src/adapters/ipc/handler/pty.rs`).

## 관련

- [포커스 정책](focus.md)
- [터미널 (Terminal)](../../surfaces/terminal/index.md)
- [Attach 메커니즘](../../dev-guide/attach-behavior.md)
- [전체화면 무대](../systems/fullscreen-stage.md): 무대 중에는 PTY grid를 진입 시점 값으로 고정한다
