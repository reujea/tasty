use egui::emath::GuiRounding as _;

use crate::adapters::ui::surface::html_script_banner::remote_address_marker;
use crate::model::PhysicalRect;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme;

/// 탐색기의 최근 폴더를 RecentFiles에 저장할 kind. 주소창 자동완성에서 다시 읽는다.
const EXPLORER_RECENT_KIND: &str = "directory";

struct EguiPanelInfo {
    pane_id: u32,
    /// If Some, this is a specific surface within a split tab.
    /// If None, this is the entire tab's standalone surface.
    surface_id: Option<u32>,
    logical_x: f32,
    logical_y: f32,
    logical_w: f32,
    logical_h: f32,
    /// 탐색기라면 빈 영역 context menu에 사용할 현재 폴더.
    explorer_cwd: Option<std::path::PathBuf>,
    /// 렌더 중 engine을 빌리고 있으므로 DAG 조회 요청만 모아 두 패스 사이에 처리한다.
    dag_poll: Option<crate::adapters::ui::surface::dag_graph::DagPollRequest>,
    /// native WebView가 비워 두는 왼쪽·오른쪽·아래 여백(논리 px). webview chrome의 내용을 자른다.
    webview_inset: [f32; 3],
}

/// 비터미널 패널을 그린다. 터미널 내용은 별도 GPU 경로에서 처리한다.
#[allow(clippy::cognitive_complexity)] // complexity-exempt: egui 즉시모드 draw — panel kind별 렌더 분기, 클로저 중첩이 구조적
pub fn draw_egui_panels(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    pane_rects: &[(u32, PhysicalRect)],
    scale_factor: f32,
) {
    let mut infos = Vec::new();
    let mut html_actions = Vec::new();
    let mut receive_actions = Vec::new();
    // 원격 표지 툴팁이 피할 WebView 영역(탭 스트립 툴팁과 같은 목록).
    let native_content: Vec<egui::Rect> = state
        .native_content_rects
        .iter()
        .map(|r| crate::adapters::ui::to_egui_rect(*r, scale_factor))
        .collect();
    {
        let ws = state.active_workspace(engine);
        let ws_id = ws.id;
        let tab_bar_h = state.tab_bar_height;
        for &(pane_id, pane_rect) in pane_rects {
            let pane = match ws.pane_layout().find_pane(pane_id) {
                Some(p) => p,
                None => continue,
            };
            let tab = match pane.tabs.get(state.navigation.tab_index(pane)) {
                Some(t) => t,
                None => continue,
            };

            let content_rect = PhysicalRect {
                x: pane_rect.x,
                y: pane_rect.y + tab_bar_h,
                width: pane_rect.width,
                height: (pane_rect.height - tab_bar_h)
                    .max(tasty_type_geometry::length::PhysicalPx(1.0)),
            };
            // 점유된 터미널도 GPU가 읽기 전용으로 그린다. 비터미널은 숨기지 않고 내용을 표시한다.
            for r in state.tab_surface_regions(engine, tab, content_rect, scale_factor) {
                if r.surface.kind() == "terminal" {
                    continue;
                }
                // 일반 surface 메뉴로 대체되지 않도록 탐색기의 현재 폴더를 함께 보관한다.
                let explorer_cwd = engine
                    .find_surface_by_id(r.id)
                    .and_then(|surface| surface.explorer())
                    .map(|p| p.current_root().to_path_buf());
                let dag_poll = engine
                    .find_surface_by_id(r.id)
                    .and_then(|surface| surface.dag())
                    .map(|p| {
                        crate::adapters::ui::surface::dag_graph::DagPollRequest::from_surface(
                            p, ws_id,
                        )
                    });
                let logical = r.rect.to_logical(scale_factor);
                let webview_inset = crate::state::webview_edges::webview_edge_inset(
                    r.rect,
                    content_rect,
                    scale_factor,
                )
                .map(|v| v.to_logical(scale_factor).value());
                let info = EguiPanelInfo {
                    pane_id,
                    surface_id: Some(r.id),
                    logical_x: logical.x.value().round_ui(),
                    logical_y: logical.y.value().round_ui(),
                    logical_w: logical.width.value().round_ui(),
                    logical_h: logical.height.value().round_ui(),
                    explorer_cwd,
                    dag_poll,
                    webview_inset,
                };
                infos.push(info);
            }
        }
    }

    // 비어 있어도 호출해야 안 보이는 DAG의 폴링 타이머를 정리한다.
    // 실제 저장소 읽기는 500ms 간격 제한을 따른다.
    {
        let requests: Vec<_> = infos.iter().filter_map(|i| i.dag_poll.clone()).collect();
        let mut dag_views = std::mem::take(&mut state.dag_graph_views);
        dag_views.poll(&engine.as_ref(), &requests);
        state.dag_graph_views = dag_views;
    }

    let mut pending_empty_action: Option<crate::empty_ui::EmptyAction> = None;
    // engine을 빌린 렌더 루프가 끝난 뒤 탐색기 액션을 적용한다.
    let mut pending_explorer_action: Option<(u32, crate::explorer_ui::ExplorerAction)> = None;
    let mut pending_explorer_previews: Vec<(u32, crate::model::ExplorerPreview)> = Vec::new();
    let mut pending_explorer_hidden: Vec<(u32, bool)> = Vec::new();

    let explorer_font = engine
        .settings
        .appearance
        .effective_font_for_kind("explorer");
    let explorer_bg = crate::theme::theme().bg_panel().to_egui();

    // Temporarily extract view stores so we can hold a `&mut View` from
    // the store at the same time as `&mut Panel` from `engine.workspaces()`.
    let mut explorer_views = std::mem::take(&mut state.explorer_views);
    explorer_views.begin_poll_frame();
    let explorer_now = std::time::Instant::now();
    // egui-winit 이 winit Focused 이벤트로 갱신하는 OS 창 포커스.
    let explorer_window_focused = ctx.input(|i| i.focused);
    let mut dag_views = std::mem::take(&mut state.dag_graph_views);
    let explorer_favorites = engine.explorer_favorites;
    // cut 대기 경로를 어둡게 표시한다. 복사·붙여넣기 완료·취소 후에는 빈 목록으로 해제된다.
    let explorer_cut_pending: std::collections::HashSet<std::path::PathBuf> =
        state.explorer_clipboard.cut_paths();
    // 렌더 중 state를 빌리기 전에 최근 폴더를 한 번 읽어 둔다.
    let explorer_recent_dirs: Vec<String> = state.recent_files.get(EXPLORER_RECENT_KIND);
    // 타입어헤드를 허용할지 판단하는 값들. 셋 다 `state`와 `engine`을 읽어야 해서
    // 가변 차용이 걸리는 루프에 들어가기 전에 미리 꺼내 둔다(위 스냅샷들과 같은 이유).
    let focused_surface_id = state.focused_surface_id(engine);
    // 전체화면 무대도 함께 확인한다. 무대가 떠 있는 동안 그 뒤의 패널로 입력이 새면
    // 안 되기 때문이다.
    let overlay_open = state.keyboard_overlay_open() || state.fullscreen_stage_active();
    // OS 에서 끌어 오는 파일. 좌표는 물리 픽셀이라 egui 논리 포인트로 바꾼다.
    let ppp = ctx.pixels_per_point();
    let os_hover = state.drop_hover.as_ref().and_then(|h| {
        h.cursor.map(|(x, y)| {
            use tasty_type_geometry::length::PhysicalPx;
            crate::explorer_ui::view::drag::OsHover {
                paths: h.paths.clone(),
                pos: egui::pos2(
                    PhysicalPx(x).to_logical(ppp).value(),
                    PhysicalPx(y).to_logical(ppp).value(),
                ),
            }
        })
    });
    // 충돌 질문은 포커스를 잡지 않는 popup 이라 따로 본다. scrim 아래 칸에 놓으면 안 된다.
    let drag_blocked = overlay_open
        || state
            .popups
            .is_open(crate::adapters::ui::popup::explorer_conflict::EXPLORER_CONFLICT_POPUP_ID);
    // 키 바인딩 전체를 파싱하는 작업이라, explorer 패널이 하나도 없는 프레임에서는
    // 만들지 않는다(`explorer_cwd`는 `ExplorerPanel`일 때만 채워진다).
    let explorer_shortcut_chars = if infos.iter().any(|i| i.explorer_cwd.is_some()) {
        crate::explorer_ui::type_ahead::unmodified_binding_chars(&engine.settings.keybindings)
    } else {
        std::collections::HashSet::new()
    };

    let mut html_content_insets = std::collections::HashMap::new();
    let mut html_bands = std::mem::take(&mut state.html_address);
    let mut html_navs = Vec::new();
    let home = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf());
    let html_hint = engine
        .settings
        .keybindings
        .explorer_focus_address
        .first()
        .map(|b| {
            crate::i18n::t_fmt(
                "html_address.empty_hint",
                &tasty_settings::KeybindingSettings::format_display(b, &engine.settings.general),
            )
        });
    for info in &infos {
        let id_suffix = info
            .surface_id
            .map_or(format!("pane_{}", info.pane_id), |sid| {
                format!("surface_{}", sid)
            });

        let ws = state.active_workspace(engine);
        let mirror_ws_id = ws.mirror.then_some(ws.id);
        let Some(pane) = ws.pane_layout().find_pane(info.pane_id) else {
            continue;
        };
        let Some(tab) = pane.tabs.get(state.navigation.tab_index(pane)) else {
            continue;
        };
        let Some(sid) = info.surface_id.or_else(|| state.navigation.surface_id(tab)) else {
            continue;
        };
        let Some(surface) = engine.find_surface_by_id(sid) else {
            continue;
        };
        let bed_focused = crate::state::surface_focus::bed_focused(
            focused_surface_id,
            sid,
            &engine.live.occupancy,
        );

        if let Some(empty) = surface.empty() {
            draw_panel_frame_no_margin(ctx, &format!("empty_panel_{}", id_suffix), info, |ui| {
                if let Some(act) = crate::empty_ui::draw_empty(ui, empty, bed_focused) {
                    pending_empty_action = Some(act);
                }
            });
        } else if let Some(ex_panel) = surface.explorer() {
            let view = explorer_views.get_or_init(ex_panel, mirror_ws_id);
            // 다른 프로그램의 변경은 입력을 받는 창의 포커스 탭에 있는 칸만 확인한다.
            view.poll_external(
                crate::adapters::ui::surface::explorer::view::poll::checks_outside_changes(
                    explorer_window_focused,
                    focused_surface_id.is_some_and(|f| tab.contains_surface(f)),
                ),
                explorer_now,
            );
            view.set_file_details_check(engine.settings.general.explorer_refresh_file_details);
            view.ops.drag.flip = tasty_settings::keybindings::parse::Combo::parse_modifiers(
                &engine.settings.keybindings.explorer_drag_flip_modifier,
            );
            view.hidden.shortcut = engine
                .settings
                .keybindings
                .explorer_toggle_hidden
                .first()
                .map(|b| {
                    tasty_settings::KeybindingSettings::format_display(b, &engine.settings.general)
                });
            view.ops.drag.blocked = drag_blocked;
            view.ops.drag.os = os_hover.clone();
            let act = draw_panel_frame(
                ctx,
                &format!("explorer_panel_{}", id_suffix),
                info,
                0,
                Some(explorer_bg),
                |ui| {
                    crate::explorer_ui::draw_explorer(
                        ui,
                        ex_panel,
                        view,
                        &explorer_font,
                        &id_suffix,
                        explorer_favorites,
                        &explorer_cut_pending,
                        &explorer_recent_dirs,
                        mirror_ws_id,
                        &crate::explorer_ui::ExplorerInput {
                            focused: info.surface_id.is_some()
                                && info.surface_id == focused_surface_id,
                            overlay_open,
                            shortcut_chars: &explorer_shortcut_chars,
                        },
                    )
                },
            );
            if let Some(a) = act
                && pending_explorer_action.is_none()
            {
                pending_explorer_action = Some((ex_panel.id, a));
            }
            if let Some(preview) = view.preview.take_change() {
                pending_explorer_previews.push((ex_panel.id, preview));
            }
            if let Some(show) = view.hidden.take_change() {
                pending_explorer_hidden.push((ex_panel.id, show));
            }
        } else if let Some(dag) = surface.dag() {
            let view = dag_views.get_or_init(dag.id);
            let mut dag_id = dag.dag_id.clone();
            let mut direction = dag.direction;
            let mut open_session = None;
            draw_panel_frame_no_margin(ctx, &format!("dag_panel_{}", id_suffix), info, |ui| {
                let target = crate::adapters::ui::surface::dag_graph::DagTarget {
                    dag_id: &mut dag_id,
                    direction: &mut direction,
                };
                open_session = crate::adapters::ui::surface::dag_graph::draw_dag_graph(
                    ui,
                    target,
                    view,
                    crate::adapters::ui::surface::dag_graph::DagChrome::Own { bed_focused },
                );
            });
            // 사용자가 상세에서 누른 세션 열기다. 사용자 조작이라 포커스를 옮긴다.
            if let Some(session) = open_session
                && !state.reveal_surface(engine, session)
            {
                tracing::warn!(session, "DAG open session: surface not found");
            }
            if (dag_id.as_ref(), direction) != (dag.dag_id.as_ref(), dag.direction)
                && let Some(target) =
                    crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
            {
                state.dispatch_intent(
                    crate::intent::Intent::Engine(
                        crate::app::engine_action::EngineAction::DagSelection {
                            target,
                            dag_id,
                            direction,
                        },
                    )
                    .from_user_context_menu(),
                );
            }
        } else if let Some(remote) = surface.remote_webview() {
            // webview 내용은 native overlay가 그린다. 여기서는 URL 부재나 overlay 숨김 때 보일 배경을 그린다.
            if crate::runtime::surface_registry::webview_kind::is_webview_kind(remote.kind()) {
                let url = remote.url();
                let chrome_label = remote.label();
                let nav = remote.nav_state();
                let nav_info = remote.nav_info();
                let html_sid = info.surface_id.filter(|_| remote.kind() == "html");
                let cwd = html_sid.and_then(|sid| engine.local_surface_cwd(sid));
                let mirror = html_sid.is_some_and(|sid| engine.is_mirror_surface(sid));
                let received = html_sid.and_then(|sid| engine.html_receive(sid));
                let html_snapshot = html_sid.and_then(|sid| engine.html_script(sid));
                let content_inset = draw_panel_frame(
                    ctx,
                    &format!("webview_chrome_{}", id_suffix),
                    info,
                    0,
                    None,
                    |ui| {
                        let panel = ui.max_rect();
                        let th = theme::theme();
                        // 시안 순서: 주소 띠 → 크기 배너 → 원격 배너 → 스크립트 배너 → 페이지.
                        let band = html_sid.map(|sid| {
                            let source =
                                crate::adapters::ui::surface::html_address::HtmlAddressSource {
                                    nav,
                                    info: &nav_info,
                                    host_url: url.as_deref(),
                                    cwd: cwd.as_deref(),
                                    home: home.as_deref(),
                                    mirror,
                                    received: received.as_ref(),
                                    remote_marker: html_snapshot
                                        .as_ref()
                                        .and_then(|s| remote_address_marker(ui.ctx(), s, sid)),
                                    native: &native_content,
                                };
                            let frame = html_bands.draw(ui, &th, sid, panel, &source);
                            if let Some(nav) = frame.nav {
                                html_navs.push((sid, nav));
                            }
                            if frame.remote_reshow
                                && let Some(snapshot) = &html_snapshot
                            {
                                html_actions.push(snapshot.request(
                                    crate::runtime::html_script::HtmlActionKind::RemoteReshow,
                                ));
                            }
                            frame.height
                        });
                        let band_h = band.unwrap_or_default();
                        let below = panel.with_min_y(panel.min.y + band_h.value());
                        let size = received.as_ref().and_then(|snapshot| {
                            crate::adapters::ui::surface::html_receive::size_banner(
                                ui,
                                &th,
                                snapshot,
                                below,
                                &mut receive_actions,
                            )
                        });
                        let banners = html_sid.map_or_else(
                            || {
                                size.map(|h| th.banner_margin() + h + th.banner_inset_gap())
                                    .unwrap_or_default()
                            },
                            |sid| {
                                crate::adapters::ui::surface::html_script_banner::draw_stack(
                                    ui,
                                    &th,
                                    html_snapshot.as_ref(),
                                    &mut html_actions,
                                    sid,
                                    below,
                                    size,
                                )
                            },
                        );
                        let page = below.with_min_y(below.min.y + banners.value());
                        let [left, right, bottom] = info.webview_inset;
                        let content = egui::Rect::from_min_max(
                            egui::pos2(page.min.x + left, page.min.y),
                            egui::pos2(page.max.x - right, page.max.y - bottom),
                        );
                        let mut chrome_ui = ui.new_child(egui::UiBuilder::new().max_rect(page));
                        chrome_ui.set_clip_rect(ui.clip_rect().intersect(page));
                        if let Some(snapshot) = &received {
                            crate::adapters::ui::surface::html_receive::page(
                                &mut chrome_ui,
                                &th,
                                snapshot,
                                content,
                            );
                            if snapshot.failed
                                || (!snapshot.receiving && !snapshot.ready && snapshot.connected)
                            {
                                crate::webview_chrome_ui::draw_webview_chrome(
                                    &mut chrome_ui,
                                    content,
                                    None,
                                    chrome_label.as_deref(),
                                    if snapshot.failed {
                                        crate::webview::NavState::Failed
                                    } else {
                                        crate::webview::NavState::Idle
                                    },
                                    None,
                                );
                            }
                        } else {
                            crate::webview_chrome_ui::draw_webview_chrome(
                                &mut chrome_ui,
                                content,
                                url.as_deref().or(nav_info.url.as_deref()),
                                chrome_label.as_deref(),
                                nav,
                                html_sid.filter(|_| !mirror).and(html_hint.as_deref()),
                            );
                        }
                        html_sid.map(|_| band_h + banners)
                    },
                );
                if let (Some(sid), Some(inset)) = (info.surface_id, content_inset) {
                    html_content_insets.insert(sid, inset);
                }
            }
        }
    }

    html_bands.finish_frame();
    html_bands.retain(|sid| engine.find_surface_by_id(sid).is_some());
    state.html_address = html_bands;
    state.webview_user_nav.extend(html_navs);

    for action in html_actions {
        state.dispatch_intent(
            crate::intent::Intent::Frame(crate::app::frame_effect::DrawEffect::Html(action).into())
                .from_user_menu("html-script-banner"),
        );
    }

    for action in receive_actions {
        state.dispatch_intent(
            crate::intent::Intent::Frame(
                crate::app::frame_effect::DrawEffect::HtmlReceive(action).into(),
            )
            .from_user_menu("html-receive-banner"),
        );
    }

    let active_ws = state.active_workspace_index(engine);
    let tab_bar_h = state.tab_bar_height;
    draw_occupied_overlays(
        ctx,
        state,
        active_ws,
        tab_bar_h,
        engine,
        pane_rects,
        scale_factor,
    );

    state.explorer_views = explorer_views;
    state.dag_graph_views = dag_views;
    state.html_content_insets = html_content_insets;

    // engine의 하위 항목을 빌린 동안 모은 원격 목록 조회를 큐로 옮긴다.
    let requests = state.explorer_views.drain_outbox();
    for (sid, req) in requests {
        let Some(projection) = engine.mirror_projection_token(req.local_ws_id) else {
            continue;
        };
        let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        else {
            continue;
        };
        state.dispatch_intent(
            crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::ListDirectory {
                request: crate::core::PendingListDirForward {
                    local_ws_id: req.local_ws_id,
                    request_id: req.request_id,
                    dir: req.dir.to_string_lossy().to_string(),
                    consumer: Some(sid),
                },
                projection,
                target: Some(target),
            })
            .from_user_menu("explorer.list_directory"),
        );
    }

    if let Some((sid, act)) = pending_explorer_action {
        apply_explorer_action(state, engine, sid, act);
    }
    // 미리보기 토글·폭은 view 가 바로 바꾸고, 레이아웃 스냅샷에 실리도록 model 에도 남긴다.
    for (sid, preview) in pending_explorer_previews {
        if let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        {
            state.dispatch_intent(
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::ExplorerPreview { target, preview },
                )
                .from_user_context_menu(),
            );
        }
    }

    // 숨김 파일 토글도 view 가 바로 바꾸고 model 에 남겨 레이아웃 스냅샷에 싣는다.
    for (sid, show) in pending_explorer_hidden {
        if let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid)
        {
            state.dispatch_intent(
                crate::intent::Intent::Engine(
                    crate::app::engine_action::EngineAction::ExplorerHidden { target, show },
                )
                .from_user_context_menu(),
            );
        }
    }

    if let Some(crate::empty_ui::EmptyAction::OpenConvertPopup(sid)) = pending_empty_action {
        state.dialogs.convert_popup = Some(sid);
        state.dialogs.convert_popup_selected = None;
        state.dispatch_intent(
            crate::intent::UiIntent::OpenPopup {
                id: "convert_surface",
                mode: crate::intent::OpenPopupMode::WithScope(
                    crate::adapters::ui::popup::PopupScope::Surface(sid),
                ),
            }
            .from_user_menu("empty_surface_convert"),
        );
    }

    emit_surface_menu_fallback(state, ctx, &infos);
}

