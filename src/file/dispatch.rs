//! 파일·URL 대상 분류와 GUI 핸들러 실행. 비동기 식별과 picker 결과는 원래 요청 대상을 유지한다.

#[cfg(feature = "gui")]
pub(crate) mod picker_apply;
#[cfg(feature = "gui")]
pub(crate) mod remote;

#[cfg(feature = "gui")]
use crate::runtime::engine_access::{EngineMut, EngineRef};
use std::path::PathBuf;

#[cfg(feature = "gui")]
use crate::file::format::DetectorId;
use crate::file::format::FileTarget;
use crate::file::handler::HandlerAction;
#[cfg(feature = "gui")]
use crate::file::handler::{FileHandler, HandlerId};
#[cfg(feature = "gui")]
use crate::state::RequestContext;
#[cfg(feature = "gui")]
use crate::state::{FileHandlerPickerData, PickerHandlerSummary};
#[cfg(feature = "gui")]
pub(crate) use picker_apply::{apply_file_picker_result, apply_identify_result};

#[cfg(feature = "gui")]
pub use crate::core::origin::FileDispatchOrigin;
#[cfg(feature = "gui")]
pub(crate) use crate::core::origin::require_origin_pane;

/// picker 표시와 핸들러 실행이 함께 옮기는 요청 출처와 크기 제한 정책.
#[cfg(feature = "gui")]
#[derive(Debug, Clone, Copy)]
pub struct FileDispatchPolicy {
    pub dispatch_origin: FileDispatchOrigin,
    pub ignore_size_limit: bool,
    /// 최종 결과를 받을 요청의 run([`crate::request_runs`]). 결과를 돌려받지 않는 요청은 None이다.
    pub run: Option<u64>,
}

/// 파일은 형식 식별을 거치고 http(s) URL은 바로 핸들러 선택·실행으로 전달한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchTarget {
    File(FileTarget),
    Url(String),
}

impl DispatchTarget {
    /// http(s) scheme과 비어 있지 않은 뒷부분만 확인한다. 완전한 URL 문법 검사는 아니다.
    pub fn http_url(uri: &str) -> Option<Self> {
        let (scheme, rest) = uri.split_once("://")?;
        let is_http = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
        (is_http && !rest.is_empty()).then(|| Self::Url(uri.to_string()))
    }

    #[cfg(feature = "gui")]
    pub fn display(&self) -> String {
        match self {
            Self::File(f) => f.display(),
            Self::Url(u) => u.clone(),
        }
    }

    fn surface_param_value(&self) -> String {
        match self {
            Self::File(f) => f.as_path().to_string_lossy().into_owned(),
            Self::Url(u) => u.clone(),
        }
    }

    /// 파일 경로만 file URI로 감싼다. 이미 URL인 대상은 그대로 OS opener에 넘긴다.
    fn system_open_uri(&self) -> String {
        match self {
            Self::File(f) => path_to_file_uri(f.as_path()),
            Self::Url(u) => u.clone(),
        }
    }
}

impl From<FileTarget> for DispatchTarget {
    fn from(target: FileTarget) -> Self {
        Self::File(target)
    }
}

/// OpenSurface가 URL을 받는다고 선언하는 파라미터 이름.
pub const URL_SURFACE_PARAM_KEY: &str = "url";

