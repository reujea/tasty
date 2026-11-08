//! 제거 확인. 본문 끝이나 popup 이 아니라 액션 바의 내용을 그 자리에서 바꾼다.

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;

use super::plugin_detail_bar_frame;
use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::plugin_add::PLUGIN_ADD_INSET;
use crate::spinner::busy_caption;

/// 확인 바 문구. 본체는 번역 문자열을, 갤러리는 영어 문자열을 넣는다.
pub struct PluginUninstallConfirmView<'a> {
    /// `Uninstall {name}?` 처럼 이름을 넣은 질문.
    pub title: &'a str,
    /// built-in 여부에 따른 한 줄 안내.
    pub note: &'a str,
    pub cancel: &'a str,
    pub uninstall: &'a str,
    /// 확인이 막 열린 프레임이면 true. Cancel 에 키보드 포커스를 준다.
    pub focus_cancel: bool,
    /// 지우는 중이면 그 문구. 두 버튼을 끄고 버튼 왼쪽에 Spinner 와 문구를 둔다.
    pub busy: Option<&'a str>,
    /// 한 줄에 들지 않으면 버튼 묶음을 다음 줄 오른쪽으로 내린다. 끄면 글 열이 줄바꿈한다.
    pub wrap: bool,
}

/// 확인 바에서 눌린 버튼.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluginUninstallConfirmClicks {
    pub cancel: bool,
    pub uninstall: bool,
}

/// alertTriangle(accent-attention-raised) · [질문(body · text-primary) / 안내(caption · text-muted)] ·
/// Cancel(ghost) · Uninstall(danger). `focus_cancel` 이면 Cancel 에 포커스를 준다.
/// Esc 로 닫는 일은 확인 상태를 가진 호출자가 맡는다.
pub fn plugin_uninstall_confirm_bar(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginUninstallConfirmView<'_>,
) -> PluginUninstallConfirmClicks {
    let mut clicks = PluginUninstallConfirmClicks::default();
    let plan = BarPlan::layout(ui, theme, view, ui.available_width());
    let gap = theme.spacing_sm.value();
    let icon = theme.icon_glyph_size_md.value();
    plugin_detail_bar_frame(ui, theme, plan.height, |ui| {
        if plan.flow == Flow::Inline {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(icon, icon), egui::Sense::hover());
            paint_glyph(ui, theme, rect, icon);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(plan.text.width, plan.text.height),
                egui::Sense::hover(),
            );
            plan.text.paint(ui, rect.min);
            (clicks.cancel, clicks.uninstall) = buttons(ui, theme, view, view.focus_cancel);
            return;
        }
        let width = ui.available_width();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, plan.height), egui::Sense::hover());
        let mut y = rect.min.y;
        if plan.flow == Flow::GlyphAbove {
            let glyph = egui::Rect::from_min_size(rect.min, egui::vec2(icon, icon));
            paint_glyph(ui, theme, glyph, icon);
            y += icon + gap;
            plan.text.paint(ui, egui::pos2(rect.min.x, y));
            y += plan.text.height + gap;
        } else {
            let row = plan.text.height.max(icon);
            let glyph = egui::Rect::from_min_size(
                egui::pos2(rect.min.x, y + (row - icon) * 0.5),
                egui::vec2(icon, icon),
            );
            paint_glyph(ui, theme, glyph, icon);
            plan.text.paint(
                ui,
                egui::pos2(rect.min.x + icon + gap, y + (row - plan.text.height) * 0.5),
            );
            y += row + gap;
        }
        let actions = egui::Rect::from_min_size(
            egui::pos2(rect.max.x - plan.actions_width, y),
            egui::vec2(plan.actions_width, rect.max.y - y),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(actions)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = gap;
        (clicks.cancel, clicks.uninstall) = buttons(&mut child, theme, view, view.focus_cancel);
    });
    clicks
}

fn paint_glyph(ui: &egui::Ui, theme: &Theme, rect: egui::Rect, icon: f32) {
    tasty_icons::ALERT_TRIANGLE
        .image(icon, theme.accent_attention_raised().to_egui())
        .paint_at(ui, rect);
}

/// 확인 바 전체 높이. 글 열이 컨트롤 높이보다 크거나 줄이 나뉘면 그만큼 커진다.
/// `width` 는 바가 차지할 폭이다.
pub fn plugin_uninstall_confirm_bar_height(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginUninstallConfirmView<'_>,
    width: f32,
) -> f32 {
    BarPlan::layout(ui, theme, view, width).height + 2.0 * theme.spacing_md.value()
}

/// 바 안쪽 배치. 디자인 `flexWrap: wrap` 의 세 경우다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    /// 글리프 · 글 · 버튼 묶음이 한 줄이다.
    Inline,
    /// 글리프와 글이 한 줄, 버튼 묶음은 다음 줄 오른쪽이다.
    ActionsBelow,
    /// 글이 글리프 옆에도 들지 않아 글리프 · 글 · 버튼 묶음이 각 줄이다.
    GlyphAbove,
}

struct BarPlan {
    flow: Flow,
    text: TextColumn,
    actions_width: f32,
    /// 바 여백 안쪽의 높이.
    height: f32,
}