/// 비터미널의 보조 버튼 release를 처리한다. 별도 클릭 위젯을 덮지 않아 내부 버튼과 링크를 가리지 않는다.
/// 탐색기가 먼저 만든 메뉴가 있으면 유지한다. 없더라도 탐색기 위에서는 일반 surface 메뉴 대신
/// 현재 폴더의 빈 영역 메뉴를 연다. 좌표는 egui 논리 좌표이며 한 프레임에 메뉴 하나만 선택한다.
fn emit_surface_menu_fallback(
    state: &mut MainViewState,
    ctx: &egui::Context,
    infos: &[EguiPanelInfo],
) {
    if state.dialogs.pending_native_menu.is_some() {
        return;
    }
    let secondary_pos = ctx.input(|i| {
        if i.pointer.secondary_clicked() {
            i.pointer.interact_pos()
        } else {
            None
        }
    });
    let Some(pos) = secondary_pos else { return };
    for info in infos {
        let Some(sid) = info.surface_id else { continue };
        let within = pos.x >= info.logical_x
            && pos.x <= info.logical_x + info.logical_w
            && pos.y >= info.logical_y
            && pos.y <= info.logical_y + info.logical_h;
        if within {
            state.dialogs.pending_native_menu = Some(match &info.explorer_cwd {
                Some(cwd) => crate::state::PendingNativeMenu::Explorer {
                    surface_id: sid,
                    paths: Vec::new(),
                    cwd: cwd.clone(),
                    single_is_dir: false,
                    x: pos.x,
                    y: pos.y,
                },
                None => crate::state::PendingNativeMenu::Surface {
                    surface_id: sid,
                    x: pos.x,
                    y: pos.y,
                },
            });
            break;
        }
    }
}

