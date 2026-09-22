# ADR-0058: plugin이 트리거하는 host 소유 popup은 즉시 ack + 이벤트 push로 비동기 결과를 회신한다

- **Status**: Accepted
- **Date**: 2026-08-01
- **Tags**: plugin, ipc, popup, async, event-bus, host-delegation, file-picker, host-agnostic, adr-0042, adr-0043, adr-0053, adr-0056

## Context

markdown plugin의 "파일 열기" 팝업 Browse 버튼을 attach(원격) 환경에서도 쓰게 하려면,
plugin이 host 소유의 인터랙티브 popup(`file_picker`, `src/adapters/ui/popup/file_picker.rs`,
ADR-0053)을 열고 사용자가 그 popup에서 파일을 확정/취소할 때까지 **여러 프레임에 걸쳐**
기다렸다가 결과(경로 배열)를 나중에 돌려받아야 한다.

기존 `fs.pick_file`(ADR-0042)은 `host.call`의 **동기 inline dispatch**(같은 프레임에 응답)
위에 있다 — `rfd::FileDialog::pick_file()`이 OS native 모달이라 그 안에서 host 메인 스레드가
블로킹되는 동안에도 다른 문제가 없었다. 하지만 host 자체 egui popup(`file_picker`)은 OS가
대신 블로킹해주지 않는다 — popup은 host의 평범한 프레임 루프 위에서 매 프레임 그려지고,
사용자가 언제 확정할지는 host도 plugin도 알 수 없다. "요청 시점"과 "결과 회신 시점"이 완전히
분리된 진짜 비동기 통신이 필요한데, 이걸 어떻게 구현할지는 문서화된 적이 없었다.

**핵심 제약**: `crates/tasty-plugin-sdk/src/host.rs`의 `HostHandle::call`은 "호스트 IPC
메서드를 동기로 호출한다. 응답까지 `timeout`(기본 60초)만큼 block"한다
(`rx.recv_timeout(self.timeout)`, `host.rs:108-139`). 이 호출은 plugin의 popup 렌더/액션
처리 스레드에서 일어난다. 이 호출을 "host가 popup을 다 그리고 사용자가 확정할 때까지" 붙잡아
두는 방식(지연 회신)으로 쓰면, 그동안 **plugin 자신의 렌더/이벤트 루프가 멈춘다** — 다른
popup을 못 그리고, 자기 자신의 Cancel 같은 입력도 못 받고, 사용자가 파일 하나 고르는 데
60초 이상 걸리면(깊은 디렉토리 탐색에서 드물지 않다) 그냥 `HostCallTimeout` 에러로 끝난다.

**이미 있는 인프라**: `crates/tasty-plugin-protocol/src/protocol.rs`는 NDJSON 위에
`PluginRequest{method,params,id}`(요청-응답)와 `PluginEvent`(id 없는 비동기 알림)를 이미
1급으로 구분해 둔다. `PluginManager::emit_host_event_to_plugin`
(`crates/tasty-host-plugin/src/manager/events.rs:83`)은 "호스트가 명시적으로 보내는
메시지이므로 구독 등록 여부와 무관"하게 envelope을 정확히 한 plugin에만 push하는
owner-unicast 경로이며, `command.invoked`(`src/plugin_bridge/key_dispatch.rs:128`)가
실사용 중이다. plugin SDK의 `on_event`(`crates/tasty-plugin-sdk/src/plugin.rs:366`)는
`event.dispatch` 수신 시 이미 호출된다(`runtime.rs:573-577`) — host가 미는 메시지를 받을
구조가 이미 있다.

