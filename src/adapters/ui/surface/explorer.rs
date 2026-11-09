//! 파일 탐색기 렌더링. 모델의 탐색 상태와 ExplorerView의 목록·선택으로 화면을 그린다.
//! 렌더 중에는 engine을 다시 가변 대여할 수 없어 사용자 동작을 모아 호출부에서 처리한다.

pub mod address;
mod commands;
pub(crate) mod create;
pub(crate) mod display_text;
mod enclosing;
mod entry_style;
mod find;
mod kind;
mod link;
mod preview;
pub(crate) mod props_time;
mod state_screen;
mod thumbs;
pub mod type_ahead;
pub mod view;
mod visible_rows;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tasty_type_geometry::length::LogicalPx;

use tasty_model::{ExplorerPanel, ExplorerViewMode, SortColumn, SortDir};
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{Table, TableSortDir, tree_row, tree_row_cut, tree_row_with_tail_fg};

use crate::adapters::ui::icons::{self, Icon};
use crate::core::explorer_favorites as favorites;
pub(crate) use crate::core::file_kind::is_image_ext;
use crate::i18n::{t, t_fmt};
use crate::settings::EffectiveFont;
use crate::theme;
pub(crate) use kind::{file_kind_word, kind_word};
use view::{DirEntryInfo, ExplorerView, LoadState, human_size};
use visible_rows::{GridMetrics, grid_metrics, open_span, scroll_target};

/// grid 셀 폭.
const CELL_W: LogicalPx = LogicalPx(80.0);
/// 사이드바 폭 (logical px — design `ExpSidebar` width 196).
const SIDEBAR_W: LogicalPx = LogicalPx(196.0);

mod columns;
mod favorites_pin;

/// `draw_explorer` 가 호스트에 위임하는 액션. 렌더 루프 종료 후 적용된다.
#[derive(Clone, Debug)]
pub enum ExplorerAction {
    /// 파일 열기 (`DomainIntent::DispatchFile`).
    OpenFile(PathBuf),
    /// 활성 탭을 디렉토리로 이동.
    Navigate(PathBuf),
    /// 뒤로/앞으로/위로.
    GoBack,
    GoForward,
    GoUp,
    /// 현재 디렉토리 새로고침.
    Refresh,
    /// 표시 모드 변경.
    SetViewMode(ExplorerViewMode),
    /// detail 정렬 컬럼 클릭(같은 컬럼이면 방향 토글).
    SetSort(SortColumn),
    /// 내부 탭 추가.
    NewTab,
    /// 내부 탭 닫기.
    CloseTab(usize),
    /// 내부 탭 선택.
    SelectTab(usize),
    /// 주소창 입력을 이동하지 않았다. 호스트가 이유를 알린다.
    AddressRejected(address::AddressRejection),
    /// 우클릭 컨텍스트 메뉴 요청 — 호스트가 OS 네이티브 메뉴를 띄운다.
    /// 좌표는 logical px (egui interact pos 기준).
    ContextMenu {
        target: ExplorerMenuTarget,
        /// 현재 디렉토리 (빈 영역 대상의 "경로 복사"·붙여넣기 기준).
        cwd: PathBuf,
        x: f32,
        y: f32,
    },
    /// 이름을 확정한 새 항목을 만든다. 대상 폴더는 명령을 시작할 때 정했다.
    Create {
        dir: PathBuf,
        name: String,
        folder: bool,
    },
    /// 좁은 칸에서 접은 명령 묶음의 메뉴. 좌표는 logical px.
    MoreMenu {
        x: f32,
        y: f32,
    },
    /// 파일 작업 표시(진행·대기열·결과 카드)에서 고른 일.
    Ops(view::ops::OpsAction),
}

/// 컨텍스트 메뉴의 대상 — 우클릭 위치/선택 상태에서 결정 (design §3.3 target rule).
#[derive(Clone, Debug)]
pub enum ExplorerMenuTarget {
    /// 빈 영역 → 현재 디렉토리(cwd) 대상.
    Empty,
    /// 단일 파일/폴더.
    Single { path: PathBuf, is_dir: bool },
    /// 다중 선택.
    Multi { paths: Vec<PathBuf> },
    /// 사이드바 즐겨찾기 항목 → "즐겨찾기에서 제거" 전용 메뉴.
    Favorite { path: PathBuf },
}

/// 타입어헤드가 키를 소비해도 되는지 판단하는 데 필요한, 탐색기 바깥에서 오는 값들.
/// 렌더 루프 안에서는 `state`와 `engine`을 읽을 수 없으므로 프레임마다 한 번 계산해
/// 넘긴다. `egui_panels`에서 즐겨찾기와 잘라내기 목록을 미리 꺼내 두는 것과 같은
/// 이유다.
pub struct ExplorerInput<'a> {
    /// 이 surface가 현재 포커스를 가지고 있는지. egui 이벤트 큐는 전역이라, 이 조건이
    /// 없으면 한 번의 입력이 열려 있는 모든 탐색기의 선택을 동시에 움직인다.
    pub focused: bool,
    /// 팝업·모달·입력 다이얼로그가 떠 있는지. 그런 상태에서도 키 입력은 egui로 들어오므로
    /// (`view::main`의 전달 조건이 `overlay_open || egui_surface`다) 여기서 막지 않으면
    /// 뒤에 있는 탐색기의 선택이 사용자 모르게 움직인다.
    pub overlay_open: bool,
    /// 수식 키 없이 단축키로 등록된 영숫자. 그 글자는 단축키가 가져간다
    /// (`type_ahead::unmodified_binding_chars`).
    pub shortcut_chars: &'a HashSet<char>,
}

/// 한 explorer surface 를 그린다. 사용자 조작이 있었으면 첫 액션을 반환.
#[allow(clippy::too_many_arguments)]
pub fn draw_explorer(
    ui: &mut egui::Ui,
    panel: &ExplorerPanel,
    view: &mut ExplorerView,
    font: &EffectiveFont,
    id_suffix: &str,
    favorites: &[favorites::ExplorerFavorite],
    cut_pending: &HashSet<PathBuf>,
    recent_dirs: &[String],
    mirror_ws_id: Option<u32>,
    input: &ExplorerInput<'_>,
) -> Option<ExplorerAction> {
    let th = theme::theme();
    let theme: &Theme = &th;
    let mut action: Option<ExplorerAction> = None;

    apply_type_ahead(ui, view, input);

    ui.set_min_size(ui.available_size());
    // 하위 위젯이 처리하지 않은 우클릭은 탐색기 전체 영역에서 받는다.
    let surface_rect = ui.max_rect();
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

    ui.vertical(|ui| {
        tab_strip(ui, theme, panel, &mut action);
        let remote = mirror_ws_id.is_some();
        toolbar(
            ui,
            theme,
            panel,
            view,
            id_suffix,
            recent_dirs,
            remote,
            &mut action,
        );
        find::bar(ui, theme, view, remote);
        let (sep_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), theme.border_width.value()),
            egui::Sense::hover(),
        );
        ui.painter().hline(
            sep_rect.x_range(),
            sep_rect.center().y,
            egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
        );

        let (mut side_ui, line, mut body_ui) = columns::split(ui, SIDEBAR_W, theme.border_width);
        sidebar(
            &mut side_ui,
            theme,
            panel,
            view,
            favorites,
            &mut action,
            mirror_ws_id,
        );
        ui.painter().vline(
            line.center().x,
            line.y_range(),
            egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
        );
        content(
            &mut body_ui,
            theme,
            panel,
            view,
            font,
            id_suffix,
            cut_pending,
            &mut action,
        );
    });

    // 탐색기 빈 영역에서도 탐색기 메뉴를 사용한다. 하위 위젯이 처리한 클릭은 건드리지 않는다.
    // 권한 거부 경로는 붙여넣을 수 없어 제외한다.
    if action.is_none() && !matches!(view.state, LoadState::NoPermission) {
        let pos = ui.input(|i| {
            if i.pointer.secondary_clicked() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });
        if let Some(pos) = pos
            && surface_rect.contains(pos)
        {
            action = Some(ExplorerAction::ContextMenu {
                target: ExplorerMenuTarget::Empty,
                cwd: panel.current_root().to_path_buf(),
                x: pos.x,
                y: pos.y,
            });
        }
    }

    action
}

