//! pane별 탭바. 앱 상태에서 화면 입력을 만들고 반환된 동작과 측정 높이를 반영한다.
//! 공용 화면 함수는 앱 상태 없이 갤러리에서도 사용할 수 있다.

mod apply;
mod tab;
mod view;

pub use apply::apply_tab_bar_actions;
pub use view::{compute_drop_index, draw_pane_tab_bars_view};

use crate::runtime::engine_read::EngineRead;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::PhysicalPx;
use tasty_type_geometry::rect::PhysicalRect;

use crate::adapters::ui::icons;
use crate::core::AttentionKind;
use crate::state::MainViewState;
use crate::theme;

/// View 입력 — pane 한 개 분의 탭 데이터.
#[derive(Clone, Debug)]
pub struct PaneTabBarView {
    /// View namespace for an uncommitted pane; pane_id remains its real anchor.
    pub draft_pane: Option<u64>,
    pub draft_tabs: Vec<Option<u64>>,
    pub loading: Vec<bool>,
    pub pane_id: u32,
    /// Pane 의 *물리* 좌표 사각형 (view 가 scale_factor 로 logical 변환).
    pub rect: PhysicalRect,
    pub tab_names: Vec<String>,
    /// 탭별 leading 아이콘. wrapper 가 registry(kind→`SurfaceKindDef.icon`)에서 해석해
    /// 담는다 — view 는 kind 를 모른 채 아이콘만 그린다(엔진 비의존 유지).
    pub tab_icons: Vec<icons::Icon>,
    /// 탭별 attention kind — 탭에 속한 surface 들의 dominant kind
    /// (`CoreState::attention_dominant_kind`). `Some(NeedsInput)`=노랑,
    /// `Some(Completion)`=파랑, `None`=attention 없음(active/평상시 색으로 폴백).
    pub tab_attention_kind: Vec<Option<AttentionKind>>,
    /// 탭별 busy(녹색 점) 여부.
    pub tab_is_busy: Vec<bool>,
    pub active_tab: usize,
    /// 이 pane 이 현재 focus 인지 — 배경 (surface0 vs mantle) 결정.
    pub is_focused: bool,
    /// 가로 스크롤 오프셋 (logical px).
    pub scroll_offset: f32,
    /// 이동 대기 표시를 둘 탭 인덱스와 형태. 없으면 None이다.
    pub move_mark: Option<(usize, TabMoveMark)>,
    /// 탭별 html 스크립트 표지와 그 표지가 가리키는 surface.
    pub tab_html_script_marker: Vec<Option<TabScriptMarker>>,
}

/// 탭 칸의 html 스크립트 표지. lock 을 누르면 이 surface 의 배너를 다시 보인다(docs/surfaces/html/index.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabScriptMarker {
    pub surface_id: u32,
    pub kind: tasty_ui_widgets::HtmlScriptMarkerKind,
    /// 표지의 surface가 `http(s)` 페이지다. allowed 툴팁이 페이지 문구를 쓴다.
    pub web: bool,
}

/// 탭 칸의 이동 대기 표시. 탭이 대상이면 링, 대상 서피스를 담은 비활성 탭이면 글리프다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabMoveMark {
    Ring,
    Glyph,
}

/// pane 탭 바 Area의 레이어. 이동 링과 점유 테두리도 이 레이어에 그려 탭 바 위, 팝업 아래에 둔다.
pub fn pane_tab_bar_layer(pane_id: u32) -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(format!("pane_tabs_{pane_id}")),
    )
}

/// View 입력 — drag 진행 중인 탭의 상태. None 이면 drag overlay 미표시.
#[derive(Clone, Debug)]
pub struct TabDragView {
    pub pane_id: u32,
    pub tab_index: usize,
    /// 현재 마우스 x (logical pane 좌표).
    pub current_x: f32,
}

/// View 입력 — 전체 pane 의 탭 바 + drag 상태 + appearance 옵션.
pub struct PaneTabBarsProps<'a> {
    pub theme: &'a Theme,
    /// 탭 slot 키캡 문자를 읽을 키바인딩 설정 (switch-number overlay 표시=동작 일치).
    pub kb: &'a crate::settings::KeybindingSettings,
    pub panes: &'a [PaneTabBarView],
    pub scale_factor: f32,
    /// 사용자 옵션 — 탭 1 개의 가로 너비 (logical px).
    pub tab_width: f32,
    /// 사용자 옵션 — 탭 라벨 폰트 크기 (logical px).
    pub tab_font_size: f32,
    /// 사용자 옵션 — 활성 탭 인디케이터 스타일 (Underline / Fill / Dot).
    pub active_tab_indicator: crate::settings::ActiveTabIndicator,
    /// 현재 drag 진행 상태 (None 이면 overlay 미표시).
    pub drag: Option<TabDragView>,
    /// switch-number overlay — 키캡을 그릴 **focused pane id**.
    /// 사용자가 `tab_switch_modifier`(대상=Tab)를 누르고 있는 동안만 `Some(focused_pane)`,
    /// 그 외엔 `None`. 이 pane 의 탭바에서만 leading 아이콘을 숫자 키캡(`Ctrl+1`…`0`)으로
    /// in-place 교체한다(비-focused pane 은 held 여도 아이콘 유지). release 시 `None` → 원복.
    pub switch_overlay_pane: Option<u32>,
    /// 드러난 네이티브 WebView 영역(egui 논리 좌표). 탭 스트립 툴팁이 이 영역을 피한다.
    pub native_content: &'a [egui::Rect],
}

/// View 가 발생시킨 사용자 의도. wrapper 가 state/engine 으로 반영.
#[derive(Clone, Debug, PartialEq)]
pub enum TabBarAction {
    SwitchTab {
        pane_id: u32,
        tab_index: usize,
    },
    CloseTab {
        pane_id: u32,
        tab_index: usize,
    },
    AddTab {
        pane_id: u32,
    },
    /// 탭스트립 우측 Split 아이콘 — 해당 pane 을 분할 (기존 split_pane 경로 재사용).
    RequestSplit {
        pane_id: u32,
    },
    /// 탭스트립 우측 Search 아이콘 — 해당 pane 활성 surface 검색 (기존 find 경로 재사용).
    OpenSearch {
        pane_id: u32,
    },
    ScrollLeft {
        pane_id: u32,
    },
    ScrollRight {
        pane_id: u32,
    },
    /// 활성 탭 전환(또는 pane 리사이즈)으로 활성 탭이 뷰포트 밖으로 밀려났을 때
    /// view 가 계산한 보정 스크롤 오프셋. 사용자가 직접 누른 액션이 아니라 뷰
    /// 렌더링 결과에 대한 보정이므로 `focus_target_pane` 대상에서 제외한다.
    AutoScrollToActiveTab {
        pane_id: u32,
        offset: f32,
    },
    /// 탭바의 탭 없는 빈 영역(뷰포트) primary click — 탭 전환 없이 그 pane 으로
    /// focus 만 이동한다.
    FocusPane {
        pane_id: u32,
    },
    OpenContextMenu {
        pane_id: u32,
        tab_index: usize,
        pos: egui::Pos2,
    },
    OpenPaneContextMenu {
        pane_id: u32,
        pos: egui::Pos2,
    },
    /// 탭 "+" 버튼 우클릭 — 프리셋으로 탭/페인 생성 진입점.
    OpenNewTabButtonContextMenu {
        pane_id: u32,
        pos: egui::Pos2,
    },
    DragStart {
        pane_id: u32,
        tab_index: usize,
    },
    DragUpdate {
        pane_id: u32,
        mouse_x: f32,
    },
    DragEnd {
        pane_id: u32,
    },
    /// 탭의 lock 표지 클릭. 그 surface 의 배너만 다시 보이며 탭 전환·포커스 이동은 없다.
    ShowHtmlScriptBanner {
        surface_id: u32,
    },
}

