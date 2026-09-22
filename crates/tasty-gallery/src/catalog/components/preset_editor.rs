//! `preset-editor` specimen — 프리셋 데모 레이아웃 미리보기(read-only) + 편집 상태
//! (selected surface 2px accent outline + 설정·remove handle cluster)를 모두 시연한다.
//! leaf 파라미터 편집은 칸 안이 아니라 surface 설정 화면이 한다 — 그 specimen 은
//! `preset_surface_settings.rs`. 디자인 `(3) gallery/preset_editor.jsx` 의 `SurfaceView` / `Pane` /
//! `PaneTree` / `SurfaceBox` 표시 부분을 구조까지 1:1 전사한다.
//!
//! 갤러리 specimen 은 정적(Theme-only, binary 미의존)이라 mini-tab 클릭 전환은
//! 본체에서만 동작한다 — 여기서는 각 pane 의 **활성 탭**만 그린다.
//!
//! 3종 구조 레벨을 서로 다른 시각 weight 로 구분:
//!  - Pane split (상위 레이아웃) → 테두리 카드 + **5px bg-app gap** (무거운 divider).
//!  - Surface split (하위 레이아웃) → **1px border-default hairline** (가벼운 divider).
//!  - Surface leaf → kind 아이콘 + 표시명 + 값 요약(`키 값`, 가운데, mono). 좁으면 degrade.
//!  - Mini tab strip → 20px, bg-sidebar. 활성 = bg-panel + 2px accent 하단 bar + kind 아이콘.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};

// 디자인 고정 px (Theme 에 대응 토큰 없는 preview 전용 치수 — jsx inline style 전사).
/// `PaneTree` 의 `gap:5` — bordered pane 카드 사이의 bg-app 공백 = 상위(pane) divider.
const PANE_GAP: LogicalPx = LogicalPx(5.0);
/// mini tab strip `height:20`.
const STRIP_H: LogicalPx = LogicalPx(20.0);
/// `Pane` 의 활성 탭 본문 `padding:3`.
const BODY_PAD: LogicalPx = LogicalPx(3.0);
/// `SurfaceBox` 의 아이콘↔라벨 `gap:6`.
const LEAF_GAP: LogicalPx = LogicalPx(6.0);
/// mini tab `padding:0 9px`.
const TAB_PAD_X: LogicalPx = LogicalPx(9.0);
/// mini tab 아이콘↔라벨 `gap:5`.
const TAB_GAP: LogicalPx = LogicalPx(5.0);
/// 편집 상태 `MiniHandle` 한 변 크기.
const E_HANDLE_SZ: LogicalPx = LogicalPx(18.0);
/// 편집 핸들 클러스터 모서리 inset.
const E_HANDLE_INSET: LogicalPx = LogicalPx(4.0);
/// 선택 leaf 핸들(설정 · remove) 사이 `gap: 2` — 본체 `demo_layout::HANDLE_GAP` 과 같은 공용
/// 항목을 읽는다.
const E_HANDLE_GAP: LogicalPx = tasty_ui_widgets::tokens::STRUCT_GAP_2;
/// add-tab `+` 버튼 폭(디자인 22×20 — strip 높이보다 2px 넓다).
const ADD_TAB_W: LogicalPx = LogicalPx(22.0);
/// mini tab close `×` 히트영역 한 변(14×14).
const CLOSE_HIT: LogicalPx = LogicalPx(14.0);
/// close `×` 왼쪽 margin(라벨과의 간격).
const CLOSE_MARGIN: LogicalPx = LogicalPx(1.0);
/// close `×` 노출 시 탭 우측 패딩(9→3 축소).
const CLOSE_TAB_PAD: LogicalPx = LogicalPx(3.0);
/// 경계 split 존 밴드 폭 비율(변 기준 바깥 30%).
const SPLIT_ZONE_EDGE: f32 = 0.3;
/// leaf 값 요약 표시 임계(본체 `demo_layout.rs` 와 동일 구조 상수). 빈 leaf 박스가
/// 이 너비/높이 미만이면 요약을 숨기고 아이콘 + kind명만 남긴다.
const LEAF_SUMMARY_MIN_W: LogicalPx = LogicalPx(96.0);
const LEAF_SUMMARY_MIN_H: LogicalPx = LogicalPx(72.0);
/// 짧은 축이 이 값 미만이면 kind명까지 숨기고 아이콘만 남긴다(icon-only degrade).
const LEAF_ICON_ONLY_MIN: LogicalPx = LogicalPx(46.0);

// ── specimen 박스 치수 ───────────────────────────────────────────────────────
//
// 데모 박스의 가로·세로다. 디자인 토큰이 아니라 **무대 크기**라 Theme 에서 오지
// 않는다 — 값이 케이스마다 다른 이유는 그 케이스가 무엇을 보여야 하는지에 있고,
// leaf 쪽 값들은 위 degrade 임계(96×72 · 46)의 위/아래를 각각 밟도록 고른 것이다.
// 임계를 바꾸면 이 박스들도 함께 봐야 한다.

/// Workspace scope — pane split 이 가로로 자라 다른 둘보다 넓다.
const SCOPE_BOX_W_WIDE: LogicalPx = LogicalPx(320.0);
/// Tab / Pane scope 공통 가로.
const SCOPE_BOX_W: LogicalPx = LogicalPx(210.0);
/// scope 3 종 공통 세로 — 나란히 세우므로 같아야 한다.
const SCOPE_BOX_H: LogicalPx = LogicalPx(220.0);

