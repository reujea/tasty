//! 탐색기 내용 영역의 상태 화면 네 개(빈 폴더 · 권한 거부 · 불러오는 중 · 읽기 오류)와
//! 즐겨찾기 추가·이름 변경 팝업을 한 무대에 놓은 예제.

mod popups;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{ButtonVariant, StateGlyph as WidgetGlyph, StateScreenView, state_screen};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 시안 상태 칸 높이(`height: 180`). 전시 치수다. 상태 줄 폭은 explorer-state-specimen-row-width 토큰이다.
const STATE_H: LogicalPx = LogicalPx(180.0);
/// 시안 팝업 두 장 사이 간격(`gap: 18`). 대응 토큰이 없다.
const POPUP_GAP: LogicalPx = LogicalPx(18.0);

/// 상태 칸의 글리프 — 아이콘 또는 Spinner.
#[derive(Clone, Copy)]
pub(super) enum StateGlyph {
    Icon(MockGlyph),
    Spinner,
}

/// 글리프와 제목의 색조. 중립은 글리프 text-muted · 제목 text-secondary 다.
#[derive(Clone, Copy)]
pub(super) enum Tone {
    Neutral,
    /// 권한 거부 — accent-warning(peach).
    Warning,
    /// 읽기 오류 — explorer-error-fg(→ accent-danger).
    Error,
}

pub(super) struct StateCell<'a> {
    pub(super) glyph: StateGlyph,
    pub(super) tone: Tone,
    pub(super) title: &'a str,
    pub(super) sub: Option<&'a str>,
    /// 번역하지 않은 OS 이유 문구(mono caption).
    pub(super) reason: Option<&'a str>,
    /// 버튼 줄(라벨, variant). 시안 Retry(secondary sm) · Go up(ghost sm).
    pub(super) actions: &'static [(&'static str, ButtonVariant)],
}

const STATES: &[StateCell<'static>] = &[
    StateCell {
        glyph: StateGlyph::Icon(icons::FOLDER_OPEN),
        tone: Tone::Neutral,
        title: "This folder is empty",
        sub: None,
        reason: None,
        actions: &[],
    },
    StateCell {
        glyph: StateGlyph::Icon(icons::LOCK),
        tone: Tone::Warning,
        title: "Permission denied",
        sub: Some("You don't have access to read this folder."),
        reason: None,
        actions: &[],
    },
    StateCell {
        glyph: StateGlyph::Spinner,
        tone: Tone::Neutral,
        title: "Loading…",
        sub: None,
        reason: None,
        actions: &[],
    },
    StateCell {
        glyph: StateGlyph::Icon(icons::ALERT_TRIANGLE),
        tone: Tone::Error,
        title: "Can't read this folder",
        sub: None,
        reason: Some("No such file or directory (os error 2)"),
        actions: &[
            ("Retry", ButtonVariant::Secondary),
            ("Go up", ButtonVariant::Ghost),
        ],
    },
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let row_w = theme.explorer_state_specimen_row_width();
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
            ui.horizontal(|ui| {
                let gap = theme.spacing_md.value();
                ui.spacing_mut().item_spacing.x = gap;
                let n = STATES.len() as f32;
                let cell_w = (row_w.value() - gap * (n - 1.0)) / n;
                for s in STATES {
                    state_cell(ui, theme, s, cell_w);
                }
            });
            // 시안 팝업 줄은 `justifyContent: center` 라 상태 줄과 같은 축의 가운데에 둔다.
            ui.horizontal_top(|ui| {
                let popups_w = theme.measure_sm * 2.0 + POPUP_GAP;
                ui.add_space(((row_w - popups_w) * 0.5).value().max(0.0));
                ui.spacing_mut().item_spacing.x = POPUP_GAP.value();
                popups::favorite(ui, theme);
                popups::rename(ui, theme);
            });
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("empty", "centered glyph + line"),
            ("permission", "warning (peach) tone"),
            ("loading", "Spinner — busy-indicator policy"),
            (
                "read error",
                "alertTriangle · explorer-error-fg · mono OS reason (max 200, untranslated)",
            ),
            (
                "error actions",
                "Retry secondary sm (re-read) · Go up ghost sm (parent, hidden at a root)",
            ),
            ("popups", "Popup language · ghost + primary footer"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "permission tone",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "explorer-error-fg",
                "read error tone",
                theme.explorer_error_fg().to_egui(),
            ),
            TokenChip::new(
                "spinner-indicator",
                "loading",
                theme.spinner_indicator().to_egui(),
            ),
            TokenChip::without_color("shadow-modal", "popup lift"),
        ],
    );

    spec::note(
        ui,
        theme,
        "Explorer font (family + size, 14px cap) gets an Appearance › Explorer sub-tab \
         mirroring the Terminal/Markdown font sections — same control layout, deferred to the \
         Settings specimen.",
    );
}

/// 내용 영역 상태 칸 하나 — 전시 칸 배경 위에 공용 상태 화면을 그린다.
fn state_cell(ui: &mut egui::Ui, theme: &Theme, s: &StateCell<'_>, w: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, STATE_H.value()), egui::Sense::hover());
    let radius = theme.corner_radius.value();
    ui.painter()
        .rect_filled(rect, radius, theme.bg_panel().to_egui());
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
        egui::StrokeKind::Inside,
    );

    state_block(ui, theme, rect, s);
}

/// 받은 사각형 가운데에 상태 블록(글리프 · 제목 · 보조 줄 · 이유 줄 · 버튼 줄)을 그린다.
/// 미리보기 패널 예제도 같은 블록을 쓴다.
pub(super) fn state_block(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, s: &StateCell<'_>) {
    let muted = theme.text_muted().to_egui();
    let (glyph_color, title_color) = match s.tone {
        Tone::Neutral => (muted, theme.text_secondary().to_egui()),
        Tone::Warning => {
            let c = theme.accent_warning().to_egui();
            (c, c)
        }
        Tone::Error => {
            let c = theme.explorer_error_fg().to_egui();
            (c, c)
        }
    };
    // 배치는 본체 탐색기와 같은 공용 상태 화면이 맡는다.
    let paint;
    let glyph = match s.glyph {
        StateGlyph::Icon(g) => {
            paint = move |ui: &mut egui::Ui, r: egui::Rect, c: egui::Color32| {
                g.image(r.height(), c).paint_at(ui, r);
            };
            WidgetGlyph::Paint(&paint)
        }
        StateGlyph::Spinner => WidgetGlyph::Spinner,
    };
    state_screen(
        ui,
        theme,
        rect,
        &StateScreenView {
            glyph,
            glyph_color,
            title: s.title,
            title_color,
            sub: s.sub,
            reason: s.reason,
            actions: s.actions,
        },
    );
}