impl TabBarAction {
    /// 직접 조작할 pane. 클릭·드래그 시작은 먼저 포커스를 옮긴다.
    /// 드래그는 클릭 없이 시작할 수 있어 별도로 포함한다. 후속 드래그 프레임은 다시 옮기지 않는다.
    /// 우클릭 메뉴는 대상 ID를 전달하며 열 때 포커스를 바꾸지 않는다.
    fn focus_target_pane(&self) -> Option<u32> {
        match *self {
            TabBarAction::SwitchTab { pane_id, .. }
            | TabBarAction::CloseTab { pane_id, .. }
            | TabBarAction::AddTab { pane_id }
            | TabBarAction::RequestSplit { pane_id }
            | TabBarAction::OpenSearch { pane_id }
            | TabBarAction::ScrollLeft { pane_id }
            | TabBarAction::ScrollRight { pane_id }
            | TabBarAction::FocusPane { pane_id }
            | TabBarAction::DragStart { pane_id, .. } => Some(pane_id),
            TabBarAction::OpenContextMenu { .. }
            | TabBarAction::OpenPaneContextMenu { .. }
            | TabBarAction::OpenNewTabButtonContextMenu { .. }
            | TabBarAction::DragUpdate { .. }
            | TabBarAction::DragEnd { .. }
            | TabBarAction::ShowHtmlScriptBanner { .. }
            | TabBarAction::AutoScrollToActiveTab { .. } => None,
        }
    }
}

/// View 의 출력 — 사용자 의도 리스트 + 측정된 탭 바 높이.
#[derive(Default)]
pub struct PaneTabBarsOutput {
    pub pending_selection: Option<u64>,
    pub actions: Vec<TabBarAction>,
    /// 첫 pane에서 측정한 탭바 높이를 물리 픽셀로 변환한 값. 측정하지 못했으면 None이다.
    pub measured_height_physical: Option<PhysicalPx>,
}

/// 탭별 busy(녹색 점) 여부 계산. `is_surface_busy()`(로컬 ∪ mirror busy 합집합)를
/// 거쳐야 원격 attach mirror surface 를 담은 탭도 dot 이 뜬다.
fn compute_tab_is_busy(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    tabs: &[crate::model::Tab],
) -> Vec<bool> {
    tabs.iter()
        .map(|t| {
            let sids = t.all_surface_ids();
            sids.iter().any(|sid| engine.is_surface_busy(*sid))
        })
        .collect()
}

/// 탭별 html 스크립트 표지. 탭 안의 html surface 중 닫힌 차단 표지(lock)를 허용 표지보다 먼저 고른다.
fn compute_tab_html_script_markers(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    tabs: &[crate::model::Tab],
) -> Vec<Option<TabScriptMarker>> {
    use tasty_model::html_script::ScriptMarker;
    use tasty_ui_widgets::HtmlScriptMarkerKind;
    tabs.iter()
        .map(|t| {
            let mut found: Option<TabScriptMarker> = None;
            for sid in t.all_surface_ids() {
                let Some(snapshot) = engine.html_script(sid) else {
                    continue;
                };
                let kind = match snapshot.marker {
                    Some(ScriptMarker::Blocked) => HtmlScriptMarkerKind::Blocked,
                    Some(ScriptMarker::Allowed) => HtmlScriptMarkerKind::Allowed,
                    None => continue,
                };
                let marker = TabScriptMarker {
                    surface_id: sid,
                    kind,
                    web: snapshot.web,
                };
                if kind == HtmlScriptMarkerKind::Blocked {
                    return Some(marker);
                }
                found.get_or_insert(marker);
            }
            found
        })
        .collect()
}

/// 이 pane의 탭 칸에 둘 이동 대기 표시.
fn tab_move_mark(
    mark: Option<crate::adapters::ui::move_source::MoveSourceMark>,
    pane_id: u32,
) -> Option<(usize, TabMoveMark)> {
    use crate::adapters::ui::move_source::MoveSourceMark as M;
    match mark? {
        M::TabRing {
            pane_id: p,
            tab_index,
        } if p == pane_id => Some((tab_index, TabMoveMark::Ring)),
        M::TabGlyph {
            pane_id: p,
            tab_index,
        } if p == pane_id => Some((tab_index, TabMoveMark::Glyph)),
        _ => None,
    }
}

