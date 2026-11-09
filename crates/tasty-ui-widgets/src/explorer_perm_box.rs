//! 탐색기 Properties 의 권한 편집 상자. 본체와 갤러리가 같은 함수를 부른다.
//! 시안 `PermEditor`: Permissions 행 아래 내용 폭(라벨 열 포함) 전체를 쓰는 상자로, Unix 는
//! 소유자·그룹·기타 × 읽기·쓰기·실행 3×3 체크박스와 바뀔 모드, Windows 는 "읽기 전용" 하나다.
//! 열 머리와 역할명은 `explorer-props-perm-cell-width` 칸 안에서 줄바꿈하고 말줄임하지 않는다.
//! 상자는 받은 폭을 넘지 않는다. 좁은 explorer 칸에서는 상자 자신의 폭으로 격자·쌓기·목록 배치를
//! 고른다([`perm_layout`]). 세 배치의 Tab 순서는 같다(소유자 R W X → 그룹 → 기타 → Cancel → Apply).
//! 문구는 호출자가 번역해 넘기고, 상태(적용 중·실패)와 값의 소유도 호출자에게 있다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::toggle::checkbox_wrapping;

/// 상자 문구. 모두 번역된 값이다.
#[derive(Clone, Copy)]
pub struct PermBoxText<'a> {
    /// 읽기 · 쓰기 · 실행 열 머리.
    pub heads: [&'a str; 3],
    /// 소유자 · 그룹 · 기타 역할명.
    pub roles: [&'a str; 3],
    pub read_only: &'a str,
    pub windows_hint: &'a str,
    pub failed: &'a str,
    pub apply: &'a str,
    pub applying: &'a str,
    pub cancel: &'a str,
}

/// 편집 중인 값. Unix 는 rwx 9비트, Windows 는 읽기 전용 특성이다.
pub enum PermBoxValue<'a> {
    Mode(&'a mut u32),
    ReadOnly(&'a mut bool),
}

pub struct PermBoxProps<'a> {
    pub text: PermBoxText<'a>,
    pub value: PermBoxValue<'a>,
    /// Unix 상자 아래 안내(폴더만 바꿈 · 특수 비트 유지). 없으면 줄을 두지 않는다.
    pub note: Option<&'a str>,
    /// 적용 실패 이유(OS 오류). 있으면 실패 줄을 보인다.
    pub failure: Option<&'a str>,
    /// 적용 중. 모든 컨트롤을 잠그고 버튼 옆에 Spinner 와 진행 문구를 둔다.
    pub busy: bool,
    /// 값이 읽은 값과 다르다. 아니면 Apply 를 끈다.
    pub changed: bool,
}

/// 상자 안에서 누른 버튼.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermBoxClick {
    None,
    Cancel,
    Apply,
}

/// Unix 상자의 배치. 상자 바깥 폭(테두리 포함)으로 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermLayout {
    /// 역할 열 + 읽기·쓰기·실행 열.
    Grid,
    /// 역할 열 없이 열 머리 아래 칸 3개. 역할명은 자기 칸 줄 위의 한 줄이다.
    Stack,
    /// 열 머리 없이 역할명 아래 라벨 붙은 체크박스 세 줄. 폭 하한이 없다.
    List,
}

/// 상자 폭 `box_width` 에 맞는 배치. 경계는 `explorer-props-perm-grid-min-width`·`-stack-min-width` 다.
/// 칸(explorer surface) 폭이 아니라 상자 폭과 비교한다.
pub fn perm_layout(theme: &Theme, box_width: LogicalPx) -> PermLayout {
    if box_width >= theme.explorer_props_perm_grid_min_width() {
        PermLayout::Grid
    } else if box_width >= theme.explorer_props_perm_stack_min_width() {
        PermLayout::Stack
    } else {
        PermLayout::List
    }
}

/// 상자 체크박스들의 기준 id. 같은 부모에 상자를 여럿 그려도 겹치지 않는다.
fn perm_box_id(ui: &egui::Ui) -> egui::Id {
    ui.next_auto_id().with("perm_box")
}

