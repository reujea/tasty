//! Layouts › Sidebar & rail › Workspace row — running count. 시안
//! `layouts/running-count.jsx`의 프레임을 옮긴다. 행 구조는 본체 `draw_workspace_card`와 같고
//! 칸·숫자·잉크는 본체와 같은 `tasty_ui_widgets` 함수로 그린다. 0은 숫자를 그리지 않는다.

use tasty_settings::AppearanceSettings;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    RowParent, RowState, TagVariant, Tooltip, TooltipPlacement, attach_refusal_mark,
    move_source_glyph_size, paint_move_source_glyph, paint_num_keycap, running_count_column,
    tag_caps, workspace_attention_badges, workspace_row_body_offset,
};

use super::sidebar::{self, WsRowSpec, with_zoom};
use crate::catalog::icons::{CHEVRON_DOWN, REMOTE};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::i18n::t_count;

/// 긴 이름은 시안의 최악의 행과 같다.
const LONG_NAME: &str = "very-long-workspace-name-for-the-narrow-sidebar";
const SCALES: [&str; 3] = ["small", "medium", "large"];
const ROW_STATES: [(RowState, &str); 6] = [
    (RowState::Rest, "rest"),
    (RowState::Hover, "hover"),
    (RowState::Press, "press"),
    (RowState::Selected, "selected"),
    (RowState::SelectedHover, "selected+hover"),
    (RowState::SelectedPress, "selected+press"),
];

/// 시안 `CountRow`의 표시 내용. 값이 곧 점 색이다.
#[derive(Clone, Copy)]
struct CountRow {
    name: &'static str,
    count: usize,
    state: RowState,
    attached: bool,
    loading: bool,
    keycap: Option<&'static str>,
    needs_input: usize,
    completion: usize,
    refused: bool,
    move_source: bool,
    mirror: bool,
    subtitle: Option<&'static str>,
    description: Option<&'static str>,
}

const fn row(name: &'static str, count: usize) -> CountRow {
    CountRow {
        name,
        count,
        state: RowState::Rest,
        attached: false,
        loading: false,
        keycap: None,
        needs_input: 0,
        completion: 0,
        refused: false,
        move_source: false,
        mirror: false,
        subtitle: None,
        description: None,
    }
}

const fn selected(name: &'static str, count: usize) -> CountRow {
    CountRow {
        state: RowState::Selected,
        ..row(name, count)
    }
}

const fn marks(name: &'static str, count: usize, ni: usize, done: usize) -> CountRow {
    CountRow {
        needs_input: ni,
        completion: done,
        ..row(name, count)
    }
}

/// 숫자, 이름 끝 칸까지 모두 있는 행. 줄어드는 것은 이름뿐이다.
const fn narrowest(count: usize, state: RowState) -> CountRow {
    CountRow {
        state,
        needs_input: 100,
        completion: 100,
        refused: true,
        move_source: true,
        ..row(LONG_NAME, count)
    }
}