impl BarPlan {
    fn layout(
        ui: &mut egui::Ui,
        theme: &Theme,
        view: &PluginUninstallConfirmView<'_>,
        width: f32,
    ) -> Self {
        let gap = theme.spacing_sm.value();
        let icon = theme.icon_glyph_size_md.value();
        let control = ControlSize::Md.height(theme);
        let inner = (width - 2.0 * PLUGIN_ADD_INSET.value()).max(0.0);
        let actions_width = buttons_width(ui, theme, view);
        let inline_w = inner - icon - actions_width - 2.0 * gap;
        if !view.wrap {
            // Installed 확인은 버튼 묶음 오른쪽에 한 칸을 더 비워 왔다.
            let text = TextColumn::layout(ui, theme, view, (inline_w - gap).max(0.0));
            return Self {
                flow: Flow::Inline,
                height: text.height.max(control),
                text,
                actions_width,
            };
        }
        // 줄을 나누는 기준은 줄바꿈 전 글 폭이다. 긴 질문은 넓은 바에서도 한 줄이어야 나눠지지 않는다.
        let natural = TextColumn::natural_width(ui, theme, view);
        if natural <= inline_w {
            let text = TextColumn::layout(ui, theme, view, inline_w);
            return Self {
                flow: Flow::Inline,
                height: text.height.max(control),
                text,
                actions_width,
            };
        }
        let beside_glyph = inner - icon - gap;
        if natural <= beside_glyph {
            let text = TextColumn::layout(ui, theme, view, beside_glyph);
            return Self {
                flow: Flow::ActionsBelow,
                height: text.height.max(icon) + gap + control,
                text,
                actions_width,
            };
        }
        let text = TextColumn::layout(ui, theme, view, inner);
        Self {
            flow: Flow::GlyphAbove,
            height: icon + gap + text.height + gap + control,
            text,
            actions_width,
        }
    }
}

/// 질문과 안내를 글 열 폭에 맞춰 줄바꿈한 결과.
struct TextColumn {
    title: Arc<egui::Galley>,
    note: Arc<egui::Galley>,
    width: f32,
    height: f32,
}

impl TextColumn {
    fn layout(
        ui: &mut egui::Ui,
        theme: &Theme,
        view: &PluginUninstallConfirmView<'_>,
        text_w: f32,
    ) -> Self {
        let ctx = ui.ctx().clone();
        let wrap = |text: &str, size: f32, color: egui::Color32| {
            let mut job = egui::text::LayoutJob::simple(
                text.to_owned(),
                egui::FontId::proportional(size),
                color,
                text_w,
            );
            job.wrap.break_anywhere = false;
            ctx.fonts(|f| f.layout_job(job))
        };
        let title = wrap(
            view.title,
            theme.font_size_body.value(),
            theme.text_primary().to_egui(),
        );
        let note = wrap(
            view.note,
            theme.font_size_caption.value(),
            theme.text_muted().to_egui(),
        );
        let height = title.size().y + note.size().y;
        Self {
            title,
            note,
            width: text_w,
            height,
        }
    }

    /// 줄바꿈하지 않은 두 줄 중 넓은 쪽의 폭.
    fn natural_width(
        ui: &mut egui::Ui,
        theme: &Theme,
        view: &PluginUninstallConfirmView<'_>,
    ) -> f32 {
        let measure = |text: &str, size: f32| {
            ui.fonts(|f| {
                f.layout_no_wrap(
                    text.to_owned(),
                    egui::FontId::proportional(size),
                    egui::Color32::PLACEHOLDER,
                )
                .size()
                .x
            })
        };
        measure(view.title, theme.font_size_body.value())
            .max(measure(view.note, theme.font_size_caption.value()))
    }

    fn paint(&self, ui: &egui::Ui, at: egui::Pos2) {
        let mut y = at.y;
        for galley in [&self.title, &self.note] {
            let h = galley.size().y;
            ui.painter().galley(
                egui::pos2(at.x, y),
                galley.clone(),
                egui::Color32::PLACEHOLDER,
            );
            y += h;
        }
    }
}

/// 진행 표시(있으면), Cancel, Uninstall 순서로 만든다. 눌린 여부를 돌려준다. 폭을 재는 보이지 않는
/// pass 는 `focus` 를 false 로 불러 그 임시 위젯에 포커스를 주지 않는다.
fn buttons(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &PluginUninstallConfirmView<'_>,
    focus: bool,
) -> (bool, bool) {
    if let Some(word) = view.busy {
        busy_caption(ui, theme, word, theme.icon_glyph_size_sm.value());
    }
    let enabled = view.busy.is_none();
    let cancel = Button::new(view.cancel)
        .variant(ButtonVariant::Ghost)
        .enabled(enabled)
        .show(ui, theme);
    if focus {
        cancel.request_focus();
    }
    let cancel = cancel.clicked();
    let uninstall = Button::new(view.uninstall)
        .variant(ButtonVariant::Danger)
        .enabled(enabled)
        .show(ui, theme)
        .clicked();
    (cancel, uninstall)
}

/// 버튼 묶음과 그 사이 간격의 폭. 보이지 않는 sizing pass 로 잰다.
fn buttons_width(ui: &mut egui::Ui, theme: &Theme, view: &PluginUninstallConfirmView<'_>) -> f32 {
    let mut probe = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(f32::INFINITY, f32::INFINITY),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center))
            .sizing_pass()
            .invisible(),
    );
    probe.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    buttons(&mut probe, theme, view, false);
    probe.min_rect().width()
}