/// `rw-r--r-- · 644` 형태의 읽기 표시.
pub fn perm_readout(bits: u32) -> String {
    let (mode, octal) = readout_parts(bits);
    format!("{mode} {octal}")
}

/// 읽기 표시의 두 덩어리. 좁으면 그 사이에서만 줄바꿈한다.
fn readout_parts(bits: u32) -> (String, String) {
    let mut text = String::with_capacity(9);
    for shift in [6u32, 3, 0] {
        let triad = (bits >> shift) & 0o7;
        text.push(if triad & 4 != 0 { 'r' } else { '-' });
        text.push(if triad & 2 != 0 { 'w' } else { '-' });
        text.push(if triad & 1 != 0 { 'x' } else { '-' });
    }
    (text, format!("· {:03o}", bits & 0o777))
}

/// 상자를 그린다. 호출자가 준 폭(Permissions 행의 내용 폭)을 모두 쓰고 넘지 않는다.
pub fn perm_box(ui: &mut egui::Ui, theme: &Theme, props: PermBoxProps<'_>) -> PermBoxClick {
    let PermBoxProps {
        text,
        value,
        note,
        failure,
        busy,
        changed,
    } = props;
    let gap = theme.spacing_sm.value();
    let mut click = PermBoxClick::None;
    // Frame 크기는 테두리를 포함하므로 지금 남은 폭이 상자의 바깥 폭이다.
    let layout = perm_layout(theme, LogicalPx(ui.available_width()));
    // 배치가 바뀌어도 체크박스 id 가 같아야 포커스가 남는다. 상자 안의 중첩과 무관한 기준 id 를 쓴다.
    // 부모의 다음 자동 id 는 상자마다 다르고, 상자 앞의 위젯이 같으면 프레임이 바뀌어도 같다.
    let ids = perm_box_id(ui);
    ui.add_space(theme.spacing_xs.value());
    egui::Frame::new()
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.explorer_props_perm_border().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(gap as i8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
            match value {
                PermBoxValue::Mode(bits) => {
                    let cells = Cells {
                        theme,
                        text: &text,
                        ids,
                        busy,
                    };
                    match layout {
                        PermLayout::Grid => cells.grid(ui, bits),
                        PermLayout::Stack => cells.stack(ui, bits),
                        PermLayout::List => cells.list(ui, bits),
                    }
                    readout(ui, theme, *bits);
                    if let Some(note) = note {
                        muted_line(ui, theme, note);
                    }
                }
                PermBoxValue::ReadOnly(read_only) => {
                    checkbox_wrapping(
                        ui,
                        theme,
                        ids.with("read_only"),
                        read_only,
                        text.read_only,
                        !busy,
                        crate::Ground::Panel,
                    );
                    muted_line(ui, theme, text.windows_hint);
                }
            }
            if let Some(reason) = failure {
                failure_line(ui, theme, text.failed, reason);
            }
            click = buttons(ui, theme, &text, busy, changed);
        });
    ui.add_space(theme.spacing_xs.value());
    click
}

/// 칸 폭에서 줄바꿈한 caption 글자. `align` 은 칸 안의 가로 정렬이다.
fn cell_text(
    ui: &egui::Ui,
    theme: &Theme,
    text: &str,
    color: egui::Color32,
    width: LogicalPx,
    align: egui::Align,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        egui::FontId::proportional(theme.font_size_caption.value()),
        color,
        width.value(),
    );
    job.halign = align;
    ui.fonts(|f| f.layout_job(job))
}

/// 체크박스 9개를 그리는 데 필요한 것. 세 배치가 같은 순서로 같은 id 의 체크박스를 만든다.
struct Cells<'a> {
    theme: &'a Theme,
    text: &'a PermBoxText<'a>,
    ids: egui::Id,
    busy: bool,
}

