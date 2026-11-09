//! App synchronizes native WebView effects against the originating runtime resources.
//! Native windows, geometry caches and input handles remain owned by MainView.
mod bed_focus;
mod construction;
mod lazy;
mod navigation;
use construction::create_missing_webviews;
pub(crate) use lazy::announce_lazy_webviews;
use lazy::{ScriptsWithoutWebview, scripts_without_webview};
use navigation::{apply_agent_history_step, apply_user_nav};

use crate::plugin::PluginManager;
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::engine_access::EngineMut;
use crate::runtime::engine_access::EngineRef;
use crate::view::MainView;
use crate::view::main::redraw::{
    MAX_WEBVIEW_CREATE_ATTEMPTS, REVEAL_PENDING_WARN_AFTER, describe_webview_url,
    host_window_has_os_focus, next_webview_attempts, should_attempt_webview,
    take_hidden_webview_focus_targets, webview_release_candidate_is_live,
};
use crate::view::ui::View;
use tasty_model::html_script::BannerPhase;

mod script_gate;
use script_gate::attach_script_gate;
mod overlay_host_settle;
mod overlay_settle;
pub(crate) use overlay_host_settle::settle_overlay_host;
use overlay_settle::{apply_reveal, settle_overlay};

pub(crate) fn synchronize(
    view: &mut MainView,
    engine: &mut EngineMut<'_>,
    plugins: Option<&PluginManager>,
    proofs: &super::html_runtime::NavigationProofs,
) {
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    {
        let mut replaced = Vec::new();
        for (sid, wv) in &view.webviews {
            if let Some(session) = find_remote_surface(view, &engine.as_ref(), *sid)
                .and_then(|remote| remote.http_session.clone())
            {
                match wv.poll_http_reload(session.clone(), engine.runtime.waker.clone()) {
                    Ok(true) => replaced.push(*sid),
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(%error,"native HTTP replacement failed");
                        session.fail();
                    }
                }
            }
        }
        for sid in replaced {
            if let Some(binding) =
                super::html_runtime::NativeWebviewBinding::capture(&engine.as_ref(), sid)
            {
                view.webview_runtime.insert(sid, binding);
            }
        }
    }
    let stale: Vec<_> = view
        .webviews
        .keys()
        .copied()
        .filter(|surface| {
            !view.webview_runtime.get(surface).is_some_and(|binding| {
                #[cfg(any(target_os = "linux", target_os = "macos", windows))]
                if view
                    .webviews
                    .get(surface)
                    .is_some_and(|wv| wv.retaining_http())
                {
                    return binding.current_surface(&engine.as_ref());
                }
                binding.current(&engine.as_ref())
            })
        })
        .collect();
    for surface in stale {
        forget_webview(view, surface, proofs);
    }
    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    view.http_preparing.retain(|_, pending| pending.current());
    sync_webviews(view, &engine.as_ref(), plugins, proofs);
    if view.base.state.dirty && view.base.surface_ready() {
        view.base.winit.request_redraw();
    }
}

/// 저장이 멈춘 동안 활성 native 페이지의 bounds는 유지하고 안내 카드의 그림·입력 구멍만 갱신한다.
/// 생성·주소 갱신·이동·설정·요청 처리는 하지 않는다.
/// 구멍을 바꾼 뒤 이 창 페이지의 입력을 막고 바뀐 shape를 GDK에 전달한다. GTK 대기열은 모든 창의
/// 페이지를 막은 뒤 `App::halted_webview_paint_deadline`에서만 비운다(Linux).
pub(crate) fn sync_halted_clipping(view: &mut MainView, engine: &EngineRef<'_>) {
    let scale_factor = view.base.gpu.scale_factor() as f64;
    let HtmlSurfaces {
        bounds: active_html,
        holes: active_holes,
        clips: active_clips,
        ..
    } = collect_html_surfaces(view, engine, scale_factor);
    for (sid, wv) in &view.webviews {
        if let Some(bounds) = active_html.get(sid) {
            wv.set_bounds(*bounds, scale_factor);
            apply_native_holes(
                wv,
                active_holes.get(sid).map_or(&[][..], Vec::as_slice),
                active_clips.get(sid).map_or(&[][..], Vec::as_slice),
                scale_factor,
            );
        }
    }
    for wv in view.webviews.values() {
        wv.hold_input_for_halt();
        wv.sync_halted_shape();
    }
}

/// `collect_html_surfaces` 의 결과. 활성 surface 는 세 맵에 같은 키로 들어 있다.
struct HtmlSurfaces {
    /// 플랫폼에 넘길 논리 좌표.
    bounds: std::collections::HashMap<u32, crate::webview::WebViewBounds>,
    /// egui 툴팁 배치에 쓸 물리 사각형.
    physical: std::collections::HashMap<u32, crate::model::PhysicalRect>,
    /// WebView 입력에서 뺄 host 입력 영역(WebView 기준 물리 px).
    holes: std::collections::HashMap<u32, Vec<crate::model::PhysicalRect>>,
    /// WebView 그림과 입력에서 함께 뺄 복원 안내 카드 영역(WebView 기준 물리 px).
    clips: std::collections::HashMap<u32, Vec<crate::model::PhysicalRect>>,
    all_ids: Vec<u32>,
}

/// surface에 옮길 탐색 상태. 실패한 HTTP 세션은 `failed`만 보고한다. 세션 refresh도 같은 값을
/// 쓰므로, 남아 있는 view의 native 상태(`done` 등)와 프레임마다 번갈아 쓰이며 다시 그리지 않는다.
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
fn mirrored_nav_state(
    native: crate::webview::NavState,
    session_failed: bool,
) -> crate::webview::NavState {
    if session_failed {
        crate::webview::NavState::Failed
    } else {
        native
    }
}

/// native WebView를 둘 surface인지. 호스트 주소가 있는 surface와, 주소 띠로 문서를 연 빈 html
/// surface다. 빈 html 탭은 사용자가 처음 주소를 열 때(사용자 탐색 큐에 들어올 때) 만들고 그 뒤로 둔다.
fn hosts_webview(view: &MainView, surface: &dyn crate::model::Surface, sid: u32) -> bool {
    if let Some(remote) = surface.as_any().downcast_ref::<RemoteSurface>() {
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if let Some(session) = remote.http_session.as_ref()
            && (session.is_current() || session.entry_body().is_some())
        {
            return true;
        }
        // 원격 문서(entry·HTTP mirror)는 위 조건이나 받은 본문이 있을 때만 만든다.
        // 로컬 plugin 문서(markdown·html)는 아래 URL 판정으로 간다.
        if let Some(mirror) = remote.lock_html_mirror() {
            return mirror.body_store().current().is_some();
        }
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        if remote.http_session.is_some() {
            return false;
        }
    }
    surface.webview_url().is_some()
        || (surface.kind() == "html"
            && (view.webviews.contains_key(&sid)
                || view.state.webview_user_nav.iter().any(|(s, _)| *s == sid)))
}

