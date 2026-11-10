//! Linux WebKitGTK wrapper (X11 only).
//! Reference: wry/src/webkitgtk/mod.rs (MIT license, Tauri)

// 이유: native 포인터·핸들 호출을 묶는 FFI 모듈이다. 파일 단위 면제는 새 코드에도 적용되므로 안전 래퍼로 옮기면 범위를 줄여야 한다.
#![allow(clippy::multiple_unsafe_ops_per_block)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib::Cast;
use gtk::prelude::*;
use webkit2gtk::{
    LoadEvent, NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt,
    PolicyDecisionType, ResponsePolicyDecision, ResponsePolicyDecisionExt, SettingsExt,
    URIRequestExt, WebView, WebViewExt,
};
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

use super::keys::WebViewKeySink;
use super::linux_shape::{self, ShapeState};
use super::load_generation::SignalOrderLoads;
use super::script_gate::{DeferredScan, Response, ScriptGate};
use super::{NavState, PendingNavigation, WebViewBounds};

mod availability;
pub(crate) use availability::http_available;
mod bed_focus;
mod content_filter;
pub(super) mod direct_paths;
mod edit;
mod navigation_policy;
mod overlay_backend;
mod remote_block;
pub(super) mod selection;
mod view_navigation;
use super::entry_response::EntryResponseSource;
use content_filter::compile_remote_block_filter;
use remote_block::RemoteBlock;

pub struct PlatformWebView {
    entry_source: Option<EntryResponseSource>,
    http: Option<super::http_native::NativeDocument>,
    webview: WebView,
    gtk_window: gtk::Window,
    x11_window: std::os::raw::c_ulong,
    xlib: x11_dl::xlib::Xlib,
    x11_display: *mut std::os::raw::c_void,
    /// Xlib 핸들은 생성 스레드에서만 사용한다. 메서드 진입에서 이 ID를 확인한다.
    origin_thread: std::thread::ThreadId,
    /// 원격 차단의 실효값(전역 || 문서 허용)과 content filter.
    remote: RemoteBlock,
    nav_state: Rc<Cell<NavState>>,
    /// web process가 끝났고 동기화 루프가 아직 꺼내 가지 않았다.
    content_process_ended: Rc<Cell<bool>>,
    /// chrome 세대. 앞 로드의 늦은 종료가 새 로드의 Loading을 바꾸지 않게 한다.
    loads: Rc<RefCell<SignalOrderLoads>>,
    pending_navigations: Rc<RefCell<Vec<PendingNavigation>>>,
    /// 마지막 main frame commit의 주소. 로드 중·실패 중 주소 표시가 쓴다.
    committed_url: Rc<RefCell<Option<String>>>,
    /// html surface의 문서 단위 스크립트 허용. 없으면 JS는 설정값을 그대로 따른다.
    script_gate: Rc<RefCell<Option<ScriptGate>>>,
    parent_x11_window: std::os::raw::c_ulong,
    /// X11 foreign 창의 GDK 참조. Drop 중 GDK 연결의 오류 트랩에도 사용한다.
    gdk_window: gtk::gdk::Window,
    /// 마지막 `set_bounds` 의 X 창 크기(물리 px). input shape 의 전체 사각형이다.
    x11_size: Cell<(i32, i32)>,
    /// X 창 shape 를 물리 px 로 설정하는 XFixes. 열지 못하면 구멍을 두지 않는다.
    xfixes: Option<x11_dl::xfixes::Xlib>,
    /// native 구멍·오버레이 구멍과 마지막으로 적용한 shape.
    shape: RefCell<ShapeState>,
    /// XFixes 없이 그림 구멍을 요청받아 경고를 남겼는지. 프레임마다 부르므로 한 번만 남긴다.
    clip_unavailable_reported: Cell<bool>,
    /// 저장이 멈춘 뒤 켠다. 켜지면 이 페이지에 오는 키·포인터 이벤트를 GTK 신호 단계에서 버린다.
    input_held: Rc<Cell<bool>>,
    /// 편집 명령의 비동기 선택 질의가 낡았는지 판정한다.
    edit_guard: edit::EditGuard,
    /// 바탕 포커스 상태를 두는 사용자 스타일시트.
    bed_focus: bed_focus::BedFocusSheet,
}

/// 게이트 상태를 한 단계 옮기고 그 결과의 JS 값과 원격 차단 값을 적용한다.
fn gate_step(
    wv: &WebView,
    gate: &RefCell<Option<ScriptGate>>,
    remote: &RemoteBlock,
    step: impl FnOnce(&ScriptGate) -> Option<bool>,
) {
    if let Some(gate) = gate.borrow().as_ref() {
        if let Some(js) = step(gate) {
            set_js(wv, js);
        }
        remote.sync(gate);
    }
}

fn set_js(wv: &WebView, enabled: bool) {
    if let Some(settings) = WebViewExt::settings(wv) {
        settings.set_enable_javascript(enabled);
    }
}

/// 창·디스플레이 종류와 라이브러리 초기화는 Permanent, X 창 생성·조회 실패는 Transient로 분류한다.
fn perm(msg: impl std::fmt::Display) -> super::WebViewCreateError {
    super::WebViewCreateError::Permanent(msg.to_string())
}

fn transient(msg: impl std::fmt::Display) -> super::WebViewCreateError {
    super::WebViewCreateError::Transient(msg.to_string())
}

/// XFixes 를 열고 region 요청에 필요한 버전을 맞춘다. 실패하면 경고하고 None 이다.
fn open_xfixes(display: *mut x11_dl::xlib::Display) -> Option<x11_dl::xfixes::Xlib> {
    let fixes = match x11_dl::xfixes::Xlib::open() {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("WebView input shape disabled: cannot open libXfixes: {e}");
            return None;
        }
    };
    let (mut event_base, mut error_base) = (0, 0);
    let (mut major, minor) = (5, 0);
    // SAFETY: display 는 호출자가 확인한 유효한 연결이고 출력 인자는 지역 변수다.
    // region 요청(버전 2 이상)을 쓰기 전에 확장과 버전을 협상한다.
    let ready = unsafe {
        (fixes.XFixesQueryExtension)(display, &mut event_base, &mut error_base) != 0
            && (fixes.XFixesQueryVersion)(display, &mut major, &minor) != 0
    };
    if !ready || major < 2 {
        tracing::warn!("WebView input shape disabled: XFixes 2.0 is not available (got {major})");
        return None;
    }
    Some(fixes)
}

