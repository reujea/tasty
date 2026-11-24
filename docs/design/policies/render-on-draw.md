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

IPC·CLI는 대상의 실제 상태를 보고한다. 숨은 터미널이라면 `read screen`은 지금 grid의 내용을, `surface.list`의 `cols`·`rows`는 지금 grid의 크기를 돌려준다(`src/adapters/ipc/handler/surface/list.rs`). `tree`의 터미널 노드는 크기를 채우지 않는다(`cols`·`rows`가 0). 기록만 해 둔 대기 값은 실제 값과 섞지 않고 `surface.list`의 별도 필드 `pending_cols`·`pending_rows`로 낸다. 기록이 없거나 attach client가 크기를 정하는 hard 점유 surface면 두 키가 없다. 보이게 되면 기록이 적용되고 키가 사라진다. surface 스크린샷(`ui.screenshot`에 `surface_id`를 준 경로, `GpuState::capture_surface_to_png`)은 터미널의 지금 grid 크기로 찍으므로 숨은 터미널은 옛 grid 크기로 찍힌다.

## 적용 사례: 숨은 터미널의 창 크기

창 크기·배율·글꼴·분할 비율·사이드바 너비가 바뀌어 터미널 목표 크기가 달라졌을 때 숨은 터미널은 다음 규칙을 따른다.

