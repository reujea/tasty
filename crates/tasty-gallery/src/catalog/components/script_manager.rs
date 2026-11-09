//! 스크립트 관리의 등록 목록·변경 안내·자동 실행 트리거·빈 상태를 보여주는 정적 예제.
//! 등록·이름 변경·삭제·단축키 연결·트리거 편집은 실제로 실행하지 않는다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{
    Button, ButtonVariant, CenterState, IconButton, IconButtonVariant, kbd,
    script_trigger_add_control, script_trigger_menu, script_trigger_menu_frame,
};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// 갤러리 프레임 최대 폭 (jsx `maxWidth: 560`). 본체는 settings content 폭을 상속하나
/// 갤러리 미러는 카드로 감싸 560 으로 bound.
const FRAME_MAX_W: LogicalPx = LogicalPx(560.0);
/// 행 중앙 컬럼의 name→path→help 사이 hairline 간격 (jsx `gap: 2` — 4px 그리드 하위).
const ROW_LINE_GAP: LogicalPx = LogicalPx(2.0);
/// 트리거 칩과 Add trigger… 컨트롤 높이(jsx `height: 16`, changed 배지와 같은 높이).
const CHIP_H: LogicalPx = LogicalPx(16.0);
/// 캡션과 프레임 사이 간격(jsx 무대 열 `gap: 6`).
const CAPTION_GAP: LogicalPx = LogicalPx(6.0);
/// 프레임 안쪽 여백(jsx `ScriptManagerFrame` `padding: 18`, 간격 토큰 밖 값).
const FRAME_PAD: LogicalPx = LogicalPx(18.0);
/// 머리줄과 목록 사이 간격(jsx `ScriptManagerFrame` `gap: 14`, 간격 토큰 밖 값).
const FRAME_GAP: LogicalPx = LogicalPx(14.0);

/// RTL 클러스터에서 kbd 키캡이 역순으로 그려지는 것을 상쇄하려 combo 파트를 미리
/// 뒤집는다(`"Ctrl+Shift+J"` → `"J+Shift+Ctrl"` → RTL 렌더 후 화면상 정순).
fn rtl_combo(combo: &str) -> String {
    combo.split('+').rev().collect::<Vec<_>>().join("+")
}

/// 한 스크립트 행(seed). `dir`+`file` 은 중간생략 경로용, `shortcut` 빈값=Unbound.
/// `triggers` 는 자동 실행에 묶인 host 수명주기 이벤트다.
struct Seed {
    name: &'static str,
    dir: &'static str,
    file: &'static str,
    shortcut: &'static str,
    changed: bool,
    triggers: &'static [&'static str],
}

const SEEDS: &[Seed] = &[
    Seed {
        name: "Reformat JSON",
        dir: "~/.tasty/scripts/",
        file: "reformat-json.lua",
        shortcut: "Ctrl+Shift+J",
        changed: false,
        triggers: &["clipboard.copy.post"],
    },
    Seed {
        name: "Tail & highlight errors",
        dir: "~/.tasty/scripts/",
        file: "tail-errors.lua",
        shortcut: "",
        changed: false,
        triggers: &["pane.create.post", "session.start.post"],
    },
    Seed {
        name: "Deploy staging",
        dir: "~/work/ops/tasty/",
        file: "deploy-staging.lua",
        shortcut: "Ctrl+Alt+D",
        changed: true,
        triggers: &[],
    },
];

/// 시안 무대: 등록 목록(bound / unbound / changed+help, 자동 실행 트리거)과 빈 상태를 나란히 둔다.
pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // 시안 무대는 `alignItems: flex-start` 라 높이가 다른 두 프레임도 위쪽을 맞춘다.
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                for (caption, empty) in [("registered scripts", false), ("empty state", true)] {
                    spec::wrap_item(ui, |ui| {
                        ui.push_id(caption, |ui| {
                            ui.spacing_mut().item_spacing.y = CAPTION_GAP.value();
                            ui.label(
                                egui::RichText::new(caption)
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                            frame(ui, theme, empty);
                        });
                    });
                }
            },
        );
    });
    add_trigger_states(ui, theme);
    meta_note(ui, theme);
}

