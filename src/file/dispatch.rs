//! 파일 디스패치 helper 잔존 모듈.
//!
//! mouse.rs ctrl+click, drag&drop, explorer plugin, IPC `file_handler.dispatch`
//! 가 모두 `DomainIntent::DispatchFile` 발화로 통일된다. Core::apply 가
//! `engine.identify_worker.spawn(...)` 호출 → 비동기 detect → AppEvent::IdentifyDone
//! → [`apply_identify_result`] 호출. picker 결과는
//! `App::dispatch_pending_picker_results` → [`apply_file_picker_result`].
//!
//! 본 모듈에는 *parse_link* (URI 분류) 와 위 두 적용 함수가 호출하는 helper
//! (`open_picker`, `execute_handler_action`) 가 있다. 두 적용 함수 자신은 창 상태를
//! 바꾸는 GUI 동작이라 gui 로 가린 하위 모듈 [`picker_apply`] 에 있다.
//!
//! 모듈 선언은 `cfg(any(feature = "gui", test))` 다 — headless 라이브러리에는 부르는 자리가
//! 없고, 링크 해석과 대상 판정은 시험이 headless 에서도 부른다. 창 상태를 바꾸는 helper
//! (핸들러 실행·picker)는 항목마다 `cfg(feature = "gui")` 다.

#[cfg(feature = "gui")]
pub(crate) mod picker_apply;

use std::path::PathBuf;

#[cfg(feature = "gui")]
use crate::file::format::DetectorId;
use crate::file::format::FileTarget;
use crate::file::handler::HandlerAction;
#[cfg(feature = "gui")]
use crate::file::handler::{FileHandler, HandlerId};
#[cfg(feature = "gui")]
use crate::state::AppState;
#[cfg(feature = "gui")]
use crate::state::{FileHandlerPickerData, PickerHandlerSummary};
#[cfg(feature = "gui")]
pub(crate) use picker_apply::{apply_file_picker_result, apply_identify_result};

/// 정의는 도메인(`core::origin`)에 있다 — 도메인의 `DispatchFile` intent 와 identify 포트가
/// 이 값을 싣고, 도메인은 이 모듈(창 상태를 받는 GUI 동작)을 부르지 않는다(ADR-0440).
#[cfg(feature = "gui")]
pub use crate::core::origin::FileDispatchOrigin;
#[cfg(feature = "gui")]
pub(crate) use crate::core::origin::require_origin_pane;

/// 핸들러 dispatch 의 대상 — 파일 경로 또는 `http(s)` URL.
///
/// 식별(`FileFormatRegistry::identify`)은 `File` 만 받는다. `Url` 은 detector 를 거치지
/// 않고 picker 와 액션 실행으로 곧장 간다. 각 액션이 URL 을 어떻게 다루는지는
/// [`handler_accepts_target`] 과 `execute_handler_action` 이 정한다 — 결정 근거는
/// `docs/adr/0272-url-targets-enter-the-handler-picker-not-identify.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchTarget {
    File(FileTarget),
    /// `http://` 또는 `https://` URL 원문. [`DispatchTarget::http_url`] 로만 만든다.
    Url(String),
}

impl DispatchTarget {
    /// `http(s)://` URL 이면 `Url` 대상을 만든다. 다른 scheme(mailto/ssh/ftp 등)은
    /// 핸들러로 열 곳이 없어 `None` — 그쪽은 OS opener 경로에 남는다.
    pub fn http_url(uri: &str) -> Option<Self> {
        let (scheme, rest) = uri.split_once("://")?;
        let is_http = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
        (is_http && !rest.is_empty()).then(|| Self::Url(uri.to_string()))
    }

    /// picker 헤더 등 화면 표시용 문자열.
    #[cfg(feature = "gui")]
    pub fn display(&self) -> String {
        match self {
            Self::File(f) => f.display(),
            Self::Url(u) => u.clone(),
        }
    }

    /// `OpenSurface` 파라미터로 넘길 값 — 경로는 lossy 문자열, URL 은 원문.
    fn surface_param_value(&self) -> String {
        match self {
            Self::File(f) => f.as_path().to_string_lossy().into_owned(),
            Self::Url(u) => u.clone(),
        }
    }