/// HTML surface 전체와 활성 surface의 영역을 수집한다. native 호출은 하지 않는다.
/// 활성 surface의 영역은 플랫폼에 넘길 논리 좌표와 egui 툴팁 배치에 쓸 물리 사각형을 함께 돌려준다.
fn collect_html_surfaces(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    scale_factor: f64,
) -> HtmlSurfaces {
    let terminal_rect = view.compute_terminal_rect();

    // Collect all Html surface IDs and their visibility/bounds
    let active_ws = view.state.active_workspace_index(engine);
    let mut active_html: std::collections::HashMap<u32, crate::webview::WebViewBounds> =
        std::collections::HashMap::new();
    let mut active_physical: std::collections::HashMap<u32, crate::model::PhysicalRect> =
        std::collections::HashMap::new();
    let mut active_holes: std::collections::HashMap<u32, Vec<crate::model::PhysicalRect>> =
        std::collections::HashMap::new();
    let mut active_clips: std::collections::HashMap<u32, Vec<crate::model::PhysicalRect>> =
        std::collections::HashMap::new();
    // 오른쪽 위 복원 안내 카드. native 창은 이 부분을 그리지 않고 포커스·bounds는 그대로 둔다.
    let notice_cards: Vec<crate::model::PhysicalRect> = view
        .state
        .restore_notices
        .card_rects()
        .iter()
        .map(|r| r.to_physical(scale_factor as f32))
        .collect();
    let mut all_html_ids: Vec<u32> = Vec::new();
    let window = view.base.gpu.size();
    let resize_band = view.window_resize_band();

    for (ws_idx, ws) in engine.workspaces().into_iter().enumerate() {
        all_html_ids.extend(ws.all_surface_ids().into_iter().filter(|sid| {
            engine
                .find_surface_by_id(*sid)
                .is_some_and(|surface| hosts_webview(view, surface, *sid))
        }));
        let pane_rects = view
            .state
            .pane_rects(engine, ws, terminal_rect, scale_factor as f32);
        let pane_dividers =
            view.state
                .pane_dividers(engine, ws, terminal_rect, scale_factor as f32);
        for (pane_id, pane_rect) in &pane_rects {
            if let Some(pane) = ws.pane_layout().find_pane(*pane_id) {
                // tab bar 아래 콘텐츠 영역 — 탭 내부 분할(SurfaceGroup)의 leaf rect
                // 는 이 영역을 기준으로 계산한다(`impl_pty` / `focus` 와 동일 유도).
                let content_rect = crate::model::PhysicalRect {
                    x: pane_rect.x,
                    y: pane_rect.y + view.state.tab_bar_height,
                    width: pane_rect.width,
                    height: (pane_rect.height - view.state.tab_bar_height)
                        .max(crate::model::PhysicalPx(1.0)),
                };
                for (tab_idx, tab) in pane.tabs.iter().enumerate() {
                    let Some(_) = tab.layout_if_initialized() else {
                        continue;
                    };
                    // Only visible if: active workspace AND active tab
                    let is_visible =
                        ws_idx == active_ws && tab_idx == view.state.navigation.tab_index(pane);
                    // native WebView가 가리면 안 되는 host 입력 영역: pane·surface 분할선 hit 띠와
                    // 창 가장자리 리사이즈 밴드. 활성 탭에서만 쓴다.
                    let input_zones = if is_visible {
                        let mut dividers = pane_dividers.clone();
                        dividers.extend(view.state.surface_dividers(
                            engine,
                            tab,
                            content_rect,
                            scale_factor as f32,
                        ));
                        crate::state::webview_edges::host_input_zones(
                            &dividers,
                            crate::model::PhysicalPx(window.width as f32),
                            crate::model::PhysicalPx(window.height as f32),
                            resize_band,
                            scale_factor as f32,
                        )
                    } else {
                        Vec::new()
                    };
                    // 비포커스 leaf에도 native WebView가 필요하므로 탭 전체를 순회한다.
                    for region in view.state.tab_surface_regions(
                        engine,
                        tab,
                        content_rect,
                        scale_factor as f32,
                    ) {
                        let sid = region.id;
                        let leaf_rect = region.rect;
                        let Some(surface) = engine.find_surface_by_id(region.id) else {
                            continue;
                        };
                        if !hosts_webview(view, surface, sid) {
                            continue;
                        }

                        if is_visible {
                            // 패널 바깥쪽 변에는 divider 드래그 영역만큼 여백을 둔다.
                            // egui chrome도 같은 여백 안쪽으로 내용을 자른다.
                            let [left, right, bottom] =
                                crate::state::webview_edges::webview_edge_inset(
                                    leaf_rect,
                                    content_rect,
                                    scale_factor as f32,
                                )
                                .map(|v| v.value() as f64);
                            // 물리 사각형을 만든 뒤 플랫폼 API에 맞는 논리 좌표로 변환한다.
                            let top = view.html_content_top(sid, scale_factor);
                            let physical = crate::webview::PhysicalWebViewBounds {
                                x: leaf_rect.x.value() as f64 + left,
                                y: leaf_rect.y.value() as f64 + top,
                                width: (leaf_rect.width.value() as f64 - left - right).max(1.0),
                                height: (leaf_rect.height.value() as f64 - bottom - top).max(1.0),
                            };
                            let bounds = crate::webview::WebViewBounds::from_physical(
                                physical,
                                scale_factor,
                            );
                            active_html.insert(sid, bounds);
                            let webview_rect = crate::model::PhysicalRect {
                                x: crate::model::PhysicalPx(physical.x as f32),
                                y: crate::model::PhysicalPx(physical.y as f32),
                                width: crate::model::PhysicalPx(physical.width as f32),
                                height: crate::model::PhysicalPx(physical.height as f32),
                            };
                            // native 창은 물리 px 를 반올림한 정수 사각형에 놓인다(`set_bounds`).
                            // 구멍도 같은 정수 사각형 기준으로 잘라 창 좌표와 한 픽셀도 어긋나지 않게 한다.
                            let px = |v: f64| crate::model::PhysicalPx(v.round() as f32);
                            let window_rect = crate::model::PhysicalRect {
                                x: px(physical.x),
                                y: px(physical.y),
                                width: px(physical.width),
                                height: px(physical.height),
                            };
                            let holes = if page_input_yields_to_band(view, sid) {
                                vec![crate::model::PhysicalRect {
                                    x: crate::model::PhysicalPx(0.0),
                                    y: crate::model::PhysicalPx(0.0),
                                    width: window_rect.width,
                                    height: window_rect.height,
                                }]
                            } else {
                                crate::state::webview_edges::webview_input_holes(
                                    window_rect,
                                    &input_zones,
                                )
                            };
                            active_holes.insert(sid, holes);
                            active_clips.insert(
                                sid,
                                crate::state::webview_edges::notice_clip_holes(
                                    window_rect,
                                    &notice_cards,
                                ),
                            );
                            active_physical.insert(sid, webview_rect);
                        }
                    }
                }
            }
        }
    }

    HtmlSurfaces {
        bounds: active_html,
        physical: active_physical,
        holes: active_holes,
        clips: active_clips,
        all_ids: all_html_ids,
    }
}

/// 필요한 설정을 읽은 뒤 HTML surface의 native WebView를 만들고 페이지를 연다.
/// 허용 직후의 재로드, 사용자 탐색, 에이전트 히스토리 이동, debug 로드 중지를 native webview에 전달한다.
fn apply_webview_requests(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
) {
    for (sid, wv) in &view.webviews {
        let reload = find_remote_surface(view, engine, *sid)
            .is_some_and(|rs| rs.with_html_script(|st| st.take_reload_request()));
        if reload {
            tracing::debug!("WebView surface {sid}: reloading after the script allowance");
            #[cfg(any(target_os = "linux", target_os = "macos", windows))]
            wv.reload_after_script_allow();
            #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
            wv.reload();
        }
    }
    apply_navigation_queues(view, engine);
    #[cfg(debug_assertions)]
    for sid in std::mem::take(&mut view.state.debug_webview_stop) {
        match view.webviews.get(&sid) {
            Some(wv) => wv.debug_stop(),
            None => tracing::warn!("debug webview stop: surface {sid} has no webview"),
        }
    }
}

