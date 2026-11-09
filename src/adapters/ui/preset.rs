//! 프리셋 목록·미리보기·편집 화면.
//! 구조 편집은 자동 저장하며 surface 설정은 확인할 때 저장한다. 설정 화면이 열린 동안은
//! 목록·범위 탭과 구조 단축키를 막아 편집 대상을 바꾸지 않는다.
//! 캐시 생성 후 저장소 레이아웃이 바뀌면 덮어쓰지 않고 저장소 값을 다시 읽어 알린다.
//! 보기 모드는 다음 프레임에 갱신하고 편집 모드는 저장 직전에 충돌을 확인한다.

#[cfg(test)]
mod cache_slot_tests;
mod demo_cache;
pub mod demo_layout;
mod layout_base;
pub mod surface_settings;
mod toolbar;
#[cfg(test)]
mod view_refresh_tests;

mod preview;
use preview::{draw_preview, draw_settings_detail};

use crate::view::preset::draft::PresetDrafts;
use tasty_presets::{PresetKind, PresetPaneNode, PresetResult, PresetSurfaceLayout};
use tasty_settings::KeybindingSettings;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::STRUCT_GAP_1;
use tasty_ui_widgets::{ControlSize, IconButton, IconButtonVariant};

use toolbar::{apply_toolbar_actions, draw_toolbar_editing, draw_toolbar_view};

use crate::adapters::ui::icons;
use crate::adapters::ui::input::shortcuts::any_binding_pressed_egui;
use crate::adapters::ui::{ToastKind, ToastManager, ToastScope};
use crate::i18n::{t, t_count};

use demo_cache::{
    DemoCache, drew_editing_last, load_demo, preset_key, refresh_view_cache, reload_after_conflict,
    set_drew_editing_last, store_demo,
};
use demo_layout::{DemoLayout, KindCatalog, ShortcutAction, ShowOutcome};
use layout_base::LayoutBase;
use surface_settings::{CfgOutcome, SurfaceCfg, breadcrumb, draw_surface_settings};

/// 설정 단축키에서 surface→tab→pane 순으로 첫 일치 동작을 고른다.
/// double-tap은 parse_binding에서 지원하지 않으므로 편집기에서도 처리하지 않는다.
fn match_preset_shortcut(
    kb: &KeybindingSettings,
    input: &egui::InputState,
    super_held: bool,
) -> Option<ShortcutAction> {
    let pressed = |b: &[String]| any_binding_pressed_egui(b, input, super_held);
    if pressed(&kb.split_surface_vertical) {
        Some(ShortcutAction::SplitSurfaceVertical)
    } else if pressed(&kb.split_surface_horizontal) {
        Some(ShortcutAction::SplitSurfaceHorizontal)
    } else if pressed(&kb.close_surface) {
        Some(ShortcutAction::CloseSurface)
    } else if pressed(&kb.new_tab) {
        Some(ShortcutAction::NewTab)
    } else if pressed(&kb.close_active) {
        Some(ShortcutAction::CloseActive)
    } else if pressed(&kb.split_pane_vertical) {
        Some(ShortcutAction::SplitPaneVertical)
    } else if pressed(&kb.split_pane_horizontal) {
        Some(ShortcutAction::SplitPaneHorizontal)
    } else if pressed(&kb.close_pane) {
        Some(ShortcutAction::ClosePane)
    } else {
        None
    }
}

// 디자인 고정 px (Theme 에 대응 토큰 없는 preset-window 셸 전용 치수 — specimen 전사).
/// 좌측 리스트 폭.
const LIST_WIDTH: LogicalPx = LogicalPx(196.0);
/// 우측 detail 툴바 높이. `size-44` 이지만 툴바 높이라는 역할의 토큰이 없다 — 값이 같은
/// `preset_cfg_header_height` 는 설정 화면 헤더의 치수라 읽으면 틀린 결합이 된다.
const TOOLBAR_HEIGHT: LogicalPx = LogicalPx(44.0);
/// 리스트 row 상하 padding.
const ROW_PAD_Y: LogicalPx = LogicalPx(7.0);
/// 리스트 row 좌우 padding (좌측 accent bar 다음 텍스트 들여쓰기).
const ROW_PAD_X: LogicalPx = LogicalPx(9.0);
/// 리스트 내부 좌우 inset (row 가 패널 가장자리에 붙지 않게).
const LIST_INSET: LogicalPx = LogicalPx(6.0);
/// rename 인라인 입력 폭.
pub(super) const RENAME_W: LogicalPx = LogicalPx(150.0);
/// 툴바 separator 높이.
pub(super) const TOOLBAR_SEP_H: LogicalPx = LogicalPx(18.0);