    /// `System` 액션이 OS opener 에 넘길 URI. 경로는 `file://` URI 로 감싸고 URL 은
    /// 원문 그대로 — URL 을 `path_to_file_uri` 에 통과시키면 `file:///https://…` 가 된다.
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

/// `URL 을 받는 surface 파라미터` 의 이름. `OpenSurface` 핸들러는 이 키를 선언했을 때만
/// URL 대상의 후보가 된다 — html 핸들러(`param_key = "url"`)가 본보기다. 다른 키
/// (`file`/`path`)는 그 surface 가 로컬 파일 경로를 기대한다는 선언이다.
pub const URL_SURFACE_PARAM_KEY: &str = "url";

/// 이 핸들러가 이 대상을 받을 수 있는지. 파일은 모든 핸들러가 받는다. URL 은
/// `System`(OS opener) 과 `param_key = "url"` 인 `OpenSurface` 만 받는다. `Ipc` 는
/// plugin 에 `path` 키로 보내는 규약이라 URL 을 받지 않는다.
///
/// picker 후보 · recent 목록 · 최종 실행 세 자리가 모두 이 판정 하나를 쓴다.
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

/// 클릭/드롭된 URI 의 종류.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    /// 식별 가능한 파일/디렉토리 경로 (file:// URI 또는 plain absolute path).
    FileTarget(PathBuf),
    /// webbrowser::open 으로 위임할 외부 URI (http, https, mailto, ftp, ssh, …).
    External(String),
}

/// URI 를 두 종류 중 하나로 분류. 경로 존재 검증은 안 함 — 호출자(`terminal_link::
/// resolve_path`) 가 이미 검증한 결과를 받는다.
pub fn parse_link(uri: &str) -> LinkKind {
    if let Some(rest) = uri.strip_prefix("file://") {
        // `file://` URI 규약:
        //   Unix:    file:///abs/path   → "/abs/path"
        //   Windows: file:///C:/path    → "/C:/path" — drive 문자 앞 / 한 개 strip
        // 양쪽 모두 앞 "/" 가 1개 더 붙어 있을 수 있다. Windows 만 drive 문자가
        // 뒤따르면 strip.
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

/// `/<letter>:/...` 모양인지 (Windows file URI 의 drive prefix).
fn looks_like_windows_drive_uri(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    bytes.len() >= 4
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes[3] == b'/' || bytes[3] == b'\\')
}

/// percent-decode (`%20` 등). 잘못된 escape 는 원본 보존.
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

/// Picker popup 을 띄운다. 후보가 비어도 호출 — empty-state UI 가 보여진다.
///
/// `candidates_are_fallback` 이 true 면 `candidates` 는 detector 매칭이 아니라
/// `FileHandlerRegistry::all_handlers()` fallback 목록 — `recent` 와 겹치는 항목은
/// 첫 그룹에서 제외한다(같은 목록 안의 두 그룹이므로 중복 표시가 된다).
///
/// 대상이 받지 못하는 핸들러([`handler_accepts_target`])는 후보에서도 recent 에서도
/// 뺀다 — recent 는 `candidates` 와 무관하게 저장 파일에서 읽히므로 따로 거른다.
#[cfg(feature = "gui")]
pub(crate) fn open_picker(
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    target: DispatchTarget,
    detector: Option<DetectorId>,
    candidates: Vec<FileHandler>,
    candidates_are_fallback: bool,
    dispatch_origin: FileDispatchOrigin,
    ignore_size_limit: bool,
) {
    let recent_entries: Vec<(HandlerId, i64)> = engine
        .file_handler_recent
        .list()
        .iter()
        .map(|e| (e.handler_id.clone(), e.last_used_at))
        .collect();
    let recent_handlers: Vec<(FileHandler, i64)> = recent_entries
        .iter()
        .filter_map(|(id, at)| engine.file_handler.get(id).map(|h| (h, *at)))
        .collect();
    let (recent, cand) = picker_lists(&target, &recent_handlers, &candidates);
    // fallback 후보는 이 형식에 매칭된 것이 아니라 전체 핸들러라 기본이 없다. 매칭
    // 후보일 때만 정렬 1순위가 "그냥 열었으면 실행됐을" 핸들러다([`apply_identify_result`]
    // 가 같은 첫 항목을 자동 실행한다).
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
    });
    state
        .popups
        .open_centered_focused(crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID);
}

