# 원격 attach (Remote attach)

- **Status**: Implemented
- **주체**: 원격 접속 사용자(점유 후 조작) · AI Agent(원격 mirror 를 정당한 행동으로 attach) · 로컬 사용자(force-detach 권한)
- **ADR**: [ADR-0007](../../adr/0007-attach-targets-remote.md) (attach 는 원격 대상 · 로컬 self-attach 는 debug 격리). 보안 위임 근거는 [ADR-0004](../../adr/0004-ipc-transport-tcp.md) (loopback trust boundary). mirror 탭 제목 폴백의 i18n 은 [ADR-0106](../../adr/0106-non-widget-user-strings-go-through-i18n.md)
- **코드**: `src/core/attach.rs`(`OccupancyRegistry`), `src/core/attach_runtime.rs`, `src/app/auto_attach.rs`, `src/adapters/ipc/handler/attach.rs`, `src/app/ipc/app_methods.rs`(`remote.workspaces`/`remote.attach`), `crates/tasty-remote/src/browse.rs`(브라우징 코어), CLI `crates/tasty-cli/src/commands/remote.rs`(clap 선언) · `crates/tasty-cli/src/local/{attach,remote_check,remote_workspaces}.rs`(실행)
- **화면**: [screens/remote-attach.md](screens/remote-attach.md)

## 목적

다른 호스트(또는 동일 머신의 다른 인스턴스)의 surface/workspace 를 **점유(occupy)** 해 mirror 로 보고 조작하는 기능. tasty 는 자체 원격 프로토콜·암호화를 만들지 않고 **SSH 에 위임**한다 — "그 호스트에 SSH 로 들어올 수 있는 사람 = attach 자격"(tmux 모델). 원격 접속 사용자가 tasty 를 쓰는 유일한 경로이며, [actors](../../concepts/actors.md) 의 **점유 모델**이 실제로 구현되는 지점이다.

## 내부 동작 (headless-valid)

### 점유 (Occupation)