fn paint_title_line(ui: &mut egui::Ui, th: &Theme, r: &CountRow) {
    let title_h =
        ui.fonts(|f| f.row_height(&egui::FontId::proportional(th.font_size_body.value())));
    ui.spacing_mut().item_spacing.x = th.workspace_running_count_gap().value();
    let slot = egui::vec2(th.workspace_dot_slot().value(), title_h);
    let (slot, _) = ui.allocate_exact_size(slot, egui::Sense::hover());
    let dot_row = WsRowSpec {
        name: r.name,
        badge: None,
        state: r.state,
        running: r.count,
        attached: r.attached,
        mirror: false,
        subtitle: None,
        description: None,
        loading: r.loading,
        keycap: None,
        boxes: false,
    };
    let painter = ui.painter_at(ui.clip_rect());
    sidebar::paint_ws_dot(
        ui,
        th,
        &painter,
        &dot_row,
        slot.center(),
        th.status_dot_size.value() * 0.5,
    );
    if let Some(digit) = r.keycap {
        paint_num_keycap(
            &painter,
            th,
            slot.center(),
            digit,
            r.state.is_selected(),
            1.0,
        );
    }
    ui.spacing_mut().item_spacing.x = th.workspace_dot_gap().value();
    let ink = r.state.meta(th, RowParent::Mantle);
    running_count_column(ui, th, r.count, title_h, ink);
    let name_color = r.state.label(th, RowParent::Mantle, th.text_secondary());
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let item_gap = (th.spacing_sm.value() - ui.spacing().item_spacing.x).max(0.0);
        let mut placed = workspace_attention_badges(ui, th, r.needs_input, r.completion);
        if r.refused {
            if placed {
                ui.add_space(item_gap);
            }
            attach_refusal_mark(ui, th, "");
            placed = true;
        }
        if r.move_source {
            if placed {
                ui.add_space(item_gap);
            }
            let size = move_source_glyph_size(th);
            let (glyph, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            paint_move_source_glyph(ui, th, glyph);
        }
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(r.name)
                        .size(th.font_size_body.value())
                        .color(name_color),
                )
                .truncate(),
            );
        });
    });
}

fn paint_body_lines(ui: &mut egui::Ui, th: &Theme, r: &CountRow, body_indent: f32) {
    let ink = r.state.meta(th, RowParent::Mantle);
    if r.mirror {
        ui.horizontal(|ui| {
            ui.add_space(body_indent);
            ui.spacing_mut().item_spacing.x = th.workspace_mirror_gap().value();
            let icon = th.workspace_mirror_icon_size().value();
            ui.add(REMOTE.image(icon, th.workspace_mirror_fg().into()));
            tag_caps(ui, th, "remote", TagVariant::Info, false);
        });
    }
    if let Some(subtitle) = r.subtitle {
        ui.horizontal(|ui| {
            ui.add_space(body_indent);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(subtitle)
                        .size(th.sidebar_button_label_font_size.value())
                        .color(ink),
                )
                .truncate(),
            );
        });
    }
    if let Some(description) = r.description {
        ui.horizontal(|ui| {
            ui.add_space(body_indent);
            let size = th.font_size_caption.value();
            let mut job = egui::text::LayoutJob {
                wrap: egui::text::TextWrapping {
                    max_width: ui.available_width(),
                    max_rows: 2,
                    ..Default::default()
                },
                ..Default::default()
            };
            job.append(
                description,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(size),
                    color: if r.state.is_selected() {
                        th.text_on_active().into()
                    } else {
                        th.text_placeholder().into()
                    },
                    line_height: Some(size * th.line_height_ui),
                    ..Default::default()
                },
            );
            let galley = ui.fonts(|f| f.layout_job(job));
            ui.label(galley);
        });
    }
}

fn count_row(ui: &mut egui::Ui, th: &Theme, width: f32, r: &CountRow) {
    let left = th.workspace_row_padding_x().value();
    let right = th.spacing_sm.value();
    let body_indent = workspace_row_body_offset(ui, th, r.count);
    let response = egui::Frame::new()
        .fill(r.state.fill(th))
        .inner_margin(egui::Margin {
            left: left as i8,
            right: right as i8,
            top: th.spacing_xs.value() as i8,
            bottom: th.spacing_xs.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_width(width - left - right);
            ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
            ui.horizontal(|ui| paint_title_line(ui, th, r));
            paint_body_lines(ui, th, r, body_indent);
        });
    if r.state.is_selected() {
        let rect = response.response.rect;
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(th.workspace_row_active_bar_width().value(), rect.height()),
        );
        ui.painter().rect_filled(bar, 0.0, th.accent_primary());
    }
}

/// 사이드바 열. 바탕 위에 행을 쌓는다.
fn column(ui: &mut egui::Ui, th: &Theme, width: f32, body: impl FnOnce(&mut egui::Ui, f32)) {
    egui::Frame::new()
        .fill(th.bg_sidebar().into())
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                body(ui, width);
            });
        });
}