/// 모아 둔 탐색기 액션을 원래 surface ID에 적용한다. 패널 변경만 `ExplorerCommand` 로 App에 보내고,
/// 메뉴·오류 표시·새로고침·파일 열기·파일 작업은 여기서 처리한다.
pub(crate) fn apply_explorer_action(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    sid: u32,
    act: crate::explorer_ui::ExplorerAction,
) {
    use crate::app::requests::explorer::ExplorerCommand as C;
    use crate::explorer_ui::ExplorerAction as A;
    match act {
        A::OpenFile(path)
            if state
                .explorer_views
                .get(sid)
                .is_some_and(|v| v.is_broken_link(&path)) =>
        {
            // 사라진 파일처럼 보이지 않도록 링크 대상이 없다는 원인을 알린다.
            state.toasts.push(
                crate::i18n::t_fmt("explorer.state.broken_link", &path.display().to_string()),
                crate::adapters::ui::ToastKind::Error,
                crate::adapters::ui::ToastScope::Surface(sid),
            );
        }
        A::OpenFile(path) => {
            // mirror 탐색기의 경로는 원격 파일이다. 식별과 핸들러 선택은 file::dispatch::remote가 맡는다.
            state.dispatch_intent(
                crate::app::command::DomainIntent::DispatchFile {
                    target: crate::file::format::FileTarget::new(path),
                    depth: crate::file::format::DetectDepth::Deep,
                    origin_surface_id: Some(sid),
                    dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
                    ignore_size_limit: false,
                    run: None,
                }
                .from_user_menu("explorer_open_file"),
            );
        }
        A::Ops(op) => state.apply_explorer_ops(engine, sid, op),
        A::Refresh => {
            if let Some(v) = state.explorer_views.get_mut(sid) {
                v.request_refresh();
            }
        }
        A::AddressRejected(why) => {
            state.toasts.push(
                why.message(),
                crate::adapters::ui::ToastKind::Error,
                crate::adapters::ui::ToastScope::Surface(sid),
            );
        }
        A::SetViewMode(m) => {
            dispatch_explorer_command(state, engine, sid, C::ViewMode(m));
            let mode = m.as_str().to_string();
            if engine.settings.general.explorer_view_mode != mode {
                state.dispatch_intent(
                    crate::intent::Intent::PatchSettings(
                        crate::app::engine_action::SettingsPatch::ExplorerMode(mode),
                    )
                    .from_user_menu("explorer.view_mode"),
                );
            }
        }
        A::SetSort(column) => dispatch_explorer_command(state, engine, sid, C::Sort(column)),
        A::Navigate(path) => {
            let recent = path.display().to_string();
            move_explorer(state, engine, sid, C::Navigate(path));
            // 사용자 탐색만 최근 폴더에 기록한다. 에이전트 cwd 변경은 별도 경로다.
            state.recent_files.add(EXPLORER_RECENT_KIND, recent);
        }
        A::GoBack => move_explorer(state, engine, sid, C::Back),
        A::GoForward => move_explorer(state, engine, sid, C::Forward),
        A::GoUp => move_explorer(state, engine, sid, C::Up),
        A::NewTab => move_explorer(state, engine, sid, C::NewTab),
        A::CloseTab(index) => move_explorer(state, engine, sid, C::CloseTab(index)),
        A::SelectTab(index) => move_explorer(state, engine, sid, C::SelectTab(index)),
        A::Create { dir, name, folder } => state.request_explorer_file(
            engine,
            sid,
            crate::app::explorer_files::Operation::Create { dir, name, folder },
            crate::intent::IntentOrigin::User {
                source: crate::intent::UserSource::Menu("explorer_create"),
            },
        ),
        A::MoreMenu { x, y } => {
            state.dialogs.pending_native_menu =
                Some(crate::state::PendingNativeMenu::ExplorerMore {
                    surface_id: sid,
                    x,
                    y,
                });
        }
        A::ContextMenu { target, cwd, x, y } => {
            use crate::explorer_ui::ExplorerMenuTarget as T;
            let (paths, single_is_dir) = match target {
                T::Favorite { path } => {
                    state.dialogs.pending_native_menu =
                        Some(crate::state::PendingNativeMenu::ExplorerFavorite {
                            surface_id: sid,
                            path,
                            x,
                            y,
                        });
                    return;
                }
                T::Empty => (Vec::new(), false),
                T::Single { path, is_dir } => (vec![path], is_dir),
                T::Multi { paths } => (paths, false),
            };
            state.dialogs.pending_native_menu = Some(crate::state::PendingNativeMenu::Explorer {
                surface_id: sid,
                paths,
                cwd,
                single_is_dir,
                x,
                y,
            });
        }
    }
}