/// 요약 2 줄이 다 보이는 박스(96×72 초과).
const LEAF_BOX_FULL: (f32, f32) = (176.0, 120.0);
/// 요약 1 줄 박스(여전히 96×72 초과).
const LEAF_BOX_ONE_ROW: (f32, f32) = (150.0, 104.0);
/// 요약이 숨는 박스 — 가로·세로 모두 96×72 미만.
const LEAF_BOX_SUMMARY_HIDDEN: (f32, f32) = (90.0, 64.0);
/// 아이콘만 남는 박스 — 짧은 축이 46 미만.
const LEAF_BOX_ICON_ONLY: (f32, f32) = (40.0, 40.0);

/// 편집 모드 박스 — 선택 outline + handle + inline form 이 함께 들어간다.
const EDIT_BOX: (f32, f32) = (300.0, 240.0);
/// 직접조작 박스 — 경계 split 존 + mini tab strip 을 함께 보인다.
const DIRECT_BOX: (f32, f32) = (320.0, 200.0);

// ── 정적 preview 모델 (디자인 build* 트리 전사) ──────────────────────

#[derive(Clone, Copy)]
enum Kind {
    Terminal,
    Markdown,
    Editor,
    Log,
}

impl Kind {
    fn icon(self) -> MockGlyph {
        match self {
            Kind::Terminal => icons::TERMINAL,
            Kind::Markdown => icons::MARKDOWN,
            Kind::Editor => icons::EDIT,
            Kind::Log => icons::LOG,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Kind::Terminal => "Terminal",
            Kind::Markdown => "Markdown",
            Kind::Editor => "Editor",
            Kind::Log => "Log",
        }
    }
    /// 디자인 KINDS accent 매핑 (terminal→success / markdown→primary / editor→agent / log→warning).
    fn accent(self, theme: &Theme) -> egui::Color32 {
        match self {
            Kind::Terminal => theme.accent_success().to_egui(),
            Kind::Markdown => theme.accent_primary().to_egui(),
            Kind::Editor => theme.accent_agent().to_egui(),
            Kind::Log => theme.accent_warning().to_egui(),
        }
    }
}

/// leaf 값 요약의 한 행 — 라벨(소문자 필드 키) + 값 + 앞자름 여부(본체 `LeafSummaryRow`
/// 전사). path-like(cwd/file) = 앞자름(경로 꼬리 유지), command/url(startup/url) = 뒤자름.
#[derive(Clone)]
struct SummaryCell {
    label: &'static str,
    value: &'static str,
    front_elide: bool,
}

/// surface leaf — kind + 값 요약(비지 않은 필드 행). 요약이 비면 아이콘 + kind명만.
struct DemoLeaf {
    kind: Kind,
    summary: Vec<SummaryCell>,
}

/// 하위 레이아웃(surface split) 트리.
enum Surf {
    Leaf(DemoLeaf),
    Split {
        row: bool,
        ratio: f32,
        first: Box<Surf>,
        second: Box<Surf>,
    },
}

struct DemoTab {
    name: &'static str,
    layout: Surf,
}

/// 상위 레이아웃(pane split) 트리.
enum Pane {
    Leaf {
        tabs: Vec<DemoTab>,
        active: usize,
    },
    Split {
        row: bool,
        ratio: f32,
        first: Box<Pane>,
        second: Box<Pane>,
    },
}

/// scope variant — workspace/pane 은 pane 트리, tab 은 단일 surface-split 트리(프레임).
enum Scope {
    PaneTree(Pane),
    TabFrame(Surf),
}

fn leaf(k: Kind) -> Surf {
    Surf::Leaf(DemoLeaf {
        kind: k,
        summary: Vec::new(),
    })
}
fn cell(label: &'static str, value: &'static str, front_elide: bool) -> SummaryCell {
    SummaryCell {
        label,
        value,
        front_elide,
    }
}
fn ssplit(row: bool, ratio: f32, a: Surf, b: Surf) -> Surf {
    Surf::Split {
        row,
        ratio,
        first: Box::new(a),
        second: Box::new(b),
    }
}
fn tab(name: &'static str, layout: Surf) -> DemoTab {
    DemoTab { name, layout }
}
fn pleaf(tabs: Vec<DemoTab>, active: usize) -> Pane {
    Pane::Leaf { tabs, active }
}
fn psplit(row: bool, ratio: f32, a: Pane, b: Pane) -> Pane {
    Pane::Split {
        row,
        ratio,
        first: Box::new(a),
        second: Box::new(b),
    }
}

// 디자인 buildWorkspace / buildTab / buildPane 전사.
fn build_workspace() -> Scope {
    Scope::PaneTree(psplit(
        true,
        0.6,
        pleaf(
            vec![
                tab(
                    "edit",
                    ssplit(false, 0.64, leaf(Kind::Editor), leaf(Kind::Terminal)),
                ),
                tab("agent", leaf(Kind::Editor)),
            ],
            0,
        ),
        pleaf(
            vec![
                tab("preview", leaf(Kind::Markdown)),
                tab(
                    "logs",
                    ssplit(true, 0.5, leaf(Kind::Log), leaf(Kind::Terminal)),
                ),
            ],
            0,
        ),
    ))
}
fn build_tab() -> Scope {
    Scope::TabFrame(ssplit(
        true,
        0.5,
        leaf(Kind::Editor),
        ssplit(false, 0.5, leaf(Kind::Terminal), leaf(Kind::Log)),
    ))
}
fn build_pane() -> Scope {
    Scope::PaneTree(pleaf(
        vec![
            tab("server", leaf(Kind::Terminal)),
            tab(
                "dev",
                ssplit(false, 0.5, leaf(Kind::Terminal), leaf(Kind::Log)),
            ),
            tab("notes", leaf(Kind::Markdown)),
        ],
        0,
    ))
}

// ── rect 분할 헬퍼 ──────────────────────────────────────────────────

