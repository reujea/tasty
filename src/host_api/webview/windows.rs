//! Windows WebView2 wrapper.
//! Reference: wry/src/webview2/mod.rs (MIT license, Tauri)
//! 부모 안의 child HWND에 WebView2 controller를 만든다. WebView2 runtime이 필요하다.

// 이유: native 포인터·핸들 호출을 묶는 FFI 모듈이다. 파일 단위 면제는 새 코드에도 적용되므로 안전 래퍼로 옮기면 범위를 줄여야 한다.
#![allow(clippy::multiple_unsafe_ops_per_block)]

#[cfg(debug_assertions)]
mod debug_host_input;
mod host_input;
mod http;
mod key_replay;
mod keyboard;
mod navigation;
mod overlay;
mod remote_block;
mod same_document;
mod view_navigation;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::mpsc;

use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[cfg(test)]
use self::keyboard::vk_to_winit_key;
use self::navigation::{
    complete_requests, replay_navigation, set_js, stop_navigation, webview_source,
};
use super::entry_response::{EntryResponseSource, windows as entry_adapter};
use super::keys::WebViewKeySink;
use super::load_generation::{is_current_load, nav_state_after_end};
use super::scan_resume::{RequestAction, ScanResume, Start, StartKind};
use super::script_gate::ScriptGate;
use super::window_region::{PointerGate, WindowRegion};
use super::{NavState, PendingNavigation, WebViewBounds};
use tasty_model::html_script::top_level::{SubframeNavigations, TopLevelAdmission};

pub(crate) use host_input::filter_message as filter_host_input;

pub struct PlatformWebView {
    hwnd: HWND,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
    _environment: ICoreWebView2Environment,
    entry_source: Option<EntryResponseSource>,
    entry_pending: Rc<RefCell<Vec<entry_adapter::PendingResponse>>>,
    _entry_profile: Option<tempfile::TempDir>,
    /// The remote HTTP document this view shows through its endpoint proxy.
    http: Option<Rc<super::http_native::NativeDocument>>,
    allow_remote: Rc<Cell<bool>>,
    /// 원격 차단 중 연 top-level 탐색(remote_block).
    top_level: Rc<RefCell<TopLevelAdmission>>,
    nav_state: Rc<Cell<NavState>>,
    /// 렌더 process가 끝났고 동기화 루프가 아직 꺼내 가지 않았다.
    content_process_ended: Rc<Cell<bool>>,
    /// 마지막에 시작한 main frame 탐색의 NavigationId. chrome 상태는 이 탐색의 종료만 따른다.
    chrome_generation: Rc<Cell<Option<u64>>>,
    /// 원격 차단 여부와 별도로 기록한 탐색 시도 큐(NavigationStarting과 같은 문서 fragment 이동).
    pending_navigations: Rc<RefCell<Vec<PendingNavigation>>>,
    /// html surface의 문서 단위 스크립트 허용. 없으면 JS는 설정값을 그대로 따른다.
    script_gate: Rc<RefCell<Option<ScriptGate>>>,
    /// 스캔 결과가 JS를 정하는 탐색의 취소·재시작 상태.
    scan_resume: Rc<RefCell<ScanResume<ICoreWebView2Deferral>>>,
    /// 마지막 main frame commit(ContentLoading)의 주소. 로드 중·실패 중 주소 표시가 쓴다.
    committed_url: Rc<RefCell<Option<String>>>,
    /// `nav_info` 조회가 실패한 적이 있다. 프레임마다 부르므로 첫 실패만 warn으로 남긴다.
    nav_info_failed: Cell<bool>,
    parent_hwnd: HWND,
    surface_id: u32,
    /// 마지막 `set_bounds`의 child HWND 크기(물리 px). 창 region의 전체 사각형이다.
    size: Cell<(i32, i32)>,
    /// 안내 카드 구멍과 대상 팝업 영역을 합친 창 region. 같으면 다시 설정하지 않는다.
    region: RefCell<WindowRegion>,
    /// 대상 팝업이 보이는 동안 이 창을 꺼서 WebView 영역의 포인터를 부모(winit)로 보낸 상태.
    pointer: RefCell<PointerGate>,
    /// host 입력 경로(winit 메시지 훅) 등록. 옵션을 쓰지 못한 view는 None이고 accelerator 경로를 쓴다.
    host_input: Option<host_input::RouteGuard>,
    /// host가 가져간 키를 결과까지 붙잡는다. 훅과 accelerator 경로가 같이 쓴다.
    key_replay: Rc<key_replay::KeyReplay>,
}

/// WebView2에는 편집 명령 API가 없어 DevTools `Input.dispatchKeyEvent`의 `commands`로 엔진 편집
/// 명령을 실행한다. 페이지 스크립트 허용과 무관하게 포커스 frame에서 실행되며, 선택 없는 복사나
/// 읽기 전용 대상의 잘라내기·붙여넣기는 아무것도 바꾸지 않는다.
pub(super) fn run_editing_command(
    webview: &ICoreWebView2,
    action: super::WebViewEditAction,
    surface_id: u32,
) {
    use super::WebViewEditAction as A;
    let command = match action {
        A::Copy => "copy",
        A::Cut => "cut",
        A::Paste => "paste",
        A::SelectAll => "selectAll",
        A::Undo => "undo",
        A::Redo => "redo",
    };
    let params = HSTRING::from(format!(
        r#"{{"type":"rawKeyDown","commands":["{command}"]}}"#
    ));
    let done =
        CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _json| {
            if let Err(e) = result {
                tracing::warn!("WebView surface {surface_id}: edit command {command} failed: {e}");
            }
            Ok(())
        }));
    // SAFETY: GUI 스레드에서 살아 있는 WebView에 호출하며 HSTRING은 호출 동안 유지된다.
    let sent =
        unsafe { webview.CallDevToolsProtocolMethod(w!("Input.dispatchKeyEvent"), &params, &done) };
    if let Err(e) = sent {
        tracing::warn!("WebView surface {surface_id}: edit command {command} failed: {e}");
    }
}

