# ADR-0056: git-viewer 원격(attach mirror) 조회 — 공유 crate + `git_viewer.query` 이벤트 채널

- **Status**: Accepted
- **Date**: 2026-07-29
- **Tags**: git-viewer, plugin, attach, mirror, ipc, event-bus, wire-format, tools-menu, egui-mesh, popup, timeout, adr-0053, adr-0028, adr-0040
- **Group**: remote-attach

## Context

`tasty-plugin-git-viewer` popup 은 `popup.open` context 로 받은 `cwd` 문자열을 plugin 프로세스
내부에서 직접 `git2::Repository::discover()` 하는 로컬 전용 설계였다. attach mirror 워크스페이스
(원격 tasty 를 SSH 터널로 mirror 한 세션)에는 실제 PTY/파일시스템 접근 채널이 없어 이 경로가
그대로 로컬 host 자신의 cwd 를 discover 하거나(우연히 git 저장소 안이면 잘못된 로컬 정보 표시),
아무 것도 못 찾아(대부분) "No git repository found" 를 오표시했다.

애초 문서화된 방향은 "`popup.set_context` 의 `context` 필드를 원격 조회 결과로 채운다"였으나,
실제 프로토콜(`tasty_plugin_protocol::PopupSetContextParams`)을 확인한 결과 이 구조체엔 임의
`context: Value` 필드가 **없다** — 그 필드는 `popup.open`(`PopupOpenParams`) 에만 있고, 최초
1 회만 보내진다. 따라서 "매 상호작용마다 갱신되는 원격 결과"를 그 경로로 흘릴 수 없다. 또한
`popup.set_context` 자체가 `geom_changed || has_input || need_bootstrap || theme_changed ||
need_full` 일 때만 host → plugin 으로 나가므로(`src/plugin_bridge/popup_render.rs`), 비동기 push
만으로는 다음 프레임에 plugin 이 다시 그린다는 보장이 없다.

## Decision

**공유 crate 분리 + attach 커스텀 이벤트 왕복 + Event Bus unicast 강제 repaint**, 세 가지를
조합한다.

1. **`crates/tasty-git-core` 신설.** `tasty-plugin-git-viewer/src/git.rs`(discover_repo/
   collect_status/collect_log/collect_diff/collect_worktrees + 데이터 타입)를 그대로 옮긴다.
   host core(`src/core/attach_runtime.rs`)와 plugin(로컬 경로) 양쪽이 이 crate 를 직접 의존해
   같은 조회 로직을 쓴다. 데이터 타입(`WorktreeEntry`/`StatusEntry`/`LogEntry`/`DiffData`/
   `DiffHunk`/`DiffLine`/`FileStatus`/`DiffLineKind`)에 `serde::Deserialize` 를 추가해 원격
   wire 응답을 이 타입으로 직접 역직렬화한다 — plugin 쪽에 별도 wire DTO 를 중복 정의하지
   않는다(필드명이 host 가 조립하는 JSON 과 1:1 이므로 그대로 재사용 가능).
   `LogEntry.summary` / `author` 는 git 에 값이 없으면 **빈 문자열**로 wire 를 탄다 — host 는
   자연어 폴백을 채우지 않고, 문구는 받아서 그리는 plugin 이 자기 lang 으로 고른다
   ([ADR-0106](0106-non-widget-user-strings-go-through-i18n.md) 결정 4 로 확정된 보강).
2. **`git_query_request`/`git_query_result` 커스텀 이벤트 쌍**(ADR-0053 `list_dir_request`/
   `list_dir_result` 와 동일 패턴 — `StreamControl` enum 밖의 raw JSON `event` 태그, 같은
   `StreamTag::Control` 채널). `kind: "snapshot" | "diff"` + 선택적 `worktree_path`(이전 응답의
   opaque 서버 경로 echo) / `diff_path` 로 최초 로드뿐 아니라 refresh·worktree 전환·파일→diff
   클릭까지 전부 이 한 쌍으로 왕복한다. 인가는 ADR-0053/ADR-0040 과 동일 "attach 점유 = 신뢰"
   (`engine.attach.client_holds_workspace(client_id)`).
3. **cwd 는 server 가 직접 판정.** `worktree_path` 가 없으면 client 가 forward 한 cwd 문자열을
   신뢰하는 대신, `surface_id` 로 서버 자신의 실제 원격 PTY 를 찾아 `Terminal::get_cwd()`
   (OSC-7 캐시 우선, 없으면 `/proc`(Linux)·`proc_pidinfo`(macOS) 폴백)를 직접 호출한다. mirror
   클라이언트의 OSC-7 재생에 의존하지 않으므로 "원격 셸이 OSC 7 을 방출하지 않는 경우"도
   커버한다. (ADR-0053 의 `list_dir` 는 여전히 client-forwarded `dir` 문자열을 쓴다 — 그 표면은
   이번 ADR 범위 밖이라 손대지 않는다.)