/// 세 `ui_scale`의 앱 폭으로 같은 내용을 그린다. 토큰과 폭이 배율을 함께 탄다.
fn three_widths(ui: &mut egui::Ui, theme: &Theme, body: impl Fn(&mut egui::Ui, &Theme, f32)) {
    for key in SCALES {
        let factor = AppearanceSettings::ui_scale_factor_for(key);
        let th = with_zoom(theme, theme.ui_zoom * factor);
        let width = AppearanceSettings {
            ui_scale: key.to_string(),
            ..Default::default()
        }
        .scaled_sidebar_width()
        .value();
        let caption = format!("{key} · {width}px · ui_scale {factor}");
        spec::cluster(ui, theme, &caption, |ui| {
            column(ui, &th, width, |ui, width| body(ui, &th, width));
        });
    }
}

/// 설명을 단 행 목록. 폭은 medium 사이드바다.
fn labelled_rows(ui: &mut egui::Ui, theme: &Theme, cases: &[(&str, CountRow)]) {
    let width = AppearanceSettings::default().scaled_sidebar_width().value();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        for (label, r) in cases {
            ui.label(
                egui::RichText::new(*label)
                    .monospace()
                    .size(theme.font_size_micro.value())
                    .color(theme.text_muted()),
            );
            column(ui, theme, width, |ui, width| count_row(ui, theme, width, r));
        }
    });
}

fn frame(ui: &mut egui::Ui, theme: &Theme, name: &str, body: impl FnOnce(&mut egui::Ui)) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, name, body);
    });
}

fn draw_values(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-values · 0 to 99 keep the name in place",
        |ui| {
            labelled_rows(
                ui,
                theme,
                &[
                    ("0 — no numeral, idle dot", row("scratch", 0)),
                    ("1", row("docs-site", 1)),
                    ("9", row("data-etl", 9)),
                    ("12 — two digits fill the column", row("monorepo", 12)),
                    ("99", row("farm", 99)),
                    ("128 → 99+, this row's column grows", row("farm-large", 128)),
                ],
            );
        },
    );
}

fn draw_row_states(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-row-states · at 3 and at 0",
        |ui| {
            for count in [3, 0] {
                let name = if count == 0 { "scratch" } else { "docs-site" };
                let cases: Vec<(String, CountRow)> = ROW_STATES
                    .iter()
                    .map(|(state, label)| {
                        let r = CountRow {
                            state: *state,
                            ..row(name, count)
                        };
                        (format!("{label} · {count}"), r)
                    })
                    .collect();
                let cases: Vec<(&str, CountRow)> = cases
                    .iter()
                    .map(|(label, r)| (label.as_str(), *r))
                    .collect();
                labelled_rows(ui, theme, &cases);
            }
        },
    );
}

fn draw_attention(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-attention · the left number is activity",
        |ui| {
            labelled_rows(
                ui,
                theme,
                &[
                    ("no attention", row("docs-site", 2)),
                    ("NeedsInput only", marks("data-etl", 1, 1, 0)),
                    ("Completion only", marks("docs-site", 0, 0, 3)),
                    ("both", marks("tasty-core", 2, 2, 5)),
                    (
                        "one surface counted twice — permission dialog",
                        marks("agent", 1, 1, 0),
                    ),
                    ("overflow on all three", marks("monorepo", 128, 100, 100)),
                ],
            );
        },
    );
}

fn draw_narrowest(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-narrowest · the name gives way, the number stays",
        |ui| {
            three_widths(ui, theme, |ui, th, width| {
                for r in [
                    narrowest(3, RowState::Rest),
                    narrowest(12, RowState::Selected),
                    row(LONG_NAME, 0),
                ] {
                    count_row(ui, th, width, &r);
                }
            });
        },
    );
}

fn band(ui: &mut egui::Ui, th: &Theme, width: f32, label: &str, count: usize) {
    let height = sidebar::category_band_height(th);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    sidebar::category_band(ui, th, rect, CHEVRON_DOWN, label, count);
}