impl Cells<'_> {
    /// 역할 열 + 읽기·쓰기·실행 열. 칸은 왼쪽부터 붙여 놓는다.
    fn grid(&self, ui: &mut egui::Ui, bits: &mut u32) {
        let theme = self.theme;
        let cell = theme.explorer_props_perm_cell_width();
        let row_min = theme.explorer_props_row_min_height();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, theme.spacing_xs.value());
            self.heads(ui, cell);
            for (r, (role, shift)) in self.text.roles.iter().zip([6u32, 3, 0]).enumerate() {
                let name = self.role_name(ui, role, cell);
                let h = LogicalPx(name.size().y).max(row_min);
                let (row, _) = ui.allocate_exact_size(
                    egui::vec2((cell * 4.0).value(), h.value()),
                    egui::Sense::hover(),
                );
                let name_y = row.center().y - name.size().y / 2.0;
                ui.painter().galley(
                    egui::pos2(row.left(), name_y),
                    name,
                    egui::Color32::PLACEHOLDER,
                );
                self.cell_row(ui, row, LogicalPx(row.left()) + cell, r, shift, bits);
            }
        });
    }

    /// 역할 열 없이 열 머리 아래 칸 3개. 역할명은 상자 폭에서 줄바꿈해 칸 줄 위에 둔다.
    fn stack(&self, ui: &mut egui::Ui, bits: &mut u32) {
        let theme = self.theme;
        let cell = theme.explorer_props_perm_cell_width();
        let row_min = theme.explorer_props_row_min_height();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, theme.spacing_xs.value());
            self.heads(ui, LogicalPx(0.0));
            for (r, (role, shift)) in self.text.roles.iter().zip([6u32, 3, 0]).enumerate() {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.explorer_props_perm_role_gap().value();
                    self.role_line(ui, role);
                    let (row, _) = ui.allocate_exact_size(
                        egui::vec2((cell * 3.0).value(), row_min.value()),
                        egui::Sense::hover(),
                    );
                    self.cell_row(ui, row, LogicalPx(row.left()), r, shift, bits);
                });
            }
        });
    }

    /// 열 머리 없이 역할명 다음에 라벨 붙은 체크박스를 한 줄씩. 글자는 어디서나 줄바꿈한다.
    fn list(&self, ui: &mut egui::Ui, bits: &mut u32) {
        let theme = self.theme;
        let row_min = theme.explorer_props_row_min_height();
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            for (r, (role, shift)) in self.text.roles.iter().zip([6u32, 3, 0]).enumerate() {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.explorer_props_perm_role_gap().value();
                    self.role_line(ui, role);
                    for (c, bit) in [4u32, 2, 1].into_iter().enumerate() {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_min.value()),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| self.bit(ui, r, c, bit << shift, self.text.heads[c], bits),
                        );
                    }
                });
            }
        });
    }

    /// 읽기 · 쓰기 · 실행 열 머리. `left` 는 첫 칸 앞에 비울 폭(격자의 역할 열)이다.
    /// 가운데 정렬하고, 줄바꿈한 머리는 위로 자라도록 아래를 맞춘다.
    fn heads(&self, ui: &mut egui::Ui, left: LogicalPx) {
        let theme = self.theme;
        let cell = theme.explorer_props_perm_cell_width();
        let heads = self.text.heads.map(|h| {
            cell_text(
                ui,
                theme,
                h,
                theme.text_muted().to_egui(),
                cell,
                egui::Align::Center,
            )
        });
        let head_h = heads.iter().map(|g| g.size().y).fold(0.0, f32::max);
        let (row, _) = ui.allocate_exact_size(
            egui::vec2((left + cell * 3.0).value(), head_h),
            egui::Sense::hover(),
        );
        for (i, galley) in heads.into_iter().enumerate() {
            // 가운데 정렬 galley 는 원점이 칸 가운데다.
            let x = LogicalPx(row.left()) + left + cell * (i as f32 + 0.5);
            let origin = egui::pos2(x.value(), row.bottom() - galley.size().y);
            ui.painter()
                .galley(origin, galley, egui::Color32::PLACEHOLDER);
        }
    }

    fn role_name(
        &self,
        ui: &egui::Ui,
        role: &str,
        width: LogicalPx,
    ) -> std::sync::Arc<egui::Galley> {
        cell_text(
            ui,
            self.theme,
            role,
            self.theme.explorer_props_value_fg().to_egui(),
            width,
            egui::Align::LEFT,
        )
    }

    /// 상자 폭에서 어디서나 줄바꿈한 역할명 한 줄.
    fn role_line(&self, ui: &mut egui::Ui, role: &str) {
        let galley = self.role_name(ui, role, LogicalPx(ui.available_width()));
        let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
        ui.painter()
            .galley(rect.min, galley, egui::Color32::PLACEHOLDER);
    }

    /// `row` 안에 `first` 부터 칸 3개를 두고 각 칸 가운데에 체크박스를 그린다.
    fn cell_row(
        &self,
        ui: &mut egui::Ui,
        row: egui::Rect,
        first: LogicalPx,
        r: usize,
        shift: u32,
        bits: &mut u32,
    ) {
        let cell = self.theme.explorer_props_perm_cell_width();
        let check = self.theme.checkbox_size().value();
        for (c, bit) in [4u32, 2, 1].into_iter().enumerate() {
            let x = first + cell * (c as f32 + 0.5);
            let center = egui::pos2(x.value(), row.center().y);
            let rect = egui::Rect::from_center_size(center, egui::vec2(check, check));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            self.bit(&mut child, r, c, bit << shift, "", bits);
        }
    }

    /// 비트 하나의 체크박스. id 는 역할·열로 정해 배치와 무관하다.
    fn bit(&self, ui: &mut egui::Ui, r: usize, c: usize, mask: u32, label: &str, bits: &mut u32) {
        let mut on = *bits & mask != 0;
        if checkbox_wrapping(
            ui,
            self.theme,
            self.ids.with((r, c)),
            &mut on,
            label,
            !self.busy,
            crate::Ground::Panel,
        )
        .changed()
        {
            *bits = if on { *bits | mask } else { *bits & !mask };
        }
    }
}