fn tab_strip(
    ui: &mut egui::Ui,
    theme: &Theme,
    panel: &ExplorerPanel,
    action: &mut Option<ExplorerAction>,
) {
    let bar_h = theme.item_height_tab.value();
    let pad_x = theme.spacing_sm.value();
    let gap = theme.spacing_xs.value();
    let icon_xs = theme.icon_glyph_size_xs.value();
    let font = egui::FontId::proportional(theme.font_size_body.value());
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, bar_h), egui::Sense::hover());
    let p = ui.painter_at(rect);

    p.rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    p.hline(
        rect.x_range(),
        rect.max.y - theme.border_width.value() * 0.5,
        egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
    );

    let active = panel.active;
    let mut x = rect.min.x;
    for (i, tab) in panel.tabs.iter().enumerate() {
        let is_active = i == active;
        // 탭 라벨은 고정 cwd(프로젝트) 이름 — 현재 폴더는 주소창(PathField)이 보여준다.
        let label = tab
            .cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| tab.cwd.to_string_lossy().to_string());
        let label = display_text::nfc(&label).into_owned();
        let galley =
            ui.fonts(|f| f.layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE));
        let tab_w = pad_x + icon_xs + gap + galley.size().x + gap + icon_xs + pad_x;
        let tab_rect =
            egui::Rect::from_min_size(egui::pos2(x, rect.min.y), egui::vec2(tab_w, bar_h));
        let resp = ui.interact(
            tab_rect,
            ui.id().with(("explorer_tab", i)),
            egui::Sense::click(),
        );

        if i > 0 && !is_active {
            ui.painter().vline(
                x,
                tab_rect.y_range(),
                egui::Stroke::new(
                    theme.border_width.value(),
                    theme.separator.to_egui_premultiplied(),
                ),
            );
        }

        if is_active {
            ui.painter()
                .rect_filled(tab_rect, 0.0, theme.bg_panel().to_egui());
            let indicator = egui::Rect::from_min_size(
                tab_rect.min,
                egui::vec2(tab_w, theme.tab_indicator_width().value()),
            );
            ui.painter()
                .rect_filled(indicator, 0.0, theme.accent_primary().to_egui());
        } else if resp.hovered() {
            ui.painter()
                .rect_filled(tab_rect, 0.0, theme.overlay_hover().to_egui_premultiplied());
        }

        let fg = if is_active {
            theme.text_primary().to_egui()
        } else {
            theme.text_muted().to_egui()
        };
        let folder_rect = egui::Rect::from_min_size(
            egui::pos2(tab_rect.min.x + pad_x, tab_rect.center().y - icon_xs / 2.0),
            egui::vec2(icon_xs, icon_xs),
        );
        icons::FOLDER
            .image(icon_xs, theme.text_muted().to_egui())
            .paint_at(ui, folder_rect);
        ui.painter().text(
            egui::pos2(tab_rect.min.x + pad_x + icon_xs + gap, tab_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &label,
            font.clone(),
            fg,
        );

        // close ✕ — 활성 상시 / 비활성 hover. 탭이 1개면 표시하지 않음.
        if panel.tabs.len() > 1 && (is_active || resp.hovered()) {
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(
                    tab_rect.max.x - pad_x - icon_xs,
                    tab_rect.center().y - icon_xs / 2.0,
                ),
                egui::vec2(icon_xs, icon_xs),
            );
            let close_resp = ui.interact(
                close_rect,
                ui.id().with(("explorer_tab_close", i)),
                egui::Sense::click(),
            );
            icons::CLOSE
                .image(icon_xs, theme.text_muted().to_egui())
                .paint_at(ui, close_rect);
            if close_resp.clicked() {
                *action = Some(ExplorerAction::CloseTab(i));
            }
        }

        if resp.clicked() && action.is_none() && !is_active {
            *action = Some(ExplorerAction::SelectTab(i));
        }
        x += tab_w;
    }

    let plus_rect = egui::Rect::from_min_size(egui::pos2(x, rect.min.y), egui::vec2(bar_h, bar_h));
    let plus_resp = ui.interact(
        plus_rect,
        ui.id().with("explorer_tab_new"),
        egui::Sense::click(),
    );
    if plus_resp.hovered() {
        ui.painter().rect_filled(
            plus_rect,
            0.0,
            theme.overlay_hover().to_egui_premultiplied(),
        );
    }
    let icon = theme.icon_glyph_size_md.value();
    let icon_rect = egui::Rect::from_center_size(plus_rect.center(), egui::vec2(icon, icon));
    icons::PLUS
        .image(icon, theme.text_secondary().to_egui())
        .paint_at(ui, icon_rect);
    if plus_resp.clicked() && action.is_none() {
        *action = Some(ExplorerAction::NewTab);
    }
}

#[allow(clippy::too_many_arguments)]
fn toolbar(
    ui: &mut egui::Ui,
    theme: &Theme,
    panel: &ExplorerPanel,
    view: &mut ExplorerView,
    id_suffix: &str,
    recent_dirs: &[String],
    remote: bool,
    action: &mut Option<ExplorerAction>,
) {
    let h = theme.item_height_interactive.value() + theme.spacing_sm.value() * 2.0;
    let pad = theme.spacing_sm.value();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());

    let inner = rect.shrink2(egui::vec2(pad, pad));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let tab = panel.active_tab();
    child.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();

        if tool_icon(
            ui,
            theme,
            icons::CHEVRON_LEFT,
            tab.can_go_back(),
            t("explorer.nav.back"),
        ) && action.is_none()
        {
            *action = Some(ExplorerAction::GoBack);
        }
        if tool_icon(
            ui,
            theme,
            icons::CHEVRON_RIGHT,
            tab.can_go_forward(),
            t("explorer.nav.forward"),
        ) && action.is_none()
        {
            *action = Some(ExplorerAction::GoForward);
        }
        if tool_icon(
            ui,
            theme,
            icons::CHEVRON_UP,
            tab.can_go_up(),
            t("explorer.nav.up"),
        ) && action.is_none()
        {
            *action = Some(ExplorerAction::GoUp);
        }
        if tool_icon(ui, theme, icons::REFRESH, true, t("explorer.nav.refresh")) && action.is_none()
        {
            *action = Some(ExplorerAction::Refresh);
        }

        ui.add_space(theme.spacing_sm.value());

        // 보기 모드 버튼 폭을 먼저 확보하고 남은 폭에 주소창을 제한한다.
        let seg_w = seg_toggle_width(theme);
        let gap = theme.spacing_sm.value();
        // 주소창 영역 뒤에 가로 item_spacing이 한 번 더 붙으므로 그만큼도 빼야 토글의 오른쪽 여백이
        // 툴바 padding과 같아진다.
        let cmd_w = commands::reserve(ui, theme, view, rect.width());
        let addr_w =
            (ui.available_width() - seg_w - gap - ui.spacing().item_spacing.x - cmd_w).max(0.0);
        let tab_index = view.tab_salt();
        ui.allocate_ui_with_layout(
            egui::vec2(addr_w, ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                address::field::address_bar(
                    ui,
                    theme,
                    panel.current_root(),
                    view,
                    id_suffix,
                    tab_index,
                    recent_dirs,
                    remote,
                    action,
                )
            },
        );
        ui.add_space(gap);
        commands::show(ui, theme, view, &tab.root, rect.width(), action);
        seg_toggle(ui, theme, tab.view_mode, action);
    });
}

