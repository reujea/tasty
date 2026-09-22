//! 요청이 **주인 창을 못 찾는** dispatch 메서드마다 그 이유를 적어 둔다.
//!
//! ## 이 명부가 답하는 물음
//!
//! `src/app/ipc/routing.rs` 는 요청의 주인 창을 못 찾으면 **포커스된 창**으로 보낸다.
//! [`docs/design/policies/focus.md`] 는 그 폴백이 답이 되는 순간 그 메서드가 포커스
//! 의존이 된다고 적는데, **어느 메서드가 그 상태인지**를 값으로 든 자리가 없었다.
//! 옆의 두 명부는 물음이 다르다:
//!
//! | | 묻는 것 |
//! |---|---|
//! | `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` | 이 **목록**을 합쳐야 하는가 |
//! | [`super::routing_key_method_scope`] 의 `PAIR_EXEMPT` | 메서드 한정 키를 그 한정 밖에서 읽는가 |
//! | 여기 | 주인 창을 **못 찾는** 요청이 왜 그래도 되는가 |
//!
//! 같은 메서드가 여러 명부에 다른 갈래로 들어갈 수 있다 — 합치면 그중 한 물음의
//! 답이 사라진다.
//!
//! ## 모수 — 이 스캔이 못 보는 것을 먼저 적는다
//!
//! 모수는 "`params.get("…")` 형태로 **라우팅이 인식하는 키**를 하나도 안 읽는 dispatch
//! arm" 이다. 그 술어가 대상 지목의 술어와 **같지 않다**는 것을 두 방향으로 적어 둔다.
//!
//! 1. **serde 로 읽는 키를 못 본다.** `serde_json::from_value::<Req>(params)` 로 받는
//!    핸들러는 키 리터럴이 구조체 필드 이름으로만 있어 이 스캔에 안 걸린다. 실측으로
//!    셋이 그렇다 — `markdown.navigate`(범용 키라 라우팅은 푼다) ·
//!    `git_viewer.query` · `file_handler.dispatch`. 앞의 둘은 명부에 남고,
//!    `file_handler.dispatch` 는 메서드 한정 origin 키로 라우팅되어 모수에서 빠진다.
//!    **짝인 두 가드도 같은 사각을 갖는다** — 그쪽도
//!    `params.get` 형태만 훑으므로 `local_surface_id` 는 인식
//!    목록에도 면제 목록에도 안 나온다.
//! 2. **`request_target` 밖에서 푸는 것을 못 본다.** `split` 의
//!    `target_surface`/`target_pane` 은 `App::find_request_owner` 가 문자열 축에서
//!    잇는다. 그래서 여기서는 지목 없음으로 잡히지만 실제로는 라우팅된다.
//! 3. **필터 키를 대상 키와 안 가른다** — `hook.list` 의 `surface_id` 는 주인 창을
//!    정하지 않는 필터인데(`crate::ipc::handler::hooks::handle_hook_list` 가 그것으로
//!    행을 거르기만 한다) 범용 키라 모수 밖으로 빠진다. **모수에서 빠지는 것과 폴백으로
//!    가는 것은 별개다** — `hook.list` 는 어느 쪽으로도 폴백에 안 닿는다:
//!    `dispatch_list_global` 이 `find_request_owner` 보다 **먼저** 돌아 합산으로
//!    단락시키고(`src/app/ipc/routing.rs` 의 step 5), 헤드리스는 engine 이 하나라
//!    폴백 자체가 없다. 그래서 여기 없는 것이 결함은 아니다. 갈리는 것은 짝인
//!    `global_hook.list` 와의 대칭뿐이다 — 그쪽은 읽는 키가 없어 모수에 들고 이 명부에
//!    `AggregatedList` 로 실려 있는데, 같은 갈래인 `hook.list` 는 술어가 안 봐서 안
//!    실린다. 즉 이 사각이 감추는 것은 폴백이 아니라 **같은 사유를 값으로 안 든 자리**다.
//!
//! 그래서 이 명부의 갈래는 "포커스로 새는가" 가 아니라 **"이 스캔이 지목을 못 본
//! 자리가 왜 그래도 되는가"** 다. 술어가 완벽하지 않다는 것을 명부가 갈래로 흡수한다 —
//! 술어를 더 정교하게 만드는 일(대상처럼 생긴 키를 이름이 아니라 성질로 정의하기)은
//! 아직 답이 없고, 그 조사가 끝나기 전까지 이 명부가 그 자리를 값으로 든다.
//!
//! ## 갈래를 왜 이렇게 갈랐나
//!
//! 물음은 하나다 — **주인 창이 안 정해져도 답이 옳은 이유가 무엇인가.** 이유가 다르면
//! 고치는 방법도 다르다: 저장소가 창 밖이면 고칠 것이 없고, 창 소유인데 합산이 답하면
//! 정본이 합산 명부이며, 생성이면 애초에 실을 id 가 없고, 열린 결함이면 축을 세워야
//! 한다. 갈래 이름만으로는 판정이 재현되지 않으므로 **모든 항목에 사유를 요구한다.**
//!
//! 자동 술어 대신 사유 명부로 판정하는 결정의 근거·대안: `docs/adr/0251-unrouted-dispatch-methods-carry-a-reason-not-a-predicate.md`.

