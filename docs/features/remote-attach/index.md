# 원격 attach (Remote attach)

- **Status**: Implemented
- **주체**: 원격 접속 사용자(점유 후 조작) · AI Agent(원격 mirror 를 정당한 행동으로 attach) · 로컬 사용자(force-detach 권한)
- **설계**: [원격 접속 프로필 + Passkey (remote_tool)](../remote-profiles/index.md) (attach 는 원격 대상 · 로컬 self-attach 는 debug 격리). 보안 위임 근거는 [에이전트 메모리 시스템](../../design/systems/memory.md) (loopback trust boundary). mirror 탭 제목 폴백의 i18n 은 [국제화 (i18n)](../../dev-guide/i18n.md)
- **코드**: `src/core/attach.rs`(`OccupancyRegistry`), `src/remote/server.rs`, `src/app/auto_attach.rs`, `src/adapters/ipc/handler/attach.rs`, `src/app/ipc/app_methods.rs`(`remote.workspaces`/`remote.attach`), `crates/tasty-remote/src/browse.rs`(브라우징 코어), CLI `crates/tasty-cli/src/commands/remote.rs`(clap 선언) · `crates/tasty-cli/src/local/{attach,remote_check,remote_workspaces}.rs`(실행)
- **화면**: [아래 절](#화면)

## 목적

다른 호스트(또는 동일 머신의 다른 인스턴스)의 surface/workspace 를 **점유(occupy)** 해 mirror 로 보고 조작하는 기능. tasty 는 자체 원격 프로토콜·암호화를 만들지 않고 **SSH 에 위임**한다 — "그 호스트에 SSH 로 들어올 수 있는 사람 = attach 자격"(tmux 모델). 원격 접속 사용자가 tasty 화면을 mirror 로 보고 조작하는 경로이며(SSH 셸·터널로 CLI·IPC 를 쓰는 것은 에이전트와 같은 조건이다), [actors](../../concepts/actors.md) 의 **점유 모델**이 실제로 구현되는 지점이다.

## 내부 동작 (headless-valid)

### 점유 (Occupation)

attach 의 본질은 **강한(hard) 배타 점유**다 — [Attach 메커니즘](../../dev-guide/attach-behavior.md) 2계층 점유 중 강한 점유 계층의 사례다(약한 점유는 [child-terminal](../child-terminal/index.md)). 개념 정의는 [actors 점유 모델](../../concepts/actors.md#점유-occupation-모델), 여기선 동작:

- **배타 lock**: 한 surface 는 한 client 만 점유한다(`OccupancyRegistry`). 점유는 `stream.open{target}` 핸드셰이크의 `attach.acquire` 로 잡고, 동시 attach 는 holder 정보를 담아 `already_attached` 로 거부.
- **점유 중 격리**: 점유된 surface 의 서버 로컬 입력(GUI 키 / `surface.send`)은 차단되고, **점유 client 입력만** PTY 에 도달한다. 로컬 사용자·AI Agent 는 그 대상에 대해 **readonly** — 내용은 보이되 조작은 막힌다. readonly 는 PTY/TUI 조작(키 입력·마우스 트래킹 보고·휠 스크롤·Ctrl+click 링크 열기)만 차단하는 것이고, **드래그로 텍스트를 선택해 클립보드로 복사하는 tasty 자체 기능은 예외적으로 계속 동작**한다 — PTY 에 아무것도 보내지 않는 순수 로컬 UI 동작이기 때문이다(좌표·복사 텍스트는 실제 렌더되는 mirror 기준). 근거: [Attach 메커니즘](../../dev-guide/attach-behavior.md).
- **점유 중 attention 해제 권한**: 강하게 점유된 surface의 attention은 점유한 클라이언트(holder)만 지울 수 있다. 서버의 로컬 포커스나 알림 패널의 읽음·모두 읽음 처리는 attention을 지우지 못한다. 클라이언트는 `ClientAttentionClear`를 보내며 서버는 holder가 맞는지 확인한 뒤 적용한다.

  서버 알림 패널의 읽음 상태는 이 제한과 별개로 바뀐다. 약한 점유(soft)에는 이 제한이 없으며, 강한 점유가 풀리면 서버 로컬 포커스가 다시 attention을 해제할 수 있어 남은 배지도 정리된다([Surface Highlight (주의 환기)](../surface-highlight/index.md)).

  텍스트 선택과 복사는 점유 중에도 허용한다. 이 동작은 로컬 화면과 클립보드에만 영향을 준다. 반면 attention은 클라이언트에도 전달되는 공유 상태이므로 holder의 확인 없이 지우지 않는다.

- **자동 해제**: client 연결 종료(EOF) 또는 attach heartbeat TTL 만료(FIN/RST 없는 silent disconnect 감지) 시 lock 이 free 로 환원. 점유는 **휘발성** — 서버 재시작 시 전부 free(영속 안 함).
- **실패하는 attach 는 점유를 잡지 않는다**: 핸드셰이크의 스트림 프로토콜 버전(`stream.open` 의 `proto`)이 서버와 다르면 attach 를 dispatch 하기 **전에** 거절 ack(`ok:false` + 사유)로 끊는다 — 성립할 수 없는 세션이 점유만 가져가 정상 attach 를 `already_attached` 로 막는 것을 방지한다. 검증 없이 잡으면, 소켓을 닫지 않는 구버전/hung peer 에서는 EOF 도 안 와 heartbeat 제한 시간까지 그 workspace 가 붙잡힌다. 근거: [Attach 메커니즘](../../dev-guide/attach-behavior.md).
- **mirror 를 다시 mirror 로 붙이지 않는다**: 대상 workspace 나 surface 가 그 인스턴스의 mirror(다른 Tasty 에서 붙여 온 원격 행)면 서버가 점유를 잡기 전에 `workspace attach rejected: mirror_workspace` 로 거절한다(`attach_activation::begin`, GUI·헤드리스 공통). 원격 workspace 목록(`remote.workspaces`, `tasty remote workspaces`)은 각 항목의 `mirror` 로 이를 알려 준다. 원격 연결 팝업은 그 행을 비활성으로 두고 `mirror` 표지와 안내 문구를 보이며, CLI 목록은 그 행 아래에 같은 안내를 한 줄 덧붙인다. 연결 실패 toast 와 자동 재연결도 이 사유를 같은 안내로 보이고, 재연결은 이 사유를 받으면 바로 멈춘다. 체인이 생기면 중간 인스턴스가 끊길 때마다 바깥 mirror 가 연쇄로 흔들리고 점유가 엇갈린다.
- **self-attach(자기 인스턴스 포트로 attach)는 거절된다**. GUI 원격 연결 팝업·IPC `attach.into_gui`·IPC `remote.attach`·저장된 원격 행의 연결 모두 debug/release 공통으로 연결 전에 거절한다. handshake 는 `queue_mirror_connection` 을 거쳐 Remote worker 가 비동기로 수행하므로 교착하지 않는다. 거절하지 않으면 attach 가 성립해 자기 mirror 가 자기 workspace 를 hard 점유하고, 그동안 원본 workspace 의 로컬 입력이 막힌다. 같은 머신의 self-mirror 를 검증할 때는 별도 프로세스인 `tasty debug attach` 를 쓴다. 이 CLI 는 GUI 의 자기 포트 검사를 거치지 않고 서버 포트에 직접 붙는다.
  - **IPC 계약 요약**: 자기 자신을 향한 `remote.attach` 의 결과는 대상 표기에 따라 다르다. loopback 표기(`127.0.0.1`·`localhost`·`[::1]` + 포트, 또는 그렇게 해석되는 프로필)는 **동기 오류**(`-32602`)로 응답하고 아무것도 시작하지 않는다. SSH 경로(호스트명·LAN IP)는 판정에 터널과 `system.info` 왕복이 필요하므로, `new_workspace:true` 만 생성 전에 같은 `-32602` 로 응답하고, 기존 워크스페이스 attach 는 응답을 늦추지 않고 즉시 `{attaching:true, attempt}` 를 준 뒤 **비동기로 거절**해 `remote.refusals` 에 `via_ssh: true` 로 기록한다. 호출자는 응답의 `attempt` 로 그 기록을 찾는다. `attach.into_gui` 는 GUI 에서 항상 동기로 답한다. 받은 `port` 가 이 인스턴스의 IPC 포트면 바로 `-32602` 이고, 다른 포트(사용자가 직접 연 SSH 터널의 로컬 포트 등)는 그 포트의 `system.info` 를 워커에서 물어 같은 `instance_id` 면 `-32602`(`... reaches this instance itself ...`), 아니면 `{queued:true}` 로 답한다. 이 확인 동안 응답이 그 왕복만큼 늦어지지만 메인 루프는 막히지 않는다(이 머신 실측 응답 약 0.1~0.2초). 접수 응답 `{queued:true}` 에는 결과를 조회할 `run` 이 붙는다([연결 결과 조회](#연결-결과-조회)). 상대가 연결을 받고 답하지 않으면 확인 상한(`PROBE_TIMEOUT`, 5초)까지 기다린 뒤 판정 없이 접수한다(실측 5.42초). 연결을 거부하거나 `instance_id` 를 보고하지 않는 상대도 판정 없이 접수한다. plugin host-call 로 부른 경우도 같다. 헤드리스에는 mirror 를 만들 쪽이 없으므로 접수하지 않는다. 자기 포트를 포함한 모든 요청을 이 빌드에 구현이 없다는 `-32017`(메서드 이름 포함)로 거절하며, plugin host-call 도 같다.
  - **처리 시점 거절(IPC)**: `attach.into_gui` 는 `port` 가, `remote.attach` 는 loopback 직결 대상(`--ssh 127.0.0.1:PORT` 형식, 또는 그렇게 해석되는 프로필)의 포트가 이 인스턴스의 IPC 포트면 요청을 처리하기 전에 `-32602` 오류(`... is this instance's own IPC port ...`)로 응답한다. 큐에 넣거나 연결 시도를 시작하지 않으며, `new_workspace:true` 여도 원격(=자기)에 워크스페이스를 만들지 않는다. 이 판정은 GUI 의 응답이다. 헤드리스는 두 메서드 모두 구현하지 않아 포트와 관계없이 `-32017` 로 거절한다. 프로필은 이 판정을 위해 요청 처리 중에 파일에서 읽는다.
  - **SSH 너머의 자기 자신**: 호스트명이나 LAN IP 로 자기 머신을 지정한 SSH 경로는 요청 포트가 터널의 로컬 포트라 포트 비교에 걸리지 않는다. 그래서 터널을 연 뒤 상대의 `system.info` 가 보고한 `instance_id` 를 이 프로세스의 값과 비교해, 같으면 같은 인스턴스로 보고 거절한다. `instance_id` 는 프로세스마다 새로 만드는 무작위 값이며(`tasty list info` 로도 보인다) 같은 머신의 다른 인스턴스(다른 홈)는 값이 달라 정상 attach 된다. 신뢰 경계는 SSH 이며 별도 권한 검사는 없다.
    - `remote.attach` 의 `new_workspace:true` 는 원격(=자기)에 워크스페이스를 만들기 전에 `-32602`(`... is this instance itself; attaching to itself is refused`)로 응답한다. 기존 워크스페이스 attach 는 응답이 해석보다 먼저 나가므로(`{attaching:true}`) 거절을 `remote.refusals` 에 `via_ssh: true` 로 남긴다. 저장된 원격 행의 연결도 같은 기록을 남긴다. GUI 원격 워크스페이스 추가 팝업은 목록을 받기 전에 판정해 안내 문구를 보여 주므로 그 엔드포인트로 연결하거나 원격에 워크스페이스를 만들지 않는다.
    - 상대가 `instance_id` 를 보고하지 않는 이전 버전이면 판정하지 않고 진행한다. 판정하는 쪽은 항상 이 기능이 있는 버전이므로 그런 상대는 자기 자신일 수 없다. 판정용 `system.info` 요청 자체가 실패하면 그 연결 시도는 실패로 끝난다.
  - **거절 조회**: 엔드포인트를 해석한 뒤에 정해지는 거절 — 저장된 원격 행의 연결 거절과 SSH 경로로 기존 워크스페이스에 보낸 `remote.attach` 의 거절 — 은 IPC `remote.refusals`(CLI `tasty remote refusals`)로 조회한다. `remote.attach` 응답의 시도 번호 `attempt` 가 항목의 `attempt` 와 같다. loopback 표기는 동기 오류로 끝나므로 보통 여기 오지 않는다(처리 시점 판정 뒤에 프로필 해석 결과가 달라졌을 때만 온다). 항목은 `attempt` · `row_workspace`(저장된 원격 행의 로컬 workspace, 수동 요청은 `null`) · `remote_workspace` · `port`(loopback 은 자기 IPC 포트, SSH 는 터널의 로컬 포트) · `via_ssh`(SSH 너머 `instance_id` 로 판정했는지) · `reconnect` 이고, 인스턴스가 끝날 때까지 최근 32건을 오래된 것부터 돌려준다. **범위는 자기 포트 거절뿐이다** — 엔드포인트 해석 실패·mirror 실패·성공은 기록하지 않으므로 목록에 없다고 연결에 성공한 것은 아니다. 요청 하나의 최종 결과는 [연결 결과 조회](#연결-결과-조회)의 `run` 으로 확인한다. GUI 팝업의 거절도 이 목록에 없다.
  - <a id="연결-결과-조회"></a>**연결 결과 조회**: `attach.into_gui` 의 `{queued:true}` 와 기존 워크스페이스 `remote.attach` 의 `{attaching:true}` 응답은 `run` 을 싣는다. `run.result {run}` 은 그 시도의 최종 결과를 준다. 공유 run 저장소는 [file-handler](../file-handler/index.md#접수-뒤-최종-결과)와 같다.
    - `succeeded`: mirror 설치 완료. `workspace_id` 에 로컬 mirror 워크스페이스 ID.
    - `failed`: 엔드포인트 해석 실패, 연결 실패, 핸드셰이크 거절(예: `already_attached`), 자기 인스턴스 거절, 연결 대기열 거절(동시 연결 대기 8건 상한 등, "the mirror connection could not be queued: <사유>"), mirror 설치 실패. `result` 에 사유 한 줄.
    - `cancelled`: 같은 대상의 새 시도로 대체("replaced by a newer attach to the same target"), 시작한 창이나 engine 이 사라짐, 행이 닫히거나 사용자가 연결을 끊음, 결과 없이 시도가 사라짐. `result` 에 사유.
    - 같은 대상: 저장된 행의 연결·재연결은 같은 engine·행, 수동 요청은 `attach.into_gui` 의 `port`·`workspace`, 기존 workspace `remote.attach` 의 `profile`·`ssh`·`remote_workspace` 가 같은 것이다. 수동 요청은 엔드포인트를 해석 중이든 연결 중이든 끝나지 않은 이전 요청을 취소하고 새 요청이 결과를 받는다. 두 메서드는 서로 대체하지 않는다.
    - 처음 기록한 결과만 남으므로 대체된 시도의 늦은 결과가 새 시도의 run 을 바꾸지 않는다. `new_workspace:true` 는 생성 결과를 지연 회신하므로 run 을 싣지 않는다. 저장된 행의 자동 연결·재연결은 run 을 만들지 않고 행의 상태 표시를 따른다.
    - GUI 원격 연결 팝업에서 **연결** 이 접수된 뒤 연결·설치에 실패하면 시작한 창에 실패 토스트를 띄운다. 그 창이 닫혔거나 View 가 바뀌었으면 띄우지 않는다([토스트 정책](../../design/systems/toast.md)).
  - **저장된 원격 행의 연결 실패 안내**: [저장된 원격 행](#저장된-원격-행과-mirror-내용)의 첫 연결이 실패하거나, 재연결이 다시 시도해도 같은 결과가 나올 이유(`remote_workspace_missing`·`occupied`·`mirror_workspace`)로 끝나면 안내를 남긴다. 재연결의 이 실패는 행을 바로 **연결 끊김** 상태로 둔다. 이유는 자기 자신(`self_instance`), 저장된 attach 프로필 없음(`profile_missing`), 연결 지점 준비 실패(`unresolved`), 원격 workspace 없음(`remote_workspace_missing`), 다른 client가 점유 중(`occupied`), 대상이 mirror(`mirror_workspace`)다. 행은 자동으로 다시 연결하지 않고 사용자가 그 행을 다시 활성화할 때 한 번 다시 시도한다. 그 행을 쓰는 IPC 요청도 한 번 다시 시도한다(아래 "저장된 원격 행을 쓰는 요청"). 원격 workspace 가 없다고 확인된 행(`remote_workspace_missing`)은 예외라 사용자의 다시 활성화도 IPC 요청도 연결을 시도하지 않는다(아래 "다른 곳에서 닫힌 행"). 같은 대상으로 연결에 성공하거나 행이 닫히면 안내가 사라진다. 재연결 중의 엔드포인트 해석 실패 가운데서는 행의 attach 프로필이 없어진 경우(`profile_missing`)만 안내를 만든다. 연결 지점 준비 실패처럼 일시적일 수 있는 실패는 안내 없이 재연결을 이어 간다.
    - 사이드바의 그 행에 경고 표지(alertTriangle, `attach-refusal-glyph` = accent-warning)가 활성 여부와 관계없이 붙고, 툴팁은 대상과 이유다. 이름 뒤 순서는 move 글리프 · 경고 표지 · 배지 묶음이며 배지가 가장 오른쪽이고 항목 사이는 `space-sm`이다. 접힌 사이드바(레일)에서는 아바타 왼쪽 위 모서리에 12 칩(alertTriangle 8, 레일 배경 바탕)을 그린다.
    - 안내는 배너를 띄우지 않는다. 행의 연결 팝업이 같은 이유를 문장으로 보인다. 토스트는 띄우지 않고 포커스도 옮기지 않는다.
    - 지금 남은 안내는 `remote.refusals` 응답의 `notices`(`workspace_id` · `kind` · `target`, 워크스페이스 ID 순)로 조회한다.
- **force-detach**: **로컬 사용자만** 점유를 강제로 끊을 수 있다(서버 권한).
  끊으면 holder client 에 종료를 통지하고 대상은 **일반 surface/workspace 로 복귀**.
  release 에는 남의 점유를 강제로 푸는 IPC·CLI 가 없다 — 에이전트가 원격 사용자를 쉽게 끊는 전용 인터페이스를 두지 않는다([사용자 통제와 데이터](../../principles/control-and-data.md)). GUI 가 없는 서버의 로컬 사용자는 OS·SSH 수단으로 원격 세션을 끊는다. debug 빌드는 사용자 조작 재현으로 `tasty debug attach --force-detach`(IPC `debug.attach.*`)를 둔다([debug IPC](../../dev-guide/debug-ipc.md)).
  GUI 진입점은 **둘**이다 — 점유된 surface 우상단의 강제 끊기 버튼(그 워크스페이스가 활성일 때만 그려진다)과, 사이드바 워크스페이스 행 우클릭의 **강제 끊기** 항목.
  뒤엣것은 점유 중일 때만 나타나고(사이드바의 점유 표시와 같은 조건 `workspace_holder(ws.id).is_some()`), 즉시 끊지 않고 화면 중앙의 확인 팝업을 한 번 거친다 — 원격 세션을 끊는 비가역 행동이고 우클릭은 오조작이 쉬운 자리이기 때문이다.
  그 팝업은 window scope 라 **대상 워크스페이스로 전환하지 않아도** 보인다.
  확인 팝업 본문은 대상 이름과 결과만 적고 점유자를 식별자로 적지 않는다 — 서버는 transport 를 모르고 숫자 client id 만 들고 있다.
  surface 단위 lock(`--surface`)은 사이드바에 표시되지 않으므로 이 경로로 풀 수 없다(그 워크스페이스로 전환해 surface 버튼을 쓴다).

### 조작 접수와 응답 대기

사용자가 이미 attach된 작업영역에서 새 탭·surface 분할·pane 분할을 요청하면 다음
프레임에 일반 탭·분할 자리를 먼저 표시한다. 아직 확정되지 않은 자리는 창의 임시
레이아웃이며 서버 구조나 저장 기록, PTY·플러그인 자원을 만들지 않는다. 스피너만 디자인의
200ms 지연 뒤 나타난다. 대상 탭과 사이드바 행·레일에도 같은 대기 상태가 표시된다.

스피너는 탭의 종류 아이콘 칸, 사이드바 카드의 점 칸, 레일 아바타의 이니셜 둘레, surface의
오른쪽 아래 받침에만 그린다. 탭 제목·워크스페이스 이름·상태 표지는 바꾸지 않는다. 전환
키캡이 보이는 동안에는 키캡이 스피너 위에 놓인다. 사이드바 카드의 받침은 사이드바 배경 위에
그 행의 현재 채움(활성·hover)을 겹친 색이다. 비활성 카드의 hover 채움도 받침에 들어가고 스피너와 키캡은
그 위에 그린다. 레일 아바타의 모서리 칩과 attached ring은 스피너
위에 그린다. 스피너 자체에는 툴팁이 없다. 로딩 중인 탭·사이드바 카드·레일 아바타에 포인터를
올리면 이름 아래 muted 둘째 줄로 `loading.remote_attach`("Attaching…")를 보인다.

대기 중인 새 영역은 입력을 받지 않는다. 키·붙여넣기·마우스·IME 입력을 이전 터미널로
보내거나 나중에 재생하지 않는다. 기존 영역은 계속 사용할 수 있다. 새 영역을 닫기·이름
변경·이동·추가 분할하는 명령은 제공하지 않으며, 확정된 같은 pane에 새 탭을 연속으로
추가할 수 있다. 실패하면 해당 임시 영역만 사라진다. 요청 후 다른 탭·pane·workspace로
이동했으면 늦은 성공이 선택을 되돌리지 않는다. 에이전트 요청은 선행 배치와 선택 이동을
만들지 않는다.

닫기·탭 이동·복원·변환, 사용자 조회, 지원되는 크기·일반 입력 확인이 지연되면 우측 상단에
작은 응답 대기 표시가 나온다. 미해결 건수와 가장 오래된 요청의 경과 시간을 보여 주며,
클릭·키보드 포커스를 가져가지 않는다. 기본 표시 지연은 디자인 토큰 200ms이고, 초는
내림한다. 구조 요청은 성공 응답만으로 끝나지 않고 해당 구조가 실제 적용될 때 끝난다.
서로 합쳐진 결과의 생성 대상을 구별할 수 없으면 확정 구조를 먼저 보여 주고 임의의 새
surface로 선택을 옮기지 않는다.

일반 입력 확인은 원격 Tasty 메인 루프가 앞선 입력을 처리 경계로 전달했음을 뜻한다.
셸의 입력 소비·명령 완료·화면 출력 확인은 아니다. 따라서 조용한 셸이나 오래 실행하는
명령 자체는 대기 표시의 근거가 아니다. 구 서버가 입력 확인 기능을 알리지 않으면 기존
attach 입력을 유지하며 지원하지 않는 ACK를 기다리지 않는다. 내부 원격 홈 조회도
사용자 대기로 세지 않는다. 단절은 기존 재연결 안내로 처리하며 지연만으로 재전송하지 않는다.

native WebView는 대기 표시를 위해 숨기거나 크기를 줄이지 않는다. 네이티브 콘텐츠 위의
별도 overlay 층이 없는 경우 생성 중 표지는 탭과 LNB에서 유지한다.

### surface 단위 vs workspace 단위

- **surface attach**: 단일 터미널 surface 를 mirror. 한 연결 = 한 터미널.
- **workspace attach**: 분할 방향과 비율을 포함한 전체 트리를 mirror로 구성한다.
  멤버 터미널도 모두 surface lock에 등록하므로 하나라도 다른 client가 점유 중이면
  workspace attach를 거절한다. kind별 콘텐츠 지원은 아래와 같다.

| kind | mirror 동작 | 제한 |
|---|---|---|
| terminal | 화면 스냅샷과 이후 출력 전달 | 입력은 점유 client만 보낸다 |
| egui-mesh(번들 image/mesh_demo와 커스텀 plugin의 egui-mesh kind) | mesh 프레임 전달 | 입력 역방향 전달은 headless 서버에서만 지원한다. GUI 서버에서는 콘텐츠만 보인다 |
| markdown | 원문을 받아 client의 markdown plugin이 자체 테마·설정으로 렌더 | 원격 상대경로 이미지·링크는 client에 파일이 없어 깨질 수 있다. 주소창에 원격 경로를 넣으면 같은 surface가 그 원격 파일로 바뀌며, 원격에 파일이 없으면 원격이 거절하고 주소창에 실패가 보인다. 최근 목록은 로컬 문서에만 있다 |
| explorer | `list_dir_request` / `list_dir_result`로 디렉토리 탐색. 파일 더블클릭은 원격에 새 탭으로 연다 | 원격에서 콘텐츠를 mirror하는 kind(markdown 등)만 열 수 있다. 파일 변경(새 폴더·새 파일·이름 변경·휴지통·같은 연결 안의 붙여넣기·끌기)은 server 가 원격 쓰기 capability 를 알린 연결에서만 server 가 실행한다. 시스템에서 열기·새 탭·즐겨찾기 추가는 막는다 |
| html | Linux X11·Windows·macOS GUI에서 파일 entry를, Linux X11·Windows·macOS 27 이상 GUI에서 원격 HTTP(S)를 native WebView로 표시 | 양쪽 협상과 client HTML plugin이 필요하다. 파일은 같은 root GET, HTTP(S)는 원격 호스트에서 요청한다. native Wayland·macOS 11–26 client는 HTTP(S)를 알리지 않으며 client network fallback은 없다. macOS는 원격 loopback 주소를 별칭으로 열어 페이지의 `location.host`가 다르다 |
| 나머지 | placeholder | 콘텐츠 mirror 미지원 |

HTTP(S) 서버 전송은 GUI·headless 공통이다. Linux client는 실제 Xlib 창과 X11 GDK backend 준비를 확인한 뒤, Windows client는 GUI 창에서 HTTP·HTTPS·Reload를 선언한다. 전송 지원만으로 native viewer를 켜지 않는다. 요청은 원격 호스트의 서비스 상태를 바꿀 수 있으며 client의 HTML 설정·문서별 동의·기존 수신 한도는 그대로 적용한다.

HTML에는 별도로 협상하는 [파일 전송 기반](../../dev-guide/html-transfer.md)이 있다.
GUI·headless 서버의 최신 요청 source를 조회하고, 크기 확인 뒤 bounded chunk로 원문을
받을 수 있다. 전송 capability만으로 viewer를 켜지 않으며 별도 entry viewer 협상 뒤 HTML role을 보낸다. 구 peer는 placeholder를 유지한다. 크기 질문·스크립트·Reload·연결 수명은 [HTML surface](../../surfaces/html/index.md#원격-전송-기반)를 따른다.

**mesh**는 GUI·headless 서버 모두에서 프레임을 전달한다. GUI 서버에서는 로컬 redraw가
만든 프레임을 중계하며 별도의 렌더 치수를 결정하지 않는다.
[mesh mirror 채널](../../dev-guide/attach-behavior.md#mesh-mirror-채널)을 참고한다.

**explorer**는 활성 탭의 root만 처음 전달하고, 이후 필요한 경로를 client가 조회한다.
`ExplorerViewStore`가 surface별 요청과 캐시를 관리한다. 파일 피커와 같은 인가 규칙을
사용한다. 새 폴더·새 파일·이름 변경·휴지통·같은 연결 안의 붙여넣기·끌기는 server 가
원격 쓰기 capability 를 알린 연결에서만 server 에 요청해 실행한다. 받지 않는 server 와 시스템에서
열기·새 탭·즐겨찾기 추가는 메뉴에서 숨기거나 실행 시 차단한다([탐색기의 원격 파일 변경](../../surfaces/explorer/index.md#mirrorattach-explorer-의-파일-변경)).
파일 더블클릭은 client가 파일 이름만으로 식별하고, `open_surface` 핸들러 중 client에서 콘텐츠를
mirror하는 kind만 원격 새 탭으로 forward한다. 1순위 핸들러가 그런 kind가 아니면 원격에 열 수 있는
핸들러만 담은 핸들러 picker를 띄운다. 사용자 origin이라 client가 새 탭을
선택한다. 원격(서버)은 자기 선택을 유지한다. 원격에 그 kind가 없어 실패하면 아래 실패 회신 규칙대로 toast가 뜬다. 그 밖의 형식은
아래 "다운로드 후 실행" 대화상자로 묻는다([파일 핸들러](../file-handler/index.md)).
mirror 터미널의 파일 경로 링크를 수식키로 클릭해도 같은 판정을 거친다. 링크 우클릭의 "연결 동작"은 자동 실행 없이 원격에 열 수 있는 핸들러만 담은 picker를 열고, 그런 핸들러가 없으면 같은 안내로 끝난다. 파일 선택 창의 원격 경로는 이 경로를 쓰지 않는다.
headless 서버도 같다. attach 요청이 오면 서버가 plugin을 시작해 markdown kind를 등록하고,
그 뒤의 더블클릭은 서버에 markdown 탭을 만들어 client가 원문을 받는다. 등록이 끝나기 전에 도착한
요청은 원격에 kind가 없는 경우와 같이 아래 실패 회신 규칙을 따른다.

**markdown**은 핸드셰이크에서 원격 경로와 탭 제목을 받고 필요한 때 원문을 요청한다.
client에 번들 plugin이 아직 등록되지 않았으면 placeholder를 유지하다 등록 후 채운다.
전송 형태로 700 KiB를 넘으면 잘린 원문과 잘림 표시를 보낸다. JSON 이스케이프가 필요한
따옴표·개행도 이 크기에 포함된다. 서버에서 큰 파일 열기 확인을 기다리는 중이어도 이 채널은
상한 내에서 읽는다. 그 확인은 렌더링 여부에 관한 것이며 파일 읽기 권한 검사는 아니다.
연결이 끊기면 끊김 화면을 표시하고, 재연결 후 원문을 표시할 수 없는 문서는 다시 요청한다.
이미 원문을 표시 중인 문서는 원격 변경 신호를 받아도 자동으로 다시 읽지 않는다
(이 문서).

미지원 kind는 기술적 제한이며 보안을 위해 제외한 것은 아니다. 콘텐츠 접근의 신뢰 경계는
[SSH와 loopback](../../design/systems/memory.md)이다. 자세한 범위는
[attach IPC](../../dev-guide/attach-behavior.md#ipc-표면-attach)와
[plugin 권한](../../dev-guide/plugin-permissions.md)을 따른다.

### 화면 동기화

attach 성립 직후 서버가 현재 화면을 **1회 스냅샷**으로 push 하고, 이후 변화는 delta 로 흐른다. client 는 PTY 없는 mirror 터미널에 바이트를 전달해 같은 grid 를 재구성한다. 프로토콜·mux 상세는 → [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md).

workspace mirror 의 **탭 제목**은 스냅샷 pane JSON 의 tab `name`(원격 `Pane::to_attach_json` 이 `Tab::display_name()` 으로 항상 채운다)을 그대로 쓴다. `name` 이 빠진 비정상 스냅샷이거나 트리가 비어 placeholder tab/pane 을 합성할 때만 로컬 번역값 `attach.tab_title_fallback` 을 제목으로 쓴다 — 방어 폴백이지만 사용자 표면이라 하드코딩하지 않는다([국제화 (i18n)](../../dev-guide/i18n.md)).

**그리드 크기는 client 가 구동(client-driven)** — mirror 의 cols×rows 는 그것을 띄운 **로컬 pane 크기**를 따르고, 원격 PTY 를 그 크기로 reflow 시킨다(docs/features/remote-attach/index.md). "remote authoritative" 는 메커니즘으로만 남는다: 원격 PTY 가 실제 크기의 실제 상태의 기준이라 콘텐츠 래핑(reflow)을 담당하고, 그 확정 크기를 client 에 되돌린다. client가 크기를 요청하고 원격이 확정된 크기를 회신한다. 구현:

- mirror 는 detached 터미널(PTY 없음)이라 로컬 레이아웃 리사이즈 스윕(`Core::resize_all_terminals` / `MainViewState::resize_all`)이 detached 터미널을 로컬에 직접 적용하지 않고, 목표 grid 를 forward 큐에 넣는다. `about_to_wait` 에서 **`StreamControl::ClientResize`**(client→server) 로 원격에 요청한다.
- 서버가 그 surface 의 **실제 원격 PTY 를 요청 크기로 resize**(reflow)한다(holder 만 구동 가능 — 배타 점유라 구동자는 항상 유일). 서버가 **GUI 인스턴스**(창 보유)면 그 host 창의 레이아웃 sweep(`Core::resize_all_terminals`)이 **hard-점유 surface 를 skip**(`is_hard_occupied`)해 자기 창 grid 로 되돌리지 않는다 — skip 이 없으면 host 창이 client-driven grid 를 덮어써 mirror 가 host 창 크기에 고정된다(레터박스). headless 서버는 창이 없어 무해하지만 GUI-hosted 서버엔 필수. detach 시 원복.
- 원격 grid 가 실제로 바뀌면 서버가 기존 **`Control` 프레임(`StreamControl::Resize`, server→client)** 으로 확정 cols/rows 를 통지하고, client 가 그 echo 로만 mirror 를 리사이즈한다. → 로컬을 낙관적으로 먼저 바꾸지 않아(원격 reflow 전 잘못된 grid 재생 방지) desync 가 없다.
- 서버는 모든 크기 요청에 응답한다. 이미 같은 크기면 현재 크기의 `Resize` 를, 점유자가 아니거나 대상이 terminal 이 아니면 `ResizeRejected` 를 보낸다. client 는 응답을 5 초 기다리고, 응답이 없거나 거절되면 같은 요청을 한 번 다시 보낸다. 그래도 실패하면 그 mirror 워크스페이스 위에 크기 동기화 실패 배너를 띄운다. 배너는 실패한 탭 이름을 보이고(같은 이름이 여럿이면 한 번만 보이고 ` ×k` 로 수를 붙인다), **다시 시도**(여럿이면 **모두 다시 시도**)는 같은 요청을 한 번 더 보내며, × 는 배너만 닫는다. 늦게라도 그 크기가 확정되면 배너는 저절로 사라진다. 응답을 보장하지 않는 구버전 서버에 연결하면 기다리지 않는다. 상세는 [크기 요청의 응답과 재시도](../../dev-guide/attach-behavior.md#크기-요청의-응답과-재시도).
- 재attach 에서 살아남은 terminal mirror 는 새 연결이 알린 서버 grid 크기로 맞춘 뒤 새 snapshot 을 받는다. 그래서 끊긴 연결과 함께 echo 를 잃어 mirror 와 원격 grid 가 달라졌어도, 다음 리사이즈 스윕이 로컬 pane 크기와 비교해 다시 요청하고 두 grid 가 pane 크기로 수렴한다. 구조 변경 통지(delta)는 mirror 크기를 바꾸지 않는다.
- 렌더러는 mirror 의 실제 grid 크기로 셀을 pane 좌상단에 배치한다. mirror 가 pane 크기로 reflow 되므로 pane 을 채운다(별도의 고정 80×24 영역에 그리지 않는다). 초기 attach 순간(원격 기본 80×24 → 첫 forward reflow)에는 약 1 RTT 의 짧은 깜빡임이 있을 수 있다.

`StreamControl` 은 `event` 태그 기반 확장 enum 이라, 새 이벤트도 새 `StreamTag` 없이 variant 로 추가된다. 구버전 서버는 `ClientResize` 를 무시하므로(전방호환) 기존 remote-authoritative 동작으로 graceful degrade 한다. 같은 이유로 구버전 client 는 `Attention` 프레임을 파싱하지 못해 조용히 무시하고, 구버전 서버는 그 프레임을 애초에 보내지 않는다 — 어느 쪽도 세션을 깨지 않고 attention 표시만 빠진다. **다만 그 무시가 공짜일 때만 안전하다** — 놓쳐도 잃을 것이 없는 이벤트에만 성립하는 성질이고, 아래 `Loss` 가 그 예외라 무시가 곧 "손실 없음" 으로 읽히지 않도록 **client 의 명시 선언으로 게이트**한다.

Control의 server→client 상태 메시지는 `Resize`(그리드), `Activity`(busy/idle),
`Attention`(주의 표시), `Cwd`(현재 폴더), `Foreground`(전경 프로세스 이름)다. 같은 상태를 다시 적용해도 결과는 같지만,
유실된 메시지가 다음 tick에 자동 재전송된다는 뜻은 아니다. Activity·Attention·Cwd·Foreground는
전송 전에 변화분 캐시를 갱신하므로 전송이 실패해도 값이 같으면 다시 보내지 않는다.
client ack도 없다.

mirror에서 attention을 새로 만들지 않고, 사용자 확인에 따른 해제는 `ClientAttentionClear`로
소유 인스턴스에 전달한다. 상세는 [주의 환기 전파](../../dev-guide/attach-behavior.md#주의-환기attention-전파)를 따른다.

client→server 요청 채널은 `ClientResize`(geometry 구동), `ClientAttentionClear`(주의 환기 해제 edge), `StructuralOp`(구조 변경 forward), `MeshContext`/`MeshInput`/`MeshFullResendRequest`(mesh mirror), 그리고 원격 스크린샷 캡처 업로드([remote-screenshot-clipboard](../remote-screenshot-clipboard/index.md)) · 원격 디렉토리 조회(native file picker, 이 문서) · git 조회 · markdown 원문 조회다.
이들은 **점유한 연결 자체가 권한**(docs/dev-guide/attach-behavior.md 하드 점유)이라는 같은 원칙을 쓰되 검증 지점이 두 갈래다 — 앞의 네 종류는 요청에 실린 **anchor 의 holder 인지**를 직접 확인하고(`workspace_holder(ws) == client`), 캡처·디렉토리·git·markdown 원문 조회 네 종류는 그 client 가 점유한 워크스페이스를 가진 engine(`client_holds_workspace`)을 찾아 그 engine 안에서 처리한다 — 인가되는 집합이 그 engine 전체라는 뜻이다(이 문서).
네이티브 bulk 파일 전송(이 문서)은 이 모델 **밖**이다 — 전용 연결이라 holder 가 아니고, 결속 workspace 를 **소유한** engine 으로 라우팅한 뒤 commit 시점의 finalize 가 인가를 확인한다.

### 모드

- **mirror-dump**(`--dump-after <ms>`): N ms 출력을 수집해 grid 를 재구성하고 텍스트를 stdout 으로 출력 후 종료. GUI 없이 attach 파이프라인을 검증하는 경로.
- **raw 브리지**(`--raw`): stdin↔stdout passthrough(detach 키 `Ctrl+\`). 세션이 끝나면 SGR을 초기화한다. 대체 화면·커서 숨김 같은 다른 모드는 복원하지 않는다. **surface 단위만** — workspace 모드는 다중 터미널이라 불가.
- **단발 입력**(`--send <str>`): attach 직후 1회 입력(escape 디코딩). workspace 모드는 `--send-to <remote_surface_id>` 로 대상 지정.

### GUI mirror

`tasty remote attach --into-gui --target-port <원격포트> --workspace <원격ws>` → 이 명령을 받은 **로컬 GUI 인스턴스**가 client 가 되어 원격 워크스페이스를 mirror 로 재구성한다(`attach.into_gui`). 터널을 호스트에 맡기려면 `--target-port` 대신 `--ssh`·`--profile` 을 준다(`remote.attach`, 아래 [원격 attach (IPC — focus 중립)](#원격-attach-ipc--focus-중립)).
mirror Workspace 는 일반 워크스페이스로 사이드바에 노출되되 **이름과 subtitle 사이 별도 줄의 `>_→` glyph 와 Tag `info` "REMOTE"**(collapsed 레일은 아바타 우하단 하늘색 corner chip)로 로컬과 구분(`Workspace.mirror`).
status dot 은 실행상태(running/idle) 전용이며 mirror 색을 싣지 않는다 — 원격 origin 은 별도 시각 축(디자인 `workspace-mirror-fg`, notif=우상단 / attached=둘레 ring 과 채널 분리).
mirror 콘텐츠(grid) 갱신은 원격 출력이 올 때 즉시, 3초 tick 은 backstop.
실행상태(status dot 의 초록/회색)는 별도 채널로, 원격이 1Hz 로 자신의 busy 상태를 계산해 attach 스트림으로 forward 하고(mirror 터미널은 로컬 PTY 가 없어 스스로 계산할 방법이 없다 — 이 forward 가 유일한 소스) client 가 그 값을 반영한다.
메커니즘 상세는 [dev-guide/attach-behavior "활동(busy) 상태 전파"](../../dev-guide/attach-behavior.md#활동busy-상태-전파).

<a id="원격-위치-표시"></a>

#### 원격 위치 표시

사이드바를 보지 않아도 지금 보는 위치가 원격임을 알 수 있도록 다음 자리에 호스트를 보인다. 원격 호스트를 보이는 다른 자리도 같은 글자를 쓴다. 모양은 [디자인 정본](../../dev-guide/design-change-workflow.md)을 따른다.

| 위치 | 언제 | 문서 |
|---|---|---|
| 상태바 좌측 맨 앞 | focus surface 가 mirror workspace 에 있을 때 | [상태바](../workspace-status-bar/index.md) |
| 핸들러 picker 머리 경로 줄 앞 | mirror 경로에 연 원격 picker(원격에 열 수 있는 후보만) | [핸들러 picker](../file-handler/index.md) |
| 다운로드 후 실행 대화상자 경로 줄 앞 | mirror 경로에 원격에 열 핸들러가 없을 때 | [다운로드 후 실행](#원격-파일-다운로드-후-실행) |
| 링크 런 아래 툴팁 | mirror 의 파일 경로 링크에 수식키 hover | [터미널 링크](../terminal-link/index.md) |

- **호스트 글자**: 연결할 때 고른 `tasty-attach` 프로필 이름이다. 원격 연결 팝업, `remote attach --into-gui --profile`(IPC `remote.attach`의 `profile`), 저장된 원격 행의 attach 프로필이 프로필 이름을 세션에 넘긴다. 연결 시점 값이며 재연결해도 바꾸지 않는다. 연결 뒤 프로필 이름을 바꾸거나 지워도 이미 붙은 mirror 의 표시는 그대로다.
- **프로필 없이 붙은 mirror**: `--ssh <dest>`, `--target-port`(`attach.into_gui`), debug attach 로 붙은 mirror 는 이 연결의 loopback 엔드포인트 `127.0.0.1:<로컬 포트>`(`remote_label`)를 쓴다. 글자는 `ClientSessionState::host_text` 한 곳에서 정한다.
- **재연결 중**: 연결이 끊겨 다시 붙는 동안 상태바는 원격 색을 빼고 상태어를 붙인다. 끊긴 연결을 활성처럼 보이지 않게 한다.
- File Picker 의 host 배지와 탐색기 원격 작업의 호스트 표시(`HostMark`, 쓰기 거절·대기·결과 모름 문구의 `{host}`)도 같은 글자를 쓴다.

**주의 환기(attention)도 같은 방향의 별도 채널이다** — 원격이 자기 surface 의 attention(작업 완료 / 응답 필요)을 같은 1Hz tick 에 `StreamControl::Attention{surface_id, kind}` 로 forward 하고 client 가 자기 `AttentionStore` 에 반영한다.
attention 의 관리 주체는 **surface 를 소유한 인스턴스**다: producer(완료 IPC/CLI, Claude 플러그인 훅, OSC 133 명령 완료, toast)가 전부 PTY 가 있는 쪽에서 돌고, 특히 `needs_input` 은 서버 훅에서만 나와 미러가 스스로 만들 수 없다.
반영된 값은 로컬 attention 과 **같은 store** 에 들어가므로 미러 워크스페이스에서도 사이드바 개수 배지·surface 테두리·탭 제목 색이 그대로 동작한다.
미러는 자기 판단으로 attention 을 만들지 않는다 — 미러 터미널도 서버 바이트를 파싱해 OSC 133 D·Bell 등을 발생시키지만 `raise_attention` 이 mirror surface 를 걸러내므로, 이 push 가 미러 attention 의 **유일한 소스**다(알림 패널 아이템·토스트는 억제 대상이 아니라 그대로 뜬다).
반대로 **해제는 미러에서 서버로 되돌아간다** — 미러 사용자가 그 surface 를 확인(실-포커스 또는 미러 로컬 알림 읽음)해 레코드가 실제로 지워지면 `StreamControl::ClientAttentionClear` 1 회가 서버로 가고 서버 레코드도 제거된다.
미러 사용자의 행동은 서버의 해제 경로를 발동시킬 수 없어, 이 전달이 없으면 서버 배지에 해제 주체가 아예 없다.
메커니즘 상세는 [dev-guide/attach-behavior "주의 환기(attention) 전파"](../../dev-guide/attach-behavior.md#주의-환기attention-전파), 기능 문서는 [features/surface-highlight](../surface-highlight/index.md#원격-attach-mirror-로의-전파-serverclient).

**surface 의 현재 폴더(cwd)도 같은 방향의 채널이다** — 원격이 점유 surface 의 cwd 를 같은 1Hz tick 에 `StreamControl::Cwd{surface_id, cwd}` 로 보낸다.
mirror 터미널은 로컬 셸이 없어 원격 셸이 OSC 7 을 쏠 때만 cwd 를 알 수 있었는데, 원격은 셸 프로세스를 직접 조회할 수 있어 어떤 셸이든 값이 온다.
그 값은 **원격 경로**로 표시되어 로컬 파일시스템 연산(새 로컬 워크스페이스의 시작 폴더, 로컬 git 브랜치 조회, preset 저장)에는 쓰이지 않고, 원격에서 실행되거나 원격을 조회하는 소비자(예: mirror 에서 연 파일 피커의 시작 폴더)만 쓴다.
규칙과 근거는 [surface-cwd invariant §3-2](../../design/policies/cwd.md#3-2-원격-출처-cwd-는-로컬-실행-경로로-새지-않는다) · 이 문서, 메커니즘은 [dev-guide/attach-behavior "surface cwd 전파"](../../dev-guide/attach-behavior.md#surface-cwd-전파).

**전경 프로세스 이름도 같은 방향의 채널이다** — 원격이 점유 surface 의 전경 프로세스 이름을 같은 1Hz tick 에 `StreamControl::Foreground{surface_id, name}` 로 보낸다. 이름을 모르게 되면 `null` 을 보내 client 가 지운다.
mirror 는 이 이름으로 로컬 surface 와 같게 동작한다: 앱별 Shift+Enter 규칙, 상태바 프로그램 이름, 마우스 캡처 제외·배너 제외 목록과 배너 더보기 메뉴, 프로그램이 바뀔 때 캡처 배너 닫기, 원격 자식이 셸로 돌아온 판정, 클릭 커서 이동(1Hz 값이라 전환 직후 지연이 있다), `surface.foreground_process`(원격 PID 는 `null`). 규칙과 목록은 client 의 설정으로 판정한다.
첫 값을 받기 전이나 이 메시지를 보내지 않는 구버전 서버에서는 이름 미상으로 동작해 지금과 같다. 메커니즘은 [dev-guide/attach-behavior "전경 이름 전파"](../../dev-guide/attach-behavior.md#전경-이름-전파).

**서버가 프레임을 버리면 `Loss`로 알린다.** PTY 출력 일부를 놓친 client는 그 상태로
계속 그리면 잘못된 화면을 보여줄 수 있다. 그래서 서버는 그 연결에서 버린 프레임 수를 `StreamControl::Loss{frames}` 로 되돌려
준다. **받겠다고 선언한 연결에만** 간다(`ClientLossNotify` — GUI mirror · GUI bulk 전송 · CLI
`tasty remote attach`·`tasty tool attach` 가 선언한다) — 선언 안 한 peer 의 바이트 열은 무변경이고, 이 통지를 못 읽는
client 에게 "무시" 는 "손실 없음" 과 구별되지 않기 때문이다. 통지를 받은 client 는 **다시
attach 해 화면을 새로 받는다** — 서버가 터미널 snapshot 을 만드는 자리가 attach 하나뿐이라서다.
GUI mirror 는 "원격 화면 일부를 놓쳤다" 경고 toast 를 띄운 뒤 스스로 다시 붙고(수동 attach 도
같다), 그 사이 mirror 터미널의 출력 위치 표지(stream)가 바뀌어 공백을 건너 읽는 에이전트는
불일치 오류를 받는다. CLI 는 stderr 로 알리고 다시 붙으며, bulk 전송은 결과를 모르므로 중단으로
끝난다. 서버 전체의 손실 누계와 지금 쌓인 양은 `tasty list pressure` 의 `stream_push` 로
읽는다([telemetry](../telemetry/index.md)). 종류별 계약·순서·한도는
[dev-guide/attach-behavior "통지를 받은 client 가 하는 일"](../../dev-guide/attach-behavior.md#통지를-받은-client-가-하는-일-재동기화-계약),
근거는 [Attach 메커니즘](../../dev-guide/attach-behavior.md)에 정리되어 있다.


### mirror 워크스페이스 내 구조 변경

mirror 워크스페이스는 "통째로 원격" 인 원격 워크스페이스의 뷰다 — 입력(키스트로크)은 이미 원격 PTY 로 forward 된다. 그 안에서의 **구조 변경**(surface/pane split · 새 탭 · 닫기 · 탭 순서 변경 · 닫은 항목 복원)을 로컬에서 실행하면 로컬 셸 PTY 가 mirror 에 섞여 "workspace 전체가 remote" 불변식을 깬다. 따라서 mirror 워크스페이스 구조 변경은 **로컬에서 실행하지 않고**, 대신 **원격 인스턴스에서 실행되도록 forward** 한다.

- **판별·로컬 차단**: App의 journal 요청 해소가 명시 대상의 mirror 여부를 확인한다. `src/app/journal/commands/forwarding.rs`가 지원 op를 고정하고 로컬 구조·PTY를 변경하지 않는다. 지원하지 않는 교체 이동·respawn·adopt는 원 요청에 오류를 반환한다.
- **forward 요청**: 로컬 anchor와 원격 anchor, connection registration, origin을 원 요청에 고정한다. journal에 forward 의무를 확정한 뒤 App의 `src/app/journal/forward.rs`가 원 연결로 전달한다. 로컬 View의 선택 후속 처리는 원 View identity·선택 세대를 확인한 뒤에만 실행한다.
- **원격 실행**: 서버의 `src/app/journal/commands/inbound.rs`는 holder와 명시 대상을 검증하고 공통 journal 명령·효과 경계로 실행한다. 확정 결과를 `StructuralResult`로 보내고 해당 workspace의 `StructuralDelta`, 새 surface의 출력 tap 순서로 공개한다.

- **점유 상속 (필수 불변식)**: forward 로 원격에 **새로 생긴 터미널은 그 workspace 의 hard 점유를 상속**한다("workspace 전체가 remote" 유지 — docs/dev-guide/attach-behavior.md 은 점유가 surface 생성 방식과 무관함을 못박는다).
  점유는 attach 시점 멤버 스냅샷으로 끝나는 게 아니라, 구조 변경으로 늘어난 멤버까지 확장돼야 한다.
  `execute_forwarded_structural_op` 이 added 터미널을 `OccupancyRegistry::add_workspace_member` 로 `surface_locks`(→`is_hard_occupied`: 서버 입력차단·resize sweep skip·readonly) + `surface_to_workspace`(→`feed_attached_workspace_input`/`apply_attached_workspace_resize` 의 holder 검증)에 같은 holder 로 등록한다.
  이 등록이 빠지면 새 surface 가 비점유로 남아 (1) host 창 sweep 이 자기 grid 로 되돌리는 레터박스 (2) 점유 미표시 (3) client 입력·resize 거부가 발생한다.
  - 생성도 점유 차단 대상이다. `tasty claude/codex spawn`은 `terminal.spawn` →
    `tab::handle_tab_create` → `apply_create_tab`을 사용한다. 로컬 호출로 생성한 직후
    원격 점유를 상속하면 호출자 자신이 새 자식에게 입력하지 못하므로 생성부터 거절한다.
    `pty.attach_surface`도 같은 후처리를 쓰며, 아래 비-holder 구조 변경 차단으로 hard 점유
    workspace의 pane에 입양하는 요청을 거절한다([Attach 메커니즘](../../dev-guide/attach-behavior.md)).
  - hard-occupied workspace 에 새로 생긴 surface 는(차단을 통과한, 즉 holder 본인의 forward 경로로 생긴 surface 는) 위와 동일하게 `OccupancyRegistry::add_workspace_member` + `tap_surface_for_stream` 이 실행돼야 한다 — 실행되지 않으면 PTY/화면버퍼는 정상인데 attach client 로의 스트리밍만 시작되지 않아 그 tab 이 검정 화면으로만 보인다(스트림 tap 이 아예 안 걸린 상태).
    journal publication이 새 멤버의 원 generation을 pending tap으로 기록하고 `src/remote/structure_sync.rs`가 Result/Delta 이후 전송 성공 분기에서 구독을 연다. notifier와 pending reply/tap은 `RemoteState`가 소유한다.
- **실패 회신**: 원격이 op 를 실패 처리(대표적으로 **원격에 등록되지 않은 plugin surface kind** — 원격의 kind 레지스트리가 그 호스트에서 생성 가능한 kind 의 authority)하면 `ok:false`+`reason` 을 회신하고, client 가 실패 toast(`attach.toast.mirror_structural_forward_failed`)를 띄운다.
  단 에이전트가 건 op — IPC 구조 요청(split · tab.create/close/move · pane.close · surface.close · `image.open`)과 에이전트 origin intent(`markdown.navigate`(주소창 이동·외부 요청) · `file_handler.dispatch`(`origin_surface_id` 없이)의 새 탭) — 의 실패는 toast 없이 warn 로그로만 남는다([내부 팝업 시스템](../../design/systems/popup.md)). markdown 주소창 이동의 원격 실패는 toast 대신 plugin 이 `run.result` 로 읽어 실패 종류별 문장으로 주소창에 표시하고, 30초 안에 결과가 없으면 대기 행을 보인다([Markdown surface](../../surfaces/markdown/index.md)).
  사용자가 만진 plugin popup 에서 온 `file_handler.dispatch`·`markdown.navigate`(markdown 파일열기 팝업의 새 탭 열기·제자리 변환)는 사용자 origin 이라 그 실패는 toast 가 된다([파일 핸들러 (File handler)](../file-handler/index.md)).
  요청/응답이라 실패 시 로컬·원격 어느 쪽도 구조가 바뀌지 않는다.
  - **`reason` 은 원격에서 실패를 낸 자리의 문구 그대로다** — split · 새 탭 · convert 모두 같다(예: convert 대상 kind 가 원격에 없으면 `unknown surface kind: <kind>`). client 는 그것을 고치지 않고 toast 끝 괄호 안에 싣는다. 사유가 없을 때의 폴백은 두 층이다: 원격 convert 가 사유 없이 실패하면 `surface <id> was not converted` 를 싣고(원인을 짐작한 "not found" 류 문구를 쓰지 않는다 — 원인이 다를 때 실제 사유를 가린다, [Attach 메커니즘](../../dev-guide/attach-behavior.md)), wire 에 `reason` 이 아예 없으면 client 는 괄호 없는 기본 문구만 띄운다.
- **역반영 (성공 시)**: 성공한 forward 로 원격에 생긴/사라진 surface 를 mirror 트리에 반영한다. 원격이 실행 후 워크스페이스 **전체 트리+surfaces** 를 `StreamControl::StructuralDelta` 로 push(`StructuralResult` 성공 회신 **직후**)하고, client 가 이를 받아 mirror 트리를 증분 재구성한다. survivor(이미 mirror 로 존재하는 원격 surface)는 **기존 mirror 터미널을 그대로 유지**(scrollback 보존)하고, 새 원격 surface 만 새 mirror 로 추가, 사라진 surface 는 제거한다. 최소 증분(surface 별 diff) 대신 full-tree 재동기화를 쓰는 이유: client 는 surface 매핑만 보유한다는 불변식을 지키면서 split·새 탭·닫기(cascade)·탭 순서 변경을 균일하게 반영하기 위함. pane 상위 배치(direction/ratio)도 핸드셰이크와 동일한 트리 필드로 정확히 승계된다.
- **focus 는 원격이 아니라 client 가 보존한다**: 위 역반영 트리가 담는 focus(어느 pane/탭이 focused 인지)는 원격 값 그대로다 — 순수 pane/탭 전환은 forward 되지 않으므로 원격의 focus 는 사실상 워크스페이스 생성 시점에 고정돼 있다. 매 역반영마다 이 고정값으로 로컬을 통째로 교체하면 사용자가 mirror 안에서 실제로 보고 있던 pane/탭이 매번 첫 pane/첫 탭으로 튀는 문제가 있었다. client 는 교체 직전 로컬 focus 위치를 remote surface id 기준으로 기억해뒀다가 교체 직후 그 위치로 되돌린다.
  - **단, "무관한 delta 로부터 옛 focus 를 지키는 것"과 "이번 조작 자체의 결과로 focus 가 움직여야 하는 것"은 다른 문제다.** 사용자가 mirror 안에서 직접 새 탭/split 을 만들면 옛 focus 복원만으로는 새로 생긴 리소스로 focus 가 전혀 안 옮겨가고, focus 중인 surface 자체를 닫으면 복원 대상이 사라져 원격의 고정값(대개 워크스페이스 첫 pane/첫 탭/첫 surface)으로 튀어 버린다.
    그래서 client 는 이 op 이 **실제 사용자 GUI 조작**(단축키/버튼/컨텍스트 메뉴 — IPC/CLI/에이전트 호출은 제외)이었는지를 forward 시점부터 태그(`user_triggered`)해두고, 성공 회신에 상관지어(op_id) 뒤따르는 delta 적용에서: 새 탭/split 이면 새로 생긴 surface 로 focus 를 옮기고, close 로 옛 focus 복원이 실패했으면 닫히기 **전** 캡처해둔 인접 후보(같은 tab 의 형제 surface, 또는 같은 pane 의 인접 탭)로 fallback 한다.
    IPC/CLI 로 같은 조작을 했을 때는 이 태그가 항상 꺼져 있어 focus 가 그대로 안 움직인다(회귀 없음 — "포커스 독립성" 유지).
  - 메커니즘 상세는 [dev-guide/attach-behavior "focus 보존"](../../dev-guide/attach-behavior.md#mirror-구조-변경-forward).
- **서버 쪽에서 바뀐 구조도 역반영한다**: 원격 셸이 끝나(`exit`) 원격에서 탭/pane 이 닫히면, forward 가 없었어도 원격이 같은 `StructuralDelta` 를 push 해 mirror 에서도 곧바로 사라진다. 원격에서 로컬 경로로 워크스페이스에 편입된 surface 도 같은 메시지로 mirror 에 나타난다(그 surface 의 화면 스냅샷보다 트리가 먼저 간다). holder 가 이미 받은 트리와 내용이 같으면 다시 보내지 않는다. 새 메시지는 없다 — 역반영 메시지를 forward 성공 말고도 보낼 뿐이다([Attach 메커니즘](../../dev-guide/attach-behavior.md)). 그 셸이 워크스페이스의 마지막 surface 였으면 아래 강제 detach 와 같다.
- **역반영 대신 강제 detach (workspace 자체가 cascade 로 사라지는 경우)**: workspace 의 **마지막 surface** 를 forward `CloseSurface` 로 닫으면, 원격의 `close_case_workspace`("Case 4: last pane in workspace")가 트리 일부가 아니라 **workspace 자체**를 통째로 purge 한다 — 이 경우 되돌릴 delta 자체가 없다.
  `src/app/journal/commands/inbound.rs`의 `deliver_result`가 실행 후 원 workspace의 부재를 확인하면 `force_detach_workspace`로 holder를 분리하고 점유를 정리한다.
  client 는 이를 일반 force-detach 와 동일하게 처리해 mirror 를 정리한다 — 재attach 없이도 즉시 반영된다.
  메커니즘 상세는 [dev-guide/attach-behavior "점유 레지스트리"](../../dev-guide/attach-behavior.md#점유-레지스트리-occupancyregistry).
- **닫은 항목 복원(`Ctrl+Shift+T`)도 forward 대상이다 ([Attach 메커니즘](../../dev-guide/attach-behavior.md))**: mirror 를 보는 중에 누르면 **원격에서** 닫혔던 탭이 되살아나 mirror 에 나타나고, 그 안의 입력은 원격 PTY 로 간다. 복원은 새 PTY spawn 이고 스냅샷의 스크롤백은 서버 디스크에 있으므로 서버만 실행할 수 있다. **원격에 복원할 것이 없어도 로컬 항목을 대신 되살리지 않는다** — 안내 toast 만 뜨고 로컬 스택은 그대로 남아, 로컬 워크스페이스로 돌아가 같은 키를 누르면 그때 복원된다. 즉 **스택이 둘이고 보고 있는 워크스페이스가 어느 쪽을 쓸지 정한다.**
  - **원격에서 누른 복원은 그 워크스페이스 것만 꺼낸다**: 서버의 복원 스택은 워크스페이스로 스코프돼, mirror 사용자의 복원이 서버 앞에 앉은 사용자가 방금 닫은 다른 워크스페이스의 탭을 가져가지 않는다. 반대 방향은 스코프를 걸지 않는다 — 서버 로컬 복원은 지금까지처럼 전역 LIFO 다. forward 된 close 가 서버 자신의 트리에서 탭을 없애므로, 그 기계 앞의 사용자에게도 undo 가 남아야 하기 때문이다.
  - **mirror 안에서 손으로 닫은 것은 되돌려진다**: 원격 사용자의 손으로 닫힌 탭/pane/surface 는 서버의 복원 스택에 남는다.
  - **에이전트가 닫은 것은 어느 쪽 복원 스택에도 안 남는다**: 클라이언트의 에이전트가 CLI/IPC 로 mirror 를 닫으면 그 close 도 원격으로 forward 되지만, forward 가 "에이전트의 요청" 이라는 표시를 싣고 서버는 그것을 복원 스택에 넣지 않는다. 그렇지 않으면 서버 앞 사용자의 복원 단축키가 에이전트가 닫은 것을 되살린다 — 사용자의 닫은 항목 히스토리는 사용자가 닫은 것만 담는다([포커스 정책](../../design/policies/focus.md)). 표시가 없는 forward(이 표시 이전의 클라이언트)는 사용자 조작으로 읽는다([Attach 메커니즘](../../dev-guide/attach-behavior.md)).
  - **복원된 탭의 옛 스크롤백은 mirror 에 전량 오지 않는다**: 서버 쪽 터미널은 스크롤백까지 되살아나지만, mirror 는 기존 규약대로 보이는 화면 1 회 스냅샷 + 이후 변경분만 받는다. 원격 윈도우에서 보면 스크롤백이 그대로 있다.
- **mirror 워크스페이스 자체를 닫는 것**은 로컬 mirror 뷰를 걷어내는 정당한 로컬 동작이라 차단·forward 대상이 아니다.
- **`terminal.spawn` 은 forward 대상이 아니라 거부 대상이다 ([Attach 메커니즘](../../dev-guide/attach-behavior.md))**: 위 forward 는 fire-and-forget 이라 응답이 원격에서 생긴 리소스의 id 를 담지 않는다.
  `tasty claude/codex/terminal spawn` 이 타는 `terminal.spawn` 은 그 id 를 **동기로** 받아 child registry 등록·soft 점유·후속 command 주입까지 이어가야 하므로 이 응답 모델 위에 얹힐 수 없다.
  막지 않으면 로컬은 에러를 돌려주는데 forward 큐는 IPC 응답과 무관하게 드레인되어 **호출자가 관리하지 못하는 원격 탭**이 생긴다.
  그래서 mirror 워크스페이스를 대상으로 한 `terminal.spawn` 은 tab/surface 를 하나도 만들지 않고 `invalid_params` 로 즉시 거부하며, 메시지에 mirror 사유와 대안(다른 워크스페이스 사용 / 원격 인스턴스에서 직접 spawn)을 담는다.
  나머지 구조 변경은 아래 "현재 범위"대로 mirror 에서도 forward 된다 — 이 거부는 `terminal.spawn` 한 method 에만 적용된다.
  `tasty claude spawn`·`tasty codex spawn` 의 mirror 대상은 이 forward 가 아니라 아래 [원격 자식 에이전트](#원격-자식-에이전트) 경로로 서버에서 실행된다.
- **탭·페인 교체 이동은 forward 하지 않고 로컬에서 차단한다**: 탭 헤더 메뉴의 `탭 이동`/`페인 이동`으로 고른 교체 이동(`ReplaceTabWithTab`/`ReplacePaneWithPane`)은 source 나 target 중 하나라도 mirror 워크스페이스면 App의 mirror 구조 요청 해소가 로컬 실행을 거부하고, 대응 `StructuralOp` 가 없어 원격으로 보내지 않는다. 사용자에게는 차단 toast(`attach.toast.mirror_structural_blocked`)만 뜬다([surface-move](../surface-move/index.md)).
- **터미널 재시작·PTY 입양은 forward 하지 않고 로컬에서 거부한다**: mirror surface 대상 `RespawnTerminal`(`surface.respawn_terminal`·`terminal.respawn`)과 mirror pane 대상 `AdoptTerminal`(`pty.attach_surface`)은 App의 mirror 구조 요청 해소가 거부한다. 둘 다 로컬 PTY 를 mirror 트리에 넣는 동작이라 원격 트리와 어긋나고, 대응 `StructuralOp` 가 없어 원격으로 보내지 않는다. IPC 응답은 mirror 사유를 담은 오류이며 트리·PTY registry 는 바뀌지 않는다.
- **탭·페인 preset 적용은 forward 하지 않고 로컬에서 거부한다**: tab preset 의 대상 pane(생략하면 활성 workspace)이나 pane preset 의 대상 workspace(생략하면 활성 workspace)가 mirror 면 `preset.apply` 와 preset 팝업의 적용은 탭·pane 을 만들기 전에 `MirrorStructuralBlocked` 로 거부한다. terminal leaf 면 로컬 셸을 mirror 트리에 넣게 되고, preset 은 대응 `StructuralOp` 가 없어 원격으로 보낼 수 없다. IPC 응답은 mirror 사유를 담은 오류이고, 사용자가 적용했으면 차단 toast(`attach.toast.mirror_structural_blocked`)가 뜬다. workspace preset 은 새 로컬 workspace 를 만들므로 거부하지 않는다.

**현재 범위**: surface split / pane split / 새 탭 / surface·tab·pane 닫기 / workspace 전체 닫기(사이드바 메뉴·단축키, 아래 "mirror 닫기와 연결 끊기") / 탭 순서 변경(`MoveTab` — 탭 헤더 메뉴의 교체 이동 `탭 이동`과 다르다) / 닫은 항목 복원 / surface convert(kind 변환, `markdown.navigate`/`image.open`/host convert 팝업이 모두 이 경로를 탄다 — 변환 결과의 cwd 는 op 의 `cwd` 필드로 전달되고, 비어 있으면 원격이 대상 surface 의 실제 PTY 에서 직접 resolve 한다.
[surface-cwd invariant §3-1](../../design/policies/cwd.md#3-1-mirror원격-attach-forward-경로도-같은-불변식-대상)) / surface 이동(move-surface)이 forward 대상이며, 성공 시 원격 실행 결과가 mirror 트리에 역반영된다.
move-surface 는 **source/target 이 같은 mirror workspace 안에 있을 때만** forward 된다 — 로컬(비-mirror) workspace 와의 경계를 넘는 이동은 로컬 전용 surface_id 를 원격에 그대로 보내는 꼴이 되어(원격 트리의 무관한 surface 와 id 가 우연히 겹칠 위험) 여전히 로컬 차단 toast(`mirror_structural_blocked`)를 유지한다. 같은 workspace 안의 이동도 client 는 source 와 target 을 **둘 다** 같은 mirror 세션의 매핑으로 원격 ID 로 바꿔 보내고, target 의 원격 ID 를 모르면 보내지 않는다. 서버는 target 도 점유된 같은 workspace 안의 살아 있는 surface 인지 다시 확인하고, 아니면 `no live surface N (named by 'structural_op.move_surface')` 로 거절해 다른 workspace 의 surface 를 덮어쓰지 않는다([attach 동작](../../dev-guide/attach-behavior.md)).

### mirror 닫기와 연결 끊기

에이전트 mirror(CLI·IPC로 만든 mirror) 카드의 우클릭 메뉴는 마지막 묶음에 **워크스페이스 연결 끊기**(메뉴 id 8)와 **워크스페이스 닫기**(id 6)를 함께 둔다. 로컬 카드에는 닫기만 있다. 저장된 원격 행은 아래 "저장된 원격 행의 메뉴와 닫기"의 묶음을 쓴다. 두 항목은 메뉴를 연 workspace ID로 대상을 다시 찾으며, 그 사이 대상이 사라졌으면 아무것도 하지 않는다. 서버 쪽 **강제 끊기**(id 7, 이 인스턴스의 workspace를 점유한 다른 client를 내보낸다)와 다르다.

- **워크스페이스 닫기(서버 닫기)**: 사이드바 메뉴뿐 아니라 `close_workspace` 단축키와, 마지막 surface·pane에서 workspace 닫기로 넘어가는 `close_surface`·`close_pane`·`close_active` 단축키(더블 탭 포함)도 같은 서버 닫기다. 로컬 workspace의 "마지막 것을 닫으면 workspace가 닫힌다"와 같은 규칙을 따르며, 서버를 유지하려면 연결 끊기를 쓴다. `DomainIntent::CloseWorkspace`가 mirror를 가리키면 `StructuralOp::CloseWorkspace{anchor_surface_id}`로 forward한다. 서버는 그 연결이 점유한 workspace를 `intent.close`의 workspace 대상으로 닫고 PTY를 끝낸다. 사용자 요청이면 서버의 닫은 항목 기록에 남는다.
  - 보내기 전에 연결을 확인한다. 연결이 없거나 재연결 중이면 `attach.toast.workspace_close_reconnecting`, 서버 descriptor에 `ipc.stream.workspace-close`가 없으면 `attach.toast.workspace_close_unsupported`, 같은 mirror의 닫기가 회신을 기다리면 `attach.toast.workspace_close_pending`을 띄우고 보내지 않는다.
  - 같은 mirror의 닫기 두 개가 첫 전송 전에 함께 준비돼도(키 반복·메뉴와 단축키) `execute_journal_forwards`가 전송 직전에 `workspace_close::send_refusal`로 다시 보고 두 번째를 보내지 않고 취소한다. 기다리던 op id가 덮어써지지 않아 첫 성공 회신이 mirror를 정리한다.
  - client는 그 op의 성공 회신을 받은 뒤에만 mirror·세션을 정리한다. 성공 안내는 `attach.toast.workspace_closed_on_server`다. 서버 거절은 기존 구조 변경 실패 안내로 끝나며 mirror를 남긴다. 회신 전에 연결이 끝나면 `attach.toast.workspace_close_unknown`을 띄우고 기존 단절 처리(저장된 행이면 재연결, 아니면 정리)를 따른다.
  - 서버의 마지막 workspace를 닫으면 서버는 빈 창 대신 새 터미널 workspace를 만든다. 서버 로컬 사용자의 마지막 workspace 닫기(창 닫기)와 구분한다.
  - 저장된 원격 행을 서버에서 닫으면 행도 닫혀 저장 항목이 지워진다.
- **워크스페이스 연결 끊기**: `Intent::DisconnectMirror{workspace_id}`를 App이 처리한다. 에이전트는 IPC `remote.disconnect`(CLI `tasty remote disconnect --workspace <mirror id>`)로 같은 정리를 부른다. 그 mirror의 재연결 endpoint·connection 시도를 취소하고 사용자 종료 정리(`cleanup_mirror_workspace(.., false)`)로 mirror·문서 surface·heartbeat를 정리한 뒤 transport를 놓아 socket·worker·소유 터널을 회수한다. 서버 workspace와 프로세스는 남고 서버 점유는 `Detach`(전달하지 못하면 EOF/TTL)로 풀린다. 다른 mirror 세션은 유지한다.
  - 재연결 중에도 끊을 수 있다. 늦게 도착한 재연결 결과는 세션·epoch가 없어 설치되지 않는다.
  - 저장된 원격 행은 남는다. 행을 **연결 끊김** 상태로 두고(`enter_disconnected`), 사용자가 끊은 행은 다시 활성화해도 자동으로 연결하지 않는다(`Remote.user_disconnected`, 아래 "사용자 대기 갈래"). 원격 연결 팝업에서 같은 행을 다시 고르면 연결한다. 네트워크 단절의 자동 재연결은 그대로다.
  - 마지막 workspace였으면 창을 닫지 않고 기본 터미널 workspace를 만든다. 사용자의 닫은 항목 기록에는 넣지 않는다.

### 저장된 원격 행의 메뉴와 닫기

저장된 원격 행의 우클릭 메뉴는 일반 workspace 메뉴 끝에 연결 묶음과 닫기 묶음을 둔다(`src/view/main/saved_row_menu.rs`). 카테고리가 꺼져 있어도 열린다. 켜짐 규칙과 문구 키는 갤러리 표본과 같은 `tasty_ui_widgets::remote_row_menu`가 정한다. 할 수 없는 항목은 숨기지 않고 끈다.

| 항목 | 켜지는 상태 | 동작 |
|---|---|---|
| Connect · Reconnect(id 9) | 연결 끊김, 연결 전. 다른 곳에서 닫힌 행은 끈다 | 사용자 요청으로 연결한다(`Intent::SavedRow{Connect}`). 이번 실행에서 형상이 있었던 행은 Reconnect 문구다 |
| Disconnect(id 8) | 연결됨, 연결 중, 재연결 중 | 위 "워크스페이스 연결 끊기"와 같다. 시도 중이면 그 시도를 취소한다 |
| Close locally(id 10) | 항상 | 행을 이 Tasty에서 없앤다(`Intent::SavedRow{CloseLocally}`). 붙어 있으면 `Detach`만 보내고, 원격 workspace와 프로세스는 남는다. 연결 중인 시도도 취소한다 |
| Close remote workspace…(id 11) | 연결됨 | 확인 대화를 연 뒤 위 서버 닫기를 보낸다. 성공하면 행도 사라진다 |

- 행의 상태는 App이 engine마다 싣는 `remote.row_states`를 읽는다(`EngineRead::remote_row_state`). 세션 단계가 `Connected`여도 손실 뒤 재동기화(Resync)로 소켓을 놓은 동안은 재연결 중으로 본다.
- `close_workspace` 단축키를 저장된 원격 행에서 누르면 로컬에서 닫기와 같다(`state::workspace::close_workspace_at`).
- **원격 닫기 확인**(`src/adapters/ui/popup/confirm_close_remote.rs`): Window 범위, scrim 가운데, 폭 `info-modal-width`(440)다. 경고 글리프와 제목(행 이름), 본문(프로필), Cancel(기본 포커스)과 위험 버튼을 그린다. Esc·Cancel 은 닫기만 하고 아무것도 보내지 않으며, 바깥 클릭으로는 닫히지 않는다. 콘텐츠는 갤러리와 같은 `tasty_ui_widgets::close_remote_confirm`이다.
- 사용자가 보낸 구조 요청이 저장된 원격 행을 비우면(workspace 닫기, 또는 지우는 surface가 그 workspace의 전부인 탭·pane·surface 닫기) dispatch가 요청을 보내지 않고 같은 확인을 연다(`workspace_close::saved_row_close`). 마지막 탭·pane 닫기에서 열리면 본문 앞에 `remote.row.close_remote.last` 한 줄을 더한다. 확인하면 담아 둔 요청을 이 팝업 출처(`UserSource::Menu("confirm_close_remote")`)로 다시 보내고, dispatch는 그 출처를 다시 묻지 않는다. 에이전트 요청(IPC `workspace.close` 등)은 묻지 않고 서버로 보낸다.
- 다른 곳(다른 client, 원격 Tasty)에서 원격 workspace가 닫히면 행은 남고 안내로 이유를 보인다. 메뉴와 연결 팝업의 Reconnect는 비활성이다.

### 서버(피점유)측 비-holder 구조 변경 차단

위 절이 다루는 것은 **client(점유 holder)측** 구조 변경이 원격(서버)에서 실행되도록 forward 되는 경로다. 반대 방향 — **서버 자신이 hard-occupied 상태인 자기 workspace 에 대해, 점유 holder 가 아닌 제3자(서버 로컬 IPC/CLI/agent)가 직접** 구조 변경 IPC(`split`/`workspace.close`/`tab.create`/`terminal.spawn`/`pane.close`/`tab.close`/`tab.move`/`surface.close`/`markdown.navigate`/`image.open`/`preset.apply`/`pty.attach_surface`)를 호출하는 경우도 배타성 위반이다 — [Attach 메커니즘](../../dev-guide/attach-behavior.md) 이 정의하는 hard 점유의 배타성은 입력(`apply_send_to_surface`)·resize(`resize_all_terminals`)뿐 아니라 구조 변경까지 적용돼야 한다.

- **차단 대상**: 위 IPC 12종을 **일반 IPC/CLI 진입점**(서버 로컬 호출)으로 직접 호출하고, 대상 pane/tab/surface(`terminal.spawn` 은 `pane` 오버라이드까지 반영해 확정된 **최종 pane**) 가 hard-occupied workspace 에 속한 경우.
  요청은 `invalid_params` 에러(안내 문구: "점유 중이라 불가능, 다른 workspace 사용")로 거부되고 트리는 전혀 바뀌지 않는다.
  `terminal.spawn`(`tasty claude/codex spawn` 이 호출) 은 [Attach 메커니즘](../../dev-guide/attach-behavior.md) 으로 이 목록에 추가됐다 — spawn 자체는 성공 응답을 주면서 그 결과물(새 surface)이 즉시 같은 hard lock 을 상속받아, spawn 을 호출한 쪽조차 자기 결과물에 입력을 못 넣게 되는 부작용을 막기 위함.
  다만 `terminal.spawn` 의 판정만 다른 11종과 **집행 지점이 다르다** — 나머지는 라우터 가드(`hard_occupied_structural_guard`)가 method 파라미터로 대상을 찾지만, `terminal.spawn` 의 실제 대상은 `pane` 오버라이드까지 반영해 확정된 pane 이라 그것을 아는 핸들러 안(`spawn_target_guard`)에서 건다([Attach 메커니즘](../../dev-guide/attach-behavior.md)).
  덕분에 `--workspace <비점유 ws>` + `--pane <hard-occupied ws 의 pane>` 조합으로 이 가드를 우회하던 구멍도 함께 닫혔다.
  `markdown.navigate`/`image.open` 은 convert 진입점이 kind 별로 흩어져 있어 이 두 method 만 커버한다(완전하지 않음 — host 범용 convert 팝업은 `state.dispatch_intent` 를 직접 호출해 이 IPC 라우팅 자체를 안 타고, 향후 새 kind 가 자기 전용 convert 진입 method 를 추가하면 이 목록에 없는 한 가드가 적용되지 않는다).
- **차단 대상이 아닌 경우(중요)**: 점유 holder 본인이 mirror 안에서 실제로 만든 구조 변경이 위 forward 경로로 서버에 도달해 실행되는 것은 **정상 동작이며 이 차단의 대상이 아니다** — "attach 연결 자체가 그 workspace 에 대한 구조 변경 권한을 증명한다"는 forward 모델(위 절)을 그대로 유지한다. `terminal.spawn` 은 forward 대상(위 "현재 범위")에 포함되지 않으므로 이 예외와 무관 — 가드 추가가 holder 의 정당한 forward 요청을 막는 회귀는 없다.
  `preset.apply` 는 탭 preset 이면 `target_pane_id`, pane preset 이면 `target_workspace_id` 의 workspace 로 판정하고, 대상을 생략하면 적용 코드와 같게 그 창의 활성 workspace 를 대상으로 본다. workspace preset 은 새 workspace 를 만들므로 차단하지 않는다.
  `pty.attach_surface` 는 `pane_id` 의 workspace 로 판정한다. 거부되면 PTY 는 headless registry 에 그대로 남는다.
  `terminal.kill` 은 child 를 닫기 전에 `surface.close` 와 같은 surface 단위 점유 검사로 거부한다. 점유를 먼저 강제 해제하지 않으므로 holder 는 분리되지 않고 child 관계도 남는다([child-terminal](../child-terminal/index.md)).
- **차단 근거**: [포커스 정책](../../design/policies/focus.md)(에이전트 행동의 부수효과가 사용자 상태에 닿지 않아야 함) — 서버 로컬에서 만든/닫은/옮긴 탭이 점유 client 화면에 통지 없이 편입/소멸/재배치되면, 원격 사용자가 보고 있는 화면에 자신이 하지 않은 변화가 일어나는 셈이라 이 원칙을 위반한다.
- **메커니즘**: [dev-guide/attach-behavior "서버 로컬(비-holder) 구조 변경 차단"](../../dev-guide/attach-behavior.md#서버-로컬비-holder-구조-변경-차단).

### 저장된 원격 행의 연결과 재연결

저장된 원격 행은 사용자가 그 행을 화면에 띄울 때 연결한다(`src/app/auto_attach.rs::maybe_connect_visible_row`). 행이 가리키는 attach 프로필을 해석해 SSH 터널을 열고 같은 행에 mirror 를 설치하며, 행의 이름·카테고리·위치는 그대로 둔다. 자동 연결은 mirror 를 채우기만 하고 포커스와 활성 workspace 를 옮기지 않는다([포커스 독립성](../../design/policies/focus.md#cliipc-포커스-독립-원칙)). 다시 시작한 직후의 행은 형상 없이 기다리다가 선택되면 연결한다.

- **첫 연결 실패**: 이유를 행 안내로 남기고(위 "저장된 원격 행의 연결 실패 안내") 사용자가 그 행으로 다시 옮기거나 다시 연결을 고를 때까지 자동으로 시도하지 않는다(`Remote.pending_reactivation`). 같은 실패가 될 수 있는 SSH 접속을 되풀이하지 않기 위해서다. 예외로 그 행을 쓰는 IPC 요청은 한 번 다시 시도한다(`connect_saved_row` 가 `pending_reactivation` 을 지운다). 에이전트가 그 행을 쓰려 할 때 연결하기 위해서다. 첫 연결이 `remote_workspace_missing` 으로 끝난 행은 다른 곳에서 닫힌 행이라 IPC 요청도 시도하지 않고 거절한다.
- **연결이 끊기면 mirror 는 살아 있는 채로 재연결한다**: heartbeat TTL 만료·force-detach 등 원격발 disconnect 로 저장된 행의 세션이 끊기면 mirror workspace/터미널을 걷어내지 않고 `Reconnecting` 상태로 두고(`src/app/attach_client/connection.rs::enter_reconnecting`), 지수 백오프(0.5s→30s, ±20% jitter)로 다시 연결한다(`src/app/auto_attach.rs::maybe_trigger_reconnect`).
  - Reconnecting 에 들어갈 때 옛 연결의 소켓을 바로 닫는다(`ConnectionWorkers::close_socket`). 열어 두면 응답이 멈췄던 서버가 돌아온 직후 옛 연결을 살아 있는 점유자로 보고 새 attach 를 `already_attached` 로 거절한다.
  - 재연결에 성공하면 살아 있던 surface 는 scrollback/local id 를 그대로 유지한 채(survivor mapping, `merge_survivor_mapping`) 연결만 새로 맺는다. 사용자가 그 행으로 돌아오면(엣지) 백오프 대기 없이 바로 한 번 더 시도한다.
  - 다른 client 가 그 원격 workspace 를 점유 중이면(`already_attached`) 다시 시도해도 같으므로 재연결을 멈추고 `occupied` 안내를 남긴 채 행을 바로 **연결 끊김** 상태로 둔다(`on_reconnect_attempt_failed`). `remote_workspace_missing`·`mirror_workspace` 도 같다.
- **재연결 시간 창**: 끊긴 뒤 설정 `[remote_attach] reconnect_timeout_secs`(설정 창 General › Remote attach 의 `Keep reconnecting for`, 기본 300초, 0~3600초, 범위를 벗어난 파일 값은 300으로 되돌린다)이 지나도 연결하지 못하면 재연결을 멈추고 행을 **연결 끊김** 상태로 바꾼다. toast 는 띄우지 않고 연결 팝업이 알린다. 0이면 끊기는 즉시 연결 끊김이 된다. 연결 끊김 행은 사용자가 다시 활성화하면 연결한다. 시간 창은 재연결 예약과 함께 연결 결과를 받기 전(`drain_auto_attach_results`)과 준비된 연결을 설치하기 전(`poll_pending_mirror_installs`)에도 확인한다(`end_expired_reconnect_windows`). 시간 창이 끝난 뒤 준비된 연결은 설치하지 않고 행을 연결 끊김으로 둔다.
- **상태**: 행의 세션 상태는 `Connected` · `Reconnecting` · `Disconnected` 셋이다(`tasty_remote::client_session::SessionState`).
- **보이면 연결, 사용자가 끊은 행은 명시적 연결로만**: 화면에 띄운 행은 바로 연결한다(`first_connect_due`). 사용자가 끊은 행은 다시 활성화해도 연결하지 않고, 팝업·메뉴·원격 연결 팝업의 연결처럼 명시적으로 연결하면 그 표지를 지운다.

### 저장된 원격 행의 표시

연결되지 않은 저장된 원격 행은 toast 대신 연결 팝업과 표지로 상태를 보인다. 재연결 시작·성공·포기는 toast 를 띄우지 않는다. 팝업 내용과 표지는 갤러리 "Saved remote workspace" 절과 같은 공용 위젯이다.

- **연결 팝업**(`src/adapters/ui/popup/remote_row.rs`, 내용은 `tasty_ui_widgets::remote_row_popup`): 행이 연결되지 않은 동안 그 workspace 범위로 workspace 영역 가운데에 뜨고, 연결되면 닫힌다. 포커스를 가져가지 않고 바깥 클릭으로 닫히지 않는다. 폭은 360(배율 적용, workspace 영역보다 넓으면 줄인다)이다.
  - 이번 실행에서 형상이 있었던 행은 마지막 화면 위에 scrim 을 깔고 띄운다(`remote_row_over_screen`). scrim 이 포인터를 받아 아래 터미널은 hover·클릭을 받지 않고, scrim 이 덮일 때 그 행에서 연 파일 핸들러 picker 와 링크 툴팁을 닫는다. picker 는 연 요청의 출처 workspace 를 기록한다. 사용자 조작은 그때의 활성 workspace, 출처 surface 를 준 요청(IPC `file_handler.dispatch` 의 `origin_surface_id` 등)은 그 surface 의 workspace 다. 연결돼 있던 저장 행이 끊기면 사용자가 다른 workspace 에 있어도 그 행에서 연 picker 를 그때 닫는다. picker 는 열 때 그 행의 연결 여부를 함께 기록하므로, 한 번도 그려지기 전에 끊겨도 첫 동기화에서 닫힌다. 이미 끊긴 행에서 연 picker 와 로컬 workspace 에서 연 picker 는 남는다. 형상이 없던 행은 빈 workspace 에 scrim 없이 띄운다(`remote_row`).
  - 제목과 본문은 상태에 따라 연결 중, 연결 전, 연결 실패(이유), 재연결 중, 재연결 포기, 사용자가 끊음, 다른 곳에서 닫힘(`remote.row.popup.closed_title`) 중 하나다. 갈래는 `popup_kind` 가 정한다.
  - 필드는 attach 프로필과 원격 workspace 다. 원격 workspace 는 번호로 보인다. 원격 이름은 저장하지 않는다.
  - 버튼은 Connect·Reconnect 이고 `Intent::SavedRow{Connect}` 를 팝업 출처로 보낸다. 시도 중이면 spinner 와 Connecting·Reconnecting 문구로 끈다. 다른 곳에서 닫힌 행은 Reconnect 를 끈 채 그린다.
- **사이드바 카드와 rail**: 카드의 원격 줄은 상태어(`remote.row.state.*` 등)를 붙이고 연결되지 않은 동안 원격 색을 뺀다. rail 의 mirror 칩은 연결되지 않은 동안 꺼진 칩으로 그린다. 연결되지 않은 행은 바쁨 수를 세지 않고, 시도 중이면 시도를 시작한 시각부터 로딩 표시를 그린다. 연결 실패 안내는 카드 툴팁에 프로필 → 원격 workspace 와 이유로 보인다.
- **상태바**: 연결 끊김 행은 재연결 중처럼 원격 색을 빼고 `status_bar.remote_disconnected` 상태어를 붙인다.
- **거절 안내**: 배너는 없다. 팝업과 카드가 같은 이유를 보인다.
- **다른 곳에서 닫힌 행**: 이번 실행에서 원격 workspace 가 없다고 확인된 행이다. 연결돼 있던 동안 다른 곳에서 닫혀 재연결이 `remote_workspace_missing` 으로 끝난 행과, 첫 연결이 같은 이유로 끝난 행이 모두 해당하며 형상 유무와 상관없다. 다시 활성화해도 자동으로 연결하지 않고, 팝업은 다른 곳에서 닫힘 갈래로 버튼을 끄며 메뉴의 Connect·Reconnect 도 끈다(`row_closed_elsewhere`). 그 행을 쓰는 요청은 연결을 시도하지 않고 바로 `remote_workspace_missing` 으로 거절한다. 이 표지는 메모리에만 있고 저장하지 않는다. 다시 시작하면 그 행은 아직 붙지 않은 행이고 Connect 가 켜진다.

### 저장된 원격 행을 쓰는 요청

IPC·CLI·plugin 요청이 연결되지 않은 저장된 원격 행을 쓰려 하면 그 행을 연결하고, 연결될 때까지 요청을 기다리게 한 뒤 이어서 실행한다(`src/app/auto_attach/row_use.rs`). 게이트(권한·rate limit)를 지난 뒤에 판정하므로 거절될 요청이 연결을 일으키지 않는다. 연결된 행의 요청은 그대로 실행한다.

- **연결을 일으키는 메서드**: `terminal.spawn`·`attach.agent_target`(workspace·pane·surface), 입력(`surface.send`·`send_key`·`send_combo`·`send_to`·`send_wait_idle`, `terminal.tell`), 원본 읽기(`surface.screen_text`·`read_since_mark`·`set_mark`), 구조 변경(`split`·`tab.create`·`tab.move`·`tab.close`·`pane.close`·`surface.close`·`surface.close_self`·`terminal.kill`·`image.open`·`markdown.navigate`), `git_viewer.query`, `file_handler.dispatch`. 대상은 요청의 workspace·pane·tab·surface 인자로 찾는다. 재시작 뒤 아직 붙지 않은 행에는 pane·tab·surface 가 없으므로 workspace 인자를 받는 메서드만 닿는다.
- **연결하지 않는 메서드**: 조회(`workspace.list`·`tree`·`surface.list`·`terminal.state` 등)와 원격 목록 조회(`remote.workspaces`·`remote.refusals`), 로컬 메타·훅·알림, 행 정보 변경(`workspace.update`·`workspace.move`), `remote.disconnect`, 이미 연결을 기다리는 `attach.agent_call`, 위 목록에 없는 메서드. 목록을 조회해도 행은 연결되지 않는다. 연결은 그 행을 쓰는 요청, 화면에 보이게 된 행, 아래 명시적 연결 요청만 일으킨다.
- **화면에 보이면 연결**: 아직 붙지 않은 행은 화면에 보이게 되면 요청 없이 바로 연결을 시작한다(재시작 뒤 복원된 활성 행, 창 포커스 전환, 이웃 행 선택). 위 "보이면 연결" 항목과 같다.
- **연결 대기**: 그 행을 쓰는 요청은 연결을 시작하고 아래 상한까지 기다린 뒤, 연결되면 같은 요청을 실행한 결과로, 연결하지 못하거나 거절되면 이유를 담은 오류로 답한다.
- **대기 상한**: 35초(`ROW_WAIT_LIMIT`, `attach.agent_call` 의 응답 상한과 같다). 연결을 기다리는 요청은 64개까지 둔다.
- **오류**: 연결하지 못하면 `-32000` 과 `data: {reason, workspace_id}` 로 답한다. plugin host-call 은 같은 코드와 메시지를 받는다(`data` 없음). `reason` 은 다음 중 하나다.
  - `window_parked`: 그 행을 가진 창이 닫혀 있다(parked engine). 연결하지 않고 바로 답한다. 창을 여는 것은 포커스를 바꾸는 동작이라 요청으로 하지 않는다.
  - `user_disconnected`: 사용자가 이 실행 중 그 행의 연결을 끊었다. 그 행을 쓰는 요청은 다시 붙이지 않는다. 아래 `remote.connect` 로 명시적으로 연결한 뒤 쓴다.
  - `remote_workspace_missing`(바로 답함): 이번 실행에서 원격 workspace 가 없다고 확인된 행이다(위 "다른 곳에서 닫힌 행"). 첫 연결 실패 뒤 다른 이유의 행은 요청이 한 번 더 연결을 시도하지만 이 행은 시도하지 않는다. `remote.attach` 도 같다.
  - `connect_timeout`: 상한 안에 연결되지 않았다.
  - 연결 실패 안내와 같은 이름: `self_instance`·`profile_missing`·`unresolved`·`remote_workspace_missing`·`occupied`·`mirror_workspace`.
  - `busy`: 연결을 기다리는 요청이 상한만큼 있다.
- 연결 뒤 대상 surface 가 원격에서 사라졌으면 다시 실행한 요청이 "없는 대상" 오류로 끝난다. 연결한 뒤에는 지금의 mirror 규칙을 따른다(예: mirror 를 대상으로 한 `terminal.spawn` 은 거절하고 원격 자식은 `attach.agent_target` → `attach.agent_call` 경로다).
- **같은 연결 대상의 `remote.attach`**: 프로필과 원격 workspace id 가 저장된 원격 행과 같으면 새 mirror 를 만들지 않고 그 행을 연결한다. 응답은 `{attaching, remote_workspace, workspace_id, saved_row: true, run}` 이다. 이미 연결된 행이면 `attaching: false` 와 성공으로 끝난 `run` 을, 이미 연결 중인 행에 합류해도 `run` 을 주며, 그 시도가 끝나면 같은 결과(연결되면 성공, 연결 끊김·첫 연결 실패면 그 이유로 실패)를 남긴다. 재연결 중인 행에 합류하면 재연결 구간이 끝날 때까지 기다린다. 손실 뒤 재동기화(Resync) 중인 행에 오면 그 재동기화에 합류해, 다시 살아나면 성공으로, 재동기화가 실패해 재연결 끝에 연결하지 못하면 그 이유로 실패로 끝난다. 위의 `window_parked`·`user_disconnected` 는 같은 오류로 답한다. 연결해도 그 행은 저장된 원격 행으로 남는다.
- **명시적 연결 요청 `remote.connect`**: `{workspace_id}`(저장된 원격 행의 로컬 id, CLI `tasty remote connect --workspace <id>`)로 그 행을 연결한다. 사용자가 끊은 행도 그 표지를 지우고 붙으며, 그 뒤 그 행을 쓰는 요청은 다시 보통 행처럼 처리된다. 응답은 `{attaching, workspace_id, run}` 이고 `remote.attach` 의 저장 행 합류와 같이 이미 연결된 행이면 `attaching: false` 와 성공으로 끝난 `run`, 연결 중인 행이면 그 시도에 합류한 `run`, 아니면 새 시도의 `run` 을 준다. 재연결 중인 행은 사용자의 연결 버튼처럼 새 재연결 구간을 시작한다. 바로 거절하는 경우는 `-32000` 과 `data.reason` 이 `not_a_saved_row`(저장된 원격 행이 아님: 로컬 workspace·에이전트 mirror·없는 id), `remote_workspace_missing`, `window_parked` 일 때다. 이 메서드 자체는 위 "연결을 일으키는 메서드" 목록에 들지 않아 `user_disconnected` 로 막히지 않는다. 포커스는 옮기지 않는다.
- **`remote.disconnect`**: 저장된 원격 행이면 아직 붙지 않은 행도 성공으로 답하고 진행 중인 시도를 취소한다. 행은 남고 다시 불러도 성공이다. 다시 붙이려면 `remote.connect` 를 쓴다. 같은 대상의 `remote.attach` 는 `user_disconnected` 로 거절된다.
- **`workspace.list`**: 저장된 원격 행에 `remote_state` 를 싣는다. 값은 `connected` · `connecting`(첫 연결 중) · `reconnecting` · `disconnected` · `failed`(첫 연결이 실패해 다시 열기를 기다림) · `not_connected` 이고, 저장된 행이 아니면 `null` 이다. 조회는 연결을 일으키지 않는다.

IPC `remote.attach` 등 에이전트가 붙인 저장되지 않은 mirror 는 재연결 대상이 아니라 끊기면 바로 정리된다. 상세: [`dev-guide/attach-behavior` "GUI 자동 재연결 스코프"](../../dev-guide/attach-behavior.md#gui-자동-재연결-스코프) / ["재연결 시 세션 상태 보존"](../../dev-guide/attach-behavior.md#재연결-시-세션-상태-보존).

### 원격 포트 발견 실패 진단

포트 발견(`ssh.rs::discover_remote_port`, auto fallback 체인 subcommand→file-unix→file-windows)이 전 단계 실패하면 `PortDiscoveryError` 로 원인을 6분류한다 — **원격 stderr 문자열 매칭에 의존하지 않는다**(원격 로케일에 따라 문자열이 달라져 신뢰 불가, 실측: 한국어 "그런 파일이나 디렉터리가 없습니다" / 영어 "No such file or directory"). 대신 ssh(1) exit code 로 로케일 독립적으로 판정한다:

- **SSH 연결 자체 실패**(exit 255, 시그널 종료, 로컬 ssh spawn 실패) — 네트워크/인증 문제.
- **원격 인스턴스 미실행**(그 외 비정상 종료 — 원격 명령까지는 도달했다는 확정 신호) — 관례 위치에 포트 파일이 없거나 `tasty port` 가 실패. 이 저장소가 다루는 attach 실패의 대다수를 차지하는 케이스.
- **원격 인스턴스 종료 확정**(subcommand 단계의 `tasty port` 가 종료 코드 3으로 끝남) — 원격 tasty 가 포트 파일 옆 `tasty.instance` 로 그 포트를 쓴 인스턴스가 끝났다고 판별했다(아래 "남은 포트 파일"). 추정이 아니라 원격의 판별이라 auto 체인은 여기서 멈춘다. 파일 모드로 넘어가면 같은 남은 파일을 판별 없이 읽어 죽은 포트를 돌려주기 때문이다. 셸 감지(`tool remote-profile detect`)는 이 응답을 subcommand 모드가 동작하는 것으로 본다. 이 판별이 없는 옛 원격 tasty 는 종료 코드 1로 끝나 위 "원격 인스턴스 미실행" 추정과 fallback 이 그대로 적용된다.
- **포트 파싱 실패**(exit 0, stdout 을 포트 숫자로 못 읽음) — 연결·명령은 성공했지만 출력이 이례적(원격 tasty 버전 불일치 등).
- **취소**(사용자가 조회를 끊음 — `tasty_ssh::SshCancel`) — 확정 사실이라 대표 에러 선택에서 최우선.
- **타임아웃**(상한 안에 아무 답도 오지 않음) — 무응답 호스트(패킷이 조용히 버려지는 IP·방화벽 DROP·꺼진 머신), 보이지 않는 프롬프트 대기, 또는 체인 전체 예산 소진. 아래 "연결 시도 상한" 참고. `SshConnectionFailed` 로 접지 않고 따로 두는 이유는 ① 사용자가 취할 조치가 다르고(도달성/회선 점검 vs 인증·호스트키 점검), ② 타임아웃 kill 도 시그널 종료라 접으면 원격발 시그널 종료와 뭉개지기 때문이다.

각 분류는 `lang/{en,ko,ja,zh-Hans}.toml` `[ssh.port_discovery]` 의 번역된 문구로만 노출된다 — 원격 raw stderr·내부 명령(`cat`/`type`)·포트 파일 경로는 에러의 `Display` 에 담기지 않고 생성 시점에 `tracing::debug!` 로만 로그된다(`PortDiscoveryError::detail()`).
Auto 체인이 전 단계 실패하면 가장 확정적인 분류(취소 > 인스턴스 종료 확정 > 타임아웃 > SSH 연결 실패 > 인스턴스 미실행 > 파싱 실패 순)를 대표 에러로 고른다 — 한 단계가 무응답이면 다른 단계의 "연결 실패" 는 앞 단계에서 시간을 써서 남은 시간이 부족해진 결과일 수 있어 대표로 삼으면 오도한다 — 마지막 단계 에러만 남기면 정보량이 가장 적은 사유가 노출되던 문제를 막는다.
이 분류는 `ssh::discover_remote_port`/`remote_browse::resolve_endpoint` 를 공유 소비하는 모든 경로(GUI 원격 워크스페이스 추가 팝업, `tasty remote check`/`remote workspaces`/`tool attach`, IPC `remote.workspaces`/`remote.attach`, 자동 재연결)에 동일하게 적용된다.

### 연결 시도 상한 (no-hang)

포트 발견은 **무기한 블록하지 않는다.** 상한이 없으면 무응답 호스트에서 연결 수립이 OS 기본 SYN 재시도(리눅스 `tcp_syn_retries=6` ≈ 127초)에 맡겨지고, auto 체인은 그 대기를 3배로 곱한다. 상수는 `crates/tasty-ssh/src/lib.rs` 에 `pub const` 로 노출되며(소비자가 진행 표시/문구를 같은 값에 맞출 수 있게), 세 겹이 각각 다른 구간을 덮는다 . TCP 연결만 제한하면 인증·명령·프롬프트 대기가 남고, 단계별 상한만 두면 auto 체인의 전체 대기가 늘어나므로 연결·단계·전체 예산을 각각 둔다.

| 상수 | 값 | 덮는 구간 |
|------|-----|-----------|
| `SSH_CONNECT_TIMEOUT` | 10초 | ssh(1) `-o ConnectTimeout` — 연결 수립(TCP + 배너). ssh 가 홉마다 적용하므로 ProxyJump 다단도 홉별로 이 상한을 받는다. |
| `PORT_DISCOVERY_STEP_TIMEOUT` | 20초 | ssh 자식 프로세스 1개의 프로세스 레벨 상한. `ConnectTimeout` 이 못 보는 구간(인증 핸드셰이크, 원격 명령 실행, `BatchMode=no` 로 뜬 프롬프트 대기)까지. 넘기면 kill + wait 로 좀비 없이 거둔다. |
| `PORT_DISCOVERY_TOTAL_TIMEOUT` | 45초 | `discover_remote_port` / `detect_port_mode` **호출 1회 전체**. 각 단계는 `min(단계 상한, 남은 예산)` 만 받고 예산이 소진되면 남은 단계는 ssh 를 띄우지 않고 즉시 타임아웃 — auto 체인(3단계)이나 명시 `port_file`(`cat`→`type` 2회)에서 총 대기가 단계 수만큼 곱해지는 것을 막는다. |

무응답 호스트 실측: auto 체인 ~30초(3 × 연결 상한), 최악 45초. 상한은 **포트 발견/감지 경로를 공유하는 모든 소비자**에 동시에 적용된다 — GUI 원격 워크스페이스 추가 팝업, 도구 메뉴 > Remote connections 재감지, `tasty remote workspaces`/`remote check`/`tool attach`/`tool remote-profile detect`, IPC `remote.workspaces`/`remote.attach`/`remote.profile.detect`, 그리고 저장된 원격 행의 연결·재연결 워커.

**사용자 지정을 기본값보다 우선한다**: 프로필 `extra_options` 에 `ConnectTimeout=<초>` 를 직접 넣으면 그 값이 적용된다(ssh(1) 은 같은 키의 **먼저 나온 값**을 쓰고, tasty 는 기본값을 `extra_options` 뒤에 붙인다). 느린 회선/다단 ProxyJump 에서 상향하는 수단이다. 전체 예산(45초)은 프로필로 조정하지 않는다.

### 무응답 끊김 시간

상대의 프레임을 일정 시간 받지 못하면 연결을 끊는다. 시간은 `config.toml` 의 `[remote_attach] heartbeat_timeout_secs` 로 정한다. 기본값은 30초, 범위는 10~600초이며 범위를 벗어난 파일 값은 30으로 되돌린다. 인터넷 경유처럼 짧은 정체가 있는 회선은 값을 늘린다.

- 새 연결부터 적용한다. 재시작은 필요 없고 이미 열린 연결은 그대로다. 파일을 직접 고치면 다음 시작 때 읽는다.
- 서버와 GUI client 가 각자 자기 설정을 쓰고 협상하지 않는다. 끊는 쪽은 프레임을 받지 못하는 프로세스이고 그 프로세스의 값을 쓴다. 설정을 읽지 않는 CLI attach 쪽은 30초다.
- 설정 창의 General › Remote attach 서브탭(`src/view/settings/ui/tabs/remote_attach.rs`)에서 `Disconnect after` 로 바꿀 수 있다. 같은 서브탭의 `Keep reconnecting for` 는 [재연결 시간 창](#저장된-원격-행의-연결과-재연결)이다. 범위를 벗어난 입력은 확정할 때 경계값으로 맞춘다. 디자인↔본체 매핑은 [design-gallery-mapping](../../design/systems/design-gallery-mapping.md) 의 Remote attach 절.

세부 계약은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#무응답-제한-설정)에 있다.

### 남은 포트 파일

인스턴스가 비정상 종료하면 `tasty.port` 와 `tasty.instance` 가 남는다. `tasty port` 는 포트 파일과 같은 폴더의 `tasty.instance` 를 읽어, 그 기록이 같은 포트를 적었는데 기록한 PID 가 없거나 시작 시각이 다른 프로세스(PID 재사용)면 포트를 출력하지 않는다. stderr 에 포트 파일이 남은 것이라고 알리고 종료 코드 3(`tasty_ssh::PORT_COMMAND_EXITED_INSTANCE_CODE`)으로 끝난다. 판별은 두 번째 실행이 쓰는 것과 같은 `tasty_ipc::instance_file` 이다.

- 인스턴스 파일이 없거나(옛 버전, headless 실행, 다른 폴더를 가리키는 `--port-file`), 읽지 못하거나, 다른 포트를 적었거나, 아직 포트를 적지 않았으면 판별하지 않고 지금처럼 포트를 출력한다.
- 원격의 `file-unix`·`file-windows` 모드와 프로필의 명시 `port_file` 은 `tasty port` 를 거치지 않아 이 판별이 없다. 그 경우는 아래 "원격 포트의 리스너 없음" 이 알린다.
- 일반 CLI 요청은 연결을 그대로 시도한다. 연결이 실패한 뒤에만 같은 판별을 해, 끝난 인스턴스의 포트 파일이면 "Is tasty running?" 대신 포트 파일이 남은 것이라고 알린다.

### 원격 포트의 리스너 없음

`ssh -L` 은 로컬 리스너를 먼저 열기 때문에 원격 포트에 받을 프로세스가 없어도 터널 수립은 성공한다. 이 경우 터널을 지난 첫 요청에서 ssh 가 `channel N: open failed: connect failed: …` 를 stderr 에 쓰고 로컬 연결을 닫는다. 터널은 ssh stderr 를 사용자 stderr 로 그대로 넘기면서 이 줄을 표시하고, 요청이 응답 없이 끊긴 연결(EOF·재설정·중단)로 실패했고 이 줄이 나왔으면 "원격 포트에 리스너가 없다(원격 tasty 가 실행 중이 아니거나 포트 파일이 오래됐다)" 로 알린다(`ssh.tunnel.listener_absent`, `remote check` 는 `cli.remote_check.listener_absent`).

- 판정에 쓰는 `open failed: connect failed` 는 ssh 클라이언트가 거절 사유 코드를 바꾼 고정 문자열이라 원격 OS·로케일과 무관하다. 그 뒤의 사유 문구는 원격 OS 가 만들어 보지 않는다. 판정은 실패한 뒤에만 하며 줄을 최대 0.5초 기다린다. 성공 경로의 지연은 없다.
- 리스너는 있지만 응답이 IPC 가 아니면(다른 서비스) 이 줄이 없어 기존 IPC 실패 안내가 나온다.
- 적용 경로: `tasty remote check`·`remote workspaces`·`remote new-workspace`, GUI 를 거치지 않는 `tasty remote attach`·`tasty tool attach`, IPC `remote.attach` 의 새 workspace 생성(`--into-gui --new-workspace`)과 기존 workspace 연결(`--into-gui --workspace`).
- IPC `remote.attach` 는 터널 뒤 첫 요청이 워커의 자기 인스턴스 검사(`system.info`)라 두 경로 모두 그 실패를 먼저 분류한다. 새 workspace 는 이어지는 `workspace.create` 실패도, 기존 workspace 는 연결 작업자의 첫 attach 연결 실패도 분류한다. 연결 작업자는 터널을 연결 안으로 넘기므로 넘기기 전에 판정 핸들(`tasty_ssh::ListenerVerdict`)을 받아 둔다.
- 터널이 준비 전에 끝나면(인증 거절 등) ssh 가 마지막에 쓴 stderr 를 넘기는 스레드를 최대 0.5초 기다린 뒤 오류를 돌려줘, 원인 줄이 오류보다 먼저 나온다. 파이프를 물려받은 다른 프로세스(ProxyCommand 등)가 남아 그동안 닫히지 않으면 기다리지 않고 돌려준다.

### 원격 생존 확인

`tasty remote check --ssh|--profile` — 원격 인스턴스가 *지금 살아있는지* 단발 판정. 포트 발견만으론 stale 포트 파일을 오판할 수 있어, 터널 수립 후 가벼운 IPC(`system.info`) 1회 응답까지 확인해야 alive(exit 0). 실패(거부/EOF/타임아웃)는 dead(exit≠0). 원격 `tasty port` 가 남은 포트 파일을 판별하면 터널을 열기 전에 실패하고, 터널을 연 뒤 리스너가 없으면 위 "원격 포트의 리스너 없음" 안내로 실패한다. 세 단계 모두 상한이 있어 무응답 호스트에서도 단발 판정이 성립한다 — ① 포트 발견은 위 "연결 시도 상한", ② 터널 ready-probe 5초, ③ IPC 프로브 5초(`remote_browse::PROBE_TIMEOUT`).

### 원격 워크스페이스 브라우징 (Browse)

`remote attach` 가 대상 workspace id 를 **미리 알아야** 동작하는 것과 달리, 브라우징은 그 id 를 **발견**한다 — attach 프로필/ssh 대상에 붙어 원격 인스턴스의 워크스페이스 목록(각 `id`/`name`/`pane_count`/`busy_count`/`attached`)을 받아온다. 흐름: 접속 스펙 resolve → (SSH 터널 or `127.0.0.1:PORT` loopback 직결) → 그 포트로 `workspace.list` + `attach.list` **2회 IPC** → workspace 단위 lock 을 join 해 `attached`(타 client 점유 여부)/`holder` 를 채운다(서버측 변경 0). 순수 조회라 로컬 사용자 상태(focus/닫은항목/선택)에 닿지 않는다([포커스 독립성](../../design/policies/focus.md#cliipc-포커스-독립-원칙)).

이 능력은 **CLI(`remote workspaces`)와 IPC method(`remote.workspaces`) 양면**으로 노출된다(에이전트 기능 기준 — 에이전트가 CLI 없이 소켓만으로도 브라우징 가능). 둘 다 동일한 코어(`tasty_remote::browse`)를 공유하며, 블로킹 SSH I/O 는 호스트 IPC 경로에서 **워커 스레드**로 돌려 이벤트루프를 막지 않는다. RA02 원격 추가 팝업의 우측 목록이 이 출력을 데이터 소스로 소비한다. IPC `remote.workspaces`가 실패하면 -32050으로 답하고, 문구는 `remote browse failed: <바깥 문맥>: <원격의 원인>` 처럼 원인 체인까지 담는다(`remote.attach` 생성 실패와 같은 형식).

### 원격 요청의 세션 토큰

세션 토큰(`TASTY_SESSION_TOKEN`)은 그 프로세스를 띄운 로컬 Tasty의 `session.issue`가 발급하므로 다른 인스턴스는 모른다. 원격 인스턴스로 보내는 IPC 요청(브라우징의 `workspace.list`·`attach.list`, 원격 워크스페이스 생성의 `workspace.create`, `remote check`의 `system.info`, SSH 터널로 붙기 전 이 인스턴스 자신인지 묻는 `system.info`)에는 세션 토큰을 싣지 않는다. Tasty 안의 터미널에서 띄운 GUI·CLI가 부모 Tasty의 토큰을 상속해도 원격은 토큰 없는 요청으로 받는다. 원격 쪽 권한 경계는 SSH 접속이다. attach 채널도 토큰을 싣지 않는다(아래 비-목표). 원격·GUI 경로는 로컬 세션 토큰을 보내지 않는다. GUI의 원격 요청은 사용자 조작이므로 `--ssh 127.0.0.1:<port>` loopback 직결로 같은 머신의 인스턴스(토큰을 발급한 부모 인스턴스 포함)를 대상으로 해도 토큰 없는 요청으로 보낸다.

### 원격 attach (IPC — focus 중립)

IPC method `remote.attach` { `remote_workspace`, `profile?`/`ssh?` }(CLI `tasty remote attach --into-gui --workspace <id> --ssh|--profile`) — 권한 토큰 없이 세션 토큰 호출자도 부른다. 선택한 원격 워크스페이스를 **로컬 mirror 로 attach**(호스트가 워커 스레드에서 SSH 터널을 세우고 mirror 를 재구성).
**focus 중립**이 핵심: 이 IPC/에이전트 경로는 mirror workspace 를 *조용히 생성만* 하고 focus 를 그 ws 로 옮기지 않는다(`active_workspace` 불변).
새 mirror 로의 focus 이동은 **사용자 입력 경로 전용 별도 단계**(RA02 팝업에서 사용자가 확정할 때)이며, release IPC 에는 focus 변경 API 가 없다([포커스 정책](../../design/policies/focus.md)).
회신은 즉시 `{attaching:true}`(`attempt`·`run` 포함) — 연결 결과는 [`run.result`](#연결-결과-조회)로 읽고, mirror 는 비동기로 나타나므로 `tasty list workspaces`(세션 토큰이면 `surface.read` 필요)로 확인한다: mirror 워크스페이스는 행에 `[mirror]` 가 붙고(`remote-ws (id:7) [mirror] (1 panes)`), `workspace.list` IPC 응답에는 `mirror: true` 로 실린다. 저장된 원격 행이면 `remote_row`(`profile`·`remote_workspace`)가 함께 실리고, 아니면 `null` 이다.

호출자가 알아야 할 동작:

- **받는 창**: 마지막으로 포커스된 창이다. 호출자는 창을 고를 수 없다. 포커스가 앱 밖으로 나가도 이 창은 그대로다. 받을 창이 하나도 없으면(macOS 백그라운드 최소화 등으로 engine 이 모두 parked 일 때) `-32602`(`no engine is available to receive a mirror`)로 응답한다. `attach.into_gui` 는 같은 상태에서 첫 parked engine 에 접수해 `{queued:true}` 로 응답한 뒤 run 이 `mirror creation has no originating View` 로 실패한다. mirror 가 생기지 않는 것은 같고 실패를 받는 자리만 다르다.
- **앞선 시도 교체**: 같은 대상(`profile`·`ssh`·`remote_workspace`)의 수동 요청이 아직 끝나지 않았으면 새 요청이 앞선 요청을 취소하고, 앞선 요청의 run 은 `cancelled` 로 끝난다. 연결을 마친 mirror 는 건드리지 않는다. GUI 원격 추가 팝업의 연결은 이 수동 요청 표지가 없어 교체 대상이 아니다. 이미 점유된 원격 workspace 에 요청하면 run 이 `workspace attach rejected: already_attached` 로 실패한다.
- **실패**: 기존 workspace 는 접수 응답 뒤에 실패하므로 `run.result` 의 `state: failed` 와 `result` 로 받는다(닫힌 포트, 없는 프로필, 없는 원격 workspace, SSH 인증 실패). `new_workspace:true` 는 오류 응답 `-32050` 으로 받는다. 어느 쪽이든 mirror 행과 터널이 남지 않는다.
- **SSH**: ssh 는 호출자가 아니라 호스트 프로세스에서 호스트의 환경·agent 로 실행되고 stdin 은 닫혀 있다. 호출자에게 프롬프트가 전달되는 경로는 없다.
- **정리**: 에이전트가 붙인 mirror 는 저장된 행이 아니라서 끊기면 재연결 없이 바로 정리된다. 원격 workspace 를 남기고 mirror 만 떼려면 `tasty remote disconnect --workspace <mirror id>`(IPC `remote.disconnect`)를 쓴다. 대상은 원격 id 가 아니라 이쪽 mirror workspace id 이고, 응답의 `remote_workspace` 로 나중에 `remote attach --into-gui --workspace <원격 id>` 를 다시 부른다. mirror 워크스페이스를 `close workspace` 로 닫을 수는 없다. 마지막 surface 를 닫으면(`tasty close surface`, `surface.write` 필요) 원격 workspace 가 함께 닫히고 mirror 도 사라진다.
- **plugin 호스트 호출**: 메서드 표는 plugin 을 막지 않지만 App 층 처리는 외부 IPC 전용이다. plugin 이 호스트 호출로 `remote.attach`·`remote.refusals`·`remote.workspaces`·`remote.disconnect` 를 부르면 권한 거절 없이 `-32017` 을 받는다. `attach.into_gui` 만 호스트 호출을 처리한다. plugin 프로세스가 외부 IPC 로 부르면 토큰 없이 접속하므로 권한 표와 무관하게 Local 로 처리된다.

### 원격 워크스페이스 추가 팝업 (GUI picker — 사용자 경로)

위 브라우징/attach 능력을 **로컬 사용자가 직접 조작**하는 GUI 표면.
사이드바에서 **카테고리 헤더 우클릭(카테고리 on) / 새 워크스페이스(+) 버튼 우클릭 · 빈 배경 우클릭(그룹·플랫 모드 공통) → "원격 워크스페이스 추가"** 로 연다(`remote_attach` headless 팝업, 680×460 2-pane).
워크스페이스 카드 우클릭에는 없다(카테고리 ON/OFF 에 따라 노출 위치가 갈리도록 재배치 — [`sidebar/screens/sidebar.md`](../sidebar/index.md#화면) 참고).
좌측은 `tasty-attach` 프로필 목록(remote_tool 이 편집하는 같은 스토어를 **소비만** 함), 우측은 선택 프로필의 원격 워크스페이스를 **4상태**(initial / connecting / error+retry / loaded[+empty])로 표시한다.
조회 워커와 SSH 터널은 `tasty_remote::browser`의 세션이 소유한다. 팝업은 요청 ID와 표시 값만 보관하고, App이 원 engine·View를 확인한 결과를 전달받는다.
이미 타 client 가 점유한 원격 ws 는 Tag `attached` 의 `in use` 표시 + 선택 불가(중복 mirror 방지). inactive 프로필은 Tag `warning` 이다. 두 칩은 자체 채움이 있어 선택 행에서도 바뀌지 않는다. 선택 행의 상태 점은 `status-dot-*-active` 색이다.

**Connect 확정 = 사용자 동작 → focus 이동**: 원격 workspace를 골라 Connect하면 팝업은 `BrowserRequest::Connect`를 사용자 의도로 전달한다. App은 원 browse의 engine·window·View identity를 확인하고, 그 대상의 mirror 설치 요청에 사용자 활성화 의도를 고정한다. Remote가 보관한 조회 터널은 연결에 재사용된다. 설치가 완료되고 원 View와 선택 generation이 여전히 유효할 때 새 mirror를 선택하며, IPC `remote.attach`의 선택 중립 경로와 구분한다. 자기 인스턴스 연결은 공용 `dispatch_attach` 입구에서 거절한다.

**같은 연결 대상의 저장된 원격 행**: 고른 프로필과 원격 workspace 가 저장된 원격 행과 같으면 새 행을 만들지 않는다. 형상이 남은 행은 그 행의 재연결로 붙인다. 형상이 없는 행은 팝업 연결이 그 행을 넘겨받는다(`take_over_saved_row`). 진행 중인 자동 첫 연결을 취소하고 행 토큰을 바꿔 늦은 결과를 낡은 것으로 만든 뒤, 팝업 연결을 그 행의 진행 중 시도로 둔다. 연결이 끝날 때까지 행은 연결 중(`connecting`)이고, 그 행에 보류한 요청과 합류한 `remote.attach` run 은 팝업 연결의 결과로 끝난다. 설치 대기열에 들기 전에 실패하면 첫 연결 실패로 끝난다.

**조회 중(connecting) 사용자 조작 + 정리 계약**: Remote의 `poll_browsers`가 조회 시작 뒤 **20초**(`BROWSE_DEADLINE`) 경과를 관측하면 원 attempt에 취소를 요청하고 timeout 실패 값으로 전환한다. App과 팝업은 원 View·요청 ID가 일치하는 결과만 error 상태(+ Retry)로 표시한다. UI의 경과 표시도 같은 기한 상수를 사용하지만 취소와 worker 수명의 소유자는 Remote다.
그 전에 사용자가 직접 중단할 수 있다. 조회 중 footer의 "중단"은 팝업을 닫지 않고 원 요청을 취소해 initial로 돌아간다. 헤더 × / Esc, 다른 프로필 선택도 해당 요청의 취소를 보낸다.
Remote는 원 attempt의 SSH 취소와 터널 회수를 요청하고 worker의 실제 종료·join을 관측한다. 취소 요청을 자식 프로세스 회수 완료로 간주하지 않는다. 늦은 결과는 원 attempt/session 검사로 폐기하고, 함께 도착한 터널도 Remote의 회수 경로로 넘긴다([터널 수명](../../dev-guide/attach-behavior.md)).

**"+ 새 워크스페이스" 행 — 원격에 만들어서 붙는 경로**: loaded 목록의 **첫 행**은 원격에 이미 있는 워크스페이스가 아니라 `+ 새 워크스페이스` 다. 이 행을 고르고 확정하면 조회에 쓴 같은 터널로 원격에 `workspace.create` 를 1회 보내고, 그 응답의 ws id 를 **기존 Connect 와 똑같은 attach 지점**으로 넘긴다. 이름/cwd 를 묻는 UI 는 없다 — params 를 빈 객체로 보내 원격의 기본값(`type`=terminal, 기본 이름, 원격 자기 활성 surface 의 cwd 상속)을 쓴다. 클라이언트는 원격 파일시스템 경로를 모르므로 cwd 를 지어내지 않는다(명시 지정은 IPC/CLI 쪽 몫).

- **버튼이 아니라 목록 행**이다. 이웃 ws 행과 같은 select-then-confirm 을 따르고 확정은 footer 가 한다 — 그때 primary 라벨이 `Connect` → `Create & connect` 로 바뀐다. 목록 안의 행인데 혼자만 클릭 즉시 실행되면 그 자체가 상호작용 불일치이고, 원격을 **변경하는** 동작 직전의 되돌릴 수 있는 순간도 사라진다.
- **구분은 세 채널 동시** — `plus` 글리프 · accent 라벨 · 행 아래 1px 구분선. 색 하나로만 구분하지 않는다. 글리프는 ws 행의 status-dot 과 **같은 폭 슬롯** 안에서 center 되어, 이름 열의 좌측 정렬선이 아래 행들과 픽셀 동일하다.
- **키보드**: 목록이 뜬 뒤 ↑/↓ 가 새 행을 1번으로 삼아 행을 옮기고, 이미 attach 된(고를 수 없는) 행은 건너뛴다. 양 끝에서는 반대쪽 끝으로 돈다(convert·preset 목록 popup 과 같은 관례). 선택이 없을 때 ↓ 는 첫 행, ↑ 는 마지막 행을 고르고, 옮긴 행이 보이도록 스크롤한다. Enter 는 footer 확정과 같다. 생성 중에는 키도 받지 않는다. 목록 안 탐색 키라 `KeybindingSettings` 대상이 아니다.
- **원격 ws 가 0개여도 이 행은 나온다.** loaded 렌더 경로가 하나라서, 목록이 비면 caps 헤더 + 이 행 하나 + muted 한 줄로 degrade 한다(전용 center-state 는 없다). 그때는 이 행이 **미리 선택**돼 있어 pane 이 뜬 순간부터 확정 버튼이 살아 있다 — 빈 원격이 막다른 길이 아니다.
- **생성 왕복 중 / 실패는 행 안에서** 표현한다. 팝업의 `start_create`는 생성 중이거나 준비되지 않았으면 요청을 추가하지 않고, 준비된 요청 ID로 `BrowserRequest::Create`를 큐에 넣는다. App은 원 engine·window·View identity를 확인한 뒤 Remote에 실행을 맡긴다. 터널과 진행 중 attempt는 팝업이 아니라 `tasty_remote::browser`의 세션이 소유하며, 생성 워커에는 포트와 원 attempt token을 넘긴다. Remote도 진행 중 attempt가 있으면 새 생성을 오류로 거절한다.
  생성 대기는 Remote의 `poll_browsers`가 10초 경과 시 취소하고 실패 값으로 돌린다. 소켓의 5초 `PROBE_TIMEOUT`은 개별 read/write 제한이며, 연결과 여러 부분 읽기를 합친 전체 호출의 절대 상한은 아니다. 취소 요청을 worker 종료·회수 완료로 간주하지 않는다.
  생성 중에는 행의 글리프가 스피너로, 라벨이 "워크스페이스 만드는 중…"으로 바뀌고 확정 버튼은 비활성이다. 아래 목록은 이름을 `text-muted`로 읽을 수 있게 두고 선택만 막는다. App이 원 View를 확인하고 팝업이 같은 요청 ID인지 확인한 결과만 표시한다. 실패하면 행 하단에 원격 메시지와 "다시 시도"가 붙고, 응답에 유효한 워크스페이스 ID가 없으면 `remote_attach.create_failed_generic`로 안내한다. **팝업은 열린 채 기존 목록을 유지한다.**
  실행·시간 제한은 [`browser.rs`](../../../crates/tasty-remote/src/browser.rs)와 [`browse.rs`](../../../crates/tasty-remote/src/browse.rs), 원 View 전달은 [`remote_browser.rs`](../../../src/app/remote_browser.rs), 요청·표시 값은 [`remote_attach.rs`](../../../src/adapters/ui/popup/remote_attach.rs)가 담당한다.

**알려진 제약 (이 경로 한정)**:

- **생성은 성공했는데 attach 가 실패하면 원격에 워크스페이스가 남는다.** 정리 수단은 원격의 `workspace.close` IPC, 또는 mirror 안에서 마지막 surface 를 닫아 원격 ws 를 purge 시키는 기존 경로다(아래 "역반영 대신 강제 detach").
- **레이아웃을 저장하는 원격에서는 그 워크스페이스가 재시작 후에도 남는다.** 원격이 만든 것은 mirror 가 아닌 일반 워크스페이스라 원격의 슬롯 파일에 영속된다(비영속 제외 대상은 로컬 mirror 뿐 — 아래 절). 즉 이 기능은 **원격의 영속 상태를 늘린다**: 무심코 여러 번 확정하면 원격에 워크스페이스가 계속 쌓인다.
- self(loopback) attach 거절은 그대로 적용된다 — 새 ws 를 만들었더라도 대상 포트가 자기 자신이면 attach 되지 않는다(생성만 되고 끝). debug 와 release 모두 같은 사전 검사를 거친다.

### 원격 자식 에이전트

mirror 를 `--workspace`(숫자 id)나 `--pane` 으로 지정한 `tasty claude spawn`·`tasty codex spawn` 은 자식을 서버에서 서버의 provider 설치·설정·로그인으로 실행한다. client 에서는 그 자식의 mirror surface 가 부모 아래 자식으로 등록되어 `tell`·`children`·`state`·`kill` 을 로컬 자식처럼 쓴다.

- 서버 훅이 보고한 완료는 기존 attach holder 연결로 client 에 돌아와 부모의 완료 로그·Codex 대화에 기록된다. 훅에는 부모 주소나 비밀이 없고, 별도 서버·포트를 열지 않는다.
- 서버의 다른 사용자·도구는 점유 중인 그 자식에 입력하거나 그 자식을 닫을 수 없다. 원격 출처 표지를 꾸민 일반 요청은 host 가 거절한다.
- 연결이 끊긴 동안 자식의 상태는 `unknown` 으로 보이고, 같은 서버 프로세스에 다시 붙으면 쌓인 완료가 한 번 기록된다. 결과를 확정할 수 없는 지시는 성공이나 실패로 단정하지 않고 결과불명으로 답한다. 서버 프로세스가 다시 시작하면 그 전 자식의 결과는 이어지지 않는다.
- `--cwd`·`--profile-file` 은 서버 기준 절대 경로만 받는다. 이 기능이 없는 구 서버의 mirror 에는 자식을 만들지 않고 오류로 끝난다.

전달·출처·통지 계약은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#원격-자식-에이전트-ipcstreamagent-children), provider 별 동작은 [원격 attach 자식](../../plugins/remote-attach-children.md)에 있다.

### 저장된 원격 행과 mirror 내용

원격 attach의 mirror 내용(pane·tab·surface 형상)은 연결 세션의 projection이며 로컬 journal 구조에 포함되지 않는다. journal snapshot과 호환 export의 로컬 canonical 모델은 mirror를 일반 workspace로 저장하거나 복원하지 않는다.

사용자가 원격 연결 팝업에서 프로필을 골라 붙인 workspace는 **저장된 원격 행**이 된다. 행 하나는 journal의 `remote_rows` 항목(이벤트 `remote_row.set`·`remote_row.removed`)으로 저장되며, 값은 이름·부제·설명·카테고리·자리와 연결 대상(attach 프로필 이름, 원격 workspace id)이다. 자리는 사이드바에서 바로 앞에 있는 로컬 workspace(`after`, 없으면 맨 앞)와 저장 행 가운데의 순번(`rank`)이다. 다른 저장 행이나 에이전트 mirror가 생기거나 없어져도 자리는 그대로고, 앞 로컬 workspace가 없어지면(닫기, 마지막 pane을 다른 workspace로 옮긴 교체 이동) 그 앞에 남은 가장 가까운 로컬 workspace 뒤로 바뀐다. 저장 행 하나를 지우면 남은 행의 순번을 화면 순서대로 빈칸 없이 다시 매겨, 뒤에 더한 행이 다시 시작한 뒤에도 뒤에 돌아온다. 이름·설명·카테고리를 바꾸거나 행을 옮기면 같은 항목을 고쳐 저장하고, 행이 닫히면 항목을 지운다. 원격에서 온 이름과 카테고리는 저장된 행의 값을 덮지 않는다. 없는 카테고리를 가리키거나 그 카테고리를 지우면 기본 카테고리로 저장한다. 다시 시작하면 저장된 행이 형상 없는 mirror 행으로 같은 위치에 돌아오고, 마지막으로 선택한 workspace가 그 행이었다면 그 행이 다시 선택된다. 원격 행은 로컬 구조 비교(`DIGEST_EXCLUDED`)에서 뺀다.

IPC `remote.attach`·`attach.into_gui`처럼 에이전트가 붙인 mirror는 저장하지 않는다. 팝업의 선택 프로필·목록·요청 식별자는 View의 비영속 상태다.

### 창 없는 상태(parked)에서의 세션 수명

attach 세션의 수명은 **창(window)이 아니라 engine 에 매인다.** 마지막 창을 닫거나(macOS 는 최소화도) 창은 사라지지만 engine 은 `App.engines` 에 parked 관계로 그대로 살아 있고([multi-window](../../architecture/multi-window.md), [App·Engine·View 상태 소유권](../../dev-guide/app-state-ownership.md) — parked engine 은 레이아웃 슬롯 점유를 유지한다), mirror 워크스페이스와 그 mirror 터미널도 그 engine 안에 남는다. 따라서:

- **parking 만으로는 세션이 끊기지 않는다.** 고아 판정(`detach_orphaned_mirror_sessions`)이 묻는 것은 "창이 있는가"가 아니라 **"그 mirror 워크스페이스를 들고 있는 engine 이 살아 있는가"** 다 — 창 있는 engine 과 parked engine 을 함께 본다. 창 유무로 판정하면 사용자가 창을 최소화했을 뿐인데 원격에 `Detach` 가 나가 점유가 조용히 풀린다.
- **설치를 기다리던 첫 연결은 시작한 창이 park 되면 버린다.** 연결 준비가 끝나 설치를 기다리던 첫 연결(재연결이 아닌 시도)은 시작한 창과 View 가 그대로일 때만 설치한다. 재연결과 Resync 연결의 설치 대기는 창 대신 그 mirror 워크스페이스가 남아 있고 세션 연결 세대가 그대로인지로 판단하므로 park 만으로 버리지 않는다. 창이 없는 동안에는 설치를 미루고 창이 다시 생기면 이어서 설치한다. 재연결 시간 만료나 연결 오류 같은 다른 종료 조건은 park 중에도 그대로 적용된다. 첫 연결을 버릴 때 저장된 원격 행의 진행 중 시도 표지(`Remote.active`)는 행 연결 토큰이 그 시도의 것일 때만 지운다. 그사이 행을 닫거나 끊거나 원격 추가 팝업이 넘겨받았으면 그쪽이 표지를 정리하거나 새 시도가 가진다. 그 행은 다시 보이게 되거나 사용자가 다시 고를 때 연결할 행으로 남아, 창을 다시 열어도 연결 중으로 멈추지 않는다. 그 행을 기다리던 요청은 `window_parked` 로, 합류한 `remote.attach` run 은 같은 이유로 실패로 끝난다.
- **사용자가 mirror 워크스페이스를 직접 닫으면** 어느 engine 에도 그 워크스페이스가 없으므로 고아로 판정되어 기존대로 정리된다 — `Detach` 통지 → 원격 점유 해제 + 저장된 원격 행의 진행 중 시도 표지(`Remote.active`) 해제 + 터널 kill. 두 상황(창이 없어졌을 뿐 vs 워크스페이스가 없어짐)은 이 판정으로 구분된다.
- **정리는 parked engine 에도 동일하게 적용된다.** mirror 워크스페이스 행뿐 아니라 mirror 터미널·mirror busy 엔트리·mesh 프레임 캐시를 함께 걷어내고 `active_workspace` 인덱스를 클램프한다.
  걷어낸 뒤 그 engine 에 워크스페이스가 하나도 없으면(사용자가 로컬 워크스페이스를 다 닫고 mirror 만 남겼던 경우) 기본 터미널 워크스페이스를 다시 만들어 활성으로 삼는다 — 원격이 끊겼다고 사용자 창을 닫지 않고, 워크스페이스 0 개인 창이 다음 redraw 에서 죽지도 않는다(`MainViewState::recreate_workspace_if_empty`, 에이전트가 마지막 surface 를 닫은 경우와 같은 복구 — [Attach 메커니즘](../../dev-guide/attach-behavior.md)).
  판정과 정리의 순회 범위는 **같아야** 한다 — 판정이 살아 있다고 본 engine 을 정리가 못 찾으면, 그 engine 이 나중에 창에 다시 실릴 때 아무 데도 연결되지 않은 mirror 워크스페이스가 되살아난다.
- parked engine 에는 창이 없으므로 정리 시 toast 를 쌓지 않는다(토스트 수명이 wall-clock 기준이라 창 복원 시점엔 이미 만료된다).
- **도착하는 mirror 이벤트도 parked engine 에 즉시 적용된다**([Attach 메커니즘](../../dev-guide/attach-behavior.md)).
  `apply_attach_client_output` 은 적용 대상을 **창 있는 engine → parked engine** 순으로 찾고(`mirror_output_host`), 대상을 찾은 **뒤에야** reader 버퍼를 drain 한다.
  창이 없는 동안 도착한 `Data`/`Resize`/`StructuralDelta`/`Activity`/`Attention`/`Cwd`/`Mesh` 는 그 engine 의 mirror 터미널·매핑·트리에 도착 순서대로 반영되므로, 창 복원 시 mirror 는 이미 최신이고 `remote_to_local` 매핑도 desync 되지 않는다 — 로컬 PTY 출력이 parked engine 에서도 파싱되는 것과 같은 대칭이다.
  판정·정리·적용 세 순회의 범위는 **같다**.
  어느 engine 에도 워크스페이스가 없으면(고아) drain 하지 않고 두며, 같은 프레임의 고아 정리가 세션째 걷어낸다.
- parked engine 에 적용될 때는 창 표면이 필요한 부수효과만 생략한다 — toast 는 로그로 대체(위와 같은 이유), repaint 요청은 없음(복원 시 새 창이 그 engine 을 그대로 그린다). 상태 변경(터미널 grid·트리·매핑)은 창 유무와 무관하게 항상 적용된다.

## 인터페이스

- **AI Agent / 원격 (CLI)**:
  - `tasty tool attach <name> [SURFACE] [--workspace <id>] [옵션…]` — **tasty-attach 프로필**로 attach(profile-우선 편의 표면). `--list` = tasty-attach 목록만.
  - `tasty remote attach [SURFACE] --ssh|--profile [옵션…]` — 원격 surface attach. `--profile` 은 **tasty-attach kind**(docs/features/remote-profiles/index.md; ref/inline resolve).
  - `tasty remote attach --workspace <id> --ssh|--profile …` — 원격 workspace attach.
  - `tasty remote attach --into-gui --target-port <p> --workspace <ws>` — 호출자가 포워딩한 포트로 실행 GUI 에 mirror(IPC `attach.into_gui`). `--ssh`·`--profile` 을 함께 줘도 쓰지 않는다.
  - `tasty remote attach --into-gui --workspace <ws> --ssh|--profile` — GUI 가 터널을 직접 열어 mirror(IPC `remote.attach`, focus 중립). 접수 응답을 JSON 한 줄로 출력한 뒤 연결 결과(`run.result`)를 최대 90초 기다려 둘째 줄로 출력한다. 최종 상태가 `succeeded` 가 아니거나 상한 안에 끝나지 않으면 종료 코드 1 이다. 상한을 넘겨도 연결은 계속되며 첫 줄의 `run` 을 `tasty file-handler result --run <run>` 으로 다시 읽는다.
  - `tasty remote attach --into-gui --new-workspace [--name <n>] [--cwd <원격경로>] --ssh|--profile` — 원격에 워크스페이스를 만들고 mirror(IPC `remote.attach` 의 `new_workspace`). 생성 결과 한 줄을 출력하고 끝난다. run 이 없으므로 mirror 연결 결과는 `tasty list workspaces`(세션 토큰이면 `surface.read` 필요)로 확인한다.
  - `tasty remote check --ssh|--profile` — 원격 생존 확인(`--profile` = tasty-attach).
  - `tasty remote workspaces --ssh|--profile [--json]` — 원격 워크스페이스 목록 조회(browse). `--ssh 127.0.0.1:<port>` 로 loopback 직결(터널 없이 로컬 e2e).
  - `tasty remote new-workspace --ssh|--profile [--name <n>] [--cwd <원격경로>] [--json]` — 원격에 워크스페이스 생성(원격 mutate). 출력 id 를 `remote attach --workspace <id>` 에 넘기면 "만들고 그 자리에서 attach" 가 CLI 만으로 완성된다. `--cwd` 는 **원격 파일시스템** 기준이며 원격이 존재를 검증한다. 원격 active 는 불변(Agent origin).
  - `tasty remote refusals` — 이 인스턴스를 대상으로 해서 거절한 최근 attach 시도와, 연결하지 못한 저장된 원격 행의 안내(IPC `remote.refusals`). 읽기 전용.
  - `tasty remote disconnect --workspace <mirror id>` — 원격 workspace 와 그 프로세스는 남기고 이쪽 mirror 만 뗀다(IPC `remote.disconnect`, mirror 카드의 **워크스페이스 연결 끊기**와 같은 정리). 성공하면 `{disconnected:true, workspace_id, remote_workspace}` 를 출력한다. attach 세션이 없는 workspace 면 `-32602` 다. 저장된 원격 행은 `tasty remote connect` 로 다시 연결한다.
  - `tasty remote connect --workspace <local id>` — 저장된 원격 행을 명시적으로 연결한다(IPC `remote.connect`). 사용자가 끊은 행도 다시 붙는다. 접수 응답 한 줄을 출력한 뒤 `run.result` 로 최종 결과를 기다려 두 번째 줄로 출력하고, 최종 상태가 `succeeded` 가 아니거나 90초 안에 끝나지 않으면 종료 코드 1 이다. 바로 거절되면(`not_a_saved_row`·`remote_workspace_missing`·`window_parked`) 오류로 끝난다.
- **IPC (`attach.*`)**: `acquire`/`release`(stream 핸드셰이크), `into_gui`, `list`(점유 목록 조회), 원격 자식 에이전트용 `agent_target`·`agent_call`·`agent_commit`·`child_notice`·`child_notices`. 남의 점유를 푸는 메서드는 없다. 표 상세 → [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md#ipc-표면-attach).
- **IPC (`remote.*` — 원격 브라우징/생성/attach, IPC·CLI 양면 — [API 규약](../../dev-guide/api-conventions.md#ipc-메서드를-cli로-제공하는-기준))**: `remote.workspaces` { `profile?`/`ssh?` } → 원격 ws 목록(browse, 워커 스레드+지연 회신). `remote.attach` { `remote_workspace` | `new_workspace`, `profile?`/`ssh?`, `name?`, `cwd?` } → 원격 ws 를 로컬 mirror 로 attach(**focus 중립**: mirror 생성만, focus 이동 없음). `new_workspace:true` 면 원격에 워크스페이스를 먼저 만들고 그것을 attach 한다 — `remote_workspace` 와 상호배타이며, 이때만 **생성 완료까지 기다렸다 지연 회신**해 새 `remote_workspace` id 를 돌려준다(기존 ws attach 는 즉시 `{attaching:true}` 유지). 두 응답 모두 시도 번호 `attempt` 를 싣고, 기존 ws attach 는 최종 결과를 조회할 `run` 도 싣는다(위 연결 결과 조회). 자기 자신이 대상이면: loopback 표기는 이 응답 대신 동기 `-32602` 오류가 오고, SSH 경로는 `new_workspace:true` 만 생성 전 `-32602` 이며 기존 ws attach 는 `{attaching:true}` 응답 뒤 비동기 거절이 `remote.refusals`(`via_ssh: true`)에 남는다(위 self-attach 절의 IPC 계약 요약). `remote.refusals` {} → 자기 포트라서 연결하지 않은 최근 시도 목록(위 self-attach 절). `remote.disconnect` { `workspace_id` } → 그 mirror 의 연결만 끊는다(위 "mirror 닫기와 연결 끊기"). 저장되지 않은 mirror 의 두 번째 호출은 같은 끝 상태에서 `-32602` 로 답하고, 저장된 원격 행은 몇 번 불러도 성공이다(위 "저장된 원격 행을 쓰는 요청"). `remote.connect` { `workspace_id` } → 저장된 원격 행을 명시적으로 연결한다(위 같은 절). `remote.attach`·`remote.refusals`·`remote.disconnect`·`remote.connect` 는 권한 토큰을 요구하지 않는다([플러그인 권한](../../dev-guide/plugin-permissions.md#토큰-없이도-열려-있는-메서드)). CLI 와 코어 공유 — 조회는 `tasty_remote::browse`, 생성은 `tasty_remote::create`.
- **로컬 self attach**(별도 CLI 프로세스가 포트 파일로 로컬 인스턴스를 mirror): 사용자 mirror 조작 재현 성격이라 release 에 없음 — `tasty debug attach`(debug 빌드 전용, [`dev-guide/debug-ipc`](../../dev-guide/debug-ipc.md)). 같은 머신의 다른 인스턴스에 loopback 주소(`127.0.0.1:PORT`·`localhost:PORT`·`[::1]:PORT`)로 붙는 GUI 원격 연결 팝업·IPC `remote.attach`·저장된 원격 행의 연결은 release 에도 있다(아래 비-목표 절).
- **프로필**: `--profile`/`tool attach` 이 참조하는 tasty-attach 프로필(및 그것이 `ssh_ref` 로 참조하는 ssh 프로필)은 [remote-profiles](../remote-profiles/index.md) 이 관리.

## 원격 파일 다운로드 후 실행

원격(mirror)에서 열 핸들러가 없는 원격 파일 **하나**를 client 로 받아 client 의 로컬 핸들러 설정대로 여는 기능이다. 동기화가 아니다. 받은 파일을 고쳐도 원격 파일은 바뀌지 않는다. 기본 동작이 아니며 사용자가 명시적으로 고를 때만 실행한다.

- **대상**: 일반 파일 하나다. 폴더와 폴더를 가리키는 링크는 서버가 거절하고(`directory`), FIFO·장치·소켓도 거절한다(`not_regular`). 파일을 가리키는 링크는 링크 대상의 내용을 링크 이름으로 받는다.
- **크기 확인**: 1 MiB(1,048,576 바이트)까지는 묻지 않고 받는다. 1 바이트라도 넘으면 조각을 요청하기 전에 한 번 확인을 기다리고, 확인하면 다시 묻지 않고 끝까지 받는다.
- **받는 위치**: Tasty 데이터 홈 아래 `download-run/<실행별 폴더>/<원래 파일 이름>` 이다. 서버가 준 이름의 경로 구성 요소는 지운다. 사용자가 고르는 저장 위치·다운로드 목록·폴더 열기는 없다. 24 시간이 지난 실행별 폴더는 다음 다운로드를 시작할 때 지운다.
- **실행**: 다 받으면 그 파일을 사용자 요청으로 로컬 dispatch 한다. 이후는 로컬 핸들러 설정을 그대로 따르며 원격 전용 분기가 없다. 그 핸들러가 탭을 여는데 활성 workspace 가 mirror 이면 새 로컬 workspace(자동 이름)를 만들어 거기서 연다. 이 workspace 는 일반 workspace 이며 사용자가 닫는다. 받은 파일은 실행별 폴더 정리로 지워질 수 있다. 그 파일을 가리키던 탭은 재시작 뒤에도 복원되며 해당 kind 의 파일 읽기 실패 처리를 따른다(markdown 은 읽기 오류 상태로 연다).
- **취소·실패**: 확인 단계나 전송 중 취소, 연결 끊김·다른 연결로 교체, 전송 중 원격 파일 변경·교체(`changed`), 로컬 쓰기 실패에서는 받은 부분과 실행별 폴더를 지우고 핸들러를 실행하지 않는다.
- **옛 서버**: attach descriptor 에 `ipc.stream.file-download` 가 없는 서버에는 요청을 보내지 않고 시작 단계에서 실패한다.
- **진입점**: mirror 탐색기 더블클릭, mirror 터미널 경로 링크의 수식키 클릭과 우클릭 "연결 동작", 파일 열기 도구의 원격 선택, mirror origin 을 준 `file_handler.dispatch` 에서 원격에 열 수 있는 핸들러가 하나도 없으면 "다운로드 후 실행" 대화상자(`download_run`)를 띄운다. 사용자 요청과 사용자 입력을 증명하지 못한 plugin 중계 요청(PluginUnverified, 핸들러 picker 와 같은 범위)이 대상이고, 외부 IPC(Agent) 요청은 팝업 없이 warn 로그로 끝난다. 제목·원격 경로(줄 맨 앞에 [원격 위치 표시](#원격-위치-표시)의 호스트 표지, picker 와 같은 공용 위젯)·설명·"파일 하나만" 안내와 Cancel / Download and run 이 있고, Esc·바깥 클릭은 Cancel 과 같다. 연결이 이 기능을 알리지 않은 옛 서버면 Download and run 을 비활성으로 두고 그 이유를 항상 보이게 적는다(대화상자를 열 때마다 연결 상태를 다시 읽는다). 새 탭 picker 의 html 판정 토스트(`explorer.state.remote_open_unsupported`)는 그대로다. debug 빌드의 `debug.remote_download.*`(CLI `tasty debug remote-download start|confirm|cancel`)는 같은 다운로드를 대화상자 없이 시작·확인·취소한다([debug IPC](../../dev-guide/debug-ipc.md)).
- **큰 파일 확인**: 1 MiB 를 넘으면 `download_run_large` 팝업이 파일 이름과 크기(0.1 단위 올림, 1024 MiB 미만은 MiB, 이상은 GiB)를 보이고 Cancel / Download and run 을 묻는다. Esc·바깥 클릭은 Cancel 과 같아 아무것도 받지 않는다.
- **진행**: 조각을 받는 동안 `download_run_progress` 팝업이 업로드 진행 팝업과 같은 형상(파일 이름·진행 막대·받은 크기/전체·속도)에 "Downloading to open locally" 제목과 Stop download 버튼을 보인다. Esc·바깥 클릭으로 닫히지 않고 다운로드도 멈추지 않는다. Stop download 만 다운로드를 멈추며 받은 부분을 지우고 핸들러를 실행하지 않는다. 모든 다운로드가 끝나면 닫힌다.
- **실패 안내**: 사용자가 멈춘 경우를 빼고 실패하면 전송 실패 팝업(`transfer_error`)을 "Download failed" 제목으로 띄운다. 사유 칸은 원인마다 번역한 문장(`download_run.reason.*`, 옛 서버는 `download_run.dialog.unsupported_server`)을 UI 서체로 쓰고 Dismiss 만 둔다.
- **무결성**: 내용 해시는 주고받지 않는다. 서버가 조각마다 첫 응답의 식별 정보(크기·수정 시각·생성 시각·Unix (device, inode))와 지금 파일을 대조하고, client 가 조각의 위치·크기·끝 표시를 처음 받은 크기와 대조한다. 전송 중 손상은 SSH 전송의 무결성 보호에 맡긴다. 같은 크기·같은 시각으로 내용만 바뀐 파일은 알아보지 못한다.
- 전송 계약은 [Attach 메커니즘](../../dev-guide/attach-behavior.md#원격-파일-다운로드-ipcstreamfile-download)에 있다.

## 원격 파일 전송 수신측 저장 정책

원격 attach 채널 위 native bulk 파일 전송(docs/features/remote-attach/index.md)의 **수신측**은 저장 폴더와 폴더 최대 용량을 설정한다("원격이 경로를 소유"). 설정은 `Settings.remote_transfer` — `dir`(저장 폴더, 빈 값이면 기본 `~/.tasty/transfers/`) + `max_mb`(폴더 최대 용량, MiB, 기본 500).

- **IPC/CLI (focus 독립, 전역 설정)**: `settings.get_remote_transfer` / `settings.set_remote_transfer {dir?, max_mb?}` (local-only) · `tasty settings {get-remote-transfer, set-remote-transfer --dir --max-mb}`.
- **GUI 설정**: Settings › General › Remote transfer 서브탭(`src/view/settings/ui/tabs/remote_transfer.rs`) — 저장 폴더(Browse 포함)와 최대 용량(MiB). 디자인↔본체 매핑은 [design-gallery-mapping](../../design/systems/design-gallery-mapping.md) 의 Remote transfer 절.
- **용량 사전 거부**: 전송 시작(`BulkBegin.total_size`) 시점에 `현재 폴더 사용량 + total_size` 가 상한을 넘으면 청크 수신 전에 거부하고 `BulkResult{ok:false, reason:"capacity exceeded"}` 를 회신한다(경계 `== max` 는 허용, `> max` 거부). 폴더 사용량은 1-depth 파일 크기 단순 합산.

## mirror 터미널 이미지 붙여넣기 → 원격 경로 삽입

mirror(attach) 터미널에 클립보드 **이미지**를 붙여넣으면, 로컬 PNG 경로 대신 그 이미지를 bulk 파일 전송 채널(docs/features/remote-attach/index.md)로 원격에 업로드하고 **원격 파일시스템 경로**를 터미널 입력에 삽입한다(원격이 읽을 수 있는 경로). 로컬(비-mirror) 터미널의 이미지 붙여넣기는 기존대로 로컬 temp PNG 경로를 삽입한다(회귀 없음). 텍스트 붙여넣기는 mirror 여부와 무관하게 불변.

- **호스트 generic**: surface kind/claude 특화가 아니라 모든 mirror 터미널에 적용된다(claude CLI 가 그 경로 포맷을 인식하는지는 런타임 외부 동작 — 범위 밖).
- **판정**: 붙여넣기 시점에 focused surface 가 mirror workspace(`Workspace.mirror`) 소속인지로 mirror/로컬을 가른다(판정을 트리거 시점에 끝내 업로드 완료 전 포커스 변경에 흔들리지 않게).
- **비동기**: 업로드(블로킹)는 백그라운드 스레드에서 수행하고(메인 루프 무블록), 완료 시 원격 경로를 그 mirror surface 입력에 삽입한다 — mirror surface 입력은 forwarder 로 원격 PTY stdin 에 투명 전달되므로 별도 삽입 API 가 없다. 진행/완료/실패 피드백은 전송 진행 · 실패 팝업이 담당한다(아래).

## 전송 진행 · 실패 팝업

bulk 파일 전송과 mirror 터미널 이미지 붙여넣기 업로드에 대한 사용자 피드백 UI 2종(scrim 중앙 headless PopupDef). 현재 트리거 소스는 mirror 터미널 이미지 붙여넣기 하나다(일반 파일 전송 UI 는 후속).

- **진행 팝업(`transfer_progress`)**: download glyph + "Receiving file" + mono pct → 파일명(mono 말줄임) → **determinate 4px progress bar**(recessed track `bg-app` + accent fill `accent-primary`, **0ms 무애니** — 바이트 수신 시에만 fill 폭 이동, 수신 바이트에 비례하는 표시) → `transferred / total` + rate → ghost Cancel. `close_on_outside_click=false`(전송 중 실수 dismiss 방지), 모든 파일 완료 시 self-close. 다중 파일은 행 반복. Cancel 은 진행 관망만 중단(동기 워커라 실제 전송 abort 불가 — 백그라운드 전송은 완료됨).
- **실패 팝업(`transfer_error`)**: danger glyph + "Transfer failed" + `<파일명> could not be received.` + mono reason well(command-well: `bg-app`+separator, danger 텍스트). 기본 dismiss(Esc/scrim). danger-fill 버튼 금지. **원격 거부**(수신측 용량 상한 초과 등 `BulkResult{ok:false}`)면 재시도 무의미 → **Dismiss 단독**; **전송 중 실패**(전송/프로토콜 에러)면 → **Dismiss + Retry**(원본 바이트를 기존 업로드 큐에 재투입). 거부 vs 전송에러 판정은 `upload_file_over_bulk` 의 `Err` 접두(`BULK_REJECT_PREFIX`)로 한다.
- **배율**: 두 팝업의 폭과 헤더·본문·푸터·사유 well 여백은 UI 배율을 곱한다. 줄 높이는 글자 크기에서 계산하므로 배율을 바꿔도 카드 높이가 내용과 맞는다.
- **진행률 전달**: `upload_file_over_bulk` 에 `on_progress(sent, total)` 콜백을 추가해 청크 전송마다 통지 → 이미지 업로드 워커가 `transfer_progress` 채널 + `AppEvent::TransferProgressTick` 로 메인에 흘림 → `drain_transfer_progress` 가 해당 행을 갱신. 완료(Ok/Err)는 기존 `ImageUploadReady` 경로가 행 제거 + 성공 삽입/실패 승격을 처리한다.

## 비-목표 (Out of scope)

- **자체 원격 프로토콜/암호화/인증** — 전부 SSH 에 위임. attach 채널에 별도 토큰 없음(연결 경계 = 권한 경계).
- **단발 화면 읽기** — attach 세션을 열 필요 없음. 정식 경로는 `tasty read screen` / `tasty read since-mark`(별도 기능).
- **self attach(자기 인스턴스)의 release 노출** — release 는 자기 인스턴스 포트를 대상으로 한 attach 를 거절하고, 로컬 self-mirror CLI(`tasty debug attach`)는 debug 빌드에만 둔다. 같은 머신의 **다른** 인스턴스에 `127.0.0.1:PORT`·`localhost:PORT`·`[::1]:PORT` 로 붙는 것은 비-목표가 아니다 — GUI 원격 연결 팝업·IPC `remote.attach`·자동 attach 는 release 에서도 SSH 없이 직결한다(CLI `remote attach`·`tool attach`·`remote check` 에는 loopback 분기가 없다). 같은 OS 사용자의 다른 인스턴스는 이미 같은 권한으로 실행되고 같은 사용자 프로세스의 접근은 Tasty 가 막지 못한다. loopback TCP 도 연결한 OS 사용자를 인증하지 않아 공유 머신의 격리 수단이 아니다. 따라서 이 직결은 SSH 로 정한 신뢰 경계를 넓히지 않는다([원격 접속 프로필 + Passkey (remote_tool)](../remote-profiles/index.md) · [에이전트 메모리 시스템](../../design/systems/memory.md)).
- **프로토콜 프레임/터널 연결/재연결 백오프 등 메커니즘** — [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md).
- **SSH 프로필 CRUD** — [ssh-tool](../remote-profiles/index.md).

## Acceptance Criteria

- Given 동일 머신 두 인스턴스 When 한쪽이 다른 쪽 surface 를 attach Then mirror grid 가 원본과 일치한다(`--dump-after` 로 검증).
- Given mirror workspace 의 explorer When 사용자가 `.md` 파일을 더블클릭 Then 원격에 그 파일의 markdown 탭이 생겨 client 에서 선택되고(원격 선택은 유지된다), client 는 원격 원문을 그린다. client 파일시스템과 로컬 최근 목록은 쓰이지 않는다(`file::dispatch::remote` 테스트).
- Given mirror workspace 의 explorer When 원격에 열 수 있는 `open_surface` 핸들러가 없는 파일을 더블클릭 Then 로컬 picker·System·Ipc 핸들러 없이 "다운로드 후 실행" 대화상자가 뜨고, Download and run 을 누르기 전에는 아무것도 받지 않는다(`file::dispatch::remote` 테스트).
- Given mirror origin 을 준 plugin 중계 `file_handler.dispatch`(사용자 입력을 증명하지 못한 요청) When 원격에 열 수 있는 핸들러가 없는 파일을 이름으로 연다 Then 토스트 없이 같은 대화상자가 에이전트 출처로 뜨고, 외부 IPC 요청이면 팝업 없이 warn 로그로 끝난다(`file::dispatch::remote` 테스트).
- Given mirror 워크스페이스의 surface When 원격에 등록되지 않은 kind 로 convert 가 forward 돼 원격에서 실패 Then 회신 사유는 원격이 낸 `unknown surface kind: <kind>` 그대로이고, 대상 surface 가 있는데 "not found" 로 오지 않는다(`core::attach_runtime` 의 `forward_convert_unknown_kind_reports_the_remote_reason`).
- Given surface 가 이미 점유됨 When 다른 client 가 attach 시도 Then holder 정보를 담아 거부된다.
- Given 점유된 surface When 서버측 GUI 키/`surface.send` Then 입력이 차단되고 client 입력만 도달한다.
- Given 점유 상태 When 로컬 사용자가 점유 테두리의 강제 끊기 버튼을 누르거나 사이드바 확인 팝업에서 확인 Then holder 가 종료되고 대상이 일반 surface 로 복귀한다.
- Given release 빌드 When 어떤 호출자든 `attach.force_detach`·`attach.force_detach_workspace` 를 보내거나 `tasty remote attach --force-detach` 를 실행 Then 알 수 없는 메서드·인자 오류로 끝나고 점유는 유지된다.
- Given workspace 가 점유된 상태 When 로컬 사용자가 사이드바에서 그 행을 우클릭 Then **강제 끊기** 항목이 보인다.
- Given 연결된 에이전트 mirror 카드와 로컬 카드 When 각각 우클릭 Then mirror 에는 **워크스페이스 연결 끊기** 와 **워크스페이스 닫기** 가 함께 있고 로컬 카드에는 연결 끊기가 없다.
- Given 저장된 원격 행 When 우클릭 Then 끝에 Connect 또는 Reconnect · Disconnect 와 Close locally · Close remote workspace… 가 있고, 연결 상태에서 할 수 없는 항목은 꺼져 보인다.
- Given 연결된 저장 행 When **Close locally** Then 행이 사라지고 원격 workspace 는 원격에 그대로 있다. 끊긴 행에서도 된다.
- Given 연결된 저장 행 When **Close remote workspace…** 또는 마지막 탭 닫기 Then 확인 대화가 열리고, 확인해야 원격 workspace 가 닫히며 행도 사라진다. 마지막 탭에서 열린 확인에는 마지막 탭 안내 줄이 있다.
- Given 연결된 mirror R When **워크스페이스 닫기** Then 서버 목록에서 R 만 사라지고 R 의 프로세스가 끝나며, 성공 회신 뒤 client 의 R mirror 와 세션이 정리된다. 다른 서버 workspace 와 로컬 workspace 는 유지된다.
- Given 서버가 닫기를 거절하거나 capability 가 없는 구 서버 When **워크스페이스 닫기** Then mirror 는 남고 실패·미지원 안내가 뜬다. 회신 전에 연결이 끝나면 결과를 모른다고 안내하며 성공으로 표시하지 않는다.
- Given 연결된 mirror R When **워크스페이스 연결 끊기** Then client 의 R mirror 만 사라지고 서버의 R 과 프로세스는 같은 ID 로 남으며 서버 점유가 풀린다. 저장된 원격 행이면 행은 연결 끊김으로 남는다. 다시 attach 하면 같은 workspace 와 출력에 접근한다.
- Given mirror 로 붙은 원격 workspace R When 에이전트가 `tasty remote disconnect --workspace <mirror id>` Then 이쪽 mirror 는 사라지고 원격 R 과 그 프로세스는 남으며, `remote attach --into-gui --workspace <R>` 로 다시 붙으면 같은 화면이 보인다.
- Given 그 항목을 누른 상태 When 확인 팝업이 떠 있다 Then 점유는 아직 유지되고, 취소·Escape·바깥 클릭 어느 쪽으로 닫아도 점유가 그대로 남는다.
- Given 확인 팝업이 떠 있는 동안 원격이 스스로 끊었다 When 다음 프레임 Then 팝업은 즉시 닫힌다(대상 없는 확인을 남기지 않는다).
- Given client 연결 종료(EOF) Then 점유 lock 이 자동 free 된다.
- Given client 가 FIN/RST 없이 조용히 끊김(silent disconnect) When attach heartbeat TTL 이 만료 Then 점유 lock 이 EOF 와 동일하게 자동 free 되고, 같은 surface/workspace 로 새 client 의 재attach 가 성공한다.
- Given workspace attach When 멤버 터미널 하나가 이미 다른 client 점유 Then workspace attach 가 거부된다.
- Given stale 포트 파일만 있는 죽은 인스턴스 When `tasty remote check` Then dead(exit≠0)로 판정한다.
- Given 원격에 `tasty.port` 와 `tasty.instance` 가 남았고 그 PID 가 없거나 다른 프로세스다 When 원격에서 `tasty port` Then 포트를 출력하지 않고 종료 코드 3으로 끝나며 포트 파일이 남은 것이라고 알린다(`crates/tasty-cli/src/port_file.rs`, `crates/tasty-ipc/src/instance_file.rs` 시험. 실측: 격리 홈 debug GUI 를 `kill -9` 한 뒤 rc 3). 인스턴스가 실행 중이면 지금처럼 포트를 출력하고 0으로 끝난다(실측).
- Given 같은 상태 When auto 모드 포트 발견 Then 파일 모드로 넘어가지 않고 원격 인스턴스 종료 확정으로 실패한다(`crates/tasty-ssh/src/lib.rs` 의 ssh 대역 시험).
- Given 원격 포트 파일이 가리키는 포트에 리스너가 없다 When `tasty remote check`·`remote workspaces`·`remote new-workspace`·`tool attach` Then 오류가 원격 tasty 미실행 또는 오래된 포트 파일을 원인으로 적고 "터널을 연결했지만" 문장을 쓰지 않는다. 원격 포트에 다른 서비스가 리스닝 중이면 기존 IPC 실패 안내가 나온다(실측: 8945hs 의 빈 포트와 sshd 포트).
- Given 부모 Tasty의 `TASTY_SESSION_TOKEN`을 상속한 GUI·CLI When 다른 인스턴스로 원격 브라우징·생성·생존 확인 요청을 보낸다 Then 요청에 세션 토큰이 없고, 원격은 `session_token unknown` 없이 응답한다.
- Given workspace attach 대상에 egui-mesh surface(번들 image/mesh_demo 또는 커스텀 plugin 의 egui-mesh kind) 가 있음 When client 가 GUI mirror 로 attach Then 그 surface 의 실제 렌더 콘텐츠가 mirror pane 에 표시된다(placeholder 아님). markdown은 아래의 원문 전달 채널을 사용한다.
  - image — 미검증.
  - mesh_demo — 미검증.
  - 2종 공통 시각적 렌더 확인(실제 GUI attach client 로 mirror pane 화면 비교) — 미검증.
- Given workspace attach 대상에 markdown surface 가 있음 When client 가 attach Then 그 surface 는 placeholder 가 아니라 전용 role 로 도착하고 원격 파일 경로와 탭 제목이 함께 실린다(`tests/attach_markdown_content_loopback.rs`).
- Given mirror 의 markdown surface When client 가 원문을 요청 Then 서버가 실제 디스크의 원문을 그대로 회신하고, 읽을 수 없는 파일은 연결이 끊기거나 무응답이 되는 대신 사유와 함께 실패로 답한다(`tests/attach_markdown_content_loopback.rs`).
  - client GUI 가 그 원문을 실제로 렌더하는 것 — 실측(GUI 두 인스턴스 loopback attach): client plugin 이 원격 원문으로 HTML 을 만들어 올리고(`markdown surface N: loaded … bytes of HTML`), 화면에 원격 문서의 제목·본문이 그려진다.
- Given client 에 번들 markdown plugin 이 등록돼 있음 When mirror 트리에 markdown role leaf 가 있음 Then 그 leaf 는 빈 surface 가 아니라 markdown kind 로컬 surface 로 만들어지고 원격 파일 경로가 plugin 에 전달된다 — 구조 delta 로 트리가 다시 와도 같은 surface 를 재사용한다(`src/app/attach_client/survivors/tests.rs` 단위 테스트).
- Given `markdown` kind 를 다른 plugin 이 차지함 When 같은 트리 Then 그 leaf 는 빈 surface 로 남는다(`src/app/attach_client/survivors/tests.rs` 단위 테스트).
- Given client 의 번들 markdown plugin 이 attach 시점에 안 떠 있음(부팅 때 꺼져 있던 plugin) When plugin 을 켠 뒤 그 mirror 워크스페이스를 연다 Then 빈 surface 로 남지 않고 plugin 이 뜬 뒤 원격 문서로 채워진다 — kind 대기 자리가 kind 등록 뒤 mirror 문서로 실제화되는 것(`src/app/attach_client/survivors/tests.rs` 단위 테스트)과 plugin 이 그 복원 data 를 로컬 snapshot 과 구별하는 것(`crates/tasty-plugin-markdown/src/tests.rs`)을 고정한다. 실측(GUI 두 인스턴스 loopback attach): 켜기 전 surface 목록 `Pending` → 켜고 연 뒤 `Remote`, 원문이 그려지고 탭 제목은 원격 파일 이름이다.
- Given mirror markdown 문서 When plugin 이 그 문서를 연다 Then 원격 경로가 이 머신에 실재해도 파일을 읽지 않고, 상대경로를 풀 디렉토리를 두지 않으며, layout 에 남길 snapshot 도 내지 않는다 — 원문은 요청해 받은 것만 그린다(`crates/tasty-plugin-markdown/src/tests.rs`).
- Given mirror markdown 문서 When 새로고침 버튼을 누르거나 `markdown.reload` 를 부른다 Then 원격 원문을 다시 요청하고, 늦게 온 옛 회신은 버린다(`crates/tasty-plugin-markdown/src/tests.rs`). 실측(GUI 두 인스턴스): 버튼 클릭과 `markdown.reload` 모두 원문을 다시 받아 stale 표시를 끈 문서로 다시 그린다.
- Given mirror markdown 문서(원문을 기다리는 중이든 이미 표시 중이든) When 저장된 원격 행 세션의 attach 연결이 끊겨 재연결 대기에 들어감 Then 문서는 로딩 상태로 멈추지도 옛 원문을 최신처럼 남기지도 않고 "원격 연결이 끊어졌다" 는 끊김 화면으로 바뀐다 — 요청 송신 자체가 실패해도 같은 실패 결과가 간다. plugin 이 abandon 을 받으면 원문을 받은 뒤에도 끊김 상태가 되는 것(`crates/tasty-plugin-markdown/src/tests.rs`)과 끊김 상태가 원문보다 앞서 그려지는 것(`crates/tasty-plugin-markdown/src/render.rs` 단위 테스트)을 고정한다. 실측(GUI 두 인스턴스, 서버 종료): 원문을 표시 중이던 문서가 끊김 문구 화면으로 다시 그려졌다. 저장되지 않은 mirror 는 mirror 워크스페이스째 정리되고 끊김 toast 가 뜬다(실측).
- Given 끊김 화면을 보이는 survivor mirror markdown 문서 When 같은 서버에 재연결 Then 사용자가 누르지 않아도 원문을 다시 받아 그린다 — 끊긴 동안 바뀐 원문이면 바뀐 원문이 온다. client 가 재연결 직후 survivor 문서마다 변경 신호를 한 번 보내고, plugin 은 그 신호로 원문을 보여 주던 문서는 stale 표시만, 원문을 못 보여 주던 문서(끊김·실패)는 재요청한다(`crates/tasty-plugin-markdown/src/tests.rs`). 실측(GUI 두 인스턴스, 서버를 SIGSTOP 해 heartbeat 만료로 끊고 SIGCONT 로 재연결): 같은 로컬 surface 가 끊김 화면에서 원문으로 돌아왔고, 재연결 신호 송신을 빼는 변이에서는 끊김 화면에 머물렀다. 서버를 재시작하면 원격 surface id 가 바뀌어 survivor 가 아니라 새 leaf 로 만들어지고 새로 원문을 받는다(실측).
- Given mirror markdown 문서가 원문을 받아 표시 중 When 원격(GUI 서버)에서 그 문서가 다시 그려짐(파일 수정·테마 변경 등) Then client 는 원문을 자동으로 다시 받지 않고 새로고침 버튼 색만 바뀌며, 버튼을 누르면 그때 최신 원문이 온다 — 서버가 신호를 워크스페이스를 점유한 client 전부에 한 번씩 보내는 것과 점유가 없으면 아무것도 안 보내는 것(`src/remote/server.rs` 단위 테스트), client 가 자기 mirror 문서의 신호만 plugin 에 넘기는 것(`src/app/attach_client/output/tests.rs` 단위 테스트), plugin 이 신호로는 stale 표시만 켜는 것(`crates/tasty-plugin-markdown/src/tests.rs`)을 각각 고정한다.
  `webview.set_url` 수신이 그 신호 송신으로 이어지는 것은 `src/adapters/ipc/handler/webview.rs` 단위 테스트가 고정한다(신호 호출을 빼는 변이에서 실패).
  실측(GUI 두 인스턴스): 서버에서 원문 파일을 고치자 client 문서는 원문을 다시 받지 않고, 새로고침 버튼의 stale 표시만 바뀐 HTML 로 다시 그려졌다(HTML 크기 차이가 툴팁 문구 차이와 같다).
  서버가 헤드리스면 이 신호는 없다(원격 문서가 다시 그려지는 경로가 host 에 닿지 않는다).
- Given 전송 형태로 예산(700 KiB)을 넘는 문서 When client 가 원문을 요청 Then 세션이 끊기지 않고 잘린 원문이 "잘렸다" 표시와 함께 도착하며, 그 판정은 원문 바이트가 아니라 이스케이프된 길이로 이뤄진다(`tests/attach_markdown_content_loopback.rs`).
- Given attach 점유가 없는 client When 원문을 요청 Then 파일을 한 바이트도 읽히지 않고 거절된다(`tests/attach_markdown_content_loopback.rs`).
- Given 워크스페이스 둘 중 **한쪽만** 점유한 client When 다른 워크스페이스의 markdown 원문을 요청 Then 인가되어 원문이 도착한다 — 점유는 인스턴스 단위 신뢰이지 워크스페이스별 권한이 아니다(`tests/attach_markdown_content_loopback.rs`).
- Given 원격 자식을 지원하는 서버의 mirror 와 같은 부모 When Claude·Codex 자식을 동시에 spawn Then 각 응답은 자기가 만든 자식만 싣는다(`src/remote/agent_children/state.rs` 의 `overlapping_calls_of_one_origin_keep_their_own_children`). 실측(Linux 두 인스턴스 loopback): 두 자식이 각각 서버 surface 로 생기고 client 부모 아래 등록됐다.
- Given 원격 자식 When 서버에서 Stop 훅이 보고 Then client 부모의 완료 로그에 한 줄 기록되고 서버에는 완료 로그가 생기지 않는다. 실측(Linux 두 인스턴스, fixture 훅과 실제 Claude haiku 자식). Codex 대화가 부모면 Claude·Codex 자식의 턴 결과가 그 대화로 돌아온다(실측).
- Given holder 연결이 끊김 When `terminal.children`·`terminal.state` Then 원격 자식은 `liveness: unknown`, `state: unknown` 이고, 같은 서버에 재연결하면 그동안 쌓인 통지가 중복 없이 한 번 적용된다. 실측(Linux, 두 인스턴스 사이 TCP 중계를 끊었다 같은 주소로 복구). 통지 재전송과 중복 제거는 `src/remote/agent_children/outbox.rs` 의 `a_disconnect_makes_in_flight_notices_due_again_for_the_next_connection` 과 `client.rs` 의 `keys_are_seen_once` 가 고정한다.
- Given 서버의 공개 IPC·CLI·다른 plugin When 일반 namespace 요청에 `attach_context`·`attach_target_surface`·`attach:` 부모를 싣는다 Then host 가 plugin 에 넘기기 전에 `-32602` 로 거절한다(`crates/tasty-host-plugin/src/manager/tests_request_origin.rs` 의 `a_general_namespace_request_cannot_carry_remote_attach_markers`, 실측).
- Given 닫힌 원격 자식 When 서버 plugin 이 닫힌 뒤 10분 안에 그 자식의 결과를 맡긴다 Then 쌓여 client 확인으로 끝나고, 기록이 없으면(서버 재시작·기간 경과) `rejected` 다(`state.rs` 의 `a_closed_child_still_takes_its_last_results_within_the_retention`·`a_child_record_gone_by_expiry_or_restart_rejects_new_results`).
- Given 자식이 mirror 에 나타났지만 부모 아래 등록하지 못함 When call 이 끝남 Then 성공으로 답하지 않고 plugin 의 late_spawn 으로 넘겨 그 자식을 닫게 한다(`client.rs` 의 `a_child_that_could_not_be_registered_goes_to_the_late_path`).
- Given 원격 실행을 선언하지 않은 plugin 명령 When 숫자 `--workspace` 와 상대 경로 인자 Then CLI 는 mirror 여부를 묻지 않고 로컬 기준 절대 경로로 바꾼다(`crates/tasty-cli/src/dynamic/tests.rs` 의 `a_method_without_remote_execution_keeps_local_paths_even_for_a_numeric_workspace`).
- Given mesh mirror pane 이 표시 중 When client 가 그 pane 을 클릭/타이핑 Then 원격 plugin 프로세스의 상태가 실제로 바뀌고 그 결과가 mirror 에 반영된다(예: mesh_demo 클릭 카운터 증가).
- Given mesh mirror pane 에 텍스처 delta 체인 단절(예: 재연결) When client 가 감지 Then `MeshFullResendRequest` 로 전체 텍스처 상태를 재수신해 정상 렌더를 회복한다.

> 프로토콜과 터미널 화면 동기화는 headless로 검증할 수 있다 — 같은 머신에 인스턴스를 띄우고 debug 빌드의 `tasty debug attach --workspace <id> --dump-after <ms>` 로 그 인스턴스의 포트에 SSH 없이 직접 붙어 grid 일치를 확인한다. 포트는 `TASTY_HOME` 이 가리키는 데이터 루트의 포트 파일에서 읽으므로 서버 인스턴스의 `TASTY_HOME` 으로 실행한다. workspace id 는 `tasty remote workspaces --ssh 127.0.0.1:<port>` 로 찾는다.
>
> `tasty remote attach`·`tool attach`·`remote check` 에는 loopback 분기가 없다. `--ssh 127.0.0.1:<port>` 를 줘도 SSH 로 원격 포트를 찾으려다 "SSH connection failed" 로 끝난다. `127.0.0.1:PORT`·`localhost:PORT`·`[::1]:PORT` 직결은 browse(`remote workspaces`·`remote new-workspace`), IPC `remote.*`, GUI 원격 연결 팝업, 저장된 원격 행의 연결에만 있다.

## 구현

- 점유 레지스트리: `src/core/attach.rs` `OccupancyRegistry`(hard: `surface_locks` / `workspace_locks` / `surface_to_workspace`, acquire/release/force_detach/release_all_for_client · soft: 별도 엔트리 + acquire_soft/release_soft, docs/dev-guide/attach-behavior.md). 휘발성.
- 런타임/스냅샷: `src/remote/server.rs`(서버측 수신, transport 무관 loopback), `src/remote/readonly.rs`(서버측 readonly mirror), `src/app/attach_poll.rs`(3초 tick).
- 저장된 원격 행의 연결·재연결: `src/app/auto_attach.rs`(행을 띄우면 SSH 터널 + GUI mirror, 재연결 시간 창).
- IPC: `src/adapters/ipc/handler/attach.rs`(`attach.*`). 원격 브라우징/attach IPC는 `src/app/ipc/app_methods/remote.rs`와 `src/app/auto_attach.rs`를 거친다. `src/app/attach_client.rs`는 원 engine/attempt에 연결된 결과를 Remote projection에 설치한다. IPC 경로는 사용자 선택을 옮기지 않는다.
- GUI picker: `src/adapters/ui/popup/remote_attach.rs`가 `BrowserRequest`를 보내고 `src/app/remote_browser.rs`가 원 browse owner를 확인해 연결한다. Connect는 원 engine/View의 mirror 설치 target에 사용자 활성화 의도를 고정한다. 현재 선택을 직접 쓰는 옛 `start_gui_attach`/`pending_gui_attach_user` 경로는 사용하지 않는다.
  새 원격 workspace 생성도 같은 browser 요청/결과 경계를 사용한다.
  갤러리 specimen 진입점·치수 상수: `crates/tasty-gallery/src/catalog/components/remote_attach.rs`(loaded / 새 행 5상태 / 우측 pane 상태).
  자식 `remote_attach/new_row.rs`는 새 행 5상태, `panes.rs`는 좌우 pane 조립, `rows.rs`는 프로필·workspace 행과 공용 dot 슬롯을 담당한다.
- mirror 비영속: 로컬 journal 모델과 Remote projection을 분리하며, 호환 export는 `src/runtime/journal_payload/legacy_export.rs`에서 로컬 모델만 읽는다.
- mirror 이미지 붙여넣기 → 원격 업로드: `src/view/main/clipboard.rs`(이미지 분기에서 `Workspace.mirror` 판정 → 원 target을 담은 `EngineAction::ImageUpload`를 보내고 App이 해당 `RemoteState.pending_image_uploads`에 PNG 요청을 보관, 비-mirror 는 기존 로컬 PNG 경로 유지), `src/app/image_upload.rs`(`poll_image_uploads`: 큐 drain → 백그라운드 `upload_file_over_bulk` → 결과 채널 → `dispatch_paste`(원격 경로 삽입) 또는 전송 실패 팝업 승격). 업로드 API 는 `src/app/attach_client/bulk.rs::upload_file_over_bulk`(bulk 클라 송신, 동기 블로킹).
- 전송 진행/실패 팝업: 호스트 팝업 `src/adapters/ui/popup/transfer.rs`(`TRANSFER_PROGRESS_POPUP_ID`/`TRANSFER_ERROR_POPUP_ID`, `TransferProgress`/`TransferRow`/`TransferError` + draw/sizer), PopupDef 등록 `defs.rs`(둘 다 headless scrim; progress `close_on_outside_click=false`), scrim/bg 매칭 `popup.rs`·`popup/draw.rs`, DialogState 슬롯 `src/state/dialogs.rs`(`transfer_progress: Option` + `transfer_error: VecDeque`), self-close cleanup `PopupDef.on_close`(`transfer.rs`의 `on_close_transfer_progress`/`on_close_transfer_error`).
  진행률 전달: `upload_file_over_bulk` 의 `on_progress(sent,total)` 콜백(bulk 송신 경로 침범 최소) → 이미지 업로드 워커가 `transfer_progress` 채널 + `AppEvent::TransferProgressTick`(`event.rs`/`event_handler.rs`) → `image_upload.rs`(`begin/drain/finish_transfer_progress_row`, `push_transfer_error`, `format_rate`; 실패 분류는 `BULK_REJECT_PREFIX` 접두로 거부 vs 전송에러).
  갤러리 specimen `crates/tasty-gallery/src/catalog/components/transfer.rs`(progress/error 2종).
  i18n `[transfer.progress]`/`[transfer.error]`.
- 브라우징 코어(CLI/IPC 공유): `crates/tasty-remote/src/browse.rs`(`browse`/`resolve_endpoint`/`probe_method` — loopback 직결 + `workspace.list`+`attach.list` 병합).
- mesh mirror(egui-mesh surface): 프로토콜/분류/서버 구독·forward/클라이언트 렌더·입력 전체 상세는 [dev-guide/egui-mesh-channel "attach mesh mirror 소비 경로"](../../dev-guide/egui-mesh-channel.md#attach-mesh-mirror-소비-경로).
- CLI: `crates/tasty-cli/src/commands/remote.rs`(clap 선언), `crates/tasty-cli/src/dispatch.rs`(갈래 판정), `local/attach.rs`(`run_attach_*` 세션 머신), `local/remote_check.rs`, `local/remote_workspaces.rs`(browse 얇은 래퍼), `crates/tasty-ssh/src/lib.rs`(SSH 연결 + `PortDiscoveryError`/`PortDiscoveryFailureKind` 원인 분류, `pick_most_informative` 로 auto 체인 대표 에러 선택).

## 화면

화면정의서 — **원격 attach 화면 (GUI mirror)**.

- **시각 소스**: mirror 는 기존 워크스페이스 렌더러를 쓰며, 로딩·대기 표시는 `gallery/layouts-loading-prelayout.jsx`의 공용 LoadingOverlay·PendingIndicator를 따른다. 시각 요소는 사이드바/작업영역 문서로 위임(연결 개념).
- 응답 대기 툴팁은 요청 종류마다 해당 종류의 가장 오래된 접수부터 흐른 시간을 한 줄씩 보여 준다. 내부 요청·객체 ID는 표시하지 않는다. 칩 본문은 전체 요청 수와 전체에서 가장 오래된 시간을 유지한다.

원격 attach 는 대부분 headless/CLI 동작이고, GUI 로 보이는 것은 **mirror 워크스페이스**, **요청의 선행 배치·응답 대기 표시**, **점유된 대상의 readonly 표시**다. 둘 다 별도 화면을 만들지 않고 기존 화면의 상태로 표현한다.

### 트리거

`tasty remote attach --into-gui …` 또는 저장된 원격 행 활성화 → 로컬 GUI 에 mirror 워크스페이스가 **추가**된다(포커스/active 강제 전환 없음 — 부모 기획).

### UI 요소 인벤토리

- **mirror 워크스페이스** — 사이드바에 일반 워크스페이스처럼 노출되되 **이름과 subtitle 사이 별도 줄의 `>_→` glyph 와 Tag `info` "REMOTE"**(collapsed 레일은 아바타 우하단 하늘색 corner chip)로 로컬과 구분한다(`Workspace.mirror`). status dot 은 실행상태 전용이라 mirror 색을 싣지 않는다. 사이드바 표현 → [`features/sidebar/`](../sidebar/index.md).
- **mirror 콘텐츠** — 원격 트리(Pane/Tab/Surface)를 그대로 재구성해 [작업 영역](../work-area/index.md#화면) 렌더러가 그린다. 별도 위젯 없음.
- **점유된 대상의 readonly 표시** — 서버측에서 다른 client 가 점유한 surface 는 내용은 보이되 조작이 차단된 readonly 로 렌더(작업영역의 deferred/readonly 상태와 동일 경로).

### 상태별 시각

- **mirror 활성** — 이름·subtitle 사이 Tag `info` REMOTE(레일=우하단 chip). 원격 출력이 오면 즉시 갱신(3초 tick 은 backstop). status dot 은 별개로 실행상태(running/idle)만 표시.
- **점유 중(서버측)** — 그 surface 는 readonly. force-detach 되면 일반 surface 로 복귀.
- **강제 끊기 확인 팝업** — 사이드바 워크스페이스 행 우클릭의 강제 끊기가 여는 380px destructive confirm(헤더 close 글리프 danger + 대상 이름이 든 본문 + 취소/강제 끊기(danger)). headless · window scope · 중앙 정렬이라 대상 워크스페이스가 비활성이어도 보인다. 갤러리 specimen 은 점유 테두리와 같은 카드에 있다(`crates/tasty-gallery/src/catalog/components/occupancy_borders.rs`).
- **세션 끊김** — 자동 재연결(지수 백오프) 동안의 표시도 mirror 워크스페이스 상태로 흡수. 상태바 원격 항목은 원격 색을 빼고 재연결 상태어를 붙인다([원격 위치 표시](#원격-위치-표시)).

### 시각 소스

`gallery/layouts-loading-prelayout.jsx`는 생성 선행 배치와 응답 대기 표시의 시각 소스다. dot/워크스페이스 항목은 사이드바 시각 소스, mirror 타일은 작업영역 시각 소스를 따른다. 점유/mirror 는 *상태*일 뿐 새 레이아웃이 아니다.

확정 구조 변경은 영향받은 workspace에만 변경 통지를 예약한다. 이동은 이전과 이후 소속을 모두 포함하고 삭제는 이전 소속으로 판단한다. 같은 holder에 같은 트리를 보냈으면 중복 전송을 생략하며, 점유 해제·연결 종료 시 해당 전송 캐시를 제거한다. Result→Delta→tap 순서는 유지한다.