/// view-mode 아이콘 토글의 총 폭(예약용). design SegToggle: pad + 3seg + 2gap + 2border.
fn seg_toggle_width(theme: &Theme) -> f32 {
    let pad = theme.spacing_xs.value();
    let gap = theme.spacing_xs.value();
    let seg = theme.icon_glyph_size_md.value() + theme.spacing_sm.value();
    pad * 2.0 + seg * 3.0 + gap * 2.0 + theme.border_width.value() * 2.0
}

/// grid/list/detail 아이콘 토글 (design `SegToggle`): 컨테이너 surface-raised +
/// border-default 1px + radius, active 세그먼트 = segtoggle-on-bg 채움 + segtoggle-on-fg,
/// inactive = text-muted. tooltip 은 i18n 라벨(텍스트 라벨 제거 대신 aria/tooltip 유지).
fn seg_toggle(
    ui: &mut egui::Ui,
    theme: &Theme,
    mode: ExplorerViewMode,
    action: &mut Option<ExplorerAction>,
) {
    let pad = theme.spacing_xs.value();
    let gap = theme.spacing_xs.value();
    let h = theme.item_height_interactive.value();
    let seg_w = theme.icon_glyph_size_md.value() + theme.spacing_sm.value();
    let icon = theme.icon_glyph_size_md.value();
    let total_w = seg_toggle_width(theme);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect(
        rect,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    let segs = [
        (ExplorerViewMode::Grid, icons::GRID, "explorer.view.grid"),
        (
            ExplorerViewMode::List,
            icons::LIST_VIEW,
            "explorer.view.list",
        ),
        (
            ExplorerViewMode::Detail,
            icons::DETAIL,
            "explorer.view.detail",
        ),
    ];
    let seg_h = h - pad * 2.0;
    let mut sx = rect.min.x + theme.border_width.value() + pad;
    for (m, ic, key) in segs {
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(sx, rect.center().y - seg_h / 2.0),
            egui::vec2(seg_w, seg_h),
        );
        let resp = ui
            .interact(
                seg_rect,
                ui.id().with(("exp_seg", key)),
                egui::Sense::click(),
            )
            .on_hover_text(t(key));
        let active = m == mode;
        if active {
            ui.painter().rect_filled(
                seg_rect,
                theme.corner_radius_sm.value(),
                theme.segtoggle_on_bg().to_egui(),
            );
        } else if resp.hovered() {
            ui.painter().rect_filled(
                seg_rect,
                theme.corner_radius_sm.value(),
                theme.overlay_hover().to_egui_premultiplied(),
            );
        }
        let fg = if active {
            theme.segtoggle_on_fg().to_egui()
        } else {
            theme.text_muted_raised().to_egui()
        };
        let ir = egui::Rect::from_center_size(seg_rect.center(), egui::vec2(icon, icon));
        ic.image(icon, fg).paint_at(ui, ir);
        if resp.clicked() && !active && action.is_none() {
            *action = Some(ExplorerAction::SetViewMode(m));
        }
        sx += seg_w + gap;
    }
}

#[allow(clippy::too_many_arguments)]
fn sidebar(
    ui: &mut egui::Ui,
    theme: &Theme,
    panel: &ExplorerPanel,
    view: &mut ExplorerView,
    favorites: &[favorites::ExplorerFavorite],
    action: &mut Option<ExplorerAction>,
    mirror_ws_id: Option<u32>,
) {
    let full = ui.available_size();
    ui.painter().rect_filled(
        egui::Rect::from_min_size(ui.cursor().min, full),
        0.0,
        theme.bg_sidebar().to_egui(),
    );
    view.ops.drag.reserve_sidebar(ui);
    ui.spacing_mut().item_spacing.y = 0.0;

    let current = panel.current_root().to_path_buf();

    // 트리와 하단 즐겨찾기에 독립 스크롤 영역을 주고 즐겨찾기 높이를 먼저 확보한다.
    // 낮은 본문에서는 즐겨찾기를 빼고 트리가 본문 전체를 쓴다.
    let fav_h = favorites_pin::height(theme, full.y);
    let files_h = fav_h.map_or(full.y, |h| {
        (full.y - h - theme.border_width.value()).max(0.0)
    });

    ui.allocate_ui_with_layout(
        egui::vec2(full.x, files_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.add_space(theme.spacing_xs.value());
            sidebar_caption(ui, theme, t("explorer.sidebar.tree"));
            let root = panel.cwd().to_path_buf();
            let output = egui::ScrollArea::vertical()
                .id_salt(("explorer_sidebar_files", view.tab_salt()))
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    tree_node(ui, theme, view, &root, 0, &current, action, mirror_ws_id);
                });
            self::view::drag_scroll::feed(
                ui,
                theme,
                &view.ops.drag,
                &output,
                theme.tree_row_height(),
            );
        },
    );
    let Some(fav_h) = fav_h else {
        return;
    };

    let (sep, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        sep.x_range(),
        sep.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );

    ui.allocate_ui_with_layout(
        egui::vec2(full.x, fav_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            sidebar_caption(ui, theme, t("explorer.sidebar.favorites"));
            egui::ScrollArea::vertical()
                .id_salt("explorer_sidebar_favorites")
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    if favorites.is_empty() {
                        favorites_empty(ui, theme);
                    } else {
                        for fav in favorites {
                            favorite_row(ui, theme, fav, &current, action);
                        }
                    }
                });
        },
    );
}