**직접적인 선례가 이미 프로덕션에 있다**: `git_viewer.query` IPC
(`src/adapters/ipc/handler/git_viewer.rs`, [ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md),
이 ADR 바로 직전에 Accepted됨)가 정확히 이 shape을 이미 구현·검증했다 — plugin이
`host.call("git_viewer.query", {...})`을 호출하면 host는 **즉시** `{request_id}`만 회신하고
(`handle_query`, `git_viewer.rs:40-71`), 실제 원격 attach 왕복이 끝나면
`PluginManager::emit_host_event_to_plugin`으로 `git_viewer.query_result` 이벤트를 plugin에
push한다(`src/app/attach_client.rs:1171`). git-viewer의 경우 비동기가 필요한 이유는 "원격
attach 세션 왕복 지연"이고, 이 ADR의 경우는 "사용자 인터랙션 지연"이라 원인은 다르지만,
"plugin→host 호출이 여러 프레임/여러 왕복 뒤에야 결과를 얻는다"는 구조는 동일하다.

git-viewer 사례와 이 사례의 차이점 하나는 중요하다: git-viewer의 popup은 **plugin 소유**
(egui-mesh, `[[contributes.popup]]`)라 로컬 모드는 IPC 없이 plugin 프로세스 내부에서 즉시
끝나고, 원격 모드만 `git_viewer.query`를 탄다. 반면 `file_picker`는 **host 소유**
popup(`PopupDef`)이라 로컬이든 원격이든 트리거 자체가 항상 cross-process다 — "즉시 끝나는
로컬 지름길"이 아예 없다. 즉 이 ADR의 케이스가 오히려 (b)를 더 강하게 요구한다.

ADR-0042는 "host 무지 원칙"(host는 이 메서드가 어느 plugin·어느 용도인지 몰라야 한다)을
`fs.pick_file` 하나에 대해서만 적었다. 이 ADR은 이 원칙을 "plugin이 트리거하는 host 소유
인터랙티브 popup" 전반에 재사용 가능한 패턴으로 일반화해야 한다.

## Decision

**(b) 즉시 ack + 이벤트 push**를 채택한다. (a) 지연 회신(`PluginToPluginNamespace` 패턴
재사용, `crates/tasty-host-plugin/src/manager/ipc_dispatch.rs:86`의 선례)은 구현이 더
간단하지만, 위 "핵심 제약"이 보여주듯 plugin 프로세스를 사용자 인터랙션이 끝날 때까지
멈춰 세우고 60초 타임아웃 위험까지 진다 — file_picker처럼 "사용자가 얼마나 걸릴지 알 수
없는" 인터랙션에는 구조적으로 맞지 않는다.

`file_picker`를 구체 사례로 메시지 흐름을 확정한다(이후 항목 6이 이 흐름을 재사용 가능한
일반 패턴으로 명명한다):

1. **트리거**: 신규 IPC `file_picker.trigger {filters?: string[]} → {request_id: u64}`.
   `gui` feature 전용(host GUI popup을 여는 메서드라 headless 빌드엔 없음, `fs.pick_file`과
   동일 근거). 권한은 **`FsRead`**로 등록한다 — 파일을 **고르는** 것은 read 관심사라는
   ADR-0042의 근거를 그대로 따른다(`fs.pick_file`/`git_viewer.query`가 이미 동일 근거로
   `FsRead`를 쓴다).