/// `rect` 를 split 비율로 나눈다. `divider` 만큼을 가운데 띠로 빼고 first/second 에 분배.
/// 반환 = (first, divider_rect, second). surface 는 divider 를 1px hairline 으로 칠하고,
/// pane 은 divider 를 칠하지 않아 bg-app 공백(=상위 divider)으로 남긴다.
fn split_rects(
    rect: egui::Rect,
    row: bool,
    ratio: f32,
    divider: LogicalPx,
) -> (egui::Rect, egui::Rect, egui::Rect) {
    if row {
        let avail = (LogicalPx(rect.width()) - divider).max(LogicalPx(0.0));
        let fw = avail * ratio;
        let first = egui::Rect::from_min_size(rect.min, egui::vec2(fw.value(), rect.height()));
        let mid = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + fw.value(), rect.min.y),
            egui::vec2(divider.value(), rect.height()),
        );
        let second = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + (fw + divider).value(), rect.min.y),
            egui::vec2((avail - fw).value(), rect.height()),
        );
        (first, mid, second)
    } else {
        let avail = (LogicalPx(rect.height()) - divider).max(LogicalPx(0.0));
        let fh = avail * ratio;
        let first = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), fh.value()));
        let mid = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + fh.value()),
            egui::vec2(rect.width(), divider.value()),
        );
        let second = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + (fh + divider).value()),
            egui::vec2(rect.width(), (avail - fh).value()),
        );
        (first, mid, second)
    }
}

// ── 재귀 렌더 ───────────────────────────────────────────────────────

/// 하위 레이아웃(surface split). Leaf = kind 박스, Split = 1px hairline 으로 분할.
fn draw_surf(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, node: &Surf) {
    match node {
        Surf::Leaf(l) => draw_surface_box(ui, theme, rect, l),
        Surf::Split {
            row,
            ratio,
            first,
            second,
        } => {
            let (r1, line, r2) = split_rects(rect, *row, *ratio, theme.border_width);
            draw_surf(ui, theme, r1, first);
            ui.painter_at(rect)
                .rect_filled(line, 0.0, theme.border_default().to_egui());
            draw_surf(ui, theme, r2, second);
        }
    }
}

/// surface leaf — bg-app fill, 가운데 kind 아이콘(accent) + 표시명(mono, secondary) +
/// 값 요약(중앙 정렬). 본체 `demo_layout.rs::draw_leaf_preview` 와 같은 규칙으로 그린다:
/// 값이 채워진 필드를 `키 값` 한 줄로, 박스 <96×72 → 요약 숨김, 짧은 축 <46 → kind명도 숨김.
///
/// **본체를 부르지 않는 것이 의도다.** 갤러리는 본체를 비추는 specimen 이라 본체 함수를
/// 부르면 거울이 아니라 본체 자신이 되고, 본체가 틀리게 그려도 갤러리가 똑같이 틀리게
/// 그려 대조가 사라진다(`docs/dev-guide/gallery-first.md`).
fn draw_surface_box(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, leaf: &DemoLeaf) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_app().to_egui());

    let icon = theme.icon_glyph_size_md;
    let label_h = theme.font_size_caption;
    // summary-gap = 행↔행, kind명↔요약, 라벨↔값 gap 모두 space-xs.
    let gap = theme.spacing_xs;
    let row_h = theme.font_size_caption;

    let short_axis = rect.width().min(rect.height());
    let show_kind = short_axis >= LEAF_ICON_ONLY_MIN.value();
    let show_summary = show_kind
        && rect.width() >= LEAF_SUMMARY_MIN_W.value()
        && rect.height() >= LEAF_SUMMARY_MIN_H.value();
    let rows: &[SummaryCell] = if show_summary { &leaf.summary } else { &[] };

    let mut total = icon;
    if show_kind {
        total += LEAF_GAP + label_h;
    }
    if !rows.is_empty() {
        total += gap + row_h * rows.len() as f32 + gap * (rows.len() as f32 - 1.0);
    }

    let cx_x = LogicalPx(rect.center().x);
    let mut y = LogicalPx(rect.center().y) - total.scaled(0.5);

    paint_glyph(
        ui,
        leaf.kind.icon(),
        egui::pos2(cx_x.value(), (y + icon.scaled(0.5)).value()),
        icon,
        leaf.kind.accent(theme),
    );
    y += icon;

    if show_kind {
        y += LEAF_GAP;
        ui.painter_at(rect).text(
            egui::pos2(cx_x.value(), (y + label_h.scaled(0.5)).value()),
            egui::Align2::CENTER_CENTER,
            leaf.kind.label(),
            egui::FontId::monospace(label_h.value()),
            theme.text_secondary().to_egui(),
        );
        y += label_h;
    }

    if !rows.is_empty() {
        y += gap;
        let label_font = egui::FontId::monospace(theme.font_size_micro.value());
        let value_font = egui::FontId::monospace(row_h.value());
        let inner_w = (LogicalPx(rect.width()) - gap.scaled(2.0)).max(LogicalPx(0.0));
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                y += gap;
            }
            let row_cy = y + row_h.scaled(0.5);
            let label_w = LogicalPx(text_width(ui, row.label, label_font.clone()));
            let avail = (inner_w - label_w - gap).max(LogicalPx(0.0));
            let value = elide_to_width(ui, row.value, value_font.clone(), avail, row.front_elide);
            let value_w = LogicalPx(text_width(ui, &value, value_font.clone()));
            let line_w = label_w + gap + value_w;
            let start_x = cx_x - line_w.scaled(0.5);
            let p = ui.painter_at(rect);
            p.text(
                egui::pos2(start_x.value(), row_cy.value()),
                egui::Align2::LEFT_CENTER,
                row.label,
                label_font.clone(),
                theme.preset_leaf_label_fg().to_egui(),
            );
            p.text(
                egui::pos2((start_x + label_w + gap).value(), row_cy.value()),
                egui::Align2::LEFT_CENTER,
                &value,
                value_font.clone(),
                theme.preset_leaf_value_fg().to_egui(),
            );
            y += row_h;
        }
    }
}