- 숨은 터미널은 새 목표 크기를 기록하고 dirty 표시를 켠다. grid reflow와 OS PTY resize는 하지 않는다.
- 화면에 보이게 되는 순간 grid reflow와 PTY 크기 통지를 한 번에 적용한다.
- 숨은 동안 `read screen`·`surface.list`·surface 스크린샷은 옛 크기의 실제 grid를 보고한다(위 절).
- attach 임시 탭이 pane을 덮는 동안 그 아래 실제 탭도 숨은 터미널이다. 목표는 기록되고 임시 탭이 사라져 보이게 될 때 적용된다.
- attach client의 숨은 mirror도 같다. 목표 크기를 기록만 하고 `StreamControl::ClientResize`를 보내지 않는다. 보이게 될 때 한 번 보낸다. 보이는 동안 보낸 요청의 응답 마감이 숨은 동안 지나면 다시 보내지도 실패로 올리지도 않는다. 보이는 mirror라도 마감이 지난 요청이 지금 레이아웃 목표와 다르면 다시 보내지 않는다([Attach 메커니즘](../../dev-guide/attach-behavior.md#리사이즈-전파-mirror-geometry)).

구현은 레이아웃 리사이즈 스윕 하나에 있다. `MainViewState::resize_all`(`src/state/layout.rs`)은 모든 workspace의 초기화된 탭을 돌며 target마다 그 창이 지금 그리는지(`drawn`)를 싣는다. 그리는 판정은 `MainViewState::draws_tab` 하나이고 창이 그리는 영역(`surface_regions`)과 가시성 판정(`draws_surface`, `shows_any_surface`)도 이 함수를 쓴다. attach 임시 탭이 가린 탭도 영역(`tab_layout_regions`)을 받아 숨은 target이 된다. 최소화·트레이로 숨긴 창(`frames_stopped`)이면 모든 target이 숨는다. 적용은 `TerminalStore::apply_layout_size`(`src/runtime/terminal_store.rs`)가 한다. 숨은 target은 항목의 목표 크기 기록만 바꾸고, 보이는 target은 기록을 지우고 grid reflow·resize tap·OS 크기 예약(mirror는 서버 요청)을 한 번 한다. 숨은 동안 여러 번 바뀌어도 마지막 목표 하나만 남는다. 동작은 `src/runtime/terminal_store/layout_tests.rs`와 `src/state/layout_sweep_tests.rs`가 확인한다.

보이게 되는 순간은 그 터미널을 처음 그리는 프레임이다. 일반 프레임은 그리기 전에 스윕하므로 그 프레임에 적용된다. 사이드바·탭 바 클릭처럼 그리기 단계(egui) 안에서 workspace나 탭이 바뀌면 그 프레임의 스윕은 이미 끝나 새로 보인 터미널이 옛 크기로 한 장 그려진다. 그리기가 마지막 스윕이 처리하지 않은 터미널을 그렸거나(`MainViewState::draws_unswept_terminal`) 그 프레임에 터미널 글꼴이 바뀌었으면 다음 redraw를 요청해 바로 다음 프레임에 맞춘다. 프레임 단계 순서 가드(`src/source_guards/frame_phase_order.rs`)는 호출 순서 문자열만 보므로 egui 안의 전환은 잡지 못한다. 이 경우는 다음 redraw 요청이 막는다.

## 적용이 일으킨 출력

dirty를 적용하는 시점(보이게 될 때의 grid reflow와 PTY 크기 통지)은 Tasty가 정한다. 그 적용을 받은 앱이 화면을 다시 그리며 내는 출력은 [실행 중 표시](busy-indicator.md)를 새로 켜지 않고, 출력 침묵을 세는 타이머도 다시 시작시키지 않는다. 키 입력 에코를 다루는 규칙과 같은 모양이다.

- 적용 전에 이미 busy였던 터미널은 그대로 busy다. 적용이 busy를 끄거나 줄이지 않는다.
- 적용 전에 idle이었던 터미널은 이 출력 때문에 busy가 되지 않는다.
- 예외로, 터미널 상태를 읽지 못해(락 경합) busy로 추정한 조회는 busy 유지 기록(latch)을 남기지 않는다. 그 터미널은 다음 조회에서 idle로 돌아갈 수 있다([실행 중 표시](busy-indicator.md)).
- 규칙은 네 소비자에 적용한다.
  - busy 판정. 사이드바의 실행 중 점과 개수, 탭의 점, 자식 상태의 `pty_busy` 근거가 읽는다.
  - 자식 Claude·Codex 상태의 출력 침묵 판정(`derive_child_state`, `src/core/state/child_liveness.rs`). busy가 아니어도 최근 출력이 있으면 `active`(근거 `recent_output`)를 내는 판정이다.
  - [idle-timeout 훅](../../features/hooks/index.md)의 출력 침묵 타이머. 이 출력은 훅이 세는 '출력 없음'을 깨지 않는다.
  - 플러그인 출력 스캐너. `surface.read_since_scan_mark` 응답의 `text_origins`가 재그리기 범위를 알린다([터미널 출력 구조화](../../features/terminal-output/index.md#출력-스캐너-전용-커서)). 호스트는 분류를 줄 뿐이고, 정지 알림에서 재그리기를 빼는 것은 이 필드를 읽는 claude plugin과 함께일 때의 plugin 몫이다. 정지 알림에 효과가 나려면 실행 서버에 이 필드를 싣는 Tasty와 이 필드를 읽는 plugin이 둘 다 있어야 한다.
- 출력 침묵을 세는 두 경로(자식 상태, idle-timeout 훅)는 적용 전부터 세던 침묵을 이어서 센다. 침묵 시간은 적용 전 마지막 출력부터 계속 늘어나며, 이 출력 때문에 0으로 돌아가지 않는다.

이 규칙은 dirty 적용만이 아니라 Tasty가 OS PTY 크기를 바꾸는 모든 경우에 같다. 보이는 터미널의 창 크기 변경, 보이게 될 때의 적용, Windows 절전 복귀의 깨우기 통지가 여기에 든다. OS 크기 통지가 그 터미널에 재그리기 구간을 열고, 구간 안의 출력은 네 소비자에서 재그리기로 본다. 스캐너가 받는 분류는 청크가 들어올 때 침묵 판정과 같은 경계로 정해 둔 값이라 새 구간이 앞 구간을 이어받지 않고 새 시작 경계를 열 때 같은 시각의 청크는 투입 순서로 갈린다. 구간을 열기 전에 들어온 청크는 시각이 구간 시작과 같아도 일반 출력이고, 연 뒤에 들어온 청크는 같은 시각이어도 재그리기 쪽이다. 앞 구간이 살아 있는 동안 들어온 청크는 이미 그 구간 안이라 재그리기이고, 같은 시각의 다음 열기는 앞 구간의 시작 경계를 이어받으므로 그 분류는 바뀌지 않는다. 크기를 바꾸지 않는 재적용과 실패한 적용은 구간을 열지 않는다. Windows 절전 복귀의 깨우기 통지는 같은 크기여도 앱이 다시 그리므로 구간을 연다. 구간 길이는 700ms(`RESIZE_REDRAW_WINDOW`)다. 사용자 결정(2026-11-24)이며, Codex가 크기 변경을 약 500ms 모았다가 다시 그리는 것을 측정한 값에 PTY throttle과 여유를 더했다.

시간으로 가르는 판정이라 다음 한계가 있다(사용자 결정으로 받아들인다).

- 구간 안에서 시작해 끝난 진짜 출력과 입력 에코도 재그리기로 분류된다. busy를 켜지 않고 두 침묵 경로의 침묵도 깨지 않는다.
- 통지가 이어지는 동안(창 테두리를 끄는 동안)은 구간이 계속 이어지므로, 그동안의 출력은 idle-timeout 훅의 침묵도 깨지 않는다. 출력이 이어지는 중에도 드래그가 훅 시간보다 길면 훅이 발화할 수 있다.

구현은 `crates/tasty-terminal/src/resize_redraw.rs`에 있다. 로컬 PTY의 OS 적용과 구간 개폐는 `flush_os_resize`, Windows 깨우기 통지는 `TerminalStore::nudge_os_size`가 한다. busy 판정은 입력 에코 자리에서 `last_activity_is_resize_redraw`를 함께 보고, 자식 상태 관측(`src/runtime/child_observation.rs`)과 idle-timeout 훅(`src/hook_runtime/engine.rs`)은 `Terminal::silence_since`를 읽는다. attach client의 mirror는 서버의 `Resize` 응답으로 grid가 바뀔 때 같은 구간을 연다(busy는 서버 판정을 그대로 쓴다). mirror는 출력과 응답을 reader가 받은 시각으로 판정하므로 적용이 밀려도 분류가 바뀌지 않는다(`Terminal::feed_bytes_at`, `Terminal::note_resize_redraw_at`). 재연결하면 옛 구간은 새 연결이 시작한 시각에 닫혀, 설치 전에 받은 새 연결의 출력도 옛 구간에 들지 않는다(`Terminal::bind_external_input`). 스캐너용 분류는 투입 때 `TerminalState::chunk_origin`이 정해 출력 보존 버퍼 옆(`crates/tasty-terminal/src/output_origins.rs`)에 남기고, OS 적용이 진행 중일 때 들어온 청크는 적용을 확정하면 재그리기, 되돌리면 일반 출력으로 정한다. 다만 열기 전부터 있던 구간이 이미 덮는 진행 중 청크는 바로 재그리기로 정하고 적용이 실패해도 그대로 둔다. 이어지는 통지가 앞 구간을 이어받는 조건을 포함한 판정 조건은 `resize_redraw_tests.rs`, 스캐너용 분류는 `scan_origins_tests.rs`, 재연결 닫기는 `src/app/attach_client/resources.rs`의 시험, 두 침묵 경로의 호출부는 `child_observation.rs`·`hook_runtime/engine.rs`의 시험이 정본이다.

mirror는 서버의 PTY throttle 경계를 알지 못한다. 서버가 한 throttle 간격 안에 크기를 바꿨다가 되돌려 OS 크기가 그대로면 서버는 구간을 열지 않지만, mirror는 받은 두 응답으로 구간을 연다. 그래서 그 직후 출력을 mirror의 자식 상태·idle-timeout은 재그리기로 볼 수 있다. 재연결 닫기가 이미 들어온 출력의 판정을 바꿀 수 있고 서버의 첫 화면도 live 출력과 같은 경로로 오므로, mirror의 스캔 읽기는 출처 정보를 싣지 않는다.

## 예외

지금 값이 필요한 계약은 그릴 때까지 미루지 않는다.

- **attach 서버의 크기 요청 적용**: 서버는 holder client의 `StreamControl::ClientResize`를 받으면 서버 화면에 그 surface가 보이는지와 관계없이 바로 grid를 그 크기로 바꾸거나 같은 크기임을 확인하고 응답한다(`EngineMut::apply_attached_workspace_resize`, `src/remote/server/attach_resize.rs` → `TerminalStore::resize_for_attach`, `src/runtime/terminal_store.rs`). OS PTY 크기 통지는 이때 `Pty::schedule_resize`로 예약만 하고, 실제 통지는 `crates/tasty-terminal/src/pty.rs`의 flush와 throttle을 거친다. 응답은 grid 확정을 뜻하며 OS 적용 확인이 아니다. 그 surface를 그리는 쪽은 client이고, 요청은 client가 그리는 크기에서 나온다. client는 `ipc.stream.resize-ack` 계약에 따라 `Resize`·`ResizeRejected` 응답을 기다리고, 마감이 지나면 다시 보낸 뒤 실패 배너를 띄운다(`src/app/attach_client/resize_sync.rs`). 서버가 적용을 미루면 이 응답이 늦어 거짓 실패가 난다. 협상 규칙은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#크기-요청의-응답과-재시도)에 있다.
- **첫 배치**: PTY를 만든 뒤 처음 받는 레이아웃 목표(숨은 workspace에 띄운 새 child, 부팅 복원, 닫기 되돌리기로 살린 터미널)는 숨어 있어도 한 번 적용한다. 그러지 않으면 보기 전까지 창 전체 터미널 영역 크기로 실행된다. 그 뒤의 변경부터 위 규칙을 따른다.
- **서버가 정한 mirror 크기**: attach client의 mirror grid는 서버의 `Resize` 응답과 재연결 descriptor로 바뀐다. 서버가 정한 값이라 숨은 mirror에서도 바로 바뀐다. 바뀐 크기가 기록한 목표와 같아지면 기록을 지운다.
- **서버 GUI의 detach 뒤 원복**: 서버 창에서 숨은 workspace를 client가 점유했다가 detach하면 그 터미널은 client 크기로 남고, 서버 창에서 보이게 될 때 서버 레이아웃 크기로 한 번 돌아간다.
- **journal halt 프레임**: halt 중에는 엔진을 읽기만 하므로 스윕하지 않고 동결된 grid를 그린다. halt는 같은 인스턴스에서 풀리지 않으므로 그 grid는 인스턴스가 끝날 때까지 유지된다.
- **headless**: 화면이 없어 그릴 때가 오지 않는다. 레이아웃 리사이즈 스윕 `MainViewState::resize_all`은 `gui` 기능에서만 컴파일되므로 headless에는 미룰 창 크기 계산이 없다. headless 터미널의 크기는 만들 때의 기본 크기(`EngineRuntime`의 `default_cols`·`default_rows`)이거나 위 attach 크기 요청으로 정해진 값이다.

에이전트가 surface나 터미널 크기를 직접 지정하는 IPC는 없다. surface 없이 셸을 만드는 `pty.spawn`은 엔진 기본 크기로 시작한다(`src/adapters/ipc/handler/pty.rs`).

## 관련

- [포커스 정책](focus.md)
- [터미널 (Terminal)](../../surfaces/terminal/index.md)
- [Attach 메커니즘](../../dev-guide/attach-behavior.md)
- [전체화면 무대](../systems/fullscreen-stage.md): 무대 중에는 PTY grid를 진입 시점 값으로 고정한다