2. **host 핸들러** (신규 `src/adapters/ipc/handler/file_picker.rs`)는:
   - 새 monotonic `request_id`를 발급한다(`git_viewer.rs`의 `next_git_query_request_id()`와
     같은 패턴의 신규 카운터 — **주의**: 이 `request_id`는 ADR-0053의
     `FpLoadState::Loading{request_id, ..}`(popup 내부의 원격 디렉토리 나열 요청 상관관계
     id)와는 **완전히 별개의 네임스페이스**다. 둘 다 필드명이 `request_id`라 구현 시
     혼동하기 쉬우므로 코드 주석으로 명시해야 한다.
   - `(plugin_id, request_id)`를 popup의 요청자 정보로 기록한다 — `FilePickerData`
     (`src/state/dialogs.rs`)에 `requester: Option<FilePickerRequester>` 필드를 신설하고,
     Tools 메뉴가 여는 경우(`tools_menu.rs:123`)는 `None`으로 채운다.
   - 기존 `popup::file_picker::open(state, engine)`(`file_picker.rs:691`)을
     `open(state, engine, requester: Option<FilePickerRequester>)`으로 확장해 그대로
     호출한다 — popup의 로컬/원격 판별·엔트리 로드 로직(ADR-0053)은 전혀 손대지 않는다.
   - **popup 확정을 기다리지 않고** 그 자리에서 `JsonRpcResponse::success(id, json!({
     "request_id": request_id }))`를 반환한다. plugin의 `host.call`은 거의 즉시 돌아온다.
3. popup은 기존 로컬/원격 생명주기(ADR-0053)를 그대로 밟는다 — 사용자가 몇 프레임 뒤에
   확정하든 취소하든 이 ADR과 무관하게 동작한다.
4. **결과 회신**: 확정/취소가 일어나는 두 지점(`src/app/dispatch/file_picker.rs`의 로컬
   confirm 경로 `dispatch_pending_file_picker_results`, 원격 confirm 경로
   `apply_remote_confirm`)에서 `FilePickerData.requester`가 `Some`이면, 기존 동작(로컬은
   `DomainIntent::DispatchFile`, 원격은 클립보드 복사+toast)에 **더해** 다음을 호출한다:
   ```
   plugin_manager.emit_host_event_to_plugin(
       &requester.plugin_id,
       "file_picker.result",
       &json!({ "request_id": requester.request_id, "paths": paths }),
       EventScope::System,
   );
   ```
   취소(사용자가 popup을 그냥 닫음) 시에는 `{ "request_id": ..., "paths": [], "cancelled":
   true }`. 정확한 Rust 타입(derive된 struct vs 인라인 `json!`)은 구현에 맡기되, 이 이벤트의
   **key(`"file_picker.result"`)와 최소 wire 필드(`request_id`, `paths`, `cancelled`)는 이
   ADR이 고정**한다.
5. **plugin 수신**: 신규 콜백은 필요 없다 — 기존 `on_event`(`EventDispatchCtx`)가
   `envelope.key == "file_picker.result"`를 받는다. plugin은 자신이 `file_picker.trigger`
   호출 시 받은 `request_id`를 자체 pending-map에 들고 있다가 이 이벤트로 상관관계를
   맞춘다(자기 자신의 `host.call` in-flight 상태를 추적하는 것과 대칭적인 패턴).
6. `PendingRequestKind`/`send_surface_request`(host가 plugin에게 요청을 보내고 plugin의
   응답을 기다리는 **반대 방향** 패턴)는 재사용하지 않는다 — 이 흐름은 plugin이 시작하고
   결과가 **이벤트 채널**로 돌아오므로 그 패턴과 방향이 다르다.

**일반 패턴으로 명명(host 무지 원칙의 일반화)**: "plugin이 트리거하는 host 소유 인터랙티브
popup"은 앞으로 다음 명명 규약을 따른다 — `<popup_id>.trigger {…popup별 context} →
{request_id}`(동기 ack, 절대 사용자 인터랙션을 기다리지 않는다) + `<popup_id>.result` 이벤트
`{request_id, …popup별 결과}`(owner-unicast, `emit_host_event_to_plugin`). host는 어느
plugin이 왜 요청했는지 몰라도 되며(`context`/결과 필드는 순전히 plugin을 위한 것 —
`fs.pick_file`의 `filters`와 동일한 위치), 이 규약을 따르는 한 host 코드는 특정 plugin
이름을 하드코딩하지 않는다. 이걸 하나의 공유 generic IPC 메서드(예:
`host_popup.trigger {popup_id, context}`)로 통합하지는 **않는다** — popup마다 context/결과
페이로드 형태가 본질적으로 다르기 때문(근거는 아래 Alternatives Considered).

## Consequences

- **얻은 것**: plugin의 렌더/입력 루프가 popup 대기 중에도 절대 멈추지 않는다. 60초
  `HostCallTimeout` 위험이 원천 차단된다(트리거 자체는 즉시 끝나고, 결과 대기는 blocking
  호출이 아니다). `git_viewer.query`(ADR-0056)와 정확히 같은 shape이라 구현자가 새 패턴을
  발명하지 않고 기존 코드를 그대로 본떠 짤 수 있다. 로컬/원격 confirm이 이미 서로 다른 코드
  경로이므로, 이벤트 push 훅을 그 두 지점에 대칭적으로 추가하기만 하면 되고 popup 자체의
  로컬/원격 분기 로직(ADR-0053)은 전혀 건드리지 않는다. host 소유 popup은 자기 프레임
  루프가 매 tick 자체적으로 다시 그려지므로, ADR-0056이 plugin 소유 egui-mesh popup 때문에
  신설해야 했던 "강제 repaint 플래그"(`plugin_mesh_popup_pending_repaint`) 같은 장치가 이
  경우엔 필요 없다.
- **잃은 것**: `file_picker`가 "Tools 메뉴 단일 트리거"에서 "Tools 메뉴 + 임의 plugin"
  다중 트리거 대상으로 넓어져 동시성 정책(같은 시점에 두 요청이 겹치는 경우 거부/큐잉)이
  필요해진다 — 이 ADR은 그 정책 자체를 확정하지 않는다(구현 단계의 몫). `request_id`
  네임스페이스가 하나 더 늘어(ADR-0053의 내부 `list_dir` request_id와 별개) 구현 시 혼동
  가능성이 생긴다.
- **운영 비용 / 유지 부담**: `"file_picker.result"` 이벤트 key와 `request_id` 상관관계
  로직이 host/plugin 양쪽에 각자 정의된다(ADR-0056이 이미 겪은 것과 동일 계열 부채 — 둘을
  묶는 공유 crate가 없다). 이 패턴을 쓰는 popup이 늘어날수록 이 리터럴 중복도 늘어난다(아래
  Reconsideration Triggers 참고).

## Alternatives Considered

- **(a) 지연 회신**: `file_picker.trigger`의 `host.call` 자체를 popup 확정/취소 시점까지
  host가 응답하지 않고 붙잡아 두는 방식(`PluginToPluginNamespace` 패턴 재사용 가능,
  `send_ipc_result`를 확정 시점에 호출). 구현은 더 단순하지만, `HostHandle::call`의 60초
  기본 타임아웃과 plugin 단일 dispatch 스레드 블로킹 때문에 사용자가 파일 하나 고르는 데
  1분 넘게 걸리면 그냥 에러로 끝나고, 그동안 plugin은 자기 자신의 다른 popup/입력도 처리
  못 한다. 기각.
- **ADR-0043의 반대 방향(popup 소유권을 plugin으로 옮기기)**: markdown의 `file-open`처럼
  `file_picker` 자체를 plugin 소유 egui-mesh popup으로 재작성해, plugin이 자기 프로세스
  안에서 직접 여러 프레임을 기다리게 한다. 기각 — `file_picker`는 로컬/원격 겸용(ADR-0053)
  이고, 원격 브라우징은 host의 attach 세션 판별(`Workspace.mirror`)·mirror workspace
  상태·`read_dir_entries` 공유 로직에 이미 깊게 결합돼 있다. 이 전부를 plugin 프로세스로
  옮기면 attach "점유 = 신뢰" 모델과 mirror 세션 내부 상태를 plugin에도 노출해야 해서 신뢰
  경계가 넓어진다. 또한 `file_picker`는 애초에 여러 plugin이 재사용할 generic host UI로
  설계됐다(ADR-0053 자체가 Tools 메뉴 전용 host 기능으로 시작) — 특정 plugin에 소유권을
  주면 그 재사용성이 사라진다. markdown de-pluginize의 "비활성 시 host에 markdown 동작이
  남으면 안 된다" 원칙(`docs/dev-guide/plugin-development.md`)과도 무관하다 — `file_picker`는
  애초에 markdown 전용이 아닌 host generic 기능이라 이 원칙이 적용될 대상 자체가 아니다.
- **공유 generic IPC 메서드 하나로 모든 host popup 트리거 통합**(예: `host_popup.trigger
  {popup_id, context}` / `host_popup.result {popup_id, request_id, data}`): 지금은 기각.
  ADR-0043의 Alternative B와 같은 근거 — 각 popup의 context/결과 페이로드 형태가 이미
  서로 다르고(`file_picker`는 `filters`, 향후 색상 선택기라면 초기 색상 등), 하나의
  `Value` 필드로 뭉치면 타입 안정성이 사라지고 host 라우팅 코드의 의도가 흐려진다. 대신
  이름 규약(`<popup_id>.trigger`/`<popup_id>.result`)만 표준화하고 실제 IPC 메서드는
  popup별로 유지한다 — 재사용 필요성이 실증되면(아래 Reconsideration Triggers) 그때
  추출한다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 이 `<popup_id>.trigger`/`.result` 패턴을 `file_picker` 이후 **세 번째** host popup이
  재사용하려 할 때 — 공유 generic `host_popup.trigger`/`.result` 메서드로 추출할지 재검토
  (위 Alternatives 세 번째 항목).
- `file_picker`에 서로 다른 두 요청(다른 plugin 간, 또는 Tools 메뉴와 plugin 간)이 동시에
  겹치는 사례가 실사용에서 반복되면 — 현재 단일 인스턴스 가정이 깨지므로 큐잉/거부 정책을
  구현 디테일이 아니라 이 ADR 수준의 결정으로 승격한다.
- host IPC dispatch가 동기 inline 모델에서 완전히 벗어나(ADR-0042의 동일 Trigger 조건)
  `host.call` 자체가 더 이상 같은 틱에서 즉시 회신을 보장하지 않게 될 때 — 이 ADR의
  "트리거는 항상 즉시 ack" 전제가 깨진다.
- `<popup_id>.result`류 이벤트 key/plugin id 상수가 host/plugin 양쪽에 계속 늘어나(ADR-0056의
  동일 계열 Trigger) 리터럴 중복이 실질 유지부담이 되면 — 공유 상수 소스를 마련한다.

## References

- [ADR-0042](0042-fs-pick-file-native-dialog-host-delegation.md) — host 무지 원칙 최초 정의,
  본 ADR이 예견됐던 Reconsideration Trigger("host IPC dispatch가 async가 될 때")
- [ADR-0043](0043-convert-input-popup-capability.md) — 비슷한 "여러 프레임 대기" 문제를 popup
  소유권 이전으로 우회한 반대 사례(Alternatives Considered에서 대조)
- [ADR-0053](0053-native-file-picker-remote-attach-channel.md) — `file_picker` popup 자체,
  로컬/원격 겸용 설계, 본 ADR이 그대로 재사용하는 부분
- [ADR-0056](0056-git-viewer-remote-attach-git-query-channel.md) — `git_viewer.query`
  IPC, 본 ADR이 채택한 "즉시 ack + 이벤트 push" shape의 실제 프로덕션 선례
- [ADR-0498](0498-the-file-picker-trigger-answers-only-a-plugin-caller.md) — 이 메서드를 plugin 호출자에게만 연 후속 결정(CLI·agent 는 `-32016`)
- `crates/tasty-plugin-protocol/src/protocol.rs` — 전송 계층 메시지 타입(요청-응답 vs
  이벤트)
- `crates/tasty-host-plugin/src/manager/events.rs` — `emit_host_event_to_plugin`
- `crates/tasty-plugin-sdk/src/host.rs` — `HostHandle::call`의 블로킹 시맨틱(본 ADR의 핵심
  제약)
- `crates/tasty-plugin-sdk/src/runtime.rs`, `crates/tasty-plugin-sdk/src/plugin.rs` — plugin
  쪽 `on_event` 수신 경로
- `src/adapters/ipc/handler/git_viewer.rs` — 본 ADR이 직접 모델로 삼은 기존 구현
- `src/adapters/ui/popup/file_picker.rs`, `src/app/dispatch/file_picker.rs` — 이 ADR이
  트리거 대상으로 삼는 popup과 confirm dispatch 지점
- `src/state/dialogs.rs` — `FilePickerData`