/// 사용자 탐색 큐를 먼저, 에이전트 히스토리 큐를 다음에 비운다. 각 큐 안에서는 들어온 순서다.
fn apply_navigation_queues(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
) {
    for (sid, nav) in std::mem::take(&mut view.state.webview_user_nav) {
        apply_user_nav(view, engine, sid, nav);
    }
    for (sid, step) in std::mem::take(&mut view.state.webview_agent_nav) {
        apply_agent_history_step(view, engine, sid, step);
    }
}

/// 사용자 탐색 한 건에 할 일.
#[derive(Debug, PartialEq, Eq)]
enum UserNavPlan {
    /// 로드가 일어나지 않을 조작이다.
    Skip,
    /// 탐색한다. 같은 문서 fragment 이동은 commit·로드 종료 신호가 없어 표지를 내릴 수 없으므로
    /// 사용자 출처 표지를 세우지 않는다.
    Navigate { mark_user_origin: bool },
}

fn plan_user_nav(
    nav: &crate::webview::WebViewUserNav,
    info: &crate::model::WebViewNavInfo,
) -> UserNavPlan {
    use crate::webview::{WebViewHistoryStep, WebViewUserNav};
    let possible = match nav {
        WebViewUserNav::Back => WebViewHistoryStep::Back.loads(info),
        WebViewUserNav::Forward => WebViewHistoryStep::Forward.loads(info),
        WebViewUserNav::Reload => WebViewHistoryStep::Reload.loads(info),
        WebViewUserNav::Open(url) => crate::webview::is_navigable_url(url),
    };
    if !possible {
        return UserNavPlan::Skip;
    }
    // 뒤로·앞으로가 같은 문서의 fragment 항목으로 가는 경우는 미리 알 수 없어 표지를 세운다(남은 한계).
    let mark_user_origin = match nav {
        WebViewUserNav::Open(url) => {
            !crate::model::is_same_document_fragment_move(info.url.as_deref(), url)
        }
        _ => true,
    };
    UserNavPlan::Navigate { mark_user_origin }
}

/// 호스트가 넣은 주소가 새 로드인지. 같은 문서 fragment 이동이면 호스트 로드 표지를 세우지 않는다.
fn host_load_marks_origin(current: Option<&str>, url: &str) -> bool {
    !crate::model::is_same_document_fragment_move(current, url)
}

/// 에이전트 히스토리 이동 한 건에 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentStepPlan {
    /// 로드가 일어나지 않을 이동이다(히스토리 처음의 뒤로, 끝의 앞으로, 문서 없는 새로고침).
    Skip,
    /// 탐색하고 호스트 출처 표지를 세운다.
    Navigate,
}

/// 사용자 탐색의 뒤로·앞으로·새로고침과 같은 기준으로 정한다. 같은 문서 fragment 히스토리 항목으로
/// 가는지는 미리 알 수 없어 그 경우도 탐색으로 보고 표지를 세운다(남은 한계).
fn plan_agent_history_step(
    step: crate::webview::WebViewHistoryStep,
    info: &crate::model::WebViewNavInfo,
) -> AgentStepPlan {
    if step.loads(info) {
        AgentStepPlan::Navigate
    } else {
        AgentStepPlan::Skip
    }
}

/// 계획대로 출처 표지를 세운다. 에이전트 이동은 호스트 출처라 다음 문서는 이전 문서의 열람을
/// 이어받지 않는다. 버리는 이동은 표지를 남기지 않는다.
fn mark_agent_step(plan: AgentStepPlan, st: &mut tasty_model::html_script::HtmlScriptState) {
    if plan == AgentStepPlan::Navigate {
        st.on_host_load_requested();
    }
}

/// 생성 실패 횟수를 갱신하고 첫 실패와 재시도 중단 시에만 경고한다.
/// 영구 실패는 즉시 상한으로, 일시 실패는 한 번씩 올린다.
fn record_webview_failure(
    view: &mut MainView,
    sid: u32,
    attempts: u32,
    err: &crate::webview::WebViewCreateError,
) {
    let next = next_webview_attempts(attempts, err);
    view.webview_create_attempts.insert(sid, next);
    if attempts == 0 {
        tracing::warn!("Failed to create WebView for surface {sid}: {err}");
    }
    if next >= MAX_WEBVIEW_CREATE_ATTEMPTS {
        // 영구 실패는 한 번에 상한이 되므로 로그에는 실제 시도 횟수를 쓴다.
        let made = attempts + 1;
        tracing::warn!(
            concat!(
                "Giving up on the WebView for surface {} after {} attempt(s): ",
                "{} — 이 surface 는 다시 열 때까지 비어 있다"
            ),
            sid,
            made,
            err
        );
    }
}

/// URL scheme이 있으면 페이지를 열고, 없으면 HTML 본문으로 로드한다.
fn load_initial_url(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
    wv: &crate::webview::PlatformWebView,
    url: Option<&String>,
) {
    let Some(url) = url else {
        // 주소 띠로 문서를 여는 빈 html 탭이다(`hosts_webview`). 첫 로드는 같은 redraw의
        // 사용자 탐색 큐가 한다.
        tracing::debug!("WebView surface {sid}: created without a host URL for a user navigation");
        return;
    };
    note_host_webview_load(view, engine, sid);
    if find_remote_surface(view, engine, sid).is_some_and(|rs| rs.html_mirror.is_some())
        || crate::webview::is_navigable_url(url)
    {
        wv.load_url(url);
    } else {
        wv.load_html(url);
    }
    view.webview_loaded_urls.insert(sid, url.clone());
}

/// 페이지가 계속 숨겨져 있으면 surface별로 한 번 경고한다.
/// 상태 변경이나 재로드는 하지 않는다.
fn note_reveal_pending(view: &mut MainView, pending: &[(u32, crate::webview::NavState)]) {
    let now = std::time::Instant::now();
    for &(sid, nav) in pending {
        let entry = view
            .webview_reveal_pending
            .entry(sid)
            .or_insert((now, false));
        if !entry.1 && now.duration_since(entry.0) >= REVEAL_PENDING_WARN_AFTER {
            entry.1 = true;
            tracing::warn!(
                "WebView surface {sid}: still hidden {:?} after it became active \
                 (nav_state={nav:?}); the pane shows host chrome, not the page",
                REVEAL_PENDING_WARN_AFTER
            );
        }
    }
    // 드러났거나 사라진 surface 는 추적에서 뺀다 — 다시 보류되면 시계가 새로 시작한다.
    view.webview_reveal_pending
        .retain(|sid, _| pending.iter().any(|(p, _)| p == sid));
}

/// web content process가 끝난 WebView 중 호스트가 HTML 본문을 실은 페이지는 다음 동기화에서
/// 같은 본문을 다시 싣게 한다. 엔진은 다음 로드에서 process를 새로 띄운다. 이런 페이지는
/// 새로고침 수단이 문서 안에 있고, 플러그인이 같은 본문을 다시 보내도 로드를 건너뛰므로 스스로
/// 복구할 방법이 없다. 주소로 연 페이지는 사용자가 이동한 위치를 덮지 않도록 그대로 둔다.
fn forget_ended_host_documents(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
) {
    let ended: Vec<u32> = view
        .webviews
        .iter()
        .filter(|(_, wv)| wv.take_content_process_ended())
        .map(|(sid, _)| *sid)
        .filter(|sid| {
            view.webview_loaded_urls
                .get(sid)
                .is_some_and(|url| !crate::webview::is_navigable_url(url))
                && find_remote_surface(view, engine, *sid).is_none_or(|rs| rs.html_mirror.is_none())
        })
        .collect();
    for sid in ended {
        tracing::info!("WebView surface {sid}: reloading the document after its process ended");
        view.webview_loaded_urls.remove(&sid);
    }
}