/// 즐겨찾기 빈 상태 (design `FavoritesEmpty`): 흐린 별 + "No favorites yet" + 힌트.
fn favorites_empty(ui: &mut egui::Ui, theme: &Theme) {
    let inset = theme.spacing_sm.value();
    ui.add_space(theme.spacing_xs.value());
    ui.horizontal(|ui| {
        ui.add_space(inset);
        ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
        let sz = theme.icon_glyph_size_sm.value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
        // 즐겨찾기 별 아이콘 톤. 대응 토큰 없음 — 같은 아이콘이 두 곳에서
        // 서로 다른 값을 쓴다(수렴은 디자인 판단).
        const FAV_STAR_ICON_OPACITY: f32 = 0.55;
        icons::STAR
            .image(
                sz,
                theme
                    .text_muted()
                    .to_egui()
                    .gamma_multiply(FAV_STAR_ICON_OPACITY),
            )
            .paint_at(ui, r);
        ui.label(
            egui::RichText::new(t("explorer.sidebar.favorites_empty"))
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space(inset);
        ui.spacing_mut().item_spacing.x = 0.0;
        let hint = t_fmt(
            "explorer.sidebar.favorites_empty_hint",
            t("explorer.context_menu.add_to_favorites"),
        );
        let action_label = t("explorer.context_menu.add_to_favorites");
        let micro = theme.font_size_caption.value();
        if let Some(pos) = hint.find(action_label) {
            let (before, rest) = hint.split_at(pos);
            let after = &rest[action_label.len()..];
            for (seg, muted) in [(before, false), (action_label, true), (after, false)] {
                if seg.is_empty() {
                    continue;
                }
                let color = if muted {
                    theme.text_muted().to_egui()
                } else {
                    theme.text_placeholder().to_egui()
                };
                ui.label(egui::RichText::new(seg).size(micro).color(color));
            }
        } else {
            ui.label(
                egui::RichText::new(hint)
                    .size(micro)
                    .color(theme.text_placeholder().to_egui()),
            );
        }
    });
    ui.add_space(inset);
}

/// 즐겨찾기 한 행. 클릭 → 해당 경로로 이동, 우클릭 → 컨텍스트 메뉴.
/// 별은 채운 별(STAR_FILL) + accent-warning(골드), 현재 폴더면 surface-active 하이라이트.
fn favorite_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    fav: &favorites::ExplorerFavorite,
    current: &Path,
    action: &mut Option<ExplorerAction>,
) {
    let star = icons::STAR_FILL;
    let star_color = theme.accent_warning_glyph().to_egui();
    let selected = fav.path == current;
    let resp = tree_row(
        ui,
        theme,
        0,
        false,
        false,
        Some(&|ui, rect, _c| star.image(rect.height(), star_color).paint_at(ui, rect)),
        &fav.label,
        None,
        selected,
    );
    self::view::drag::note_favorite(ui, &fav.path, resp.rect);
    if resp.clicked() && action.is_none() {
        *action = Some(ExplorerAction::Navigate(fav.path.clone()));
    }
    if resp.secondary_clicked() && action.is_none() {
        let pos = ui
            .input(|i| i.pointer.interact_pos())
            .unwrap_or_else(|| resp.rect.center());
        *action = Some(ExplorerAction::ContextMenu {
            target: ExplorerMenuTarget::Favorite {
                path: fav.path.clone(),
            },
            cwd: fav.path.clone(),
            x: pos.x,
            y: pos.y,
        });
    }
}

fn sidebar_caption(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add_space(theme.spacing_xs.value());
    ui.horizontal(|ui| {
        ui.add_space(theme.spacing_sm.value());
        ui.label(
            egui::RichText::new(text.to_uppercase())
                .font(tasty_ui_widgets::label_font(theme.font_size_micro.value()))
                .color(theme.text_muted().to_egui()),
        );
    });
    ui.add_space(theme.spacing_xs.value());
}

/// 재귀 트리 노드. `dir` 자체 행을 그리고, 펼쳐져 있으면 하위 디렉토리도.
/// `current` 와 같은 노드는 surface-active 로 하이라이트(design TreeNode active).
#[allow(clippy::too_many_arguments)]
fn tree_node(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    dir: &Path,
    depth: u16,
    current: &Path,
    action: &mut Option<ExplorerAction>,
    mirror_ws_id: Option<u32>,
) {
    let open = view.expanded.contains(dir);
    let has_children = !view.tree_children_of(dir, mirror_ws_id).is_empty();
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.to_string_lossy().to_string());
    let folder = icons::FOLDER;
    let folder_color = theme.text_muted().to_egui();
    let selected = dir == current;
    let resp = tree_row(
        ui,
        theme,
        depth,
        has_children,
        open,
        Some(&|ui, rect, _c| folder.image(rect.height(), folder_color).paint_at(ui, rect)),
        &name,
        None,
        selected,
    );
    self::view::drag::note_tree(ui, dir, resp.rect, open);
    // chevron 영역은 펼치기, 나머지 영역은 해당 경로로 이동한다.
    if resp.clicked() {
        let toggle_zone = resp.rect.left() + depth as f32 * theme.spacing_md.value() + 24.0;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let is_toggle = has_children && pointer.map(|p| p.x <= toggle_zone).unwrap_or(false);
        if is_toggle {
            if open {
                view.expanded.remove(dir);
            } else {
                view.expanded.insert(dir.to_path_buf());
            }
        } else if action.is_none() {
            *action = Some(ExplorerAction::Navigate(dir.to_path_buf()));
        }
    }
    // 트리 우클릭은 본문 선택을 바꾸지 않고 해당 폴더의 메뉴를 연다.
    if resp.secondary_clicked() && action.is_none() {
        let pos = ui
            .input(|i| i.pointer.interact_pos())
            .unwrap_or_else(|| resp.rect.center());
        *action = Some(ExplorerAction::ContextMenu {
            target: ExplorerMenuTarget::Single {
                path: dir.to_path_buf(),
                is_dir: true,
            },
            cwd: dir.to_path_buf(),
            x: pos.x,
            y: pos.y,
        });
    }
    if open {
        let children: Vec<PathBuf> = view
            .tree_children_of(dir, mirror_ws_id)
            .iter()
            .map(|e| e.path.clone())
            .collect();
        for child in children {
            tree_node(
                ui,
                theme,
                view,
                &child,
                depth + 1,
                current,
                action,
                mirror_ws_id,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn content(
    ui: &mut egui::Ui,
    theme: &Theme,
    panel: &ExplorerPanel,
    view: &mut ExplorerView,
    font: &EffectiveFont,
    id_suffix: &str,
    cut_pending: &HashSet<PathBuf>,
    action: &mut Option<ExplorerAction>,
) {
    let mode = panel.active_tab().view_mode;
    let root = panel.current_root().to_path_buf();

    let status_h = theme.item_height_interactive.value();
    let body_h = (ui.available_height() - status_h).max(0.0);
    let body = ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), body_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            let ui = &mut preview::split(ui, theme, view, id_suffix);
            let bed = tasty_ui_widgets::surface_bed(theme, view.bed_focused);
            ui.painter().rect_filled(ui.max_rect(), 0.0, bed);
            view.ops.drag.reserve_files(ui);
            if state_screen::show_for(ui, theme, view, &root, action)
                || state_screen::show_find(ui, theme, view, action)
            {
                return;
            }
            let row = match mode {
                ExplorerViewMode::Detail => theme.table_cell_height(),
                ExplorerViewMode::List => theme.tree_row_height(),
                ExplorerViewMode::Grid => {
                    LogicalPx(grid_metrics(theme, font).cell_h) + theme.spacing_md
                }
            };
            let output = egui::ScrollArea::vertical()
                .id_salt(format!("explorer_content_{id_suffix}_{}", view.tab_salt()))
                .auto_shrink([false, false])
                // 기본 최소 높이(64)는 낮은 칸에서 본문을 늘려 상태줄을 칸 밖으로 민다.
                .min_scrolled_height(0.0)
                .drag_to_scroll(false)
                .show(ui, |ui| match mode {
                    ExplorerViewMode::Grid => {
                        grid_view(ui, theme, view, font, cut_pending, &root, action)
                    }
                    ExplorerViewMode::List => {
                        list_view(ui, theme, view, cut_pending, &root, action)
                    }
                    ExplorerViewMode::Detail => detail_view(
                        ui,
                        theme,
                        panel,
                        view,
                        id_suffix,
                        cut_pending,
                        &root,
                        action,
                    ),
                });
            if mode != ExplorerViewMode::Detail {
                self::view::drag_scroll::feed(ui, theme, &view.ops.drag, &output, row);
            }
        },
    );

    // 빈 영역 우클릭 → cwd 메뉴 (권한 거부 상태는 제외 — 붙여넣기 불가).
    if !matches!(view.state, LoadState::NoPermission) {
        handle_background_context(ui, view, body.response.rect, &root, action);
    }
    self::view::ops::footer(ui, theme, view, body.response.rect, action, status_line);
}

