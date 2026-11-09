//! 복원 실패·저장 멈춤을 알리는 비포커스 안내(RestoreNotice)와 창 오른쪽 위 안내 열의 배치.
//! 본체와 갤러리가 함께 호출한다.
//!
//! 구성은 디자인 `restore_notice.jsx`를 따른다. popup 타이틀바(제목·×), 원인 줄(alertCircle 글리프),
//! 원본 위치(mono label과 선택·줄바꿈되는 경로 상자, 복사 버튼), 설명, 오른쪽 정렬 Continue 버튼이다.
//! 안내는 나타날 때 포커스를 가져가지 않는다. 사용자가 안쪽 버튼에 포커스를 둔 상태의 Esc는
//! Continue와 같다. ×와 Continue는 같은 응답이다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::icon_button::{IconButtonState, paint_icon_button_state};
use crate::popup_title::{
    elide_popup_title, paint_popup_title_glyph, popup_title_font, popup_title_text_rect,
};

/// 안내 한 장의 콘텐츠. 문구는 모두 번역된 문자열이다.
pub struct RestoreNoticeView<'a> {
    /// 위젯 상태를 구분하는 id. 같은 화면의 안내마다 달라야 한다.
    pub id_salt: egui::Id,
    pub title: &'a str,
    /// × 버튼의 접근성 이름과 툴팁.
    pub close_label: &'a str,
    /// 첫 줄의 원인 문장.
    pub cause: &'a str,
    /// 경로 위의 font-label caption. 대문자로 표시한다.
    pub label: &'a str,
    pub path: &'a str,
    pub copy_label: &'a str,
    pub body: &'a str,
    pub continue_label: &'a str,
    /// popup 타이틀바 높이. 본체는 popup 매니저와 같은 값을 넘긴다.
    pub title_bar_height: LogicalPx,
}

/// 그린 결과.
#[derive(Clone, Debug)]
pub struct RestoreNoticeOutput {
    /// ×·Continue·안쪽 포커스의 Esc 중 하나가 눌렸다.
    pub dismissed: bool,
    /// 경로 복사 버튼이 눌렸다. 클립보드 쓰기는 호출부가 한다.
    pub copy_path: bool,
    /// 안내 전체 사각형.
    pub rect: egui::Rect,
    /// 글리프를 읽지 못했으면 그 오류. 기록은 호출부가 한다.
    pub glyph_error: Option<egui::load::LoadError>,
    /// 이번 프레임에 안쪽 컨트롤이 키보드 포커스를 가졌다. 호출부는 이때만 키 입력을 egui로 보낸다.
    pub focused_inside: bool,
}

/// 안내 열의 위치. 작업 영역 오른쪽 위에서 `notice-column-inset`만큼 들어간 지점에 오른쪽 끝을 맞춘다.
/// `top`은 오른쪽 위 pane의 탭 바(와 열린 검색 막대) 아래 경계다. 폭은 `notice-column-width`이며
/// 작업 영역 − 2 × inset을 넘지 않는다.
pub fn notice_column_rect(theme: &Theme, work_area: egui::Rect, top: f32) -> egui::Rect {
    let inset = theme.notice_column_inset().value();
    let width = theme
        .notice_column_width()
        .value()
        .min((work_area.width() - 2.0 * inset).max(0.0));
    let right = work_area.max.x - inset;
    egui::Rect::from_min_max(
        egui::pos2(right - width, top + inset),
        egui::pos2(right, work_area.max.y - inset),
    )
}

type GlyphError = std::cell::RefCell<Option<egui::load::LoadError>>;