/// 창 핸들·종류 오류는 Permanent, 이후 native 생성·등록 오류는 Transient로 분류한다.
/// 실제 일시 오류인지 모두 확인한 분류는 아니며 호출자의 재시도 상한이 필요하다.
fn perm(msg: impl std::fmt::Display) -> super::WebViewCreateError {
    super::WebViewCreateError::Permanent(msg.to_string())
}

fn transient(msg: impl std::fmt::Display) -> super::WebViewCreateError {
    super::WebViewCreateError::Transient(msg.to_string())
}

impl PlatformWebView {
    pub(crate) fn entry_response_source(&self) -> Option<EntryResponseSource> {
        self.entry_source.clone()
    }

    pub fn new(
        window: &impl HasWindowHandle,
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        entry_source: Option<super::entry_response::EntryResponseSource>,
    ) -> std::result::Result<Self, super::WebViewCreateError> {
        Self::create(
            window,
            bounds,
            scale_factor,
            surface_id,
            key_bridge,
            entry_source,
            None,
        )
    }

    pub(crate) fn new_http(
        window: &impl HasWindowHandle,
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        document: super::http_native::NativeDocument,
    ) -> std::result::Result<Self, super::WebViewCreateError> {
        let document = Some(Rc::new(document));
        Self::create(
            window,
            bounds,
            scale_factor,
            surface_id,
            key_bridge,
            None,
            document,
        )
    }

    fn create(
        window: &impl HasWindowHandle,
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        entry_source: Option<EntryResponseSource>,
        http: Option<Rc<super::http_native::NativeDocument>>,
    ) -> std::result::Result<Self, super::WebViewCreateError> {
        // Native script enablement is fixed per navigation, so the endpoint's script policy
        // decides for a remote HTTP document instead.
        let http_js = http.is_some();
        let http_environment = http
            .as_deref()
            .map(http::HttpEnvironment::new)
            .transpose()
            .map_err(perm)?;
        let entry_environment = entry_source
            .as_ref()
            .map(|_| entry_adapter::EntryEnvironment::new())
            .transpose()
            .map_err(perm)?;

        let parent = match window.window_handle().map_err(perm)?.as_raw() {
            RawWindowHandle::Win32(w) => HWND(w.hwnd.get() as *mut std::ffi::c_void),
            _ => return Err(perm("Not a Win32 window")),
        };

        // SAFETY: winit main thread에서 유효한 부모 HWND를 받아 child window와 COM 객체를 만든다.
        // WNDPROC는 Win32 호출 시그니처를 사용한다. 비동기 결과를 기다리는 동안 wait_with_pump가 메시지를 처리한다.
        unsafe {
            // COM 초기화 결과는 로그만 남기고 이후 WebView2 생성 결과로 진행 여부를 판단한다.
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                tracing::trace!("CoInitializeEx returned non-success HRESULT: {hr:?}");
            }

            let class_name = w!("TASTY_WEBVIEW");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(std::mem::transmute::<
                    usize,
                    unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                >(DefWindowProcW as *const () as usize)),
                hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let physical = bounds.to_physical(scale_factor);
            let x = physical.x as i32;
            let y = physical.y as i32;
            let w = physical.width as i32;
            let h = physical.height as i32;

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR::null(),
                WS_CHILD | WS_CLIPCHILDREN | WS_VISIBLE,
                x,
                y,
                w,
                h,
                Some(parent),
                None,
                None,
                None,
            )
            .map_err(|e| transient(format!("CreateWindowExW failed: {e}")))?;