fn status_line(ui: &mut egui::Ui, theme: &Theme, view: &ExplorerView) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), theme.item_height_interactive.value()),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
    );
    let text = view.status_text();
    let text = display_text::nfc(&text);
    ui.painter().text(
        egui::pos2(rect.left() + theme.spacing_md.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(theme.font_size_caption.value()),
        theme.text_muted().to_egui(),
    );
}

/// 우클릭 대상 확정 후 `ContextMenu` 액션을 만든다 (design §3.3 target rule):
/// 선택 밖 항목이면 그 항목만 선택, 선택 안이면 현재 선택 유지.
fn emit_entry_context(
    view: &mut ExplorerView,
    entry: &DirEntryInfo,
    pos: egui::Pos2,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) {
    if !view.selected.contains(&entry.path) {
        view.select_only(&entry.path);
    }
    let target = if view.selected.len() > 1 {
        let mut paths: Vec<PathBuf> = view.selected.owned().collect();
        paths.sort();
        ExplorerMenuTarget::Multi { paths }
    } else {
        ExplorerMenuTarget::Single {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
        }
    };
    if action.is_none() {
        *action = Some(ExplorerAction::ContextMenu {
            target,
            cwd: root.to_path_buf(),
            x: pos.x,
            y: pos.y,
        });
    }
}

/// grid/list 엔트리 우클릭 핸들러 (Response 기반). 처리했으면 true.
fn handle_entry_context(
    view: &mut ExplorerView,
    entry: &DirEntryInfo,
    resp: &egui::Response,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) -> bool {
    if !resp.secondary_clicked() {
        return false;
    }
    let pos = resp.interact_pointer_pos().unwrap_or_default();
    emit_entry_context(view, entry, pos, root, action);
    true
}

/// content 빈 영역 우클릭 → cwd 대상 메뉴 (선택 해제).
fn handle_background_context(
    ui: &egui::Ui,
    view: &mut ExplorerView,
    rect: egui::Rect,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) {
    if action.is_some() {
        return;
    }
    let pos = ui.input(|i| {
        if i.pointer.secondary_clicked() {
            i.pointer.interact_pos()
        } else {
            None
        }
    });
    if let Some(pos) = pos
        && rect.contains(pos)
    {
        view.clear_selection();
        *action = Some(ExplorerAction::ContextMenu {
            target: ExplorerMenuTarget::Empty,
            cwd: root.to_path_buf(),
            x: pos.x,
            y: pos.y,
        });
    }
}

/// 단일/토글/범위 선택 처리 (modifiers 반영). 더블클릭이면 폴더는 이동, 파일은 연다.
/// `..` 는 상위 이동만 하고 선택 대상이 아니다.
fn handle_entry_interaction(
    ui: &egui::Ui,
    view: &mut ExplorerView,
    entry: &DirEntryInfo,
    (clicked, dbl): (bool, bool),
    action: &mut Option<ExplorerAction>,
) {
    if dbl {
        if action.is_none() {
            *action = Some(view::keys::open_action(entry));
        }
    } else if clicked && entry.name != ".." {
        let mods = ui.input(|i| i.modifiers);
        view.click_select(&entry.path, mods.command || mods.ctrl, mods.shift);
    }
}

/// current 에 부모가 있으면(파일시스템 루트 아님) `..` 상위 이동 대상 경로.
fn parent_nav_target(current: &Path) -> Option<PathBuf> {
    current.parent().map(|p| p.to_path_buf())
}

/// 이번 프레임에 들어온 문자 입력을 타입어헤드로 처리해 선택할 항목과 스크롤 대상을
/// 정한다.
///
/// 이벤트를 큐에서 빼지는 않는다. 주소창의 `TextEdit`도 같은 큐를 읽지만, 주소창을
/// 편집 중일 때는 아래 `addr_editing` 조건이 이 함수 전체를 막으므로 입력이 두 번
/// 처리되지 않는다. 큐를 직접 건드리면 오히려 다른 위젯의 입력을 삼킬 수 있다.
fn apply_type_ahead(ui: &egui::Ui, view: &mut ExplorerView, input: &ExplorerInput<'_>) {
    // 스크롤 대상은 한 프레임만 유지한다. 남겨두면 매 프레임 다시 스크롤해서, 사용자가
    // 휠로 다른 곳을 보는 동안에도 화면이 끌려간다. 방금 만든 항목은 목록에 나타난 프레임에 한 번 스크롤한다.
    view.scroll_to = view.take_reveal();

    if !input.focused
        || input.overlay_open
        || view.text_input_active()
        || !matches!(view.state, LoadState::Ok)
        || view.entries.is_empty()
    {
        // 조건이 맞지 않는 프레임에서는 버퍼도 비운다. 그대로 두면 다시 돌아왔을 때
        // 예전 접두사가 이어져, 사용자가 입력하지 않은 글자로 검색하게 된다.
        view.type_ahead.reset();
        return;
    }

    view.refresh_filter();
    let mut type_ahead = std::mem::take(&mut view.type_ahead);
    let now = std::time::Instant::now();
    ui.input(|i| {
        for ch in i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Text(text) => type_ahead::type_ahead_char(text),
                _ => None,
            })
            .filter(|ch| !input.shortcut_chars.contains(&ch.to_ascii_lowercase()))
        {
            let selected = view.single_selected_index();
            if let Some(i) = type_ahead.feed_names(ch, now, view.shown_names(), selected) {
                let path = view.shown_at(i).expect("matched item exists").path.clone();
                view.select_only(&path);
                view.scroll_to = Some(path);
            }
        }
    });
    view.type_ahead = type_ahead;
}

/// 합성 `..` 엔트리. **렌더 전용** — `view.entries`/선택/상태줄/컨텍스트 메뉴에는 절대
/// 넣지 않는다. 각 뷰가 목록 앞에 특수 행으로 그리고 `Navigate(parent)` 만 emit 한다.
fn dotdot_entry(parent: PathBuf) -> DirEntryInfo {
    DirEntryInfo {
        path: parent,
        name: "..".to_string(),
        is_dir: true,
        size: 0,
        modified: None,
        ext: String::new(),
        link: Default::default(),
        os_hidden: false,
        change: None,
    }
}

/// Grid 칸 하나를 그릴 때 함께 넘기는 값.
#[derive(Clone, Copy)]
struct EntryCtx<'a> {
    theme: &'a Theme,
    cut_pending: &'a HashSet<PathBuf>,
    query: &'a str,
    root: &'a Path,
    metrics: GridMetrics,
    parent: tasty_ui_widgets::RowParent,
}