use std::collections::{BTreeMap, BTreeSet};

use super::routing_key_method_scope as scope;

/// 주인 창이 안 정해져도 답이 옳은 이유.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Why {
    /// 판정 대상(저장소 또는 효과)이 창 밖에 있다 — `Core` · 프로세스 전역 · 파일 ·
    /// OS. 어느 창으로 가도 같은 답이라 라우팅할 것이 없다.
    NotWindowOwned,
    /// 창 소유이지만 `dispatch_list_global` 이 전 창을 합쳐 답한다.
    AggregatedList,
    /// 생성이라 실을 대상 id 가 애초에 없다. 요청이 닿은 창에 만들어진다.
    CreatesWithoutATarget,
    /// 대상이 있고 라우팅도 되지만 `request_target` 밖(`App::find_request_owner`)에서
    /// 풀려 이 스캔에 안 보인다.
    RoutedOutsideRequestTarget,
    /// 대상 키를 serde 구조체로 읽어 이 스캔이 못 본다. 라우팅이 그 키를 아는지는
    /// 사유 칸에 적는다.
    TargetReadByDeserializer,
    /// debug 표면. 사용자 조작 재현이라 포커스 독립 축의 범위 밖이다.
    DebugOnly,
    /// 창별 관측값임을 응답의 소유 ID로 명시한다. 상태를 바꾸거나 전역값으로 위장하지 않는다.
    ScopedObservation,
    /// **창 소유인데 대상 축도 합산도 없다 — 열린 결함.** 무엇이 막고 있는지를 사유에 적는다.
    PerWindowOpenDefect,
}
use Why::*;