/// 시안 `LIFECYCLE_EVENTS` 중 앞쪽 여섯 이벤트. 메뉴를 연 모습의 표본이다.
const MENU_SAMPLE: &[&str] = &[
    "app.ready.post",
    "window.create.post",
    "window.close.pre",
    "tab.create.post",
    "tab.close.pre",
    "pane.create.post",
];

/// Add trigger… 의 세 상태 — 평소(점선) · 메뉴를 연 모습 · 모든 이벤트가 걸려 disabled.
fn add_trigger_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::cluster(
        ui,
        theme,
        "Add trigger… — rest · menu open · all events bound (disabled)",
        |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_xl.value();
                for (label, enabled, open) in [
                    ("rest", true, false),
                    ("open", true, true),
                    ("all bound", false, false),
                ] {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                        ui.label(
                            egui::RichText::new(label)
                                .size(theme.font_size_caption.value())
                                .color(theme.text_muted().to_egui()),
                        );
                        script_trigger_add_control(
                            ui,
                            theme,
                            "Add trigger…",
                            CHIP_H.value(),
                            enabled,
                            open,
                        );
                        if open {
                            script_trigger_menu_frame(ui, theme, |ui| {
                                script_trigger_menu(ui, theme, MENU_SAMPLE)
                            });
                        }
                    });
                }
            });
        },
    );
}

fn frame(ui: &mut egui::Ui, theme: &Theme, empty: bool) {
    // 시안 프레임은 bg-panel · border-strong · radius · shadow-modal 이다.
    kit::frame_card(ui, theme, FRAME_MAX_W, kit::panel_fill(theme), |ui| {
        kit::region_sym(ui, FRAME_PAD, FRAME_PAD, |ui| {
            ui.spacing_mut().item_spacing.y = FRAME_GAP.value();
            header(ui, theme);
            if empty {
                empty_state(ui, theme);
            } else {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for s in SEEDS {
                        script_row(ui, theme, s);
                    }
                });
            }
        });
    });
}

fn header(ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal_top(|ui| {
        let right_w = 96.0; // "Add script" 버튼 대략 폭 예약 (secondary sm + plus).
        let left_w = (ui.available_width() - right_w - theme.spacing_md.value()).max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(left_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = ROW_LINE_GAP.value();
                ui.label(
                    egui::RichText::new("Scripts")
                        .size(theme.font_size_max.value())
                        .strong()
                        .color(theme.text_primary().to_egui()),
                );
                ui.set_max_width(theme.measure_md.value().min(left_w));
                ui.label(
                    egui::RichText::new(
                        "Register and manage Lua scripts you can run with a shortcut. \
                         Binding a trigger is done in Keybindings; each script is verified \
                         against the SHA recorded when it was added.",
                    )
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
                );
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            Button::new("Add script")
                .variant(ButtonVariant::Secondary)
                .size(tasty_ui_widgets::ControlSize::Sm)
                .leading_icon(&|ui, rect, c| icons::PLUS.image(rect.width(), c).paint_at(ui, rect))
                .show(ui, theme);
        });
    });
}