/// 폴더나 내부 탭을 바꾸는 명령. 주소창 편집을 취소해 이전 버퍼가 다른 대상에 적용되지 않게 한다.
fn move_explorer(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    sid: u32,
    command: crate::app::requests::explorer::ExplorerCommand,
) {
    dispatch_explorer_command(state, engine, sid, command);
    if let Some(v) = state.explorer_views.get_mut(sid) {
        v.cancel_addr_edit();
        // 타입어헤드 버퍼도 같은 이유로 버린다. `sync` 의 재적재 경로만으로는
        // cwd 가 같은 내부 탭 사이 전환이 안 잡힌다.
        v.reset_type_ahead();
    }
}

/// 지금 surface 세대에 묶어 App에 보낸다. surface가 이미 없으면 보내지 않는다.
fn dispatch_explorer_command(
    state: &mut MainViewState,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    sid: u32,
    command: crate::app::requests::explorer::ExplorerCommand,
) {
    if let Some(target) = crate::runtime::surface_binding::SurfaceBinding::capture(engine, sid) {
        state.dispatch_intent(
            crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::Explorer(
                crate::app::requests::explorer::BoundExplorerCommand::new(target, command),
            ))
            .from_user_context_menu(),
        );
    }
}

/// egui Area와 Frame을 만들고 body의 반환값을 전달한다.
fn draw_panel_frame<R, F>(
    ctx: &egui::Context,
    id: &str,
    info: &EguiPanelInfo,
    margin: i8,
    bg_color: Option<egui::Color32>,
    body: F,
) -> R
where
    F: FnOnce(&mut egui::Ui) -> R,
    R: Default,
{
    let th = theme::theme();
    let bg = bg_color.unwrap_or(th.bg_app().into());
    let mut out: R = R::default();
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(egui::pos2(info.logical_x, info.logical_y))
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_min_size(egui::vec2(info.logical_w, info.logical_h));
            ui.set_max_size(egui::vec2(info.logical_w, info.logical_h));
            let panel_rect = ui.max_rect();
            let mut clip_ui = ui.new_child(egui::UiBuilder::new().max_rect(panel_rect));
            clip_ui.set_clip_rect(panel_rect);
            clip_ui.painter().rect_filled(panel_rect, 0.0, bg);
            egui::Frame::new()
                .fill(bg)
                .inner_margin(egui::Margin::same(margin))
                .show(&mut clip_ui, |ui| {
                    out = body(ui);
                });
        });
    out
}