/// (메서드, 갈래, 사유).
const ROSTER: &[(&str, Why, &str)] = &[
    (
        "memory.count",
        NotWindowOwned,
        "저장소가 `core.with_memory` 하나다 — 핸들러가 `_state`·`_engine` 을 받기만 하고 안 쓴다. memory store 는 `new_with_ids` 인자로 전 engine 이 같은 Arc 를 든다",
    ),
    (
        "memory.delete",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.exists",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.export",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    ("memory.gc", NotWindowOwned, "상동 — 같은 공유 memory store"),
    (
        "memory.get",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.import",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.list",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.put",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.query",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.scopes",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.stats",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.count",
        NotWindowOwned,
        "secret 도 같은 저장소다(`core.with_memory`) — 네임스페이스만 다르다",
    ),
    (
        "memory.secret.delete",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.exists",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.get",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.list",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.put",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.scopes",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "memory.secret.stats",
        NotWindowOwned,
        "상동 — 같은 공유 memory store",
    ),
    (
        "telemetry.summary",
        NotWindowOwned,
        "telemetry 행은 `core` 의 memory store 에 앉는다. 조회 핸들러의 `_state`·`_engine` 은 미사용이다",
    ),
    (
        "telemetry.timeseries",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.top",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.anomaly.list",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.cap.list",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.cap.remove",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.cap.reset",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.cap.status",
        NotWindowOwned,
        "상동 — 같은 공유 store, `_state`·`_engine` 미사용",
    ),
    (
        "telemetry.cap.set",
        NotWindowOwned,
        "저장은 같은 공유 store 다. `engine` 을 쓰는 곳은 cap id 를 짓는 `telemetry_seq` 하나이고, 그 카운터도 창 생성 경로가 첫 engine 것으로 공유시킨다(`App::ensure_engine_and_plugins`)",
    ),
    (
        "remote.profile.list",
        NotWindowOwned,
        "`RemoteProfiles::load()` — 파일에서 읽는다. 핸들러가 engine 도 state 도 안 받는다",
    ),
    (
        "remote.profile.get",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.profile.add",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.profile.remove",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.profile.import",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.profile.detect",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.profile.list_local",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.passkey.list",
        NotWindowOwned,
        "`Passkeys::load()` — 상동",
    ),
    (
        "remote.passkey.get",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.passkey.add",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "remote.passkey.remove",
        NotWindowOwned,
        "상동 — 파일 기반, engine 인자 없음",
    ),
    (
        "webhook.list",
        NotWindowOwned,
        "핸들러가 engine 도 state 도 안 받는다 — 웹훅 등록부는 프로세스 전역이다",
    ),
    (
        "webhook.info",
        NotWindowOwned,
        "상동 — 프로세스 전역 등록부",
    ),
    (
        "webhook.config",
        NotWindowOwned,
        "상동 — 프로세스 전역 등록부",
    ),
    (
        "webhook.register",
        NotWindowOwned,
        "상동 — 프로세스 전역 등록부",
    ),
    (
        "webhook.unregister",
        NotWindowOwned,
        "상동 — 프로세스 전역 등록부",
    ),
    (
        "webhook.sweep",
        NotWindowOwned,
        "상동 — 프로세스 전역 등록부",
    ),
    (
        "preset.list",
        NotWindowOwned,
        "`core.preset_store`(공유 Mutex). `_state` 는 받기만 하고 안 쓴다",
    ),
    ("preset.get", NotWindowOwned, "상동 — `core.preset_store`"),
    ("preset.save", NotWindowOwned, "상동 — `core.preset_store`"),
    (
        "preset.rename",
        NotWindowOwned,
        "상동 — `core.preset_store`",
    ),
    (
        "preset.delete",
        NotWindowOwned,
        "상동 — `core.preset_store`",
    ),
    (
        "session.list",
        NotWindowOwned,
        "`core` 만 받는다 — 세션 토큰은 창의 것이 아니다",
    ),
    ("session.issue", NotWindowOwned, "상동 — `core` 만 받는다"),
    ("session.revoke", NotWindowOwned, "상동 — `core` 만 받는다"),
    (
        "hook_handler.list",
        NotWindowOwned,
        "`hook_handler::global()` — 프로세스 전역이다. hook **핸들러**가 전역이고 hook **인스턴스**만 창 소유라는 구분이 여기서 갈린다",
    ),
    (
        "hook_handler.get",
        NotWindowOwned,
        "상동 — 전역 등록부. `id` 는 핸들러 **이름**(문자열)이라 창을 가리키지 않는다",
    ),
    (
        "hook_handler.upsert",
        NotWindowOwned,
        "상동 — 쓰기도 같은 전역 등록부에 닿고 영속 대상은 `~/.tasty/hook-handlers.toml` 파일 하나다. 어느 창으로 가도 같은 것을 고친다",
    ),
    (
        "hook_handler.remove",
        NotWindowOwned,
        "상동 — `upsert` 의 짝. 창 소유 상태를 하나도 안 읽는다",
    ),
    ("hook_handler.reload", NotWindowOwned, "상동 — 전역 등록부"),
    (
        "hook_handler.dispatch",
        NotWindowOwned,
        "상동 — 전역 등록부. `id` 는 핸들러 **이름**(문자열)이라 창을 가리키지 않는다",
    ),
    (
        "completion_strategy.list",
        NotWindowOwned,
        "`completion_strategy::global()` — 프로세스 전역",
    ),
    (
        "agent.rate_limit_set",
        NotWindowOwned,
        "한도는 `core` 에 앉는다 — 핸들러의 `_state`·`_engine` 은 미사용이다",
    ),
    (
        "agent.rate_limit_list",
        NotWindowOwned,
        "상동 — `core` 소유, `_state`·`_engine` 미사용",
    ),
    (
        "agent.rate_limit_status",
        NotWindowOwned,
        "상동 — `core` 소유, `_state`·`_engine` 미사용",
    ),
    (
        "agent.rate_limit_remove",
        NotWindowOwned,
        "상동 — `core` 소유, `_state`·`_engine` 미사용",
    ),
    (
        "approval.get",
        NotWindowOwned,
        "`engine.approval_store` 를 읽지만 그 Arc 는 전 창이 공유한다 — 창 생성 경로(`App::ensure_engine_and_plugins`)가 첫 engine 의 것으로 덮어쓴다. 생성자만 읽으면 창별로 보인다",
    ),
    (
        "approval.respond",
        NotWindowOwned,
        "상동 — `approval_store` Arc 는 창 생성 경로가 공유시킨다",
    ),
    (
        "approval.cancel",
        NotWindowOwned,
        "상동 — `approval_store` Arc 는 창 생성 경로가 공유시킨다",
    ),
    (
        "settings.get_remote_transfer",
        NotWindowOwned,
        "`engine.settings` 는 창마다의 사본이지만 값이 하나다 — `App::cascade_settings_updated` 가 모든 main window 에 같은 `Settings` 를 써 넣는다",
    ),
    (
        "settings.get_plugin_setting",
        NotWindowOwned,
        "상동 — 설정은 전 창 동일 사본",
    ),
    (
        "settings.set_remote_transfer",
        NotWindowOwned,
        "상동 — 설정은 전 창 동일 사본",
    ),
    (
        "theme.query",
        NotWindowOwned,
        "테마는 `crate::theme::theme()` 로 프로세스 전역이고 `ui_zoom` 만 `engine.settings` 에서 온다 — 그 설정도 전 창 동일 사본이다",
    ),
    (
        "surface.kinds",
        NotWindowOwned,
        "`engine.surface_registry` 는 창 생성 경로가 첫 engine 의 Arc 로 공유시킨다 — 그래야 plugin 이 register 한 kind 가 두 번째 창에서도 보인다",
    ),
    (
        "surface.raw_key",
        NotWindowOwned,
        "효과가 창이 아니라 **OS 전역**이다(CGEvent 키 주입). `engine` 은 `--enable-input-simulation` 게이트를 확인하는 데만 쓰인다",
    ),
    (
        "surface.switch_input_source",
        NotWindowOwned,
        "상동 — macOS 입력 소스 전환은 시스템 전역이다",
    ),
    (
        "file_handler.reload",
        NotWindowOwned,
        "`engine.file_format`·`engine.file_handler` 를 다시 읽는데, 그 둘도 창 생성 경로가 첫 engine 의 Arc 로 공유시킨다 — 한 번 reload 하면 전 창에 반영된다",
    ),
    (
        "workspace.list",
        AggregatedList,
        "워크스페이스는 창 소유이고 id 가 `IdGenerator` 공유라 이어 붙이면 키가 된다. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "surface.list",
        AggregatedList,
        "상동 — surface id 공유. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "pane.list",
        AggregatedList,
        "상동 — pane id 공유. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "pty.list",
        AggregatedList,
        "상동 — headless pty id 공유. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "output.observe_list",
        AggregatedList,
        "상동 — observer id 공유. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "workspace_category.list",
        AggregatedList,
        "상동 — category id 공유. 예약 `normal`(id 0)만 한 줄로 접는다. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "image.list",
        AggregatedList,
        "상동 — 항목의 키가 `surface_id` 다. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "global_hook.list",
        AggregatedList,
        "상동 — global hook id 공유. 지목은 `Kind::GlobalHook` 이 따로 푼다. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "attach.list",
        AggregatedList,
        "상동 — 두 배열의 키가 `surface_id`·`workspace_id` 다. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "tree",
        AggregatedList,
        "이름이 `*.list` 가 아니라 이름 기반 census 에서 빠져 있었다 — 성질은 같다. 합산 여부의 정본은 `crates/tasty-doc-guards/tests/window_owned_lists_are_classified.rs` 의 명부다",
    ),
    (
        "workspace_category.create",
        CreatesWithoutATarget,
        "상동 — `name` 만 받는다",
    ),
    (
        "pty.spawn",
        CreatesWithoutATarget,
        "상동 — `request_target` 의 doc 이 이 형태를 명시한다(대상이 아니라 생성이라 실을 id 자체가 없다)",
    ),
    (
        "global_hook.set",
        CreatesWithoutATarget,
        "상동 — `condition`·`command` 만 받는다. 만들어진 훅의 지목은 `Kind::GlobalHook` 이 따로 푼다",
    ),
    (
        "attach.into_gui",
        CreatesWithoutATarget,
        "어느 **로컬** 창에 mirror 를 붙일지 지목하는 인자가 없다 — `workspace` 는 원격 workspace id 라 로컬 라우팅에 안 쓴다(`request_target` 의 doc 이 그렇게 적는다). 요청이 닿은 창의 `pending_gui_attach` 에 큐잉된다",
    ),
    (
        "file_picker.trigger",
        CreatesWithoutATarget,
        "대상 인자가 없다 — 팝업은 요청이 닿은 창의 `state.dialogs` 에 뜬다. `owner_popup_instance` 는 plugin 쪽 팝업 식별자지 창을 안 가리킨다",
    ),
    (
        "split",
        RoutedOutsideRequestTarget,
        "대상은 있고 라우팅도 된다 — `target_surface`/`target_pane` 을 `App::find_request_owner` 가 푼다(값이 id 와 nickname 을 겸해 문자열로 실리므로 숫자만 보는 `request_target` 밖에서 이어 푼다). 두 키가 `params_resource_id` 의 배열에 없어 이 스캔에는 지목 없음으로 보인다",
    ),
    (
        "markdown.navigate",
        TargetReadByDeserializer,
        "`surface_id` 를 serde 구조체(`NavigateReq`)로 읽는다. 그 키는 `params_resource_id` 의 범용 키라 **라우팅은 푼다** — `params.get(\"…\")` 형태만 훑는 이 스캔이 못 볼 뿐이다",
    ),
    (
        "debug.banner.show",
        DebugOnly,
        "debug 표면이다. 사용자 조작을 재현하는 검증 도구라 release 에 없고(불가침 원칙 1), 포커스 독립성은 *에이전트 기능*에 거는 요구라 같은 잣대로 재지 않는다",
    ),
    (
        "debug.banner.close",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.banner.list",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.banner.set_countdown",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.host_popup.open",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.host_popup.close",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.host_popup.list",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.modifier_hint.hold",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.modifier_hint.state",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.tool.invoke",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.tool.list",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.switch_tab",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.switch_workspace",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.close_workspace",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.settings.apply",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "debug.gpu.stall",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "ui.state",
        DebugOnly,
        "상동 — debug 표면, 포커스 독립 축의 범위 밖",
    ),
    (
        "notification.list",
        AggregatedList,
        "engine별 패널 저장소를 전 창/parked에서 모으고 공유 생성 ID 역순으로 전체 50개만 반환한다. UI 패널과 읽음 상태의 소유권은 각 engine에 남는다",
    ),
    (
        "system.pressure",
        NotWindowOwned,
        "프로세스 게이지다 — 값은 `Core` 의 원자값과 plugin manager 가 올리는 왕복 누계이고 창이 하나도 없는 headless 에서도 같은 자리가 센다. 어느 창으로 가도 같은 답이라 라우팅할 대상이 없다",
    ),
    (
        "system.info",
        ScopedObservation,
        "version은 전역이고 기존 count/index는 조회 engine 값이다. scope=engine, workspace_ids, active_workspace_id, layout_slot으로 귀속을 명시한다. 기존 ID 라우팅으로 비포커스 engine을 조회할 수 있고 전역 목록은 workspace.list가 소유한다",
    ),
    (
        "recent.query",
        NotWindowOwned,
        "`RecentFiles::load` 가 state.db 에 귀속된 캐시의 공유 핸들을 반환한다. 모든 창이 같은 목록을 읽고 쓰므로 요청이 닿은 창에 따라 답이 바뀌지 않는다",
    ),
    (
        "git_viewer.query",
        TargetReadByDeserializer,
        "`local_surface_id` 를 serde 로 읽어 창별 큐에 넣지만 App이 모든 main 큐를 drain한 뒤 전역 attach_client_sessions에서 그 ID의 세션을 찾는다. 큐를 받은 창이 조회 대상을 결정하지 않는다. parked 큐의 drain 여부와 실제 SSH 성공은 별도 생명주기/전송 축이다",
    ),
];

/// dispatch arm 수의 하한 — **연기 검사**다. 파서가 죽으면 예외가 아니라 조용한 0 이
/// 되고, 모수가 비면 아래 집합 동등은 양쪽이 빈 집합이라 그냥 통과한다.
/// 값의 근거: 2026-09-09 실측 **261 개**.
const MIN_METHODS: usize = 200;

/// 라우팅이 인식하는 키를 하나도 안 읽는 dispatch 메서드.
fn unrouted_methods() -> BTreeSet<String> {
    let routing = scope::routing_source();
    // 모양 필터를 걸지 않는다 — `surface`·`parent`·`target`·`pane` 은 `_id` 로 안
    // 끝나지만 라우팅이 인식하는 대상 키다. 빼고 물으면 `terminal.*` 일곱이 지목
    // 없음으로 잡힌다(실측).
    let generic = scope::generic_keys_all(&routing);
    assert!(!generic.is_empty(), "범용 키를 못 뽑았다 — 대조군이 죽었다");
    let scoped: BTreeSet<String> = scope::scoped_pairs(&routing)
        .into_iter()
        .map(|(m, _)| m)
        .collect();
    assert!(!scoped.is_empty(), "한정 쌍을 못 뽑았다 — 대조군이 죽었다");

    let files = scope::handler_sources();
    let index = scope::fn_index(&files);
    let root_src = files
        .iter()
        .find(|(rel, _)| rel == scope::HANDLER_ROOT)
        .map(|(_, s)| s.clone())
        .unwrap_or_default();
    let mut keys_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (names, expr) in scope::dispatch_arms(&root_src) {
        let mut seen = BTreeSet::new();
        let keys = scope::reachable_keys_with(
            &index,
            &[],
            &expr,
            scope::RESOLVE_DEPTH,
            &mut seen,
            scope::params_keys_in,
        );
        for name in names {
            keys_of
                .entry(name)
                .or_default()
                .extend(keys.iter().cloned());
        }
    }
    assert!(
        keys_of.len() >= MIN_METHODS,
        "dispatch arm 을 {} 개만 걷었다(하한 {MIN_METHODS}). 파서가 죽으면 아래 집합 \
         동등은 양쪽이 빈 집합이라 그냥 통과한다",
        keys_of.len()
    );
    keys_of
        .into_iter()
        .filter(|(m, keys)| !scoped.contains(m) && !keys.iter().any(|k| generic.contains(k)))
        .map(|(m, _)| m)
        .collect()
}

/// 명부와 스캔 결과가 **집합 동등**이다 — 새 메서드가 지목 없이 들어오는 것과
/// 면제가 stale 이 되는 것을 둘 다 잡는다.
#[test]
fn every_unrouted_method_is_classified() {
    let found = unrouted_methods();
    let listed: BTreeSet<String> = ROSTER.iter().map(|(m, _, _)| (*m).to_string()).collect();
    let missing: Vec<&String> = found.difference(&listed).collect();
    let stale: Vec<&String> = listed.difference(&found).collect();
    assert!(
        missing.is_empty(),
        "주인 창을 못 찾는데 명부에 없는 메서드다. 갈래와 사유를 달아 `ROSTER` 에 \
         적어라 — 적히지 않은 것이 조용히 포커스된 창으로 간다:\n  {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "명부에 있는데 스캔에 안 잡히는 메서드다 — 지목이 생겼거나 arm 이 사라졌다. \
         생겼으면 지우고, 사라졌으면 이름을 맞춰라:\n  {}",
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn every_entry_carries_a_reason() {
    let empty: Vec<&str> = ROSTER
        .iter()
        .filter(|(_, _, why)| why.trim().is_empty())
        .map(|(m, _, _)| *m)
        .collect();
    assert!(empty.is_empty(), "사유가 빈 명부 항목: {empty:?}");
}

/// 한 메서드가 두 행에 있으면 빨감 — 갈래가 둘이면 답도 둘이고 다음 사람은 먼저 읽은
/// 쪽을 믿는다. 기대값을 명부에서 도출하므로 명부가 자라도 손볼 데가 없다.
#[test]
fn no_method_is_listed_twice() {
    let names: BTreeSet<&str> = ROSTER.iter().map(|(m, _, _)| *m).collect();
    assert_eq!(
        names.len(),
        ROSTER.len(),
        "같은 메서드가 여러 행에 있다 — 어느 갈래가 옳은지 정해 한 행만 남겨라"
    );
}

/// 열린 결함의 수를 박아 둔다. 고쳤으면 갈래를 옮기고 이 수를 함께 내려라 — 남겨
/// 두면 다음 사람이 이미 닫힌 것을 다시 센다.
#[test]
fn the_open_ones_are_not_silently_emptied() {
    let open: Vec<&str> = ROSTER
        .iter()
        .filter(|(_, w, _)| *w == PerWindowOpenDefect)
        .map(|(m, _, _)| *m)
        .collect();
    assert_eq!(
        open.len(),
        0,
        "창 소유인데 대상 축도 합산도 없는 항목의 수가 바뀌었다: {open:?}"
    );
}