fn draw_categories(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-categories · band count = workspaces, row count = running surfaces",
        |ui| {
            three_widths(ui, theme, |ui, th, width| {
                band(ui, th, width, "Workspaces", 2);
                count_row(ui, th, width, &selected("agents-prod", 2));
                count_row(ui, th, width, &row("scratch", 0));
                band(ui, th, width, "Services", 3);
                let infra = CountRow {
                    subtitle: Some("terraform + k8s"),
                    ..row("infra", 0)
                };
                count_row(ui, th, width, &infra);
                count_row(ui, th, width, &marks("api-gateway", 1, 1, 0));
                count_row(ui, th, width, &marks("data-pipeline", 4, 0, 1));
            });
        },
    );
}

/// 0과 1을 위아래로 둔다. 값이 바뀌어도 이름 x와 잘린 글자 수가 그대로다.
fn draw_zero_column(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-zero-column · the empty column keeps the name in place",
        |ui| {
            three_widths(ui, theme, |ui, th, width| {
                for count in [0, 1] {
                    count_row(ui, th, width, &row("docs-site", count));
                    count_row(ui, th, width, &row(LONG_NAME, count));
                    count_row(ui, th, width, &selected("tasty-core", count));
                    count_row(ui, th, width, &marks("data-etl", count, 2, 5));
                }
            });
        },
    );
}

fn draw_marks(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-marks · the column does not move",
        |ui| {
            labelled_rows(
                ui,
                theme,
                &[
                    (
                        "switch keycap over the dot",
                        CountRow {
                            keycap: Some("2"),
                            ..row("docs-site", 3)
                        },
                    ),
                    (
                        "switch keycap · value 0",
                        CountRow {
                            keycap: Some("3"),
                            ..row("scratch", 0)
                        },
                    ),
                    (
                        "switch keycap · selected · value 0",
                        CountRow {
                            keycap: Some("4"),
                            ..selected("scratch", 0)
                        },
                    ),
                    (
                        "switch keycap · selected",
                        CountRow {
                            keycap: Some("1"),
                            ..selected("tasty-core", 12)
                        },
                    ),
                    (
                        "attached ring",
                        CountRow {
                            attached: true,
                            ..row("api-gateway", 1)
                        },
                    ),
                    (
                        "attached ring · value 0",
                        CountRow {
                            attached: true,
                            ..row("api-gateway", 0)
                        },
                    ),
                    (
                        "attach wait spinner · value 0",
                        CountRow {
                            loading: true,
                            ..row("build-farm", 0)
                        },
                    ),
                    (
                        "attach wait spinner · value 2",
                        CountRow {
                            loading: true,
                            ..row("build-farm", 2)
                        },
                    ),
                    (
                        "mirror · subtitle · description · value 0",
                        CountRow {
                            mirror: true,
                            subtitle: Some("mirror → gb10"),
                            description: Some("nightly ETL across the warehouse, last run 18m ago"),
                            ..row("data-pipeline", 0)
                        },
                    ),
                    (
                        "mirror · subtitle · description",
                        CountRow {
                            mirror: true,
                            subtitle: Some("mirror → gb10"),
                            description: Some("nightly ETL across the warehouse, last run 18m ago"),
                            ..row("data-pipeline", 3)
                        },
                    ),
                    (
                        "99+ moves every body line together",
                        CountRow {
                            mirror: true,
                            subtitle: Some("mirror → gb10"),
                            description: Some("nightly ETL across the warehouse, last run 18m ago"),
                            ..row("data-pipeline", 128)
                        },
                    ),
                ],
            );
        },
    );
}