/// 한 줄 ellipsis(본체 `elide_to_width` 전사). `front=true` 면 선두를 잘라 앞에 `…`
/// (경로 꼬리 유지), false 면 말미를 잘라 뒤에 `…`.
fn elide_to_width(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    max_w: LogicalPx,
    front: bool,
) -> String {
    if max_w <= LogicalPx(0.0) {
        return String::new();
    }
    if LogicalPx(text_width(ui, text, font.clone())) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    if front {
        for start in 1..chars.len() {
            let candidate: String = std::iter::once('…')
                .chain(chars[start..].iter().copied())
                .collect();
            if LogicalPx(text_width(ui, &candidate, font.clone())) <= max_w {
                return candidate;
            }
        }
        "…".to_string()
    } else {
        for end in (1..chars.len()).rev() {
            let candidate: String = chars[..end]
                .iter()
                .copied()
                .chain(std::iter::once('…'))
                .collect();
            if LogicalPx(text_width(ui, &candidate, font.clone())) <= max_w {
                return candidate;
            }
        }
        "…".to_string()
    }
}

/// 상위 레이아웃(pane split). Leaf = pane 카드, Split = 5px bg-app gap 으로 분할.
fn draw_pane_tree(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, node: &Pane) {
    match node {
        Pane::Leaf { tabs, active } => draw_pane_card(ui, theme, rect, tabs, *active),
        Pane::Split {
            row,
            ratio,
            first,
            second,
        } => {
            // divider(=PANE_GAP) 는 칠하지 않는다 — preview body 의 bg-app 이 그대로 비쳐
            // 무거운 상위 divider 가 된다 (surface hairline 보다 의도적으로 두껍게).
            let (r1, _gap, r2) = split_rects(rect, *row, *ratio, PANE_GAP);
            draw_pane_tree(ui, theme, r1, first);
            draw_pane_tree(ui, theme, r2, second);
        }
    }
}

/// pane 카드 = 테두리 카드 + mini tab strip + 활성 탭의 surface 레이아웃.
fn draw_pane_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    tabs: &[DemoTab],
    active: usize,
) {
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    let sep = theme.separator.to_egui();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, radius, theme.bg_app().to_egui());

    // mini tab strip.
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), STRIP_H.value()));
    p.rect_filled(strip, 0.0, theme.bg_sidebar().to_egui());

    let tab_font = egui::FontId::proportional(theme.font_size_caption.value());
    let icon_sz = theme.icon_glyph_size_sm;
    let mut x = LogicalPx(strip.min.x);
    for (i, t) in tabs.iter().enumerate() {
        let on = i == active;
        let lw = LogicalPx(text_width(ui, t.name, tab_font.clone()));
        let tw = TAB_PAD_X + icon_sz + TAB_GAP + lw + TAB_PAD_X;
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(x.value(), strip.min.y),
            egui::vec2(tw.value(), STRIP_H.value()),
        );
        if on {
            p.rect_filled(tab_rect, 0.0, theme.bg_panel().to_egui());
            // 2px accent 하단 bar.
            let bar = egui::Rect::from_min_size(
                egui::pos2(
                    tab_rect.min.x,
                    tab_rect.max.y - theme.tab_indicator_width.value(),
                ),
                egui::vec2(tw.value(), theme.tab_indicator_width.value()),
            );
            p.rect_filled(bar, 0.0, theme.accent_primary().to_egui());
        }
        if i > 0 {
            // 탭 사이 separator (borderRight).
            p.vline(x.value(), strip.y_range(), egui::Stroke::new(bw, sep));
        }
        let icon_c = egui::pos2(
            tab_rect.min.x + (TAB_PAD_X + icon_sz.scaled(0.5)).value(),
            tab_rect.center().y,
        );
        let icon_color = if on {
            tab_kind(t).accent(theme)
        } else {
            theme.text_muted().to_egui()
        };
        paint_glyph(ui, tab_kind(t).icon(), icon_c, icon_sz, icon_color);
        ui.painter_at(strip).text(
            egui::pos2(
                tab_rect.min.x + (TAB_PAD_X + icon_sz + TAB_GAP).value(),
                tab_rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            t.name,
            tab_font.clone(),
            if on {
                theme.text_primary().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );
        x += tw;
    }
    // strip border-bottom.
    ui.painter_at(rect)
        .hline(strip.x_range(), strip.max.y, egui::Stroke::new(bw, sep));

    // 활성 탭 본문 — padding 3, bg-app.
    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, strip.max.y), rect.max);
    let inner = body.shrink(BODY_PAD.value());
    let active_tab = tabs.get(active).or_else(|| tabs.first());
    if let Some(t) = active_tab {
        draw_surf(ui, theme, inner, &t.layout);
    }

    // 카드 외곽 border.
    ui.painter_at(rect).rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// 탭 대표 kind = 첫 leaf (디자인 `activeKind`, mini-tab 아이콘 구동).
fn tab_kind(t: &DemoTab) -> Kind {
    let mut n = &t.layout;
    loop {
        match n {
            Surf::Leaf(l) => return l.kind,
            Surf::Split { first, .. } => n = first,
        }
    }
}

// ── 편집 상태 렌더 (디자인 `SurfaceBox` edit + `MiniHandle`) ──
//
// 정적 specimen 이라 인터랙션은 없다 — "편집 모드의 시각"만 보인다. 모든 surface 가
// 1px separator outline 을 달고, `selected`(방문 순서 index) surface 는 2px accent
// inset outline + 우상단 handle cluster(설정 · remove)를 보인다. 선택돼도 가운데는
// 비선택과 같은 kind 아이콘 + 표시명이다 — 칸 안에 폼을 그리지 않는다.