fn resync_webview_urls(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    all_html_ids: &[u32],
) {
    forget_ended_host_documents(view, engine);
    for &sid in all_html_ids {
        let Some(url) = find_webview_url(view, engine, sid) else {
            continue;
        };
        if view.webview_loaded_urls.get(&sid) == Some(&url) {
            continue;
        }
        if let Some(wv) = view.webviews.get(&sid) {
            if host_load_marks_origin(wv.nav_info().url.as_deref(), &url) {
                note_host_webview_load(view, engine, sid);
            }
            if find_remote_surface(view, engine, sid).is_some_and(|rs| rs.html_mirror.is_some())
                || crate::webview::is_navigable_url(&url)
            {
                wv.load_url(&url);
            } else {
                wv.load_html(&url);
            }
            view.webview_loaded_urls.insert(sid, url);
        }
    }
}

/// plugin 이 `webview.post_message` 로 쌓은 메시지 스크립트를 실린 문서에서 돌린다.
/// URL 재동기화 뒤에 돌아 같은 프레임에 문서를 바꿨으면 새 문서가 받는다.
/// WebView 가 아직 없으면 큐를 남겨 다음 프레임에 돌린다. 단 처음 보일 때 만드는 kind 는 버린다
/// ([`lazy::scripts_without_webview`]).
fn drain_pending_scripts(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    all_html_ids: &[u32],
) {
    for &sid in all_html_ids {
        let Some(rs) = find_remote_surface(view, engine, sid) else {
            continue;
        };
        let Some(wv) = view.webviews.get(&sid) else {
            let lazy = crate::runtime::surface_registry::webview_kind::is_lazy_webview_kind(
                rs.kind_static,
            );
            if scripts_without_webview(lazy) == ScriptsWithoutWebview::Drop {
                let dropped = rs.take_scripts().len();
                if dropped > 0 {
                    tracing::debug!(
                        "WebView surface {sid}: dropped {dropped} message(s) before its lazy webview exists"
                    );
                }
            }
            continue;
        };
        for script in rs.take_scripts() {
            wv.eval_js(&script);
        }
    }
}

/// 드러난 WebView 영역을 기록한다. 탭 스트립 툴팁은 다음 프레임에 이 영역을 피해 배치한다.
/// HashMap 순회 순서가 바뀌어도 같은 값으로 보도록 surface id 순으로 정렬한다.
fn publish_native_content_rects(
    view: &mut MainView,
    active: &std::collections::HashMap<u32, crate::model::PhysicalRect>,
    mut revealed: Vec<u32>,
) {
    revealed.sort_unstable();
    let rects: Vec<_> = revealed
        .iter()
        .filter_map(|sid| active.get(sid).map(|r| (*sid, *r)))
        .collect();
    let current = &view.state.native_content_rects;
    let unchanged = current.len() == rects.len()
        && current
            .iter()
            .zip(&rects)
            .all(|(a, b)| a.0 == b.0 && a.1.approx_eq(&b.1));
    if !unchanged {
        view.state.native_content_rects = rects;
        view.mark_dirty();
    }
}

/// native WebView에서 구멍을 뺀다. `input`은 host 입력 영역(분할선 hit 띠·창 리사이즈 밴드)으로
/// 포인터만 빼고, `clip`은 복원 안내 카드로 그림과 포인터를 함께 뺀다. bounds·표시·포커스는
/// 바꾸지 않는다. 둘 다 비면 원래 영역으로 돌아간다. `input`을 빼는 방법은 Linux에만 있고 다른
/// OS는 `webview_edge_inset` 여백으로 피한다.
/// 주소 띠가 키보드 포커스를 가진 동안 페이지 창 전체를 입력에서 뺄지. X 는 포커스 창의 자손 위에
/// 포인터가 있으면 키를 그 자손에 보내므로, Linux 에서는 포인터가 페이지 위에 있어도 키가 호스트
/// 창으로 오도록 띠 편집 동안 페이지가 포인터를 받지 않게 한다. 편집이 끝나면 다음 동기화에서 되돌린다.
/// macOS·Windows 는 키를 키보드 포커스 창에만 보내므로 빼지 않는다.
#[cfg(target_os = "linux")]
fn page_input_yields_to_band(view: &MainView, sid: u32) -> bool {
    view.state.html_address.has_keyboard_focus_on(sid)
}

#[cfg(not(target_os = "linux"))]
fn page_input_yields_to_band(_view: &MainView, _sid: u32) -> bool {
    false
}

fn apply_native_holes(
    wv: &crate::webview::PlatformWebView,
    input: &[crate::model::PhysicalRect],
    clip: &[crate::model::PhysicalRect],
    scale_factor: f64,
) {
    // 구멍은 이미 정수 픽셀 경계다(`webview_edges`). 반올림은 부동소수 오차만 없앤다.
    let ints = |rects: &[crate::model::PhysicalRect]| -> Vec<[i32; 4]> {
        rects
            .iter()
            .map(|r| [r.x, r.y, r.width, r.height].map(|v| v.value().round() as i32))
            .collect()
    };
    wv.set_native_holes(&ints(input), &ints(clip), scale_factor);
}

/// Synchronize native WebView instances with the current state.
/// 살아있는 webview 마다 현재 설정을 해석해, 마지막 적용값과 다를 때만 backend 에 재적용한다
/// (변경 없으면 backend 호출 0 — 매 프레임 호출 회피).
fn reapply_changed_settings(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
) {
    let live_sids: Vec<u32> = view.webviews.keys().copied().collect();
    for sid in live_sids {
        let resolved = resolve_webview_settings(view, engine, sid);
        if view.webview_applied_settings.get(&sid) == Some(&resolved) {
            continue;
        }
        if let Some(wv) = view.webviews.get(&sid) {
            tracing::debug!("WebView surface {sid}: requesting settings {resolved:?}");
            resolved.apply(wv);
        }
        view.webview_applied_settings.insert(sid, resolved);
    }
}

/// WebView와 그 surface의 적재·설정·생성 기록을 지운다. 다음 생성은 처음 만드는 것과 같다.
fn forget_webview(
    view: &mut MainView,
    surface: u32,
    proofs: &super::html_runtime::NavigationProofs,
) {
    view.webviews.remove(&surface);
    view.webview_runtime.remove(&surface);
    view.webview_loaded_urls.remove(&surface);
    view.webview_applied_settings.remove(&surface);
    view.webview_create_attempts.remove(&surface);
    proofs.invalidate(&view.state.webview_identity, surface);
}