/// 첫 줄(`..`·이름 입력 칸과 그 뒤 항목)은 늘 그리고, 그 아래 줄은 화면에 걸친 줄만 그린다.
fn grid_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    font: &EffectiveFont,
    cut_pending: &HashSet<PathBuf>,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) {
    ui.add_space(theme.spacing_md.value());
    let gap = theme.spacing_md.value();
    let query = view.find_query().to_owned();
    let ctx = EntryCtx {
        theme,
        cut_pending,
        query: &query,
        root,
        metrics: grid_metrics(theme, font),
        parent: tasty_ui_widgets::RowParent::bed(view.bed_focused),
    };
    let parent = parent_nav_target(root).filter(|_| view.search_root().is_none());
    let lead = usize::from(parent.is_some());
    let at = create::place::slot_index(view);
    let count = view.shown_count() + usize::from(at.is_some());
    let cols = (((ui.available_width() + gap) / (CELL_W.value() + gap)).floor() as usize).max(1);
    let in_first = cols.saturating_sub(lead).min(count);
    let rest_lines = (count - in_first).div_ceil(cols);
    let target = scroll_target(view).map(|t| create::place::virtual_index(at, t));
    ui.spacing_mut().item_spacing = egui::vec2(gap, 0.0);
    view.note_grid(ui, (CELL_W.value(), ctx.metrics.cell_h), gap, (cols, lead));
    create::place::fit_layout(view, at, |l| l.lead += 1);

    ui.horizontal(|ui| {
        if let Some(p) = &parent {
            let (dd, up) = (dotdot_entry(p.clone()), EntryCtx { query: "", ..ctx });
            let tinted = view.ops.drag.drops_on(&dd.path);
            let resp = grid_cell(ui, &up, &dd, (false, false, tinted), None);
            if resp.double_clicked() && action.is_none() {
                *action = Some(ExplorerAction::Navigate(p.clone()));
            }
        }
        create::place::grid_run(ui, &ctx, view, at, 0..in_first, target, action);
    });

    let pitch = gap + ctx.metrics.cell_h;
    let target_line = target.and_then(|t| Some(t.checked_sub(in_first)? / cols));
    let span = open_span(ui, pitch, rest_lines, target_line);
    let lines = create::place::GridLines {
        at,
        in_first,
        cols,
        count,
        gap,
    };
    create::place::grid_lines(ui, &ctx, view, lines, span.clone(), action);
    ui.add_space((rest_lines - span.end) as f32 * pitch);
}

fn grid_entry(
    ui: &mut egui::Ui,
    ctx: &EntryCtx<'_>,
    view: &mut ExplorerView,
    e: &DirEntryInfo,
    scroll_here: bool,
    action: &mut Option<ExplorerAction>,
) {
    let cut = ctx.cut_pending.contains(&e.path);
    let under_drop = view.ops.drag.drops_on(&e.path);
    let state = (view.selected.contains(&e.path), cut, under_drop);
    let thumb = view.thumbs.texture(ui.ctx(), e, view.is_remote());
    // 보이는 칸만 그리므로 칸의 위젯 id 를 화면 위치가 아니라 항목 경로에 묶는다.
    // 그래야 스크롤해도 누름·hover 상태가 같은 항목에 남는다.
    let resp = ui
        .push_id(&e.path, |ui| grid_cell(ui, ctx, e, state, thumb.as_ref()))
        .inner;
    let resp = link::broken_tooltip(e, view.is_remote(), view.hit_tooltip(e, resp));
    if ui.is_rect_visible(resp.rect) {
        view.thumbs.want(e, view.is_remote());
    }
    if scroll_here {
        resp.scroll_to_me(Some(egui::Align::Center));
    }
    if !handle_entry_context(view, e, &resp, ctx.root, action) {
        self::view::drag::note(ui, e, resp.rect);
        handle_entry_interaction(ui, view, e, (resp.clicked(), resp.double_clicked()), action);
    }
}

/// 그리드 셀. 잘라내기 대기 중인 이름은 `cut-pending-fg` 이탤릭이고 선택·호버 배경은 유지한다.
fn grid_cell(
    ui: &mut egui::Ui,
    ctx: &EntryCtx<'_>,
    e: &DirEntryInfo,
    (selected, cut, under_drop): (bool, bool, bool),
    thumb: Option<&egui::TextureHandle>,
) -> egui::Response {
    let EntryCtx { theme, query, .. } = *ctx;
    let GridMetrics { slot, cell_h, .. } = ctx.metrics;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(CELL_W.value(), cell_h), egui::Sense::click());
    let p = ui.painter_at(rect);

    let state = tasty_ui_widgets::RowState::from_response(selected, &resp);
    state.paint_fill(&p, rect, theme.corner_radius.value(), theme);

    let ink = entry_style::RowInk::of(state.under_drop(under_drop), ctx.parent, cut);
    let (icon, glyph_color) = entry_style::entry_icon(theme, e, selected, ink.tinted());
    // 선택 행의 모양 글리프만 hover·press 잉크로 바꾼다. hue 글리프는 on-active 띠를 유지한다.
    let glyph_color = if glyph_color == theme.text_on_active().to_egui() {
        state.glyph(theme).to_egui()
    } else {
        glyph_color
    };
    let glyph_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.center().x,
            rect.top() + theme.spacing_sm.value() + slot / 2.0,
        ),
        egui::vec2(slot, slot),
    );
    thumbs::paint_slot(ui, theme, glyph_rect, thumb, icon, glyph_color);

    // 이름은 위에서부터 최대 세 줄로 표시하고 넘치면 끝을 줄인다.
    let label_color = ink.name(theme, e, theme.text_secondary());
    let mut job = find::name_job(
        theme,
        &e.name,
        query,
        egui::FontId::proportional(ctx.metrics.label_font),
        label_color,
        Some(ctx.metrics.label_line_h),
        selected,
    );
    if cut {
        tasty_ui_widgets::italicize_name(&mut job);
    }
    job.halign = egui::Align::Center;
    // 링크는 마지막 줄 끝에 link 글리프가 들어갈 폭을 남긴다.
    let tail = link::has_tail(e).then(|| link::tail_width(theme, e));
    job.wrap = egui::text::TextWrapping {
        max_width: (CELL_W - theme.spacing_xs.scaled(2.0)).value() - tail.unwrap_or(0.0),
        max_rows: 3,
        overflow_character: Some('…'),
        ..Default::default()
    };
    let galley = ui.fonts(|f| f.layout_job(job));
    let label_at = egui::pos2(
        rect.center().x,
        glyph_rect.bottom() + theme.spacing_xs.value(),
    );
    if let (Some(_), Some(row)) = (tail, galley.rows.last()) {
        let row = row.rect.translate(label_at.to_vec2());
        link::paint_tail(ui, theme, row.right(), row.center().y, selected);
    }
    p.galley(label_at, galley, label_color);

    resp
}