/// 파일은 모든 핸들러가 받는다. URL은 System 또는 url 키를 쓰는 OpenSurface만 받는다.
pub fn handler_accepts_target(action: &HandlerAction, target: &DispatchTarget) -> bool {
    match target {
        DispatchTarget::File(_) => true,
        DispatchTarget::Url(_) => match action {
            HandlerAction::System => true,
            HandlerAction::OpenSurface { param_key, .. } => param_key == URL_SURFACE_PARAM_KEY,
            HandlerAction::Ipc { .. } => false,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    /// file:// 접두사가 있는 입력에서 얻은 경로. 존재·절대 경로 여부는 별도로 확인해야 한다.
    FileTarget(PathBuf),
    External(String),
}

/// file://만 경로로 바꾸며 나머지는 외부 문자열로 둔다. 경로 존재나 scheme 허용 여부는 검사하지 않는다.
pub fn parse_link(uri: &str) -> LinkKind {
    if let Some(rest) = uri.strip_prefix("file://") {
        let path_str = if cfg!(windows) && looks_like_windows_drive_uri(rest) {
            rest.trim_start_matches('/')
        } else {
            rest
        };
        let decoded = percent_decode_lossy(path_str);
        return LinkKind::FileTarget(PathBuf::from(decoded));
    }
    LinkKind::External(uri.to_string())
}

fn looks_like_windows_drive_uri(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    bytes.len() >= 4
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes[3] == b'/' || bytes[3] == b'\\')
}

/// %xx를 바이트별 문자로 바꾼다. 잘못된 escape는 남기며 UTF-8 다중 바이트를 복원하지는 않는다.
fn percent_decode_lossy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]));
            if let (Some(a), Some(b)) = h {
                out.push(((a << 4) | b) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 빈 후보도 picker로 표시한다. 대상에 맞는 핸들러만 후보·최근 목록에 남긴다.
#[cfg(feature = "gui")]
pub(crate) fn open_picker(
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    target: DispatchTarget,
    detector: Option<DetectorId>,
    candidates: Vec<FileHandler>,
    candidates_are_fallback: bool,
    policy: FileDispatchPolicy,
) {
    open_picker_from_read(
        state,
        &engine.read(),
        target,
        detector,
        candidates,
        candidates_are_fallback,
        policy,
    );
}

/// Build only popup presentation from the borrowed handler catalog.
#[cfg(feature = "gui")]
pub(crate) fn open_picker_from_read(
    state: &mut RequestContext,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    target: DispatchTarget,
    detector: Option<DetectorId>,
    candidates: Vec<FileHandler>,
    candidates_are_fallback: bool,
    policy: FileDispatchPolicy,
) {
    let FileDispatchPolicy {
        dispatch_origin,
        ignore_size_limit,
        run,
    } = policy;
    // 이전 picker를 기다리던 요청은 결과 없이 끝난다.
    if let Some(guard) = state
        .dialogs
        .file_handler_picker
        .take()
        .and_then(|old| old.run)
    {
        finish_run(
            Some(guard.take()),
            crate::request_runs::RunState::Cancelled,
            "replaced by a newer file handler picker",
            serde_json::Value::Null,
        );
    }
    let recent_entries: Vec<(HandlerId, i64)> = state
        .file_handler_recent
        .list()
        .iter()
        .map(|e| (e.handler_id.clone(), e.last_used_at))
        .collect();
    let recent_handlers: Vec<(FileHandler, i64)> = recent_entries
        .iter()
        .filter_map(|(id, at)| {
            engine
                .file_handler
                .handler(id)
                .map(|handler| (handler, *at))
        })
        .collect();
    let (recent, cand) = picker_lists(&target, &recent_handlers, &candidates);
    // 전체 목록을 대체 후보로 쓴 경우에는 자동 실행할 기본 핸들러가 없다.
    let default_handler = (!candidates_are_fallback)
        .then(|| candidates.first().map(|h| h.id.clone()))
        .flatten();
    let target_display = target.display();
    state.dialogs.file_handler_picker = Some(FileHandlerPickerData {
        origin_surface_id: None,
        dispatch_origin,
        target,
        target_display,
        detector,
        candidates: cand,
        candidates_are_fallback,
        recent,
        default_handler,
        selected: None,
        result: None,
        ignore_size_limit,
        run: run.map(|run| {
            crate::request_runs::RunGuard::new(
                run,
                "the file handler picker closed without a choice",
            )
        }),
        remote_host: None,
    });
    let intent = crate::intent::UiIntent::OpenPopup {
        id: crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID,
        mode: crate::intent::OpenPopupMode::CenteredFocused,
    };
    state.dispatch_intent(match dispatch_origin {
        FileDispatchOrigin::User => intent.from_user_menu("file_dispatch"),
        FileDispatchOrigin::Agent | FileDispatchOrigin::PluginUnverified => intent.from_agent_ipc(),
    });
}

/// 최근 목록과 후보 양쪽에서 대상에 맞지 않는 항목을 빼고 중복 후보를 제거한다.
#[cfg(feature = "gui")]
fn picker_lists(
    target: &DispatchTarget,
    recent_handlers: &[(FileHandler, i64)],
    candidates: &[FileHandler],
) -> (Vec<PickerHandlerSummary>, Vec<PickerHandlerSummary>) {
    let recent_ids: Vec<&HandlerId> = recent_handlers.iter().map(|(h, _)| &h.id).collect();
    let recent = recent_handlers
        .iter()
        .filter(|(h, _)| handler_accepts_target(&h.action, target))
        .map(|(h, at)| handler_to_summary(h, Some(*at)))
        .collect();
    let cand = candidates
        .iter()
        .filter(|h| handler_accepts_target(&h.action, target))
        .filter(|h| !recent_ids.contains(&&h.id))
        .map(|h| handler_to_summary(h, None))
        .collect();
    (recent, cand)
}

/// 원격 경로는 로컬 핸들러에 넘기지 않도록 후보와 최근 목록이 모두 빈 picker를 만든다.
/// `remote_host`는 머리 경로 줄 앞 표지의 호스트 글자다(비면 `Remote` 낱말).
#[cfg(feature = "gui")]
pub(crate) fn open_remote_placeholder_picker(
    state: &mut RequestContext,
    target: FileTarget,
    remote_host: String,
) {
    let target = DispatchTarget::File(target);
    let target_display = target.display();
    state.dialogs.file_handler_picker = Some(FileHandlerPickerData {
        origin_surface_id: None,
        dispatch_origin: FileDispatchOrigin::Agent,
        target,
        target_display,
        detector: None,
        candidates: Vec::new(),
        candidates_are_fallback: false,
        recent: Vec::new(),
        default_handler: None,
        selected: None,
        result: None,
        ignore_size_limit: false,
        run: None,
        remote_host: Some(remote_host),
    });
    // 호출자는 링크 메뉴와 마우스 클릭뿐이라 사용자 요청으로 연다.
    state.dispatch_intent(
        crate::intent::UiIntent::OpenPopup {
            id: crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID,
            mode: crate::intent::OpenPopupMode::CenteredFocused,
        }
        .from_user_context_menu(),
    );
}

#[cfg(feature = "gui")]
fn handler_to_summary(h: &FileHandler, last_used_at: Option<i64>) -> PickerHandlerSummary {
    // 번역이 없으면 키 자체 대신 짧은 ID 표시를 사용하도록 None을 반환한다.
    let display_name = h.display_name_i18n_key.as_deref().and_then(|k| {
        let translated = crate::i18n::t(k);
        (translated != k).then(|| translated.to_string())
    });
    PickerHandlerSummary {
        id: h.id.clone(),
        display_name,
        owner: h.owner.clone(),
        surface_kind: match &h.action {
            HandlerAction::OpenSurface { surface_kind, .. } => Some(surface_kind.clone()),
            HandlerAction::Ipc { .. } | HandlerAction::System => None,
        },
        last_used_at,
    }
}

/// 요청의 run에 최종 결과를 남긴다. run이 없는 요청(사용자 조작 등)이면 하지 않는다.
#[cfg(feature = "gui")]
pub(crate) fn finish_run(
    run: Option<u64>,
    state: crate::request_runs::RunState,
    result: impl Into<String>,
    extra: serde_json::Value,
) {
    if let Some(run) = run {
        crate::request_runs::finish(run, state, result, extra);
    }
}

#[cfg(feature = "gui")]
pub(crate) fn fail_run(run: Option<u64>, reason: impl Into<String>) {
    finish_run(
        run,
        crate::request_runs::RunState::Failed,
        reason,
        serde_json::Value::Null,
    );
}

/// 핸들러 실행을 요청한다. true는 IPC 큐 등록·OS 위임·탭 생성 요청을 포함하며 최종 열기 성공은 아니다.
/// 명시 origin이 사라졌거나 대상 종류를 받지 못하거나 OS에 넘기지 못하면 false다.
/// 요청의 run은 OS 위임과 실패는 여기서, 탭 생성은 journal 완료가, IPC는 플러그인 응답이 끝낸다.
#[cfg(feature = "gui")]
pub fn execute_handler_action(
    core: &mut crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    handler: &FileHandler,
    target: &DispatchTarget,
    origin_surface_id: Option<u32>,
    policy: FileDispatchPolicy,
) -> bool {
    let FileDispatchPolicy {
        dispatch_origin,
        ignore_size_limit,
        run,
    } = policy;
    if let Err(reason) = handler_may_run(&engine.as_ref(), handler, target, origin_surface_id) {
        fail_run(run, reason);
        return false;
    }
    if let Some(run) = run {
        let action = match &handler.action {
            HandlerAction::OpenSurface { .. } => "open_surface",
            HandlerAction::Ipc { .. } => "ipc",
            HandlerAction::System => "system",
        };
        crate::request_runs::amend(
            run,
            None,
            serde_json::json!({ "handler": handler.id.as_str(), "action": action }),
        );
    }
    match &handler.action {
        HandlerAction::OpenSurface {
            surface_kind,
            param_key,
        } => {
            // 이유: 파일 크기 확인은 plugin이 맡으므로 호환 인자를 여기서는 사용하지 않는다.
            let _ = ignore_size_limit;

            let params = open_surface_params(param_key, target);
            open_surface_tab(
                core,
                state,
                engine,
                surface_kind,
                params,
                origin_surface_id,
                policy,
            )
        }
        HandlerAction::Ipc { method, .. } => enqueue_handler_ipc(state, method, target, run),
        HandlerAction::System => {
            let opened = open_system_target(state, target, dispatch_origin);
            if opened {
                finish_run(
                    run,
                    crate::request_runs::RunState::Succeeded,
                    "handed to the OS opener",
                    serde_json::Value::Null,
                );
            } else {
                fail_run(run, "the OS opener could not be started");
            }
            opened
        }
    }
}

/// 실행하지 못하는 이유를 돌려준다. 이유는 호출자에게 돌려줄 영어 문장이다.
#[cfg(feature = "gui")]
fn handler_may_run(
    engine: &EngineRef<'_>,
    handler: &FileHandler,
    target: &DispatchTarget,
    origin_surface_id: Option<u32>,
) -> Result<(), String> {
    if let Some(sid) = origin_surface_id
        && let Err(message) = require_origin_pane(engine, sid)
    {
        tracing::warn!("{message}");
        return Err(message);
    }
    if remote::rejects_handler_for_origin(engine, handler, origin_surface_id) {
        return Err(format!(
            "file handler {} cannot open a remote file",
            handler.id
        ));
    }
    if !handler_accepts_target(&handler.action, target) {
        tracing::warn!(
            handler_id = %handler.id,
            target = %target.display(),
            "file handler does not accept this dispatch target; not executed",
        );
        return Err(format!(
            "file handler {} does not accept {}",
            handler.id,
            target.display()
        ));
    }
    Ok(())
}

/// OS 기본 프로그램에 넘긴다. 사용자가 직접 연 요청의 실패만 토스트로 알리고
/// 에이전트·미증명 플러그인 요청에는 사용자 화면에 띄우지 않는다(docs/design/systems/toast.md).
#[cfg(feature = "gui")]
fn open_system_target(
    state: &mut RequestContext,
    target: &DispatchTarget,
    dispatch_origin: FileDispatchOrigin,
) -> bool {
    let uri = target.system_open_uri();
    let opened = crate::terminal_link::open_uri(&uri);
    if let Some(message) = system_open_feedback(opened, dispatch_origin, &target.display()) {
        state.toasts.push(
            message,
            crate::adapters::ui::ToastKind::Error,
            crate::adapters::ui::ToastScope::Window,
        );
    }
    opened
}

#[cfg(feature = "gui")]
fn system_open_feedback(
    opened: bool,
    dispatch_origin: FileDispatchOrigin,
    target: &str,
) -> Option<String> {
    if dispatch_origin != FileDispatchOrigin::User {
        return None;
    }
    crate::terminal_link::open_failure_message(opened, "toast.open_system_failed", target)
}

#[cfg(feature = "gui")]
fn enqueue_handler_ipc(
    state: &mut RequestContext,
    method: &str,
    target: &DispatchTarget,
    run: Option<u64>,
) -> bool {
    let DispatchTarget::File(file) = target else {
        tracing::warn!(method, "Ipc handler reached with a URL target");
        fail_run(run, format!("IPC handler {method} cannot take a URL"));
        return false;
    };
    state
        .pending_handler_ipc
        .push((method.to_string(), file.clone(), run));
    true
}

fn open_surface_params(param_key: &str, target: &DispatchTarget) -> serde_json::Value {
    serde_json::json!({ param_key: target.surface_param_value() })
}

/// origin이 있으면 그 pane에, 없으면 현재 pane에 탭 생성을 요청한다.
#[cfg(feature = "gui")]
pub(crate) fn open_surface_tab(
    _core: &mut crate::app::services::AppServices,
    state: &mut RequestContext,
    engine: &mut EngineMut<'_>,
    surface_kind: &str,
    params: serde_json::Value,
    origin_surface_id: Option<u32>,
    policy: FileDispatchPolicy,
) -> bool {
    let FileDispatchPolicy {
        dispatch_origin,
        run,
        ..
    } = policy;
    let origin_pane = match origin_surface_id
        .map(|sid| require_origin_pane(engine, sid))
        .transpose()
    {
        Ok(pane) => pane,
        Err(message) => {
            tracing::warn!("{message}");
            fail_run(run, message);
            return false;
        }
    };
    match origin_pane {
        Some(pane_id) => {
            // 비동기 에이전트 결과가 사용자 선택을 바꾸지 않도록 origin에 따라 선택 여부를 정한다.
            let intent = crate::app::command::DomainIntent::CreateTab {
                pane_id,
                cwd: None,
                kind: surface_kind.to_string(),
                name: None,
                surface_params: params.clone(),
                activate: dispatch_origin.selects_result(),
            };
            state.dispatch_intent(
                match dispatch_origin {
                    FileDispatchOrigin::User => intent.from_user_menu("file_dispatch"),
                    FileDispatchOrigin::Agent | FileDispatchOrigin::PluginUnverified => {
                        intent.from_agent_ipc()
                    }
                }
                .with_completion(run),
            );
        }
        None => {
            // 위임한 핸들러도 사용자·에이전트를 구분하므로 origin을 보존한다.
            let intent = crate::intent::Intent::NewTab {
                kind: Some(surface_kind.to_string()),
                params,
            };
            state.dispatch_intent(
                match dispatch_origin {
                    FileDispatchOrigin::User => intent.from_user_menu("file_dispatch"),
                    FileDispatchOrigin::Agent | FileDispatchOrigin::PluginUnverified => {
                        intent.from_agent_ipc()
                    }
                }
                .with_completion(run),
            );
        }
    }
    true
}

/// 경로 구분자를 바꾸고 file URI 접두사를 붙인다. 특수문자 percent-encoding은 하지 않는다.
fn path_to_file_uri(abs: &std::path::Path) -> String {
    let s = abs.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "gui")]
    #[test]
    fn only_a_users_failed_system_open_gets_a_toast_message() {
        for origin in [
            FileDispatchOrigin::User,
            FileDispatchOrigin::Agent,
            FileDispatchOrigin::PluginUnverified,
        ] {
            assert_eq!(system_open_feedback(true, origin, "/tmp/a.pdf"), None);
            let failed = system_open_feedback(false, origin, "/tmp/a.pdf");
            assert_eq!(
                failed.is_some(),
                origin == FileDispatchOrigin::User,
                "{origin:?}: {failed:?}"
            );
        }
    }

    #[test]
    fn parse_link_external_http() {
        assert_eq!(
            parse_link("https://example.com/a?b=1"),
            LinkKind::External("https://example.com/a?b=1".into()),
        );
    }

    #[test]
    fn parse_link_external_mailto() {
        assert_eq!(
            parse_link("mailto:a@b.com"),
            LinkKind::External("mailto:a@b.com".into()),
        );
    }

    #[test]
    fn parse_link_file_unix() {
        assert_eq!(
            parse_link("file:///home/u/a.md"),
            LinkKind::FileTarget(PathBuf::from("/home/u/a.md")),
        );
    }

    #[test]
    fn parse_link_file_percent_decoded() {
        assert_eq!(
            parse_link("file:///home/u/My%20Doc.md"),
            LinkKind::FileTarget(PathBuf::from("/home/u/My Doc.md")),
        );
    }

    #[test]
    fn parse_link_file_percent_passthrough_invalid() {
        assert_eq!(
            parse_link("file:///home/%ZZ/a"),
            LinkKind::FileTarget(PathBuf::from("/home/%ZZ/a")),
        );
    }

    #[cfg(feature = "gui")]
    fn handler(id: &str, action: HandlerAction) -> FileHandler {
        FileHandler {
            id: HandlerId::new(id),
            detector: DetectorId::new("markdown"),
            priority: 50,
            owner: crate::file::handler::HandlerOwner::Host,
            action,
            display_name_i18n_key: None,
            disabled: false,
        }
    }

    fn open_surface(kind: &str, key: &str) -> HandlerAction {
        HandlerAction::OpenSurface {
            surface_kind: kind.into(),
            param_key: key.into(),
        }
    }

    fn ipc() -> HandlerAction {
        HandlerAction::Ipc {
            method: "com.example.x.open".into(),
            owner_plugin_id: "com.example.x".into(),
        }
    }

    fn url(u: &str) -> DispatchTarget {
        DispatchTarget::http_url(u).expect("http(s) url")
    }

    fn file(p: &str) -> DispatchTarget {
        DispatchTarget::File(FileTarget::new(p))
    }

    #[test]
    fn http_url_accepts_only_http_and_https() {
        assert_eq!(
            DispatchTarget::http_url("https://example.com/page"),
            Some(DispatchTarget::Url("https://example.com/page".into())),
        );
        assert!(DispatchTarget::http_url("HTTP://example.com").is_some());
        assert_eq!(DispatchTarget::http_url("mailto:a@b.com"), None);
        assert_eq!(DispatchTarget::http_url("ftp://example.com/a"), None);
        assert_eq!(DispatchTarget::http_url("file:///tmp/a.md"), None);
        assert_eq!(DispatchTarget::http_url("https://"), None);
    }

    #[test]
    fn system_action_passes_url_through_unwrapped() {
        assert_eq!(
            url("https://example.com/page").system_open_uri(),
            "https://example.com/page",
        );
    }

    #[test]
    fn open_surface_action_forwards_url_as_param() {
        let params = open_surface_params("url", &url("https://example.com/page"));
        assert_eq!(
            params,
            serde_json::json!({ "url": "https://example.com/page" })
        );
    }

    #[test]
    fn path_targets_are_unchanged() {
        let t = file("/tmp/a.md");
        assert_eq!(t.system_open_uri(), "file:///tmp/a.md");
        assert_eq!(
            open_surface_params("file", &t),
            serde_json::json!({ "file": "/tmp/a.md" }),
        );
        for action in [
            open_surface("markdown", "file"),
            open_surface("html", "url"),
            ipc(),
            HandlerAction::System,
        ] {
            assert!(handler_accepts_target(&action, &t));
        }
    }

    #[test]
    fn url_target_acceptance_per_action_kind() {
        let t = url("https://example.com/page");
        assert!(handler_accepts_target(&HandlerAction::System, &t));
        assert!(handler_accepts_target(&open_surface("html", "url"), &t));
        assert!(!handler_accepts_target(
            &open_surface("markdown", "file"),
            &t
        ));
        assert!(!handler_accepts_target(&open_surface("x", "path"), &t));
        assert!(!handler_accepts_target(&ipc(), &t));
    }

    #[cfg(feature = "gui")]
    #[test]
    fn picker_lists_drop_handlers_that_cannot_take_a_url_from_recent_and_candidates() {
        let md = handler("host/md", open_surface("markdown", "file"));
        let html = handler("host/html", open_surface("html", "url"));
        let plugin = handler("com.example.x/open", ipc());
        let system = handler("host/system", HandlerAction::System);
        let recent = vec![
            (md.clone(), 1_700_000_000),
            (plugin.clone(), 1_700_000_100),
            (html.clone(), 1_700_000_200),
        ];
        let candidates = vec![md, html, plugin, system];

        let (recent_rows, cand_rows) =
            picker_lists(&url("https://example.com/page"), &recent, &candidates);
        let ids = |rows: &[PickerHandlerSummary]| -> Vec<String> {
            rows.iter().map(|r| r.id.as_str().to_string()).collect()
        };
        assert_eq!(ids(&recent_rows), vec!["host/html"]);
        assert_eq!(ids(&cand_rows), vec!["host/system"]);

        let (recent_rows, cand_rows) = picker_lists(&file("/tmp/a.md"), &recent, &candidates);
        assert_eq!(
            ids(&recent_rows),
            vec!["host/md", "com.example.x/open", "host/html"]
        );
        assert_eq!(ids(&cand_rows), vec!["host/system"]);
    }

    /// 최근 목록을 채워도 원격 경로 picker에 로컬 핸들러가 나타나지 않아야 한다.
    #[cfg(feature = "gui")]
    #[test]
    fn remote_placeholder_picker_carries_no_recent_even_when_recent_is_populated() {
        let (mut state, mut engine_session) = crate::state::tests::test_state();
        let mut engine = engine_session.borrow_mut();
        let any = engine
            .runtime
            .file_handler
            .all_handlers()
            .into_iter()
            .next()
            .expect("host default handlers exist");
        state.file_handler_recent.record(&any.id);

        open_picker(
            &mut state,
            &mut engine,
            file("/remote/a.md"),
            None,
            Vec::new(),
            false,
            crate::file::dispatch::FileDispatchPolicy {
                dispatch_origin: FileDispatchOrigin::Agent,
                ignore_size_limit: false,
                run: None,
            },
        );
        assert!(
            !state
                .dialogs
                .file_handler_picker
                .as_ref()
                .unwrap()
                .recent
                .is_empty()
        );
        assert!(
            state
                .dialogs
                .file_handler_picker
                .as_ref()
                .unwrap()
                .remote_host
                .is_none()
        );

        open_remote_placeholder_picker(&mut state, FileTarget::new("/remote/a.md"), "gx10".into());
        let picker = state.dialogs.file_handler_picker.as_ref().unwrap();
        assert!(picker.recent.is_empty());
        assert!(picker.candidates.is_empty());
        // 원격 경로 picker 만 머리에 호스트를 싣는다. 로컬 대상 picker 는 None 이다.
        assert_eq!(picker.remote_host.as_deref(), Some("gx10"));
    }

    #[cfg(windows)]
    #[test]
    fn parse_link_file_windows_drive() {
        assert_eq!(
            parse_link("file:///C:/Users/a.txt"),
            LinkKind::FileTarget(PathBuf::from("C:/Users/a.txt")),
        );
    }
}