/// Creates webviews for new Html panels, destroys removed ones,
/// updates bounds and visibility based on active workspace/tab.
fn sync_webviews(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    plugin_manager: Option<&PluginManager>,
    proofs: &super::html_runtime::NavigationProofs,
) {
    let scale_factor = view.base.gpu.scale_factor() as f64;
    let HtmlSurfaces {
        bounds: active_html,
        physical: active_physical,
        holes: active_holes,
        clips: active_clips,
        all_ids: all_html_ids,
    } = collect_html_surfaces(view, engine, scale_factor);
    lazy::release_hidden_webviews(view, engine, &active_html, proofs);
    create_missing_webviews(view, engine, &all_html_ids, &active_html, scale_factor);
    // 아래 표시 판정보다 먼저 둔다. 이 프레임에 드러나는 WebView도 새 상태로 드러난다.
    bed_focus::sync(view, engine);
    resync_webview_urls(view, engine, &all_html_ids);
    drain_pending_scripts(view, engine, &all_html_ids);
    update_html_script_banners(view, engine, &all_html_ids);

    // When any egui overlay (context menu, popup, dialog) is open,
    // hide all WebViews so they don't cover the overlay.
    // Native views are always above the wgpu render surface in OS z-order.
    let overlay_open = view.state.has_egui_overlay_open();
    // native 편집 키는 host 큐보다 먼저 실행되므로 키보드를 가져가는 overlay·전체화면 무대 상태를 bridge에 알린다.
    view.webview_key_bridge.set_overlay_open(
        view.state.keyboard_overlay_open() || view.state.fullscreen_stage_active(),
    );

    // 호스트 키 설정 또는 plugin 명령·override의 revision이 바뀔 때만
    // native 콜백이 사용할 단축키 스냅샷을 다시 만든다.
    let plugin_epoch =
        plugin_manager.map(|m| (m.command_registry.revision(), m.config.shortcut_revision()));
    let categories_enabled = engine.runtime.settings.general.workspace_categories_enabled;
    if view.webview_policy_src.as_ref().map(|(kb, cat)| (kb, *cat))
        != Some((&engine.runtime.settings.keybindings, categories_enabled))
        || view.webview_policy_plugin_epoch != plugin_epoch
    {
        let kb = &engine.runtime.settings.keybindings;
        let plugin_combos = plugin_manager
            .map(|m| crate::plugin_bridge::key_dispatch::webview_command_bindings(m, kb))
            .unwrap_or_default();
        view.webview_key_bridge.set_policy(
            crate::adapters::ui::input::shortcuts::webview_shortcut_policy(
                kb,
                plugin_combos,
                categories_enabled,
            ),
        );
        view.webview_policy_src = Some((kb.clone(), categories_enabled));
        view.webview_policy_plugin_epoch = plugin_epoch;
    }
    // plugin surface에서는 그 plugin의 명령만 실행되므로 bridge가 surface별로 실행 범위를 판정한다.
    view.webview_key_bridge.set_surface_owners(
        view.webviews
            .keys()
            .filter_map(|&sid| {
                let surface = engine.find_surface_by_id(sid)?;
                let remote = surface
                    .as_any()
                    .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()?;
                Some((sid, remote.plugin_id.clone()))
            })
            .collect(),
    );
    // 주소창 포커스 단축키는 주소 띠가 있는 html 페이지에서만 가져가고 다른 WebView 페이지에는 남긴다.
    // mirror 주소 띠는 읽기 전용이므로 편집 단축키 대상에서 제외한다.
    view.webview_key_bridge.set_address_surfaces(
        view.webviews
            .keys()
            .copied()
            .filter(|&sid| {
                engine
                    .find_surface_by_id(sid)
                    .is_some_and(|s| s.kind() == "html")
                    && !engine.is_mirror_surface(sid)
            })
            .collect(),
    );

    // overlay를 열면 WebView 키보드 포커스를 호스트 창으로 돌린다. 숨기기만으로는
    // 포커스가 해제되지 않는다. 닫을 때 자동으로 native 자식에 포커스를 돌려주지는 않는다.
    // 다른 앱의 포커스를 바꾸지 않도록 이 창이 OS 포커스를 가질 때만 처리한다.
    if overlay_open {
        if !view.webview_overlay_focus_released
            && host_window_has_os_focus(view.base.state.focused, || {
                view.webviews.values().any(|wv| wv.holds_keyboard_focus())
            })
        {
            for wv in view.webviews.values() {
                wv.release_keyboard_focus();
            }
            view.webview_overlay_focus_released = true;
        }
    } else {
        view.webview_overlay_focus_released = false;
    }

    // 주소 띠가 키보드 포커스를 새로 얻으면 그 surface의 페이지에서 포커스를 회수해 키가 페이지로
    // 가지 않게 한다. 띠가 포커스를 가진 동안에만 OS 포커스를 조회한다.
    let webviews = &view.webviews;
    if let Some(sid) = view.state.html_address.sync_page_focus(|sid| {
        webviews
            .get(&sid)
            .is_some_and(|wv| wv.holds_keyboard_focus())
    }) && let Some(wv) = view.webviews.get(&sid)
    {
        wv.release_keyboard_focus();
    }

    // 탭·workspace 전환으로 활성 탭에서 빠진 webview도 숨기기 전에 포커스를 회수한다.
    // Linux·Windows의 webview는 별도 OS 창이라 숨겨도 포커스가 부모로 돌아오지 않을 수 있다.
    // 숨긴 뒤에는 backend가 포커스가 이미 밖에 있다고 판정할 수 있어 순서가 중요하다.
    let now_active: std::collections::HashSet<u32> = active_html.keys().copied().collect();
    let release = take_hidden_webview_focus_targets(
        &view.webview_prev_active,
        &now_active,
        |sid| {
            webview_release_candidate_is_live(sid, &all_html_ids, |s| {
                view.webviews.contains_key(&s)
            })
        },
        &mut view.webview_focus_release_pending,
        || {
            host_window_has_os_focus(view.base.state.focused, || {
                view.webviews.values().any(|wv| wv.holds_keyboard_focus())
            })
        },
    );
    for sid in release {
        if let Some(wv) = view.webviews.get(&sid) {
            wv.release_keyboard_focus();
        }
    }
    view.webview_prev_active = now_active;

    // 대상 팝업이 떠 있으면 backend 가 낼 수 있는 모드를 정하고 적용한다. 지원하지 않거나 실패하면
    // `Hide` 이고 아래 표시 판정이 지금처럼 모든 WebView 를 숨긴다.
    let blocked = view.state.fullscreen_stage_active() || view.state.tutorial.active.is_some();
    let overlay_views: Vec<(
        &crate::webview::PlatformWebView,
        Option<crate::model::PhysicalRect>,
    )> = view
        .webviews
        .iter()
        .map(|(sid, wv)| (wv, active_physical.get(sid).copied()))
        .collect();
    let overlay_mode = settle_overlay(
        &mut view.state.webview_overlay,
        overlay_open,
        blocked,
        &overlay_views,
        scale_factor,
        || view.base.gpu.overlay_surface_ceiling(),
    );
    drop(overlay_views);

    // 토스트 카드와 passive popup은 egui가 그리므로 native 페이지가 덮는다. 겹치는 WebView만
    // 떠 있는 동안 숨긴다. 키보드 포커스를 가진 WebView는 숨기지 않는다.
    // 숨기면 사용자가 치던 키가 페이지에도 터미널에도 가지 않고 사라진다.
    let passive_popup_rects = view
        .state
        .popups
        .visible_passive_rects()
        .into_iter()
        .map(|r| crate::model::LogicalRect {
            x: crate::model::LogicalPx(r.min.x),
            y: crate::model::LogicalPx(r.min.y),
            width: crate::model::LogicalPx(r.width()),
            height: crate::model::LogicalPx(r.height()),
        });
    let cover_rects: Vec<crate::model::PhysicalRect> = view
        .state
        .toasts
        .card_rects()
        .iter()
        .copied()
        .chain(passive_popup_rects)
        .map(|r| r.to_physical(scale_factor as f32))
        .collect();

    // Native completion alone cannot end remote resource consent/receiving. Keep the existing
    // host page state visible until both navigation and the received document are ready.
    let mut planned = Vec::new();
    let mut reveal_pending: Vec<(u32, crate::webview::NavState)> = Vec::new();
    for (sid, wv) in &view.webviews {
        // active 면 bounds 는 숨겨져 있어도 갱신(다음 reveal 대비).
        if let Some(bounds) = active_html.get(sid) {
            wv.set_bounds(*bounds, scale_factor);
            // 오른쪽 위 안내와 겹친 부분만 그림·포인터에서 뺀다. 숨기거나 포커스를 옮기지 않는다.
            apply_native_holes(
                wv,
                active_holes.get(sid).map_or(&[][..], Vec::as_slice),
                active_clips.get(sid).map_or(&[][..], Vec::as_slice),
                scale_factor,
            );
        }
        // 스캔을 기다리며 미룬 탐색 결정을 끝낸다. 읽기가 끝나면 작업 스레드가 이 윈도우를 깨운다.
        wv.resume_script_decision();
        let nav = wv.nav_state();
        // 드러나야 할 자리에 있는데(활성 tab · overlay 없음) nav 가 Done 이 아니면
        // 그 프레임의 그 surface 는 "만들어졌지만 안 보이는" 상태다.
        let under_toast = hidden_under_toast(active_physical.get(sid), &cover_rects, || {
            wv.receives_keyboard_input()
        });
        let wants_reveal = crate::state::webview_overlay::reveal_wanted(
            overlay_mode,
            overlay_open,
            active_html.contains_key(sid),
            under_toast,
        );
        let receiving = engine
            .read()
            .html_receive(*sid)
            .is_some_and(|snapshot| snapshot.connected && snapshot.receiving);
        let reveal = wants_reveal && nav == crate::webview::NavState::Done && !receiving;
        if wants_reveal && !reveal && !receiving {
            reveal_pending.push((*sid, nav));
        }
        planned.extend(reveal.then_some(*sid));
    }
    // 대상 팝업 아래 페이지는 오버레이 창에 같은 모드·페이지의 그림이 올라간 프레임에만 드러난다.
    let revealed = apply_reveal(
        view,
        overlay_mode,
        overlay_open,
        &active_physical,
        planned,
        scale_factor,
    );
    let any_visible = !revealed.is_empty();
    publish_native_content_rects(view, &active_physical, revealed);
    note_reveal_pending(view, &reveal_pending);
    // 키 폴링 tick 의 게이트(`app::webview_keys`) — 드러난 webview 가 없으면
    // 키가 그리로 갈 수 없으므로 폴링을 세우지 않는다.
    view.webview_any_visible = any_visible;

    // Remove webviews for closed Html surfaces
    view.webviews.retain(|sid, _| all_html_ids.contains(sid));
    view.webview_runtime
        .retain(|sid, _| all_html_ids.contains(sid));
    view.webview_applied_settings
        .retain(|sid, _| all_html_ids.contains(sid));
    view.webview_bed_focus
        .retain(|sid, _| all_html_ids.contains(sid));
    view.webview_loaded_urls
        .retain(|sid, _| all_html_ids.contains(sid));
    // 시도 횟수도 같이 지운다 — surface 를 닫았다 열면 예산이 새로 생긴다.
    view.webview_create_attempts
        .retain(|sid, _| all_html_ids.contains(sid));

    reapply_changed_settings(view, engine);
    apply_webview_requests(view, engine);

    // native nav_state 를 RemoteSurface 로 mirror — egui 렌더 경로(egui_panels →
    // webview_chrome)가 다음 프레임에 읽어 loading/error chrome 을 그린다. 주소와 히스토리
    // 이동 가능 여부도 함께 옮긴다. borrow 충돌 회피를 위해 먼저 수집한 뒤 기록한다. 전이가
    // 있으면 mark_dirty 로 한 프레임 더 그려 chrome 을 갱신한다(가시성 전환 자체는 위에서 native 즉시 적용).
    let navs: Vec<(u32, crate::webview::NavState, crate::model::WebViewNavInfo)> = view
        .webviews
        .iter()
        .map(|(s, w)| (*s, w.nav_state(), w.nav_info()))
        .collect();
    let mut nav_changed = false;
    for (sid, nav, info) in navs {
        let Some(rs) = find_remote_surface(view, engine, sid) else {
            continue;
        };
        #[cfg(any(target_os = "linux", target_os = "macos", windows))]
        let nav = mirrored_nav_state(
            nav,
            rs.http_session
                .as_ref()
                .is_some_and(|session| session.failed()),
        );
        if rs.nav_state() != nav {
            rs.set_nav_state(nav);
            nav_changed = true;
        }
        nav_changed |= rs.set_webview_nav(info);
    }
    if nav_changed {
        view.mark_dirty();
    }

    // navigation 시도를 소유 plugin에 알린다. 원격 콘텐츠 차단 여부와 별개이며
    // plugin이 없어도 큐를 비운다. 사용자 제스처와 페이지 작성자가 확인된 경우에만
    // 사용자 탐색 기록을 남긴다. plugin 응답보다 먼저 기록해야 한다(docs/features/file-handler/index.md).
    let identity = view.state.webview_identity.clone();
    for (sid, wv) in &view.webviews {
        for nav in wv.take_pending_navigations() {
            if let Some(manager) = plugin_manager {
                let remote = find_remote_surface(view, engine, *sid);
                let owner =
                    remote.map(
                        |remote| crate::plugin_bridge::user_navigation::NavigationOwner {
                            plugin_id: remote.plugin_id.clone(),
                            wrote_page: remote.webview_page_by_owner(),
                        },
                    );
                proofs.record(
                    &identity,
                    *sid,
                    owner.as_ref(),
                    remote.map(|remote| &remote.webview_url),
                    &nav,
                );
                if let Some(owner) = owner {
                    manager.send_webview_navigation_attempt(
                        &owner.plugin_id,
                        &tasty_plugin_protocol::WebviewNavigationAttemptParams {
                            surface_id: *sid,
                            url: nav.url,
                        },
                    );
                }
            }
        }
    }
    let live: Vec<_> = view.webviews.keys().copied().collect();
    let takeovers: Vec<_> = live
        .iter()
        .map(|&sid| {
            (
                sid,
                find_remote_surface(view, engine, sid)
                    .is_some_and(|remote| remote.take_webview_owner_takeover()),
            )
        })
        .collect();
    proofs.settle_frame(&identity, &live, &takeovers);
}