fn script_row(ui: &mut egui::Ui, theme: &Theme, s: &Seed) {
    // 시안 `ScriptRow` 안쪽 여백 8 4(space-sm · space-xs).
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(
            theme.spacing_xs.value() as i8,
            theme.spacing_sm.value() as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                ui.vertical(|ui| {
                    ui.add_space(ROW_LINE_GAP.value());
                    kit::icon(
                        ui,
                        icons::SCRIPT,
                        theme.icon_glyph_size_md,
                        theme.text_muted().to_egui(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(tasty_ui_widgets::ControlSize::Sm)
                        .show(ui, theme, &|ui, rect, c| {
                            icons::TRASH.image(rect.width(), c).paint_at(ui, rect)
                        });
                    IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(tasty_ui_widgets::ControlSize::Sm)
                        .show(ui, theme, &|ui, rect, c| {
                            icons::EDIT.image(rect.width(), c).paint_at(ui, rect)
                        });
                    IconButton::new()
                        .variant(IconButtonVariant::Ghost)
                        .size(tasty_ui_widgets::ControlSize::Sm)
                        .show(ui, theme, &|ui, rect, c| {
                            icons::KEYBOARD.image(rect.width(), c).paint_at(ui, rect)
                        });
                    if s.shortcut.is_empty() {
                        ui.label(
                            egui::RichText::new("Unbound")
                                .size(theme.font_size_term_sm.value())
                                .italics()
                                .color(theme.text_muted().to_egui()),
                        );
                    } else {
                        // RTL 배치에서도 보조 키 순서가 유지되도록 조각을 역순으로 전달한다.
                        kbd(
                            ui,
                            theme,
                            &rtl_combo(s.shortcut),
                            tasty_ui_widgets::Ground::Panel,
                        );
                    }
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = ROW_LINE_GAP.value();
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                            ui.label(
                                egui::RichText::new(s.name)
                                    .size(theme.font_size_body.value())
                                    .strong()
                                    .color(theme.text_primary().to_egui()),
                            );
                            if s.changed {
                                changed_badge(ui, theme);
                            }
                        });
                        script_path(ui, theme, s.dir, s.file);
                        if s.changed {
                            ui.label(
                                egui::RichText::new(crate::i18n::t(
                                    "settings.scripts.changed_help",
                                ))
                                .size(theme.font_size_caption.value())
                                .color(theme.accent_warning_raised().to_egui()),
                            );
                        }
                        trigger_row(ui, theme, s.triggers);
                    });
                });
            });
        });
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w, theme.border_width.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// 경로 — dir(text-muted) + file(text-secondary), mono `font-size-caption`(11).
fn script_path(ui: &mut egui::Ui, theme: &Theme, dir: &str, file: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(
            egui::RichText::new(dir)
                .size(theme.font_size_caption.value())
                .monospace()
                .color(theme.text_muted().to_egui()),
        );
        ui.label(
            egui::RichText::new(file)
                .size(theme.font_size_caption.value())
                .monospace()
                .color(theme.text_secondary().to_egui()),
        );
    });
}

/// changed 배지 — warn 글리프 12 + "changed", font-label micro(10), accent-warning
/// color-mix(40% border / 12% bg).
fn changed_badge(ui: &mut egui::Ui, theme: &Theme) {
    let warn = theme.accent_warning().to_egui();
    let micro = theme.font_size_micro.value();
    let glyph = theme.icon_glyph_size_xs.value(); // 12
    let galley = ui.painter().layout_no_wrap(
        "changed".to_owned(),
        tasty_ui_widgets::label_font(micro),
        egui::Color32::PLACEHOLDER,
    );
    let gap = theme.spacing_xs.value(); // 4 — 글리프↔라벨
    let pad_x = theme.spacing_sm.value(); // 8 좌우 패딩(디자인 padding 0 space-sm)
    let h = 16.0; // jsx height 16 (배지 고정)
    let w = pad_x * 2.0 + glyph + gap + galley.rect.width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let radius = theme.corner_radius_sm.value();
    // 채움은 `tint-fill-alpha`. 테두리 계수는 디자인이 "채움만" 으로 한정한
    // 부분 사용이라(docs/design/systems/theme.md#ui-코드의-색상-접근) 이 자리 고유 값으로 남는다.
    const BADGE_STROKE_OPACITY: f32 = 0.4;
    ui.painter()
        .rect_filled(rect, radius, warn.gamma_multiply(theme.tint_fill_alpha()));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            theme.border_width.value(),
            warn.gamma_multiply(BADGE_STROKE_OPACITY),
        ),
        egui::StrokeKind::Inside,
    );
    let gy = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    icons::ALERT_TRIANGLE.image(glyph, warn).paint_at(ui, gy);
    let pos = egui::pos2(
        rect.left() + pad_x + glyph + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, warn);
}