attach 의 본질은 **강한(hard) 배타 점유**다 — [ADR-0040](../../adr/0040-occupancy-soft-hard-tiers-agent-occupant.md) 2계층 점유 중 강한 점유 계층의 사례다(약한 점유는 [child-terminal](../child-terminal/index.md)). 개념 정의는 [actors 점유 모델](../../concepts/actors.md#점유-occupation-모델), 여기선 동작:

- **배타 lock**: 한 surface 는 한 client 만 점유한다(`OccupancyRegistry`). 점유는 `stream.open{target}` 핸드셰이크의 `attach.acquire` 로 잡고, 동시 attach 는 holder 정보를 담아 `already_attached` 로 거부.
- **점유 중 격리**: 점유된 surface 의 서버 로컬 입력(GUI 키 / `surface.send`)은 차단되고, **점유 client 입력만** PTY 에 도달한다. 로컬 사용자·AI Agent 는 그 대상에 대해 **readonly** — 내용은 보이되 조작은 막힌다. readonly 는 PTY/TUI 조작(키 입력·마우스 트래킹 보고·휠 스크롤·Ctrl+click 링크 열기)만 차단하는 것이고, **드래그로 텍스트를 선택해 클립보드로 복사하는 tasty 자체 기능은 예외적으로 계속 동작**한다 — PTY 에 아무것도 보내지 않는 순수 로컬 UI 동작이기 때문이다(좌표·복사 텍스트는 실제 렌더되는 mirror 기준). 근거: [ADR-0049](../../adr/0049-hard-occupancy-selection-exception.md).
- **점유 중 attention 해제 권한**: 점유된 surface 의 **주의 환기(attention) 해제도 홀더만** 할 수 있다 — 서버 로컬 사건(그 surface 의 실 렌더 포커스 · 알림 패널의 읽음 처리·"모두 읽음")은 attention 을 지우지 못한다. 확인의 주체가 곧 그 surface 의 주체이기 때문이다. 홀더의 확인은 `ClientAttentionClear`(client→server) 로 들어와 holder 검증 후 적용된다([ADR-0104](../../adr/0104-mirror-attention-clear-forwarded-to-owner.md)). 게이트 범위는 좁다 — **알림 자체는 점유와 무관하게 읽음 처리되고**(읽음은 이 인스턴스 사용자의 패널 상태), soft 점유에는 걸리지 않으며, 점유가 풀리면 서버 로컬 포커스가 자동으로 해제 주체로 복귀해 stale 배지를 회수한다. 근거: [ADR-0109](../../adr/0109-hard-occupancy-attention-clear-holder-only.md). 이 게이트는 위 selection 예외(ADR-0049)와 **다른 축**이다 — selection 은 로컬 사용자 화면·클립보드에만 존재해 홀더가 보는 것을 바꾸지 않지만, attention 레코드는 push 채널로 홀더에게 그대로 전달되는 공유 상태다.
- **자동 해제**: client 연결 종료(EOF) 또는 attach heartbeat TTL 만료(FIN/RST 없는 silent disconnect 감지) 시 lock 이 free 로 환원. 점유는 **휘발성** — 서버 재시작 시 전부 free(영속 안 함).
- **실패하는 attach 는 점유를 잡지 않는다**: 핸드셰이크의 스트림 프로토콜 버전(`stream.open` 의 `proto`)이 서버와 다르면 attach 를 dispatch 하기 **전에** 거절 ack(`ok:false` + 사유)로 끊는다 — 성립할 수 없는 세션이 점유만 가져가 정상 attach 를 `already_attached` 로 막는 것을 방지한다. 검증 없이 잡으면, 소켓을 닫지 않는 구버전/hung peer 에서는 EOF 도 안 와 heartbeat TTL(20초)까지 그 workspace 가 붙잡힌다. 근거: [ADR-0116](../../adr/0116-attach-handshake-validated-before-occupancy.md).
- **self-attach(자기 인스턴스 포트로 attach)는 거절된다** — debug/release 공통. GUI attach 핸드셰이크는 메인 스레드에서 동기 대기하는데 그 응답을 만드는 것도 같은 메인 스레드라 자기 자신 대상이면 교착으로 반드시 실패하고, 실패하는 동안 대상 workspace 점유만 남는다. 로컬 self-mirror 가 필요하면 별도 프로세스인 `tasty debug attach` 를 쓴다(같은 이유로 교착이 없다).
- **force-detach**: **로컬 사용자만** 점유를 강제로 끊을 수 있다(서버 권한). 끊으면 holder client 에 종료를 통지하고 대상은 **일반 surface/workspace 로 복귀**. GUI 진입점은 **둘**이다 — 점유된 surface 우상단의 강제 끊기 버튼(그 워크스페이스가 활성일 때만 그려진다)과, 사이드바 워크스페이스 행 우클릭의 **강제 끊기** 항목. 뒤엣것은 점유 중일 때만 나타나고(사이드바의 점유 표시와 같은 술어 `workspace_holder(ws.id).is_some()`), 즉시 끊지 않고 화면 중앙의 확인 팝업을 한 번 거친다 — 원격 세션을 끊는 비가역 행동이고 우클릭은 오조작이 쉬운 자리이기 때문이다. 그 팝업은 window scope 라 **대상 워크스페이스로 전환하지 않아도** 보인다. 확인 팝업 본문은 대상 이름과 결과만 적고 점유자를 식별자로 적지 않는다 — 서버는 transport 를 모르고 숫자 client id 만 들고 있다. surface 단위 lock(`--surface`)은 사이드바에 표시되지 않으므로 이 경로로 풀 수 없다(그 워크스페이스로 전환해 surface 버튼을 쓴다).

### surface 단위 vs workspace 단위

- **surface attach**: 단일 터미널 surface 를 mirror. 한 연결 = 한 터미널.
- **workspace attach**: 워크스페이스를 점유하면 그 안 **모든 터미널 surface 를 트리째 mirror**(분할 방향/비율 포함)한다. **bundled egui-mesh surface**(image/mesh_demo — bundled 화이트리스트에 등록된 kind 한정. markdown 은 Stage B(webview 전환)로 이 화이트리스트에서 빠졌다)는 **mesh mirror** 로 실제 콘텐츠가 보이고 클릭/타이핑까지 원격 plugin 에 도달한다(인터랙티브). **markdown surface 는 렌더 픽셀이 아니라 원문을 나르는 전용 채널로 mirror** 된다 — 서버가 그 문서의 원문을 보내고 client 의 markdown plugin 이 **자기 테마·자기 설정으로 다시 그린다**([ADR-0255](../../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md)). client 는 그 leaf 를 빈 surface 가 아니라 **로컬 markdown surface** 로 만들고(탭 제목은 원격 파일 이름), 그 surface 의 plugin 이 원문을 요청해 받는다. client 의 번들 markdown plugin 이 attach 시점에 꺼져 있었다면 그 자리는 plugin 을 기다리는 자리로 남았다가 plugin 이 뜬 뒤 문서로 채워진다(설치돼 있지 않으면 계속 그 자리로 남는다). 스코프: 원격 문서의 상대경로 이미지·링크는 client 에 그 파일이 없어 깨지고, mirror 문서에서는 주소창으로 다른 파일을 열 수 없다(뷰어라 편집·저장은 원래 없다). **explorer surface 는 File Picker 와 동일한 `list_dir_request`/`list_dir_result` 채널을 재사용해 browse-only 로 mirror** 된다 — 디렉토리 목록 열람·내비게이션만 가능하고 rename/delete/파일 내용 열기는 스코프 밖([ADR-0059](../../adr/0059-explorer-remote-attach-list-dir-reuse-browse-only.md)). 이 browse-only 제약은 더블클릭 열기뿐 아니라 **컨텍스트 메뉴·키보드 단축키 레벨까지 강제**된다 — 붙여넣기/잘라내기/이름 변경/삭제/시스템에서 열기/새 탭으로 열기/즐겨찾기 추가는 mirror explorer 에서 메뉴 노출부터 숨겨지거나 클릭 시 차단된다(상세: [explorer 기능 문서](../explorer/index.md#mirrorattach-explorer-의-browse-only-강제)). 그 외 비-터미널 surface(화이트리스트 밖 kind 포함 — 예: html)는 여전히 mirror 불가라 placeholder 로 숨긴다. workspace lock 은 멤버 터미널 전부를 surface lock 에도 등록하므로, 멤버가 이미 다른 client 에 점유돼 있으면 workspace attach 를 **거부**(부분 점유 충돌 방지).
  - **mesh 프레임 forward 는 서버가 headless 든 GUI 든 동작한다.** GUI 가 서버(창 보유)인 경우 로컬 창의 자체 redraw 가 이미 그 plugin 을 구동 중이므로, attach forward 는 그 결과(이미 만들어진 mesh 프레임)를 옆에서 읽어 client 에 중계할 뿐 별도 geometry 권위를 만들지 않는다(로컬 redraw 가 여전히 권위) — [dev-guide/attach-behavior "mesh mirror 채널"](../../dev-guide/attach-behavior.md#mesh-mirror-채널) 참고. 단, attach client 의 클릭/타이핑을 로컬 plugin 에 되먹이는 입력 역방향 forward 는 서버가 headless 일 때만 배선돼 있다 — GUI 가 서버면 mesh 콘텐츠는 보이지만 아직 인터랙티브하지 않다(후속 작업).
  - **explorer mirror 는 File Picker 선례를 그대로 재사용한다.** 초기 root 는 원격 explorer 의 활성 탭 root 만 보내고(전체 탭 아님), 탭 전환·트리 펼침 등 새 경로가 필요해질 때마다 client 가 그 시점에 `list_dir_request` 를 보내는 on-demand 재조회다. `ExplorerViewStore`(surface 별)가 경로별 pending/캐시 상태를 자체 소유하므로 host 에 별도 "request_id → consumer" 레지스트리가 없다. wire 인가 범위는 File Picker 와 동일("attach 점유 = 신뢰") — explorer 전용 필드를 추가하지 않는다.
  - **markdown mirror 는 내용을 나르지 화면을 나르지 않는다.** 핸드셰이크에는 좌표(원격 경로·탭 제목)만 실리고 원문은 client 가 필요할 때 따로 가져온다. 한 번에 나르는 양에는 상한이 있고(전송 형태로 700 KiB), 넘으면 잘린 채 "잘렸다" 는 표시와 함께 온다 — 따옴표·개행이 많은 문서는 그만큼 일찍 걸린다. 서버가 열지 않기로 확인 대기 중인 큰 파일이라도 이 채널은 그 상한 안에서 읽어 보낸다 — 그 확인은 그리는 쪽의 물음이지 읽기 권한의 경계가 아니다. client 의 markdown plugin 이 아직 안 떴으면 그 자리는 plugin 을 기다리다 뜬 뒤 문서로 채워진다. 연결이 끊겨 재연결을 기다리는 동안 문서는 옛 원문 대신 끊김 화면을 보이고, 재연결되면 원문을 저절로 다시 받는다 — 원격 변경 신호로는 다시 받지 않는다는 규칙의 예외는 원문을 못 보여 주는 문서뿐이다([ADR-0268](../../adr/0268-a-disconnected-mirror-markdown-shows-the-disconnect-and-reconnect-refetches.md)).
  - **placeholder 로 남는 비-터미널 surface 의 mirror 불가는 기술적 미구현이지 보안상 의도적 배제가 아니다.** attach 로 나가는 콘텐츠 전체는 이미 SSH+loopback 연결 경계 신뢰 모델([ADR-0004](../../adr/0004-ipc-transport-tcp.md), [attach-behavior "IPC 표면"](../../dev-guide/attach-behavior.md#ipc-표면-attach))에 위임돼 있다 — mesh mirror 가 bundled 화이트리스트 밖 kind 나 서드파티 plugin 으로 확장되더라도, 콘텐츠 종류가 다르다는 이유로 별도 권한 게이트를 새로 만들 근거는 없다([dev-guide/plugin-permissions](../../dev-guide/plugin-permissions.md) 참고).

### 화면 동기화

attach 성립 직후 서버가 현재 화면을 **1회 스냅샷**으로 push 하고, 이후 변화는 delta 로 흐른다. client 는 PTY 없는 mirror 터미널에 바이트를 먹여 같은 grid 를 재구성한다. 프로토콜·mux 상세는 → [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md).

workspace mirror 의 **탭 제목**은 스냅샷 pane JSON 의 tab `name`(원격 `Pane::to_attach_json` 이 `Tab::display_name()` 으로 항상 채운다)을 그대로 쓴다. `name` 이 빠진 비정상 스냅샷이거나 트리가 비어 placeholder tab/pane 을 합성할 때만 로컬 번역값 `attach.tab_title_fallback` 을 제목으로 쓴다 — 방어 폴백이지만 사용자 표면이라 하드코딩하지 않는다([ADR-0106](../../adr/0106-non-widget-user-strings-go-through-i18n.md)).

**그리드 크기는 client 가 구동(client-driven)** — mirror 의 cols×rows 는 그것을 띄운 **로컬 pane 크기**를 따르고, 원격 PTY 를 그 크기로 reflow 시킨다(ADR-0045). "remote authoritative" 는 메커니즘으로만 남는다: 원격 PTY 가 실제 크기의 단일 진실원이라 콘텐츠 래핑(reflow)을 담당하고, 그 확정 크기를 client 에 되돌린다. client 는 **의도를 밀고(요청)** 원격은 **결과를 확정(echo)** 한다. 구현:

- mirror 는 detached 터미널(PTY 없음)이라 로컬 레이아웃 리사이즈 스윕(`Core::resize_all_terminals` / `AppState::resize_all`)이 detached 터미널을 로컬에 직접 적용하지 않고, 목표 grid 를 forward 큐에 넣는다. `about_to_wait` 에서 **`StreamControl::ClientResize`**(client→server) 로 원격에 요청한다.
- 서버가 그 surface 의 **실제 원격 PTY 를 요청 크기로 resize**(reflow)한다(holder 만 구동 가능 — 배타 점유라 구동자는 항상 유일). 서버가 **GUI 인스턴스**(창 보유)면 그 host 창의 레이아웃 sweep(`Core::resize_all_terminals`)이 **hard-점유 surface 를 skip**(`is_hard_occupied`)해 자기 창 grid 로 되돌리지 않는다 — skip 이 없으면 host 창이 client-driven grid 를 덮어써 mirror 가 host 창 크기에 고정된다(레터박스). headless 서버는 창이 없어 무해하지만 GUI-hosted 서버엔 필수. detach 시 원복.
- 원격 grid 가 실제로 바뀌면 서버가 기존 **`Control` 프레임(`StreamControl::Resize`, server→client)** 으로 확정 cols/rows 를 통지하고, client 가 그 echo 로만 mirror 를 리사이즈한다. → 로컬을 낙관적으로 먼저 바꾸지 않아(원격 reflow 전 잘못된 grid 재생 방지) desync 가 없다.
- 렌더러는 mirror 의 실제 grid 크기로 셀을 pane 좌상단에 배치한다. mirror 가 pane 크기로 reflow 되므로 pane 을 채운다(과거의 80×24 좌상단 소영역 + 배경 레터박스는 사라진다). 초기 attach 순간(원격 기본 80×24 → 첫 forward reflow)에는 약 1 RTT 의 짧은 깜빡임이 있을 수 있다.

`StreamControl` 은 `event` 태그 기반 확장 enum 이라, 새 이벤트도 새 `StreamTag` 없이 variant 로 추가된다. 구버전 서버는 `ClientResize` 를 무시하므로(전방호환) 기존 remote-authoritative 동작으로 graceful degrade 한다. 같은 이유로 구버전 client 는 `Attention` 프레임을 파싱하지 못해 조용히 무시하고, 구버전 서버는 그 프레임을 애초에 보내지 않는다 — 어느 쪽도 세션을 깨지 않고 attention 표시만 빠진다. **다만 그 무시가 공짜일 때만 안전하다** — 놓쳐도 잃을 것이 없는 이벤트에만 성립하는 성질이고, 아래 `Loss` 가 그 예외라 무시가 곧 "손실 없음" 으로 읽히지 않도록 **client 의 명시 선언으로 게이트**한다.

Control 채널을 흐르는 server→client 상태 push 는 현재 네 종류다 — `Resize`(확정 grid), `Activity`(busy/idle), `Attention`(주의 환기), `Cwd`(surface 의 현재 폴더). 넷 다 델타가 아니라 **멱등 상태**이고, 서버가 매번 자기 live 상태에서 재-diff 하므로 **프레임이 유실돼도** 다음 tick 에 자동 수렴한다(client ack 없음). 수렴이 보장되는 축은 이 wire 유실 하나뿐이다 — client 가 자기 로컬 상태를 직접 바꾸면 서버 값은 그대로라 재-push 가 없다. 그래서 attention 은 미러가 자기 값을 바꾸는 두 축을 각각 다르게 다룬다: **발동**은 미러에서 아예 일어나지 않게 막고, **해제**는 `ClientAttentionClear`(client→server) 로 소유 인스턴스에 되돌린다. 상세는 [attach-behavior "주의 환기(attention) 전파"](../../dev-guide/attach-behavior.md#주의-환기attention-전파).

client→server 요청 채널은 `ClientResize`(geometry 구동), `ClientAttentionClear`(주의 환기 해제 edge), `StructuralOp`(구조 변경 forward), `MeshContext`/`MeshInput`/`MeshFullResendRequest`(mesh mirror), 그리고 원격 스크린샷 캡처 업로드([remote-screenshot-clipboard](../remote-screenshot-clipboard/index.md)) · 원격 디렉토리 조회(native file picker, [ADR-0053](../../adr/0053-native-file-picker-remote-attach-channel.md)) · git 조회 · markdown 원문 조회다. 이들은 **점유한 연결 자체가 권한**(ADR-0040 하드 점유)이라는 같은 원칙을 쓰되 검증 지점이 두 갈래다 — 앞의 네 종류는 요청에 실린 **anchor 의 holder 인지**를 직접 확인하고(`workspace_holder(ws) == client`), 캡처·디렉토리·git·markdown 원문 조회 네 종류는 그 client 가 점유한 워크스페이스를 가진 engine(`client_holds_workspace`)을 찾아 그 engine 안에서 처리한다 — 인가되는 집합이 그 engine 전체라는 뜻이다([ADR-0255](../../adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md)). 네이티브 bulk 파일 전송([ADR-0054](../../adr/0054-remote-filesystem-native-over-attach-stream.md))은 이 모델 **밖**이다 — 전용 연결이라 holder 가 아니고, 결속 workspace 를 **소유한** engine 으로 라우팅한 뒤 commit 시점의 finalize 가 인가를 확인한다.

### 모드

- **mirror-dump**(`--dump-after <ms>`): N ms 출력을 수집해 grid 를 재구성하고 텍스트를 stdout 으로 출력 후 종료. GUI 없이 attach 파이프라인을 검증하는 경로.
- **raw 브리지**(`--raw`): stdin↔stdout passthrough(detach 키 `Ctrl+\`). **surface 단위만** — workspace 모드는 다중 터미널이라 불가.
- **단발 입력**(`--send <str>`): attach 직후 1회 입력(escape 디코딩). workspace 모드는 `--send-to <remote_surface_id>` 로 대상 지정.

### GUI mirror

`tasty remote attach --into-gui --target-port <원격포트> --workspace <원격ws>` → 이 명령을 받은 **로컬 GUI 인스턴스**가 client 가 되어 원격 워크스페이스를 mirror 로 재구성한다(`attach.into_gui`). mirror Workspace 는 일반 워크스페이스로 사이드바에 노출되되 **이름과 subtitle 사이 별도 줄의 하늘색 "REMOTE" pill**(`>_→` glyph 포함; collapsed 레일은 아바타 우하단 하늘색 corner chip)로 로컬과 구분(`Workspace.mirror`). status dot 은 실행상태(running/idle) 전용이며 mirror 색을 싣지 않는다 — 원격 origin 은 별도 시각 축(디자인 `workspace-mirror-fg`, notif=우상단 / attached=둘레 ring 과 채널 분리). mirror 콘텐츠(grid) 갱신은 원격 출력이 올 때 즉시, 3초 tick 은 backstop. 실행상태(status dot 의 초록/회색)는 별도 채널로, 원격이 1Hz 로 자신의 busy 상태를 계산해 attach 스트림으로 forward 하고(mirror 터미널은 로컬 PTY 가 없어 스스로 계산할 방법이 없다 — 이 forward 가 유일한 소스) client 가 그 값을 반영한다. 메커니즘 상세는 [dev-guide/attach-behavior "활동(busy) 상태 전파"](../../dev-guide/attach-behavior.md#활동busy-상태-전파).

**주의 환기(attention)도 같은 방향의 별도 채널이다** — 원격이 자기 surface 의 attention(작업 완료 / 응답 필요)을 같은 1Hz tick 에 `StreamControl::Attention{surface_id, kind}` 로 forward 하고 client 가 자기 `AttentionStore` 에 반영한다. attention 의 진실 원천은 **surface 를 소유한 인스턴스**다: producer(완료 IPC/CLI, Claude 플러그인 훅, OSC 133 명령 완료, toast)가 전부 PTY 가 있는 쪽에서 돌고, 특히 `needs_input` 은 서버 훅에서만 나와 미러가 스스로 만들 수 없다. 반영된 값은 로컬 attention 과 **같은 store** 에 들어가므로 미러 워크스페이스에서도 사이드바 개수 배지·surface 테두리·탭 제목 색이 그대로 동작한다. 미러는 자기 판단으로 attention 을 만들지 않는다 — 미러 터미널도 서버 바이트를 파싱해 OSC 133 D·Bell 등을 발화하지만 `raise_attention` 이 mirror surface 를 걸러내므로, 이 push 가 미러 attention 의 **유일한 소스**다(알림 패널 아이템·토스트는 억제 대상이 아니라 그대로 뜬다). 반대로 **해제는 미러에서 서버로 되돌아간다** — 미러 사용자가 그 surface 를 확인(실-포커스 또는 미러 로컬 알림 읽음)해 레코드가 실제로 지워지면 `StreamControl::ClientAttentionClear` 1 회가 서버로 가고 서버 레코드도 제거된다. 미러 사용자의 행동은 서버의 해제 경로를 발동시킬 수 없어, 이 전달이 없으면 서버 배지에 해제 주체가 아예 없다. 메커니즘 상세는 [dev-guide/attach-behavior "주의 환기(attention) 전파"](../../dev-guide/attach-behavior.md#주의-환기attention-전파), 기능 문서는 [features/surface-highlight](../surface-highlight/index.md#원격-attach-mirror-로의-전파-serverclient).

**surface 의 현재 폴더(cwd)도 같은 방향의 채널이다** — 원격이 점유 surface 의 cwd 를 같은 1Hz tick 에 `StreamControl::Cwd{surface_id, cwd}` 로 보낸다. mirror 터미널은 로컬 셸이 없어 원격 셸이 OSC 7 을 쏠 때만 cwd 를 알 수 있었는데, 원격은 셸 프로세스를 직접 조회할 수 있어 어떤 셸이든 값이 온다. 그 값은 **원격 경로**로 표시되어 로컬 파일시스템 연산(새 로컬 워크스페이스의 시작 폴더, 로컬 git 브랜치 조회, preset 저장)에는 쓰이지 않고, 원격에서 실행되거나 원격을 조회하는 소비자(예: mirror 에서 연 파일 피커의 시작 폴더)만 쓴다. 규칙과 근거는 [surface-cwd invariant §3-2](../../architecture/invariants/surface-cwd.md) · [ADR-0267](../../adr/0267-mirror-surface-cwd-is-pushed-by-the-server.md), 메커니즘은 [dev-guide/attach-behavior "surface cwd 전파"](../../dev-guide/attach-behavior.md#surface-cwd-전파).

**서버가 프레임을 버렸다는 통지(`Loss`)는 위 넷과 성질이 반대다** — 위 넷은 멱등 상태라 유실돼도
다음 tick 에 수렴하지만, PTY 출력처럼 수렴하지 않는 것이 유실되면 mirror 는 끊긴 데이터를 연속으로
계속 그린다. 그래서 서버는 그 연결에서 버린 프레임 수를 `StreamControl::Loss{frames}` 로 되돌려
준다. **받겠다고 선언한 연결에만** 간다(`ClientLossNotify` — GUI mirror · GUI bulk 전송 · CLI
`tasty attach` 가 선언한다) — 선언 안 한 peer 의 바이트 열은 무변경이고, 이 통지를 못 읽는
client 에게 "무시" 는 "손실 없음" 과 구별되지 않기 때문이다. 통지를 받은 client 는 **다시
attach 해 화면을 새로 받는다** — 서버가 터미널 snapshot 을 만드는 자리가 attach 하나뿐이라서다.
GUI mirror 는 "원격 화면 일부를 놓쳤다" 경고 toast 를 띄운 뒤 스스로 다시 붙고(수동 attach 도
같다), 그 사이 mirror 터미널의 출력 위치 표지(stream)가 바뀌어 공백을 건너 읽는 에이전트는
불일치 오류를 받는다. CLI 는 stderr 로 알리고 다시 붙으며, bulk 전송은 결과를 모르므로 중단으로
끝난다. 서버 전체의 손실 누계와 지금 쌓인 양은 `tasty list pressure` 의 `stream_push` 로
읽는다([telemetry](../telemetry/index.md)). 종류별 계약·순서·한도는
[dev-guide/attach-behavior "통지를 받은 client 가 하는 일"](../../dev-guide/attach-behavior.md#통지를-받은-client-가-하는-일-재동기화-계약),
근거는 [ADR-0334](../../adr/0334-a-dropped-stream-frame-is-told-to-the-clients-that-asked-for-it.md) ·
[ADR-0400](../../adr/0400-attach-loss-is-resynced-per-connection-with-the-strongest-contract-it-carries.md).

### mirror 워크스페이스 내 구조 변경

mirror 워크스페이스는 "통째로 원격" 인 원격 워크스페이스의 뷰다 — 입력(키스트로크)은 이미 원격 PTY 로 forward 된다. 그 안에서의 **구조 변경**(surface/pane split · 새 탭 · 닫기 · 탭 순서 변경 · 닫은 항목 복원)을 로컬에서 실행하면 로컬 셸 PTY 가 mirror 에 섞여 "workspace 전체가 remote" 불변식을 깬다. 따라서 mirror 워크스페이스 구조 변경은 **로컬에서 실행하지 않고**, 대신 **원격 인스턴스에서 실행되도록 forward** 한다.

- **판별·로컬 차단**: 단일 mutate 진입점 `Core::apply` 가 대상 워크스페이스가 mirror 면 로컬 실행을 거부(`MirrorStructuralBlocked`, `CoreState::mirror_workspace_index_for_structural`) — 로컬 트리/PTY 는 절대 바뀌지 않는다. `Core::apply` 를 우회하는 UI 직접 조작(`AppState::add_tab`/`add_kind_tab`/`close_active_*`/`close_tab`, 탭 드래그·컨텍스트 메뉴 이동)은 `AppState::forward_mirror_structural` 가드가 로컬 실행 대신 대응 `StructuralOp`(new-tab→`NewTab`, close→`CloseSurface`/`CloseTab`/`ClosePane`, 순서변경→`MoveTab`; anchor = focused/대상 pane 의 로컬 surface id)를 같은 forward 큐(`pending_structural_forward`)에 직접 쌓아 원격으로 보낸다 — 로컬 차단만 하던 과거와 달리 `Core::apply` 경로와 동형으로 forward 된다.
- **forward (요청)**: `Core::apply` 가 로컬 차단과 동시에 그 구조 op 를 `StructuralOp` 로 만들어(anchor = **로컬** surface id) forward 큐에 넣고, App 이 `about_to_wait` 에서 drain 해 anchor 를 **원격 surface id 로 치환**한 뒤 attach stream 의 `StreamTag::Control`(`StreamControl::StructuralOp`)로 원격에 보낸다. attach 연결 자체가 hard 점유 holder 이므로 연결이 곧 구조 변경 권한을 증명한다(ADR-0040). op 는 **원격 surface id 로 anchor** 되어 원격이 자기 트리에서 pane/tab/workspace 를 resolve 한다 — client 는 surface 매핑만 보유하면 된다.
- **원격 실행**: 원격이 `StructuralOp` 를 수신(`StreamHub::pump_inbound` 분류)해 holder 를 검증한 뒤, IPC 핸들러가 부르는 것과 같은 도메인 실행 함수(split/tab.create/tab.close/tab.move/pane.close/surface.close — `core::structural_exec`)로 실제로 실행한다(원격 ws 는 mirror 가 아니라 실제 PTY 를 spawn). 대응하는 도메인 실행 함수가 없는 셋(convert · move-surface · 닫은 항목 복원)은 원격이 `Core::apply` 를 직접 부른다. 결과는 `StreamControl::StructuralResult{op_id, ok, reason?}` 로 회신.
- **점유 상속 (필수 불변식)**: forward 로 원격에 **새로 생긴 터미널은 그 workspace 의 hard 점유를 상속**한다("workspace 전체가 remote" 유지 — ADR-0040 은 점유가 surface 생성 방식과 무관함을 못박는다). 점유는 attach 시점 멤버 스냅샷으로 끝나는 게 아니라, 구조 변경으로 늘어난 멤버까지 확장돼야 한다. `execute_forwarded_structural_op` 이 added 터미널을 `OccupancyRegistry::add_workspace_member` 로 `surface_locks`(→`is_hard_occupied`: 서버 입력차단·resize sweep skip·readonly) + `surface_to_workspace`(→`feed_attached_workspace_input`/`apply_attached_workspace_resize` 의 holder 검증)에 같은 holder 로 등록한다. 이 등록이 빠지면 새 surface 가 비점유로 남아 (1) host 창 sweep 이 자기 grid 로 되돌리는 레터박스 (2) 점유 미표시 (3) client 입력·resize 거부가 발생한다.
  - **생성 경로도 차단 대상에 포함된다(정책 변경, [ADR-0060](../../adr/0060-block-terminal-spawn-into-hard-occupied-workspace.md)).** 과거엔 "새 리소스를 추가만 하는 생성 경로는 홀더의 화면을 안 흔드니 차단 대상이 아니다"였으나, 그 논거는 홀더 관점만 다뤘다 — spawn 을 호출한 로컬 agent 자신이 그 직후 자기 결과물(방금 만든 surface)에 입력을 못 넣게 되는 부작용(`terminal.tell`/`surface.send` 가 원인 불명의 `"Surface not found"` 로 실패)은 검토되지 않았다. `tasty claude/codex spawn` 이 실제로 타는 경로는 `pty.attach_surface` 가 아니라 **`terminal.spawn`**(→ `tab::handle_tab_create` → `apply_create_tab`)이다 — 위 문서 서술은 과거 오기였다. 그래서 차단 대상은 `terminal.spawn` 을 포함한 7종이다(아래 절 참고). `pty.attach_surface`(headless PTY → Surface 승격, `AdoptTerminal`)도 같은 `tap_new_workspace_member` 후처리를 타 이론상 동일한 부작용을 가질 수 있으나, 이번 변경의 확인된 필수 스코프는 `terminal.spawn` 로 한정했다 — `pty.attach_surface` 가 가드 미적용 상태로 남아있는 것은 알려진 갭이다(재검토 조건은 ADR-0060 참고).
  - hard-occupied workspace 에 새로 생긴 surface 는(차단을 통과한, 즉 holder 본인의 forward 경로로 생긴 surface 는) 위와 동일하게 `OccupancyRegistry::add_workspace_member` + `tap_surface_for_stream` 이 실행돼야 한다 — 실행되지 않으면 PTY/화면버퍼는 정상인데 attach client 로의 스트리밍만 시작되지 않아 그 tab 이 검정 화면으로만 보인다(스트림 tap 이 아예 안 걸린 상태). `CoreState::tap_new_workspace_member`(`src/core/attach_runtime.rs`)가 `apply_create_tab`(`src/core/impl_tab.rs`)/`apply_split_pane`/`apply_split_surface`(`src/core/impl_split.rs`)/`apply_adopt_terminal`(`src/core/impl_attach.rs`) 공통 후처리로 이를 수행한다 — `hub`/`client_id` 를 호출 체인에 새로 꿰지 않고, `OccupancyRegistry` 에 boot 시 주입된 notifier(`StreamHub`, `notify_detached` 와 동일 패턴)를 재사용한다.
- **실패 회신**: 원격이 op 를 실패 처리(대표적으로 **원격에 등록되지 않은 plugin surface kind** — 원격의 kind 레지스트리가 그 호스트에서 생성 가능한 kind 의 authority)하면 `ok:false`+`reason` 을 회신하고, client 가 실패 toast(`attach.toast.mirror_structural_forward_failed`)를 띄운다. 단 에이전트 origin intent(`markdown.navigate` · `file_handler.dispatch`(`origin_surface_id` 없이)의 새 탭)가 forward 한 op 의 실패는 toast 없이 warn 로그로만 남는다([ADR-0503](../../adr/0503-an-agent-intents-apply-failure-goes-to-the-log-not-a-user-toast.md)). 요청/응답이라 실패 시 로컬·원격 어느 쪽도 구조가 바뀌지 않는다.
- **역반영 (성공 시)**: 성공한 forward 로 원격에 생긴/사라진 surface 를 mirror 트리에 반영한다. 원격이 실행 후 워크스페이스 **전체 트리+surfaces** 를 `StreamControl::StructuralDelta` 로 push(`StructuralResult` 성공 회신 **직후**)하고, client 가 이를 받아 mirror 트리를 증분 재구성한다. survivor(이미 mirror 로 존재하는 원격 surface)는 **기존 mirror 터미널을 그대로 유지**(scrollback 보존)하고, 새 원격 surface 만 새 mirror 로 추가, 사라진 surface 는 제거한다. 최소 증분(surface 별 diff) 대신 full-tree 재동기화를 쓰는 이유: client 는 surface 매핑만 보유한다는 불변식을 지키면서 split·새 탭·닫기(cascade)·탭 이동을 균일하게 반영하기 위함. pane 상위 배치(direction/ratio)도 핸드셰이크와 동일한 트리 필드로 정확히 승계된다.
- **focus 는 원격이 아니라 client 가 보존한다**: 위 역반영 트리가 담는 focus(어느 pane/탭이 focused 인지)는 원격 값 그대로다 — 순수 pane/탭 전환은 forward 되지 않으므로 원격의 focus 는 사실상 워크스페이스 생성 시점에 고정돼 있다. 매 역반영마다 이 고정값으로 로컬을 통째로 교체하면 사용자가 mirror 안에서 실제로 보고 있던 pane/탭이 매번 첫 pane/첫 탭으로 튀는 문제가 있었다. client 는 교체 직전 로컬 focus 위치를 remote surface id 기준으로 기억해뒀다가 교체 직후 그 위치로 되돌린다.
  - **단, "무관한 delta 로부터 옛 focus 를 지키는 것"과 "이번 조작 자체의 결과로 focus 가 움직여야 하는 것"은 다른 문제다.** 사용자가 mirror 안에서 직접 새 탭/split 을 만들면 옛 focus 복원만으로는 새로 생긴 리소스로 focus 가 전혀 안 옮겨가고, focus 중인 surface 자체를 닫으면 복원 대상이 사라져 원격의 고정값(대개 워크스페이스 첫 pane/첫 탭/첫 surface)으로 튀어 버린다. 그래서 client 는 이 op 이 **실제 사용자 GUI 조작**(단축키/버튼/컨텍스트 메뉴 — IPC/CLI/에이전트 호출은 제외)이었는지를 forward 시점부터 태그(`user_triggered`)해두고, 성공 회신에 상관지어(op_id) 뒤따르는 delta 적용에서: 새 탭/split 이면 새로 생긴 surface 로 focus 를 옮기고, close 로 옛 focus 복원이 실패했으면 닫히기 **전** 캡처해둔 인접 후보(같은 tab 의 형제 surface, 또는 같은 pane 의 인접 탭)로 fallback 한다. IPC/CLI 로 같은 조작을 했을 때는 이 태그가 항상 꺼져 있어 focus 가 그대로 안 움직인다(회귀 없음 — "포커스 독립성" 유지).
  - 메커니즘 상세는 [dev-guide/attach-behavior "focus 보존"](../../dev-guide/attach-behavior.md#mirror-구조-변경-forward).
- **서버 쪽에서 바뀐 구조도 역반영한다**: 원격 셸이 끝나(`exit`) 원격에서 탭/pane 이 닫히면, forward 가 없었어도 원격이 같은 `StructuralDelta` 를 push 해 mirror 에서도 곧바로 사라진다. 원격에서 로컬 경로로 워크스페이스에 편입된 surface 도 같은 메시지로 mirror 에 나타난다(그 surface 의 화면 스냅샷보다 트리가 먼저 간다). 새 메시지는 없다 — 역반영 메시지를 forward 성공 말고도 보낼 뿐이다([ADR-0481](../../adr/0481-a-server-side-structure-change-reaches-the-holder-as-a-delta.md)). 그 셸이 워크스페이스의 마지막 surface 였으면 아래 강제 detach 와 같다.
- **역반영 대신 강제 detach (workspace 자체가 cascade 로 사라지는 경우)**: workspace 의 **마지막 surface** 를 forward `CloseSurface` 로 닫으면, 원격의 `close_case_workspace`("Case 4: last pane in workspace")가 트리 일부가 아니라 **workspace 자체**를 통째로 purge 한다 — 이 경우 되돌릴 delta 자체가 없다. `execute_forwarded_structural_op`(`src/core/attach_runtime.rs`)이 실행 후 워크스페이스를 재조회해 실패를 확인하면, delta 재구성을 시도하는 대신 `force_detach_workspace` 를 호출해 holder 를 강제 detach(Control `force_detached` + `Detach`)시키고 `OccupancyRegistry` 의 lock 도 함께 정리한다. client 는 이를 일반 force-detach 와 동일하게 처리해 mirror 를 정리한다 — 재attach 없이도 즉시 반영된다. 메커니즘 상세는 [dev-guide/attach-behavior "점유 레지스트리"](../../dev-guide/attach-behavior.md#점유-레지스트리-occupancyregistry).
- **닫은 항목 복원(`Ctrl+Shift+T`)도 forward 대상이다 ([ADR-0264](../../adr/0264-mirror-restore-closed-item-runs-on-the-remote.md))**: mirror 를 보는 중에 누르면 **원격에서** 닫혔던 탭이 되살아나 mirror 에 나타나고, 그 안의 입력은 원격 PTY 로 간다. 복원은 새 PTY spawn 이고 스냅샷의 스크롤백은 서버 디스크에 있으므로 서버만 실행할 수 있다. **원격에 복원할 것이 없어도 로컬 항목을 대신 되살리지 않는다** — 안내 toast 만 뜨고 로컬 스택은 그대로 남아, 로컬 워크스페이스로 돌아가 같은 키를 누르면 그때 복원된다. 즉 **스택이 둘이고 보고 있는 워크스페이스가 어느 쪽을 쓸지 정한다.**
  - **원격에서 누른 복원은 그 워크스페이스 것만 꺼낸다**: 서버의 복원 스택은 워크스페이스로 스코프돼, mirror 사용자의 복원이 서버 앞에 앉은 사용자가 방금 닫은 다른 워크스페이스의 탭을 가져가지 않는다. 반대 방향은 스코프를 걸지 않는다 — 서버 로컬 복원은 지금까지처럼 전역 LIFO 다. forward 된 close 가 서버 자신의 트리에서 탭을 없애므로, 그 기계 앞의 사용자에게도 undo 가 남아야 하기 때문이다.
  - **mirror 안에서 손으로 닫은 것은 되돌려진다**: 원격 사용자의 손으로 닫힌 탭/pane/surface 는 서버의 복원 스택에 남는다.
  - **에이전트가 닫은 것은 어느 쪽 복원 스택에도 안 남는다**: 클라이언트의 에이전트가 CLI/IPC 로 mirror 를 닫으면 그 close 도 원격으로 forward 되지만, forward 가 "에이전트의 요청" 이라는 표시를 싣고 서버는 그것을 복원 스택에 넣지 않는다. 그렇지 않으면 서버 앞 사용자의 복원 단축키가 에이전트가 닫은 것을 되살린다 — 사용자의 닫은 항목 히스토리는 사용자가 닫은 것만 담는다([identity](../../identity.md) 원칙 1). 표시가 없는 forward(이 표시 이전의 클라이언트)는 사용자 조작으로 읽는다([ADR-0480](../../adr/0480-a-forwarded-close-carries-who-asked-for-it.md)).
  - **복원된 탭의 옛 스크롤백은 mirror 에 전량 오지 않는다**: 서버 쪽 터미널은 스크롤백까지 되살아나지만, mirror 는 기존 규약대로 보이는 화면 1 회 스냅샷 + 이후 변경분만 받는다. 원격 윈도우에서 보면 스크롤백이 그대로 있다.
- **mirror 워크스페이스 자체를 닫는 것**은 로컬 mirror 뷰를 걷어내는 정당한 로컬 동작이라 차단·forward 대상이 아니다.
- **`terminal.spawn` 은 forward 대상이 아니라 거부 대상이다 ([ADR-0086](../../adr/0086-reject-terminal-spawn-into-mirror-workspace.md))**: 위 forward 는 fire-and-forget 이라 응답이 원격에서 생긴 리소스의 id 를 담지 않는다. `tasty claude/codex/terminal spawn` 이 타는 `terminal.spawn` 은 그 id 를 **동기로** 받아 child registry 등록·soft 점유·후속 command 주입까지 이어가야 하므로 이 응답 모델 위에 얹힐 수 없다. 막지 않으면 로컬은 에러를 돌려주는데 forward 큐는 IPC 응답과 무관하게 드레인되어 **원격에만 탭이 남는 고아**가 생긴다. 그래서 mirror 워크스페이스를 대상으로 한 `terminal.spawn` 은 tab/surface 를 하나도 만들지 않고 `invalid_params` 로 즉시 거부하며, 메시지에 mirror 사유와 대안(다른 워크스페이스 사용 / 원격 인스턴스에서 직접 spawn)을 담는다. 나머지 구조 변경은 mirror 에서도 그대로 forward 된다 — 거부는 `terminal.spawn` 한 method 에만 적용된다.

**현재 범위**: surface split / pane split / 새 탭 / surface·tab·pane 닫기 / 탭 이동 / 닫은 항목 복원 / surface convert(kind 변환, `markdown.navigate`/`image.open`/host convert 팝업이 모두 이 경로를 탄다 — 변환 결과의 cwd 는 op 의 `cwd` 필드로 전달되고, 비어 있으면 원격이 대상 surface 의 실제 PTY 에서 직접 resolve 한다. [surface-cwd invariant §3-1](../../architecture/invariants/surface-cwd.md)) / surface 이동(move-surface)이 forward 대상이며, 성공 시 원격 실행 결과가 mirror 트리에 역반영된다. move-surface 는 **source/target 이 같은 mirror workspace 안에 있을 때만** forward 된다 — 로컬(비-mirror) workspace 와의 경계를 넘는 이동은 로컬 전용 surface_id 를 원격에 그대로 보내는 꼴이 되어(원격 트리의 무관한 surface 와 id 가 우연히 겹칠 위험) 여전히 로컬 차단 toast(`mirror_structural_blocked`)를 유지한다. `Core::apply` 를 우회하는 UI 직접 경로(탭 드래그 등)도 아직 차단만 한다.

### 서버(피점유)측 비-holder 구조 변경 차단

위 절이 다루는 것은 **client(점유 holder)측** 구조 변경이 원격(서버)에서 실행되도록 forward 되는 경로다. 반대 방향 — **서버 자신이 hard-occupied 상태인 자기 workspace 에 대해, 점유 holder 가 아닌 제3자(서버 로컬 IPC/CLI/agent)가 직접** 구조 변경 IPC(`split`/`tab.create`/`terminal.spawn`/`pane.close`/`tab.close`/`tab.move`/`surface.close`/`markdown.navigate`/`image.open`)를 호출하는 경우도 배타성 위반이다 — [ADR-0040](../../adr/0040-occupancy-soft-hard-tiers-agent-occupant.md) 이 정의하는 hard 점유의 배타성은 입력(`apply_send_to_surface`)·resize(`resize_all_terminals`)뿐 아니라 구조 변경까지 적용돼야 한다.

- **차단 대상**: 위 IPC 9종을 **일반 IPC/CLI 진입점**(서버 로컬 호출)으로 직접 호출하고, 대상 pane/tab/surface(`terminal.spawn` 은 `pane` 오버라이드까지 반영해 확정된 **최종 pane**) 가 hard-occupied workspace 에 속한 경우. 요청은 `invalid_params` 에러(안내 문구: "점유 중이라 불가능, 다른 workspace 사용")로 거부되고 트리는 전혀 바뀌지 않는다. `terminal.spawn`(`tasty claude/codex spawn` 이 호출) 은 [ADR-0060](../../adr/0060-block-terminal-spawn-into-hard-occupied-workspace.md) 으로 이 목록에 추가됐다 — spawn 자체는 성공 응답을 주면서 그 결과물(새 surface)이 즉시 같은 hard lock 을 상속받아, spawn 을 호출한 쪽조차 자기 결과물에 입력을 못 넣게 되는 부작용을 막기 위함. 다만 `terminal.spawn` 의 판정만 다른 8종과 **집행 지점이 다르다** — 나머지는 라우터 가드(`hard_occupied_structural_guard`)가 method 파라미터로 대상을 찾지만, `terminal.spawn` 의 실제 대상은 `pane` 오버라이드까지 반영해 확정된 pane 이라 그것을 아는 핸들러 안(`spawn_target_guard`)에서 건다([ADR-0086](../../adr/0086-reject-terminal-spawn-into-mirror-workspace.md)). 덕분에 `--workspace <비점유 ws>` + `--pane <hard-occupied ws 의 pane>` 조합으로 이 가드를 우회하던 구멍도 함께 닫혔다. `markdown.navigate`/`image.open` 은 convert 진입점이 kind 별로 흩어져 있어 이 두 method 만 커버한다(완전하지 않음 — host 범용 convert 팝업은 `state.dispatch_intent` 를 직접 호출해 이 IPC 라우팅 자체를 안 타고, 향후 새 kind 가 자기 전용 convert 진입 method 를 추가하면 이 목록에 없는 한 가드가 적용되지 않는다).
- **차단 대상이 아닌 경우(중요)**: 점유 holder 본인이 mirror 안에서 실제로 만든 구조 변경이 위 forward 경로로 서버에 도달해 실행되는 것은 **정상 동작이며 이 차단의 대상이 아니다** — "attach 연결 자체가 그 workspace 에 대한 구조 변경 권한을 증명한다"는 forward 모델(위 절)을 그대로 유지한다. `terminal.spawn` 은 forward 대상 IPC(split/tab.create/tab.close/tab.move/pane.close/surface.close/convert/move-surface 8종)에 포함되지 않으므로 이 예외와 무관 — 가드 추가가 holder 의 정당한 forward 요청을 막는 회귀는 없다.
- **알려진 갭**: `pty.attach_surface`(`AdoptTerminal`) 경로도 같은 `tap_new_workspace_member` 후처리를 타 이론상 `terminal.spawn` 과 동일한 부작용을 가질 수 있으나, 아직 이 가드 대상에 포함되지 않았다(재검토 조건은 ADR-0060 참고).
- **차단 근거**: [`docs/identity.md`](../../identity.md) 원칙1(에이전트 행동의 부수효과가 사용자 상태에 닿지 않아야 함) — 서버 로컬에서 만든/닫은/옮긴 탭이 점유 client 화면에 통지 없이 편입/소멸/재배치되면, 원격 사용자가 보고 있는 화면에 자신이 하지 않은 변화가 일어나는 셈이라 이 원칙을 위반한다.
- **메커니즘**: [dev-guide/attach-behavior "서버 로컬(비-holder) 구조 변경 차단"](../../dev-guide/attach-behavior.md#서버-로컬비-holder-구조-변경-차단).

### 자동 매핑

`tasty set workspace --id <id> --ssh-profile <name> --remote-workspace <N>`(또는 `--ssh <user@host>`)로 로컬 워크스페이스에 원격 대상을 선언적으로 매핑한다(`Workspace.attach_mapping`, 슬롯 파일 영속). 매핑된 워크스페이스를 **활성화하면** 호스트가 자동으로 프로필 resolve → SSH 터널 → GUI mirror 를 띄운다. `remote_workspace` 가 None 이면 skip(ID 명시 필요), 이미 attach 중이면 재트리거 안 함. 자동 attach 는 mirror 를 *추가*만 하고 포커스/active 전환을 강제하지 않는다([포커스 독립성](../../identity.md)).

**연결이 끊기면 mirror 는 살아있는 채로 자동 재연결을 시도한다**: heartbeat TTL 만료·force-detach 등 원격발 disconnect 로 앵커(매핑된 워크스페이스) 세션이 끊기면, mirror workspace/터미널을 걷어내는 대신 `Reconnecting` 상태로 전이해 살려두고(`src/app/attach_client.rs::enter_reconnecting`), 지수 백오프(0.5s→30s, ±20% jitter)로 재연결을 자동 시도한다(`src/app/auto_attach.rs::maybe_trigger_reconnect`). 재연결에 성공하면 살아있던 surface 는 scrollback/local id 를 그대로 유지한 채(survivor mapping, `merge_survivor_mapping`) 연결만 새로 맺는다 — 사용자가 아무 조작을 하지 않아도(그 워크스페이스를 계속 보고 있어도) 백그라운드에서 재시도가 진행된다. 사용자가 그 워크스페이스로 전환해 돌아오면(엣지) 백오프 대기 없이 즉시 한 번 더 시도한다. 다른 클라이언트가 여전히 그 원격 워크스페이스를 점유 중이면(`already_attached`) 지수 증가 없이 30초 간격으로 계속 대기하고, 20회 시도 후에도 실패하면 자동 재시도를 멈추고 안내 toast 를 띄운다(단, 그 워크스페이스를 왕복하는 수동 재시도는 계속 유효). 앵커가 없는 임시 mirror(IPC `remote.attach` 등)는 이 대상이 아니라 기존처럼 즉시 정리된다. 상세: [`dev-guide/attach-behavior` "GUI 자동 재연결 스코프"](../../dev-guide/attach-behavior.md#gui-자동-재연결-스코프) / ["재연결 시 세션 상태 보존"](../../dev-guide/attach-behavior.md#재연결-시-세션-상태-보존).

### 원격 포트 발견 실패 진단

포트 발견(`ssh.rs::discover_remote_port`, auto fallback 체인 subcommand→file-unix→file-windows)이 전 단계 실패하면 `PortDiscoveryError` 로 원인을 4분류한다 — **원격 stderr 문자열 매칭에 의존하지 않는다**(원격 로케일에 따라 문자열이 달라져 신뢰 불가, 실측: 한국어 "그런 파일이나 디렉터리가 없습니다" / 영어 "No such file or directory"). 대신 ssh(1) exit code 로 로케일 독립적으로 판정한다:

- **SSH 연결 자체 실패**(exit 255, 시그널 종료, 로컬 ssh spawn 실패) — 네트워크/인증 문제.
- **원격 인스턴스 미실행**(그 외 비정상 종료 — 원격 명령까지는 도달했다는 확정 신호) — 관례 위치에 포트 파일이 없거나 `tasty port` 가 실패. 이 저장소가 다루는 attach 실패의 대다수를 차지하는 케이스.
- **포트 파싱 실패**(exit 0, stdout 을 포트 숫자로 못 읽음) — 연결·명령은 성공했지만 출력이 이례적(원격 tasty 버전 불일치 등).
- **타임아웃**(상한 안에 아무 답도 오지 않음) — 무응답 호스트(패킷이 조용히 버려지는 IP·방화벽 DROP·꺼진 머신), 보이지 않는 프롬프트 대기, 또는 체인 전체 예산 소진. 아래 "연결 시도 상한" 참고. `SshConnectionFailed` 로 접지 않고 따로 두는 이유는 ① 사용자가 취할 조치가 다르고(도달성/회선 점검 vs 인증·호스트키 점검), ② 타임아웃 kill 도 시그널 종료라 접으면 원격발 시그널 종료와 뭉개지기 때문이다.

각 분류는 `lang/{en,ko,ja}.toml` `[ssh.port_discovery]` 의 번역된 문구로만 노출된다 — 원격 raw stderr·내부 명령(`cat`/`type`)·포트 파일 경로는 에러의 `Display` 에 담기지 않고 생성 시점에 `tracing::debug!` 로만 로그된다(`PortDiscoveryError::detail()`). Auto 체인이 전 단계 실패하면 가장 확정적인 분류(타임아웃 > SSH 연결 실패 > 인스턴스 미실행 > 파싱 실패 순)를 대표 에러로 고른다 — 한 단계가 무응답이면 다른 단계의 "연결 실패" 는 그 타임아웃이 예산을 먹어 굶긴 결과일 수 있어 대표로 삼으면 오도한다 — 마지막 단계 에러만 남기면 정보량이 가장 적은 사유가 노출되던 문제를 막는다. 이 분류는 `ssh::discover_remote_port`/`remote_browse::resolve_endpoint` 를 공유 소비하는 모든 경로(GUI 원격 워크스페이스 추가 팝업, `tasty remote check`/`remote workspaces`/`tool attach`, IPC `remote.workspaces`/`remote.attach`, 자동 재연결)에 동일하게 적용된다.

### 연결 시도 상한 (no-hang)

포트 발견은 **무기한 블록하지 않는다.** 상한이 없으면 무응답 호스트에서 연결 수립이 OS 기본 SYN 재시도(리눅스 `tcp_syn_retries=6` ≈ 127초)에 맡겨지고, auto 체인은 그 대기를 3배로 곱한다. 상수는 `crates/tasty-ssh/src/lib.rs` 에 `pub const` 로 노출되며(소비자가 진행 표시/문구를 같은 값에 맞출 수 있게), 세 겹이 각각 다른 구간을 덮는다 — 근거는 [ADR-0070](../../adr/0070-port-discovery-timeout.md).

| 상수 | 값 | 덮는 구간 |
|------|-----|-----------|
| `SSH_CONNECT_TIMEOUT` | 10초 | ssh(1) `-o ConnectTimeout` — 연결 수립(TCP + 배너). ssh 가 홉마다 적용하므로 ProxyJump 다단도 홉별로 이 상한을 받는다. |
| `PORT_DISCOVERY_STEP_TIMEOUT` | 20초 | ssh 자식 프로세스 1개의 프로세스 레벨 상한. `ConnectTimeout` 이 못 보는 구간(인증 핸드셰이크, 원격 명령 실행, `BatchMode=no` 로 뜬 프롬프트 대기)까지. 넘기면 kill + wait 로 좀비 없이 거둔다. |
| `PORT_DISCOVERY_TOTAL_TIMEOUT` | 45초 | `discover_remote_port` / `detect_port_mode` **호출 1회 전체**. 각 단계는 `min(단계 상한, 남은 예산)` 만 받고 예산이 소진되면 남은 단계는 ssh 를 띄우지 않고 즉시 타임아웃 — auto 체인(3단계)이나 명시 `port_file`(`cat`→`type` 2회)에서 총 대기가 단계 수만큼 곱해지는 것을 막는다. |

무응답 호스트 실측: auto 체인 ~30초(3 × 연결 상한), 최악 45초. 상한은 **포트 발견/감지 경로를 공유하는 모든 소비자**에 동시에 적용된다 — GUI 원격 워크스페이스 추가 팝업, 도구 메뉴 > Remote connections 재감지, `tasty remote workspaces`/`remote check`/`tool attach`/`tool remote-profile detect`, IPC `remote.workspaces`/`remote.attach`/`remote.profile.detect`, 그리고 매핑 워크스페이스 활성화 시의 자동 attach 워커.

**사용자 지정이 기본값을 이긴다**: 프로필 `extra_options` 에 `ConnectTimeout=<초>` 를 직접 넣으면 그 값이 적용된다(ssh(1) 은 같은 키의 **먼저 나온 값**을 쓰고, tasty 는 기본값을 `extra_options` 뒤에 붙인다). 느린 회선/다단 ProxyJump 에서 상향하는 수단이다. 전체 예산(45초)은 프로필로 조정하지 않는다.

### 원격 생존 확인

`tasty remote check --ssh|--profile` — 원격 인스턴스가 *지금 살아있는지* 단발 판정. 포트 발견만으론 stale 포트 파일을 오판할 수 있어, 터널 수립 후 가벼운 IPC(`system.info`) 1회 응답까지 확인해야 alive(exit 0). 실패(거부/EOF/타임아웃)는 dead(exit≠0). 세 단계 모두 상한이 있어 무응답 호스트에서도 단발 판정이 성립한다 — ① 포트 발견은 위 "연결 시도 상한", ② 터널 ready-probe 5초, ③ IPC 프로브 5초(`remote_browse::PROBE_TIMEOUT`).

### 원격 워크스페이스 브라우징 (Browse)

`remote attach` 가 대상 workspace id 를 **미리 알아야** 동작하는 것과 달리, 브라우징은 그 id 를 **발견**한다 — attach 프로필/ssh 대상에 붙어 원격 인스턴스의 워크스페이스 목록(각 `id`/`name`/`pane_count`/`busy_count`/`attached`)을 받아온다. 흐름: 접속 스펙 resolve → (SSH 터널 or `127.0.0.1:PORT` loopback 직결) → 그 포트로 `workspace.list` + `attach.list` **2회 IPC** → workspace 단위 lock 을 join 해 `attached`(타 client 점유 여부)/`holder` 를 채운다(서버측 변경 0). 순수 조회라 로컬 사용자 상태(focus/닫은항목/선택)에 닿지 않는다([포커스 독립성](../../identity.md)).

이 능력은 **CLI(`remote workspaces`)와 로컬 IPC method(`remote.workspaces`) 양면**으로 노출된다(원칙 2 — 에이전트가 CLI 없이 소켓만으로도 브라우징 가능). 둘 다 동일한 코어(`tasty_remote::browse`)를 공유하며, 블로킹 SSH I/O 는 호스트 IPC 경로에서 **워커 스레드**로 돌려 이벤트루프를 막지 않는다. RA02 원격 추가 팝업의 우측 목록이 이 출력을 데이터 소스로 소비한다.

### 원격 attach (IPC — focus 중립)

로컬 IPC method `remote.attach` { `remote_workspace`, `profile?`/`ssh?` } — 선택한 원격 워크스페이스를 **로컬 mirror 로 attach**(호스트가 워커 스레드에서 SSH 터널을 세우고 mirror 를 재구성). **focus 중립**이 핵심: 이 IPC/에이전트 경로는 mirror workspace 를 *조용히 생성만* 하고 focus 를 그 ws 로 옮기지 않는다(`active_workspace` 불변). 새 mirror 로의 focus 이동은 **사용자 입력 경로 전용 별도 단계**(RA02 팝업에서 사용자가 확정할 때)이며, release IPC 에는 focus 변경 API 가 없다(원칙 3). 회신은 즉시 `{attaching:true}`(fire-and-forget) — mirror 는 비동기로 나타나므로 `tasty list workspaces` 로 확인한다: mirror 워크스페이스는 행에 `[mirror]` 가 붙고(`remote-ws (id:7) [mirror] (1 panes)`), `workspace.list` IPC 응답에는 `mirror: true` 로 실린다.

### 원격 워크스페이스 추가 팝업 (GUI picker — 사용자 경로)

위 브라우징/attach 능력을 **로컬 사용자가 직접 조작**하는 GUI 표면. 사이드바에서 **카테고리 헤더 우클릭(카테고리 on) / 새 워크스페이스(+) 버튼 우클릭 · 빈 배경 우클릭(그룹·플랫 모드 공통) → "원격 워크스페이스 추가"** 로 연다(`remote_attach` headless 팝업, 680×460 2-pane). 워크스페이스 카드 우클릭에는 없다(카테고리 ON/OFF 에 따라 노출 위치가 갈리도록 재배치 — [`sidebar/screens/sidebar.md`](../sidebar/screens/sidebar.md) 참고). 좌측은 `tasty-attach` 프로필 목록(remote_tool 이 편집하는 같은 스토어를 **소비만** 함), 우측은 선택 프로필의 원격 워크스페이스를 **4상태**(initial / connecting / error+retry / loaded[+empty])로 표시한다. 조회는 위 browse 코어(`tasty_remote::browse`)를 **워커 스레드**로 돌려(폴링 슬롯) UI 를 막지 않는다. 이미 타 client 가 점유한 원격 ws 는 lavender `in use` 배지 + 선택 불가(중복 mirror 방지).

**Connect 확정 = 사용자 동작 → focus 이동**: 원격 ws 를 골라 Connect 하면 조회에 쓴 SSH 터널을 재사용해 mirror 로 attach 하고, **새 mirror ws 로 focus 가 이동**한다(사용자가 확정한 결과). 이 focus 이동은 IPC/에이전트 경로(위 `remote.attach`, focus 중립)와 분리된 **사용자 입력 전용 큐**(`CoreState.pending_gui_attach_user`)를 통해서만 일어난다 — release IPC 는 이 큐에 push 하지 못한다(원칙 1②). 컨텍스트 메뉴 진입은 `from_user_context_menu()` 로 마킹하고, self(loopback) attach 는 release 에서 `dispatch_pending_gui_attach` 게이트가 차단한다.

**조회 중(connecting) 사용자 조작 + 정리 계약**: connecting 은 **시간 제한**이 있다. 워커 자체 상한(위 "연결 시도 상한" 45초 + 터널 ready 5초 + IPC 프로브 5초)만으로는 최악 ~55초를 아무것도 못 하고 기다려야 하므로, UI 가 **20초**(`BROWSE_DEADLINE`, ADR-0053 원격 file picker 와 같은 매 프레임 경과 판정) 안에 결과가 없으면 **워커보다 먼저** 포기하고(진행 중 조회는 취소) error 상태(+ Retry)로 전이한다 — 워커가 슬롯을 영영 못 채워도(스레드 패닉 등) connecting 에 갇히지 않는다. 그 전에 사용자가 직접 끊을 수도 있다: 조회 중에는 footer 의 ghost 버튼이 **"중단"** 이 되어 팝업을 닫지 않고 조회만 끊고 initial 로 돌아간다(닫기는 헤더 × / Esc). 어느 경로든(중단 · 타임아웃 · 다른 프로필 재선택 · 팝업 닫기) **진행 중 워커의 자식 ssh 를 kill + reaping** 한다 — 포트 발견 단계의 자식은 `SshTunnel` 의 Drop 회수 계약 밖에 있어 별도 취소 핸들(`tasty_ssh::SshCancel`)이 필요하다([`dev-guide/attach-behavior.md`](../../dev-guide/attach-behavior.md) "터널 생명주기"). 취소 뒤 워커가 뒤늦게 채운 결과는 아무도 읽지 않고 워커 종료와 함께 drop 되며, 그때 `BrowseOk.tunnel` 도 함께 drop 되어 터널이 새지 않는다.

**"+ 새 워크스페이스" 행 — 원격에 만들어서 붙는 경로**: loaded 목록의 **첫 행**은 원격에 이미 있는 워크스페이스가 아니라 `+ 새 워크스페이스` 다. 이 행을 고르고 확정하면 조회에 쓴 같은 터널로 원격에 `workspace.create` 를 1회 보내고, 그 응답의 ws id 를 **기존 Connect 와 똑같은 attach 지점**으로 넘긴다. 이름/cwd 를 묻는 UI 는 없다 — params 를 빈 객체로 보내 원격의 기본값(`type`=terminal, 기본 이름, 원격 자기 활성 surface 의 cwd 상속)을 쓴다. 클라이언트는 원격 파일시스템 경로를 모르므로 cwd 를 지어내지 않는다(명시 지정은 IPC/CLI 쪽 몫).

- **버튼이 아니라 목록 행**이다. 이웃 ws 행과 같은 select-then-confirm 을 따르고 확정은 footer 가 한다 — 그때 primary 라벨이 `Connect` → `Create & connect` 로 바뀐다. 목록 안의 행인데 혼자만 클릭 즉시 실행되면 그 자체가 상호작용 불일치이고, 원격을 **변경하는** 동작 직전의 되돌릴 수 있는 순간도 사라진다.
- **구분은 세 채널 동시** — `plus` 글리프 · accent 라벨 · 행 아래 1px 구분선. 색 하나로만 구분하지 않는다. 글리프는 ws 행의 status-dot 과 **같은 폭 슬롯** 안에서 center 되어, 이름 열의 좌측 정렬선이 아래 행들과 픽셀 동일하다.
- **원격 ws 가 0개여도 이 행은 나온다.** loaded 렌더 경로가 하나라서, 목록이 비면 caps 헤더 + 이 행 하나 + muted 한 줄로 degrade 한다(전용 center-state 는 없다). 그때는 이 행이 **미리 선택**돼 있어 pane 이 뜬 순간부터 확정 버튼이 살아 있다 — 빈 원격이 막다른 길이 아니다.
- **생성 왕복 중 / 실패는 행 안에서** 표현한다. 왕복은 워커 스레드로 돌리고(터널은 팝업이 계속 쥔 채 `port` 복사본만 넘긴다) UI 상한은 `CREATE_DEADLINE`(10초, 소켓 자체는 5초 read/write 타임아웃) — 그 사이 글리프가 스피너로, 라벨이 "워크스페이스 만드는 중…" 으로 바뀌고 아래 목록은 dim + inert 된다. 확정 버튼도 그동안 비활성이고, `start_create` 자체가 진행 중이면 no-op 이라 연타가 원격에 워크스페이스를 두 개 만들지 않는다. 실패하면 행 하단에 원격 메시지 + "다시 시도" 가 인라인으로 붙고 **팝업은 열린 채 목록도 그대로 남는다** — 실패 후 다음 수가 보통 기존 워크스페이스를 고르는 것이기 때문이다.

**알려진 제약 (이 경로 한정)**:

- **생성은 성공했는데 attach 가 실패하면 원격에 워크스페이스가 남는다.** 워크스페이스를 지우는 IPC 는 없다(소멸은 마지막 surface close cascade 로만 일어난다). 정리 수단은 mirror 안에서 마지막 surface 를 닫아 원격 ws 를 purge 시키는 기존 경로다(아래 "역반영 대신 강제 detach").
- **그 워크스페이스는 원격 재시작 후에도 남는다.** 원격이 만든 것은 mirror 가 아닌 일반 워크스페이스라 원격의 슬롯 파일에 영속된다(비영속 제외 대상은 로컬 mirror 뿐 — 아래 절). 즉 이 기능은 **원격의 영속 상태를 늘린다**: 무심코 여러 번 확정하면 원격에 워크스페이스가 계속 쌓인다.
- release 빌드의 self(loopback) attach 차단 게이트는 그대로 적용된다 — 새 ws 를 만들었더라도 대상 포트가 자기 자신이면 attach 되지 않는다(생성만 되고 끝). debug 빌드에서만 통과한다.

### mirror workspace 비영속

원격 attach 로 생긴 mirror workspace(`Workspace.mirror`)는 **원격 점유가 살아있는 세션 동안만** 유효하다. 슬롯 파일에 저장하면 재시작 시 원격 없는 **죽은 일반 workspace** 로 복원되므로, `SavedLayout::capture`(`src/core/layout_persistence/capture.rs`)가 캡처 순회에서 mirror workspace 를 **제외**하고 `active_workspace` 인덱스도 필터 후 위치로 remap 한다(자동 attach mirror·GUI picker mirror 공통). 팝업 세션 상태(선택 프로필/조회 결과/선택 행/생성 워커)도 egui temp 메모리(비영속)라 tasty 종료 시 함께 사라진다.

### 창 없는 상태(parked)에서의 세션 수명

attach 세션의 수명은 **창(window)이 아니라 engine 에 매인다.** 마지막 창을 닫거나(macOS 는 최소화도) 창은 사라지지만 engine 은 `parked_states` 에 그대로 살아 있고([multi-window](../../architecture/multi-window.md), [ADR-0087](../../adr/0087-layout-slot-occupancy-model.md) — parked engine 은 레이아웃 슬롯 점유를 유지한다), mirror 워크스페이스와 그 mirror 터미널도 그 engine 안에 남는다. 따라서:

- **parking 만으로는 세션이 끊기지 않는다.** 고아 판정(`detach_orphaned_mirror_sessions`)이 묻는 것은 "창이 있는가"가 아니라 **"그 mirror 워크스페이스를 들고 있는 engine 이 살아 있는가"** 다 — 창 있는 engine 과 parked engine 을 함께 본다. 창 유무로 판정하면 사용자가 창을 최소화했을 뿐인데 원격에 `Detach` 가 나가 점유가 조용히 풀린다.
- **사용자가 mirror 워크스페이스를 직접 닫으면** 어느 engine 에도 그 워크스페이스가 없으므로 고아로 판정되어 기존대로 정리된다 — `Detach` 통지 → 원격 점유 해제 + anchor 게이트 해제 + 터널 kill. 두 상황(창이 없어졌을 뿐 vs 워크스페이스가 없어짐)은 이 판정으로 구분된다.
- **정리는 parked engine 에도 동일하게 적용된다.** mirror 워크스페이스 행뿐 아니라 mirror 터미널·mirror busy 엔트리·mesh 프레임 캐시를 함께 걷어내고 `active_workspace` 인덱스를 클램프한다. 판정과 정리의 순회 범위는 **같아야** 한다 — 판정이 살아 있다고 본 engine 을 정리가 못 찾으면, 그 engine 이 나중에 창에 다시 실릴 때 아무 데도 연결되지 않은 mirror 워크스페이스가 되살아난다.
- parked engine 에는 창이 없으므로 정리 시 toast 를 쌓지 않는다(토스트 수명이 wall-clock 기준이라 창 복원 시점엔 이미 만료된다).
- **도착하는 mirror 이벤트도 parked engine 에 즉시 적용된다**([ADR-0110](../../adr/0110-mirror-events-apply-to-parked-engines.md)). `apply_attach_client_output` 은 적용 대상을 **창 있는 engine → parked engine** 순으로 찾고(`mirror_output_host`), 대상을 찾은 **뒤에야** reader 버퍼를 drain 한다. 창이 없는 동안 도착한 `Data`/`Resize`/`StructuralDelta`/`Activity`/`Attention`/`Cwd`/`Mesh` 는 그 engine 의 mirror 터미널·매핑·트리에 도착 순서대로 반영되므로, 창 복원 시 mirror 는 이미 최신이고 `remote_to_local` 매핑도 desync 되지 않는다 — 로컬 PTY 출력이 parked engine 에서도 파싱되는 것과 같은 대칭이다. 판정·정리·적용 세 순회의 범위는 **같다**. 어느 engine 에도 워크스페이스가 없으면(고아) drain 하지 않고 두며, 같은 프레임의 고아 정리가 세션째 걷어낸다.
- parked engine 에 적용될 때는 창 표면이 필요한 부수효과만 생략한다 — toast 는 로그로 대체(위와 같은 이유), repaint 요청은 없음(복원 시 새 창이 그 engine 을 그대로 그린다). 상태 변경(터미널 grid·트리·매핑)은 창 유무와 무관하게 항상 적용된다.

## 인터페이스

- **AI Agent / 원격 (CLI)**:
  - `tasty tool attach <name> [SURFACE] [--workspace <id>] [옵션…]` — **tasty-attach 프로필**로 attach(profile-우선 편의 표면). `--list` = tasty-attach 목록만.
  - `tasty remote attach [SURFACE] --ssh|--profile [옵션…]` — 원격 surface attach. `--profile` 은 **tasty-attach kind**(ADR-0032; ref/inline resolve).
  - `tasty remote attach --workspace <id> --ssh|--profile …` — 원격 workspace attach.
  - `tasty remote attach --force-detach [--workspace <id>]` — 점유 강제 해제(로컬 JSON-RPC; `--ssh` 와 상호배타).
  - `tasty remote attach --into-gui --target-port <p> --workspace <ws>` — 실행 GUI 에 mirror.
  - `tasty remote check --ssh|--profile` — 원격 생존 확인(`--profile` = tasty-attach).
  - `tasty remote workspaces --ssh|--profile [--json]` — 원격 워크스페이스 목록 조회(browse). `--ssh 127.0.0.1:<port>` 로 loopback 직결(터널 없이 로컬 e2e).
  - `tasty remote new-workspace --ssh|--profile [--name <n>] [--cwd <원격경로>] [--json]` — 원격에 워크스페이스 생성(원격 mutate). 출력 id 를 `remote attach --workspace <id>` 에 넘기면 "만들고 그 자리에서 attach" 가 CLI 만으로 완성된다. `--cwd` 는 **원격 파일시스템** 기준이며 원격이 존재를 검증한다. 원격 active 는 불변(Agent origin).
  - `tasty set workspace --id <id> --ssh-profile <name> --remote-workspace <N>` — 자동 매핑 선언.
- **IPC (`attach.*`)**: `acquire`/`release`(stream 핸드셰이크), `force_detach`/`force_detach_workspace`, `into_gui`, `list`(점유 목록 조회). 표 상세 → [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md#ipc-표면-attach).
- **IPC (`remote.*` — 원격 브라우징/생성/attach, 원칙 2)**: `remote.workspaces` { `profile?`/`ssh?` } → 원격 ws 목록(browse, 워커 스레드+지연 회신). `remote.attach` { `remote_workspace` | `new_workspace`, `profile?`/`ssh?`, `name?`, `cwd?` } → 원격 ws 를 로컬 mirror 로 attach(**focus 중립**: mirror 생성만, focus 이동 없음). `new_workspace:true` 면 원격에 워크스페이스를 먼저 만들고 그것을 attach 한다 — `remote_workspace` 와 상호배타이며, 이때만 **생성 완료까지 기다렸다 지연 회신**해 새 `remote_workspace` id 를 돌려준다(기존 ws attach 는 즉시 `{attaching:true}` 유지). CLI 와 코어 공유 — 조회는 `tasty_remote::browse`, 생성은 `tasty_remote::create`.
- **로컬 self attach**: 사용자 mirror 조작 재현 성격이라 release 에 없음 — `tasty debug attach`(debug 빌드 전용, [`dev-guide/debug-ipc`](../../dev-guide/debug-ipc.md)).
- **프로필**: `--profile`/`tool attach` 이 참조하는 tasty-attach 프로필(및 그것이 `ssh_ref` 로 참조하는 ssh 프로필)은 [remote-profiles](../remote-profiles/index.md) 이 관리.

## 원격 파일 전송 수신측 저장 정책

원격 attach 채널 위 native bulk 파일 전송(ADR-0054)의 **수신측**은 저장 폴더와 폴더 최대 용량을 설정한다("원격이 경로를 소유"). 설정은 `Settings.remote_transfer` — `dir`(저장 폴더, 빈 값이면 기본 `~/.tasty/transfers/`) + `max_mb`(폴더 최대 용량, MiB, 기본 500).

- **IPC/CLI (focus 독립, 전역 설정)**: `settings.get_remote_transfer` / `settings.set_remote_transfer {dir?, max_mb?}` (local-only) · `tasty settings {get-remote-transfer, set-remote-transfer --dir --max-mb}`.
- **GUI 설정**: Settings › General › Remote transfer 서브탭(`src/view/settings/ui/tabs/remote_transfer.rs`) — 저장 폴더(Browse 포함)와 최대 용량(MiB). 디자인↔본체 매핑은 [design-gallery-mapping](../../design/systems/design-gallery-mapping.md) 의 Remote transfer 절.
- **용량 사전 거부**: 전송 시작(`BulkBegin.total_size`) 시점에 `현재 폴더 사용량 + total_size` 가 상한을 넘으면 청크 수신 전에 거부하고 `BulkResult{ok:false, reason:"capacity exceeded"}` 를 회신한다(경계 `== max` 는 허용, `> max` 거부). 폴더 사용량은 1-depth 파일 크기 단순 합산.

## mirror 터미널 이미지 붙여넣기 → 원격 경로 삽입

mirror(attach) 터미널에 클립보드 **이미지**를 붙여넣으면, 로컬 PNG 경로 대신 그 이미지를 bulk 파일 전송 채널(ADR-0054)로 원격에 업로드하고 **원격 파일시스템 경로**를 터미널 입력에 삽입한다(원격이 읽을 수 있는 경로). 로컬(비-mirror) 터미널의 이미지 붙여넣기는 기존대로 로컬 temp PNG 경로를 삽입한다(회귀 없음). 텍스트 붙여넣기는 mirror 여부와 무관하게 불변.

- **호스트 generic**: surface kind/claude 특화가 아니라 모든 mirror 터미널에 적용된다(claude CLI 가 그 경로 포맷을 인식하는지는 런타임 외부 동작 — 범위 밖).
- **판정**: 붙여넣기 시점에 focused surface 가 mirror workspace(`Workspace.mirror`) 소속인지로 mirror/로컬을 가른다(판정을 트리거 시점에 끝내 업로드 완료 전 포커스 변경에 흔들리지 않게).
- **비동기**: 업로드(블로킹)는 백그라운드 스레드에서 수행하고(메인 루프 무블록), 완료 시 원격 경로를 그 mirror surface 입력에 삽입한다 — mirror surface 입력은 forwarder 로 원격 PTY stdin 에 투명 전달되므로 별도 삽입 API 가 없다. 진행/완료/실패 피드백은 전송 진행 · 실패 팝업이 담당한다(아래).

## 전송 진행 · 실패 팝업

bulk 파일 전송과 mirror 터미널 이미지 붙여넣기 업로드에 대한 사용자 피드백 UI 2종(scrim 중앙 headless PopupDef). 현재 트리거 소스는 mirror 터미널 이미지 붙여넣기 하나다(일반 파일 전송 UI 는 후속).

- **진행 팝업(`transfer_progress`)**: download glyph + "Receiving file" + mono pct → 파일명(mono 말줄임) → **determinate 4px progress bar**(recessed track `bg-app` + accent fill `accent-primary`, **0ms 무애니** — 바이트 수신 시에만 fill 폭 이동, 시스템 최초 determinate) → `transferred / total` + rate → ghost Cancel. `close_on_outside_click=false`(전송 중 실수 dismiss 방지), 모든 파일 완료 시 self-close. 다중 파일은 행 반복. Cancel 은 진행 관망만 중단(동기 워커라 실제 전송 abort 불가 — 백그라운드 전송은 완료됨).
- **실패 팝업(`transfer_error`)**: danger glyph + "Transfer failed" + `<파일명> could not be received.` + mono reason well(command-well: `bg-app`+separator, danger 텍스트). 기본 dismiss(Esc/scrim). danger-fill 버튼 금지. **원격 거부**(수신측 용량 상한 초과 등 `BulkResult{ok:false}`)면 재시도 무의미 → **Dismiss 단독**; **전송 중 실패**(전송/프로토콜 에러)면 → **Dismiss + Retry**(원본 바이트를 기존 업로드 큐에 재투입). 거부 vs 전송에러 판정은 `upload_file_over_bulk` 의 `Err` 접두(`BULK_REJECT_PREFIX`)로 한다.
- **진행률 배선**: `upload_file_over_bulk` 에 `on_progress(sent, total)` 콜백을 추가해 청크 전송마다 통지 → 이미지 업로드 워커가 `transfer_progress` 채널 + `AppEvent::TransferProgressTick` 로 메인에 흘림 → `drain_transfer_progress` 가 해당 행을 갱신. 완료(Ok/Err)는 기존 `ImageUploadReady` 경로가 행 제거 + 성공 삽입/실패 승격을 처리한다.

## 비-목표 (Out of scope)

- **자체 원격 프로토콜/암호화/인증** — 전부 SSH 에 위임. attach 채널에 별도 토큰 없음(연결 경계 = 권한 경계).
- **단발 화면 읽기** — attach 세션을 열 필요 없음. 정식 경로는 `tasty read screen` / `tasty read since-mark`(별도 기능).
- **로컬 loopback attach 의 release 노출** — debug 전용.
- **프로토콜 프레임/터널 결선/재연결 백오프 등 메커니즘** — [dev-guide/attach-behavior](../../dev-guide/attach-behavior.md).
- **SSH 프로필 CRUD** — [ssh-tool](../remote-profiles/index.md).

## Acceptance Criteria

- Given 동일 머신 두 인스턴스 When 한쪽이 다른 쪽 surface 를 attach Then mirror grid 가 원본과 일치한다(`--dump-after` 로 검증).
- Given surface 가 이미 점유됨 When 다른 client 가 attach 시도 Then holder 정보를 담아 거부된다.
- Given 점유된 surface When 서버측 GUI 키/`surface.send` Then 입력이 차단되고 client 입력만 도달한다.
- Given 점유 상태 When 로컬 사용자가 `--force-detach` Then holder 가 종료되고 대상이 일반 surface 로 복귀한다.
- Given workspace 가 점유된 상태 When 로컬 사용자가 사이드바에서 그 행을 우클릭 Then **강제 끊기** 항목이 보인다.
- Given 그 항목을 누른 상태 When 확인 팝업이 떠 있다 Then 점유는 아직 유지되고, 취소·Escape·바깥 클릭 어느 쪽으로 닫아도 점유가 그대로 남는다.
- Given 확인 팝업이 떠 있는 동안 원격이 스스로 끊었다 When 다음 프레임 Then 팝업은 즉시 닫힌다(대상 없는 확인을 남기지 않는다).
- Given client 연결 종료(EOF) Then 점유 lock 이 자동 free 된다.
- Given client 가 FIN/RST 없이 조용히 끊김(silent disconnect) When attach heartbeat TTL 이 만료 Then 점유 lock 이 EOF 와 동일하게 자동 free 되고, 같은 surface/workspace 로 새 client 의 재attach 가 성공한다.
- Given workspace attach When 멤버 터미널 하나가 이미 다른 client 점유 Then workspace attach 가 거부된다.
- Given stale 포트 파일만 있는 죽은 인스턴스 When `tasty remote check` Then dead(exit≠0)로 판정한다.
- Given workspace attach 대상에 bundled egui-mesh surface(image/mesh_demo) 가 있음 When client 가 GUI mirror 로 attach Then 그 surface 의 실제 렌더 콘텐츠가 mirror pane 에 표시된다(placeholder 아님). markdown 은 Stage B(webview 전환)로 egui-mesh 화이트리스트에서 빠져 이 mesh-mirror 채널을 쓰지 않는다 — 아래 원문 채널로 따로 mirror 된다(과거 `attach_markdown_mesh_mirror_loopback.rs` 통합 타깃이 mesh 경로를 검증했고, markdown 이 그 채널을 벗어나며 삭제됐다).
  - image — 미검증.
  - mesh_demo — 미검증.
  - 2종 공통 시각적 렌더 확인(실제 GUI attach client 로 mirror pane 화면 비교) — 미검증.
- Given workspace attach 대상에 markdown surface 가 있음 When client 가 attach Then 그 surface 는 placeholder 가 아니라 전용 role 로 도착하고 원격 파일 경로와 탭 제목이 함께 실린다(`tests/attach_markdown_content_loopback.rs`).
- Given mirror 의 markdown surface When client 가 원문을 요청 Then 서버가 실제 디스크의 원문을 그대로 회신하고, 읽을 수 없는 파일은 연결이 끊기거나 무응답이 되는 대신 사유와 함께 실패로 답한다(`tests/attach_markdown_content_loopback.rs`).
  - client GUI 가 그 원문을 실제로 렌더하는 것 — 실측(GUI 두 인스턴스 loopback attach): client plugin 이 원격 원문으로 HTML 을 만들어 올리고(`markdown surface N: loaded … bytes of HTML`), 화면에 원격 문서의 제목·본문이 그려진다.
- Given client 에 번들 markdown plugin 이 등록돼 있음 When mirror 트리에 markdown role leaf 가 있음 Then 그 leaf 는 빈 surface 가 아니라 markdown kind 로컬 surface 로 만들어지고 원격 파일 경로가 plugin 에 전달된다 — 구조 delta 로 트리가 다시 와도 같은 surface 를 재사용한다(`src/app/attach_client.rs` 단위 테스트).
- Given `markdown` kind 를 다른 plugin 이 차지함 When 같은 트리 Then 그 leaf 는 빈 surface 로 남는다(`src/app/attach_client.rs` 단위 테스트).
- Given client 의 번들 markdown plugin 이 attach 시점에 안 떠 있음(부팅 때 꺼져 있던 plugin) When plugin 을 켠 뒤 그 mirror 워크스페이스를 연다 Then 빈 surface 로 남지 않고 plugin 이 뜬 뒤 원격 문서로 채워진다 — kind 대기 자리가 kind 등록 뒤 mirror 문서로 실제화되는 것(`src/app/attach_client.rs` 단위 테스트)과 plugin 이 그 복원 data 를 로컬 snapshot 과 구별하는 것(`crates/tasty-plugin-markdown/src/tests.rs`)을 고정한다. 실측(GUI 두 인스턴스 loopback attach): 켜기 전 surface 목록 `Pending` → 켜고 연 뒤 `Remote`, 원문이 그려지고 탭 제목은 원격 파일 이름이다.
- Given mirror markdown 문서 When plugin 이 그 문서를 연다 Then 원격 경로가 이 머신에 실재해도 파일을 읽지 않고, 상대경로를 풀 디렉토리를 두지 않으며, layout 에 남길 snapshot 도 내지 않는다 — 원문은 요청해 받은 것만 그린다(`crates/tasty-plugin-markdown/src/tests.rs`).
- Given mirror markdown 문서 When 새로고침 버튼을 누르거나 `markdown.reload` 를 부른다 Then 원격 원문을 다시 요청하고, 늦게 온 옛 회신은 버린다(`crates/tasty-plugin-markdown/src/tests.rs`). 실측(GUI 두 인스턴스): 버튼 클릭과 `markdown.reload` 모두 원문을 다시 받아 stale 표시를 끈 문서로 다시 그린다.
- Given mirror markdown 문서(원문을 기다리는 중이든 이미 표시 중이든) When anchor 매핑이 있는 세션의 attach 연결이 끊겨 재연결 대기에 들어감 Then 문서는 로딩 상태로 멈추지도 옛 원문을 최신처럼 남기지도 않고 "원격 연결이 끊어졌다" 는 끊김 화면으로 바뀐다 — 요청 송신 자체가 실패해도 같은 실패 결과가 간다. plugin 이 abandon 을 받으면 원문을 받은 뒤에도 끊김 상태가 되는 것(`crates/tasty-plugin-markdown/src/tests.rs`)과 끊김 상태가 원문보다 앞서 그려지는 것(`crates/tasty-plugin-markdown/src/render.rs` 단위 테스트)을 고정한다. 실측(GUI 두 인스턴스, 서버 종료): 원문을 표시 중이던 문서가 끊김 문구 화면으로 다시 그려졌다. anchor 가 없는 수동 attach 는 mirror 워크스페이스째 정리되고 끊김 toast 가 뜬다(실측).
- Given 끊김 화면을 보이는 survivor mirror markdown 문서 When 같은 서버에 재연결 Then 사용자가 누르지 않아도 원문을 다시 받아 그린다 — 끊긴 동안 바뀐 원문이면 바뀐 원문이 온다. client 가 재연결 직후 survivor 문서마다 변경 신호를 한 번 보내고, plugin 은 그 신호로 원문을 보여 주던 문서는 stale 표시만, 원문을 못 보여 주던 문서(끊김·실패)는 재요청한다(`crates/tasty-plugin-markdown/src/tests.rs`). 실측(GUI 두 인스턴스, 서버를 SIGSTOP 해 heartbeat 만료로 끊고 SIGCONT 로 재연결): 같은 로컬 surface 가 끊김 화면에서 원문으로 돌아왔고, 재연결 신호 송신을 빼는 변이에서는 끊김 화면에 머물렀다. 서버를 재시작하면 원격 surface id 가 바뀌어 survivor 가 아니라 새 leaf 로 만들어지고 새로 원문을 받는다(실측).
- Given mirror markdown 문서가 원문을 받아 표시 중 When 원격(GUI 서버)에서 그 문서가 다시 그려짐(파일 수정·테마 변경 등) Then client 는 원문을 자동으로 다시 받지 않고 새로고침 버튼 색만 바뀌며, 버튼을 누르면 그때 최신 원문이 온다 — 서버가 신호를 워크스페이스를 점유한 client 전부에 한 번씩 보내는 것과 점유가 없으면 아무것도 안 보내는 것(`src/core/attach_runtime.rs` 단위 테스트), client 가 자기 mirror 문서의 신호만 plugin 에 넘기는 것(`src/app/attach_client.rs` 단위 테스트), plugin 이 신호로는 stale 표시만 켜는 것(`crates/tasty-plugin-markdown/src/tests.rs`)을 각각 고정한다. `webview.set_url` 수신이 그 신호 송신으로 이어지는 것은 `src/adapters/ipc/handler/webview.rs` 단위 테스트가 고정한다(신호 호출을 빼는 변이에서 실패). 실측(GUI 두 인스턴스): 서버에서 원문 파일을 고치자 client 문서는 원문을 다시 받지 않고, 새로고침 버튼의 stale 표시만 바뀐 HTML 로 다시 그려졌다(HTML 크기 차이가 툴팁 문구 차이와 같다). 서버가 헤드리스면 이 신호는 없다(원격 문서가 다시 그려지는 경로가 host 에 닿지 않는다).
- Given 전송 형태로 예산(700 KiB)을 넘는 문서 When client 가 원문을 요청 Then 세션이 끊기지 않고 잘린 원문이 "잘렸다" 표시와 함께 도착하며, 그 판정은 원문 바이트가 아니라 이스케이프된 길이로 이뤄진다(`tests/attach_markdown_content_loopback.rs`).
- Given attach 점유가 없는 client When 원문을 요청 Then 파일을 한 바이트도 읽히지 않고 거절된다(`tests/attach_markdown_content_loopback.rs`).
- Given 워크스페이스 둘 중 **한쪽만** 점유한 client When 다른 워크스페이스의 markdown 원문을 요청 Then 인가되어 원문이 도착한다 — 점유는 인스턴스 단위 신뢰이지 워크스페이스별 권한이 아니다(`tests/attach_markdown_content_loopback.rs`).
- Given mesh mirror pane 이 표시 중 When client 가 그 pane 을 클릭/타이핑 Then 원격 plugin 프로세스의 상태가 실제로 바뀌고 그 결과가 mirror 에 반영된다(예: mesh_demo 클릭 카운터 증가).
- Given mesh mirror pane 에 텍스처 delta 체인 단절(예: 재연결) When client 가 감지 Then `MeshFullResendRequest` 로 전체 텍스처 상태를 재수신해 정상 렌더를 회복한다.

> 전부 headless 검증 가능 — 동일 머신 다중 인스턴스 + loopback 직결(`127.0.0.1:PORT`)로 SSH 없이도 attach 파이프라인을 재현, `--dump-after` 로 grid 일치 확인.

## 구현

- 점유 레지스트리: `src/core/attach.rs` `OccupancyRegistry`(hard: `surface_locks` / `workspace_locks` / `surface_to_workspace`, acquire/release/force_detach/release_all_for_client · soft: 별도 엔트리 + acquire_soft/release_soft, ADR-0040). 휘발성.
- 런타임/스냅샷: `src/core/attach_runtime.rs`(서버측 수신, transport 무관 loopback), `src/core/attach_readonly.rs`(서버측 readonly mirror), `src/app/attach_poll.rs`(3초 tick).
- 자동 매핑: `src/app/auto_attach.rs`(`Workspace.attach_mapping` 활성화 시 SSH 터널 + GUI mirror).
- IPC: `src/adapters/ipc/handler/attach.rs`(`attach.*`). 원격 브라우징/attach IPC(`remote.workspaces`/`remote.attach`)는 `src/app/ipc/app_methods.rs`(워커 스레드+지연 회신). focus 중립 mirror 생성은 `src/app/auto_attach.rs`(수동 트리거 `anchor=None` 재사용) → `src/app/attach_client.rs::start_gui_attach`(`workspaces.push` 만, `active_workspace` 불변; 새 mirror ws id 반환).
- GUI picker 팝업(사용자 경로): `src/adapters/ui/popup/remote_attach.rs`(2-pane 상태머신 + browse 워커 폴링), `defs.rs`(headless PopupDef), 진입 컨텍스트 메뉴 2곳 `src/view/main/redraw.rs`(NewWorkspaceButton / Workspace). Connect → `CoreState.pending_gui_attach_user` 큐 → `App::dispatch_pending_gui_attach`(사용자 경로 drain) → `start_gui_attach` + `focus_mirror_workspace`(새 mirror 로 focus 이동, 사용자 경로 전용). "+ 새 워크스페이스" 확정은 그 큐에 넣기 전에 원격 `workspace.create` 워커(`spawn_create`/`poll_create`) 한 번을 끼우고, 성공 응답의 id 로 **같은 큐**에 합류한다. 갤러리 specimen 진입점·치수 상수: `crates/tasty-gallery/src/catalog/components/remote_attach.rs`(loaded / 새 행 5상태 / 우측 pane 상태). 자식 `remote_attach/new_row.rs`는 새 행 5상태, `panes.rs`는 좌우 pane 조립, `rows.rs`는 프로필·workspace 행과 공용 dot 슬롯을 담당한다.
- mirror 비영속: `src/core/layout_persistence/capture.rs`(`SavedLayout::capture` 가 `ws.mirror` 제외 + active 인덱스 remap). 회귀 테스트 `core::state` `mirror_workspace_not_persisted`.
- mirror 이미지 붙여넣기 → 원격 업로드: `src/view/main/clipboard.rs`(이미지 분기에서 `Workspace.mirror` 판정 → `CoreState.pending_image_uploads` 큐에 PNG 바이트 push, 비-mirror 는 기존 로컬 PNG 경로 유지), `src/app/image_upload.rs`(`poll_image_uploads`: 큐 drain → 백그라운드 `upload_file_over_bulk` → 결과 채널 → `dispatch_paste`(원격 경로 삽입) 또는 전송 실패 팝업 승격). 업로드 API 는 `src/app/attach_client.rs::upload_file_over_bulk`(bulk 클라 송신, 동기 블로킹).
- 전송 진행/실패 팝업: 호스트 팝업 `src/adapters/ui/popup/transfer.rs`(`TRANSFER_PROGRESS_POPUP_ID`/`TRANSFER_ERROR_POPUP_ID`, `TransferProgress`/`TransferRow`/`TransferError` + draw/sizer), PopupDef 등록 `defs.rs`(둘 다 headless scrim; progress `close_on_outside_click=false`), scrim/bg 매칭 `popup.rs`·`popup/draw.rs`, DialogState 슬롯 `src/state/dialogs.rs`(`transfer_progress: Option` + `transfer_error: VecDeque`), self-close cleanup `PopupDef.on_close`(`transfer.rs`의 `on_close_transfer_progress`/`on_close_transfer_error`). 진행률 배선: `upload_file_over_bulk` 의 `on_progress(sent,total)` 콜백(bulk 송신 경로 침범 최소) → 이미지 업로드 워커가 `transfer_progress` 채널 + `AppEvent::TransferProgressTick`(`event.rs`/`event_handler.rs`) → `image_upload.rs`(`begin/drain/finish_transfer_progress_row`, `push_transfer_error`, `format_rate`; 실패 분류는 `BULK_REJECT_PREFIX` 접두로 거부 vs 전송에러). 갤러리 specimen `crates/tasty-gallery/src/catalog/components/transfer.rs`(progress/error 2종). i18n `[transfer.progress]`/`[transfer.error]`.
- 브라우징 코어(CLI/IPC 공유): `crates/tasty-remote/src/browse.rs`(`browse`/`resolve_endpoint`/`probe_method` — loopback 직결 + `workspace.list`+`attach.list` 병합).
- mesh mirror(bundled egui-mesh surface): 프로토콜/분류/서버 구독·forward/클라이언트 렌더·입력 전체 상세는 [dev-guide/egui-mesh-channel "attach mesh mirror 소비 경로"](../../dev-guide/egui-mesh-channel.md#attach-mesh-mirror-소비-경로).
- CLI: `crates/tasty-cli/src/commands/remote.rs`(clap 선언), `crates/tasty-cli/src/dispatch.rs`(갈래 판정), `local/attach.rs`(`run_attach_*` 세션 머신), `local/remote_check.rs`, `local/remote_workspaces.rs`(browse 얇은 래퍼), `crates/tasty-ssh/src/lib.rs`(SSH 결선 + `PortDiscoveryError`/`PortDiscoveryFailureKind` 원인 분류, `pick_most_informative` 로 auto 체인 대표 에러 선택).

## 화면

- [screens/remote-attach.md](screens/remote-attach.md) — GUI mirror 워크스페이스 + 점유 readonly 표시(사이드바 mirror glyph/chip / 작업영역 렌더로 연결).
</content>