/// 모드와 `· 8진수` 사이에서만 줄바꿈하는 읽기 표시. 두 덩어리 사이는 mono 반 글자다.
fn readout(ui: &mut egui::Ui, theme: &Theme, bits: u32) {
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let half = ui.fonts(|f| f.glyph_width(&font, '0')) / 2.0;
    let (mode, octal) = readout_parts(bits);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(half, 0.0);
        for part in [mode, octal] {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(part)
                        .font(font.clone())
                        .color(theme.text_secondary().to_egui()),
                )
                .extend(),
            );
        }
    });
}

fn muted_line(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        )
        .wrap(),
    );
}

/// alertCircle + 실패 제목(explorer-props-perm-error-fg) + OS 이유(mono muted).
fn failure_line(ui: &mut egui::Ui, theme: &Theme, title: &str, reason: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
        let error = theme.explorer_props_perm_error_fg().to_egui();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_xs.value();
            let g = theme.icon_glyph_size_sm.value();
            let (r, _) = ui.allocate_exact_size(egui::vec2(g, g), egui::Sense::hover());
            tasty_icons::ALERT_CIRCLE.image(g, error).paint_at(ui, r);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(title)
                        .size(theme.font_size_caption.value())
                        .color(error),
                )
                .wrap(),
            );
        });
        ui.add(
            egui::Label::new(
                egui::RichText::new(reason)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(theme.text_muted().to_egui()),
            )
            .wrap(),
        );
    });
}