/// 자동 실행 줄 — "Auto-run:" 캡션 · 트리거 칩(누르면 제거) · 점선 Add trigger… 컨트롤.
/// 칩이 넘치면 다음 줄로 넘어간다.
fn trigger_row(ui: &mut egui::Ui, theme: &Theme, triggers: &[&str]) {
    // 시안 `marginTop: 2`.
    ui.add_space(ROW_LINE_GAP.value());
    ui.horizontal_wrapped(|ui| {
        let gap = theme.spacing_xs.value();
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        ui.label(
            egui::RichText::new("Auto-run:")
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
        for event in triggers {
            trigger_chip(ui, theme, event);
        }
        script_trigger_add_control(ui, theme, "Add trigger…", CHIP_H.value(), true, false);
    });
}

/// 트리거 칩 — font-label micro 이벤트명 + 오른쪽 close 글리프 12, 높이 16, 안쪽 여백 0 4,
/// border-default 실선. 칩 전체가 제거 영역이다.
fn trigger_chip(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    let fg = theme.text_secondary().to_egui();
    let glyph = theme.icon_glyph_size_xs.value();
    let gap = theme.spacing_xs.value();
    let pad_x = theme.spacing_xs.value();
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        tasty_ui_widgets::label_font(theme.font_size_micro.value()),
        fg,
    );
    let w = pad_x * 2.0 + galley.rect.width() + gap + glyph;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, CHIP_H.value()), egui::Sense::hover());
    let bw = theme.border_width.value();
    let stroke = egui::Stroke::new(bw, theme.border_default().to_egui());
    let radius = theme.corner_radius_sm.value();
    ui.painter()
        .rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    let pos = egui::pos2(
        rect.left() + pad_x,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, fg);
    let gr = egui::Rect::from_min_size(
        egui::pos2(rect.right() - pad_x - glyph, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    icons::CLOSE
        .image(glyph, theme.text_muted().to_egui())
        .paint_at(ui, gr);
}

/// 목록이 들어갈 자리에 공용 CenterState 를 자연 높이로 그린다(본체 Settings 와 같은 호출).
fn empty_state(ui: &mut egui::Ui, theme: &Theme) {
    CenterState::empty(
        icons::SCRIPT,
        crate::i18n::t("settings.scripts.empty_title"),
    )
    .sub_line(Some(crate::i18n::t("settings.scripts.empty_body")))
    .show(ui, theme, None);
}

fn meta_note(ui: &mut egui::Ui, theme: &Theme) {
    spec::meta(
        ui,
        theme,
        &[
            ("home", "Settings › Misc › Scripts (subsection)"),
            ("row", "name · path · shortcut · actions · auto-run"),
            ("shortcut", "Kbd badge or italic Unbound"),
            ("changed", "peach badge + help line (SHA mismatch)"),
            (
                "auto-run",
                "trigger chips + Add trigger… (13 lifecycle events)",
            ),
            ("chip", "mono event · click removes · hover 12% overlay"),
            (
                "add control",
                "dashed 1px border-default · 4 / 4 · disabled when all events are bound",
            ),
            (
                "add menu",
                "min 200 · max 220 then scroll · menu-* · mono micro rows",
            ),
            ("actions", "bind · rename · remove"),
            ("empty", "glyph + Add script prompt"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "changed badge + help",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "border-default",
                "chip + add-control border",
                theme.border_default().to_egui(),
            ),
            TokenChip::new(
                "overlay-active",
                "chip hover / menu open",
                theme.overlay_active().to_egui_premultiplied(),
            ),
            TokenChip::new("text-muted", "Unbound", theme.text_muted().to_egui()),
            TokenChip::without_color("font-mono", "path · trigger chips"),
            TokenChip::without_color("border-dash", "→ size-4"),
            TokenChip::without_color("border-dash-gap", "→ size-4"),
            TokenChip::without_color("trigger-menu-min-width", "→ field-width-lg 200"),
            TokenChip::without_color("trigger-menu-max-height", "→ autocomplete-max-height 220"),
        ],
    );
    spec::note(
        ui,
        theme,
        "Two independent run paths: a manual shortcut (bound in the Keybindings tab, shown as \
         the Kbd badge) and auto-run triggers (edited inline here — the script fires when the \
         host emits a bound lifecycle event). A script can have either, both, or neither. The \
         changed state is informational — the script still runs, but re-confirms once (TOFU) \
         because the on-disk file drifted from the registered hash.",
    );
    spec::note(
        ui,
        theme,
        "Add trigger… is the shared dashed control: border-default at border-dash 4 / \
         border-dash-gap 4, dashes on straight edges only, corners solid. Its menu lists the \
         events not yet bound (min width trigger-menu-min-width, scrolls past \
         trigger-menu-max-height, menu-* container). When every event is bound the control \
         stays in place, disabled.",
    );
}
