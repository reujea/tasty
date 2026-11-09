//! 라우팅 키 읽기를 찾지 못한 메서드를 이유별로 분류한다.
//! 저장소가 창 밖에 있거나 전 창 목록을 합치는 경우, 생성 요청, 다른 방식으로 대상을 찾는 경우를 구별한다.
//! 창 소유 상태인데 대상 지정도 집계도 없으면 해결되지 않은 결함으로 기록한다(docs/design/policies/focus.md).
//!
//! 검사 결과가 실제 라우팅 실패 목록은 아니다. params 읽기 문법과 제한된 호출 추적을 사용하므로
//! serde 필드나 request_target 밖의 대상 해석을 놓칠 수 있다. 필터 키와 대상 키도 완전히 구별하지 못한다.
//! 예를 들어 hook.list는 surface_id를 읽어 목록에서 빠지지만, 실제로는 전 창 집계가 먼저 처리한다.
//! 이 명부는 목록 집계 정책과 메서드별 키 예외 명부를 대신하지 않는다.

use std::collections::{BTreeMap, BTreeSet};

use super::routing_key_method_scope as scope;

/// 소스 검사에서 라우팅 키를 찾지 못한 이유.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Why {
    /// 저장소나 효과가 창 밖에 있어 창 선택에 의존하지 않는다.
    NotWindowOwned,
    /// 창 소유 목록을 전 창에서 모아 응답한다.
    AggregatedList,
    /// 생성 전에는 대상 ID가 없어 요청을 받은 창에 만든다.
    CreatesWithoutATarget,
    /// request_target 밖에서 대상을 해석한다.
    RoutedOutsideRequestTarget,
    /// serde로 읽어 소스 검색에서 빠진 키. 실제 라우팅 여부는 사유에 적는다.
    TargetReadByDeserializer,
    /// 사용자 조작을 재현하는 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.
    DebugOnly,
    /// 창별 조회라는 사실을 응답의 소유 ID로 명시한다.
    ScopedObservation,
    /// 창 소유 상태인데 대상 지정도 집계도 없는 미해결 문제.
    PerWindowOpenDefect,
}
use Why::*;