/// 편집 트리 순회 상태 — leaf 방문 순서 index 로 선택 leaf 를 지정한다.
struct EditWalk {
    next: usize,
    selected: usize,
}

fn draw_surf_edit(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    node: &Surf,
    w: &mut EditWalk,
) {
    match node {
        Surf::Leaf(l) => {
            let idx = w.next;
            w.next += 1;
            draw_surface_box_edit(ui, theme, rect, l.kind, idx == w.selected);
        }
        Surf::Split {
            row,
            ratio,
            first,
            second,
        } => {
            let (r1, line, r2) = split_rects(rect, *row, *ratio, theme.border_width);
            draw_surf_edit(ui, theme, r1, first, w);
            ui.painter_at(rect)
                .rect_filled(line, 0.0, theme.border_default().to_egui());
            draw_surf_edit(ui, theme, r2, second, w);
        }
    }
}

/// 편집 상태 surface — 중앙 라벨(선택 여부 무관) + 비선택 1px separator outline /
/// 선택 2px accent outline + handle cluster.
fn draw_surface_box_edit(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    kind: Kind,
    selected: bool,
) {
    ui.painter_at(rect)
        .rect_filled(rect, 0.0, theme.bg_app().to_egui());

    let icon = theme.icon_glyph_size_md;
    let label_h = theme.font_size_caption;
    let total = icon + LEAF_GAP + label_h;
    let icon_cy = LogicalPx(rect.center().y) - total.scaled(0.5) + icon.scaled(0.5);
    paint_glyph(
        ui,
        kind.icon(),
        egui::pos2(rect.center().x, icon_cy.value()),
        icon,
        kind.accent(theme),
    );
    ui.painter_at(rect).text(
        egui::pos2(
            rect.center().x,
            (icon_cy + icon.scaled(0.5) + LEAF_GAP + label_h.scaled(0.5)).value(),
        ),
        egui::Align2::CENTER_CENTER,
        kind.label(),
        egui::FontId::monospace(label_h.value()),
        theme.text_secondary().to_egui(),
    );

    // outline: 선택 = 2px accent, 비선택 = 1px separator (편집 가능 영역 표시).
    if selected {
        let bw = theme.tab_indicator_width.value();
        ui.painter_at(rect).rect_stroke(
            rect.shrink(bw * 0.5),
            0.0,
            egui::Stroke::new(bw, theme.accent_primary().to_egui()),
            egui::StrokeKind::Inside,
        );
        draw_handle_cluster_mock(ui, theme, rect);
    } else {
        let bw = theme.border_width.value();
        ui.painter_at(rect).rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(bw, theme.separator.to_egui()),
            egui::StrokeKind::Inside,
        );
    }
}

/// 우상단 handle cluster mock — `[설정(톱니)] [remove(danger)]`. split-right/down
/// 핸들은 경계 hover-split 존이 대체해 제거됐다.
fn draw_handle_cluster_mock(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let remove = egui::Rect::from_min_size(
        egui::pos2(
            rect.max.x - (E_HANDLE_INSET + E_HANDLE_SZ).value(),
            rect.min.y + E_HANDLE_INSET.value(),
        ),
        egui::vec2(E_HANDLE_SZ.value(), E_HANDLE_SZ.value()),
    );
    let settings = remove.translate(egui::vec2(-(E_HANDLE_SZ + E_HANDLE_GAP).value(), 0.0));
    mini_handle_mock(ui, theme, settings, icons::SETTINGS, false);
    mini_handle_mock(ui, theme, remove, icons::TRASH, true);
}

/// 경계 split 존 overlay mock — Left 존 활성 예시(밴드 채움 + 안쪽 변 2px 분할선).
/// 정적 specimen 이라 crosshair 커서·실시간 hover 추적은 없다 — "존 활성"을 **고정
/// 상태**로만 전사한다(본체 live 와 100% 동형 불가 → parity-notes).
fn draw_split_zone_overlay_mock(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let x = rect.min.x + rect.width() * SPLIT_ZONE_EDGE;
    let band = egui::Rect::from_min_max(rect.min, egui::pos2(x, rect.max.y));
    let divider = theme.tab_indicator_width.value(); // 2px 분할선(accent bar 와 동일 굵기).
    let p = ui.painter_at(rect);
    p.rect_filled(band, 0.0, theme.preset_split_zone_bg().to_egui());
    p.vline(
        x,
        band.y_range(),
        egui::Stroke::new(divider, theme.preset_split_zone_border().to_egui()),
    );
}

fn mini_handle_mock(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    glyph: MockGlyph,
    danger: bool,
) {
    let radius = theme.corner_radius_sm.value();
    ui.painter_at(rect).rect(
        rect,
        radius,
        theme.surface_raised().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_strong().to_egui()),
        egui::StrokeKind::Inside,
    );
    let color = if danger {
        theme.accent_danger().to_egui()
    } else {
        theme.text_secondary().to_egui()
    };
    paint_glyph(ui, glyph, rect.center(), E_HANDLE_SZ.scaled(0.62), color);
}

/// 편집 상태 Tab scope mock 프레임(strip 없음 + selected surface).
fn draw_scope_body_edit(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    surf: &Surf,
    selected: usize,
) {
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, radius, theme.bg_app().to_egui());
    let mut w = EditWalk { next: 0, selected };
    draw_surf_edit(ui, theme, rect.shrink(BODY_PAD.value()), surf, &mut w);
    ui.painter_at(rect).rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
}