/// 화면 입력을 만들고 결과를 앱 상태에 반영한다.
pub fn draw_pane_tab_bars(
    ctx: &egui::Context,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    pane_rects: &[(u32, PhysicalRect)],
    terminal_rect: PhysicalRect,
    scale_factor: f32,
) {
    let th = theme::theme();
    let focused_pane_id = state.focused_pane_id(engine);
    let move_mark = crate::adapters::ui::move_source::resolve(
        state.pending_move,
        &state.navigation,
        engine,
        state.active_workspace_index(engine),
        pane_rects,
        state.tab_bar_height,
        Some(&state.layout_previews),
        scale_factor,
    );

    let mut panes: Vec<PaneTabBarView> = Vec::new();
    {
        let ws = state.active_workspace(engine);
        for &(pane_id, pane_rect) in pane_rects {
            let pane = match ws.pane_layout().find_pane(pane_id) {
                Some(p) => p,
                None => continue,
            };
            let tab_attention_kind: Vec<Option<AttentionKind>> = pane
                .tabs
                .iter()
                .map(|t| {
                    let sids = t.all_surface_ids();
                    engine.attention_dominant_kind(&sids)
                })
                .collect();
            let tab_is_busy = compute_tab_is_busy(engine, &pane.tabs);
            let tab_html_script_marker = compute_tab_html_script_markers(engine, &pane.tabs);
            panes.push(PaneTabBarView {
                draft_pane: None,
                draft_tabs: Vec::new(),
                loading: Vec::new(),
                pane_id,
                rect: pane_rect,
                tab_names: pane
                    .tabs
                    .iter()
                    .map(|t| {
                        let sid = state.navigation.surface_id(t);
                        let name = engine.tab_display_name(t, sid);
                        if engine
                            .find_surface_by_id(sid.unwrap_or(0))
                            .is_some_and(|s| s.kind() == "explorer")
                        {
                            crate::explorer_ui::display_text::nfc(&name).into_owned()
                        } else {
                            name
                        }
                    })
                    .collect(),
                tab_icons: pane
                    .tabs
                    .iter()
                    .map(|t| {
                        let kind = engine
                            .find_surface_by_id(state.navigation.surface_id(t).unwrap_or(0))
                            .map(|s| s.kind())
                            .unwrap_or("terminal");
                        engine
                            .surface_registry
                            .get(kind)
                            .and_then(|d| d.icon.clone())
                            .map(|n| icons::from_name(&n))
                            .unwrap_or(icons::FILE)
                    })
                    .collect(),
                tab_attention_kind,
                tab_is_busy,
                active_tab: state.navigation.tab_index(pane),
                is_focused: pane_id == focused_pane_id,
                scroll_offset: state
                    .tab_bar_scroll
                    .get(&pane_id)
                    .copied()
                    .unwrap_or_default()
                    .value(),
                move_mark: tab_move_mark(move_mark, pane_id),
                tab_html_script_marker,
            });
        }
    }

    let now = std::time::Instant::now();
    let delay = std::time::Duration::from_secs_f64(th.loading_overlay_delay().to_secs_f64());
    let selected = state
        .attach_pending
        .selected(engine, &state.navigation)
        .map(|entry| entry.id);
    for pane in &mut panes {
        let real = engine
            .find_pane_by_id(pane.pane_id)
            .expect("view pane exists");
        pane.draft_tabs.resize(pane.tab_names.len(), None);
        pane.loading.resize(pane.tab_names.len(), false);
        if pane.pane_id == focused_pane_id
            && state
                .attach_waits
                .get(&state.active_workspace(engine).id)
                .and_then(|times| times.iter().map(|wait| wait.admitted).min())
                .is_some_and(|time| now.duration_since(time) >= delay)
            && let Some(loading) = pane.loading.get_mut(pane.active_tab)
        {
            *loading = true;
        }
        if state
            .attach_pending
            .selected(engine, &state.navigation)
            .is_some_and(|entry| entry.pane == pane.pane_id && !entry.visible)
        {
            pane.active_tab = usize::MAX;
        }
        for entry in state
            .attach_pending
            .visible(engine, state.active_workspace(engine).id)
            .filter(|entry| entry.pane == pane.pane_id)
        {
            let loading = now.duration_since(entry.receipt.admitted) >= delay;
            match entry.creation {
                Some(crate::state::attach_pending::Creation::Tab) => {
                    if selected == Some(entry.id) {
                        pane.active_tab = pane.tab_names.len();
                    }
                    pane.tab_names
                        .push(crate::i18n::t("attach.pending.new_tab").to_string());
                    pane.tab_icons.push(
                        engine
                            .surface_registry
                            .get(&entry.kind)
                            .and_then(|kind| kind.icon.clone())
                            .map(|name| icons::from_name(&name))
                            .unwrap_or(icons::FILE),
                    );
                    pane.draft_tabs.push(Some(entry.id.0));
                    pane.loading.push(loading);
                    pane.tab_attention_kind.push(None);
                    pane.tab_is_busy.push(false);
                    pane.tab_html_script_marker.push(None);
                }
                Some(crate::state::attach_pending::Creation::Surface(_)) | None => {
                    if let Some(index) = real.tabs.iter().position(|tab| tab.id == entry.tab) {
                        pane.loading[index] |= loading;
                    }
                }
                _ => {}
            }
        }
    }
    for (id, rect, is_pane) in state.pending_regions(engine, terminal_rect, scale_factor) {
        if !is_pane {
            continue;
        }
        let Some(entry) = state
            .attach_pending
            .entries
            .iter()
            .find(|entry| entry.id == id)
        else {
            continue;
        };
        panes.push(PaneTabBarView {
            draft_pane: Some(id.0),
            draft_tabs: vec![Some(id.0)],
            loading: vec![now.duration_since(entry.receipt.admitted) >= delay],
            pane_id: entry.pane,
            rect,
            tab_names: vec![crate::i18n::t("attach.pending.new_tab").to_string()],
            tab_icons: vec![
                engine
                    .surface_registry
                    .get(&entry.kind)
                    .and_then(|kind| kind.icon.clone())
                    .map(|name| icons::from_name(&name))
                    .unwrap_or(icons::FILE),
            ],
            tab_attention_kind: vec![None],
            tab_is_busy: vec![false],
            active_tab: 0,
            is_focused: selected == Some(id),
            scroll_offset: 0.0,
            move_mark: None,
            tab_html_script_marker: vec![None],
        });
    }

    let appearance = &engine.settings.appearance;
    let tab_w = appearance.tab_width;
    let tab_font_size = appearance.tab_font_size;

    let drag = state.dialogs.tab_drag.as_ref().map(|d| TabDragView {
        pane_id: d.pane_id,
        tab_index: d.tab_index,
        current_x: d.current_x,
    });

    // 사용자 modifier 입력으로 갱신한 스냅샷에서 키캡을 표시할 pane을 읽는다.
    let switch_overlay_pane = state.switch_overlay().and_then(|o| match o.target {
        crate::adapters::ui::switch_overlay::SwitchTarget::Tab => o.pane_id,
        crate::adapters::ui::switch_overlay::SwitchTarget::Workspace
        | crate::adapters::ui::switch_overlay::SwitchTarget::Category => None,
    });

    let native_content: Vec<egui::Rect> = state
        .native_content_rects
        .iter()
        .map(|(_, r)| crate::adapters::ui::to_egui_rect(*r, scale_factor))
        .collect();

    let props = PaneTabBarsProps {
        theme: &th,
        kb: &engine.settings.keybindings,
        panes: &panes,
        scale_factor,
        tab_width: tab_w,
        tab_font_size,
        active_tab_indicator: appearance.active_tab_indicator,
        drag,
        switch_overlay_pane,
        native_content: &native_content,
    };

    let output = draw_pane_tab_bars_view(ctx, &props);

    if let Some(h_phys) = output.measured_height_physical {
        state.tab_bar_height = h_phys;
    }

    if let Some(id) = output.pending_selection {
        state.attach_pending.select(
            crate::state::attach_pending::PendingId(id),
            &state.navigation,
        );
    }
    apply_tab_bar_actions(state, engine, output.actions, &panes, tab_w, scale_factor);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_target_pane_primary_click_actions_carry_pane_id() {
        let primary = [
            TabBarAction::SwitchTab {
                pane_id: 7,
                tab_index: 0,
            },
            TabBarAction::CloseTab {
                pane_id: 7,
                tab_index: 0,
            },
            TabBarAction::AddTab { pane_id: 7 },
            TabBarAction::RequestSplit { pane_id: 7 },
            TabBarAction::OpenSearch { pane_id: 7 },
            TabBarAction::ScrollLeft { pane_id: 7 },
            TabBarAction::ScrollRight { pane_id: 7 },
            TabBarAction::FocusPane { pane_id: 7 },
            TabBarAction::DragStart {
                pane_id: 7,
                tab_index: 0,
            },
        ];
        for action in primary {
            assert_eq!(
                action.focus_target_pane(),
                Some(7),
                "{action:?} 는 그 pane 으로 focus 를 옮겨야 한다"
            );
        }
    }

    #[test]
    fn focus_target_pane_context_menu_and_drag_update_end_actions_are_none() {
        let non_focus = [
            TabBarAction::OpenContextMenu {
                pane_id: 7,
                tab_index: 0,
                pos: egui::Pos2::ZERO,
            },
            TabBarAction::OpenPaneContextMenu {
                pane_id: 7,
                pos: egui::Pos2::ZERO,
            },
            TabBarAction::OpenNewTabButtonContextMenu {
                pane_id: 7,
                pos: egui::Pos2::ZERO,
            },
            TabBarAction::DragUpdate {
                pane_id: 7,
                mouse_x: 0.0,
            },
            TabBarAction::DragEnd { pane_id: 7 },
            TabBarAction::AutoScrollToActiveTab {
                pane_id: 7,
                offset: 0.0,
            },
        ];
        for action in non_focus {
            assert_eq!(
                action.focus_target_pane(),
                None,
                "{action:?} 는 focus 를 옮기면 안 된다(우클릭은 조작 commit 이 아니고, DragUpdate/DragEnd 는 DragStart 에서 이미 focus 가 이동한 뒤의 후속 프레임)"
            );
        }
    }

    #[test]
    fn compute_drop_index_first_slot() {
        let idx = compute_drop_index(100.0, 100.0, 0.0, 3, 120.0, 1.0, 400.0);
        assert_eq!(idx, 0);
    }

    #[test]
    fn compute_drop_index_middle_slot() {
        // mouse_x=281, pane_x=100 → content_x=181 → slot=181/121 ≈ 1.496 → round 1
        let idx = compute_drop_index(281.0, 100.0, 0.0, 3, 120.0, 1.0, 400.0);
        assert_eq!(idx, 1);
    }

    #[test]
    fn compute_drop_index_last_slot_clamped() {
        let idx = compute_drop_index(10_000.0, 100.0, 0.0, 3, 120.0, 1.0, 400.0);
        assert_eq!(idx, 2);
    }

    #[test]
    fn compute_drop_index_accounts_for_scroll() {
        let idx0 = compute_drop_index(100.0, 100.0, 0.0, 5, 120.0, 1.0, 400.0);
        let idx_scroll = compute_drop_index(100.0, 100.0, 121.0, 5, 120.0, 1.0, 400.0);
        assert_eq!(idx0, 0);
        assert_eq!(idx_scroll, 1);
    }

    fn test_theme() -> Theme {
        tasty_themes::mocha_fallback()
    }

    fn run_view(panes: Vec<PaneTabBarView>, drag: Option<TabDragView>) -> PaneTabBarsOutput {
        run_view_on(&egui::Context::default(), panes, drag)
    }

    /// 같은 Context로 여러 프레임을 그려 활성 탭의 자동 스크롤을 검사한다.
    fn run_view_on(
        ctx: &egui::Context,
        panes: Vec<PaneTabBarView>,
        drag: Option<TabDragView>,
    ) -> PaneTabBarsOutput {
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let mut out = PaneTabBarsOutput::default();
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            let props = PaneTabBarsProps {
                theme: &theme,
                kb: &kb,
                panes: &panes,
                scale_factor: 1.0,
                tab_width: 160.0,
                tab_font_size: 12.0,
                active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                drag: drag.clone(),
                switch_overlay_pane: None,
                native_content: &[],
            };
            out = draw_pane_tab_bars_view(ctx, &props);
        }));
        out
    }

    /// 시안 우선순위: Ctrl 전환 키캡은 같은 아이콘 칸의 스피너 위에 그린다.
    #[test]
    fn switch_keycap_paints_above_the_tab_loading_spinner() {
        let mut theme = test_theme();
        theme.reduced_motion = true;
        let kb = crate::settings::KeybindingSettings::default();
        let mut pane = mk_pane(3, &["server", "build-server"], 0, true);
        pane.loading = vec![false, true];
        let panes = vec![pane];
        let ctx = egui::Context::default();
        // 첫 프레임은 Area 크기 측정이라 도형을 내보내지 않을 수 있어 두 번째 프레임을 본다.
        let frame = |ctx: &egui::Context| {
            let props = PaneTabBarsProps {
                theme: &theme,
                kb: &kb,
                panes: &panes,
                scale_factor: 1.0,
                tab_width: 160.0,
                tab_font_size: 12.0,
                active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                drag: None,
                switch_overlay_pane: Some(3),
                native_content: &[],
            };
            let _view = draw_pane_tab_bars_view(ctx, &props);
        };
        drop(ctx.run(egui::RawInput::default(), frame));
        let output = ctx.run(egui::RawInput::default(), frame);
        let (spinner, keycap) =
            crate::adapters::ui::attach_loading::paint_order::spinner_and_keycap(&output, &theme);
        let (spinner, keycap) = (spinner.expect("spinner"), keycap.expect("keycap"));
        assert!(keycap > spinner, "{keycap} <= {spinner}");
    }

    #[test]
    fn pending_pane_strip_has_no_widget_identity_collision_with_its_real_anchor() {
        let real = mk_pane_w(7, &["server"], 0, false, 400.0);
        let mut draft = mk_pane_w(7, &["New tab"], 0, true, 400.0);
        draft.rect.x = PhysicalPx(402.0);
        draft.draft_pane = Some(1);
        draft.draft_tabs = vec![Some(1)];
        draft.loading = vec![true];
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let result = run_view_on(&ctx, vec![real.clone(), draft.clone()], None);
            assert!(result.actions.iter().all(|action| !matches!(
                action,
                TabBarAction::AddTab { .. } | TabBarAction::RequestSplit { .. }
            )));
        }
    }

    fn mk_pane(pane_id: u32, names: &[&str], active: usize, focused: bool) -> PaneTabBarView {
        mk_pane_w(pane_id, names, active, focused, 800.0)
    }

    fn mk_pane_w(
        pane_id: u32,
        names: &[&str],
        active: usize,
        focused: bool,
        width: f32,
    ) -> PaneTabBarView {
        let n = names.len();
        PaneTabBarView {
            draft_pane: None,
            draft_tabs: Vec::new(),
            loading: Vec::new(),
            pane_id,
            rect: PhysicalRect {
                x: PhysicalPx(0.0),
                y: PhysicalPx(0.0),
                width: PhysicalPx(width),
                height: PhysicalPx(600.0),
            },
            tab_names: names.iter().map(|s| s.to_string()).collect(),
            tab_icons: vec![icons::TERM; n],
            tab_attention_kind: vec![None; n],
            tab_is_busy: vec![false; n],
            active_tab: active,
            is_focused: focused,
            scroll_offset: 0.0,
            move_mark: None,
            tab_html_script_marker: vec![None; n],
        }
    }

    /// [`TabBarAction::AutoScrollToActiveTab`] 중 주어진 pane 대상인 것의 offset.
    fn auto_scroll_offset(out: &PaneTabBarsOutput, pane_id: u32) -> Option<f32> {
        out.actions.iter().find_map(|a| match *a {
            TabBarAction::AutoScrollToActiveTab { pane_id: p, offset } if p == pane_id => {
                Some(offset)
            }
            _ => None,
        })
    }

    #[test]
    fn switching_to_offscreen_tab_scrolls_it_into_view() {
        let pane = mk_pane(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 7, true);
        let out = run_view(vec![pane], None);

        let tab_w = 160.0;
        let separator_w = 1.0;
        let viewport_w = 704.0;
        let tab_start = 7.0 * (tab_w + separator_w);
        let tab_end = tab_start + tab_w;

        let offset = auto_scroll_offset(&out, 1).expect("offscreen 전환은 보정을 emit 해야 한다");
        assert!(offset <= tab_start);
        assert!(offset + viewport_w >= tab_end);
    }

    #[test]
    fn switching_to_first_tab_wraps_scroll_back_into_view() {
        let ctx = egui::Context::default();
        let mut pane = mk_pane(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 7, true);
        pane.scroll_offset = 612.0; // max_scroll
        run_view_on(&ctx, vec![pane], None);

        let mut pane = mk_pane(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 0, true);
        pane.scroll_offset = 612.0;
        let out = run_view_on(&ctx, vec![pane], None);

        let offset = auto_scroll_offset(&out, 1).expect("wrap-around 전환도 보정을 emit 해야 한다");
        assert_eq!(offset, 0.0, "첫 탭은 뷰포트 좌측 끝(0)에서 보여야 한다");
    }

    #[test]
    fn active_tab_unchanged_does_not_override_manual_scroll() {
        // 활성 탭·크기가 그대로면 사용자가 이동한 스크롤을 덮어쓰지 않는다.
        let ctx = egui::Context::default();
        let pane = mk_pane(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 0, true);
        run_view_on(&ctx, vec![pane], None);

        let mut pane = mk_pane(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 0, true);
        pane.scroll_offset = 400.0;
        let out = run_view_on(&ctx, vec![pane], None);

        assert_eq!(
            auto_scroll_offset(&out, 1),
            None,
            "활성 탭이 그대로면 수동 스크롤 상태를 덮어쓰면 안 된다"
        );
    }

    #[test]
    fn pane_resize_reveals_correction_even_without_active_change() {
        // 활성 탭은 그대로이고 pane만 좁아진 경우에도 필요한 스크롤을 보정한다.
        let ctx = egui::Context::default();
        let wide = mk_pane_w(
            1,
            &["A", "B", "C", "D", "E", "F", "G", "H"],
            7,
            true,
            2000.0,
        );
        run_view_on(&ctx, vec![wide], None);

        let narrow = mk_pane_w(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 7, true, 800.0);
        let out = run_view_on(&ctx, vec![narrow], None);

        let tab_w = 160.0;
        let separator_w = 1.0;
        let viewport_w = 704.0;
        let tab_start = 7.0 * (tab_w + separator_w);
        let tab_end = tab_start + tab_w;
        let offset =
            auto_scroll_offset(&out, 1).expect("resize 로 out-of-view 가 된 경우도 보정해야 한다");
        assert!(offset <= tab_start);
        assert!(offset + viewport_w >= tab_end);
    }

    #[test]
    fn viewport_that_already_shows_active_tab_emits_no_correction() {
        let pane = mk_pane(1, &["A", "B", "C", "D", "E", "F", "G", "H"], 0, true);
        let out = run_view(vec![pane], None);
        assert_eq!(auto_scroll_offset(&out, 1), None);
    }

    #[test]
    fn view_idle_emits_no_actions() {
        let panes = vec![mk_pane(1, &["A", "B"], 0, true)];
        let out = run_view(panes, None);
        assert!(out.actions.is_empty());
        assert!(out.measured_height_physical.is_some());
    }

    #[test]
    fn view_measures_bar_height_for_first_pane() {
        let panes = vec![
            mk_pane(1, &["A"], 0, true),
            mk_pane(2, &["X", "Y"], 0, false),
        ];
        let out = run_view(panes, None);
        assert!(out.measured_height_physical.unwrap_or_default().value() > 0.0);
    }

    #[test]
    fn view_empty_panes_returns_default_output() {
        let out = run_view(vec![], None);
        assert!(out.actions.is_empty());
        assert!(out.measured_height_physical.is_none());
    }

    #[test]
    fn view_with_drag_does_not_panic() {
        let panes = vec![mk_pane(1, &["A", "B", "C"], 1, true)];
        let drag = Some(TabDragView {
            pane_id: 1,
            tab_index: 1,
            current_x: 240.0,
        });
        let out = run_view(panes, drag);
        assert!(out.actions.is_empty());
    }

    fn test_engine() -> crate::runtime::engine_session::EngineSession {
        let waker: tasty_terminal::Waker = std::sync::Arc::new(|| {});
        crate::runtime::engine_session::EngineSession::new(80, 24, waker).expect("engine")
    }

    fn tab_with_surface(sid: crate::model::SurfaceId) -> crate::model::Tab {
        let surface = crate::model::SurfaceDescriptor::new(sid, "empty");
        crate::model::Tab::new_with_surface(1, "t".to_string(), surface)
    }

    /// 로컬 PTY가 없는 mirror surface도 원격 busy 상태로 탭 표시를 갱신한다.
    #[test]
    fn compute_tab_is_busy_true_for_mirror_only_surface() {
        let mut engine_session = test_engine();
        let mut engine = engine_session.borrow_mut();
        let sid = 4242;
        engine.set_mirror_surface_busy(sid, true);
        let tabs = vec![tab_with_surface(sid)];

        let result = compute_tab_is_busy(&engine.read(), &tabs);

        assert_eq!(result, vec![true]);
    }

    #[test]
    fn compute_tab_is_busy_false_when_idle() {
        let mut engine_session = test_engine();
        let engine = engine_session.borrow_mut();
        let sid = 4343;
        let tabs = vec![tab_with_surface(sid)];

        let result = compute_tab_is_busy(&engine.read(), &tabs);

        assert_eq!(result, vec![false]);
    }

    /// 탭 0 가운데 줄의 `x`를 누르고 떼어 나온 동작을 돌려준다.
    fn click_tab_row(pane: &PaneTabBarView, x: f32) -> Vec<TabBarAction> {
        let ctx = egui::Context::default();
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let panes = vec![pane.clone()];
        let pos = egui::pos2(x, theme.tab_bar_height.value() / 2.0);
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let frames = [
            vec![egui::Event::PointerMoved(pos)],
            vec![egui::Event::PointerMoved(pos)],
            vec![press(true)],
            vec![press(false)],
        ];
        let mut actions = Vec::new();
        for events in frames {
            let input = egui::RawInput {
                events,
                ..Default::default()
            };
            drop(ctx.run(input, |ctx| {
                let props = PaneTabBarsProps {
                    theme: &theme,
                    kb: &kb,
                    panes: &panes,
                    scale_factor: 1.0,
                    tab_width: 160.0,
                    tab_font_size: 12.0,
                    active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                    drag: None,
                    switch_overlay_pane: None,
                    native_content: &[],
                };
                actions = draw_pane_tab_bars_view(ctx, &props).actions;
            }));
        }
        actions
    }

    fn marker_pane(kind: tasty_ui_widgets::HtmlScriptMarkerKind) -> PaneTabBarView {
        let mut pane = mk_pane(1, &["page", "b"], 1, true);
        pane.tab_html_script_marker[0] = Some(TabScriptMarker {
            surface_id: 7,
            kind,
            web: false,
        });
        pane
    }

    /// 탭 0을 오른쪽에서 왼쪽으로 훑어 표지가 받은 클릭 위치를 모은다.
    fn marker_click_xs(pane: &PaneTabBarView) -> Vec<f32> {
        (0..40)
            .map(|k| 2.0 + 4.0 * k as f32)
            .filter(|&x| {
                click_tab_row(pane, x)
                    .iter()
                    .any(|a| matches!(a, TabBarAction::ShowHtmlScriptBanner { surface_id: 7 }))
            })
            .collect()
    }

    #[test]
    fn blocked_marker_click_reshows_the_banner_instead_of_switching() {
        let pane = marker_pane(tasty_ui_widgets::HtmlScriptMarkerKind::Blocked);
        let xs = marker_click_xs(&pane);
        assert!(!xs.is_empty());
        let hit = test_theme().html_script_marker_hit().value();
        // 표지 칸은 탭 오른쪽 묶음 안에만 있다.
        assert!(xs.iter().all(|&x| x > 160.0 - 64.0 && x < 160.0), "{xs:?}");
        assert!(xs.last().unwrap() - xs[0] <= hit, "{xs:?}");
        let on_marker = click_tab_row(&pane, xs[0]);
        assert!(
            !on_marker
                .iter()
                .any(|a| matches!(a, TabBarAction::SwitchTab { .. })),
            "{on_marker:?}"
        );
        // 표지 밖은 평소처럼 탭 전환이다.
        assert!(
            click_tab_row(&pane, 20.0)
                .iter()
                .any(|a| matches!(a, TabBarAction::SwitchTab { tab_index: 0, .. }))
        );
    }

    #[test]
    fn allowed_marker_and_no_marker_take_no_click() {
        let allowed = marker_pane(tasty_ui_widgets::HtmlScriptMarkerKind::Allowed);
        assert!(marker_click_xs(&allowed).is_empty());
        let mut none = allowed.clone();
        none.tab_html_script_marker[0] = None;
        assert!(marker_click_xs(&none).is_empty());
    }

    /// 이동 대기 표시를 단 pane 하나를 그린 뒤 모든 도형을 돌려준다.
    fn shapes_with_move_mark(mark: Option<(usize, TabMoveMark)>) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        let mut pane = mk_pane(1, &["a", "b", "c"], 0, true);
        pane.move_mark = mark;
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let panes = vec![pane];
        // Area는 첫 프레임을 크기 측정에만 쓰므로 두 번째 프레임의 도형을 읽는다.
        let mut out = egui::FullOutput::default();
        for _ in 0..2 {
            out = ctx.run(egui::RawInput::default(), |ctx| {
                let props = PaneTabBarsProps {
                    theme: &theme,
                    kb: &kb,
                    panes: &panes,
                    scale_factor: 1.0,
                    tab_width: 160.0,
                    tab_font_size: 12.0,
                    active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                    drag: None,
                    switch_overlay_pane: None,
                    native_content: &[],
                };
                drop(draw_pane_tab_bars_view(ctx, &props));
            });
        }
        out.shapes.into_iter().map(|c| c.shape).collect()
    }

    /// 새 Area는 페이드인 중이라 알파가 곱해진 색으로 나온다. 알파를 걷어 낸 색으로 비교한다.
    fn is_move_color(c: egui::Color32) -> bool {
        let pink: egui::Color32 = test_theme().move_source_ring().into();
        // egui는 감마 공간에서 알파를 곱하므로 같은 공간에서 나눈다.
        let a = u32::from(c.a());
        let un = |x: u8| (u32::from(x) * 255 / a.max(1)) as u8;
        a > 0
            && [(c.r(), pink.r()), (c.g(), pink.g()), (c.b(), pink.b())]
                .iter()
                .all(|&(x, y)| un(x).abs_diff(y) <= 3)
    }

    fn move_colored_segments(shapes: &[egui::Shape]) -> Vec<[egui::Pos2; 2]> {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::LineSegment { points, stroke } if is_move_color(stroke.color) => {
                    Some(*points)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn move_ring_dashes_stay_on_the_marked_tab_cell() {
        let segments = move_colored_segments(&shapes_with_move_mark(Some((1, TabMoveMark::Ring))));
        assert!(!segments.is_empty());
        // 탭 1은 탭 0(160)과 구분선(1) 뒤에서 시작하고 자기 오른쪽 구분선까지 포함한다.
        for [a, b] in segments {
            for p in [a, b] {
                assert!((161.0..=322.0).contains(&p.x), "{p:?}");
            }
        }
    }

    #[test]
    fn move_glyph_marks_the_tab_without_a_ring() {
        // 글리프는 SVG 이미지라 이미지 로더가 없는 테스트 컨텍스트에서는 도형으로 확인할 수 없다.
        let shapes = shapes_with_move_mark(Some((2, TabMoveMark::Glyph)));
        assert!(move_colored_segments(&shapes).is_empty());
    }

    #[test]
    fn no_move_mark_paints_no_move_color() {
        let shapes = shapes_with_move_mark(None);
        assert!(move_colored_segments(&shapes).is_empty());
    }

    /// 이미지 로더를 설치하고 pane 하나를 여러 프레임 그린 뒤 마지막 프레임의 도형을 돌려준다.
    /// 새 Area의 페이드인이 끝나도록 충분히 그린다.
    fn settled_strip_shapes(pane: PaneTabBarView) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let panes = vec![pane];
        let mut out = egui::FullOutput::default();
        for _ in 0..30 {
            out = ctx.run(egui::RawInput::default(), |ctx| {
                let props = PaneTabBarsProps {
                    theme: &theme,
                    kb: &kb,
                    panes: &panes,
                    scale_factor: 1.0,
                    tab_width: 160.0,
                    tab_font_size: 12.0,
                    active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                    drag: None,
                    switch_overlay_pane: None,
                    native_content: &[],
                };
                drop(draw_pane_tab_bars_view(ctx, &props));
            });
        }
        let mut shapes = Vec::new();
        fn flatten(s: egui::Shape, out: &mut Vec<egui::Shape>) {
            match s {
                egui::Shape::Vec(v) => v.into_iter().for_each(|s| flatten(s, out)),
                other => out.push(other),
            }
        }
        out.shapes
            .into_iter()
            .for_each(|c| flatten(c.shape, &mut shapes));
        shapes
    }

    /// 폭 400인 `scrolling_pane`의 두 화살표 칸 중심 x. split·search 두 칸(28씩)을 뺀 344가
    /// [왼쪽 화살표][viewport][오른쪽 화살표]다.
    fn arrow_centers_x() -> [f32; 2] {
        let bar_h = test_theme().tab_bar_height.value();
        [bar_h / 2.0, 400.0 - 28.0 * 2.0 - bar_h / 2.0]
    }

    /// 두 화살표 칸 가운데에 칠한 `tab_scroll_arrow_glyph_size` 정사각 이미지의 중심과 tint.
    /// 같은 크기의 탭 칸 move 글리프는 칸 중심에 있지 않아 제외된다.
    fn arrow_glyphs(shapes: &[egui::Shape]) -> Vec<(egui::Pos2, egui::Color32)> {
        let size = test_theme().tab_scroll_arrow_glyph_size().value();
        let centers = arrow_centers_x();
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r)
                    if r.brush.is_some()
                        && (r.rect.width() - size).abs() < 0.01
                        && (r.rect.height() - size).abs() < 0.01
                        && centers.iter().any(|x| (r.rect.center().x - x).abs() < 0.01) =>
                {
                    Some((r.rect.center(), r.fill))
                }
                _ => None,
            })
            .collect()
    }

    fn scrolling_pane(scroll: f32) -> PaneTabBarView {
        let mut pane = mk_pane_w(1, &["a", "b", "c", "d", "e", "f", "g", "h"], 0, true, 400.0);
        pane.scroll_offset = scroll;
        pane
    }

    #[test]
    fn scroll_arrows_are_chevrons_in_square_strip_height_cells() {
        let th = test_theme();
        let shapes = settled_strip_shapes(scrolling_pane(0.0));
        let [left_center, right_center] = arrow_centers_x();
        let glyphs = arrow_glyphs(&shapes);
        assert_eq!(glyphs.len(), 2, "{glyphs:?}");
        assert!((glyphs[0].0.x - left_center).abs() < 0.01, "{glyphs:?}");
        assert!((glyphs[1].0.x - right_center).abs() < 0.01, "{glyphs:?}");
        assert_eq!(glyphs[0].1, th.tab_scroll_arrow_fg_disabled().to_egui());
        assert_eq!(glyphs[1].1, th.tab_scroll_arrow_fg().to_egui());
        let arrow_text = shapes.iter().any(|s| match s {
            egui::Shape::Text(t) => matches!(t.galley.text(), "<" | ">"),
            _ => false,
        });
        assert!(!arrow_text, "화살표를 문자로 그리지 않는다");
    }

    #[test]
    fn scrolled_to_the_end_disables_the_right_arrow() {
        let th = test_theme();
        let glyphs = arrow_glyphs(&settled_strip_shapes(scrolling_pane(10_000.0)));
        assert_eq!(
            glyphs.iter().map(|g| g.1).collect::<Vec<_>>(),
            vec![
                th.tab_scroll_arrow_fg().to_egui(),
                th.tab_scroll_arrow_fg_disabled().to_egui()
            ]
        );
    }

    fn hidden(i: usize, scroll: f32) -> Option<tasty_ui_widgets::TabScrollArrowSide> {
        // 칸 폭 160 + 구분선 1, viewport 300.
        view::hidden_move_side(
            i,
            tasty_type_geometry::length::LogicalPx(161.0),
            tasty_type_geometry::length::LogicalPx(160.0),
            tasty_type_geometry::length::LogicalPx(scroll),
            tasty_type_geometry::length::LogicalPx(300.0),
        )
    }

    #[test]
    fn a_move_target_counts_as_visible_only_when_its_cell_is_fully_inside() {
        use tasty_ui_widgets::TabScrollArrowSide::{Left, Right};
        // 탭 1은 [161, 321]. viewport [scroll, scroll + 300].
        assert_eq!(
            hidden(1, 21.0),
            None,
            "오른쪽 끝이 viewport 끝과 같으면 보인다"
        );
        assert_eq!(
            hidden(1, 20.5),
            Some(Right),
            "0.5만 넘쳐도 오른쪽에 가려진다"
        );
        assert_eq!(
            hidden(1, 161.0),
            None,
            "왼쪽 끝이 viewport 시작과 같으면 보인다"
        );
        assert_eq!(hidden(1, 161.5), Some(Left), "0.5만 잘려도 왼쪽에 가려진다");
        assert_eq!(hidden(0, 0.0), None);
        assert_eq!(hidden(7, 0.0), Some(Right));
        assert_eq!(hidden(0, 400.0), Some(Left));
    }

    fn marked_pane(mark: TabMoveMark, target: usize, scroll: f32) -> PaneTabBarView {
        let mut pane = scrolling_pane(scroll);
        pane.move_mark = Some((target, mark));
        pane
    }

    #[test]
    fn the_arrow_toward_a_hidden_move_target_turns_pink_for_tab_and_surface_targets() {
        let th = test_theme();
        let pink = th.tab_scroll_arrow_move_fg().to_egui();
        for mark in [TabMoveMark::Ring, TabMoveMark::Glyph] {
            // 스크롤 0에서 탭 7은 오른쪽 밖이다. 왼쪽은 끝에 닿아 disabled로 남는다.
            let glyphs = arrow_glyphs(&settled_strip_shapes(marked_pane(mark, 7, 0.0)));
            assert_eq!(
                glyphs.iter().map(|g| g.1).collect::<Vec<_>>(),
                vec![th.tab_scroll_arrow_fg_disabled().to_egui(), pink],
                "{mark:?}"
            );
            // 끝까지 스크롤하면 탭 0은 왼쪽 밖이다. 오른쪽은 끝에 닿아 disabled다.
            let glyphs = arrow_glyphs(&settled_strip_shapes(marked_pane(mark, 0, 10_000.0)));
            assert_eq!(
                glyphs.iter().map(|g| g.1).collect::<Vec<_>>(),
                vec![pink, th.tab_scroll_arrow_fg_disabled().to_egui()],
                "{mark:?}"
            );
        }
    }

    #[test]
    fn a_fully_visible_move_target_leaves_the_arrows_alone() {
        let th = test_theme();
        let glyphs = arrow_glyphs(&settled_strip_shapes(marked_pane(
            TabMoveMark::Ring,
            0,
            0.0,
        )));
        assert_eq!(
            glyphs.iter().map(|g| g.1).collect::<Vec<_>>(),
            vec![
                th.tab_scroll_arrow_fg_disabled().to_egui(),
                th.tab_scroll_arrow_fg().to_egui()
            ]
        );
    }

    fn run_frame(
        ctx: &egui::Context,
        pane: &PaneTabBarView,
        raw: egui::RawInput,
        actions: &mut Vec<TabBarAction>,
    ) {
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let panes = vec![pane.clone()];
        drop(ctx.run(raw, |ctx| {
            let props = PaneTabBarsProps {
                theme: &theme,
                kb: &kb,
                panes: &panes,
                scale_factor: 1.0,
                tab_width: 160.0,
                tab_font_size: 12.0,
                active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                drag: None,
                switch_overlay_pane: None,
                native_content: &[],
            };
            actions.extend(draw_pane_tab_bars_view(ctx, &props).actions);
        }));
    }

    /// 분홍 화살표 클릭은 한 칸 스크롤만 낸다. 대상으로 건너뛰거나 포커스를 옮기지 않는다.
    #[test]
    fn clicking_the_pink_arrow_is_the_normal_step_scroll() {
        let ctx = egui::Context::default();
        let pane = marked_pane(TabMoveMark::Ring, 7, 0.0);
        let bar_h = test_theme().tab_bar_height.value();
        let right = egui::pos2(arrow_centers_x()[1], bar_h / 2.0);
        let mut actions = Vec::new();
        // Area가 크기를 재고 페이드인을 마칠 때까지 입력 없이 그린 뒤 누르고 뗀다.
        let steps =
            std::iter::repeat_n(None, 30).chain([Some(None), Some(Some(true)), Some(Some(false))]);
        for step in steps {
            let mut raw = egui::RawInput::default();
            let Some(pressed) = step else {
                run_frame(&ctx, &pane, raw, &mut Vec::new());
                continue;
            };
            raw.events.push(egui::Event::PointerMoved(right));
            if let Some(pressed) = pressed {
                raw.events.push(egui::Event::PointerButton {
                    pos: right,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            run_frame(&ctx, &pane, raw, &mut actions);
        }
        let non_auto: Vec<_> = actions
            .into_iter()
            .filter(|a| !matches!(a, TabBarAction::AutoScrollToActiveTab { .. }))
            .collect();
        assert_eq!(non_auto, vec![TabBarAction::ScrollRight { pane_id: 1 }]);
    }

    fn cluster_sizes() -> tab::ClusterSizes {
        tab::ClusterSizes {
            close: tasty_type_geometry::length::LogicalPx(16.0),
            busy: tasty_type_geometry::length::LogicalPx(6.0),
            move_glyph: tasty_type_geometry::length::LogicalPx(12.0),
            marker: tasty_type_geometry::length::LogicalPx(16.0),
            gap: tasty_type_geometry::length::LogicalPx(4.0),
            label_gap: tasty_type_geometry::length::LogicalPx(8.0),
        }
    }

    fn range(a: f32, b: f32) -> egui::Rangef {
        egui::Rangef::new(a, b)
    }

    #[test]
    fn the_status_cluster_orders_marker_move_busy_close_four_apart() {
        let all = tab::ClusterItems {
            marker: true,
            move_glyph: true,
            busy: true,
        };
        let slots = tab::status_cluster(
            tasty_type_geometry::length::LogicalPx(200.0),
            cluster_sizes(),
            all,
        );
        assert_eq!(slots.close, range(184.0, 200.0));
        assert_eq!(slots.busy, Some(range(174.0, 180.0)));
        assert_eq!(slots.move_glyph, Some(range(158.0, 170.0)));
        assert_eq!(slots.marker, Some(range(138.0, 154.0)));
        // 묶음 왼쪽 끝 138에서 tab-gap 8을 띄운다.
        assert_eq!(slots.label_right.value(), 130.0);
    }

    /// close 칸은 항상 자리를 잡는다. 없는 항목은 칸도 간격도 차지하지 않는다.
    #[test]
    fn the_cluster_reserves_close_and_only_the_items_present() {
        let right = tasty_type_geometry::length::LogicalPx(200.0);
        let none = tab::status_cluster(right, cluster_sizes(), tab::ClusterItems::default());
        assert_eq!(none.close, range(184.0, 200.0));
        assert_eq!(
            (none.busy, none.move_glyph, none.marker),
            (None, None, None)
        );
        assert_eq!(none.label_right.value(), 176.0);
        let marker_only = tab::status_cluster(
            right,
            cluster_sizes(),
            tab::ClusterItems {
                marker: true,
                ..Default::default()
            },
        );
        assert_eq!(marker_only.marker, Some(range(164.0, 180.0)));
        assert_eq!(marker_only.label_right.value(), 156.0);
    }

    /// 좁은 탭에서 모든 항목이 있는 묶음은 크기를 지키고 긴 제목만 말줄임된다.
    #[test]
    fn a_narrow_tab_ellipsises_the_label_and_keeps_the_cluster() {
        let th = test_theme();
        let long = "quarterly-report-final.html";
        let mut pane = mk_pane(1, &[long, "b"], 1, true);
        pane.tab_is_busy[0] = true;
        pane.move_mark = Some((0, TabMoveMark::Glyph));
        pane.tab_html_script_marker[0] = Some(TabScriptMarker {
            surface_id: 7,
            kind: tasty_ui_widgets::HtmlScriptMarkerKind::Blocked,
            web: false,
        });
        let shapes = settled_strip_shapes(pane);
        let label = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text().starts_with("quarterly") => Some(t),
                _ => None,
            })
            .expect("제목이 칠해진다");
        assert!(
            label.galley.text().ends_with('…'),
            "{}",
            label.galley.text()
        );
        // 탭 폭 160의 오른쪽 끝 space-xs 안쪽부터 close · busy · move · 표지와 간격 셋을 뺀 곳이
        // 묶음 왼쪽 끝이고, 제목은 그보다 tab-gap 앞에서 끝난다.
        let gap = th.tab_status_gap().value();
        let busy_right = 160.0 - th.spacing_xs.value() - th.tab_close_size().value() - gap;
        let busy_center = busy_right - th.tab_dot_size().value() / 2.0;
        let cluster_left = busy_right
            - th.tab_dot_size().value()
            - gap
            - tasty_ui_widgets::move_source_glyph_size(&th)
            - gap
            - th.html_script_marker_hit().value();
        let label_right = label.pos.x + label.galley.size().x;
        assert!(
            label_right <= cluster_left - th.tab_gap().value() + 0.01,
            "제목 {label_right} 이 묶음 자리 {cluster_left} 를 침범한다"
        );
        let dot = shapes.iter().any(|s| match s {
            egui::Shape::Circle(c) => {
                (c.center.x - busy_center).abs() < 0.01
                    && (c.radius * 2.0 - th.tab_dot_size().value()).abs() < 0.01
            }
            _ => false,
        });
        assert!(dot, "busy 점이 제 크기로 제 칸에 있다");
    }

    /// 비활성 탭의 lock 칸 위에 포인터가 있어도 탭 hover라 close가 보인다.
    #[test]
    fn hovering_the_lock_of_an_inactive_tab_still_shows_close() {
        let th = test_theme();
        let pane = marker_pane(tasty_ui_widgets::HtmlScriptMarkerKind::Blocked);
        let slots = tab::status_cluster(
            tasty_type_geometry::length::LogicalPx(160.0) - th.spacing_xs,
            tab::ClusterSizes::of(&th),
            tab::ClusterItems {
                marker: true,
                ..Default::default()
            },
        );
        let y = th.tab_bar_height.value() / 2.0;
        let close_glyph_at = |shapes: &[egui::Shape]| {
            let size = th.icon_glyph_size_xs.value();
            shapes.iter().any(|s| match s {
                egui::Shape::Rect(r) => {
                    r.brush.is_some()
                        && (r.rect.width() - size).abs() < 0.01
                        && (r.rect.center().x - slots.close.center()).abs() < 0.01
                }
                _ => false,
            })
        };
        let on_lock = egui::pos2(slots.marker.expect("표지 칸").center(), y);
        assert!(close_glyph_at(&hovered_strip_shapes(&pane, on_lock)));
        // 탭 밖(두 번째 탭 위)이면 첫 탭의 close는 숨는다.
        assert!(!close_glyph_at(&hovered_strip_shapes(
            &pane,
            egui::pos2(240.0, y)
        )));
    }

    /// 포인터를 `pos`에 둔 채 여러 프레임 그린 뒤 마지막 프레임의 도형을 돌려준다.
    fn hovered_strip_shapes(pane: &PaneTabBarView, pos: egui::Pos2) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let theme = test_theme();
        let kb = crate::settings::KeybindingSettings::default();
        let panes = vec![pane.clone()];
        let mut out = egui::FullOutput::default();
        for _ in 0..30 {
            let raw = egui::RawInput {
                events: vec![egui::Event::PointerMoved(pos)],
                ..Default::default()
            };
            out = ctx.run(raw, |ctx| {
                let props = PaneTabBarsProps {
                    theme: &theme,
                    kb: &kb,
                    panes: &panes,
                    scale_factor: 1.0,
                    tab_width: 160.0,
                    tab_font_size: 12.0,
                    active_tab_indicator: crate::settings::ActiveTabIndicator::default(),
                    drag: None,
                    switch_overlay_pane: None,
                    native_content: &[],
                };
                drop(draw_pane_tab_bars_view(ctx, &props));
            });
        }
        let mut shapes = Vec::new();
        fn flatten(s: egui::Shape, out: &mut Vec<egui::Shape>) {
            match s {
                egui::Shape::Vec(v) => v.into_iter().for_each(|s| flatten(s, out)),
                other => out.push(other),
            }
        }
        out.shapes
            .into_iter()
            .for_each(|c| flatten(c.shape, &mut shapes));
        shapes
    }

    /// 탭 폭 160·스크롤 160이면 가려진 활성 탭 0의 닫기 칸 [4, 20]이 왼쪽 화살표 칸 [0, 24] 안에 놓인다.
    /// 화살표를 누르면 한 칸 스크롤만 나오고 가려진 탭은 닫히지 않는다.
    #[test]
    fn the_left_arrow_click_does_not_close_the_tab_hidden_under_it() {
        let ctx = egui::Context::default();
        let pane = scrolling_pane(160.0);
        let bar_h = test_theme().tab_bar_height.value();
        let left = egui::pos2(arrow_centers_x()[0], bar_h / 2.0);
        let mut actions = Vec::new();
        let steps =
            std::iter::repeat_n(None, 30).chain([Some(None), Some(Some(true)), Some(Some(false))]);
        for step in steps {
            let mut raw = egui::RawInput::default();
            let Some(pressed) = step else {
                run_frame(&ctx, &pane, raw, &mut Vec::new());
                continue;
            };
            raw.events.push(egui::Event::PointerMoved(left));
            if let Some(pressed) = pressed {
                raw.events.push(egui::Event::PointerButton {
                    pos: left,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            run_frame(&ctx, &pane, raw, &mut actions);
        }
        let non_auto: Vec<_> = actions
            .into_iter()
            .filter(|a| !matches!(a, TabBarAction::AutoScrollToActiveTab { .. }))
            .collect();
        assert_eq!(non_auto, vec![TabBarAction::ScrollLeft { pane_id: 1 }]);
    }
}