const ROSTER: &[(&str, Why, &str)] = &[
    (
        "clipboard.set_text",
        NotWindowOwned,
        "OS 클립보드 하나를 AppServices 의 클립보드 포트로 쓴다. 창 상태를 읽지 않는다. engine 라우터 갈래는 App 층을 거치지 않는 plugin 호출용이다.",
    ),
    (
        "image.open",
        RoutedOutsideRequestTarget,
        "journal create_spec가 surface_id를 읽어 conversion 대상을 고정한다. handler의 journal-only 거절 경로에는 키 읽기가 없다.",
    ),
    (
        "memory.count",
        NotWindowOwned,
        "모든 engine이 new_with_ids로 같은 memory store Arc를 받는다. 핸들러는 core.with_memory를 사용하고 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "memory.delete",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.exists",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.export",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.gc",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.get",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.import",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다. 항목별 surface scope는 GUI 라우터가 붙인 모든 engine의 열린 surface로 판정한다.",
    ),
    (
        "memory.list",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.put",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.query",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.scopes",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.stats",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.count",
        NotWindowOwned,
        "secret 도 같은 저장소다(`core.with_memory`) — 네임스페이스만 다르다",
    ),
    (
        "memory.secret.delete",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.exists",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.get",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.list",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.put",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.scopes",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "memory.secret.stats",
        NotWindowOwned,
        "모든 창이 같은 memory store를 공유한다.",
    ),
    (
        "telemetry.summary",
        NotWindowOwned,
        "telemetry는 core의 공유 memory store에 저장한다. 조회 핸들러는 state·engine 인자를 사용하지 않는다.",
    ),
    (
        "telemetry.timeseries",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.top",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.anomaly.list",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.cap.list",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.cap.remove",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.cap.reset",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.cap.status",
        NotWindowOwned,
        "공유 store를 사용하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "telemetry.cap.set",
        NotWindowOwned,
        "공유 memory store에 저장한다. cap ID를 만드는 telemetry_seq도 창 생성 때 첫 engine의 카운터를 공유한다.",
    ),
    (
        "remote.profile.list",
        NotWindowOwned,
        "`RemoteProfiles::load()` — 파일에서 읽는다. 핸들러가 engine 도 state 도 안 받는다",
    ),
    (
        "remote.profile.get",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.profile.add",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.profile.remove",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.profile.import",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.profile.detect",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.profile.list_local",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.passkey.list",
        NotWindowOwned,
        "Passkeys::load로 파일에서 읽으며 창별 상태가 아니다.",
    ),
    (
        "remote.passkey.get",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.passkey.add",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "remote.passkey.remove",
        NotWindowOwned,
        "파일에서 읽거나 쓰며 engine 인자를 받지 않는다.",
    ),
    (
        "webhook.list",
        NotWindowOwned,
        "핸들러가 engine 도 state 도 안 받는다 — 웹훅 등록부는 프로세스 전역이다",
    ),
    (
        "webhook.info",
        NotWindowOwned,
        "프로세스 전역 웹훅 등록부를 사용한다.",
    ),
    (
        "webhook.config",
        NotWindowOwned,
        "프로세스 전역 웹훅 등록부를 사용한다.",
    ),
    (
        "webhook.register",
        NotWindowOwned,
        "프로세스 전역 웹훅 등록부를 사용한다.",
    ),
    (
        "webhook.unregister",
        NotWindowOwned,
        "프로세스 전역 웹훅 등록부를 사용한다.",
    ),
    (
        "webhook.sweep",
        NotWindowOwned,
        "프로세스 전역 웹훅 등록부를 사용한다.",
    ),
    (
        "preset.list",
        NotWindowOwned,
        "`core.preset_store`(공유 Mutex). `_state` 는 받기만 하고 안 쓴다",
    ),
    (
        "preset.get",
        NotWindowOwned,
        "창들이 공유하는 core.preset_store를 사용한다.",
    ),
    (
        "preset.save",
        NotWindowOwned,
        "창들이 공유하는 core.preset_store를 사용한다.",
    ),
    (
        "preset.rename",
        NotWindowOwned,
        "창들이 공유하는 core.preset_store를 사용한다.",
    ),
    (
        "preset.delete",
        NotWindowOwned,
        "창들이 공유하는 core.preset_store를 사용한다.",
    ),
    (
        "session.list",
        NotWindowOwned,
        "`core` 만 받는다 — 세션 토큰은 창의 것이 아니다",
    ),
    (
        "session.issue",
        NotWindowOwned,
        "공유 core에서 세션 토큰을 처리한다.",
    ),
    (
        "session.revoke",
        NotWindowOwned,
        "공유 core에서 세션 토큰을 처리한다.",
    ),
    (
        "hook_handler.list",
        NotWindowOwned,
        "hook_handler::global은 프로세스 전역이다. hook 인스턴스가 창에 속하는 것과 구별한다.",
    ),
    (
        "hook_handler.get",
        NotWindowOwned,
        "전역 등록부의 id는 문자열 핸들러 이름이며 창 ID가 아니다.",
    ),
    (
        "hook_handler.upsert",
        NotWindowOwned,
        "전역 훅 핸들러 등록부와 공통 hook-handlers.toml을 수정하므로 창별 상태가 아니다.",
    ),
    (
        "hook_handler.remove",
        NotWindowOwned,
        "전역 훅 핸들러를 제거하며 창별 상태를 읽지 않는다.",
    ),
    (
        "hook_handler.reload",
        NotWindowOwned,
        "전역 훅 핸들러 등록부를 사용한다.",
    ),
    (
        "hook_handler.dispatch",
        NotWindowOwned,
        "전역 등록부의 id는 문자열 핸들러 이름이며 창 ID가 아니다.",
    ),
    (
        "hook_handler.result",
        NotWindowOwned,
        "run 값으로 프로세스 전역 실행 기록(request_runs)을 조회한다. run은 창 ID가 아니며 창 상태를 읽지 않는다.",
    ),
    (
        "run.result",
        NotWindowOwned,
        "run 값으로 프로세스 전역 실행 기록(request_runs)을 조회하고 호출자 신원으로 읽을 수 있는 run만 돌려준다. 창 상태를 읽지 않는다.",
    ),
    (
        "completion_strategy.list",
        NotWindowOwned,
        "`completion_strategy::global()` — 프로세스 전역",
    ),
    (
        "agent.report_append",
        NotWindowOwned,
        "workspace ID는 주소 문자열 안에 있고 task 저장소는 모든 창이 같은 memory store다. 핸들러는 engine의 공유 task_scope만 사용한다.",
    ),
    (
        "agent.rate_limit_set",
        NotWindowOwned,
        "한도는 core가 소유하며 핸들러는 state·engine 인자를 사용하지 않는다.",
    ),
    (
        "agent.rate_limit_list",
        NotWindowOwned,
        "core의 한도를 처리하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "agent.rate_limit_status",
        NotWindowOwned,
        "core의 한도를 처리하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "agent.rate_limit_remove",
        NotWindowOwned,
        "core의 한도를 처리하며 state·engine 인자는 사용하지 않는다.",
    ),
    (
        "approval.get",
        NotWindowOwned,
        "창 생성 때 engine.approval_store를 첫 engine의 Arc로 맞춰 모든 창이 같은 저장소를 사용한다.",
    ),
    (
        "approval.respond",
        NotWindowOwned,
        "창 생성 때 공유한 approval_store Arc를 사용한다.",
    ),
    (
        "approval.cancel",
        NotWindowOwned,
        "창 생성 때 공유한 approval_store Arc를 사용한다.",
    ),
    (
        "settings.get_remote_transfer",
        NotWindowOwned,
        "engine별 settings 사본은 App::cascade_settings_updated가 같은 Settings로 갱신한다.",
    ),
    (
        "settings.get_input_rules",
        NotWindowOwned,
        "Settings are synchronized across all main windows by cascade_settings_updated",
    ),
    (
        "settings.set_input_rule",
        NotWindowOwned,
        "Settings are synchronized across all main windows by cascade_settings_updated",
    ),
    (
        "settings.remove_input_rule",
        NotWindowOwned,
        "Settings are synchronized across all main windows by cascade_settings_updated",
    ),
    (
        "settings.initialize_input_rule",
        NotWindowOwned,
        "Settings are synchronized across all main windows by cascade_settings_updated",
    ),
    (
        "settings.get_plugin_setting",
        NotWindowOwned,
        "설정 사본은 모든 창에 같은 값으로 동기화한다.",
    ),
    (
        "settings.set_remote_transfer",
        NotWindowOwned,
        "설정 사본은 모든 창에 같은 값으로 동기화한다.",
    ),
    (
        "settings.get_keybinding",
        NotWindowOwned,
        "단축키 설정은 모든 창에 같은 값으로 동기화한 engine.settings에서 읽는다.",
    ),
    (
        "theme.query",
        NotWindowOwned,
        "테마는 프로세스 전역이고 ui_zoom은 모든 창에 동기화된 engine.settings에서 읽는다.",
    ),
    (
        "surface.kinds",
        NotWindowOwned,
        "창 생성 때 surface_registry를 첫 engine의 Arc로 맞춰 플러그인이 등록한 형식을 모든 창에서 공유한다.",
    ),
    (
        "surface.raw_key",
        NotWindowOwned,
        "CGEvent 키 주입은 OS 전역에 영향을 준다. engine은 enable-input-simulation 설정 확인에 사용한다.",
    ),
    (
        "surface.switch_input_source",
        NotWindowOwned,
        "macOS 입력 소스는 창별이 아닌 시스템 설정이다.",
    ),
    (
        "file_handler.reload",
        NotWindowOwned,
        "file_format과 file_handler는 창 생성 때 같은 Arc를 공유하므로 reload 결과가 모든 창에 반영된다.",
    ),
    (
        "file_handler.detectors",
        NotWindowOwned,
        "모든 창이 공유하는 engine.runtime.file_format Arc를 조회한다.",
    ),
    (
        "workspace.list",
        AggregatedList,
        "창별 워크스페이스를 공유 IdGenerator의 고유 ID로 모은다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "surface.list",
        AggregatedList,
        "공유 surface ID로 전 창 목록을 합친다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "pane.list",
        AggregatedList,
        "공유 pane ID로 전 창 목록을 합친다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "pty.list",
        AggregatedList,
        "공유 PTY ID로 목록을 합친다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "output.observe_list",
        AggregatedList,
        "공유 observer ID로 목록을 합친다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "workspace_category.list",
        AggregatedList,
        "공유 category ID로 목록을 합치고 예약된 normal(id 0)은 하나만 남긴다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "image.list",
        AggregatedList,
        "surface_id로 전 창의 이미지 목록을 합친다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "global_hook.list",
        AggregatedList,
        "공유 global hook ID로 목록을 합친다. 대상은 Kind::GlobalHook으로 찾고 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "attach.list",
        AggregatedList,
        "surface_id·workspace_id로 두 목록을 합친다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "tree",
        AggregatedList,
        "이름은 list가 아니지만 창별 트리를 합쳐 응답한다. 집계 정책은 window_owned_lists_are_classified 명부에서 확인한다.",
    ),
    (
        "pty.spawn",
        CreatesWithoutATarget,
        "새 PTY를 생성하는 요청이므로 기존 PTY ID가 없다.",
    ),
    (
        "global_hook.set",
        CreatesWithoutATarget,
        "condition과 command로 새 훅을 만든다. 생성 뒤의 대상 선택은 Kind::GlobalHook으로 처리한다.",
    ),
    (
        "file_picker.trigger",
        CreatesWithoutATarget,
        "대상 창 인자가 없어 요청을 받은 창의 dialogs에 팝업을 만든다. owner_popup_instance는 플러그인 팝업 ID다.",
    ),
    (
        "markdown.navigate",
        TargetReadByDeserializer,
        "NavigateReq의 surface_id를 serde로 읽는다. 라우팅의 범용 키이므로 대상은 찾지만 params.get 형태만 읽는 검사에서는 놓친다.",
    ),
    (
        "debug.banner.show",
        DebugOnly,
        "사용자 조작을 재현하는 debug 기능이다. release에는 없으며 에이전트 기능의 포커스 독립 규칙과 구별한다.",
    ),
    (
        "debug.banner.close",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.banner.list",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.banner.set_countdown",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.host_popup.open",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.host_popup.close",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.host_popup.list",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.webview.overlay",
        DebugOnly,
        "WebView 오버레이 지원 보고를 덮어쓰는 debug 재현 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.modifier_hint.hold",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.modifier_hint.state",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.tool.invoke",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.tool.list",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.switch_tab",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.switch_workspace",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.close_workspace",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.settings.apply",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "debug.gpu.stall",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "ui.state",
        DebugOnly,
        "사용자 조작 재현용 debug 기능으로 포커스 독립 규칙의 적용 대상이 아니다.",
    ),
    (
        "notification.list",
        AggregatedList,
        "engine별 패널 저장소를 전 창/parked에서 모으고 공유 생성 ID 역순으로 전체 50개만 반환한다. UI 패널과 읽음 상태의 소유권은 각 engine에 남는다",
    ),
    (
        "system.pressure",
        NotWindowOwned,
        "Core의 원자값과 플러그인 왕복 누계를 읽는 프로세스 압력 조회다. 창이 없는 헤드리스에서도 같은 집계를 사용한다.",
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
        "local_surface_id를 serde로 읽어 창별 큐에 넣는다. App이 모든 main 큐를 처리한 뒤 전역 attach_client_sessions에서 ID로 세션을 찾으므로 큐를 받은 창이 조회 대상을 정하지 않는다. parked 큐 처리와 실제 SSH 성공은 별도 확인이 필요하다.",
    ),
];

