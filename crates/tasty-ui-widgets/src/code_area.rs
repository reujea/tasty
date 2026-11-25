//! 줄 번호 거터가 있는 여러 줄 고정폭 입력. 상자는 Input 과 같고(surface-raised · border-default ·
//! radius · focus 테두리와 ring · invalid · disabled), 글자는 mono caption 이다.
//! 줄을 바꿔 감싸지 않는다. 테두리를 포함한 바깥 높이가 `codearea-max-height` 에 닿으면 상자 안에서
//! 스크롤하며,
//! 거터는 가로 스크롤에도 왼쪽에 남는다.
//! 디자인의 오류 줄 번호 semibold 는 egui UI 에 굵은 글꼴을 등록하지 않아 재현하지 않는다.

use std::sync::Arc;

use tasty_type_appearance::theme::Theme;

/// CodeArea 빌더.
pub struct CodeArea<'a> {
    id_salt: egui::Id,
    placeholder: &'a str,
    min_rows: usize,
    invalid: bool,
    error_line: Option<usize>,
    enabled: bool,
    keys: Option<CodeAreaKeys<'a>>,
}

/// 확정·취소 키 판정. 위젯은 키를 정하지 않고, 호출자가 단축키 설정을 읽어 넘긴다. 맞은 키
/// 이벤트는 소비해야 한다 — 확정 키가 줄바꿈으로 함께 들어가지 않게 위젯보다 먼저 부른다.
#[derive(Clone, Copy)]
pub struct CodeAreaKeys<'a> {
    pub submit: &'a dyn Fn(&mut egui::InputState) -> bool,
    pub cancel: &'a dyn Fn(&mut egui::InputState) -> bool,
}

/// 한 프레임의 결과.
pub struct CodeAreaOutput {
    /// 글자 영역 TextEdit 의 응답(`changed()` 로 변경 감지). `rect` 는 상자 전체다.
    pub response: egui::Response,
    /// 포커스 중 [`CodeAreaKeys::submit`] 이 맞았다. 그 키는 줄바꿈을 넣지 않는다.
    pub submit: bool,
    /// 포커스 중 [`CodeAreaKeys::cancel`] 이 맞았다.
    pub cancel: bool,
}

impl<'a> CodeArea<'a> {
    /// `id_salt` 는 프레임마다 같은 값이어야 커서와 스크롤 위치가 유지된다. 글자 영역의 id 는
    /// `egui::Id::new(id_salt)` 이므로 호출자가 이 id 로 포커스를 요청할 수 있다.
    pub fn new(id_salt: impl std::hash::Hash) -> Self {
        Self {
            id_salt: egui::Id::new(id_salt),
            placeholder: "",
            min_rows: 4,
            invalid: false,
            error_line: None,
            enabled: true,
            keys: None,
        }
    }