/// surface_id 로 surface 를 전 workspace 에서 찾는다. 탭당 1 개만 보는
/// `Tab::surface()`(포커스 leaf) 가 아니라 각 탭의 `SurfaceLayout` 트리 전체를
/// 훑으므로, 탭 내부 분할(SurfaceGroup)의 비포커스 leaf 도 도달한다.
fn find_surface_anywhere<'e>(
    _view: &MainView,
    engine: &EngineRef<'e>,
    surface_id: u32,
) -> Option<&'e dyn crate::model::Surface> {
    engine.find_surface_by_id(surface_id)
}

/// surface_id 로 RemoteSurface 를 찾아 반환. nav_state mirror 기록에 쓴다.
fn find_remote_surface<'e>(
    view: &MainView,
    engine: &EngineRef<'e>,
    surface_id: u32,
) -> Option<&'e crate::plugin_bridge::remote_surface::RemoteSurface> {
    find_surface_anywhere(view, engine, surface_id)?
        .as_any()
        .downcast_ref::<crate::plugin_bridge::remote_surface::RemoteSurface>()
}

/// webview surface 의 kind.
fn webview_surface_kind(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    surface_id: u32,
) -> Option<&'static str> {
    find_surface_anywhere(view, engine, surface_id).map(|s| s.kind())
}