/// picker 의 두 그룹(recent, 후보)을 만든다 — **한 목록 안의 두 묶음**이다. 둘 다
/// 대상이 받지 못하는 핸들러를 빼고, 후보는 recent 와 겹치는 항목을 뺀다(같은 목록에
/// 두 번 나온다). recent 에서 걸러진 핸들러는 후보 쪽 중복 제거에도 쓰이지 않는다 —
/// 걸러졌으면 어느 그룹에도 없다.
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

/// 원격(mirror) surface 의 경로 링크용 빈 picker. 화면 경로가 원격 호스트 경로라 로컬
/// 핸들러로 열 수 없으므로 후보도 **recent 도** 싣지 않는다 — `open_picker` 는 recent 를
/// 저장 파일에서 채우므로, 그것을 쓰면 사용자가 recent 를 골라 로컬 핸들러가 원격 경로로
/// 실행된다.
#[cfg(feature = "gui")]
pub(crate) fn open_remote_placeholder_picker(state: &mut AppState, target: FileTarget) {
    let target = DispatchTarget::File(target);
    let target_display = target.display();
    state.dialogs.file_handler_picker = Some(FileHandlerPickerData {
        origin_surface_id: None,
        // 원격 placeholder 는 실행 경로가 없다(핸들러 후보도 recent 도 싣지 않는다).
        // 어느 값이어도 선택 정책에 닿지 않으므로 보수적인 쪽을 둔다.
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
    });
    state
        .popups
        .open_centered_focused(crate::adapters::ui::popup::file_handler_picker::PICKER_POPUP_ID);
}