4. **plugin→host `git_viewer.query` IPC 신설**(`FsRead` 권한 재사용, `crates/tasty-ipc/src/
   method_meta.rs`). 동기 `fs.pick_file`(ADR-0042)과 달리 **비동기 accept** 패턴 — 호출은
   `request_id` 만 즉시 회신하고, `CoreState::pending_git_query_forward` 에 큐잉된 요청을 다음
   tick 에 attach 세션으로 실제 forward 한다. 결과는 `PluginManager::emit_host_event_to_plugin`
   (Event Bus 의 owner-unicast 경로 — 구독 등록과 무관하게 `command.invoked` 와 동일하게 항상
   전달됨)으로 `git_viewer.query_result` 이벤트를 plugin 에 push 한다.
5. **강제 repaint 플래그.** `popup.set_context` 의 기존 dirty 판정만으로는 이 비동기 push 가
   다음 프레임 렌더를 트리거하지 못하므로, `AppState.plugin_mesh_popup_pending_repaint:
   HashSet<u64>` 를 신설해 `emit_host_event_to_plugin` 직후 해당 popup 인스턴스를 여기 넣는다.
   `popup_render.rs` 의 dirty OR-체인에 `need_repaint` 항목으로 합류시킨다.
6. **payload 예산 캡.** `GIT_QUERY_BYTE_BUDGET = 700 * 1024`(list_dir 의 `LIST_DIR_ENTRIES_BYTE_
   BUDGET` 과 동일 근거 — `MAX_FRAME_LEN` 1MiB 보다 충분히 작게). snapshot 은 worktrees→
   status→log 순차 캡, diff 는 hunk 단위 캡. 잘렸는지는 최상위 `truncated: bool`(status/log/
   diff 세 플래그의 OR) 하나로만 plugin 에 전달한다 — 세부 원인 구분은 버리고, plugin 은 이
   플래그를 현재 UI 배너 없이 무시한다(list_dir 의 toast 같은 별도 채널이 popup egui-mesh 안엔
   없음).
7. **`is_current` 배지는 client 가 고정.** 서버는 요청마다 discovery root(worktree_path 오버라이드
   또는 surface cwd)를 기준으로 `is_current` 를 다시 계산해 보내므로, worktree 전환 요청을 보내면
   그 전환 대상이 "current" 로 보일 수 있다. 로컬 모드(popup 을 연 위치에 배지가 고정)와 동일한
   동작을 유지하기 위해, plugin 은 **최초 스냅샷의 `active_worktree_path` 를 popup 인스턴스
   생애주기 동안 고정**해 두고, 매 스냅샷 응답 적용 시 그 고정 경로와 문자열 비교로 `is_current`
   를 client 측에서 재계산한다(서버가 보낸 값을 덮어씀).
8. **mirror disconnect 시 진행 중 요청 강제 abandon.** 요청을 보낸 뒤 attach 세션 자체가
   끊기면(force-detach/EOF/heartbeat TTL) 응답이 영영 오지 않아 popup 이 "Loading…" 에
   무한정 멈춘다. `cleanup_mirror_workspace(from_disconnect=true)` 가 `request_id = 0`
   sentinel(실제 발급은 1부터 시작)로 `git_viewer.query_result` 를 쏘고, plugin
   `apply_remote_reply` 는 이 sentinel 을 "지금 뭔가 기다리고 있다면(= `pending_request_id`
   가 `Some`) 무조건 버려라"로 해석한다 — 정확히 어떤 `request_id` 를 기다리는지는 host 가
   모르므로(그 상태는 plugin 내부에만 있음) 값이 아닌 존재 여부로만 판정한다. 아무것도 기다리고
   있지 않은 인스턴스(이미 데이터를 보여주는 중 등)는 무시한다. 여러 mirror workspace 를 동시에
   쓰는 중이면 다른(살아있는) workspace 의 git-viewer popup 까지 함께 리셋될 수 있는 보수적
   근사다 — git-viewer 는 단일 primary popup 인스턴스만 활성 조회를 하므로 실질적으로는 그
   하나만 영향받는다.

## Consequences

- **얻은 것**: mirror 워크스페이스에서 git-viewer 가 원격 저장소의 실제 status/log/diff/worktrees
  를 보여준다. host/plugin 양쪽 로직이 `tasty-git-core` 하나로 합쳐져 이후 조회 로직 변경이 한
  곳으로 수렴한다. `git_viewer.query` 가 상호작용별로 재요청 가능해 최초 로드 스냅샷만 고치는
  얕은 수정에 그치지 않는다.