/// `..` 와 이름 입력 줄은 늘 그리고, 항목 줄은 화면에 걸친 줄만 그린다.
fn list_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    cut_pending: &HashSet<PathBuf>,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let query = view.find_query().to_owned();
    if let Some(p) = parent_nav_target(root).filter(|_| view.search_root().is_none()) {
        let up = icons::FOLDER;
        let dd = dotdot_entry(p.clone());
        let rest = entry_style::RowInk::at_rest(view, &dd, false);
        let (resp, _) = tree_row_with_tail_fg(
            ui,
            theme,
            Some(&|ui, rect, c| up.image(rect.height(), c).paint_at(ui, rect)),
            "..",
            "",
            false,
            0.0,
            Some(rest.name(theme, &dd, theme.tree_row_fg())),
        );
        if resp.double_clicked() && action.is_none() {
            *action = Some(ExplorerAction::Navigate(p));
        }
    }
    let row_h = theme.tree_row_height().value();
    let at = create::place::slot_index(view);
    let count = view.shown_count() + usize::from(at.is_some());
    view.note_rows(ui, row_h);
    let editor_h = theme.table_cell_height().value();
    create::place::fit_layout(view, at, |l| l.origin.y += editor_h);
    let target = scroll_target(view).map(|t| create::place::virtual_index(at, t));
    let span = open_span(ui, row_h, count, target);
    let (real, edit) = create::place::list_span(ui, theme, view, (at, row_h), &span, action);
    let items = view.shown_range(real);
    for (i, e) in items.iter().enumerate() {
        if edit == Some(i) {
            create::name_row(ui, theme, view, create::Slot::List, action);
        }
        let selected = view.selected.contains(&e.path);
        let cut = cut_pending.contains(&e.path);
        let ink = entry_style::RowInk::at_rest(view, e, cut);
        let row = egui::vec2(ui.available_width(), row_h);
        // 행 상태는 행 위젯이 고른다. 누른 동안이 아니면 포인터 아래 행은 쉬는 행이 아니다.
        let resting = ui.input(|i| i.pointer.any_down())
            || !ui.rect_contains_pointer(egui::Rect::from_min_size(ui.cursor().min, row));
        let tinted = ink.tinted() && resting;
        let (icon, glyph_color) = entry_style::entry_icon(theme, e, selected, tinted);
        let rest = ink.name(theme, e, theme.tree_row_fg());
        // 보이는 줄만 그리므로 줄의 위젯 id 를 화면 위치가 아니라 항목 경로에 묶는다.
        let resp = ui
            .push_id(&e.path, |ui| {
                let tail = link::tail_width(theme, e);
                let paint_icon = |ui: &mut egui::Ui, rect: egui::Rect, _c: egui::Color32| {
                    icon.image(rect.height(), glyph_color).paint_at(ui, rect);
                };
                let (resp, name_end) = if cut {
                    tree_row_cut(
                        ui,
                        theme,
                        Some(&paint_icon),
                        &e.name,
                        &query,
                        selected,
                        tail,
                        rest,
                    )
                } else {
                    tree_row_with_tail_fg(
                        ui,
                        theme,
                        Some(&paint_icon),
                        &e.name,
                        &query,
                        selected,
                        tail,
                        Some(rest),
                    )
                };
                if tail > 0.0 {
                    link::paint_tail(ui, theme, name_end, resp.rect.center().y, selected);
                }
                resp
            })
            .inner;
        let resp = link::broken_tooltip(e, view.is_remote(), view.hit_tooltip(e, resp));
        if !handle_entry_context(view, e, &resp, root, action) {
            self::view::drag::note(ui, e, resp.rect);
            handle_entry_interaction(ui, view, e, (resp.clicked(), resp.double_clicked()), action);
        }
    }
    if edit == Some(items.len()) {
        create::name_row(ui, theme, view, create::Slot::List, action);
    }
    ui.add_space((count - span.end) as f32 * row_h);
}

#[allow(clippy::too_many_arguments)]
fn detail_view(
    ui: &mut egui::Ui,
    theme: &Theme,
    panel: &ExplorerPanel,
    view: &mut ExplorerView,
    id_suffix: &str,
    cut_pending: &HashSet<PathBuf>,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) {
    let remote = view.is_remote();
    let tab = panel.active_tab();
    let columns = tasty_ui_widgets::explorer_detail_columns(
        theme,
        tasty_ui_widgets::ExplorerDetailHeads {
            name: (t("explorer.column.name"), SortColumn::Name),
            size: (t("explorer.column.size"), SortColumn::Size),
            modified: (t("explorer.column.modified"), SortColumn::Modified),
            kind: (t("explorer.column.type"), SortColumn::Type),
            folder: view.search_root().map(|_| t("explorer.type.folder")),
        },
    );
    let dir = match tab.sort_dir {
        SortDir::Asc => TableSortDir::Asc,
        SortDir::Desc => TableSortDir::Desc,
    };
    let search_root = view.search_root().map(Path::to_path_buf);
    // 하위 폴더 검색 결과는 목록을 대신하므로 `..` 를 두지 않는다.
    let parent = parent_nav_target(root).filter(|_| search_root.is_none());
    let query = view.find_query().to_owned();
    // 합성 행을 포함한 번호를 원본 목록 번호로 옮겨 화면에 걸친 행만 빌린다.
    let dotdot = parent.map(dotdot_entry);
    let placeholder = view.create.as_ref().map(|_| create::placeholder_row(root));
    let at = create::place::slot_index(view);
    let lead = usize::from(dotdot.is_some());
    let editor_row = at.map(|a| lead + a);
    let scroll_row = scroll_target(view).map(|i| lead + create::place::virtual_index(at, i));
    let count = lead + view.shown_count() + usize::from(placeholder.is_some());
    let row_at = |i: usize| {
        let row = if i < lead {
            dotdot.as_ref()
        } else if Some(i) == editor_row {
            placeholder.as_ref()
        } else {
            let index = i - lead - usize::from(editor_row.is_some_and(|at| i > at));
            view.shown_at(index)
        };
        (i, row.expect("detail row index is within count"))
    };
    let editor_cell = std::cell::Cell::new(None);
    let drawn_name_cell = std::cell::Cell::new(None);
    let out = Table::new(columns)
        .active_sort(tab.sort_column, dir)
        .header_fill(theme.table_header_bg().to_egui())
        // Size 제목 끝을 본문 Size 값처럼 날짜 열에서 띄운다(design DetailHeader paddingRight).
        .header_pad_right(theme.spacing_sm)
        .selectable(true)
        .virtual_rows(true)
        .scroll_to_row(scroll_row)
        .id_salt(format!("explorer_detail_{id_suffix}_{}", view.tab_salt()))
        .show_indexed(
            ui,
            theme,
            count,
            row_at,
            // `..`(name == "..", read_dir 은 이 이름을 반환하지 않음) 는 선택 대상 아님.
            |(_, row): &(usize, &DirEntryInfo)| {
                row.name != ".." && view.selected.contains(&row.path)
            },
            |ui, th, &(index, row), col| {
                if col == 0 && drawn_name_cell.get().is_none() {
                    drawn_name_cell.set(Some((index, ui.max_rect())));
                }
                let cut = cut_pending.contains(&row.path);
                let sel = row.name != ".." && view.selected.contains(&row.path);
                let ink = entry_style::RowInk::in_table(ui, view, row, cut, sel);
                // 하위 폴더 검색은 Name 뒤에 Folder 를 두고 Type 을 뺀다. 열 번호를 평소 배치로 옮긴다.
                let col = match (&search_root, col) {
                    (Some(_), 1) => 4,
                    (Some(_), c) if c > 1 => c - 1,
                    (_, c) => c,
                };
                // `..` 행은 이름 말고는 비워 둔다.
                let blank = row.name == "..";
                let text = match col {
                    0 if row.name.is_empty() => return editor_cell.set(Some(ui.max_rect())),
                    0 => {
                        self::view::drag::note_row(ui, row);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                            let sz = th.icon_glyph_size_md.value();
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
                            let (icon, c) = entry_style::entry_icon(th, row, sel, ink.tinted());
                            icon.image(sz, c).paint_at(ui, rect);
                            let name_fg = ink.name(th, row, th.table_row_fg());
                            let mut job = find::name_job(
                                th,
                                &row.name,
                                &query,
                                egui::FontId::proportional(th.font_size_body.value()),
                                name_fg,
                                None,
                                sel,
                            );
                            if cut {
                                tasty_ui_widgets::italicize_name(&mut job);
                            }
                            // 꼬리 글리프 폭을 먼저 빼고 이름을 그 앞에서 말줄임한다. 폭은 열의 clip 끝으로
                            // 잰다(남은 폭을 다 채우면 이름 열이 스크롤 안에서 넓어진다).
                            let room = ui.clip_rect().right() - ui.cursor().left();
                            let text_w = ui.fonts(|f| f.layout_job(job.clone()).rect.width());
                            let w = text_w.min(room - link::tail_width(th, row)).max(0.0);
                            // 이름 뒤 간격은 이름을 놓을 때 정해지므로 꼬리의 space-xs 를 먼저 둔다.
                            ui.spacing_mut().item_spacing.x = th.spacing_xs.value();
                            let name = ui
                                .allocate_ui(egui::vec2(w, th.font_size_body.value()), |ui| {
                                    ui.add(egui::Label::new(job).truncate())
                                })
                                .inner;
                            link::broken_tooltip(row, remote, name);
                            if link::has_tail(row) {
                                link::add_tail(ui, th, sel);
                            }
                        });
                        return;
                    }
                    // 오른쪽 정렬 셀의 앞 여백으로 날짜 열과 간격을 둔다.
                    1 => {
                        ui.add_space(th.spacing_sm.value());
                        (!blank).then(|| human_size(row.is_dir, row.size))
                    }
                    2 => (!blank).then(|| crate::core::fs_list::format_modified(row.modified)),
                    4 => search_root
                        .as_deref()
                        .map(|r| find::hit_folder(r, &row.path)),
                    _ => (!blank).then(|| kind_word(row)),
                };
                // Type 열만 본문 글꼴 크기로, 나머지 값은 고정폭으로 쓴다.
                let text =
                    egui::RichText::new(display_text::nfc(&text.unwrap_or_default()).into_owned())
                        .color(ink.meta(th));
                let size = th.font_size_caption.value();
                // Type 열은 좁아 낱말을 말줄임하고, 줄였으면 호버에 전체 낱말이 보인다.
                if col == 3 {
                    ui.add(egui::Label::new(text.size(size)).truncate());
                } else {
                    ui.label(text.font(egui::FontId::monospace(size)));
                }
            },
        );
    self::view::drag_scroll::feed(
        ui,
        theme,
        &view.ops.drag,
        &out.scroll,
        theme.table_cell_height(),
    );

    if let Some(key) = out.clicked_sort
        && action.is_none()
    {
        *action = Some(ExplorerAction::SetSort(key));
    }
    // 눌린 행만 복제한다. 아래 처리는 view 를 바꾸므로 목록 참조를 더 쓰지 않는다.
    let clicked_entry = |i: Option<usize>| {
        i.map(row_at)
            .filter(|(_, e)| !e.name.is_empty())
            .map(|(_, e)| e.clone())
    };
    let secondary = clicked_entry(out.secondary_clicked_row);
    let clicked = clicked_entry(out.clicked_row);
    let row_h = theme.table_cell_height().value();
    view.note_detail(drawn_name_cell.get(), count - view.shown_count(), row_h);
    create::place::fit_layout(view, at, |_| {});
    // 편집 줄 자리가 화면 밖이라 그리지 않았으면 그린 행에서 자리를 계산한다. 입력은 화면 밖에서도
    // 그려야 포커스를 잃지 않는다. 행 높이는 표 기본값이다.
    let editor_cell = editor_cell.get().or_else(|| {
        let (index, cell) = drawn_name_cell.get()?;
        let shift = (editor_row? as f32 - index as f32) * row_h;
        Some(cell.translate(egui::vec2(0.0, shift)))
    });
    create::detail_row(ui, theme, view, editor_cell, action);
    if let Some(e) = &secondary
        && e.name != ".."
    // `..` 는 컨텍스트 메뉴 대상 아님
    {
        let pos = ui.ctx().pointer_interact_pos().unwrap_or_default();
        emit_entry_context(view, e, pos, root, action);
    }
    if let Some(e) = &clicked {
        let dbl = ui.input(|inp| {
            inp.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
        handle_entry_interaction(ui, view, e, (true, dbl), action);
    }
}

fn tool_icon(ui: &mut egui::Ui, theme: &Theme, icon: Icon, enabled: bool, tip: &str) -> bool {
    let sz = theme.item_height_interactive.value();
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(sz, sz), sense);
    if enabled && resp.hovered() {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.overlay_hover().to_egui_premultiplied(),
        );
    }
    let glyph = theme.icon_glyph_size_md.value();
    let gr = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    // disabled 도구 아이콘은 opacity 없이 disabled ink를 쓴다.
    let color = if enabled {
        theme.text_secondary().to_egui()
    } else {
        theme.state_disabled_fg().to_egui()
    };
    icon.image(glyph, color).paint_at(ui, gr);
    let resp = if enabled {
        resp.on_hover_text(tip)
    } else {
        resp
    };
    enabled && resp.clicked()
}