/// surface kind를 등록한 plugin의 WebView 설정을 읽는다. 기본값은 그 plugin의 선언을 따른다.
/// html의 문서 단위 스크립트 허용은 이 값을 전역 sandbox로 받아 backend 게이트가 적용한다(docs/surfaces/html/index.md).
fn resolve_webview_settings(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    surface_id: u32,
) -> crate::webview::HtmlWebViewSettings {
    let source = webview_surface_kind(view, engine, surface_id)
        .and_then(crate::runtime::surface_registry::webview_kind::webview_settings_source);
    crate::webview::HtmlWebViewSettings::resolve(source.as_ref(), &engine.runtime.settings)
}

/// Find the URL for an Html panel by surface ID.
fn find_webview_url(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    surface_id: u32,
) -> Option<String> {
    find_surface_anywhere(view, engine, surface_id)?
        .webview_url()
        .map(|u| u.to_string())
}

fn note_host_webview_load(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
) {
    if let Some(rs) = find_remote_surface(view, engine, sid) {
        rs.with_html_script(|st| {
            if rs.html_mirror.is_none() || !st.loading_before_commit() {
                st.on_host_load_requested();
            }
        });
    }
}

fn update_html_script_banners(
    view: &mut MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    all_html_ids: &[u32],
) {
    note_user_selection(view, engine);
    let mut phases = std::collections::HashMap::new();
    for &sid in all_html_ids {
        let Some(phase) = update_html_script_banner(view, engine, sid) else {
            continue;
        };
        if phase != BannerPhase::Hidden {
            phases.insert(sid, phase);
        }
    }
    if phases != view.html_script_phases {
        view.base.state.dirty = true;
    }
    view.html_script_phases = phases;
}

fn note_user_selection(view: &mut MainView, engine: &crate::runtime::engine_access::EngineRef<'_>) {
    let focused = view.state.focused_surface_id(engine);
    let selected = match view.html_script_seen_focus.replace(focused) {
        Some(prev) if prev != focused => focused,
        _ => None,
    };
    if let Some(sid) = selected
        && let Some(rs) = find_remote_surface(view, engine, sid)
        && rs.kind_static == "html"
    {
        tracing::debug!("html script banner: surface {sid} selected by the user");
        rs.with_html_script(|st| st.on_user_view());
    }
}

fn update_html_script_banner(
    view: &MainView,
    engine: &crate::runtime::engine_access::EngineRef<'_>,
    sid: u32,
) -> Option<BannerPhase> {
    let rs = find_remote_surface(view, engine, sid).filter(|rs| rs.kind_static == "html")?;
    let phase = rs.with_html_script(|st| st.update_banner());
    let before = view
        .html_script_phases
        .get(&sid)
        .copied()
        .unwrap_or(BannerPhase::Hidden);
    if phase != before {
        tracing::debug!(
            "html script banner: surface {sid} {} -> {}",
            before.as_str(),
            phase.as_str()
        );
    }
    Some(phase)
}

/// 토스트 카드나 passive popup 때문에 이 WebView를 숨길지. 겹치고 키보드 포커스가 없을 때만 참이다.
/// 포커스 조회는 OS 호출이므로 겹칠 때만 한다.
fn hidden_under_toast(
    webview: Option<&crate::model::PhysicalRect>,
    cards: &[crate::model::PhysicalRect],
    receives_keyboard_input: impl FnOnce() -> bool,
) -> bool {
    webview.is_some_and(|rect| covered_by_any(rect, cards)) && !receives_keyboard_input()
}

/// WebView 사각형이 토스트 카드 하나와라도 면적을 공유하면 참이다. 변이 맞닿기만 하면 거짓이다.
fn covered_by_any(
    webview: &crate::model::PhysicalRect,
    cards: &[crate::model::PhysicalRect],
) -> bool {
    cards.iter().any(|card| {
        card.x < webview.x + webview.width
            && webview.x < card.x + card.width
            && card.y < webview.y + webview.height
            && webview.y < card.y + card.height
    })
}

#[cfg(test)]
mod toast_cover_tests {
    use super::{covered_by_any, hidden_under_toast};
    use crate::model::{PhysicalPx, PhysicalRect};
    use crate::state::webview_overlay::OverlayMode;

    fn reveal_wanted(overlay_open: bool, active: bool, under_toast: bool) -> bool {
        crate::state::webview_overlay::reveal_wanted(
            OverlayMode::Hide,
            overlay_open,
            active,
            under_toast,
        )
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> PhysicalRect {
        PhysicalRect {
            x: PhysicalPx(x),
            y: PhysicalPx(y),
            width: PhysicalPx(w),
            height: PhysicalPx(h),
        }
    }

    #[test]
    fn card_inside_webview_covers_it() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        assert!(covered_by_any(
            &webview,
            &[rect(1000.0, 640.0, 260.0, 40.0)]
        ));
    }

    #[test]
    fn card_in_another_pane_leaves_webview_visible() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        assert!(!covered_by_any(
            &webview,
            &[rect(300.0, 640.0, 200.0, 40.0)]
        ));
    }

    #[test]
    fn touching_edge_is_not_overlap() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        assert!(!covered_by_any(
            &webview,
            &[rect(400.0, 640.0, 200.0, 40.0)]
        ));
        assert!(!covered_by_any(
            &webview,
            &[rect(700.0, 700.0, 200.0, 40.0)]
        ));
    }

    #[test]
    fn touching_the_other_two_edges_is_not_overlap() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        // 카드 왼쪽 변이 WebView 오른쪽 변에 닿는다(오른쪽 이웃 pane의 토스트).
        assert!(!covered_by_any(
            &webview,
            &[rect(1280.0, 640.0, 200.0, 40.0)]
        ));
        // 카드 아래 변이 WebView 위 변에 닿는다.
        assert!(!covered_by_any(&webview, &[rect(700.0, 20.0, 200.0, 40.0)]));
    }

    #[test]
    fn a_focused_webview_under_a_card_stays_visible() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        let card = [rect(1000.0, 640.0, 260.0, 40.0)];
        assert!(!hidden_under_toast(Some(&webview), &card, || true));
        assert!(hidden_under_toast(Some(&webview), &card, || false));
    }

    #[test]
    fn focus_is_not_queried_without_overlap() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        let card = [rect(300.0, 640.0, 200.0, 40.0)];
        assert!(!hidden_under_toast(Some(&webview), &card, || {
            panic!("포커스를 조회했다")
        }));
        assert!(!hidden_under_toast(None, &card, || panic!(
            "포커스를 조회했다"
        )));
    }

    #[test]
    fn a_webview_hidden_under_a_toast_is_not_revealed() {
        assert!(reveal_wanted(false, true, false));
        assert!(!reveal_wanted(false, true, true));
        assert!(!reveal_wanted(true, true, false));
        assert!(!reveal_wanted(false, false, false));
    }

    /// 오버레이를 지원하는 모드에서도 토스트·passive 팝업 아래 WebView 는 숨긴다.
    #[test]
    fn the_toast_rule_holds_in_every_overlay_mode() {
        for mode in [OverlayMode::Opaque, OverlayMode::Transparent] {
            assert!(!crate::state::webview_overlay::reveal_wanted(
                mode, true, true, true
            ));
            assert!(!crate::state::webview_overlay::reveal_wanted(
                mode, false, true, true
            ));
        }
    }

    #[test]
    fn partial_overlap_across_the_boundary_covers() {
        let webview = rect(600.0, 60.0, 680.0, 640.0);
        assert!(covered_by_any(&webview, &[rect(590.0, 640.0, 20.0, 40.0)]));
    }

    #[test]
    fn no_cards_cover_nothing() {
        assert!(!covered_by_any(&rect(0.0, 0.0, 10.0, 10.0), &[]));
    }
}

#[cfg(test)]
mod nav_origin_tests {
    use super::*;
    use crate::model::WebViewNavInfo;
    use crate::webview::WebViewUserNav;