/// rename 인라인 편집 상태 (egui temp memory 에 보관 — 프레임 간 유지).
#[derive(Clone)]
pub(super) struct RenameState {
    kind: PresetKind,
    original: String,
    buffer: String,
    request_focus: bool,
}

/// 편집 모드 툴바의 name/subtitle 인라인 버퍼 (egui temp memory 보관). `key`
/// (`{kind}:{name}`)가 바뀌면 store 값으로 재초기화한다. subtitle 은 Workspace 만
/// 실제 필드를 가지며 Tab/Pane 은 구조 파생이라 편집 불가(버퍼 미사용).
#[derive(Clone, Default)]
pub(super) struct EditMetaState {
    key: String,
    name: String,
    subtitle: String,
}

/// Only the editor's two text buffers; no store or entire egui memory is copied.
#[derive(Clone)]
pub(crate) struct ToolbarDraft {
    rename: Option<RenameState>,
    metadata: Option<EditMetaState>,
}
impl ToolbarDraft {
    pub(crate) fn capture(ctx: &egui::Context) -> Self {
        ctx.data_mut(|data| Self {
            rename: data
                .get_temp::<Option<RenameState>>(egui::Id::new("preset_rename_state"))
                .flatten(),
            metadata: data.get_temp(egui::Id::new("preset_edit_meta")),
        })
    }
    pub(crate) fn retain_input(&mut self, ctx: &egui::Context) {
        let latest = Self::capture(ctx);
        if let Some(latest) = latest.metadata {
            if let Some(original) = &mut self.metadata {
                original.name = latest.name;
                original.subtitle = latest.subtitle;
            } else {
                self.metadata = Some(latest);
            }
        }
        if let Some(latest) = latest.rename {
            self.rename = Some(latest);
        }
    }