            let (env_tx, env_rx) = mpsc::channel();
            let (folder, options) = match (&entry_environment, &http_environment) {
                (Some(entry), _) => (PCWSTR(entry.folder.as_ptr()), Some(&entry.options)),
                (None, Some(http)) => (PCWSTR(http.folder.as_ptr()), Some(&http.options)),
                (None, None) => (PCWSTR::null(), None),
            };
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                folder,
                options,
                &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                    move |_hr, env| {
                        if let Err(e) = env_tx.send(env) {
                            tracing::warn!("WebView2 env handoff failed: {e}");
                        }
                        Ok(())
                    },
                )),
            )
            .map_err(|e| transient(format!("CreateEnvironment failed: {e}")))?;

            let env = webview2_com::wait_with_pump(env_rx)
                .map_err(|e| transient(format!("Environment wait failed: {e}")))?
                .ok_or_else(|| transient("No environment returned"))?;

            // entry·HTTP view는 비영속 프로필이다. host 입력 경로를 먼저 시도하고 안 되면 기존 경로로 만든다.
            let in_private = entry_source.is_some() || http.is_some();
            let (controller, host_input_on) =
                host_input::create_controller(&env, hwnd, in_private, surface_id)
                    .map_err(transient)?;

            controller
                .SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                })
                .map_err(|e| transient(format!("SetBounds failed: {e}")))?;

            let webview: ICoreWebView2 = controller
                .CoreWebView2()
                .map_err(|e| transient(format!("CoreWebView2 failed: {e}")))?;

            // 브라우저 자체 accelerator가 페이지의 찾기 키 등을 먼저 소비하지 않도록 끈다.
            // 구형 API나 호출 실패는 로그 후 계속하므로 그 경우 기존 accelerator가 남을 수 있다.
            match webview.Settings() {
                Ok(settings) => match settings.cast::<ICoreWebView2Settings3>() {
                    Ok(settings3) => {
                        if let Err(e) = settings3.SetAreBrowserAcceleratorKeysEnabled(false) {
                            tracing::warn!(
                                "WebView2 SetAreBrowserAcceleratorKeysEnabled failed: {e}"
                            );
                        }
                    }
                    Err(e) => tracing::warn!(
                        "WebView2 ICoreWebView2Settings3 cast failed (구형 런타임?): {e}"
                    ),
                },
                Err(e) => tracing::warn!("WebView2 Settings() failed: {e}"),
            }

            let nav_state = Rc::new(Cell::new(NavState::Idle));
            let content_process_ended = Rc::new(Cell::new(false));
            let script_gate: Rc<RefCell<Option<ScriptGate>>> = Rc::new(RefCell::new(None));
            let scan_resume = Rc::new(RefCell::new(ScanResume::new(surface_id)));
            let gate_request = script_gate.clone();
            let nav_request = nav_state.clone();
            let resume_request = scan_resume.clone();
            // 차단 상태에서 http(s) 요청을 빈 403 응답으로 대체한다. 앞 스캔 결과를 받은 탐색의 file
            // 문서 요청은 지문을 확인할 때까지 붙잡는다(scan_resume).
            let allow_remote = Rc::new(Cell::new(false));
            let top_level: Rc<RefCell<TopLevelAdmission>> = Rc::default();
            let top_request = top_level.clone();
            let frames: Rc<RefCell<SubframeNavigations>> = Rc::default();
            let frames_request = frames.clone();
            // A remote HTTP document's requests all reach its endpoint, which decides them.
            if http.is_none() {
                webview
                    .AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
                    .map_err(|e| transient(format!("AddWebResourceRequestedFilter failed: {e}")))?;
            }
            let env_cb = env.clone();
            let entry_pending = Rc::new(RefCell::new(Vec::new()));
            let pending_request = Rc::clone(&entry_pending);
            let entry_request = entry_source.clone();
            let allow_cb = allow_remote.clone();
            let mut token: i64 = 0;
            let handler = WebResourceRequestedEventHandler::create(Box::new(
                move |sender, args| -> windows::core::Result<()> {
                    let Some(args) = args else { return Ok(()) };
                    let request = args.Request()?;
                    let mut uri = windows::core::PWSTR::null();
                    request.Uri(&mut uri)?;
                    let uri_str = uri.to_string().unwrap_or_default();
                    // WebView2가 할당한 URI를 CoTaskMemFree로 해제한다.
                    CoTaskMemFree(Some(uri.0 as *const c_void));
                    if let Some(source) = &entry_request {
                        return entry_adapter::reply(
                            source,
                            &env_cb,
                            &args,
                            &uri_str,
                            &pending_request,
                        );
                    }
                    let is_remote =
                        uri_str.starts_with("http://") || uri_str.starts_with("https://");
                    if is_remote
                        && remote_block::refuse(
                            &allow_cb,
                            &gate_request,
                            &top_request,
                            &frames_request,
                            &uri_str,
                            &args,
                        )
                    {
                        let resp =
                            env_cb.CreateWebResourceResponse(None, 403, w!("Blocked"), w!(""))?;
                        args.SetResponse(&resp)?;
                        return Ok(());
                    }
                    if !uri_str.starts_with("file:") {
                        return Ok(());
                    }
                    let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                    args.ResourceContext(&mut context)?;
                    if context != COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT {
                        return Ok(());
                    }
                    let gate = gate_request.borrow();
                    let Some(gate) = gate.as_ref() else {
                        return Ok(());
                    };
                    let action =
                        resume_request
                            .borrow_mut()
                            .document_request(gate, &uri_str, || match args.GetDeferral() {
                                Ok(deferral) => Some(deferral),
                                Err(e) => {
                                    tracing::warn!(
                                        "WebView surface {surface_id}: GetDeferral failed ({e}); \
                                         the document request is not held"
                                    );
                                    None
                                }
                            });
                    if action == RequestAction::Stop
                        && let Some(wv) = &sender
                    {
                        // GetDeferral이 실패해 본문을 붙잡지 못했다.
                        stop_navigation(
                            surface_id,
                            wv,
                            false,
                            &resume_request,
                            Some(gate),
                            &nav_request,
                        );
                    }
                    Ok(())
                },
            ));
            webview
                .add_WebResourceRequested(&handler, &mut token)
                .map_err(|e| transient(format!("add_WebResourceRequested failed: {e}")))?;

            let pending_navigations: Rc<RefCell<Vec<PendingNavigation>>> =
                Rc::new(RefCell::new(Vec::new()));
            let nav_start = nav_state.clone();
            let pending_nav = pending_navigations.clone();
            let gate_start = script_gate.clone();
            let resume_start = scan_resume.clone();
            // 게이트가 시작한 마지막 로드의 NavigationId. 새 로드가 앞 로드를 취소하면 앞 로드의
            // NavigationCompleted가 새 로드의 NavigationStarting 뒤에 올 수 있으므로, 종료 신호는
            // 이 세대의 것일 때만 게이트를 마무리한다(docs/surfaces/html/index.md).
            let gate_generation: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
            let generation_start = gate_generation.clone();
            // 마지막에 시작한 main frame 탐색의 NavigationId. 게이트 세대와 달리 fragment 이동과
            // 게이트가 없는 surface의 탐색도 기록해, chrome 상태가 그 탐색의 종료만 따르게 한다.
            let chrome_generation: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
            let chrome_start = chrome_generation.clone();
            let mut tok_start: i64 = 0;
            // NavigationStarting은 main frame 탐색에서만 온다. WebView2는 응답 단계가 없어
            // 로드 시작과 응답 결정을 여기서 함께 한다(docs/surfaces/html/index.md 후보 이벤트, 미측정).
            let entry_navigation = entry_source.clone();
            let http_start = http.as_ref().map(Rc::downgrade);
            let top_start = top_level.clone();
            let allow_start = allow_remote.clone();
            // host 입력 경로는 아래에서 등록하므로 입력 증명은 등록 뒤에 채운다.
            let click_slot: remote_block::ClickSlot = Rc::default();
            let click_start = click_slot.clone();
            let h_start = NavigationStartingEventHandler::create(Box::new(
                move |sender, args| -> windows::core::Result<()> {
                    tracing::debug!("WebView surface {surface_id}: load started");
                    nav_start.set(NavState::Loading);
                    if let Some(args) = args {
                        let mut id = 0u64;
                        let nav_id = match args.NavigationId(&mut id) {
                            Ok(()) => Some(id),
                            Err(e) => {
                                tracing::warn!(
                                    "WebView surface {surface_id}: NavigationId failed ({e}); \
                                     any navigation end counts as this load"
                                );
                                None
                            }
                        };
                        chrome_start.set(nav_id);
                        let mut uri = windows::core::PWSTR::null();
                        args.Uri(&mut uri)?;
                        let uri_str = uri.to_string().unwrap_or_default();
                        // 반환 URI의 할당은 호출자가 해제한다.
                        CoTaskMemFree(Some(uri.0 as *const c_void));
                        if entry_navigation
                            .as_ref()
                            .is_some_and(|source| source.main_resource(&uri_str))
                        {
                            args.SetCancel(true)?;
                            nav_start.set(NavState::Failed);
                            return Ok(());
                        }
                        // 제스처 여부 조회 실패도 탐색 자체는 기록하되 false로 남긴다.
                        let mut user_initiated = BOOL(0);
                        let user_gesture = match args.IsUserInitiated(&mut user_initiated) {
                            Ok(()) => user_initiated.as_bool(),
                            Err(e) => {
                                tracing::warn!(
                                    "WebView surface {surface_id}: IsUserInitiated failed ({e}); \
                                     the navigation is reported as not user-initiated"
                                );
                                false
                            }
                        };
                        // 같은 문서의 fragment 이동은 건너뛴다. 종류를 알 수 없거나 재로드면 게이트를 거친다.
                        let kind = args
                            .cast::<ICoreWebView2NavigationStartingEventArgs3>()
                            .and_then(|a| {
                                let mut k = COREWEBVIEW2_NAVIGATION_KIND::default();
                                a.NavigationKind(&mut k).map(|()| k)
                            })
                            .ok();
                        remote_block::navigation_starting(
                            &top_start,
                            &args,
                            &uri_str,
                            kind,
                            remote_block::Gesture {
                                initiated: user_gesture,
                                click: &click_start,
                            },
                            remote_block::origin(allow_start.get(), gate_start.borrow().as_ref()),
                            surface_id,
                        );
                        let skip = kind.is_some_and(|k| k != COREWEBVIEW2_NAVIGATION_KIND_RELOAD)
                            && gate_start
                                .borrow()
                                .as_ref()
                                .is_some_and(|g| g.is_fragment_move(&uri_str));
                        let mut record = true;
                        if let Some(document) = &http_start {
                            if !skip {
                                generation_start.set(nav_id);
                                if let Some(gate) = gate_start.borrow().as_ref() {
                                    gate.load_started();
                                }
                                if let Some(document) = document.upgrade() {
                                    document.session().expect_main_response(&uri_str);
                                }
                            }
                        } else if let (Some(gate), Some(wv)) =
                            (gate_start.borrow().as_ref(), &sender)
                            && !skip
                        {
                            generation_start.set(nav_id);
                            let start_kind = match kind {
                                Some(k) if k == COREWEBVIEW2_NAVIGATION_KIND_RELOAD => {
                                    StartKind::Reload
                                }
                                Some(k) if k == COREWEBVIEW2_NAVIGATION_KIND_BACK_OR_FORWARD => {
                                    StartKind::BackOrForward
                                }
                                Some(k) if k == COREWEBVIEW2_NAVIGATION_KIND_NEW_DOCUMENT => {
                                    StartKind::NewDocument
                                }
                                _ => StartKind::Unknown,
                            };
                            let start = resume_start.borrow_mut().start(
                                gate,
                                &uri_str,
                                nav_id,
                                start_kind,
                                user_gesture,
                            );
                            match start {
                                Start::Js(js) => set_js(wv, js),
                                Start::Resumed(js) => {
                                    set_js(wv, js);
                                    record = false;
                                }
                                // deferral이 없어 탐색을 취소하고, 스캔이 끝나면 UI 스레드가 같은 종류로
                                // 다시 시작한다(resume_script_decision). chrome은 Loading으로 남는다.
                                Start::Cancel => {
                                    if let Err(e) = args.SetCancel(true) {
                                        tracing::warn!(
                                            "WebView surface {surface_id}: SetCancel failed ({e}); \
                                             the document loads without scripts"
                                        );
                                        resume_start.borrow_mut().cancel_failed(nav_id);
                                        set_js(wv, false);
                                    }
                                }
                            }
                        }
                        if record {
                            pending_nav.borrow_mut().push(PendingNavigation {
                                url: uri_str,
                                user_gesture,
                            });
                        }
                    }
                    Ok(())
                },
            ));
            webview
                .add_NavigationStarting(&h_start, &mut tok_start)
                .map_err(|e| transient(format!("add_NavigationStarting failed: {e}")))?;

            // ContentLoading을 main frame commit으로 본다(docs/surfaces/html/index.md 후보 이벤트, 미측정).
            let gate_commit = script_gate.clone();
            if entry_source.is_some() {
                entry_adapter::deny_new_windows(&webview).map_err(|error| {
                    transient(format!("entry new-window boundary failed: {error}"))
                })?;
            }
            if let Some(document) = &http {
                http::install(&webview, surface_id, Rc::downgrade(document))
                    .map_err(|error| transient(format!("HTTP view setup failed: {error}")))?;
            }

            let committed_url: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
            let commit_url = committed_url.clone();
            let resume_content = scan_resume.clone();
            let nav_content = nav_state.clone();
            let http_commit = http.as_ref().map(Rc::downgrade);
            let top_commit = top_level.clone();
            let frames_commit = frames.clone();
            let mut tok_content: i64 = 0;
            let h_content = ContentLoadingEventHandler::create(Box::new(
                move |sender, args| -> windows::core::Result<()> {
                    let Some(wv) = &sender else {
                        return Ok(());
                    };
                    let nav_id = args.as_ref().and_then(|a| {
                        let mut id = 0u64;
                        a.NavigationId(&mut id).ok().map(|()| id)
                    });
                    // 앞 스캔 결과를 받은 탐색이 문서 요청 확인 없이 commit했으면 멈추고 다시 읽는다.
                    let (stop, release) = resume_content.borrow_mut().content_loading(nav_id);
                    complete_requests(surface_id, release);
                    if stop {
                        // 문서 요청 확인 없이 이미 commit했다.
                        stop_navigation(
                            surface_id,
                            wv,
                            false,
                            &resume_content,
                            gate_commit.borrow().as_ref(),
                            &nav_content,
                        );
                    }
                    top_commit.borrow_mut().end();
                    // 앞 문서의 frame은 사라진다.
                    frames_commit.borrow_mut().clear();
                    let uri_str = webview_source(wv)?;
                    *commit_url.borrow_mut() = uri_str.clone();
                    if let Some(gate) = gate_commit.borrow().as_ref()
                        && !http_commit
                            .as_ref()
                            .and_then(std::rc::Weak::upgrade)
                            .is_some_and(|document| document.session().awaiting_main_response())
                    {
                        set_js(wv, gate.committed(uri_str.as_deref()) || http_js);
                    }
                    Ok(())
                },
            ));
            webview
                .add_ContentLoading(&h_content, &mut tok_content)
                .map_err(|e| transient(format!("add_ContentLoading failed: {e}")))?;

            let nav_done = nav_state.clone();
            let gate_done = script_gate.clone();
            let generation_done = gate_generation.clone();
            let chrome_done = chrome_generation.clone();
            let resume_done = scan_resume.clone();
            let http_done = http.as_ref().map(Rc::downgrade);
            let top_done = top_level.clone();
            let mut tok_done: i64 = 0;
            let h_done = NavigationCompletedEventHandler::create(Box::new(
                move |sender, args| -> windows::core::Result<()> {
                    // The proxy challenge ends a first attempt that WebView2 repeats with credentials.
                    if http_js
                        && let Some(args) = &args
                        && http::proxy_challenge(args)
                    {
                        return Ok(());
                    }
                    let nav_id = args.as_ref().and_then(|a| {
                        let mut id = 0u64;
                        match a.NavigationId(&mut id) {
                            Ok(()) => Some(id),
                            Err(e) => {
                                tracing::warn!(
                                    "WebView surface {surface_id}: NavigationId failed on \
                                     completion ({e}); the gate treats it as the current load"
                                );
                                None
                            }
                        }
                    });
                    // 스캔을 기다리려고 취소한 탐색의 완료(OPERATION_CANCELED)와, 지문이 달라 멈춘
                    // 탐색의 완료는 로드 종료가 아니다. 게이트 대기와 chrome의 Loading을 그대로 둔다.
                    // 멈춘 탐색이 끝났으므로 붙잡은 요청을 놓아 주고 같은 종류로 다시 시작한다.
                    let ended = resume_done.borrow_mut().completed(nav_id);
                    complete_requests(surface_id, ended.release);
                    if ended.internal {
                        tracing::debug!(
                            "WebView surface {surface_id}: the navigation cancelled for its scan ended"
                        );
                        if let (Some((replay, url)), Some(wv)) = (ended.restart, &sender)
                            && let Err(e) = replay_navigation(wv, replay, &url)
                        {
                            tracing::warn!(
                                "WebView surface {surface_id}: restarting the navigation after its \
                                 check failed ({replay:?}): {e}"
                            );
                            resume_done.borrow_mut().replay_failed();
                            if let Some(gate) = gate_done.borrow().as_ref() {
                                gate.failed();
                                if let Some(js) = gate.finished() {
                                    set_js(wv, js);
                                }
                            }
                            nav_done.set(NavState::Failed);
                        }
                        return Ok(());
                    }
                    let current = is_current_load(generation_done.get(), nav_id);
                    // 앞 로드의 늦은 종료는 새 로드의 top-level 허용을 닫지 않는다.
                    if is_current_load(chrome_done.get(), nav_id) {
                        top_done.borrow_mut().end();
                    }
                    if current {
                        if let (Some(js), Some(wv)) = (
                            gate_done.borrow().as_ref().and_then(|g| g.finished()),
                            &sender,
                        ) {
                            set_js(wv, js || http_js);
                        }
                    } else {
                        tracing::debug!(
                            "WebView surface {surface_id}: ignoring the end of a superseded load"
                        );
                    }
                    let Some(args) = args else { return Ok(()) };
                    let mut is_success = BOOL(0);
                    args.IsSuccess(&mut is_success)?;
                    if is_success.as_bool() {
                        tracing::debug!("WebView surface {surface_id}: load finished");
                    } else {
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        if let Err(e) = args.WebErrorStatus(&mut status) {
                            tracing::warn!("WebView2 WebErrorStatus query failed: {e}");
                        }
                        tracing::warn!(
                            "WebView surface {surface_id}: WebView2 navigation failed: status={status:?}"
                        );
                        // 앞 로드의 늦은 실패는 새 로드의 게이트를 건드리지 않는다.
                        if current && let Some(gate) = gate_done.borrow().as_ref() {
                            gate.failed();
                        }
                        if current
                            && let Some(document) = http_done.as_ref().and_then(|d| d.upgrade())
                        {
                            http::load_failed(&document.session());
                        }
                    }
                    // 앞 로드의 늦은 종료는 새 로드의 Loading을 Done이나 Failed로 덮지 않는다.
                    match nav_state_after_end(chrome_done.get(), nav_id, is_success.as_bool()) {
                        Some(state) => nav_done.set(state),
                        None => tracing::debug!(
                            "WebView surface {surface_id}: keeping the chrome state of the newer load"
                        ),
                    }
                    Ok(())
                },
            ));
            webview
                .add_NavigationCompleted(&h_done, &mut tok_done)
                .map_err(|e| transient(format!("add_NavigationCompleted failed: {e}")))?;

            // main frame 문서를 그리던 process가 끝나면 Linux처럼 게이트 로드를 끝낸다(docs/surfaces/html/index.md, 실 기기 미측정).
            // NavigationCompleted가 뒤따라 와도 finished()는 로드 중이 아닐 때 아무것도 하지 않는다.
            // iframe·GPU·utility process 종료는 main frame 로드를 끝내지 않으므로 건너뛴다.
            let nav_failed = nav_state.clone();
            let ended_failed = content_process_ended.clone();
            let gate_failed = script_gate.clone();
            let resume_failed = scan_resume.clone();
            let mut tok_failed: i64 = 0;
            let h_failed = ProcessFailedEventHandler::create(Box::new(
                move |sender, args| -> windows::core::Result<()> {
                    let Some(args) = args else { return Ok(()) };
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    tracing::warn!(
                        "WebView surface {surface_id}: WebView2 process failed: kind={kind:?}"
                    );
                    if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                        && kind != COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                    {
                        return Ok(());
                    }
                    nav_failed.set(NavState::Failed);
                    // 렌더 process만 끝났으면 같은 WebView에 다시 탐색해 복구할 수 있다.
                    // browser process가 끝난 WebView는 다시 만들어야 하므로 표시하지 않는다.
                    if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED {
                        ended_failed.set(true);
                    }
                    resume_failed.borrow_mut().clear();
                    if let Some(gate) = gate_failed.borrow().as_ref() {
                        gate.failed();
                    }
                    if let (Some(js), Some(wv)) = (
                        gate_failed.borrow().as_ref().and_then(|g| g.finished()),
                        &sender,
                    ) {
                        set_js(wv, js || http_js);
                    }
                    Ok(())
                },
            ));
            webview
                .add_ProcessFailed(&h_failed, &mut tok_failed)
                .map_err(|e| transient(format!("add_ProcessFailed failed: {e}")))?;

            let key_replay = Rc::new(key_replay::KeyReplay::new(surface_id, webview.clone()));
            let h_key = key_replay::accelerator_key_handler(
                key_bridge.clone(),
                webview.clone(),
                key_replay.clone(),
                surface_id,
            );
            let mut tok_key: i64 = 0;
            controller
                .add_AcceleratorKeyPressed(&h_key, &mut tok_key)
                .map_err(|e| transient(format!("add_AcceleratorKeyPressed failed: {e}")))?;

            let key_bridge_focus = key_bridge.clone();
            let replay_focus = key_replay.clone();
            let mut tok_focus: i64 = 0;
            let h_focus = FocusChangedEventHandler::create(Box::new(
                move |_sender, _args| -> windows::core::Result<()> {
                    key_bridge_focus.note_focus(surface_id);
                    // 다른 곳에서 누른 키의 반복이 이 view의 옛 누름 결과를 따르지 않게 한다.
                    replay_focus.clear_decisions();
                    Ok(())
                },
            ));
            controller
                .add_GotFocus(&h_focus, &mut tok_focus)
                .map_err(|e| transient(format!("add_GotFocus failed: {e}")))?;
            let replay_blur = key_replay.clone();
            let mut tok_blur: i64 = 0;
            let h_blur = FocusChangedEventHandler::create(Box::new(
                move |_sender, _args| -> windows::core::Result<()> {
                    // 포커스를 잃기 전의 누름 결과가 늦게 와도 이 view에 다시 넘기지 않는다.
                    replay_blur.clear_decisions();
                    Ok(())
                },
            ));
            controller
                .add_LostFocus(&h_blur, &mut tok_blur)
                .map_err(|e| transient(format!("add_LostFocus failed: {e}")))?;

            let host_input = host_input_on.then(|| {
                host_input::register(
                    hwnd,
                    surface_id,
                    key_bridge.clone(),
                    webview.clone(),
                    key_replay.clone(),
                )
            });
            let click = host_input.as_ref().map(host_input::RouteGuard::click);
            click_slot.borrow_mut().clone_from(&click);
            same_document::install(&webview, surface_id, parent, &pending_navigations, click)
                .map_err(|e| transient(format!("same-document navigation setup failed: {e}")))?;
            remote_block::install(&webview, &top_level, &frames, surface_id)
                .map_err(|e| transient(format!("top-level navigation setup failed: {e}")))?;

            Ok(Self {
                hwnd,
                controller,
                webview,
                _environment: env,
                entry_source,
                entry_pending,
                _entry_profile: entry_environment
                    .map(|entry| entry.profile)
                    .or(http_environment.map(|http| http.profile)),
                http,
                allow_remote,
                top_level,
                nav_state,
                content_process_ended,
                chrome_generation,
                pending_navigations,
                script_gate,
                scan_resume,
                surface_id,
                committed_url,
                nav_info_failed: Cell::new(false),
                parent_hwnd: parent,
                size: Cell::new((w, h)),
                region: RefCell::new(WindowRegion::default()),
                pointer: RefCell::new(PointerGate::default()),
                host_input,
                key_replay,
            })
        }
    }

    /// host가 이 WebView에서 가져간 일련번호 `id`의 키를 처리했다. 처리하지 못했으면(`replay`) 붙잡은 원래
    /// 키를 다시 넘겨 엔진·페이지로 보낸다(`key_replay`). 짝이 되는 키가 없으면 아무것도 하지 않는다.
    pub fn finish_forwarded_key(&self, id: u64, replay: bool) {
        self.key_replay.finish(id, replay);
    }

    /// 이 WebView 또는 자손이 키보드 포커스를 가졌을 때만 부모로 돌린다.
    pub fn release_keyboard_focus(&self) {
        if !self.focus_is_inside() {
            return;
        }
        // SAFETY: parent_hwnd 는 이 webview 를 만든 winit 창이고 self 가 살아있는
        // 동안 valid. 호출은 main thread(winit event loop).
        unsafe {
            // 반환은 이전 포커스 핸들이다. 이전 포커스가 없을 때도 오류로 오므로 실패로 단정하지 않는다.
            if let Err(e) =
                windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.parent_hwnd))
            {
                tracing::debug!("WebView focus release: SetFocus returned {e}");
            }
        }
    }

    /// 이 WebView 또는 자손이 키보드 포커스를 가졌는지. GetFocus는 이 스레드가 비활성이면 널을
    /// 돌려주므로 참이면 이 앱이 OS 포커스를 가진 것이다. 자식이 포커스를 가지면 winit이 부모 창에
    /// Focused(false)를 보내므로 호출부가 창 활성 판정에 함께 쓴다.
    pub fn holds_keyboard_focus(&self) -> bool {
        self.focus_is_inside()
    }

    /// 지금 키를 치면 이 WebView가 받는지. Windows의 키는 포커스 창으로만 가므로
    /// [`Self::holds_keyboard_focus`]와 같다.
    pub fn receives_keyboard_input(&self) -> bool {
        self.focus_is_inside()
    }

    fn focus_is_inside(&self) -> bool {
        // SAFETY: 호출은 main thread(winit event loop). GetFocus 는 인자가 없고, 이
        // 스레드 메시지 큐가 활성이 아니면 널 HWND 를 돌려준다 — 다른 앱이 포커스를
        // 쥔 상황이 여기서 걸러진다.
        let focus = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() };
        if focus.is_invalid() {
            return false;
        }
        if focus == self.hwnd {
            return true;
        }
        // SAFETY: self.hwnd 는 self 수명 동안 valid 하고 focus 는 바로 위에서 널이 아님을
        // 확인했다.
        unsafe { windows::Win32::UI::WindowsAndMessaging::IsChild(self.hwnd, focus).as_bool() }
    }

    pub fn set_bounds(&self, bounds: WebViewBounds, scale_factor: f64) {
        let physical = bounds.to_physical(scale_factor);
        let x = physical.x as i32;
        let y = physical.y as i32;
        let w = physical.width as i32;
        let h = physical.height as i32;
        self.size.set((w, h));

        // SAFETY: SetBounds/SetWindowPos는 self가 살아있는 동안 hwnd/controller가 valid
        // 함을 Drop 시점에 정리. 호출은 main thread에서 일어남.
        unsafe {
            if let Err(e) = self.controller.SetBounds(RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            }) {
                tracing::warn!("WebView2 SetBounds failed: {e}");
            }
            if let Err(e) = SetWindowPos(
                self.hwnd,
                None,
                x,
                y,
                w,
                h,
                SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOZORDER,
            ) {
                tracing::warn!("SetWindowPos failed: {e}");
            }
        }
    }

    /// Linux 전용 처리다. Windows의 region 변경은 OS 메시지 루프가 다시 그린다.
    pub fn sync_halted_shape(&self) {}

    /// Linux 전용 처리다. 이 백엔드는 halt 중 별도 이벤트 처리를 돌리지 않는다.
    pub fn hold_input_for_halt(&self) {}

    /// 위에 egui가 그리는 안내 카드(`clip`, 이 HWND 기준 물리 px `[x, y, 너비, 높이]`)를 창 region에서
    /// 뺀다. region 밖은 이 창과 자식(WebView2)이 그리지도 포인터를 받지도 않아 부모의 그림이 보인다.
    /// HWND 크기·표시·포커스와 controller는 바꾸지 않는다. 비면 region을 지워 원래 사각형으로 돌아간다.
    /// host 입력 영역(`_input`)을 빼는 방법은 아직 없어 `webview_edge_inset` 여백으로 피한다.
    /// 대상 팝업 영역(`apply_overlay`)이 있으면 그 구멍도 함께 유지한다.
    pub fn set_native_holes(&self, _input: &[[i32; 4]], clip: &[[i32; 4]], _scale_factor: f64) {
        self.region.borrow_mut().set_clip(clip);
        if !self.refresh_region() {
            tracing::warn!("WebView2 window region update failed; the page may cover the notice");
        }
    }

    pub fn set_visible(&self, visible: bool) {
        // SAFETY: self가 살아있으면 hwnd/controller 모두 valid (Drop이 정리).
        unsafe {
            // 반환값은 이전 표시 상태이며 현재 표시 요청의 성공 여부로 해석하지 않는다.
            let _was_visible = ShowWindow(self.hwnd, if visible { SW_SHOW } else { SW_HIDE });
            if let Err(e) = self.controller.SetIsVisible(visible) {
                tracing::warn!("WebView2 SetIsVisible failed: {e}");
            }
        }
    }

    pub fn set_zoom(&self, factor: f64) {
        // SAFETY: controller는 self가 살아있는 동안 valid, main thread 호출.
        unsafe {
            if let Err(e) = self.controller.SetZoomFactor(factor) {
                tracing::warn!("WebView2 SetZoomFactor failed: {e}");
            }
        }
    }

    /// 게이트가 붙은 webview에서는 전역 sandbox 설정으로 받고, 문서 단위 판단을 적용한다.
    pub fn set_javascript_enabled(&self, enabled: bool) {
        let js = match self.script_gate.borrow().as_ref() {
            Some(gate) => gate.set_sandbox(!enabled),
            None => enabled,
        };
        set_js(&self.webview, js || self.http.is_some());
    }

    /// 문서 단위 스크립트 허용을 붙인다. 첫 로드 전에 호출한다.
    pub fn attach_script_gate(&self, gate: ScriptGate) {
        set_js(&self.webview, gate.effective_js() || self.http.is_some());
        if let Some(document) = &self.http {
            document.session().attach_script_gate(gate.clone());
        }
        *self.script_gate.borrow_mut() = Some(gate);
    }

    /// 팔레트·host 키 경로의 편집 명령. 엔진이 포커스 frame과 편집 가능 여부를 판단한다.
    pub fn execute_edit(&self, action: super::WebViewEditAction) {
        run_editing_command(&self.webview, action, self.surface_id);
    }

    /// 현재 Windows에서는 적용하지 않고 로그만 남긴다.
    pub fn set_color_scheme(&self, scheme: super::ColorScheme) {
        tracing::debug!("set_color_scheme({scheme:?}) is not implemented for Windows WebView2");
    }

    /// 다음 요청부터 WebResourceRequested가 읽을 허용 여부를 갱신한다.
    /// 문서 단위 허용은 요청마다 게이트에서 읽는다(remote_block).
    pub fn set_remote_content_allowed(&self, allowed: bool) {
        if let Some(gate) = self.script_gate.borrow().as_ref() {
            gate.set_remote_global(allowed);
        }
        self.allow_remote
            .set(self.entry_source.is_none() && allowed);
        tracing::debug!("Windows WebView2 set_remote_content_allowed({allowed})");
    }
}