/// 여백 없이 Area만 거는 변형. Empty surface처럼 배경을 직접 칠하는 경우에 사용.
fn draw_panel_frame_no_margin<F>(ctx: &egui::Context, id: &str, info: &EguiPanelInfo, body: F)
where
    F: FnOnce(&mut egui::Ui),
{
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(egui::pos2(info.logical_x, info.logical_y))
        .order(egui::Order::Background)
        .show(ctx, |ui| {
            ui.set_min_size(egui::vec2(info.logical_w, info.logical_h));
            ui.set_max_size(egui::vec2(info.logical_w, info.logical_h));
            let panel_rect = ui.max_rect();
            let mut clip_ui = ui.new_child(egui::UiBuilder::new().max_rect(panel_rect));
            clip_ui.set_clip_rect(panel_rect);
            body(&mut clip_ui);
        });
}

/// soft/hard 점유를 각각 Theme의 해당 색으로 표시한다.
/// 포커스와 관계없이 표시하며 강제 해제 버튼은 hard 점유에만 제공한다.
fn draw_occupied_overlays(
    ctx: &egui::Context,
    state: &mut crate::state::MainViewState,
    active_ws: usize,
    tab_bar_h: tasty_type_geometry::length::PhysicalPx,
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    pane_rects: &[(u32, PhysicalRect)],
    scale_factor: f32,
) {
    let th = theme::theme();

    struct Occ {
        pane_id: u32,
        sid: u32,
        hard: bool,
        // 응답 대기 테두리가 같은 자리에 오면 점유선은 그리지 않는다(강제 해제 버튼은 남긴다).
        edge: bool,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    }
    let mut occ: Vec<Occ> = Vec::new();
    {
        let Some(ws) = engine.workspace_at(active_ws) else {
            return;
        };
        for &(pane_id, pane_rect) in pane_rects {
            let Some(pane) = ws.pane_layout().find_pane(pane_id) else {
                continue;
            };
            let Some(tab) = pane.tabs.get(state.navigation.tab_index(pane)) else {
                continue;
            };
            let content_rect = PhysicalRect {
                x: pane_rect.x,
                y: pane_rect.y + tab_bar_h,
                width: pane_rect.width,
                height: (pane_rect.height - tab_bar_h)
                    .max(tasty_type_geometry::length::PhysicalPx(1.0)),
            };
            for r in state.tab_surface_regions(engine, tab, content_rect, scale_factor) {
                let hard = if engine.live.occupancy.is_content_hidden(r.id) {
                    true
                } else {
                    match engine.live.occupancy.occupancy_of(r.id) {
                        Some(occ) => occ.tier == crate::core::attach::OccupancyTier::Hard,
                        None => continue,
                    }
                };
                let logical = r.rect.to_logical(scale_factor);
                occ.push(Occ {
                    pane_id,
                    sid: r.id,
                    hard,
                    edge: tasty_ui_widgets::occupancy_edge_shows(
                        engine.attention_kind(r.id).map(Into::into),
                    ),
                    x: logical.x.value().round_ui(),
                    y: logical.y.value().round_ui(),
                    w: logical.width.value().round_ui(),
                    h: logical.height.value().round_ui(),
                });
            }
        }
    }
    if occ.is_empty() {
        return;
    }

    // 장식 테두리는 입력을 받는 Area 대신 painter로 그려 터미널·구분선 입력을 가로채지 않는다.
    // 페인 탭 바 레이어에 그려 팝업 아래에 두고, 뒤에 그리는 이동 링이 이 테두리를 덮게 한다.
    for o in occ.iter().filter(|o| o.edge) {
        let painter =
            ctx.layer_painter(crate::adapters::ui::tab_bar::pane_tab_bar_layer(o.pane_id));
        let rect = egui::Rect::from_min_size(egui::pos2(o.x, o.y), egui::vec2(o.w, o.h));
        painter.rect_stroke(
            rect,
            0.0,
            tasty_ui_widgets::occupancy_edge_stroke(&th, o.hard),
            egui::StrokeKind::Inside,
        );
    }

    // hard 점유의 강제 해제 버튼만 작은 Area로 입력을 받는다. 클릭 적용은 렌더 뒤에 한다.
    let mut pending_force_detach: Option<u32> = None;
    for o in &occ {
        if !o.hard {
            continue;
        }
        let btn_w = (o.w - 8.0).clamp(24.0, 96.0);
        let inset = th.spacing_xs.value();
        let btn_pos = egui::pos2(o.x + o.w - btn_w - inset, o.y + inset);
        let clicked = egui::Area::new(egui::Id::new(format!("attach_force_detach_{}", o.sid)))
            .fixed_pos(btn_pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| -> bool {
                ui.set_min_size(egui::vec2(btn_w, 20.0));
                ui.set_max_size(egui::vec2(btn_w, 20.0));
                ui.button(crate::i18n::t("attach.force_detach"))
                    .on_hover_text(crate::i18n::t("attach.occupied_surface"))
                    .clicked()
            })
            .inner;
        if clicked {
            pending_force_detach = Some(o.sid);
        }
    }

    if let Some(sid) = pending_force_detach
        && let Some(lock) = engine.live.occupancy.occupancy_of(sid)
    {
        state.dispatch_intent(
            crate::intent::Intent::Engine(crate::app::engine_action::EngineAction::DetachSurface {
                surface: sid,
                grant: lock.granted_seq,
            })
            .from_user_context_menu(),
        );
    }
}