impl PlatformWebView {
    pub(crate) fn entry_response_source(&self) -> Option<EntryResponseSource> {
        self.entry_source.clone()
    }

    pub fn new(
        window: &(impl HasWindowHandle + HasDisplayHandle),
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        entry_source: Option<EntryResponseSource>,
    ) -> Result<Self, super::WebViewCreateError> {
        Self::new_inner(
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
        window: &(impl HasWindowHandle + HasDisplayHandle),
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        document: super::http_native::NativeDocument,
    ) -> Result<Self, super::WebViewCreateError> {
        Self::new_inner(
            window,
            bounds,
            scale_factor,
            surface_id,
            key_bridge,
            None,
            Some(document),
        )
    }

    fn new_inner(
        window: &(impl HasWindowHandle + HasDisplayHandle),
        bounds: WebViewBounds,
        scale_factor: f64,
        surface_id: u32,
        key_bridge: Rc<dyn WebViewKeySink>,
        entry_source: Option<EntryResponseSource>,
        http: Option<super::http_native::NativeDocument>,
    ) -> Result<Self, super::WebViewCreateError> {
        let parent_xid = match window.window_handle().map_err(perm)?.as_raw() {
            RawWindowHandle::Xlib(w) => w.window,
            _ => return Err(perm("Not an X11 window (Wayland is not supported)")),
        };

        let x11_display_ptr = match window.display_handle().map_err(perm)?.as_raw() {
            RawDisplayHandle::Xlib(d) => d
                .display
                .map(|p| p.as_ptr())
                .unwrap_or(std::ptr::null_mut()),
            _ => std::ptr::null_mut(),
        };

        if !gtk::is_initialized() {
            gtk::init().map_err(|e| perm(format!("GTK init failed: {e}")))?;
        }

        let xlib =
            x11_dl::xlib::Xlib::open().map_err(|e| perm(format!("Failed to open Xlib: {e}")))?;

        let physical = bounds.to_physical(scale_factor);
        let x = physical.x.round() as i32;
        let y = physical.y.round() as i32;
        let w = physical.width.round() as u32;
        let h = physical.height.round() as u32;

        let display = if x11_display_ptr.is_null() {
            // SAFETY: XOpenDisplay(null)는 DISPLAY env에서 기본 디스플레이를 연다.
            // 호출 실패 시 null 반환 — 아래 is_null 체크로 처리.
            // PlatformWebView::new는 winit event loop (main thread)에서만 호출되므로
            // Xlib 단일 thread 가정 충족.
            unsafe { (xlib.XOpenDisplay)(std::ptr::null()) }
        } else {
            x11_display_ptr as _
        };

        if display.is_null() {
            return Err(perm("Failed to get X11 display"));
        }

        // Create X11 child window
        // SAFETY: display는 위에서 null 체크 통과한 유효한 X11 Display*.
        // parent_xid는 winit이 만든 활성 X11 윈도우. 호출은 main thread.
        let x11_window = unsafe {
            (xlib.XCreateSimpleWindow)(display, parent_xid as _, x, y, w.max(1), h.max(1), 0, 0, 0)
        };

        if x11_window == 0 {
            return Err(transient("XCreateSimpleWindow failed"));
        }

        // SAFETY: 같은 생성 스레드에서 유효한 display와 방금 만든 창을 사용한다.
        // GDK는 다른 연결로 창을 조회하므로 XFlush만으로 부족하다. XSync로 서버의 생성 처리를 기다린다.
        unsafe {
            (xlib.XMapWindow)(display, x11_window);
            (xlib.XSync)(display, 0 /* discard = False */);
        }

        let gdk_display = gtk::gdk::Display::default().ok_or_else(|| perm("No GDK display"))?;

        let x11_gdk_display: gdkx11::X11Display = gdk_display
            .downcast()
            .map_err(|_| perm("GDK display is not X11"))?;

        let gdk_window =
            match crate::platform::x11_gdk_window::foreign_gdk_window(&x11_gdk_display, x11_window)
            {
                Ok(w) => w,
                Err(e) => {
                    // SAFETY: 방금 만든 창의 GDK 래핑이 실패했으므로 넘기지 않은 X 창을 여기서 지운다.
                    unsafe { (xlib.XDestroyWindow)(display, x11_window) };
                    // SAFETY: 위와 같은 유효한 display. 파괴 요청을 서버로 내보낸다 —
                    // 여기서는 왕복이 필요 없다(뒤에서 이 창을 조회하지 않는다).
                    unsafe { (xlib.XFlush)(display) };
                    return Err(transient(e));
                }
            };

        let gtk_window = gtk::Window::new(gtk::WindowType::Toplevel);
        let gdk_win_clone = gdk_window.clone();
        gtk_window.connect_realize(move |w| {
            // foreign 창 연결에 실패하면 별도 GTK 창에 그려질 수 있어 로그로 남긴다.
            if w.window().is_none() {
                tracing::warn!(
                    "WebView surface {surface_id}: GTK window realized without a GDK window; \
                     the page will render outside the parent window"
                );
                return;
            }
            w.set_window(gdk_win_clone.clone());
        });

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        gtk_window.add(&vbox);

        // 기본 WebContext가 첫 web process를 띄우기 전에 선택 확장을 지정한다.
        selection::install_default();
        let webview = if let Some(document) = &http {
            document.view.webview().clone()
        } else {
            entry_source.as_ref().map_or_else(WebView::new, |source| {
                super::entry_response::linux::webview(source.clone())
            })
        };
        vbox.pack_start(&webview, true, true, 0);

        // 탐색 정책만으로 서브리소스를 막을 수 없어 content filter도 별도로 설치한다.
        let remote = RemoteBlock::new(
            entry_source.is_some(),
            http.is_some(),
            WebViewExt::user_content_manager(&webview),
        );
        let pending_navigations = Rc::new(RefCell::new(Vec::new()));
        let script_gate: Rc<RefCell<Option<ScriptGate>>> = Rc::new(RefCell::new(None));
        navigation_policy::install(
            &webview,
            &entry_source,
            http.as_ref().map(|document| document.session_slot()),
            &remote,
            &pending_navigations,
            &script_gate,
            surface_id,
        );

        // 실패 후 Finished가 와도 Failed를 Done으로 덮지 않는다.
        let nav_state = Rc::new(Cell::new(NavState::Idle));
        let content_process_ended = Rc::new(Cell::new(false));
        // 비동기 선택 질의 뒤 입력·문서 교체·폐기가 있었는지 판정한다.
        let edit_guard = edit::EditGuard::default();
        let loads = Rc::new(RefCell::new(SignalOrderLoads::default()));
        let committed_url: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        {
            let nav = nav_state.clone();
            let loads = loads.clone();
            let gate = script_gate.clone();
            let remote = remote.clone();
            let committed = committed_url.clone();
            // load-changed는 main frame 로드에서만 온다(측정은 docs/surfaces/html/index.md의 OS별 적용 절). 화면 문서는 commit에서 기록한다.
            let edit_guard = edit_guard.clone();
            webview.connect_load_changed(move |wv, event| match event {
                LoadEvent::Started => {
                    edit_guard.bump();
                    tracing::debug!("WebView surface {surface_id}: load started");
                    loads.borrow_mut().started();
                    nav.set(NavState::Loading);
                    gate_step(wv, &gate, &remote, |g| Some(g.load_started()));
                }
                LoadEvent::Committed => {
                    edit_guard.unready();
                    let uri = wv.uri();
                    *committed.borrow_mut() = uri.as_deref().map(str::to_string);
                    remote.end_top_level();
                    gate_step(wv, &gate, &remote, |g| Some(g.committed(uri.as_deref())));
                }
                LoadEvent::Finished => {
                    let state = loads.borrow_mut().finished();
                    // 앞 로드의 늦은 종료는 새 로드의 top-level 허용을 닫지 않는다.
                    if state.is_some() {
                        remote.end_top_level();
                    }
                    if state.is_some() && nav.get() != NavState::Failed {
                        tracing::debug!("WebView surface {surface_id}: load finished");
                        nav.set(NavState::Done);
                    }
                    gate_step(wv, &gate, &remote, ScriptGate::finished);
                }
                _ => {} // Redirected 는 무시
            });
        }
        {
            // web process가 commit 전에 종료되면 load-failed도 load-changed Finished도 오지 않는다(실측).
            // 여기서 로드를 끝내지 않으면 스크립트 게이트가 로드 중에 머물러 허용을 거절한다.
            let nav = nav_state.clone();
            let ended = content_process_ended.clone();
            let loads = loads.clone();
            let gate = script_gate.clone();
            let remote = remote.clone();
            webview.connect_web_process_terminated(move |wv, reason| {
                tracing::warn!(
                    "WebView surface {surface_id}: WebKit web process terminated ({reason:?})"
                );
                loads.borrow_mut().terminated();
                remote.end_top_level();
                nav.set(NavState::Failed);
                ended.set(true);
                gate_step(wv, &gate, &remote, |g| {
                    g.failed();
                    g.finished()
                });
            });
        }
        {
            let nav = nav_state.clone();
            let loads = loads.clone();
            let gate = script_gate.clone();
            let http_session = http.as_ref().map(|document| document.session_slot());
            webview.connect_load_failed(move |_wv, _event, failing_uri, error| {
                tracing::warn!(
                    "WebView surface {surface_id}: WebKitGTK load-failed \
                     uri={failing_uri} err={error}"
                );
                if let Some(state) = loads.borrow().failed() {
                    nav.set(state);
                }
                if let Some(gate) = gate.borrow().as_ref() {
                    gate.failed();
                }
                if let Some(slot) = &http_session {
                    let session = slot.borrow().clone();
                    let intentional=session.bridge.document().is_some_and(|document|document.with_current(|model|matches!(model.state(document.entry()),Some(tasty_remote::html_http_client::ReceiveState::Skipped|tasty_remote::html_http_client::ReceiveState::Cancelled))).unwrap_or(false));
                    if session.is_current() && !intentional {session.fail();}
                }
                true // 기본 에러 페이지 억제(host error chrome 사용)
            });
        }

        // 키는 WebKit 기본 처리 전에 host 정책에 묻고 클릭은 페이지에도 전달한다.
        {
            let bridge = key_bridge.clone();
            let edit_guard = edit_guard.clone();
            webview.connect_key_press_event(move |wv, ev| {
                // WebKit 기본 처리 전에 Super 계열을 DOM metaKey로 보이게 한다. UI 분류에는 영향이 없다.
                selection::add_meta_for_webkit(ev);
                // 이 press 신호에는 repeat 플래그가 없어 반복 입력도 host로 전달한다.
                if ev.is_modifier() {
                    return gtk::glib::Propagation::Proceed;
                }
                // host가 직접 시작한 선택 질의는 이 키 뒤에 실행하지 않는다.
                let newer = edit_guard.note_key(ev.time());
                let Some(key) = gdk_keyval_to_winit_key(ev.keyval()) else {
                    return gtk::glib::Propagation::Proceed;
                };
                let physical = x11_keycode_to_physical(ev.hardware_keycode());
                let mods = gdk_state_to_winit_mods(ev.state());
                if bridge.blocks_edit_key(&key, &physical, mods) {
                    return gtk::glib::Propagation::Stop;
                }
                // 사용자 편집 바인딩은 키를 받은 이 WebView에서 입력 순서대로 엔진 명령으로 실행한다.
                if let Some(action) = bridge.edit_action(&key, &physical, mods) {
                    // 응답 전에 host 모달·overlay가 열리면 실행하지 않도록 같은 키로 다시 묻는다.
                    let gate = bridge.clone();
                    let blocked: edit::Blocked =
                        Box::new(move || gate.blocks_edit_key(&key, &physical, mods));
                    let arm_key = selection::ArmKey::from_gdk(ev.keyval(), ev.state());
                    return edit::run_key_action(wv, action, arm_key, &edit_guard, blocked);
                }
                // 바인딩에서 빠진 조합은 확장이 더 소유하지 않게 한다. 이 메시지는 키보다 먼저 도착한다.
                if newer
                    && let Some(arm_key) = selection::ArmKey::from_gdk(ev.keyval(), ev.state())
                    && edit_guard.unclaim(arm_key)
                {
                    selection::unclaim(wv, arm_key);
                }
                if bridge.capture_key(surface_id, key, physical, mods) {
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
        }
        webview.connect_key_release_event(|_wv, ev| {
            selection::add_meta_for_webkit(ev);
            gtk::glib::Propagation::Proceed
        });
        {
            let bridge = key_bridge.clone();
            let edit_guard = edit_guard.clone();
            webview.connect_button_press_event(move |_wv, _ev| {
                edit_guard.bump();
                bridge.note_focus(surface_id);
                gtk::glib::Propagation::Proceed
            });
        }
        {
            // 포커스가 떠난 뒤 늦게 온 선택 응답으로 Copy·Cut을 실행하지 않는다.
            let edit_guard = edit_guard.clone();
            webview.connect_focus_out_event(move |_wv, _ev| {
                edit_guard.bump();
                gtk::glib::Propagation::Proceed
            });
        }
        {
            // 편집 확장의 준비 알림과 입력 순서대로 도착한 무장 키의 답.
            let edit_guard = edit_guard.clone();
            webview.connect_user_message_received(move |wv, message| {
                edit::on_view_message(wv, message, &edit_guard)
            });
        }
        // 키는 toplevel의 event 신호를 먼저 지나고 포인터는 WebView의 event 신호를 먼저 지난다.
        let input_held = Rc::new(Cell::new(false));
        {
            let held = input_held.clone();
            gtk_window.connect_event(move |_w, ev| held_input(&held, ev));
            let held = input_held.clone();
            webview.connect_event(move |_wv, ev| held_input(&held, ev));
        }

        // 필터는 비동기로 붙는다. 그전에 온 원격 요청까지 막는다고 보장하지 않는다.
        compile_remote_block_filter(
            remote.ucm(),
            remote.block_cell().clone(),
            remote.filter_slot().clone(),
        );

        gtk_window.show_all();

        // 창 shape 요청은 서버 SHAPE 확장에 기댄다. 없으면 XFixes 를 쓰지 않는다.
        let xfixes = linux_shape::shape_extension_ready(&xlib, display as _)
            .then(|| open_xfixes(display as _))
            .flatten();

        Ok(Self {
            entry_source,
            http,
            webview,
            gtk_window,
            x11_window,
            xlib,
            x11_display: display as _,
            origin_thread: std::thread::current().id(),
            remote,
            nav_state,
            content_process_ended,
            loads,
            pending_navigations,
            committed_url,
            script_gate,
            parent_x11_window: parent_xid as _,
            gdk_window,
            x11_size: Cell::new((w.max(1) as i32, h.max(1) as i32)),
            xfixes,
            shape: RefCell::default(),
            clip_unavailable_reported: Cell::new(false),
            input_held,
            edit_guard,
            bed_focus: bed_focus::BedFocusSheet::new(),
        })
    }

    /// Xlib 핸들 접근은 생성 스레드에 한정한다. release 빌드에서도 확인한다.
    fn assert_origin_thread(&self) {
        assert_eq!(
            std::thread::current().id(),
            self.origin_thread,
            "PlatformWebView(X11) accessed from a thread different from its creation \
             thread; raw Xlib Display*/Window handles require single-thread confinement \
             (no XInitThreads)"
        );
    }

    pub fn set_bounds(&self, bounds: WebViewBounds, scale_factor: f64) {
        self.assert_origin_thread();
        let physical = bounds.to_physical(scale_factor);
        let x = physical.x.round() as i32;
        let y = physical.y.round() as i32;
        let w = physical.width.round() as i32;
        let h = physical.height.round() as i32;

        // SAFETY: self가 살아있으면 x11_display/x11_window 모두 valid (Drop이 정리).
        // 호출은 main thread (winit event loop) 흐름에서만 일어남 — 위
        // assert_origin_thread 가 이를 런타임으로 강제한다.
        unsafe {
            (self.xlib.XMoveResizeWindow)(
                self.x11_display as _,
                self.x11_window,
                x,
                y,
                w.max(1) as u32,
                h.max(1) as u32,
            );
            (self.xlib.XFlush)(self.x11_display as _);
        }

        // GTK 크기·allocation은 GDK 창 배율로 나눈 논리 px다. X 창과 같은 물리 크기를 덮도록
        // 올림한다. 물리 px를 그대로 주면 GDK 배율만큼 페이지 viewport가 X 창보다 커진다.
        let gdk_scale = self.gdk_window.scale_factor().max(1);
        let gtk_w = (w.max(1) + gdk_scale - 1) / gdk_scale;
        let gtk_h = (h.max(1) + gdk_scale - 1) / gdk_scale;
        self.x11_size.set((w.max(1), h.max(1)));
        self.gtk_window.resize(gtk_w, gtk_h);

        // foreign X 창의 크기 변경을 GTK가 자동 반영하지 못하므로 allocation도 직접 갱신한다.
        self.gtk_window
            .size_allocate(&gtk::Allocation::new(0, 0, gtk_w, gtk_h));
    }

    /// 이 창에서 구멍을 뺀다. 값은 모두 이 X 창 기준 물리 px `[x, y, 너비, 높이]` 다.
    /// `input` 은 host 가 받아야 하는 입력 영역(분할선 hit 띠·창 리사이즈 밴드)으로, 입력에서만 빼고
    /// 화면에는 페이지가 그대로 보인다. `clip` 은 위에 egui 가 그리는 안내 카드로, 그림(bounding
    /// shape)과 입력에서 함께 뺀다. 빠진 곳은 부모 창(winit)의 그림이 보이고 포인터도 부모로 간다.
    /// 창 크기·매핑·포커스는 바꾸지 않는다. 둘 다 비면 shape 를 지워 원래 사각형으로 돌아간다.
    /// GDK 의 shape 는 GDK 논리 px 단위라 배율 2 에서 홀수 경계를 나타내지 못하므로 XFixes 로
    /// X 창에 물리 px 그대로 설정한다.
    pub fn set_native_holes(&self, input: &[[i32; 4]], clip: &[[i32; 4]], _scale_factor: f64) {
        self.assert_origin_thread();
        if self.xfixes.is_none() {
            if !clip.is_empty() && !self.clip_unavailable_reported.replace(true) {
                tracing::warn!(
                    "WebView clip unavailable without XFixes; the page stays drawn over the notice"
                );
            }
            return;
        }
        self.shape.borrow_mut().set_native_holes(input, clip);
        self.refresh_shape();
    }

    /// native 구멍과 오버레이 구멍을 합쳐 X 창 shape 를 맞춘다. 두 입력 중 하나가 바뀔 때마다 부른다.
    /// 오버레이 적용은 같은 프레임의 `set_bounds` 보다 먼저 올 수 있으므로 창 크기가 바뀐 프레임은 뒤의
    /// `set_native_holes` 호출이 새 크기로 다시 맞춘다.
    fn refresh_shape(&self) {
        let Some(fixes) = &self.xfixes else {
            return;
        };
        let Some(shape) = self.shape.borrow_mut().take_change(self.x11_size.get()) else {
            return;
        };
        // SAFETY: self 가 살아 있으면 display·x11_window 는 유효하고 호출부가 origin thread 를 확인했다.
        unsafe {
            linux_shape::apply(
                fixes,
                &self.xlib,
                self.x11_display as _,
                self.x11_window,
                &shape,
            );
        }
    }

    /// 저장이 멈춘 뒤 이 페이지의 키·포인터 입력을 버린다. 되돌리지 않는다.
    pub fn hold_input_for_halt(&self) {
        self.input_held.set(true);
    }

    /// 저장이 멈춘 동안 바뀐 shape를 GDK가 받게 한다. 이 창의 X 요청을 서버가 처리하게 한 뒤 GDK 연결을
    /// 왕복시켜 노출 이벤트를 GDK 대기열에 넣는다. GTK 대기열은 여기서 비우지 않는다. 다른 창의 페이지
    /// 입력까지 모두 막은 뒤 halt 중 GTK 처리 주기(`App::halted_webview_paint_deadline`)가 비운다.
    pub fn sync_halted_shape(&self) {
        self.assert_origin_thread();
        // SAFETY: self가 살아 있으면 display는 유효하고 origin thread다(위 assert).
        unsafe {
            (self.xlib.XSync)(self.x11_display as _, 0 /* discard = False */);
        }
        self.gdk_window.display().sync();
    }

    pub fn set_visible(&self, visible: bool) {
        self.assert_origin_thread();
        if visible {
            // SAFETY: self valid; main thread(위 assert_origin_thread 로 런타임 강제).
            unsafe {
                (self.xlib.XMapWindow)(self.x11_display as _, self.x11_window);
                (self.xlib.XFlush)(self.x11_display as _);
            }
            self.gtk_window.show_all();
        } else {
            // 숨긴 뒤 늦게 온 선택 응답으로 Copy·Cut을 실행하지 않는다.
            if self.gtk_window.is_visible() {
                self.edit_guard.bump();
            }
            // SAFETY: self valid; main thread(위 assert_origin_thread 로 런타임 강제).
            unsafe {
                (self.xlib.XUnmapWindow)(self.x11_display as _, self.x11_window);
                (self.xlib.XFlush)(self.x11_display as _);
            }
            self.gtk_window.hide();
        }
    }

    /// 이 WebView 내부에 포커스가 있을 때만 부모 창으로 돌린다.
    /// 숨기기와 포커스 반환은 별개이며 다른 앱의 포커스를 가져오면 안 된다.
    pub fn release_keyboard_focus(&self) {
        self.assert_origin_thread();
        if !self.x11_focus_is_inside() {
            return;
        }
        self.edit_guard.bump();
        // SAFETY: self 가 살아있으면 x11_display 는 valid 하고 parent_x11_window 는
        // 이 webview 를 만든 winit 창(부모)이다. 호출은 origin thread(위 assert).
        unsafe {
            (self.xlib.XSetInputFocus)(
                self.x11_display as _,
                self.parent_x11_window,
                x11_dl::xlib::RevertToParent,
                x11_dl::xlib::CurrentTime,
            );
        }
        // SAFETY: 위와 동일(valid display, origin thread). 요청을 즉시 밀어낸다.
        unsafe {
            (self.xlib.XFlush)(self.x11_display as _);
        }
    }

    /// X focus가 이 WebView 안에 있는지. X focus는 서버 전역이라 참이면 이 앱이 OS 포커스를 가진 것이다.
    /// 자식 창이 focus를 가지면 winit이 부모 창에 Focused(false)를 보내므로 호출부가 창 활성 판정에 함께 쓴다.
    pub fn holds_keyboard_focus(&self) -> bool {
        self.assert_origin_thread();
        self.x11_focus_is_inside()
    }

    /// 지금 키를 치면 이 WebView가 받는지. X 포커스가 자식 안에 있거나, 포커스 창이 이
    /// WebView의 조상(또는 PointerRoot)이고 포인터가 WebView 위에 있으면 참이다. X 서버는
    /// 포커스 창의 자손 위에 포인터가 있으면 키를 그 자손에 보내므로, X 포커스가 메인 창에
    /// 있어도 키가 페이지로 갈 수 있다.
    /// 오버레이가 입력 shape 를 비운 동안에는 포인터가 페이지 위에 있어도 X 가 키를 부모로 보내므로
    /// 포커스만 본다.
    pub fn receives_keyboard_input(&self) -> bool {
        self.assert_origin_thread();
        self.x11_focus_is_inside()
            || (!self.shape.borrow().overlay_holds_pointer()
                && self.x11_focus_routes_by_pointer()
                && self.x11_pointer_is_inside())
    }

    /// 포커스 창이 PointerRoot이거나 이 WebView 창의 조상이면 참이다.
    fn x11_focus_routes_by_pointer(&self) -> bool {
        let mut focus: std::os::raw::c_ulong = 0;
        let mut revert: std::os::raw::c_int = 0;
        // SAFETY: display 는 valid(호출부가 origin thread 를 확인). 두 out 파라미터는
        // 살아있는 스택 변수의 주소다.
        unsafe {
            (self.xlib.XGetInputFocus)(self.x11_display as _, &mut focus, &mut revert);
        }
        // 0은 None(키가 버려짐), 1은 PointerRoot(포인터 아래 창이 받음)다.
        match focus {
            0 => return false,
            1 => return true,
            _ => {}
        }
        let mut w = self.x11_window;
        // 손상된 부모 관계에서 무한 순회하지 않도록 깊이를 제한한다.
        for _ in 0..32 {
            let Some(parent) = self.x11_parent_of(w) else {
                return false;
            };
            if parent == 0 || parent == w {
                return false;
            }
            if parent == focus {
                return true;
            }
            w = parent;
        }
        false
    }

    /// 포인터가 이 WebView 창의 사각형 안에 있는지. 숨겨진 창도 마지막 위치로 판정한다.
    fn x11_pointer_is_inside(&self) -> bool {
        let (mut root, mut child): (std::os::raw::c_ulong, std::os::raw::c_ulong) = (0, 0);
        let (mut root_x, mut root_y, mut x, mut y): (
            std::os::raw::c_int,
            std::os::raw::c_int,
            std::os::raw::c_int,
            std::os::raw::c_int,
        ) = (0, 0, 0, 0);
        let mut mask: std::os::raw::c_uint = 0;
        // SAFETY: display 는 valid(호출부가 origin thread 를 확인), out 파라미터는 전부
        // 살아있는 스택 변수의 주소다.
        let same_screen = unsafe {
            (self.xlib.XQueryPointer)(
                self.x11_display as _,
                self.x11_window,
                &mut root,
                &mut child,
                &mut root_x,
                &mut root_y,
                &mut x,
                &mut y,
                &mut mask,
            )
        };
        if same_screen == 0 {
            return false;
        }
        let (mut geo_root, mut gx, mut gy): (
            std::os::raw::c_ulong,
            std::os::raw::c_int,
            std::os::raw::c_int,
        ) = (0, 0, 0);
        let (mut width, mut height, mut border, mut depth): (
            std::os::raw::c_uint,
            std::os::raw::c_uint,
            std::os::raw::c_uint,
            std::os::raw::c_uint,
        ) = (0, 0, 0, 0);
        // SAFETY: 위와 같다.
        let ok = unsafe {
            (self.xlib.XGetGeometry)(
                self.x11_display as _,
                self.x11_window,
                &mut geo_root,
                &mut gx,
                &mut gy,
                &mut width,
                &mut height,
                &mut border,
                &mut depth,
            )
        };
        ok != 0 && x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height
    }

    fn x11_focus_is_inside(&self) -> bool {
        let mut focus: std::os::raw::c_ulong = 0;
        let mut revert: std::os::raw::c_int = 0;
        // SAFETY: display 는 valid(위 호출부가 origin thread 를 이미 확인). 두 out
        // 파라미터는 살아있는 스택 변수의 주소다.
        unsafe {
            (self.xlib.XGetInputFocus)(self.x11_display as _, &mut focus, &mut revert);
        }
        if focus <= 1 {
            return false;
        }
        let mut w = focus;
        // 손상된 부모 관계에서 무한 순회하지 않도록 깊이를 제한한다. 더 깊은 자손은 확인하지 못한다.
        for _ in 0..32 {
            if w == self.x11_window {
                return true;
            }
            let Some(parent) = self.x11_parent_of(w) else {
                return false;
            };
            if parent == 0 || parent == w {
                return false;
            }
            w = parent;
        }
        false
    }

    fn x11_parent_of(&self, window: std::os::raw::c_ulong) -> Option<std::os::raw::c_ulong> {
        let mut root: std::os::raw::c_ulong = 0;
        let mut parent: std::os::raw::c_ulong = 0;
        let mut children: *mut std::os::raw::c_ulong = std::ptr::null_mut();
        let mut nchildren: std::os::raw::c_uint = 0;
        // SAFETY: display 는 valid(origin thread 확인 완료), out 파라미터는 전부 살아있는
        // 스택 변수 주소다. children 은 Xlib 이 할당하며 바로 아래에서 해제한다.
        let ok = unsafe {
            (self.xlib.XQueryTree)(
                self.x11_display as _,
                window,
                &mut root,
                &mut parent,
                &mut children,
                &mut nchildren,
            )
        };
        if !children.is_null() {
            // SAFETY: 바로 위 XQueryTree 가 할당해 돌려준 배열이며 여기서만 해제한다.
            unsafe {
                (self.xlib.XFree)(children.cast());
            }
        }
        (ok != 0).then_some(parent)
    }

    pub fn nav_state(&self) -> NavState {
        self.nav_state.get()
    }

    /// 마지막 호출 뒤 web process가 끝났는지 돌려주고 기록을 지운다.
    pub fn take_content_process_ended(&self) -> bool {
        self.content_process_ended.replace(false)
    }

    /// Linux는 보류한 응답 결정을 GTK 작업이 직접 끝내므로 동기화에서 할 일이 없다.
    pub fn resume_script_decision(&self) {}

    pub fn take_pending_navigations(&self) -> Vec<PendingNavigation> {
        std::mem::take(&mut *self.pending_navigations.borrow_mut())
    }

    pub fn set_zoom(&self, factor: f64) {
        self.webview.set_zoom_level(factor);
    }

    /// 게이트가 붙은 webview에서는 전역 sandbox 설정으로 받고, 문서 단위 판단을 적용한다.
    pub fn set_javascript_enabled(&self, enabled: bool) {
        let js = match self.script_gate.borrow().as_ref() {
            Some(gate) => gate.set_sandbox(!enabled),
            None => enabled,
        };
        set_js(&self.webview, js);
    }

    /// 문서 단위 스크립트 허용을 붙인다. 첫 로드 전에 호출한다.
    /// page cache를 끄면 뒤로·앞으로 가기도 응답 단계를 거쳐 모든 문서가 지문을 가진다(docs/surfaces/html/index.md).
    pub fn attach_script_gate(&self, gate: ScriptGate) {
        if let Some(settings) = WebViewExt::settings(&self.webview) {
            settings.set_enable_page_cache(false);
        }
        set_js(&self.webview, gate.effective_js());
        self.remote.sync(&gate);
        *self.script_gate.borrow_mut() = Some(gate);
    }

    pub(crate) fn retaining_http(&self) -> bool {
        self.http
            .as_ref()
            .is_some_and(|document| document.retaining())
    }
    pub(crate) fn request_http_reload(&self) -> Result<bool, String> {
        let Some(document) = &self.http else {
            return Ok(false);
        };
        self.edit_guard.bump();
        document.request_reload()?;
        Ok(true)
    }
    pub(crate) fn poll_http_reload(
        &self,
        session: std::sync::Arc<crate::app::http_session::HttpSession>,
        wake: std::sync::Arc<dyn Fn() + Send + Sync>,
    ) -> Result<bool, String> {
        let Some(document) = &self.http else {
            return Ok(false);
        };
        if !session.is_current() && !document.retaining() {
            self.set_javascript_enabled(false);
        }
        document.poll_trust(wake.clone())?;
        let Some(ready) = document.poll_reload(session.clone(), wake)? else {
            return Ok(false);
        };
        if let Some(gate) = self.script_gate.borrow_mut().as_mut() {
            *gate = gate.clone().with_http_session(session);
        }
        ready
            .resume()
            .map_err(|error| format!("retained HTTP native continuation: {error:?}"))?;
        Ok(true)
    }

    pub(crate) fn reload_after_script_allow(&self) {
        if let Some(document) = &self.http {
            let Some(body) = document.session().entry_body() else {
                return;
            };
            match document.session().bodies.replay_once(body.head().id) {
                Ok(()) => {
                    self.edit_guard.bump();
                    self.webview.reload_bypass_cache();
                }
                Err(error) => tracing::warn!(?error, "native HTTP script response replay refused"),
            }
        } else {
            self.reload();
        }
    }

    /// 팔레트·host 키 경로의 편집 명령. 엔진이 포커스 frame과 편집 가능 여부를 판단한다.
    pub fn execute_edit(&self, action: super::WebViewEditAction) {
        edit::run_editing_command(&self.webview, action, &self.edit_guard);
    }

    /// 사용자 탐색 조작에 없는 로드 중지를 debug IPC에서 재현한다.
    #[cfg(debug_assertions)]
    pub fn debug_stop(&self) {
        self.webview.stop_loading();
    }

    /// 바탕 포커스 상태를 문서에 둔다. 다음에 싣는 문서에도 남는다(`super::bed_focus`).
    pub fn set_bed_focused(&self, focused: bool) {
        self.bed_focus.set(&self.webview, focused);
    }

    /// 현재 Linux에서는 적용하지 않고 로그만 남긴다.
    pub fn set_color_scheme(&self, scheme: super::ColorScheme) {
        tracing::debug!("set_color_scheme({scheme:?}) is not implemented for Linux WebKitGTK");
    }

    /// 전역 "Allow remote content"를 적용한다. 문서 단위 허용은 게이트가 정하고 실효값은 둘의 OR다.
    /// 필터 준비 전·준비 실패 시 적용 범위가 다르다.
    pub fn set_remote_content_allowed(&self, allowed: bool) {
        if let Some(gate) = self.script_gate.borrow().as_ref() {
            gate.set_remote_global(allowed);
        }
        self.remote.set_global(allowed);
        tracing::debug!("Linux WebKitGTK set_remote_content_allowed({allowed})");
    }
}

impl Drop for PlatformWebView {
    fn drop(&mut self) {
        self.assert_origin_thread();
        if let Some(source) = &self.entry_source {
            source.retire();
        }
        self.edit_guard.retire();
        // GDK 연결의 지연 요청이 이미 지운 X 창을 참조할 수 있어 정리 중 오류 트랩으로 기록한다.
        let trap: Option<gdkx11::X11Display> = self.gdk_window.display().downcast().ok();
        if let Some(d) = &trap {
            d.error_trap_push();
        }
        if let Some(document) = self.http.take() {
            // NativeDocument revokes endpoint/body/view authority before destroying GTK or
            // letting the event pump run. Automatic field Drop would happen after this method.
            drop(document);
        } else {
            // SAFETY: 생성 스레드 확인 뒤 이 객체가 소유한 WebView를 정리한다.
            unsafe {
                self.webview.destroy();
            }
        }
        // X 창을 지우기 전에 GTK의 숨기기·닫기 요청을 처리한다.
        self.gtk_window.hide();
        pump_gtk();
        // foreign GdkWindow는 GTK 위젯 소유 창이 아니므로 widget destroy 대신 close를 사용한다.
        self.gtk_window.close();
        pump_gtk();
        // GDK는 자기 연결로 요청을 내고 X 창은 아래에서 winit 연결로 지운다. 서버는 연결 사이의 처리 순서를
        // 보장하지 않으므로 GDK 연결을 sync해 trap 전에 낸 요청(drop 직전 set_visible(false)의 hide 등)까지 먼저 처리시킨다.
        self.gdk_window.display().sync();
        // SAFETY: 이 객체가 생성한 X 창을 생성 스레드에서 지운다. GDK의 후속 오류는 위 트랩으로 기록한다.
        unsafe {
            (self.xlib.XDestroyWindow)(self.x11_display as _, self.x11_window);
            (self.xlib.XSync)(self.x11_display as _, 0 /* discard = False */);
        }
        pump_gtk();
        if let Some(d) = &trap {
            let code = d.error_trap_pop();
            if code != 0 {
                tracing::warn!(
                    x11_window = self.x11_window,
                    error_code = code,
                    "webview 정리 중 X 에러 — 에러 트랩이 abort 를 막았다"
                );
            }
        }
    }
}

/// GTK의 대기 이벤트를 처리한다. 창을 먼저 지운 뒤 지연된 unmap 요청이 나가는 일을 줄인다.
/// 전역 GTK 큐를 처리하므로 이 WebView의 콜백만 실행한다고 볼 수는 없다.
/// 보류한 응답 결정을 스캔이 끝난 뒤 GTK 스레드에서 끝낸다. 결과가 낡았으면(그 사이 새 로드) 거절한다.
fn defer_response(
    wv: WebView,
    decision: webkit2gtk::PolicyDecision,
    scan: DeferredScan,
    remote: RemoteBlock,
    surface_id: u32,
) {
    let gate = scan.gate().clone();
    gtk::glib::MainContext::default().spawn_local(async move {
        let read = gtk::gio::spawn_blocking(scan.reader()).await;
        // 표 확인과 적용은 결정을 넘기는 이 GTK 작업 안에서 한다. 읽는 동안 새 로드가 시작됐으면
        // 여기서 낡은 표로 판정되어 결정을 거절한다.
        match read.map(|result| scan.apply(result)) {
            Ok(Some(js)) => {
                set_js(&wv, js);
                remote.sync(&gate);
                decision.use_();
            }
            Ok(None) => decision.ignore(),
            Err(_) => {
                tracing::warn!(
                    "WebView surface {surface_id}: html script scan panicked; the response is refused"
                );
                decision.ignore();
            }
        }
    });
}

/// 입력을 멈춘 페이지에 온 키·포인터·터치 이벤트면 Stop을 돌려 WebKit과 host 키 처리에 보내지 않는다.
fn held_input(held: &Cell<bool>, ev: &gtk::gdk::Event) -> gtk::glib::Propagation {
    use gtk::gdk::EventType as E;
    let input = matches!(
        ev.event_type(),
        E::KeyPress
            | E::KeyRelease
            | E::ButtonPress
            | E::DoubleButtonPress
            | E::TripleButtonPress
            | E::ButtonRelease
            | E::MotionNotify
            | E::Scroll
            | E::EnterNotify
            | E::LeaveNotify
            | E::TouchBegin
            | E::TouchUpdate
            | E::TouchEnd
            | E::TouchCancel
            | E::TouchpadSwipe
            | E::TouchpadPinch
    );
    if held.get() && input {
        gtk::glib::Propagation::Stop
    } else {
        gtk::glib::Propagation::Proceed
    }
}

/// WebKit 편집 명령은 비활성 상태(선택 없음·읽기 전용·본문 붙여넣기)에서 아무것도 하지 않는다.
fn pump_gtk() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
}

/// X11 keycode에서 8을 빼 winit이 사용하는 evdev scancode로 변환한다.
fn x11_keycode_to_physical(hardware_keycode: u16) -> winit::keyboard::PhysicalKey {
    use winit::platform::scancode::PhysicalKeyExtScancode;
    let Some(evdev) = (hardware_keycode as u32).checked_sub(8) else {
        return winit::keyboard::PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Xkb(
            hardware_keycode as u32,
        ));
    };
    winit::keyboard::PhysicalKey::from_scancode(evdev)
}

/// GDK modifier를 winit 값으로 변환한다. Linux의 alt는 MOD1/ALT다.
fn gdk_state_to_winit_mods(state: gtk::gdk::ModifierType) -> winit::keyboard::ModifiersState {
    use gtk::gdk::ModifierType;
    use winit::keyboard::ModifiersState;
    let mut mods = ModifiersState::empty();
    mods.set(
        ModifiersState::CONTROL,
        state.contains(ModifierType::CONTROL_MASK),
    );
    mods.set(
        ModifiersState::SHIFT,
        state.contains(ModifierType::SHIFT_MASK),
    );
    mods.set(ModifiersState::ALT, state.contains(ModifierType::MOD1_MASK));
    mods.set(
        ModifiersState::SUPER,
        state.contains(ModifierType::SUPER_MASK),
    );
    mods
}

/// 알려진 named key와 Unicode 문자를 변환한다. 매핑할 수 없으면 페이지에 남긴다.
fn gdk_keyval_to_winit_key(keyval: gtk::gdk::keys::Key) -> Option<winit::keyboard::Key> {
    use gtk::gdk::keys::constants as k;
    use winit::keyboard::{Key, NamedKey};

    let named = match keyval {
        k::Tab | k::ISO_Left_Tab => NamedKey::Tab,
        k::Return | k::KP_Enter => NamedKey::Enter,
        k::BackSpace => NamedKey::Backspace,
        k::Delete | k::KP_Delete => NamedKey::Delete,
        k::Insert | k::KP_Insert => NamedKey::Insert,
        k::Home | k::KP_Home => NamedKey::Home,
        k::End | k::KP_End => NamedKey::End,
        k::Page_Up | k::KP_Page_Up => NamedKey::PageUp,
        k::Page_Down | k::KP_Page_Down => NamedKey::PageDown,
        k::Up | k::KP_Up => NamedKey::ArrowUp,
        k::Down | k::KP_Down => NamedKey::ArrowDown,
        k::Left | k::KP_Left => NamedKey::ArrowLeft,
        k::Right | k::KP_Right => NamedKey::ArrowRight,
        k::Escape => NamedKey::Escape,
        k::space | k::KP_Space => NamedKey::Space,
        k::F1 => NamedKey::F1,
        k::F2 => NamedKey::F2,
        k::F3 => NamedKey::F3,
        k::F4 => NamedKey::F4,
        k::F5 => NamedKey::F5,
        k::F6 => NamedKey::F6,
        k::F7 => NamedKey::F7,
        k::F8 => NamedKey::F8,
        k::F9 => NamedKey::F9,
        k::F10 => NamedKey::F10,
        k::F11 => NamedKey::F11,
        k::F12 => NamedKey::F12,
        k::F13 => NamedKey::F13,
        k::F14 => NamedKey::F14,
        k::F15 => NamedKey::F15,
        k::F16 => NamedKey::F16,
        k::F17 => NamedKey::F17,
        k::F18 => NamedKey::F18,
        k::F19 => NamedKey::F19,
        k::F20 => NamedKey::F20,
        k::F21 => NamedKey::F21,
        k::F22 => NamedKey::F22,
        k::F23 => NamedKey::F23,
        k::F24 => NamedKey::F24,
        _ => {
            let c = keyval.to_unicode()?;
            if c.is_control() {
                return None;
            }
            return Some(Key::Character(c.to_string().into()));
        }
    };
    Some(Key::Named(named))
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    #[test]
    fn keyval_maps_to_character_and_named() {
        use gtk::gdk::keys::constants as k;
        assert_eq!(
            gdk_keyval_to_winit_key(k::d),
            Some(Key::Character("d".into()))
        );
        assert_eq!(
            gdk_keyval_to_winit_key(k::equal),
            Some(Key::Character("=".into()))
        );
        assert_eq!(
            gdk_keyval_to_winit_key(k::Escape),
            Some(Key::Named(NamedKey::Escape))
        );
        assert_eq!(gdk_keyval_to_winit_key(k::Control_L), None);
        assert_eq!(
            gdk_keyval_to_winit_key(k::F13),
            Some(Key::Named(NamedKey::F13))
        );
        assert_eq!(
            gdk_keyval_to_winit_key(k::F24),
            Some(Key::Named(NamedKey::F24))
        );
    }

    #[test]
    fn modifier_state_maps_to_winit() {
        use gtk::gdk::ModifierType;
        let mods = gdk_state_to_winit_mods(ModifierType::CONTROL_MASK | ModifierType::MOD1_MASK);
        assert!(mods.control_key());
        assert!(mods.alt_key());
        assert!(!mods.shift_key());
        assert_eq!(
            gdk_state_to_winit_mods(ModifierType::empty()),
            ModifiersState::empty()
        );
    }
}
