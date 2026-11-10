//! 공용 draw_status_bar_view에 데이터와 폭을 전달하는 상태바 예제.
//! 표시할 항목을 직접 숨기지 않고 실제 위젯이 폭에 따라 선택하게 한다.
//! 예제 폭은 각 생략 단계를 확인하도록 고른 값이며 제품의 고정 폭이 아니다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{StatusBarData, draw_status_bar_view};

use crate::catalog::spec::{StageVariant, TokenChip, cluster, dont, meta, note, stage};

/// specimen 공통 기본값 — 변형마다 필요한 필드만 덮어쓴다.
fn base() -> StatusBarData {
    StatusBarData {
        remote: None,
        branch: Some("feat/dag-detail".into()),
        surface_id: Some(3),
        pane_id: Some(1),
        shell: Some("zsh".into()),
        grid: Some((120, 32)),
        theme_is_light: false,
        palette_keys: "Ctrl+K".into(),
        palette_tooltip: crate::i18n::t("tutorial.step_palette_title").into(),
        theme_tooltip: "Toggle light / dark theme".into(),
    }
}

/// 각 변종의 바 폭. 본체에서는 작업 컬럼 폭이 들어오는 자리다.
/// 앞의 둘은 디자인 jsx 의 수 그대로이고, 뒤의 넷은 **단계를 띄우는 폭**이다(모듈 doc).
const WIDE: LogicalPx = LogicalPx(720.0);
const MID: LogicalPx = LogicalPx(470.0);
const DROPS_SHELL: LogicalPx = LogicalPx(330.0);
const DROPS_SURFACE_ID: LogicalPx = LogicalPx(270.0);
const DROPS_PALETTE: LogicalPx = LogicalPx(210.0);
const FLOOR: LogicalPx = LogicalPx(60.0);

/// 말줄임을 띄우려면 [`BRANCH_MAX_W`](tasty_ui_widgets) 를 넘는 이름이 필요하다 —
/// 디자인 specimen 의 긴 브랜치 그대로다.
const LONG_BRANCH: &str = "feat/dag-detail-and-runner-badge";

/// 세 폭의 예제에서 같은 긴 브랜치를 사용해 축소 단계를 비교한다.
fn long_branch() -> StatusBarData {
    StatusBarData {
        branch: Some(LONG_BRANCH.into()),
        ..base()
    }
}

/// 한 변형을 주어진 폭으로 그린다. 카드가 더 좁으면 카드 폭이 이긴다.
fn bar(ui: &mut egui::Ui, theme: &Theme, w: LogicalPx, data: &StatusBarData) {
    let w = LogicalPx(ui.available_width().min(w.value()));
    draw_status_bar_view(ui, theme, w, data);
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    stage(ui, theme, StageVariant::Tight, |ui| {
        cluster(ui, theme, "wide — everything", |ui| {
            bar(ui, theme, WIDE, &base());
        });
        cluster(ui, theme, "narrower — grid then shell drop", |ui| {
            bar(ui, theme, DROPS_SHELL, &long_branch());
        });
        cluster(
            ui,
            theme,
            "narrow — surface id drops, branch truncates",
            |ui| {
                bar(ui, theme, DROPS_SURFACE_ID, &long_branch());
            },
        );
        cluster(ui, theme, "narrower still — the palette cap drops", |ui| {
            bar(ui, theme, DROPS_PALETTE, &long_branch());
        });
        cluster(ui, theme, "floor — branch glyph + theme glyph", |ui| {
            bar(ui, theme, FLOOR, &base());
        });
        cluster(ui, theme, "not a repo — the branch item is absent", |ui| {
            bar(
                ui,
                theme,
                MID,
                &StatusBarData {
                    branch: None,
                    ..base()
                },
            );
        });
        cluster(
            ui,
            theme,
            "detached HEAD — short sha in the branch slot",
            |ui| {
                bar(
                    ui,
                    theme,
                    MID,
                    &StatusBarData {
                        branch: Some("@ 4f9c1ab".into()),
                        ..base()
                    },
                );
            },
        );
        cluster(
            ui,
            theme,
            "non-terminal surface / no palette binding — both items absent",
            |ui| {
                bar(
                    ui,
                    theme,
                    MID,
                    &StatusBarData {
                        shell: None,
                        grid: None,
                        palette_keys: String::new(),
                        theme_is_light: true,
                        ..base()
                    },
                );
            },
        );
    });

    note(
        ui,
        theme,
        "본체는 이 view 를 `egui::Area`(Order::Foreground) 안에서 호출한다 — Area 와 \
         z-order 는 본체 정책이라 view 가 소유하지 않는다. tooltip 도 본체 wrapper 가 \
         i18n 에서 주입한다(위젯 crate 는 i18n 비의존). 바는 포커스 surface 의 읽기 전용 \
         요약이라 어느 항목도 포커스를 옮기지 않는다.",
    );

    meta(
        ui,
        theme,
        &[
            ("height", "status-bar-height (24) — UI 배율 밖"),
            ("layout", "padding 0 10 · item gap 10"),
            (
                "left",
                "remote (mirror only) · branch · surface id · shell · grid",
            ),
            ("right", "palette keycap · theme glyph"),
            ("mono", "surface id · grid 만"),
            (
                "drop order",
                "1 grid → 2 shell → 3 surface id → 4 palette cap → 5 branch text (glyph stays) → \
                 6 remote host text on a mirror (remote glyph stays)",
            ),
            ("theme", "a sun / theme glyph, not a coloured dot"),
            ("never drops", "theme glyph"),
            ("no value", "항목이 자리째 없다 — dash 를 그리지 않는다"),
            (
                "glyphs",
                "git-branch / sun / theme · statusbar-glyph-size → icon-size-xs (12)",
            ),
            ("border-top", "border-width separator"),
            ("detached HEAD", "short sha in the branch slot"),
            ("interaction", "read-only; no item moves focus"),
            ("refresh", "1Hz cache — unchanged"),
        ],
        &[
            TokenChip::new("bg-sidebar", "bar", egui::Color32::from(theme.bg_sidebar())),
            TokenChip::new(
                "separator",
                "border-top",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "statusbar-glyph",
                "branch glyph",
                egui::Color32::from(theme.statusbar_glyph()),
            ),
            TokenChip::new(
                "text-muted",
                "cell text",
                egui::Color32::from(theme.text_muted()),
            ),
            TokenChip::new(
                "statusbar-theme-glyph",
                "theme glyph",
                egui::Color32::from(theme.statusbar_theme_glyph()),
            ),
            TokenChip::without_color("icon-size-xs", "the semantic it aliases"),
            TokenChip::without_color("statusbar-glyph-size", "every inline glyph in the bar"),
            TokenChip::without_color("statusbar-dot-size", "6 — when a surface dot is shown"),
            TokenChip::without_color("status-bar-height", "24px bar"),
        ],
    );

    note(
        ui,
        theme,
        "The theme indicator used to borrow accent-warning for light and accent-agent for dark. \
         Light-vs-dark is not a warning and not an agent, so it becomes a glyph in dim ink — no \
         colour role is minted for it.",
    );
    dont(
        ui,
        theme,
        "Don't add counts, clocks, CPU meters or notification badges. Anything that changes \
         faster than the bar's 1Hz read, or that is not a property of the focused surface, \
         belongs elsewhere.",
    );
}