/// 2026-09-09 dispatch 메서드 261개를 측정한 뒤 빈 수집을 찾도록 둔 하한이다.
const MIN_METHODS: usize = 200;

fn unrouted_methods() -> BTreeSet<String> {
    let routing = scope::routing_source();
    // surface·parent·target·pane도 라우팅 키이므로 id 형태만으로 제한하지 않는다.
    let generic = scope::generic_keys_all(&routing);
    assert!(!generic.is_empty(), "범용 라우팅 키를 추출하지 못했다");
    let scoped: BTreeSet<String> = scope::scoped_pairs(&routing)
        .into_iter()
        .map(|(m, _)| m)
        .collect();
    assert!(
        !scoped.is_empty(),
        "메서드 한정 라우팅 쌍을 추출하지 못했다"
    );

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
        "dispatch 메서드를 {}개만 읽었다(하한 {MIN_METHODS}). 추출 범위를 확인한다.",
        keys_of.len()
    );
    keys_of
        .into_iter()
        .filter(|(m, keys)| !scoped.contains(m) && !keys.iter().any(|k| generic.contains(k)))
        .map(|(m, _)| m)
        .collect()
}

#[test]
fn every_unrouted_method_is_classified() {
    let found = unrouted_methods();
    let listed: BTreeSet<String> = ROSTER.iter().map(|(m, _, _)| (*m).to_string()).collect();
    let missing: Vec<&String> = found.difference(&listed).collect();
    let stale: Vec<&String> = listed.difference(&found).collect();
    assert!(
        missing.is_empty(),
        "이 검색에서 라우팅 키를 찾지 못했으나 ROSTER에 없는 메서드다. 실제 대상 선택을 확인해 분류와 사유를 적는다:\n  {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "ROSTER에 있지만 현재 검색 대상에 없는 메서드다. 키 읽기·이름 변경·삭제 여부를 확인해 명부를 갱신한다:\n  {}",
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

#[test]
fn no_method_is_listed_twice() {
    let names: BTreeSet<&str> = ROSTER.iter().map(|(m, _, _)| *m).collect();
    assert_eq!(
        names.len(),
        ROSTER.len(),
        "같은 메서드가 여러 행에 있다. 실제 사유에 맞는 분류 하나로 정리한다."
    );
}

/// 미해결 문제의 수가 줄면 분류와 기록을 함께 갱신한다.
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
        "창 소유인데 대상 지정도 집계도 없는 미해결 항목 수가 바뀌었다: {open:?}"
    );
}