/// 숫자 칸 위의 툴팁. 값은 표기와 달리 정확한 수다.
fn draw_tooltip(ui: &mut egui::Ui, theme: &Theme) {
    frame(
        ui,
        theme,
        "running-count-tooltip · over the number only, exact value",
        |ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xl.value() * 3.0;
            let room = theme.spacing_xl.value() * 2.0;
            for count in [3_usize, 128] {
                let text = t_count(
                    "sidebar.running_surfaces_tooltip",
                    count as u64,
                    &[&count.to_string()],
                );
                let (anchor, _) = ui.allocate_exact_size(
                    egui::vec2(theme.spacing_xl.value() * 4.0, room),
                    egui::Sense::hover(),
                );
                let line = egui::Rect::from_min_size(
                    egui::pos2(
                        anchor.center().x,
                        anchor.bottom() - theme.spacing_lg.value(),
                    ),
                    egui::vec2(
                        theme.workspace_running_count_min_width().value(),
                        theme.spacing_lg.value(),
                    ),
                );
                tasty_ui_widgets::paint_running_count(
                    ui.painter(),
                    theme,
                    line,
                    count,
                    theme.text_muted(),
                );
                // 버블은 스크롤과 무관한 전경 Area라, 예제가 화면 밖일 때 그리면 화면 가장자리로 끌려온다.
                if ui.clip_rect().contains_rect(anchor) {
                    Tooltip::new(&text)
                        .id_source(("running_count_tooltip_specimen", count))
                        .placement(TooltipPlacement::Top)
                        .show(ui, theme, line);
                }
            }
        },
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        three_widths(ui, theme, |ui, th, width| {
            for r in [
                selected("tasty-core", 2),
                marks("docs-site", 3, 0, 3),
                CountRow {
                    mirror: true,
                    ..marks("data-etl", 1, 1, 0)
                },
                marks("monorepo", 12, 2, 5),
                row("scratch", 0),
            ] {
                count_row(ui, th, width, &r);
            }
        });
    });
    draw_values(ui, theme);
    draw_row_states(ui, theme);
    draw_attention(ui, theme);
    draw_narrowest(ui, theme);
    draw_zero_column(ui, theme);
    draw_categories(ui, theme);
    draw_marks(ui, theme);
    draw_tooltip(ui, theme);
    spec::meta(
        ui,
        theme,
        &[
            (
                "position",
                "between the dot slot and the name, on expanded rows",
            ),
            ("value 0", "no numeral, no tooltip, the column stays empty"),
            ("value 1 or more", "numeral and tooltip"),
            ("slot → numeral", "workspace-running-count-gap"),
            (
                "column",
                "max(workspace-running-count-min-width, width of 00), numeral left-aligned",
            ),
            ("numeral → name", "workspace-dot-gap"),
            ("type", "workspace-running-count-font-size, mono"),
            ("overflow", "above workspace-running-count-max: 99+"),
            ("motion", "none, the value swaps"),
        ],
        &[
            TokenChip::new("text-muted", "rest", theme.text_muted().into()),
            TokenChip::new(
                "text-muted-raised",
                "hover, press",
                theme.text_muted_raised().into(),
            ),
            TokenChip::new("text-on-active", "selected", theme.text_on_active().into()),
            TokenChip::new(
                "text-on-active-hover",
                "selected+hover, selected+press",
                theme.text_on_active_hover().into(),
            ),
        ],
    );
    spec::do_(
        ui,
        theme,
        "Show the numeral from 1 and keep its column empty at 0 so the name stays in place when the value changes between 0 and 1.",
    );
    spec::dont(
        ui,
        theme,
        "Don't use a filled pill, an accent colour or the trailing group for the count; these treatments make it read as attention.",
    );
    spec::note(
        ui,
        theme,
        "Use one muted text tone in both themes because Latte's muted and secondary inks are identical, and the dot already distinguishes idle from running. Swap numerals without motion because updates about every two seconds would keep drawing attention. Reserving the empty column prevents name movement and changes in visible characters for truncated names. The idle dot preserves the meaning at 0. The attention pills' rule concerns a trailing group that collapses inline; the numeral column instead stabilizes the name within the leading indent beside the reserved dot slot.",
    );
}