#[cfg(test)]
mod explorer_open_tests {
    /// 탐색기 더블클릭은 mirror 여부와 관계없이 origin surface를 실은 사용자 파일 dispatch가 된다.
    #[test]
    fn an_explorer_open_dispatches_the_file_with_its_origin_on_both_sides() {
        for mirror in [false, true] {
            let (mut state, mut engine_session) = if mirror {
                crate::state::tests::test_mirror_state()
            } else {
                crate::state::tests::test_state()
            };
            let engine = engine_session.borrow_mut();
            let sid = engine
                .workspace_at(0)
                .expect("workspace index is valid")
                .all_surface_ids()[0];
            super::apply_explorer_action(
                &mut state,
                &engine.read(),
                sid,
                crate::explorer_ui::ExplorerAction::OpenFile("/some/doc.md".into()),
            );
            assert_eq!(state.toasts.len(), 0, "mirror={mirror}");
            let intents = state.take_pending_intents();
            assert_eq!(intents.len(), 1, "mirror={mirror}");
            let crate::intent::Intent::Domain(crate::app::command::DomainIntent::DispatchFile {
                origin_surface_id,
                dispatch_origin,
                ..
            }) = &intents[0].body
            else {
                panic!("expected DispatchFile, got {:?}", intents[0].body);
            };
            assert_eq!(*origin_surface_id, Some(sid));
            assert_eq!(
                *dispatch_origin,
                crate::file::dispatch::FileDispatchOrigin::User
            );
            assert!(intents[0].origin.is_user());
        }
    }
}