fn draw_scope_body(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, scope: &Scope) {
    match scope {
        Scope::PaneTree(p) => draw_pane_tree(ui, theme, rect, p),
        Scope::TabFrame(s) => {
            // 단일 탭 본문처럼 프레임(테두리 + radius + padding 3), strip 없음.
            let radius = theme.corner_radius.value();
            let bw = theme.border_width.value();
            let p = ui.painter_at(rect);
            p.rect_filled(rect, radius, theme.bg_app().to_egui());
            draw_surf(ui, theme, rect.shrink(BODY_PAD.value()), s);
            ui.painter_at(rect).rect_stroke(
                rect,
                radius,
                egui::Stroke::new(bw, theme.border_default().to_egui()),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// 라벨 붙은 scope 데모 한 칸 — 제목/부제 + 미리보기 캔버스(LivePreview outer 전사).
fn scope_demo(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    sub: &str,
    scope: &Scope,
    w: f32,
    h: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new(label)
                    .size(theme.font_size_body.value())
                    .strong()
                    .color(theme.text_primary().to_egui()),
            );
            ui.label(
                egui::RichText::new(sub)
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
        // LivePreview outer: padding 10, bg-app, radius, 1px border-default.
        let (canvas, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
        let radius = theme.corner_radius.value();
        let bw = theme.border_width.value();
        let p = ui.painter_at(canvas);
        p.rect_filled(canvas, radius, theme.bg_app().to_egui());
        p.rect_stroke(
            canvas,
            radius,
            egui::Stroke::new(bw, theme.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
        draw_scope_body(ui, theme, canvas.shrink(theme.spacing_sm.value()), scope);
    });
}

/// leaf 값 요약 데모 한 칸 — 단일 leaf 박스를 지정 크기로 그려 요약/앞뒤자름/degrade
/// 를 보인다. bg-app fill + 1px border-default 로 박스 경계를 드러낸다.
fn leaf_summary_demo(
    ui: &mut egui::Ui,
    theme: &Theme,
    caption: &str,
    leaf: &DemoLeaf,
    w: f32,
    h: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        ui.label(
            egui::RichText::new(caption)
                .monospace()
                .size(theme.font_size_micro.value())
                .color(theme.text_muted().to_egui()),
        );
        let (canvas, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
        draw_surface_box(ui, theme, canvas, leaf);
        ui.painter_at(canvas).rect_stroke(
            canvas,
            0.0,
            egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
    });
}

/// 라벨 붙은 **편집 상태** scope 데모 한 칸 — selected surface 기준.
// 갤러리 데모 draw 헬퍼 — 인자는 즉시모드 draw 컨텍스트(ui/theme/라벨/크기 등)라
// context struct 로 묶어도 호출부에서 다시 풀어써야 해 이득이 없다. 정책 #2(데모 코드) 허용.
#[allow(clippy::too_many_arguments)]
fn scope_demo_edit(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    sub: &str,
    surf: &Surf,
    selected: usize,
    w: f32,
    h: f32,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new(label)
                    .size(theme.font_size_body.value())
                    .strong()
                    .color(theme.text_primary().to_egui()),
            );
            ui.label(
                egui::RichText::new(sub)
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
        let (canvas, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
        let radius = theme.corner_radius.value();
        let bw = theme.border_width.value();
        let p = ui.painter_at(canvas);
        p.rect_filled(canvas, radius, theme.bg_app().to_egui());
        p.rect_stroke(
            canvas,
            radius,
            egui::Stroke::new(bw, theme.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
        draw_scope_body_edit(
            ui,
            theme,
            canvas.shrink(theme.spacing_sm.value()),
            surf,
            selected,
        );
    });
}

/// 편집 직접조작(preset-edit-03) mock — pane 카드 편집 상태에서 세 신규 마우스
/// affordance 를 **고정 상태 예시**로 전사한다: ① 경계 hover-split 존 overlay(본문
/// leaf 의 Left 존 활성), ② mini tab close `×`(active 탭 rest + hover 탭 강조 두 상태),
/// ③ add-tab `+`(hover 상태). 정적이라 실제 hover/crosshair 추적은 없다(parity-notes).
fn draw_edit_direct_mock(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    let radius = theme.corner_radius.value();
    let bw = theme.border_width.value();
    let sep = theme.separator.to_egui();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, radius, theme.bg_app().to_egui());

    // mini tab strip.
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), STRIP_H.value()));
    p.rect_filled(strip, 0.0, theme.bg_sidebar().to_egui());

    let tab_font = egui::FontId::proportional(theme.font_size_caption.value());
    let icon_sz = theme.icon_glyph_size_sm;
    // (kind, name, active, hovered) — active/hover 탭이 close `×` 를 노출한다(탭 2개 → 가드 통과).
    let tabs: &[(Kind, &str, bool, bool)] = &[
        (Kind::Editor, "edit", true, false),
        (Kind::Terminal, "term", false, true),
    ];
    let mut x = LogicalPx(strip.min.x);
    for (i, (kind, name, on, hovered)) in tabs.iter().enumerate() {
        let lw = LogicalPx(text_width(ui, name, tab_font.clone()));
        // × 예약: 편집 && 탭>1 → 우측 패딩 9→3 + marginLeft 1 + 14 close.
        let tw = TAB_PAD_X + icon_sz + TAB_GAP + lw + CLOSE_MARGIN + CLOSE_HIT + CLOSE_TAB_PAD;
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(x.value(), strip.min.y),
            egui::vec2(tw.value(), STRIP_H.value()),
        );
        let p = ui.painter_at(strip);
        if *on {
            p.rect_filled(tab_rect, 0.0, theme.bg_panel().to_egui());
            let bar = egui::Rect::from_min_size(
                egui::pos2(
                    tab_rect.min.x,
                    tab_rect.max.y - theme.tab_indicator_width.value(),
                ),
                egui::vec2(tw.value(), theme.tab_indicator_width.value()),
            );
            p.rect_filled(bar, 0.0, theme.accent_primary().to_egui());
        }
        if i > 0 {
            p.vline(x.value(), strip.y_range(), egui::Stroke::new(bw, sep));
        }
        let icon_c = egui::pos2(
            tab_rect.min.x + (TAB_PAD_X + icon_sz.scaled(0.5)).value(),
            tab_rect.center().y,
        );
        let icon_color = if *on {
            kind.accent(theme)
        } else {
            theme.text_muted().to_egui()
        };
        paint_glyph(ui, kind.icon(), icon_c, icon_sz, icon_color);
        ui.painter_at(strip).text(
            egui::pos2(
                tab_rect.min.x + (TAB_PAD_X + icon_sz + TAB_GAP).value(),
                tab_rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            name,
            tab_font.clone(),
            if *on {
                theme.text_primary().to_egui()
            } else {
                theme.text_muted().to_egui()
            },
        );
        // close `×` — active/hover 탭에 노출. hover 예시 = overlay-active fill + text-primary.
        let close_rect = egui::Rect::from_min_size(
            egui::pos2(
                tab_rect.max.x - (CLOSE_TAB_PAD + CLOSE_HIT).value(),
                tab_rect.center().y - CLOSE_HIT.scaled(0.5).value(),
            ),
            egui::vec2(CLOSE_HIT.value(), CLOSE_HIT.value()),
        );
        let close_color = if *hovered {
            ui.painter_at(strip).rect_filled(
                close_rect,
                theme.corner_radius_sm.value(),
                theme.overlay_active().to_egui(),
            );
            theme.text_primary().to_egui()
        } else {
            theme.text_muted().to_egui()
        };
        paint_glyph(
            ui,
            icons::CLOSE,
            close_rect.center(),
            CLOSE_HIT.scaled(0.5),
            close_color,
        );
        x += tw;
    }

    // add-tab `+` — hover 상태 예시(overlay-hover fill + text-secondary).
    let add = egui::Rect::from_min_size(
        egui::pos2(x.value(), strip.min.y),
        egui::vec2(ADD_TAB_W.value(), STRIP_H.value()),
    );
    ui.painter_at(strip)
        .rect_filled(add, 0.0, theme.overlay_hover().to_egui());
    paint_glyph(
        ui,
        icons::PLUS,
        add.center(),
        icon_sz,
        theme.text_secondary().to_egui(),
    );

    // strip border-bottom.
    ui.painter_at(rect)
        .hline(strip.x_range(), strip.max.y, egui::Stroke::new(bw, sep));

    // 활성 탭 본문 — 단일 leaf(비선택) + 경계 split 존(Left 활성) overlay.
    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, strip.max.y), rect.max);
    let inner = body.shrink(BODY_PAD.value());
    draw_surface_box_edit(ui, theme, inner, Kind::Editor, false);
    draw_split_zone_overlay_mock(ui, theme, inner);

    // 카드 외곽 border.
    ui.painter_at(rect).rect_stroke(
        rect,
        radius,
        egui::Stroke::new(bw, theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
}

/// 라벨 붙은 **편집 직접조작** 데모 한 칸 — [`draw_edit_direct_mock`] 캔버스.
fn scope_demo_direct(ui: &mut egui::Ui, theme: &Theme, label: &str, sub: &str, w: f32, h: f32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            ui.label(
                egui::RichText::new(label)
                    .size(theme.font_size_body.value())
                    .strong()
                    .color(theme.text_primary().to_egui()),
            );
            ui.label(
                egui::RichText::new(sub)
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted().to_egui()),
            );
        });
        let (canvas, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
        let radius = theme.corner_radius.value();
        let bw = theme.border_width.value();
        let p = ui.painter_at(canvas);
        p.rect_filled(canvas, radius, theme.bg_app().to_egui());
        p.rect_stroke(
            canvas,
            radius,
            egui::Stroke::new(bw, theme.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
        draw_edit_direct_mock(ui, theme, canvas.shrink(theme.spacing_sm.value()));
    });
}