/// 안내 한 장을 `ui`의 현재 위치에 `width` 폭으로 그린다.
pub fn restore_notice(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &RestoreNoticeView<'_>,
    width: f32,
) -> RestoreNoticeOutput {
    let radius = theme.corner_radius.value();
    let shadow = ui.painter().add(egui::Shape::Noop);
    let frame = ui.painter().add(egui::Shape::Noop);
    let top_left = ui.cursor().min;
    let mut dismissed = false;
    let mut copy_path = false;
    // 안쪽 컨트롤(×·경로·복사·Continue)에 포커스가 있을 때의 Esc만 Continue로 받는다.
    let mut focused_inside = false;
    // 안쪽 컨트롤과 그 영역. Continue는 마지막이다. 안쪽을 누르면 포커스를 그 컨트롤로 옮긴다.
    let mut controls: Vec<(egui::Id, egui::Rect)> = Vec::new();
    let glyph_error = std::cell::RefCell::new(None);

    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.set_width(width);
            let close = title_bar(ui, theme, view, width, &glyph_error);
            dismissed |= close.clicked();
            controls.push((close.id, close.rect));
            let padding = theme.restore_notice_padding().value();
            egui::Frame::NONE.inner_margin(padding).show(ui, |ui| {
                ui.set_width(width - 2.0 * padding);
                ui.spacing_mut().item_spacing.y = theme.restore_notice_gap().value();
                cause_row(ui, theme, view.cause, &glyph_error);
                let (path, copy) = path_block(ui, theme, view, &glyph_error);
                copy_path = copy.clicked();
                controls.push((path.id, path.rect));
                controls.push((copy.id, copy.rect));
                ui.label(
                    egui::RichText::new(view.body)
                        .size(theme.font_size_term_sm.value())
                        .color(theme.restore_notice_body_fg().to_egui()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    let response = Button::new(view.continue_label)
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Sm)
                        .show(ui, theme);
                    dismissed |= response.clicked();
                    controls.push((response.id, response.rect));
                });
            });
            // 창 자체의 OS 포커스는 따지지 않는다. 키가 들어온다면 창은 이미 포커스 상태다.
            focused_inside = ui.memory(|m| {
                m.focused()
                    .is_some_and(|id| controls.iter().any(|(control, _)| *control == id))
            });
            // egui는 Esc가 눌린 프레임을 시작할 때 포커스를 먼저 푼다. 직전 프레임의 포커스로 판정한다.
            let memory = view.id_salt.with("focused_inside");
            let was_focused = ui.data(|d| d.get_temp::<bool>(memory)).unwrap_or(false);
            ui.data_mut(|d| d.insert_temp(memory, focused_inside));
            dismissed |=
                (focused_inside || was_focused) && ui.input(|i| i.key_pressed(egui::Key::Escape));
        },
    );

    let rect = egui::Rect::from_min_max(
        top_left,
        egui::pos2(top_left.x + width, ui.min_rect().max.y),
    );
    // egui는 버튼을 눌러도 키보드 포커스를 주지 않는다. 디자인은 안쪽을 누른 뒤 컨트롤이 키를 받게
    // 하므로, 안쪽을 누르면 누른 컨트롤(없으면 Continue)에 포커스를 주고 바깥을 누르면 내려놓는다.
    // 나타나는 것만으로는 포커스를 주지 않는다.
    if let Some(pos) = ui.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.interact_pos())
            .flatten()
    }) {
        if rect.contains(pos) {
            if !dismissed
                && let Some((id, _)) = controls
                    .iter()
                    .find(|(_, rect)| rect.contains(pos))
                    .or(controls.last())
            {
                ui.memory_mut(|m| m.request_focus(*id));
                focused_inside = true;
            }
        } else if focused_inside {
            ui.memory_mut(|m| {
                if let Some(id) = m.focused()
                    && controls.iter().any(|(control, _)| *control == id)
                {
                    m.surrender_focus(id);
                }
            });
            focused_inside = false;
        }
    }
    ui.painter().set(
        shadow,
        theme.shadow_popover().to_egui().as_shape(rect, radius),
    );
    ui.painter().set(
        frame,
        egui::Shape::Rect(egui::epaint::RectShape::new(
            rect,
            radius,
            theme.bg_panel().to_egui(),
            egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
            egui::StrokeKind::Outside,
        )),
    );
    RestoreNoticeOutput {
        dismissed,
        copy_path,
        rect,
        glyph_error: glyph_error.into_inner(),
        focused_inside,
    }
}