    pub(crate) fn reconcile(&mut self, applied: &[crate::view::preset::draft::PresetApplied]) {
        use crate::view::preset::draft::PresetApplied;
        for result in applied {
            match result {
                PresetApplied::Rename { kind, from, to } => {
                    if let Some(meta) = &mut self.metadata
                        && meta.key == format!("{}:{from}", kind.as_str())
                    {
                        meta.key = format!("{}:{to}", kind.as_str());
                        meta.name = to.clone();
                    }
                    if let Some(rename) = &mut self.rename
                        && rename.kind == *kind
                        && rename.original == *from
                    {
                        rename.original = to.clone();
                        rename.buffer = to.clone();
                    }
                }
                PresetApplied::Delete { kind, name } => {
                    if self
                        .metadata
                        .as_ref()
                        .is_some_and(|meta| meta.key == format!("{}:{name}", kind.as_str()))
                    {
                        self.metadata = None;
                    }
                    if self
                        .rename
                        .as_ref()
                        .is_some_and(|rename| rename.kind == *kind && rename.original == *name)
                    {
                        self.rename = None;
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn restore(self, ctx: &egui::Context) {
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("preset_rename_state"), self.rename);
            if let Some(metadata) = self.metadata {
                data.insert_temp(egui::Id::new("preset_edit_meta"), metadata);
            } else {
                data.remove::<EditMetaState>(egui::Id::new("preset_edit_meta"));
            }
        });
    }
}

/// [`queue_layout`] 의 결과.
#[derive(Debug, PartialEq)]
enum QueuedLayout {
    /// 요청을 만들었다(또는 변경이 없다). App 결과가 도착하면 저장소 projection으로 다시 대조한다.
    Queued(Option<LayoutBase>),
    /// 캐시가 지어진 뒤 저장소의 레이아웃이 바뀌었다. 아무것도 쓰지 않았다.
    Conflict,
}

/// 캐시를 만들 때의 base와 현재 저장소를 비교한 뒤 저장한다. 다르면 Conflict이며
/// 사라진 프리셋은 되살리지 않는다.
fn queue_layout(
    store: &mut PresetDrafts,
    kind: PresetKind,
    name: &str,
    layout: &DemoLayout,
    base: &Option<LayoutBase>,
) -> PresetResult<QueuedLayout> {
    let current = LayoutBase::current(store, kind, name);
    if current.is_some() && current != *base {
        return Ok(QueuedLayout::Conflict);
    }
    write_layout(store, kind, name, layout)?;
    Ok(QueuedLayout::Queued(LayoutBase::current(store, kind, name)))
}

/// 메타데이터는 유지하고 레이아웃만 저장한다. 범위가 맞지 않거나 프리셋이 없으면 아무것도 쓰지 않는다.
fn write_layout(
    store: &mut PresetDrafts,
    kind: PresetKind,
    name: &str,
    layout: &DemoLayout,
) -> PresetResult<()> {
    match kind {
        PresetKind::Workspace => {
            let Some(node) = layout.rebuild_pane_node() else {
                return Ok(());
            };
            let Some(mut p) = store.get_workspace(name).cloned() else {
                return Ok(());
            };
            p.layout = node;
            store.queue_workspace_overwrite(p)
        }
        PresetKind::Tab => {
            let Some(surf) = layout.rebuild_surface_layout() else {
                return Ok(());
            };
            let Some(mut p) = store.get_tab(name).cloned() else {
                return Ok(());
            };
            p.tab.layout = surf;
            store.queue_tab_overwrite(p)
        }
        PresetKind::Pane => {
            let Some(pane) = layout.rebuild_single_pane() else {
                return Ok(());
            };
            let Some(mut p) = store.get_pane(name).cloned() else {
                return Ok(());
            };
            p.pane = pane;
            store.queue_pane_overwrite(p)
        }
    }
}

// 워크스페이스는 저장된 부제가 있으면 사용한다. 나머지는 구조 개수를 단수·복수 문구로 표시한다.

fn count_panes(node: &PresetPaneNode) -> usize {
    match node {
        PresetPaneNode::Leaf { .. } => 1,
        PresetPaneNode::Split { first, second, .. } => count_panes(first) + count_panes(second),
    }
}

fn count_ws_tabs(node: &PresetPaneNode) -> usize {
    match node {
        PresetPaneNode::Leaf { pane } => pane.tabs.len(),
        PresetPaneNode::Split { first, second, .. } => count_ws_tabs(first) + count_ws_tabs(second),
    }
}

fn count_surfaces(layout: &PresetSurfaceLayout) -> usize {
    match layout {
        PresetSurfaceLayout::Leaf { .. } => 1,
        PresetSurfaceLayout::Split { first, second, .. } => {
            count_surfaces(first) + count_surfaces(second)
        }
    }
}

/// 단/복수 i18n 라벨. `n==1` → `one_key`, 그 외 → `many_key`({} 치환).
fn count_label(n: usize, key: &str) -> String {
    t_count(key, n as u64, &[&n.to_string()])
}

/// 편집 가능한 **실제** subtitle 필드값(Workspace 만 보유). Tab/Pane 은 구조 파생
/// subtitle 이라 편집 불가 → 빈 문자열.
pub(super) fn workspace_subtitle_field(
    store: &PresetDrafts,
    kind: PresetKind,
    name: &str,
) -> String {
    if kind == PresetKind::Workspace {
        store
            .get_workspace(name)
            .map(|p| p.subtitle.clone())
            .unwrap_or_default()
    } else {
        String::new()
    }
}

fn subtitle(store: &PresetDrafts, kind: PresetKind, name: &str) -> String {
    match kind {
        PresetKind::Workspace => store
            .get_workspace(name)
            .map(|p| {
                if !p.subtitle.is_empty() {
                    return p.subtitle.clone();
                }
                let panes = count_label(count_panes(&p.layout), "preset.count.pane");
                let tabs = count_label(count_ws_tabs(&p.layout), "preset.count.tab");
                format!("{panes} · {tabs}")
            })
            .unwrap_or_default(),
        PresetKind::Tab => store
            .get_tab(name)
            .map(|p| count_label(count_surfaces(&p.tab.layout), "preset.count.surface"))
            .unwrap_or_default(),
        PresetKind::Pane => store
            .get_pane(name)
            .map(|p| count_label(p.pane.tabs.len(), "preset.count.tab"))
            .unwrap_or_default(),
    }
}

// 현재 실행 레이아웃 저장은 컨텍스트 메뉴에서 처리한다. 여기서는 터미널 하나의 최소 프리셋을 만든다.

fn minimal_surface() -> PresetSurfaceLayout {
    use tasty_presets::PresetSurface;
    PresetSurfaceLayout::Leaf {
        surface: PresetSurface {
            id: None,
            kind: "terminal".into(),
            cwd: None,
            startup_command: None,
            params: serde_json::Value::Null,
        },
    }
}

fn minimal_pane() -> tasty_presets::PresetPane {
    use tasty_presets::PresetTab;
    tasty_presets::PresetPane {
        tabs: vec![PresetTab {
            explicit_name: None,
            layout: minimal_surface(),
        }],
        active_tab: 0,
    }
}

/// 최소 preset 을 만들어 저장하고, 부여된 이름을 반환한다. 실패 시 `None`.
fn create_minimal(store: &mut PresetDrafts, kind: PresetKind) -> Option<String> {
    use tasty_presets::{PanePreset, TabPreset, WorkspacePreset};
    let name = store.unique_name(kind, kind.as_str());
    let result = match kind {
        PresetKind::Workspace => store.queue_workspace(WorkspacePreset {
            name: name.clone(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Leaf {
                pane: minimal_pane(),
            },
        }),
        PresetKind::Tab => store.queue_tab(TabPreset {
            name: name.clone(),
            tab: tasty_presets::PresetTab {
                explicit_name: None,
                layout: minimal_surface(),
            },
        }),
        PresetKind::Pane => store.queue_pane(PanePreset {
            name: name.clone(),
            pane: minimal_pane(),
        }),
    };
    match result {
        Ok(()) => Some(name),
        Err(e) => {
            tracing::warn!("create minimal preset failed: {e}");
            None
        }
    }
}

/// 기존 preset 의 복사본을 만들어 저장하고, 새 이름을 반환한다. 실패 시 `None`.
pub(super) fn duplicate_preset(
    store: &mut PresetDrafts,
    kind: PresetKind,
    name: &str,
) -> Option<String> {
    let new_name = store.unique_name(kind, &format!("{name}-copy"));
    let result = match kind {
        PresetKind::Workspace => match store.get_workspace(name).cloned() {
            Some(mut p) => {
                p.name = new_name.clone();
                store.queue_workspace(p)
            }
            None => return None,
        },
        PresetKind::Tab => match store.get_tab(name).cloned() {
            Some(mut p) => {
                p.name = new_name.clone();
                store.queue_tab(p)
            }
            None => return None,
        },
        PresetKind::Pane => match store.get_pane(name).cloned() {
            Some(mut p) => {
                p.name = new_name.clone();
                store.queue_pane(p)
            }
            None => return None,
        },
    };
    match result {
        Ok(()) => Some(new_name),
        Err(e) => {
            tracing::warn!("duplicate preset failed: {e}");
            None
        }
    }
}

/// 리스트 row 한 줄을 그린다. 선택 시 surface-active 채움 + 2px accent 좌측 bar.
fn draw_list_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    sub: &str,
    selected: bool,
) -> egui::Response {
    let name_h = theme.font_size_body;
    let sub_h = theme.font_size_caption;
    let row_h = ROW_PAD_Y.scaled(2.0) + name_h + STRUCT_GAP_1 + sub_h;
    let (full, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h.value()),
        egui::Sense::click(),
    );
    let rect = egui::Rect::from_min_max(
        egui::pos2(full.min.x + LIST_INSET.value(), full.min.y),
        egui::pos2(full.max.x - LIST_INSET.value(), full.max.y),
    );
    let radius = theme.corner_radius_sm.value();
    let p = ui.painter_at(full);
    if selected {
        p.rect_filled(rect, radius, theme.surface_active().to_egui());
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme.selection_edge_width.value(), rect.height()),
        );
        p.rect_filled(bar, 0.0, theme.accent_primary().to_egui());
    } else if resp.hovered() {
        p.rect_filled(rect, radius, theme.overlay_hover().to_egui_premultiplied());
    }
    let name_color = if selected {
        theme.text_on_active().to_egui()
    } else {
        theme.text_secondary().to_egui()
    };
    let sub_color = if selected {
        theme.text_on_active().to_egui()
    } else {
        theme.text_muted().to_egui()
    };
    let text_x = rect.min.x + ROW_PAD_X.value();
    let name_y = rect.min.y + ROW_PAD_Y.value();
    p.text(
        egui::pos2(text_x, name_y),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(name_h.value()),
        name_color,
    );
    p.text(
        egui::pos2(text_x, name_y + (name_h + STRUCT_GAP_1).value()),
        egui::Align2::LEFT_TOP,
        sub,
        tasty_ui_widgets::label_font(sub_h.value()),
        sub_color,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 설정 화면 아래의 목록·탭 입력을 막는다. egui는 나중에 등록한 같은 층의 위젯을 위로 본다.
fn block_input(ui: &mut egui::Ui, rect: egui::Rect, salt: &str) {
    ui.interact(
        rect,
        egui::Id::new(("preset_cfg_lock", salt)),
        egui::Sense::click_and_drag(),
    );
}

/// [`draw_preset_panel`] 본문 2분할([리스트 196px | detail] → 툴바/미리보기)의
/// 사각형들. 좌측 리스트/우측 detail 배경과 구분선도 이 시점에 함께 칠한다.
struct PresetPanelRects {
    list_rect: egui::Rect,
    toolbar_rect: egui::Rect,
    preview_rect: egui::Rect,
}

fn compute_panel_rects(ui: &egui::Ui, theme: &Theme) -> PresetPanelRects {
    let body = ui.available_rect_before_wrap();
    let bw = theme.border_width.value();
    let list_rect =
        egui::Rect::from_min_size(body.min, egui::vec2(LIST_WIDTH.value(), body.height()));
    let detail_rect = egui::Rect::from_min_max(
        egui::pos2(body.min.x + LIST_WIDTH.value(), body.min.y),
        body.max,
    );
    let painter = ui.painter();
    painter.rect_filled(list_rect, 0.0, theme.bg_sidebar().to_egui());
    painter.rect_filled(detail_rect, 0.0, theme.bg_panel().to_egui());
    painter.vline(
        body.min.x + LIST_WIDTH.value(),
        body.y_range(),
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );

    let toolbar_rect = egui::Rect::from_min_size(
        detail_rect.min,
        egui::vec2(detail_rect.width(), TOOLBAR_HEIGHT.value()),
    );
    let preview_rect = egui::Rect::from_min_max(
        egui::pos2(detail_rect.min.x, toolbar_rect.max.y),
        detail_rect.max,
    );
    ui.painter().hline(
        toolbar_rect.x_range(),
        toolbar_rect.max.y,
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );

    PresetPanelRects {
        list_rect,
        toolbar_rect,
        preview_rect,
    }
}

/// 목록 선택과 새 프리셋 버튼을 그려 selected를 갱신한다.
#[allow(clippy::too_many_arguments)]
fn draw_preset_list(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    theme: &Theme,
    store: &mut PresetDrafts,
    kind: PresetKind,
    selected: &mut Option<String>,
    resolved: &Option<String>,
    rows: &[(String, String)],
    list_rect: egui::Rect,
    locked: bool,
) {
    let mut new_clicked = false;
    let mut clicked_name: Option<String> = None;

    {
        let mut lui = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
        lui.set_clip_rect(list_rect);
        if locked {
            lui.set_opacity(theme.preset_cfg_dim_opacity());
        }
        lui.add_space(theme.spacing_sm.value());
        lui.horizontal(|ui| {
            ui.add_space(LIST_INSET.value());
            let count = t_count(
                "preset.header.count",
                rows.len() as u64,
                &[&rows.len().to_string()],
            );
            ui.label(
                egui::RichText::new(count.to_uppercase())
                    .family(tasty_ui_widgets::label_font_family())
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(LIST_INSET.value());
                if IconButton::new()
                    .variant(IconButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .show(ui, theme, &|ui, rect, c| {
                        icons::PLUS.image(rect.width(), c).paint_at(ui, rect)
                    })
                    .on_hover_text(t("preset.header.new"))
                    .clicked()
                {
                    new_clicked = true;
                }
            });
        });
        lui.add_space(theme.spacing_xs.value());

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .drag_to_scroll(false)
            .enable_scrolling(!locked)
            .show(&mut lui, |ui| {
                if rows.is_empty() {
                    ui.add_space(theme.spacing_sm.value());
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(t("preset.popup.empty"))
                                .size(theme.font_size_caption.value())
                                .color(theme.text_muted().to_egui()),
                        );
                    });
                    return;
                }
                for (name, sub) in rows {
                    let is_sel = resolved.as_deref() == Some(name.as_str());
                    if draw_list_row(ui, theme, name, sub, is_sel).clicked() {
                        clicked_name = Some(name.clone());
                    }
                }
            });
    }

    if locked {
        // 설정 초안이 사라지지 않도록 이번 프레임의 선택도 무시하고 이후 입력을 막는다.
        block_input(ui, list_rect, "list");
        return;
    }
    if let Some(n) = clicked_name {
        *selected = Some(n);
        ctx.request_repaint();
    }
    if new_clicked && let Some(n) = create_minimal(store, kind) {
        *selected = Some(n);
        ctx.request_repaint();
    }
}