fn paint_glyph(
    ui: &mut egui::Ui,
    glyph: MockGlyph,
    center: egui::Pos2,
    size: LogicalPx,
    color: egui::Color32,
) {
    let r = egui::Rect::from_center_size(center, egui::vec2(size.value(), size.value()));
    glyph.image(size.value(), color).paint_at(ui, r);
}

fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
            .size()
            .x
    })
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let workspace = build_workspace();
    let tab_scope = build_tab();
    let pane_scope = build_pane();

    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        scope_demo(
            ui,
            theme,
            "Workspace",
            "pane split + tabs + surface split",
            &workspace,
            SCOPE_BOX_W_WIDE.value(),
            SCOPE_BOX_H.value(),
        );
        scope_demo(
            ui,
            theme,
            "Tab",
            "surface split tree only",
            &tab_scope,
            SCOPE_BOX_W.value(),
            SCOPE_BOX_H.value(),
        );
        scope_demo(
            ui,
            theme,
            "Pane",
            "tab strip + active tab",
            &pane_scope,
            SCOPE_BOX_W.value(),
            SCOPE_BOX_H.value(),
        );
    });

    // leaf 값 요약 — 미선택 leaf 가 kind 아이콘 + kind명 아래에 설정값(`키 값`)을
    // 요약한다. path-like(cwd/file)=앞자름(꼬리 유지), command/url=뒤자름. degrade:
    // <96×72 → 요약 숨김, 짧은 축 <46 → 아이콘만.
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        let term = DemoLeaf {
            kind: Kind::Terminal,
            summary: vec![
                cell("cwd", "~/workspace/etc/tasty/crates/fe-ccp", true),
                cell("startup", "cargo watch -x run", false),
            ],
        };
        leaf_summary_demo(
            ui,
            theme,
            "terminal · cwd (front-elide) + startup",
            &term,
            LEAF_BOX_FULL.0,
            LEAF_BOX_FULL.1,
        );

        let md = DemoLeaf {
            kind: Kind::Markdown,
            summary: vec![cell("file", "~/tasty/docs/design/README.md", true)],
        };
        leaf_summary_demo(
            ui,
            theme,
            "markdown · file (front-elide)",
            &md,
            LEAF_BOX_ONE_ROW.0,
            LEAF_BOX_ONE_ROW.1,
        );

        let degraded = DemoLeaf {
            kind: Kind::Terminal,
            summary: vec![cell("cwd", "~/tasty", true)],
        };
        leaf_summary_demo(
            ui,
            theme,
            "degrade <96×72 · summary hidden",
            &degraded,
            LEAF_BOX_SUMMARY_HIDDEN.0,
            LEAF_BOX_SUMMARY_HIDDEN.1,
        );

        let icon_only = DemoLeaf {
            kind: Kind::Terminal,
            summary: vec![cell("cwd", "~/tasty", true)],
        };
        leaf_summary_demo(
            ui,
            theme,
            "degrade <46 · icon only",
            &icon_only,
            LEAF_BOX_ICON_ONLY.0,
            LEAF_BOX_ICON_ONLY.1,
        );
    });

    // 편집 상태: selected surface 2px accent outline + 설정 · remove handle cluster.
    let edit_tab = build_tab();
    let edit_surf = match &edit_tab {
        Scope::TabFrame(s) => s,
        Scope::PaneTree(_) => unreachable!("build_tab is a TabFrame"),
    };
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        scope_demo_edit(
            ui,
            theme,
            "Edit mode",
            "selected surface + gear · remove handles",
            edit_surf,
            1,
            EDIT_BOX.0,
            EDIT_BOX.1,
        );
    });

    // 편집 직접조작(preset-edit-03): 경계 hover-split 존 · mini tab close × · add-tab +.
    // 정적이라 고정 상태 예시로 전사(hover/crosshair 는 본체 live 전용 — parity-notes).
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        scope_demo_direct(
            ui,
            theme,
            "Edit — direct manipulation",
            "boundary split zone · tab × · add-tab",
            DIRECT_BOX.0,
            DIRECT_BOX.1,
        );
    });

    spec::meta(
        ui,
        theme,
        &[
            ("pane split", "bordered cards · 5px app-bg gap"),
            ("tab strip", "20px mini row · 2px accent bar"),
            ("surface split", "1px hairline (lower layout)"),
            ("leaf", "kind icon + label, centered (mono)"),
            ("leaf summary", "field values · key value (mono, centered)"),
            ("leaf degrade", "<96×72 hides summary · <46 icon only"),
            ("interactive", "mini tabs switch live (in app)"),
            (
                "edit: selected",
                "2px accent outline + gear · remove handles",
            ),
            ("edit: split zone", "boundary 30% band + 2px divider"),
            ("edit: tab ×", "close on active / hover (tabs > 1)"),
            ("edit: add-tab", "+ 22px, overlay-hover fill"),
            ("edit: settings", "gear or double-click → settings screen"),
        ],
        &[
            TokenChip::new("bg-app", "leaf fill / pane gap", theme.bg_app().to_egui()),
            TokenChip::new(
                "border-default",
                "pane card / surface hairline",
                theme.border_default().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "active mini tab bar",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "split-zone-bg",
                "boundary zone band (accent 22%)",
                theme.preset_split_zone_bg().to_egui(),
            ),
            TokenChip::new(
                "split-zone-border",
                "zone 2px divider (accent 55%)",
                theme.preset_split_zone_border().to_egui(),
            ),
            TokenChip::new(
                "preset-leaf-label",
                "summary field key (text-muted)",
                theme.preset_leaf_label_fg().to_egui(),
            ),
            TokenChip::new(
                "preset-leaf-value",
                "summary field value (text-secondary)",
                theme.preset_leaf_value_fg().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Two split levels read by weight — a heavy bg-app gap + border for pane (upper) splits, a \
         1px hairline for surface (lower) splits. A leaf shows its kind (Terminal / Markdown / \
         Editor / Log) plus a value summary of its configured fields (key value, mono, centered): \
         path-like keys (cwd / file) front-elide to keep the tail, command / url keys end-elide. It \
         degrades by box size — under 96×72 the summary is hidden, under 46 on the short axis only \
         the icon remains. The Edit-mode stage shows the WYSIWYG state: every \
         surface gets a faint 1px separator outline, the selected surface gets a 2px accent inset \
         outline + two handles (gear · remove) and keeps the same kind label — nothing is drawn \
         inside a cell that could clip; the gear or a double-click opens the surface settings \
         screen (next spec). The Direct-manipulation stage \
         transcribes the mouse affordances: hovering a surface boundary lights a 30% split zone \
         (accent 22% band + 2px accent 55% divider, crosshair cursor) that splits toward the edge; \
         active/hovered mini tabs show a close × (hidden when a pane has one tab); the add-tab + is \
         22px with an overlay-hover fill. Because the specimen is static, zone/× hover and the \
         crosshair are drawn as fixed-state examples — live tracking runs only in the host.",
    );

    spec::dont(
        ui,
        theme,
        "Don't render surface contents (live output). A leaf shows its kind plus a summary of its \
         configured fields (cwd / startup / file / url) — never runtime data. The preview is about \
         structure and configuration, not contents.",
    );
}