/// popup 타이틀바와 같은 모양(bg-sidebar 채움, 가운데 제목, 아래 1px 선, 오른쪽 ×)이다.
/// ×가 눌리면 true다.
fn title_bar(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &RestoreNoticeView<'_>,
    width: f32,
    errors: &GlyphError,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, view.title_bar_height.value()),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    let cr = theme.corner_radius.value() as u8;
    painter.rect_filled(
        rect,
        egui::CornerRadius {
            nw: cr,
            ne: cr,
            sw: 0,
            se: 0,
        },
        theme.bg_sidebar().to_egui(),
    );
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(theme.border_width.value(), theme.border_frame().to_egui()),
    );
    let size = theme.popup_title_btn_size();
    let close = egui::Rect::from_center_size(
        egui::pos2(
            rect.max.x - (size.scaled(0.5) + theme.popup_title_edge_inset()).value(),
            rect.center().y,
        ),
        egui::Vec2::splat(size.value()),
    );
    let font = popup_title_font(theme);
    let area = popup_title_text_rect(rect, close.min.x, theme.popup_title_text_gap());
    let shown = elide_popup_title(ui.ctx(), view.title, font.clone(), area.width());
    painter.with_clip_rect(area).text(
        area.center(),
        egui::Align2::CENTER_CENTER,
        shown,
        font,
        theme.text_primary().to_egui(),
    );
    let response = ui
        .interact(close, view.id_salt.with("close"), egui::Sense::click())
        .on_hover_text(view.close_label);
    let tint = paint_icon_button_state(
        ui.painter(),
        theme,
        close,
        IconButtonState::ghost(response.hovered(), response.is_pointer_button_down_on()),
    );
    let glyph = ControlSize::Sm.icon_glyph(theme);
    let glyph_rect = egui::Rect::from_center_size(close.center(), egui::Vec2::splat(glyph));
    if let Err(error) =
        paint_popup_title_glyph(ui.ctx(), ui.painter(), tasty_icons::CLOSE, glyph_rect, tint)
    {
        *errors.borrow_mut() = Some(error);
    }
    response
}

/// alertCircle 글리프와 원인 문장. 글리프는 문장 첫 줄의 가운데에 맞춘다.
fn cause_row(ui: &mut egui::Ui, theme: &Theme, cause: &str, errors: &GlyphError) {
    let glyph = theme.icon_glyph_size_md.value();
    let gap = theme.spacing_sm.value();
    let width = ui.available_width();
    let galley = ui.painter().layout(
        cause.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_primary().to_egui(),
        (width - glyph - gap).max(0.0),
    );
    let first_row = galley.rows.first().map_or(glyph, |row| row.height());
    let height = galley.size().y.max(glyph);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let icon = egui::Rect::from_center_size(
        egui::pos2(rect.min.x + glyph / 2.0, rect.min.y + first_row / 2.0),
        egui::Vec2::splat(glyph),
    );
    if let Err(error) = paint_popup_title_glyph(
        ui.ctx(),
        ui.painter(),
        tasty_icons::ALERT_CIRCLE,
        icon,
        theme.restore_notice_glyph().to_egui(),
    ) {
        *errors.borrow_mut() = Some(error);
    }
    ui.painter().galley(
        egui::pos2(rect.min.x + glyph + gap, rect.min.y),
        galley,
        theme.text_primary().to_egui(),
    );
}