#[cfg(test)]
mod explorer_command_tests {
    use crate::app::engine_action::EngineAction;
    use crate::app::requests::explorer::ExplorerCommand as C;
    use crate::explorer_ui::{ExplorerAction as A, ExplorerMenuTarget};

    /// 메뉴·새로고침은 화면에서 끝나고, 패널 변경만 원 surface에 묶인 사용자 실행 요청이 된다.
    #[test]
    fn only_panel_changes_leave_the_view_as_bound_explorer_commands() {
        let (mut state, mut session, sid, root) = crate::state::tests::explorer_fixture(false);
        let engine = session.borrow_mut();
        for act in [
            A::Refresh,
            A::MoreMenu { x: 1.0, y: 2.0 },
            A::ContextMenu {
                target: ExplorerMenuTarget::Empty,
                cwd: root.clone(),
                x: 1.0,
                y: 2.0,
            },
        ] {
            super::apply_explorer_action(&mut state, &engine.read(), sid, act);
        }
        assert!(state.take_pending_intents().is_empty());
        assert!(state.dialogs.pending_native_menu.is_some());

        let sub = root.join("sub");
        for (act, expected) in [
            (A::Navigate(sub.clone()), C::Navigate(sub)),
            (A::GoBack, C::Back),
            (A::GoForward, C::Forward),
            (A::GoUp, C::Up),
            (
                A::SetSort(crate::model::SortColumn::Size),
                C::Sort(crate::model::SortColumn::Size),
            ),
            (A::NewTab, C::NewTab),
            (A::CloseTab(1), C::CloseTab(1)),
            (A::SelectTab(0), C::SelectTab(0)),
        ] {
            super::apply_explorer_action(&mut state, &engine.read(), sid, act);
            let intents = state.take_pending_intents();
            assert_eq!(intents.len(), 1, "{expected:?}");
            let crate::intent::Intent::Engine(EngineAction::Explorer(request)) = &intents[0].body
            else {
                panic!("expected an explorer command, got {:?}", intents[0].body);
            };
            assert_eq!(request.command(), &expected);
            assert_eq!(request.target().surface_id(), sid);
            assert!(request.target().current_in(&engine.read()));
            assert!(intents[0].origin.is_user());
        }
    }
}