/// [`apply_toolbar_actions`] 에 전달할 이번 프레임 툴바 클릭 결과 묶음.
pub(super) struct PresetToolbarClicks {
    edit_clicked: bool,
    done_clicked: bool,
    rename_clicked: bool,
    duplicate_clicked: bool,
    delete_clicked: bool,
}

/// PresetView 의 본문을 그린다.
#[allow(clippy::too_many_arguments)]
pub fn draw_preset_panel(
    ctx: &egui::Context,
    store: &mut PresetDrafts,
    active_kind: &mut PresetKind,
    selected_workspace: &mut Option<String>,
    selected_tab: &mut Option<String>,
    selected_pane: &mut Option<String>,
    editing: &mut bool,
    selected_node: &mut Option<usize>,
    surface_cfg: &mut Option<SurfaceCfg>,
    toasts: &mut ToastManager,
    catalog: &KindCatalog,
    kb: &KeybindingSettings,
) {
    let theme = crate::theme::theme();
    if !*editing {
        *surface_cfg = None;
    }
    let locked = surface_cfg.is_some();
    let rename_id = egui::Id::new("preset_rename_state");
    let mut rename: Option<RenameState> = ctx
        .data_mut(|d| d.get_temp::<Option<RenameState>>(rename_id))
        .flatten();
    let edit_meta_id = egui::Id::new("preset_edit_meta");

    egui::CentralPanel::default().show(ctx, |ui| {
        if store.is_pending() {
            ui.disable();
        }
        let mut scope = *active_kind;
        let l1 = ui
            .scope(|ui| {
                if locked {
                    ui.set_opacity(theme.preset_cfg_dim_opacity());
                }
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut scope,
                        PresetKind::Workspace,
                        t("preset.tab.workspace"),
                    );
                    ui.selectable_value(&mut scope, PresetKind::Tab, t("preset.tab.tab"));
                    ui.selectable_value(&mut scope, PresetKind::Pane, t("preset.tab.pane"));
                });
            })
            .response
            .rect;
        if locked {
            block_input(ui, l1, "scope");
        } else {
            *active_kind = scope;
        }
        ui.separator();

        let kind = *active_kind;
        let names = store.list(kind);
        let selected: &mut Option<String> = match kind {
            PresetKind::Workspace => selected_workspace,
            PresetKind::Tab => selected_tab,
            PresetKind::Pane => selected_pane,
        };
        let resolved = selected
            .clone()
            .filter(|n| names.contains(n))
            .or_else(|| names.first().cloned());

        let rows: Vec<(String, String)> = names
            .iter()
            .map(|n| (n.clone(), subtitle(store, kind, n)))
            .collect();

        let rects = compute_panel_rects(ui, &theme);

        draw_preset_list(
            ui,
            ctx,
            &theme,
            store,
            kind,
            selected,
            &resolved,
            &rows,
            rects.list_rect,
            locked,
        );

        let current = selected
            .clone()
            .or_else(|| store.list(kind).first().cloned());
        let detail_sub = current
            .as_deref()
            .map(|n| subtitle(store, kind, n))
            .unwrap_or_default();

        let mut clicks = PresetToolbarClicks {
            edit_clicked: false,
            done_clicked: false,
            rename_clicked: false,
            duplicate_clicked: false,
            delete_clicked: false,
        };
        let mut edit_meta: Option<EditMetaState> = None;

        if locked {
            if let Some(n) = current.as_deref() {
                let detail = rects.toolbar_rect.union(rects.preview_rect);
                draw_settings_detail(
                    ui,
                    store,
                    &theme,
                    kind,
                    n,
                    detail,
                    selected_node,
                    surface_cfg,
                    toasts,
                    catalog,
                );
            } else {
                *surface_cfg = None;
            }
            // 설정 화면 뒤의 미리보기와 구조 단축키를 실행하지 않는다.
            return;
        }

        if let Some(name) = current.clone() {
            let toolbar_inner = rects
                .toolbar_rect
                .shrink2(egui::vec2(theme.spacing_md.value(), 0.0));
            let mut tui = ui.new_child(egui::UiBuilder::new().max_rect(toolbar_inner));
            tui.set_clip_rect(rects.toolbar_rect);
            tui.horizontal_centered(|ui| {
                if *editing {
                    let outcome = draw_toolbar_editing(
                        ui,
                        ctx,
                        store,
                        &theme,
                        kind,
                        &name,
                        selected,
                        toasts,
                        edit_meta_id,
                    );
                    edit_meta = outcome.edit_meta;
                    clicks.done_clicked = outcome.done_clicked;
                    return;
                }

                let view_clicks = draw_toolbar_view(
                    ui,
                    &theme,
                    store,
                    kind,
                    &name,
                    &detail_sub,
                    &mut rename,
                    selected,
                );
                clicks.rename_clicked = view_clicks.rename_clicked;
                clicks.duplicate_clicked = view_clicks.duplicate_clicked;
                clicks.delete_clicked = view_clicks.delete_clicked;
                clicks.edit_clicked = view_clicks.edit_clicked;
            });
        }

        if let Some(meta) = edit_meta {
            ctx.data_mut(|d| d.insert_temp(edit_meta_id, meta));
        }

        apply_toolbar_actions(
            ctx,
            store,
            kind,
            &current,
            editing,
            selected_node,
            selected,
            &mut rename,
            clicks,
        );

        let preview_name = selected
            .clone()
            .or_else(|| store.list(kind).first().cloned());
        match preview_name {
            Some(n) => draw_preview(
                ui,
                store,
                &theme,
                kind,
                &n,
                rects.preview_rect,
                *editing,
                selected_node,
                surface_cfg,
                toasts,
                catalog,
                kb,
            ),
            None => {
                ui.painter_at(rects.preview_rect).rect_filled(
                    rects.preview_rect,
                    0.0,
                    theme.bg_app().to_egui(),
                );
                ui.painter_at(rects.preview_rect).text(
                    rects.preview_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    t("preset.popup.empty"),
                    egui::FontId::proportional(theme.font_size_body.value()),
                    theme.text_muted().to_egui(),
                );
            }
        }
    });

    ctx.data_mut(|d| d.insert_temp(rename_id, rename));
}

/// The submitted draft includes input entered in the same render pass as Confirm.
pub(crate) fn take_confirmed_surface_cfg(ctx: &egui::Context) -> Option<SurfaceCfg> {
    ctx.data_mut(|data| {
        let id = egui::Id::new("preset.confirmed_surface_cfg");
        let draft = data.get_temp::<SurfaceCfg>(id);
        data.remove::<SurfaceCfg>(id);
        draft
    })
}