    const A: &str = "file:///docs/a.html";

    fn shown(url: Option<&str>, back: bool, forward: bool) -> WebViewNavInfo {
        WebViewNavInfo {
            url: url.map(str::to_string),
            can_go_back: back,
            can_go_forward: forward,
        }
    }

    #[test]
    fn a_same_document_fragment_open_navigates_without_the_user_mark() {
        let plan = plan_user_nav(
            &WebViewUserNav::Open(format!("{A}#sec")),
            &shown(Some(A), false, false),
        );
        assert_eq!(
            plan,
            UserNavPlan::Navigate {
                mark_user_origin: false
            }
        );
    }

    #[test]
    fn opening_another_document_carries_the_user_mark() {
        let plan = plan_user_nav(
            &WebViewUserNav::Open("file:///docs/b.html#sec".into()),
            &shown(Some(A), false, false),
        );
        assert_eq!(
            plan,
            UserNavPlan::Navigate {
                mark_user_origin: true
            }
        );
    }

    #[test]
    fn steps_that_load_nothing_are_skipped() {
        let empty = shown(None, false, false);
        assert_eq!(
            plan_user_nav(&WebViewUserNav::Back, &empty),
            UserNavPlan::Skip
        );
        assert_eq!(
            plan_user_nav(&WebViewUserNav::Forward, &empty),
            UserNavPlan::Skip
        );
        assert_eq!(
            plan_user_nav(&WebViewUserNav::Reload, &empty),
            UserNavPlan::Skip
        );
        assert_eq!(
            plan_user_nav(&WebViewUserNav::Open("about:blank".into()), &empty),
            UserNavPlan::Skip
        );
    }

    #[test]
    fn history_steps_keep_the_user_mark_even_for_a_fragment_entry() {
        // 같은 문서 fragment 항목인지 미리 알 수 없다. 문서에 적은 남은 한계다.
        let info = shown(Some(&format!("{A}#sec")), true, true);
        for nav in [
            WebViewUserNav::Back,
            WebViewUserNav::Forward,
            WebViewUserNav::Reload,
        ] {
            assert_eq!(
                plan_user_nav(&nav, &info),
                UserNavPlan::Navigate {
                    mark_user_origin: true
                }
            );
        }
    }

    #[test]
    fn an_agent_step_is_skipped_only_when_there_is_nowhere_to_go() {
        use crate::webview::WebViewHistoryStep::{Back, Forward, Reload};
        use AgentStepPlan::{Navigate, Skip};
        let cases = [
            // 문서 없는 빈 탭
            (shown(None, false, false), [Skip, Skip, Skip]),
            // 히스토리 처음
            (shown(Some(A), false, true), [Skip, Navigate, Navigate]),
            // 히스토리 끝
            (shown(Some(A), true, false), [Navigate, Skip, Navigate]),
            // 가운데의 같은 문서 fragment 항목: 미리 알 수 없어 탐색한다
            (
                shown(Some(&format!("{A}#sec")), true, true),
                [Navigate, Navigate, Navigate],
            ),
        ];
        for (info, expected) in cases {
            for (step, want) in [Back, Forward, Reload].into_iter().zip(expected) {
                assert_eq!(
                    plan_agent_history_step(step, &info),
                    want,
                    "{info:?} {step:?}"
                );
            }
        }
    }

    /// 사용자가 스크립트 없는 문서 A를 보고 있는 surface.
    fn viewed_state() -> tasty_model::html_script::HtmlScriptState {
        let mut st = tasty_model::html_script::HtmlScriptState::new(true);
        st.on_load_started();
        st.on_main_response(A, None);
        st.on_committed(Some(A));
        st.on_load_finished();
        st.on_user_view();
        st.update_banner();
        assert!(st.banner().viewed);
        st
    }

    /// 표지를 세운 뒤 이동이 스크립트가 있는 문서 B를 commit한다.
    fn commit_scripted(st: &mut tasty_model::html_script::HtmlScriptState) {
        use tasty_model::html_script::{Fingerprint, ScriptDetection, ScriptScan};
        const B: &str = "file:///docs/b.html";
        st.on_load_started();
        st.on_main_response(
            B,
            Some(ScriptScan {
                fingerprint: Fingerprint([7; 32]),
                detection: ScriptDetection::Scripts,
                remote_refs: false,
                remote_scripts: false,
            }),
        );
        st.on_committed(Some(B));
        st.on_load_finished();
        st.update_banner();
    }

    #[test]
    fn an_agent_step_marks_a_host_load_so_the_banner_waits_for_the_user() {
        use crate::webview::WebViewHistoryStep;
        let info = shown(Some(A), true, true);
        for step in [
            WebViewHistoryStep::Back,
            WebViewHistoryStep::Forward,
            WebViewHistoryStep::Reload,
        ] {
            let mut st = viewed_state();
            mark_agent_step(plan_agent_history_step(step, &info), &mut st);
            // host_requested_load == true · viewed_before_commit == false 이면 다음 문서는
            // 열람을 이어받지 않는다. 필드는 모델 밖에서 읽을 수 없어 그 결과로 단언한다.
            commit_scripted(&mut st);
            let flags = st.banner();
            assert!(
                !flags.viewed,
                "{step:?}: 다음 문서를 사용자가 본 것으로 기록했다"
            );
            assert!(flags.pending_view, "{step:?}");
            assert_eq!(st.banner_phase(), BannerPhase::Hidden, "{step:?}");
        }
        // 대조: 사용자 출처이거나 표지가 없으면 같은 commit이 배너를 띄운다.
        let mut user = viewed_state();
        user.on_user_load_requested();
        commit_scripted(&mut user);
        assert_eq!(user.banner_phase(), BannerPhase::Blocked);
        let mut unmarked = viewed_state();
        commit_scripted(&mut unmarked);
        assert_eq!(unmarked.banner_phase(), BannerPhase::Blocked);
    }

    #[test]
    fn a_skipped_agent_step_leaves_no_mark() {
        use crate::webview::WebViewHistoryStep;
        let start = shown(Some(A), false, false);
        for step in [WebViewHistoryStep::Back, WebViewHistoryStep::Forward] {
            let mut st = viewed_state();
            let before = st.clone();
            mark_agent_step(plan_agent_history_step(step, &start), &mut st);
            assert_eq!(st, before, "{step:?}");
        }
        let mut st = tasty_model::html_script::HtmlScriptState::new(true);
        let before = st.clone();
        mark_agent_step(
            plan_agent_history_step(WebViewHistoryStep::Reload, &shown(None, false, false)),
            &mut st,
        );
        assert_eq!(st, before);
    }

    #[test]
    fn a_host_fragment_move_does_not_mark_a_host_load() {
        assert!(!host_load_marks_origin(Some(A), &format!("{A}#sec")));
        assert!(host_load_marks_origin(Some(A), "file:///docs/b.html"));
        assert!(host_load_marks_origin(None, &format!("{A}#sec")));
        assert!(host_load_marks_origin(Some(A), A));
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos", windows)))]
mod failed_nav_tests {
    use super::mirrored_nav_state;
    use crate::webview::NavState;

    #[test]
    fn a_failed_session_keeps_failed_over_any_native_state() {
        for native in [
            NavState::Idle,
            NavState::Loading,
            NavState::Done,
            NavState::Failed,
        ] {
            assert_eq!(mirrored_nav_state(native, true), NavState::Failed);
        }
    }

    #[test]
    fn a_live_session_mirrors_the_native_state() {
        for native in [NavState::Idle, NavState::Loading, NavState::Done] {
            assert_eq!(mirrored_nav_state(native, false), native);
        }
    }
}