#[cfg(feature = "gui")]
fn handler_to_summary(h: &FileHandler, last_used_at: Option<i64>) -> PickerHandlerSummary {
    // 키가 번역 테이블에 없으면 `t` 가 키를 그대로 돌려준다 — 그것은 표시명이 아니라
    // 선언이 안 풀린 것이므로 `None` 으로 떨어뜨려 화면이 id 조각을 쓰게 한다.
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

/// 단일 handler action 을 실행. OpenSurface 는 즉시, Ipc 는 큐로, System 은
/// webbrowser 위임.
///
/// `origin_surface_id` 가 Some 이면 OpenSurface 는 그 surface 가 속한 *Pane* 에
/// 새 tab 으로 결과를 추가한다 (focus 독립). None 이면 focused pane 의 새 탭
/// (기존 동작). Ipc / System 의 payload 는 유지하지만 소멸한 origin 은 실행하지 않는다.
///
/// 대상을 받지 못하는 핸들러([`handler_accepts_target`])면 아무것도 실행하지 않고
/// `false` 를 돌려준다 — picker 가 이미 걸렀어도 실행 지점이 마지막 방어선이다.
#[cfg(feature = "gui")]
pub fn execute_handler_action(
    core: &mut crate::core::Core,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    handler: &FileHandler,
    target: &DispatchTarget,
    origin_surface_id: Option<u32>,
    dispatch_origin: FileDispatchOrigin,
    ignore_size_limit: bool,
) -> bool {
    if !handler_may_run(engine, handler, target, origin_surface_id) {
        return false;
    }
    match &handler.action {
        HandlerAction::OpenSurface {
            surface_kind,
            param_key,
        } => {
            // 대용량 파일 확인 게이트는 **plugin 소유**로 이전됐다: 크기 감지도 확인 팝업도
            // plugin in-process(`crates/tasty-plugin-markdown`)가 소유하고, host 는 파일
            // 크기를 stat 하지 않는다(불가침 원칙 — host 는 특정 kind 의 크기게이트를 모른다).
            // `ignore_size_limit` 은 옛 게이트의 우회 플래그였으므로 게이트 제거 후엔 소비만
            // 한다(dispatch 파이프라인 호출부 시그니처는 그대로 유지).
            let _ = ignore_size_limit;

            let params = open_surface_params(param_key, target);
            return open_surface_tab(
                core,
                state,
                engine,
                surface_kind,
                params,
                origin_surface_id,
                dispatch_origin,
            );
        }
        HandlerAction::Ipc { method, .. } => {
            return enqueue_handler_ipc(state, method, target);
        }
        HandlerAction::System => open_system_target(target),
    }
    true
}

/// 실행 전 두 관문 — origin surface 가 살아 있는 Pane 에 속하는가, 핸들러가 이 대상을
/// 받는가. 어느 쪽이든 막히면 사유를 남기고 `false`.
#[cfg(feature = "gui")]
fn handler_may_run(
    engine: &crate::core::CoreState,
    handler: &FileHandler,
    target: &DispatchTarget,
    origin_surface_id: Option<u32>,
) -> bool {
    if let Some(sid) = origin_surface_id
        && let Err(message) = require_origin_pane(engine, sid)
    {
        tracing::warn!("{message}");
        return false;
    }
    if !handler_accepts_target(&handler.action, target) {
        tracing::warn!(
            handler_id = %handler.id,
            target = %target.display(),
            "file handler does not accept this dispatch target; not executed",
        );
        return false;
    }
    true
}

/// `HandlerAction::System` — OS 기본 opener 만 호출한다(core/state/engine 미사용).
#[cfg(feature = "gui")]
fn open_system_target(target: &DispatchTarget) {
    let uri = target.system_open_uri();
    crate::terminal_link::open_uri(&uri);
}

/// Preserve the existing path-only plugin payload, with a final type check.
#[cfg(feature = "gui")]
fn enqueue_handler_ipc(state: &mut AppState, method: &str, target: &DispatchTarget) -> bool {
    let DispatchTarget::File(file) = target else {
        tracing::warn!(method, "Ipc handler reached with a URL target");
        return false;
    };
    state
        .pending_handler_ipc
        .push((method.to_string(), file.clone()));
    true
}

/// `OpenSurface` 액션이 surface 에 넘길 파라미터 — `{param_key: 대상}`. URL 대상은
/// 원문이 그대로 간다(html 핸들러 `param_key = "url"` → webview 가 그 URL 을 연다).
fn open_surface_params(param_key: &str, target: &DispatchTarget) -> serde_json::Value {
    serde_json::json!({ param_key: target.surface_param_value() })
}

/// OpenSurface 결과를 실제 tab 으로 연다. `origin_surface_id` 가 Some 이면 그 surface
/// 의 *Pane* 에 새 tab(focus 독립), None 이면 focused pane 의 새 탭.
#[cfg(feature = "gui")]
pub(crate) fn open_surface_tab(
    core: &mut crate::core::Core,
    state: &mut AppState,
    engine: &mut crate::core::CoreState,
    surface_kind: &str,
    params: serde_json::Value,
    origin_surface_id: Option<u32>,
    dispatch_origin: FileDispatchOrigin,
) -> bool {
    let origin_pane = match origin_surface_id
        .map(|sid| require_origin_pane(engine, sid))
        .transpose()
    {
        Ok(pane) => pane,
        Err(message) => {
            tracing::warn!("{message}");
            return false;
        }
    };
    match origin_pane {
        Some(pane_id) => {
            // 이 분기는 인텐트 계층을 거치지 않고 Core 로 직접 apply 하므로(링크 클릭 등
            // origin surface 의 pane 에 새 탭), 최근 목록 기록을 여기서 직접 한다. None
            // 분기는 `Intent::NewTab` 으로 위임되어 tab 핸들러가 기록한다. kind 하드코딩
            // 없이 매니페스트 `records_recent` 를 선언한 kind 만 기록(generic per-kind).
            let records_recent = engine
                .surface_registry
                .get(surface_kind)
                .is_some_and(|d| d.records_recent);
            // 에이전트가 명시 origin 으로 연 결과는 **선택하지 않는다** — 비동기 완료가
            // 사용자의 현재 탭을 갈아치우면 안 된다(ADR-0279). 사용자가 방금 그 pane 에서
            // 직접 연 것은 그 반대다: 보려고 연 것이므로 선택한다(ADR-0302).
            let intent = crate::core::intent::DomainIntent::CreateTab {
                pane_id,
                cwd: None,
                kind: surface_kind.to_string(),
                name: None,
                surface_params: params.clone(),
                activate: dispatch_origin.selects_result(),
            };
            if let Err(e) = core.apply(engine, intent) {
                tracing::warn!(
                    pane_id,
                    kind = %surface_kind,
                    "file_dispatch CreateTab failed: {e}",
                );
                return false;
            }
            if records_recent {
                state.record_recent(surface_kind, &params);
            }
        }
        None => {
            // 출처를 그대로 싣는다 — 이 분기는 `Intent::NewTab` 으로 위임되고, 그 핸들러가
            // `origin.is_user()` 로 갈리는 부수효과를 갖는다. 에이전트 요청을 사용자 발화로
            // 찍으면 그 분기가 전부 오분류된다.
            let intent = crate::intent::Intent::NewTab {
                kind: Some(surface_kind.to_string()),
                params,
            };
            state.dispatch_intent(match dispatch_origin {
                FileDispatchOrigin::User => intent.from_user_menu("file_dispatch"),
                FileDispatchOrigin::Agent => intent.from_agent_ipc(),
            });
        }
    }
    true
}

/// `Path` → `file://` URI. terminal_link 의 같은 함수가 private 이라 여기 별도 정의.
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
        // 잘못된 escape 는 그대로 통과 (lossy).
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

    /// 수정 전 코드는 모든 대상을 `path_to_file_uri` 에 통과시켜
    /// `file:///https://example.com/page` 를 OS opener 에 넘겼다.
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

    /// URL 대상에서 각 액션 종류가 받는지의 표. Ipc 는 `path` 키 규약이라 받지 않고,
    /// OpenSurface 는 `url` 파라미터를 선언한 surface 만 받는다.
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

    /// recent 는 candidates 와 무관하게 저장 파일에서 읽힌다 — URL 을 못 받는 핸들러가
    /// recent 에 있어도 picker 의 어느 그룹에도 실리지 않아야 한다.
    // picker 행은 GUI 가 소유하는 popup 상태라 headless 테스트 구성에는 대상이 없다.
    #[cfg(feature = "gui")]
    #[test]
    fn picker_lists_drop_handlers_that_cannot_take_a_url_from_recent_and_candidates() {
        let md = handler("host/md", open_surface("markdown", "file"));
        let html = handler("host/html", open_surface("html", "url"));
        let plugin = handler("com.example.x/open", ipc());
        let system = handler("host/system", HandlerAction::System);
        // recent 는 (핸들러, 마지막 사용 시각) 짝이다 — 시각은 행의 "언제" 조각이 된다.
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

        // 같은 목록이 파일 대상이면 아무것도 걸러지지 않는다(recent 중복 제거만).
        let (recent_rows, cand_rows) = picker_lists(&file("/tmp/a.md"), &recent, &candidates);
        assert_eq!(
            ids(&recent_rows),
            vec!["host/md", "com.example.x/open", "host/html"]
        );
        assert_eq!(ids(&cand_rows), vec!["host/system"]);
    }

    /// 원격 경로 picker 는 recent 가 차 있어도 어느 그룹에도 핸들러를 싣지 않는다 — 실으면
    /// 사용자가 recent 를 골라 로컬 핸들러가 원격 호스트 경로로 실행된다. 갓 만든 프로필은
    /// recent 가 비어 있어 이 결함이 안 드러나므로 recent 를 먼저 채운다.
    #[cfg(feature = "gui")]
    #[test]
    fn remote_placeholder_picker_carries_no_recent_even_when_recent_is_populated() {
        let (mut state, mut engine) = crate::state::tests::test_state();
        let any = engine
            .file_handler
            .all_handlers()
            .into_iter()
            .next()
            .expect("host default handlers exist");
        engine.file_handler_recent.record(&any.id);

        // 같은 recent 로 일반 picker 를 열면 recent 열이 찬다(전제 확인).
        open_picker(
            &mut state,
            &mut engine,
            file("/remote/a.md"),
            None,
            Vec::new(),
            false,
            FileDispatchOrigin::Agent,
            false,
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

        open_remote_placeholder_picker(&mut state, FileTarget::new("/remote/a.md"));
        let picker = state.dialogs.file_handler_picker.as_ref().unwrap();
        assert!(picker.recent.is_empty());
        assert!(picker.candidates.is_empty());
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