#[cfg(test)]
mod virtual_tests;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// 낮은 칸에 탐색기를 한 번 그리고 상태줄 글자의 사각형과 그 글자를 자르는 사각형을 돌려준다.
    fn status_rect_in_short_cell(
        state: super::LoadState,
        entries: usize,
        cell_h: f32,
    ) -> Option<(egui::Rect, egui::Rect)> {
        // 상태줄 문구를 그릴 때와 찾을 때 같은 번역을 쓰도록 그리기 전에 전역 번역을 고정한다.
        crate::i18n::init("en");
        let ctx = egui::Context::default();
        let cell = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, cell_h));
        let panel = tasty_model::ExplorerPanel::new(1, PathBuf::from("/tasty-test-no-such-dir"));
        let mut view = super::ExplorerView::new();
        view.state = state;
        view.entries = (0..entries)
            .map(|i| super::DirEntryInfo {
                path: PathBuf::from(format!("/tasty-test-no-such-dir/file-{i}.txt")),
                name: format!("file-{i}.txt"),
                is_dir: false,
                size: 0,
                modified: None,
                ext: "txt".into(),
                ..Default::default()
            })
            .collect();
        let font = crate::settings::EffectiveFont {
            font_family: String::new(),
            font_size: 13.0,
            custom_font_path: String::new(),
            line_height: 1.0,
            font_scale_mode: String::new(),
        };
        let shortcut_chars = std::collections::HashSet::new();
        let input = super::ExplorerInput {
            focused: false,
            overlay_open: false,
            shortcut_chars: &shortcut_chars,
        };
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(cell),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        super::draw_explorer(
                            ui,
                            &panel,
                            &mut view,
                            &font,
                            "short",
                            &[],
                            &std::collections::HashSet::new(),
                            &[],
                            None,
                            &input,
                        );
                    });
            },
        );
        let status = crate::i18n::t_fmt("explorer.status.items", &entries.to_string());
        out.shapes.iter().find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text() == status => {
                Some((text.visual_bounding_rect(), clipped.clip_rect))
            }
            _ => None,
        })
    }

    /// 사이드바가 칸보다 커지는 낮은 칸에서도 상태줄은 칸 안에 보인다.
    #[test]
    fn a_short_cell_keeps_the_status_line_inside() {
        // 목록이 있으면 ScrollArea 가 본문을 채운다. 없으면 상태 화면이 채운다.
        // 120 은 본문이 ScrollArea 기본 최소 높이(64)보다 낮아지는 칸이다.
        for cell_h in [157.0, 120.0] {
            for (state, entries) in [
                (super::LoadState::NoPermission, 0),
                (super::LoadState::Ok, 0),
                (super::LoadState::Ok, 30),
            ] {
                let label = format!("{cell_h} {state:?} {entries}");
                let (rect, clip) = status_rect_in_short_cell(state, entries, cell_h)
                    .unwrap_or_else(|| panic!("{label}: 상태줄을 그리지 않았다"));
                assert!(
                    rect.max.y <= cell_h && clip.contains_rect(rect),
                    "{label}: 상태줄이 칸 밖에 있거나 잘린다: {rect:?} clip {clip:?}"
                );
            }
        }
    }
}