impl Drop for PlatformWebView {
    fn drop(&mut self) {
        if let Some(source) = &self.entry_source {
            source.retire();
            entry_adapter::poll(source, &self._environment, &self.entry_pending);
        }
        // Revoke endpoint, body and view authority before WebView2 tears down the view.
        drop(self.http.take());
        // 창을 없애기 전에 메시지 훅 등록을 지운다.
        drop(self.host_input.take());
        // SAFETY: controller.Close()는 webview2 자원 해제, DestroyWindow는 child HWND 정리.
        // 둘 다 self가 처음 만들어진 main thread에서 Drop이 호출된다는 전제 (PlatformWebView는
        // !Send/!Sync 기본 — COM 객체 포함).
        unsafe {
            // 이미 닫힌 핸들 등 정리 오류도 trace에 남긴다.
            if let Err(e) = self.controller.Close() {
                tracing::trace!("WebView2 controller Close failed: {e}");
            }
            if let Err(e) = DestroyWindow(self.hwnd) {
                tracing::trace!("DestroyWindow failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, NamedKey};

    #[test]
    fn vk_maps_to_character_and_named() {
        assert_eq!(vk_to_winit_key(0x44), Some(Key::Character("d".into()))); // VK_D
        assert_eq!(vk_to_winit_key(0x32), Some(Key::Character("2".into()))); // VK_2
        assert_eq!(vk_to_winit_key(0xBB), Some(Key::Character("=".into()))); // VK_OEM_PLUS
        assert_eq!(vk_to_winit_key(0x1B), Some(Key::Named(NamedKey::Escape)));
        assert_eq!(vk_to_winit_key(0x11), None); // VK_CONTROL
        assert_eq!(vk_to_winit_key(0x7C), Some(Key::Named(NamedKey::F13))); // VK_F13
        assert_eq!(vk_to_winit_key(0x87), Some(Key::Named(NamedKey::F24))); // VK_F24
    }
}