    /// 확정·취소 키. 없으면 키로는 확정·취소하지 않는다.
    pub fn keys(mut self, keys: Option<CodeAreaKeys<'a>>) -> Self {
        self.keys = keys;
        self
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// 거터가 적어도 보여 줄 줄 수. 기본 4.
    pub fn min_rows(mut self, rows: usize) -> Self {
        self.min_rows = rows.max(1);
        self
    }

    /// danger 테두리. `error_line` 이 있으면 이것 없이도 켜진다.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// 표시할 줄(1부터). 그 줄의 거터 번호가 danger 색이 되고 글자 영역에 tint 띠가 깔린다.
    pub fn error_line(mut self, line: Option<usize>) -> Self {
        self.error_line = line;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show(self, ui: &mut egui::Ui, theme: &Theme, buf: &mut String) -> CodeAreaOutput {
        let bw = theme.border_width.value();
        let radius = theme.input_radius().value();
        let size = theme.codearea_font_size().value();
        let line_h = size * theme.line_height_ui;
        let pad_x = theme.codearea_padding_x().value();
        let pad_y = theme.codearea_padding_y().value();
        let gutter_pad = theme.spacing_xs.value();
        let font = egui::FontId::monospace(size);
        let invalid = self.invalid || self.error_line.is_some();

        let lines = buf.split('\n').count().max(self.min_rows);
        let content_h = lines as f32 * line_h + pad_y * 2.0;
        // 디자인 kit 은 border-box 라 max-height 는 테두리를 포함한 바깥 높이다.
        let outer_h = (content_h + bw * 2.0).min(theme.codearea_max_height().value());
        let width = ui.available_width();
        let (outer, _) = ui.allocate_exact_size(egui::vec2(width, outer_h), egui::Sense::hover());
        let inner = outer.shrink(bw);

        let bg = if self.enabled {
            theme.input_bg()
        } else {
            theme.state_disabled_fill()
        };
        ui.painter().rect_filled(outer, radius, bg.to_egui());

        // 거터 폭: 가장 긴 줄 번호 + 좌우 space-xs, 최소 codearea-gutter-width.
        let digits = ui.painter().layout_no_wrap(
            lines.to_string(),
            font.clone(),
            egui::Color32::PLACEHOLDER,
        );
        let gutter_w = (digits.rect.width() + gutter_pad * 2.0)
            .max(theme.codearea_gutter_width().value())
            .min(inner.width());
        let gutter = egui::Rect::from_min_size(inner.min, egui::vec2(gutter_w, inner.height()));
        let body =
            egui::Rect::from_min_max(egui::pos2(gutter.right() + bw, inner.top()), inner.max);

        let ink = if self.enabled {
            theme.input_fg().to_egui()
        } else {
            theme.state_disabled_fg().to_egui()
        };
        let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| -> Arc<egui::Galley> {
            ui.fonts(|f| f.layout_job(line_job(text, font.clone(), ink, line_h)))
        };

        // TextEdit 에 들어가기 전에 확정·취소 키를 가로챈다. 확정 키가 줄바꿈으로 먼저 처리되지 않게
        // 하려는 것이다. egui 는 프레임 시작에서 Esc 로 포커스를 이미 풀었으므로 직전 프레임의
        // 포커스도 포커스로 본다.
        let edit_id = self.id_salt;
        let focus_key = edit_id.with("was_focused");
        let was_focused = ui.data(|d| d.get_temp::<bool>(focus_key).unwrap_or(false));
        let focused = self.enabled && (was_focused || ui.memory(|m| m.has_focus(edit_id)));
        let (submit, cancel) = match self.keys {
            Some(keys) if focused => ui.input_mut(|i| ((keys.submit)(i), (keys.cancel)(i))),
            _ => (false, false),
        };

        let band_fill = theme
            .accent_danger()
            .to_egui()
            .gamma_multiply(theme.tint_fill_alpha());
        let hint = if self.enabled {
            tasty_egui_theme::hint_text(theme, self.placeholder)
        } else {
            egui::RichText::new(self.placeholder).color(ink)
        }
        .font(font.clone())
        .line_height(Some(line_h));
        let mut field = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        field.set_clip_rect(body.intersect(ui.clip_rect()));
        let scroll = egui::ScrollArea::both()
            .id_salt(self.id_salt.with("scroll"))
            .max_height(body.height())
            // 글자 영역의 끌기는 선택이다. 스크롤은 휠과 막대로 한다.
            .drag_to_scroll(false)
            .auto_shrink([false, false])
            .show(&mut field, |ui| {
                let origin = ui.cursor().min;
                let te_w = ui.available_width() - pad_x * 2.0;
                if let Some(line) = self.error_line.filter(|l| *l >= 1) {
                    let top = origin.y + pad_y + (line - 1) as f32 * line_h;
                    let band = egui::Rect::from_min_size(
                        egui::pos2(origin.x, top),
                        egui::vec2(ui.available_width().max(body.width()), line_h),
                    );
                    ui.painter().rect_filled(band, 0.0, band_fill);
                }
                egui::TextEdit::multiline(buf)
                    .id(edit_id)
                    .frame(false)
                    .margin(egui::Margin::symmetric(pad_x as i8, pad_y as i8))
                    .desired_width(te_w.max(0.0))
                    .min_size(egui::vec2(0.0, content_h))
                    .hint_text(hint)
                    .layouter(&mut layouter)
                    .interactive(self.enabled)
                    .show(ui)
                    .response
            });
        let response = scroll.inner;
        let offset_y = scroll.state.offset.y;

        // 거터는 글자 영역과 같은 세로 위치로 그리고, 가로 스크롤과 무관하게 왼쪽에 둔다.
        let painter = ui.painter_at(gutter.intersect(ui.clip_rect()));
        painter.rect_filled(gutter, 0.0, theme.codearea_gutter_bg().to_egui());
        let gutter_fg = if self.enabled {
            theme.codearea_gutter_fg().to_egui()
        } else {
            theme.state_disabled_fg().to_egui()
        };
        for i in 0..lines {
            let n = i + 1;
            let color = if self.error_line == Some(n) {
                theme.codearea_error_fg().to_egui()
            } else {
                gutter_fg
            };
            let galley =
                ui.fonts(|f| f.layout_job(line_job(&n.to_string(), font.clone(), color, line_h)));
            let top = gutter.top() + pad_y + i as f32 * line_h - offset_y;
            let pos = egui::pos2(gutter.right() - gutter_pad - galley.size().x, top);
            painter.galley(pos, galley, color);
        }
        ui.painter().vline(
            gutter.right() + bw * 0.5,
            gutter.y_range(),
            egui::Stroke::new(bw, theme.codearea_gutter_border().to_egui_premultiplied()),
        );

        let has_focus = response.has_focus();
        ui.data_mut(|d| d.insert_temp(focus_key, has_focus && !cancel));
        let border = if !self.enabled {
            theme.state_disabled_border().to_egui()
        } else if invalid {
            theme.input_border_invalid().to_egui()
        } else if has_focus {
            theme.input_border_focus().to_egui()
        } else {
            theme.input_border().to_egui()
        };
        ui.painter().rect_stroke(
            outer,
            radius,
            egui::Stroke::new(bw, border),
            egui::StrokeKind::Inside,
        );
        if has_focus {
            let ring = if invalid {
                theme.input_border_invalid().to_egui()
            } else {
                theme.input_border_focus().to_egui()
            };
            crate::focus_ring::paint_field_ring(ui.painter(), theme, outer, radius, ring);
        }

        let mut response = response;
        response.rect = outer;
        CodeAreaOutput {
            response,
            submit,
            cancel,
        }
    }
}

/// 줄을 감싸지 않고 줄 높이를 `line_h` 로 고정한 레이아웃. 글자 영역과 거터가 같은 줄 높이를 쓴다.
fn line_job(
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    line_h: f32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: font,
            color,
            line_height: Some(line_h),
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.wrap.max_width = f32::INFINITY;
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        ctx: &egui::Context,
        buf: &mut String,
        error_line: Option<usize>,
    ) -> (egui::Rect, egui::FullOutput) {
        let theme = tasty_themes::mocha_fallback();
        let mut rect = egui::Rect::NOTHING;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 600.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = CodeArea::new("t")
                        .error_line(error_line)
                        .show(ui, &theme, buf)
                        .response
                        .rect;
                });
            },
        );
        (rect, output)
    }

    /// 포커스 링은 Input과 같이 상자 테두리 바로 바깥에 틈 없이 붙은 border_width 한 줄이다.
    #[test]
    fn the_focus_ring_sits_flush_outside_the_box_edge() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut buf = "a".to_owned();
        let id = std::cell::Cell::new(None);
        let ring = |o: &egui::FullOutput| -> Vec<(egui::Rect, f32)> {
            o.shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Rect(r)
                        if r.stroke.width > 0.0 && r.stroke_kind == egui::StrokeKind::Outside =>
                    {
                        Some((r.rect, r.stroke.width))
                    }
                    _ => None,
                })
                .collect()
        };
        let frame = |ctx: &egui::Context, buf: &mut String| {
            let mut rect = egui::Rect::NOTHING;
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let o = CodeArea::new("t").show(ui, &theme, buf);
                        id.set(Some(o.response.id));
                        rect = o.response.rect;
                    });
                },
            );
            (rect, out)
        };
        let (_, rest) = frame(&ctx, &mut buf);
        assert!(ring(&rest).is_empty());
        let focus = id.get().expect("drawn");
        ctx.memory_mut(|m| m.request_focus(focus));
        let (outer, focused) = frame(&ctx, &mut buf);
        assert_eq!(ring(&focused), vec![(outer, theme.border_width.value())]);
    }

    /// 상자는 min_rows 만큼 자라고, 바깥 높이가 max-height 에 닿으면 그 높이에서 멈춘다.
    #[test]
    fn the_box_grows_with_lines_up_to_the_max_height() {
        let theme = tasty_themes::mocha_fallback();
        let line_h = theme.codearea_font_size().value() * theme.line_height_ui;
        let chrome = theme.codearea_padding_y().value() * 2.0 + theme.border_width.value() * 2.0;
        let ctx = egui::Context::default();

        let (rect, _) = run(&ctx, &mut String::new(), None);
        assert!((rect.height() - (4.0 * line_h + chrome)).abs() < 0.01);

        let mut six = "a\n".repeat(5) + "a";
        let (rect, _) = run(&ctx, &mut six, None);
        assert!((rect.height() - (6.0 * line_h + chrome)).abs() < 0.01);

        let mut many = "a\n".repeat(200);
        let (rect, _) = run(&ctx, &mut many, None);
        // max-height 는 테두리를 포함한 바깥 높이다(kit border-box).
        let max = theme.codearea_max_height().value();
        assert!((rect.height() - max).abs() < 0.01);
    }

    /// 오류 줄이 있으면 테두리가 danger 가 되고 tint 띠가 그려진다.
    #[test]
    fn an_error_line_turns_the_edge_danger_and_paints_a_band() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut text = "a\nb\nc".to_string();
        let danger = theme.input_border_invalid().to_egui();
        let band = theme
            .accent_danger()
            .to_egui()
            .gamma_multiply(theme.tint_fill_alpha());
        let fills = |o: &egui::FullOutput| -> (bool, bool) {
            let mut edge = false;
            let mut tinted = false;
            for c in &o.shapes {
                if let egui::Shape::Rect(r) = &c.shape {
                    edge |= r.stroke.width > 0.0 && r.stroke.color == danger;
                    tinted |= r.fill == band;
                }
            }
            (edge, tinted)
        };
        let (_, plain) = run(&ctx, &mut text, None);
        assert_eq!(fills(&plain), (false, false));
        let (_, marked) = run(&ctx, &mut text, Some(2));
        assert_eq!(fills(&marked), (true, true));
    }

    /// 포커스 중 넘긴 확정 키는 submit 이 되고 줄바꿈을 넣지 않는다. Enter 는 줄바꿈, 취소 키는
    /// cancel 이다. 키를 넘기지 않으면 같은 입력이 아무 동작도 하지 않는다.
    #[test]
    fn given_keys_submit_and_cancel_and_enter_is_a_newline() {
        let theme = tasty_themes::mocha_fallback();
        let ctx = egui::Context::default();
        let mut buf = "a".to_string();
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let submit_key = |i: &mut egui::InputState| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Enter,
            ))
        };
        let cancel_key =
            |i: &mut egui::InputState| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        let keys = CodeAreaKeys {
            submit: &submit_key,
            cancel: &cancel_key,
        };
        let frame = |buf: &mut String,
                     keys: Option<CodeAreaKeys<'_>>,
                     events: Vec<egui::Event>|
         -> (bool, bool) {
            let mut out = (false, false);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let o = CodeArea::new("k").keys(keys).show(ui, &theme, buf);
                        if !o.response.has_focus() {
                            o.response.request_focus();
                        }
                        out = (o.submit, o.cancel);
                    });
                },
            );
            drop(output);
            out
        };
        frame(&mut buf, Some(keys), Vec::new());
        frame(&mut buf, Some(keys), Vec::new());
        let unbound = frame(
            &mut buf,
            None,
            vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
        );
        assert_eq!(unbound, (false, false));
        buf = "a".to_string();
        frame(&mut buf, Some(keys), Vec::new());
        let submit = frame(
            &mut buf,
            Some(keys),
            vec![key(egui::Key::Enter, egui::Modifiers::COMMAND)],
        );
        assert_eq!(submit, (true, false));
        assert_eq!(buf, "a");
        let newline = frame(
            &mut buf,
            Some(keys),
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(newline, (false, false));
        assert_eq!(buf.matches('\n').count(), 1);
        let cancel = frame(
            &mut buf,
            Some(keys),
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert_eq!(cancel, (false, true));
    }
}