- **잃은 것**: `git_viewer.query_result` 이벤트 key 상수(`"git_viewer.query_result"`)와
  `GIT_VIEWER_PLUGIN_ID`(`"com.tasty.git-viewer"`) 는 host(`src/app/attach_client.rs`)와
  plugin(`crates/tasty-plugin-git-viewer/src/main.rs`) 양쪽에 리터럴로 중복 정의된다 — 이 둘을
  묶는 공유 crate 가 없어(plugin 은 별도 프로세스라 host 내부 상수를 직접 import 할 수 없음)
  동기화는 코드 리뷰/커밋 시점 수동 관리다. 서버가 요청을 받아 attach 세션으로는 전달했지만
  원격이 응답을 영영 보내지 않는 경우(연결은 살아있는데 서버측이 hang)를 잡는 **soft timeout 이
  없다** — ADR-0053 의 file picker 는 popup wrapper 가 host 프로세스 내부라 매 프레임
  `sent_at.elapsed()` 를 직접 판정할 수 있었지만, git-viewer popup 은 plugin(별도 프로세스)
  소유라 그 타이머 루프를 host 쪽에 둘 자리가 마땅치 않아 이번 스코프에서는 구현하지 않았다. 세션
  자체가 끊기는 두 경우 — 보내기 **전** 실패(`dispatch_pending_git_query_forwards` 의 send-time
  실패)와 보낸 **뒤** 세션이 끊기는 경우(`cleanup_mirror_workspace` 의 sentinel, 결정 8) — 는
  둘 다 즉시 `ok:false` 로 커버된다. 빠진 건 "연결은 계속 살아있는데 원격 tasty 프로세스 자체가
  응답만 안 주는" 좁은 케이스뿐이다.
- **운영 비용 / 유지 부담**: `git_query_snapshot`/`git_query_diff` 의 wire 조립 필드명이
  `tasty-git-core` 의 struct 필드명과 1:1 이어야 plugin 쪽 `Deserialize` 가 깨지지 않는다 — 둘 중
  하나만 바꾸면 조용히 파싱 실패(로그만 남고 UI 는 그냥 에러 표시)로 이어진다.

## Alternatives Considered

- **처음 제안된 안: `popup.set_context.context` 확장**: 기각 — 그런 필드가 프로토콜에 없다(사실 오류).
  있었다 해도 dirty-gate 문제(항목 5)는 별도로 풀어야 했다.
- **snapshot 전용(최초 로드만 원격, 상호작용은 미지원)**: 기각 — refresh/worktree 전환/diff 클릭이
  전부 로컬 상태만 재바인딩하고 끝나 원격에서는 아무 반응이 없거나 stale 데이터를 계속 보여주게
  된다. `kind`/`worktree_path`/`diff_path` 를 처음부터 파라미터화해 확장 여지를 없앴다.
- **`git2::Repository` 자체를 wire 로 직렬화**: 애초에 불가능(라이브 핸들, FFI 리소스) — 항상
  plain data 로 변환해야 하므로, 그 plain data 타입을 host/plugin 공유 crate 로 만드는 이번
  결정과 자연히 합류한다.
- **`is_current` 를 서버가 세션 단위로 기억(고정 cwd 를 서버가 들고 있음)**: 기각 — 서버는
  요청마다 stateless 하게 discover 하는 게 다른 attach 커스텀 이벤트(list_dir 등)와 일관적이다.
  "고정 배지"는 순수 UI 프레젠테이션 관심사라 plugin(뷰 소유자) 이 들고 있는 편이 계층 분리에
  맞는다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin(별도 프로세스) 소유 popup 이 host 왕복에 대해 soft timeout 을 걸어야 하는 사례가
  git-viewer 말고도 반복되면 — 그때는 plugin SDK 레벨의 범용 "pending host call TTL" 헬퍼를
  설계한다(현재는 이 ADR 스코프에서 개별 구현하지 않음).
- host↔plugin 간 공유해야 하는 이벤트 key/plugin id 상수가 3 개 이상으로 늘어나면 — 그때는
  리터럴 중복 대신 build-time 검증(예: 매니페스트 `id` 와의 정합성 테스트) 또는 공유 상수 소스를
  마련한다.
- `truncated` 플래그의 세부 원인(status/log/diff 중 무엇이 잘렸는지)을 사용자에게 보여줘야 하는
  요구가 생기면 — 현재는 하나의 bool 로 뭉개고 UI 배너도 없다.

## References

- [ADR-0053](0053-native-file-picker-remote-attach-channel.md) — 동일 patttern 의 선행 사례(`list_dir_request`/`result`, soft timeout 설계)
- [ADR-0042](0042-fs-pick-file-native-dialog-host-delegation.md) — 로컬 전용 동기 `fs.pick_file` (본 ADR 이 왜 그 모델을 재사용할 수 없었는지의 대조군)
- [ADR-0028](0028-plugin-egui-mesh-render-channel.md) — egui-mesh popup 렌더 채널(`popup.set_context` dirty-gate)
- [ADR-0040](0040-occupancy-soft-hard-tiers-agent-occupant.md) — attach 점유 신뢰 모델
- [`docs/dev-guide/attach-behavior.md`](../dev-guide/attach-behavior.md) — attach mirror 전반 동작
- `crates/tasty-git-core/src/lib.rs` — 공유 조회 로직
- `src/core/attach_runtime.rs::handle_git_query_request` — 서버측 핸들러
- `src/app/attach_client.rs` — reader thread 파싱 + `MirrorEvent::GitQueryResult` 반영 + unicast forward
- `src/adapters/ipc/handler/git_viewer.rs` — `git_viewer.query` IPC 진입점
- `crates/tasty-plugin-git-viewer/src/main.rs` — plugin 측 remote 분기