/// mono label과 경로 상자. 경로는 선택할 수 있고 어디서든 줄을 바꾼다. 경로와 복사 버튼의 응답을 돌려준다.
fn path_block(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &RestoreNoticeView<'_>,
    errors: &GlyphError,
) -> (egui::Response, egui::Response) {
    let xs = theme.spacing_xs.value();
    let sm = theme.spacing_sm.value();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = xs;
        ui.label(
            egui::RichText::new(view.label.to_uppercase())
                .family(crate::label_font_family())
                .size(theme.font_size_micro.value())
                .color(theme.restore_notice_label_fg().to_egui()),
        );
        egui::Frame::NONE
            .fill(theme.restore_notice_path_bg().to_egui())
            .stroke(egui::Stroke::new(
                theme.border_width.value(),
                theme.restore_notice_path_border().to_egui(),
            ))
            .corner_radius(theme.corner_radius.value())
            .inner_margin(egui::Margin {
                left: sm as i8,
                right: xs as i8,
                top: xs as i8,
                bottom: xs as i8,
            })
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing.x = xs;
                let side = ControlSize::Sm.height(theme);
                let text_width = (ui.available_width() - side - xs).max(0.0);
                ui.horizontal_top(|ui| {
                    let mut job = egui::text::LayoutJob::single_section(
                        view.path.to_owned(),
                        egui::TextFormat::simple(
                            egui::FontId::monospace(theme.font_size_caption.value()),
                            theme.restore_notice_path_fg().to_egui(),
                        ),
                    );
                    job.wrap.max_width = text_width;
                    job.wrap.break_anywhere = true;
                    let path = ui.allocate_ui(egui::vec2(text_width, 0.0), |ui| {
                        ui.set_min_width(text_width);
                        // 가로 layout의 Label은 줄바꿈 폭을 무한대로 바꾼다. 미리 배치한 galley를 넘겨
                        // 어디서든 줄을 바꾸는 폭을 지킨다.
                        let galley = ui.fonts(|fonts| fonts.layout_job(job));
                        ui.add(egui::Label::new(galley).selectable(true))
                    });
                    let response = crate::icon_button::IconButton::new()
                        .size(ControlSize::Sm)
                        .show(ui, theme, &|ui, rect, tint| {
                            if let Err(error) = paint_popup_title_glyph(
                                ui.ctx(),
                                ui.painter(),
                                tasty_icons::COPY,
                                rect,
                                tint,
                            ) {
                                *errors.borrow_mut() = Some(error);
                            }
                        })
                        .on_hover_text(view.copy_label);
                    (path.inner, response)
                })
                .inner
            })
            .inner
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    fn frame(ctx: &egui::Context, theme: &Theme, events: Vec<egui::Event>) -> RestoreNoticeOutput {
        let mut output = None;
        // 시험은 그린 결과만 보고 출력 텍스처·명령은 쓰지 않는다.
        let _output = ctx.run(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    output = Some(restore_notice(
                        ui,
                        theme,
                        &RestoreNoticeView {
                            id_salt: egui::Id::new("test_notice"),
                            title: "title",
                            close_label: "close",
                            cause: "cause",
                            label: "label",
                            path: "/path",
                            copy_label: "copy",
                            body: "body",
                            continue_label: "continue",
                            title_bar_height: LogicalPx(28.0),
                        },
                        360.0,
                    ));
                });
            },
        );
        output.expect("drawn")
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn esc_continues_only_while_a_control_inside_has_focus() {
        let theme = theme();
        let ctx = egui::Context::default();
        assert!(
            !frame(&ctx, &theme, Vec::new()).focused_inside,
            "appearing does not take focus"
        );
        assert!(
            !frame(&ctx, &theme, vec![key(egui::Key::Escape)]).dismissed,
            "Esc outside the notice is left alone"
        );
        assert!(frame(&ctx, &theme, vec![key(egui::Key::Tab)]).focused_inside);
        assert!(
            ctx.memory(|m| m.focused().is_some()),
            "Tab reached a control"
        );
        assert!(frame(&ctx, &theme, vec![key(egui::Key::Escape)]).dismissed);
    }

    fn press(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn release(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]
    }

    #[test]
    fn a_click_inside_hands_keys_to_the_notice_and_a_click_outside_takes_them_back() {
        let theme = theme();
        let ctx = egui::Context::default();
        let rect = frame(&ctx, &theme, Vec::new()).rect;
        // The cause row holds no control, so the press lands on the notice body itself.
        let inside = egui::pos2(rect.center().x, rect.top() + 40.0);
        assert!(frame(&ctx, &theme, press(inside)).focused_inside);
        assert!(frame(&ctx, &theme, release(inside)).focused_inside);
        let outside = egui::pos2(rect.left() - 100.0, rect.bottom() + 100.0);
        assert!(!frame(&ctx, &theme, press(outside)).focused_inside);
        assert!(!frame(&ctx, &theme, release(outside)).focused_inside);
        assert!(
            !frame(&ctx, &theme, vec![key(egui::Key::Escape)]).dismissed,
            "keys are back outside the notice"
        );
        frame(&ctx, &theme, press(inside));
        frame(&ctx, &theme, release(inside));
        assert!(frame(&ctx, &theme, vec![key(egui::Key::Escape)]).dismissed);
    }

    #[test]
    fn the_column_hangs_from_the_top_right_inset() {
        let theme = theme();
        let work = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1200.0, 800.0));
        let column = notice_column_rect(&theme, work, 40.0);
        let inset = theme.notice_column_inset().value();
        assert_eq!(column.max.x, 1200.0 - inset);
        assert_eq!(column.min.y, 40.0 + inset);
        assert_eq!(column.width(), theme.notice_column_width().value());
    }

    #[test]
    fn a_narrow_work_area_narrows_the_column() {
        let theme = theme();
        let work = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 400.0));
        let column = notice_column_rect(&theme, work, 0.0);
        let inset = theme.notice_column_inset().value();
        assert_eq!(column.width(), 200.0 - 2.0 * inset);
    }
}