/// 오른쪽 정렬 Cancel(ghost sm) · Apply(primary sm). 한 줄에 들어가지 않으면 Apply 가 다음 줄로 내려간다.
/// Tab 순서가 Cancel → Apply 가 되도록 왼쪽부터 만든다. 적용 중이면 둘 다 잠그고 라벨은 그대로 둔 채 왼쪽에 Spinner 와 진행 문구를 둔다.
fn buttons(
    ui: &mut egui::Ui,
    theme: &Theme,
    text: &PermBoxText<'_>,
    busy: bool,
    changed: bool,
) -> PermBoxClick {
    let mut click = PermBoxClick::None;
    let gap = theme.spacing_sm;
    let spinner_size = theme.icon_glyph_size_sm.value();
    let cancel = Button::new(text.cancel)
        .variant(ButtonVariant::Ghost)
        .size(ControlSize::Sm)
        .enabled(!busy);
    let apply = Button::new(text.apply)
        .variant(ButtonVariant::Primary)
        .size(ControlSize::Sm)
        .enabled(!busy && changed);
    let widths = [
        cancel.desired_width(ui, theme),
        apply.desired_width(ui, theme),
    ];
    let avail = LogicalPx(ui.available_width());
    // 남은 높이를 차지하지 않도록 버튼 한 줄 높이로 묶는다. 차지하면 팝업 높이 측정이 계속 커진다.
    let size = egui::vec2(avail.value(), ControlSize::Sm.height(theme));
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    let start = |ui: &mut egui::Ui, used: LogicalPx| {
        ui.spacing_mut().item_spacing.x = gap.value();
        ui.add_space((avail - used).max(LogicalPx(0.0)).value());
    };
    let word = busy.then(|| {
        LogicalPx(crate::busy_caption_width(
            ui,
            theme,
            text.applying,
            spinner_size,
        ))
    });
    let both = widths[0] + gap + widths[1];
    let inline_word = word.filter(|w| *w + gap + both <= avail);
    if let (Some(w), None) = (word, inline_word) {
        ui.allocate_ui_with_layout(size, layout, |ui| {
            start(ui, w);
            crate::busy_caption(ui, theme, text.applying, spinner_size);
        });
    }
    if both <= avail {
        ui.allocate_ui_with_layout(size, layout, |ui| {
            start(ui, both + inline_word.map_or(LogicalPx(0.0), |w| w + gap));
            if inline_word.is_some() {
                crate::busy_caption(ui, theme, text.applying, spinner_size);
            }
            if cancel.show(ui, theme).clicked() {
                click = PermBoxClick::Cancel;
            }
            if apply.show(ui, theme).clicked() {
                click = PermBoxClick::Apply;
            }
        });
    } else {
        ui.allocate_ui_with_layout(size, layout, |ui| {
            start(ui, widths[0]);
            if cancel.show(ui, theme).clicked() {
                click = PermBoxClick::Cancel;
            }
        });
        ui.allocate_ui_with_layout(size, layout, |ui| {
            start(ui, widths[1]);
            if apply.show(ui, theme).clicked() {
                click = PermBoxClick::Apply;
            }
        });
    }
    click
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: PermBoxText<'static> = PermBoxText {
        heads: ["Read", "Write", "Execute"],
        roles: ["Owner", "Group", "Others"],
        read_only: "Read-only",
        windows_hint: "hint",
        failed: "failed",
        apply: "Apply",
        applying: "Applying…",
        cancel: "Cancel",
    };

    /// 영어가 아닌 내장 언어의 앱 번역 파일에서 읽은 문구.
    fn translated_texts() -> Vec<PermBoxText<'static>> {
        tasty_i18n::BUILTIN_CODES
            .iter()
            .filter(|code| **code != "en")
            .map(|code| {
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../lang/{code}.toml"));
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{} 읽기 실패: {e}", path.display()));
                let value: toml::Value = source.parse().expect("lang toml");
                let mut map = std::collections::HashMap::new();
                tasty_i18n::flatten_catalog_toml("", &value, &mut map);
                let get = |key: &str| -> &'static str {
                    let text = map
                        .get(key)
                        .unwrap_or_else(|| panic!("lang/{code}.toml에 {key}가 없다"));
                    Box::leak(text.clone().into_boxed_str())
                };
                PermBoxText {
                    heads: [
                        get("explorer.props.perm.read"),
                        get("explorer.props.perm.write"),
                        get("explorer.props.perm.execute"),
                    ],
                    roles: [
                        get("explorer.props.perm.owner"),
                        get("explorer.props.perm.group"),
                        get("explorer.props.perm.others"),
                    ],
                    read_only: get("explorer.props.perm.read_only"),
                    windows_hint: get("explorer.props.perm.windows_hint"),
                    failed: get("explorer.props.perm.failed"),
                    apply: get("explorer.props.perm.apply"),
                    applying: get("explorer.props.perm.applying"),
                    cancel: get("button.cancel"),
                }
            })
            .collect()
    }

    /// 고정 폭 영역에 상자를 그리고 그린 높이를 돌려준다.
    fn box_height(text: PermBoxText<'static>) -> f32 {
        let ctx = egui::Context::default();
        let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
        let mut height = 0.0;
        let mut bits = 0o644;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(330.0, 800.0),
            )),
            ..Default::default()
        };
        drop(ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let top = ui.cursor().top();
                    perm_box(
                        ui,
                        &theme,
                        PermBoxProps {
                            text,
                            value: PermBoxValue::Mode(&mut bits),
                            note: None,
                            failure: None,
                            busy: false,
                            changed: false,
                        },
                    );
                    height = ui.cursor().top() - top;
                });
        }));
        height
    }

    #[test]
    fn long_heads_and_roles_wrap_instead_of_truncating() {
        let short = box_height(TEXT);
        let long = box_height(PermBoxText {
            heads: ["Read access", "Write access", "Execute access"],
            roles: ["Owning user", "Group", "Everyone else"],
            ..TEXT
        });
        // 줄바꿈한 머리와 역할명만큼 상자가 높아진다. 말줄임이면 높이가 같다.
        assert!(long > short, "long {long} short {short}");
    }

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// `width` 폭 영역에 상자를 한 프레임 그리고 상자가 차지한 크기를 돌려준다.
    fn drawn_size(ctx: &egui::Context, width: f32, props: PermBoxProps<'_>) -> egui::Vec2 {
        let theme = theme();
        let mut drawn = egui::Vec2::ZERO;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.0),
            )),
            ..Default::default()
        };
        let mut props = Some(props);
        drop(ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let response = ui.scope(|ui| {
                        if let Some(props) = props.take() {
                            perm_box(ui, &theme, props);
                        }
                    });
                    drawn = response.response.rect.size();
                });
        }));
        drawn
    }

    #[test]
    fn the_layout_follows_the_box_width_bounds() {
        let theme = theme();
        let grid = theme.explorer_props_perm_grid_min_width();
        let stack = theme.explorer_props_perm_stack_min_width();
        let half = LogicalPx(0.5);
        assert_eq!(perm_layout(&theme, LogicalPx(330.0)), PermLayout::Grid);
        assert_eq!(perm_layout(&theme, grid), PermLayout::Grid);
        assert_eq!(perm_layout(&theme, grid - half), PermLayout::Stack);
        assert_eq!(perm_layout(&theme, stack), PermLayout::Stack);
        assert_eq!(perm_layout(&theme, stack - half), PermLayout::List);
        assert_eq!(perm_layout(&theme, LogicalPx(0.0)), PermLayout::List);
    }

    /// 경계 바로 아래·하한·더 좁은 폭에서도 상자가 받은 폭을 넘지 않는다(가로로 잘리지 않는다).
    #[test]
    fn the_box_never_grows_past_the_width_it_was_given() {
        let theme = theme();
        let grid = theme.explorer_props_perm_grid_min_width().value();
        let stack = theme.explorer_props_perm_stack_min_width().value();
        let long = PermBoxText {
            heads: ["Read access", "Write access", "Execute access"],
            roles: ["Owning user", "Group", "Everyone else"],
            ..TEXT
        };
        let ctx = egui::Context::default();
        for width in [
            330.0,
            grid,
            grid - theme.spacing_xs.value(),
            stack,
            stack - theme.spacing_xl.value(),
            120.0,
        ] {
            for text in [TEXT, long].into_iter().chain(translated_texts()) {
                for (busy, failure) in [
                    (false, None),
                    (true, None),
                    (false, Some("Read-only file system (os error 30)")),
                ] {
                    let mut bits = 0o644;
                    let props = PermBoxProps {
                        text,
                        value: PermBoxValue::Mode(&mut bits),
                        note: Some("Changes this folder only, not the items in it."),
                        failure,
                        busy,
                        changed: true,
                    };
                    let drawn = drawn_size(&ctx, width, props).x;
                    assert!(drawn <= width + 0.01, "Unix 폭 {width}: 그린 폭 {drawn}");
                    let mut read_only = true;
                    let props = PermBoxProps {
                        text,
                        value: PermBoxValue::ReadOnly(&mut read_only),
                        note: None,
                        failure,
                        busy,
                        changed: true,
                    };
                    let drawn = drawn_size(&ctx, width, props).x;
                    assert!(drawn <= width + 0.01, "Windows 폭 {width}: 그린 폭 {drawn}");
                }
            }
        }
    }

    /// 번역된 Windows 안내는 좁은 상자에서 말줄임 없이 줄바꿈해 상자가 높아진다.
    #[test]
    fn the_windows_hint_wraps_in_every_translated_language() {
        let theme = theme();
        let ctx = egui::Context::default();
        let narrow = theme.explorer_props_perm_stack_min_width().value() - theme.spacing_xl.value();
        for text in translated_texts() {
            let size = |width: f32| {
                let mut read_only = false;
                drawn_size(
                    &ctx,
                    width,
                    PermBoxProps {
                        text,
                        value: PermBoxValue::ReadOnly(&mut read_only),
                        note: None,
                        failure: None,
                        busy: false,
                        changed: false,
                    },
                )
            };
            let (wide, slim) = (size(600.0), size(narrow));
            assert!(
                slim.x <= narrow + 0.01,
                "{}: 폭 {narrow}를 넘었다 ({})",
                text.read_only,
                slim.x
            );
            assert!(
                slim.y > wide.y,
                "{}: 좁은 상자의 안내가 줄바꿈하지 않았다",
                text.read_only
            );
        }
    }

    /// 칸 폭이 바뀌어 배치가 격자 → 쌓기 → 목록으로 바뀌어도 포커스한 체크박스가 포커스를 잃지 않는다.
    #[test]
    fn focus_stays_on_the_same_checkbox_across_layouts() {
        let theme = theme();
        let ctx = egui::Context::default();
        let mut focused = None;
        let widths = [
            330.0,
            theme.explorer_props_perm_grid_min_width().value() - 1.0,
            theme.explorer_props_perm_stack_min_width().value() - 1.0,
            330.0,
        ];
        for (frame, width) in widths.into_iter().enumerate() {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 800.0),
                )),
                ..Default::default()
            };
            let mut bits = 0o644;
            drop(ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        // 그룹의 쓰기 칸.
                        let id = perm_box_id(ui).with((1usize, 1usize));
                        if frame == 0 {
                            ctx.memory_mut(|m| m.request_focus(id));
                        }
                        focused = Some(id);
                        perm_box(
                            ui,
                            &theme,
                            PermBoxProps {
                                text: TEXT,
                                value: PermBoxValue::Mode(&mut bits),
                                note: None,
                                failure: None,
                                busy: false,
                                changed: false,
                            },
                        );
                    });
            }));
            assert_eq!(
                ctx.memory(|m| m.focused()),
                focused,
                "폭 {width}에서 포커스를 잃었다"
            );
        }
    }

    #[test]
    fn readout_shows_text_and_octal() {
        assert_eq!(perm_readout(0o1750), "rwxr-x--- · 750");
        assert_eq!(perm_readout(0o644), "rw-r--r-- · 644");
    }
}
