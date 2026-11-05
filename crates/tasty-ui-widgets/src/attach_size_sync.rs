//! 원격 attach mirror 의 터미널 크기 동기화가 자동 재시도 뒤에도 실패했을 때의 Workspace 배너 내용.
//! 거절 배너와 같은 계열로 행 [글리프 | 제목·본문 | 다시 시도 · 닫기]이며 모든 칸을 위쪽에 맞춘다.
//! 본문은 두 줄이다. 이름 줄은 한 줄로 두고 이름만 줄이며, 고정 문구 줄은 감싸고 자르지 않는다.
//! 좁은 스코프에서는 다시 시도 버튼만 본문 왼쪽 가장자리에 맞춰 다음 줄로 내려가고 닫기는 오른쪽 위에 남는다.
//! 여러 surface 가 함께 실패하면 한 장에 "N surfaces — a, b +n" 으로 묶고 버튼은 Retry all 이 된다.
//! 문자열은 호출자가 주입한다. 큐·표시 범위는 본체 BannerManager가 정한다.

use tasty_type_appearance::theme::Theme;

use crate::banner::{ActionRow, action_slot, banner_close_button, banner_shell};
use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::spinner::Spinner;
use crate::tooltip::{Tooltip, tooltip_hover_delay_elapsed};

/// 본문 줄에 이름으로 보이는 항목 수. 같은 이름은 한 항목이며 나머지 항목은 `+n` 으로 줄인다.
const SHOWN_NAMES: usize = 2;

/// 배너 입력값.
pub struct AttachSizeSyncBannerView<'a> {
    pub title: &'a str,
    /// 본문 첫 줄 번역문. `{}` 하나에 이름 묶음이 들어간다.
    pub names_line: &'a str,
    /// 본문 둘째 줄의 고정 문구.
    pub hint: &'a str,
    /// surface 가 둘 이상일 때의 이름 묶음 번역문. 앞 `{}` 는 수, 뒤 `{}` 는 이름이다.
    pub many: &'a str,
    /// 실패한 surface 의 이름(탭 제목). 비어 있으면 배너를 띄우지 않는 것이 호출자의 몫이다.
    pub names: &'a [&'a str],
    pub retry: &'a str,
    pub retry_all: &'a str,
    /// 닫기 버튼의 툴팁.
    pub dismiss: &'a str,
    /// 다시 시도의 응답을 기다리는 중이면 버튼을 비활성으로 두고 앞에 Spinner 를 그린다.
    pub retrying: bool,
    /// 스코프가 좁으면([`crate::banner_is_narrow`]) 다시 시도 버튼을 글 아래 줄로 내린다.
    pub narrow: bool,
}

/// 이번 프레임의 클릭.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachSizeSyncBannerClicks {
    pub retry: bool,
    pub dismiss: bool,
}

impl AttachSizeSyncBannerView<'_> {
    fn many(&self) -> bool {
        self.names.len() > 1
    }

    fn retry_label(&self) -> &str {
        if self.many() {
            self.retry_all
        } else {
            self.retry
        }
    }
}

/// `text` 가 `max_width` 를 넘으면 끝을 잘라 말줄임표를 붙인다.
fn elide_to_width(ui: &egui::Ui, text: &str, font: &egui::FontId, max_width: f32) -> String {
    let width = |s: &str| {
        ui.fonts(|f| {
            f.layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .rect
                .width()
        })
    };
    if width(text) <= max_width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut keep = chars.len();
    while keep > 0 {
        keep -= 1;
        let candidate: String = chars[..keep].iter().chain(['\u{2026}'].iter()).collect();
        if width(&candidate) <= max_width {
            return candidate;
        }
    }
    "\u{2026}".to_owned()
}

/// 이름 줄의 조각. 이름은 줄어드는 칸이고 나머지(수 · 구분자 · `+n`)는 줄지 않는다.
enum Piece<'a> {
    Fixed(String),
    Name(&'a str),
}

/// 같은 이름을 한 항목으로 묶는다. 처음 나온 순서를 지키고 각 항목의 surface 수를 센다.
fn name_entries<'a>(names: &[&'a str]) -> Vec<(&'a str, usize)> {
    let mut entries: Vec<(&str, usize)> = Vec::new();
    for &name in names {
        match entries.iter_mut().find(|(n, _)| *n == name) {
            Some((_, k)) => *k += 1,
            None => entries.push((name, 1)),
        }
    }
    entries
}

/// 이름 줄의 조각 순서. 앞 두 항목만 보이고 나머지 항목은 ` +n` 으로 줄인다.
/// 같은 이름이 여럿이면 이름 뒤에 ` ×k` 를 붙인다. 수(`N surfaces`)는 항목이 아니라 surface 수다.
fn name_pieces<'a>(view: &AttachSizeSyncBannerView<'a>) -> Vec<Piece<'a>> {
    let mut subject = Vec::new();
    let (many_before, many_after) = if view.many() {
        let mut parts = view.many.splitn(3, "{}");
        let before = parts.next().unwrap_or_default();
        let mid = parts.next().unwrap_or_default();
        let after = parts.next().unwrap_or_default();
        (format!("{before}{}{mid}", view.names.len()), after)
    } else {
        (String::new(), "")
    };
    subject.push(Piece::Fixed(many_before));
    let entries = name_entries(view.names);
    for (i, (name, k)) in entries.iter().take(SHOWN_NAMES).enumerate() {
        if i > 0 {
            subject.push(Piece::Fixed(", ".to_owned()));
        }
        subject.push(Piece::Name(name));
        if *k > 1 {
            subject.push(Piece::Fixed(format!(" \u{d7}{k}")));
        }
    }
    let rest = entries.len().saturating_sub(SHOWN_NAMES);
    if rest > 0 {
        subject.push(Piece::Fixed(format!(" +{rest}")));
    }
    subject.push(Piece::Fixed(many_after.to_owned()));

    let (before, after) = view
        .names_line
        .split_once("{}")
        .unwrap_or((view.names_line, ""));
    let mut out = vec![Piece::Fixed(before.to_owned())];
    out.extend(subject);
    out.push(Piece::Fixed(after.to_owned()));
    out.retain(|p| !matches!(p, Piece::Fixed(t) if t.is_empty()));
    out
}

/// 이름 칸의 폭. 각 칸은 본래 폭(`max` 까지)에서 출발하고, `avail` 을 넘으면 본래 폭에 비례해
/// 줄되 `floor` 아래로는 줄지 않는다(CSS `flex: 0 1 auto` 와 min/max-width). 하한은 줄어들 때만
/// 적용한다. 칸 = min(본래 폭, max(floor, 줄인 폭)) 이라 `floor` 보다 짧은 이름은 제 폭을 지킨다.
fn shrink_names(natural: &[f32], floor: f32, max: f32, avail: f32) -> Vec<f32> {
    let floor_of = |i: usize| natural[i].min(floor);
    let mut size: Vec<f32> = natural.iter().map(|w| w.min(max)).collect();
    let mut open: Vec<usize> = (0..natural.len()).collect();
    loop {
        let over = size.iter().sum::<f32>() - avail;
        let weight: f32 = open.iter().map(|&i| natural[i]).sum();
        if over <= 0.0 || weight <= 0.0 {
            return size;
        }
        let shrunk: Vec<f32> = (0..size.len())
            .map(|i| size[i] - over * natural[i] / weight)
            .collect();
        let (at_floor, rest): (Vec<usize>, Vec<usize>) =
            open.iter().partition(|&&i| shrunk[i] <= floor_of(i));
        if at_floor.is_empty() {
            for &i in &rest {
                size[i] = shrunk[i];
            }
            return size;
        }
        for &i in &at_floor {
            size[i] = floor_of(i);
        }
        open = rest;
    }
}

/// 본문 첫 줄. 한 줄로 두고 줄지 않는 조각과 이름 칸을 이어 놓는다. 이름은 칸 폭에서 말줄임하고,
/// 이름이 칸보다 짧으면 다음 조각을 칸 끝으로 민다. 줄이 `width` 를 넘으면 호출자가 잘라 그린다.
fn names_galley(
    ui: &egui::Ui,
    theme: &Theme,
    view: &AttachSizeSyncBannerView<'_>,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let size = theme.banner_body_font_size().value();
    let font = egui::FontId::proportional(size);
    let measure = |s: &str| {
        ui.fonts(|f| {
            f.layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        })
    };
    let pieces = name_pieces(view);
    let fixed: f32 = pieces
        .iter()
        .map(|p| match p {
            Piece::Fixed(t) => measure(t),
            Piece::Name(_) => 0.0,
        })
        .sum();
    let natural: Vec<f32> = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Name(n) => Some(measure(n)),
            Piece::Fixed(_) => None,
        })
        .collect();
    let boxes = shrink_names(
        &natural,
        theme.attach_sync_name_min_width().value(),
        theme.attach_sync_name_max_width().value(),
        width - fixed,
    );

    let format = |color: egui::Color32| egui::TextFormat {
        font_id: font.clone(),
        color,
        line_height: Some(size * theme.line_height_ui),
        valign: egui::Align::BOTTOM,
        ..Default::default()
    };
    let muted = theme.banner_body_fg().to_egui();
    let name_color = theme.text_secondary_raised().to_egui();
    let mut job = egui::text::LayoutJob::default();
    let mut boxes = boxes.into_iter();
    let mut pad = 0.0;
    for piece in pieces {
        match piece {
            Piece::Fixed(text) => {
                job.append(&text, pad, format(muted));
                pad = 0.0;
            }
            Piece::Name(name) => {
                let w = boxes.next().unwrap_or_default();
                let shown = elide_to_width(ui, name, &font, w);
                job.append(&shown, pad, format(name_color));
                pad = (w - measure(&shown)).max(0.0);
            }
        }
    }
    job.wrap.max_width = f32::INFINITY;
    ui.fonts(|f| f.layout_job(job))
}

/// 본문 둘째 줄. 고정 문구는 감싸고 자르지 않는다.
fn hint_galley(
    ui: &egui::Ui,
    theme: &Theme,
    text: &str,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let size = theme.banner_body_font_size().value();
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: theme.banner_body_fg().to_egui(),
            line_height: Some(size * theme.line_height_ui),
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap_width;
    ui.fonts(|f| f.layout_job(job))
}

fn title_galley(
    ui: &egui::Ui,
    theme: &Theme,
    text: &str,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let size = theme.banner_title_font_size().value();
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: theme.banner_fg().to_egui(),
            line_height: Some(size * theme.line_height_ui),
            ..Default::default()
        },
    );
    job.wrap.max_width = wrap_width;
    ui.fonts(|f| f.layout_job(job))
}

/// 다시 시도 버튼의 크기. `Button`과 같은 식으로 잰다.
fn button_size(ui: &egui::Ui, theme: &Theme, view: &AttachSizeSyncBannerView<'_>) -> egui::Vec2 {
    let font = egui::FontId::proportional(ControlSize::Sm.font_size(theme));
    let text_w = ui.fonts(|f| {
        f.layout_no_wrap(
            view.retry_label().to_owned(),
            font,
            egui::Color32::PLACEHOLDER,
        )
        .rect
        .width()
    });
    let spinner_w = if view.retrying {
        theme.icon_glyph_size_sm.value() + theme.button_gap().value()
    } else {
        0.0
    };
    egui::vec2(
        text_w + spinner_w + 2.0 * ControlSize::Sm.pad_x(theme),
        ControlSize::Sm.height(theme),
    )
}

/// 배너 셸 안쪽에 내용을 그린다. 본체 BannerManager처럼 셸을 따로 그리는 호출자가 쓴다.
pub fn attach_size_sync_banner_content(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &AttachSizeSyncBannerView<'_>,
) -> AttachSizeSyncBannerClicks {
    let row_w = ui.available_width();
    let glyph = theme.icon_glyph_size_md.value();
    let gap = theme.banner_gap().value();
    let nudge = theme.banner_glyph_offset().value();
    let text_gap = theme.banner_text_gap().value();
    let layout = ActionRow {
        row_w,
        glyph,
        gap,
        button: button_size(ui, theme, view),
        close: theme.icon_button_size_sm().value(),
        pair_gap: theme.spacing_xs.value(),
        narrow: view.narrow,
    };
    let text_w = layout.text_width();

    let title = title_galley(ui, theme, view.title, text_w);
    let names = names_galley(ui, theme, view, text_w);
    let hint = hint_galley(ui, theme, view.hint, text_w);
    let text_h = title.size().y + text_gap + names.size().y + text_gap + hint.size().y;
    let place = layout.place(glyph + nudge, text_h);

    let (row, _) = ui.allocate_exact_size(egui::vec2(row_w, place.row_h), egui::Sense::hover());
    let glyph_rect =
        egui::Rect::from_min_size(row.min + egui::vec2(0.0, nudge), egui::vec2(glyph, glyph));
    tasty_icons::ALERT_TRIANGLE
        .image(glyph, theme.attach_sync_glyph().to_egui())
        .paint_at(ui, glyph_rect);

    let text_x = row.left() + glyph + gap;
    let names_top = row.top() + title.size().y + text_gap;
    let hint_top = names_top + names.size().y + text_gap;
    let painter = ui.painter();
    painter.galley(
        egui::pos2(text_x, row.top()),
        title,
        egui::Color32::PLACEHOLDER,
    );
    // 이름 줄은 줄바꿈하지 않으므로 글 열 폭을 넘는 끝은 잘라 그린다.
    let names_clip = egui::Rect::from_min_size(
        egui::pos2(text_x, names_top),
        egui::vec2(text_w, names.size().y),
    );
    painter
        .with_clip_rect(painter.clip_rect().intersect(names_clip))
        .galley(names_clip.min, names, egui::Color32::PLACEHOLDER);
    painter.galley(
        egui::pos2(text_x, hint_top),
        hint,
        egui::Color32::PLACEHOLDER,
    );

    let spinner_size = theme.icon_glyph_size_sm;
    let spinner = |ui: &mut egui::Ui, rect: egui::Rect, _: egui::Color32| {
        Spinner::new()
            .size(spinner_size.value())
            .color(theme.spinner_indicator().to_egui())
            .paint_in(ui, theme, rect);
    };
    let mut button = Button::new(view.retry_label())
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Sm)
        .enabled(!view.retrying);
    if view.retrying {
        button = button
            .leading_icon(&spinner)
            .leading_icon_size(spinner_size);
    }
    let retry = button
        .show(
            &mut action_slot(ui, row.min + place.button.to_vec2(), layout.button),
            theme,
        )
        .clicked()
        && !view.retrying;
    let close = banner_close_button(
        &mut action_slot(
            ui,
            row.min + place.close.to_vec2(),
            egui::Vec2::splat(layout.close),
        ),
        theme,
    );
    if tooltip_hover_delay_elapsed(ui.ctx(), theme, close.id, close.hovered()) {
        Tooltip::new(view.dismiss)
            .id_source(close.id)
            .show(ui, theme, close.rect);
    }
    AttachSizeSyncBannerClicks {
        retry,
        dismiss: close.clicked(),
    }
}

/// 셸과 내용을 함께 그린다. 갤러리처럼 BannerManager 없이 배너 한 장을 보이는 곳에서 쓴다.
pub fn attach_size_sync_banner(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &AttachSizeSyncBannerView<'_>,
) -> AttachSizeSyncBannerClicks {
    let mut clicks = AttachSizeSyncBannerClicks::default();
    banner_shell(ui, theme, 1.0, |ui| {
        clicks = attach_size_sync_banner_content(ui, theme, view);
    });
    clicks
}

#[cfg(test)]
mod tests {
    use super::*;

    const HINT: &str = "The remote may still be using the old size.";

    fn view<'a>(names: &'a [&'a str], retrying: bool) -> AttachSizeSyncBannerView<'a> {
        AttachSizeSyncBannerView {
            title: "Couldn't sync the terminal size with the remote",
            names_line: "{}",
            hint: HINT,
            many: "{} surfaces — {}",
            names,
            retry: "Retry",
            retry_all: "Retry all",
            dismiss: "Dismiss",
            retrying,
            narrow: false,
        }
    }

    fn theme() -> Theme {
        Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
    }

    /// 배너를 폭 `width`로 한 번 그려 셸 rect와 모든 도형을 돌려준다.
    fn render(width: f32, v: &AttachSizeSyncBannerView<'_>) -> (egui::Rect, Vec<egui::Shape>) {
        let theme = theme();
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 400.0),
            )),
            ..Default::default()
        };
        let out = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let before = ui.cursor().min;
                    attach_size_sync_banner(ui, &theme, v);
                    rect = egui::Rect::from_min_max(before, ui.min_rect().max);
                });
        });
        (rect, out.shapes.into_iter().map(|c| c.shape).collect())
    }

    fn texts(shapes: &[egui::Shape]) -> Vec<String> {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect()
    }

    /// 글자 도형 하나의 rect 와 galley.
    fn text_shape(
        shapes: &[egui::Shape],
        pred: impl Fn(&str) -> bool,
    ) -> (egui::Rect, std::sync::Arc<egui::Galley>) {
        shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if pred(t.galley.text()) => {
                    Some((s.visual_bounding_rect(), t.galley.clone()))
                }
                _ => None,
            })
            .expect("text shape")
    }

    /// galley 에서 `color` 로 칠한 조각의 글자.
    fn colored(galley: &egui::Galley, color: egui::Color32) -> Vec<&str> {
        galley
            .job
            .sections
            .iter()
            .filter(|sec| sec.format.color == color)
            .map(|sec| &galley.job.text[sec.byte_range.clone()])
            .collect()
    }

    #[test]
    fn one_surface_reads_its_name_on_the_first_line_and_the_fixed_copy_on_the_second() {
        let (_, shapes) = render(460.0, &view(&["build"], false));
        let (names, _) = text_shape(&shapes, |t| t == "build");
        let (hint, _) = text_shape(&shapes, |t| t == HINT);
        assert!(hint.top() >= names.bottom(), "{hint:?} not under {names:?}");
        assert!((hint.left() - names.left()).abs() <= 0.5);
        assert!(texts(&shapes).iter().any(|t| t == "Retry"));
    }

    #[test]
    fn several_surfaces_share_one_card_with_two_names_and_a_count() {
        let theme = theme();
        let names = ["release", "build", "tests"];
        let (_, shapes) = render(460.0, &view(&names, false));
        let (_, line) = text_shape(&shapes, |t| t.starts_with("3 surfaces"));
        assert_eq!(line.text(), "3 surfaces — release, build +1");
        assert_eq!(
            colored(&line, theme.text_secondary_raised().to_egui()),
            ["release", "build"]
        );
        assert_eq!(
            colored(&line, theme.banner_body_fg().to_egui()),
            ["3 surfaces — ", ", ", " +1"]
        );
        // "build" 는 하한 40 보다 짧지만 칸이 제 폭이라 뒤 " +1" 이 이름 바로 뒤에 붙는다.
        let plus = line
            .job
            .sections
            .iter()
            .find(|sec| &line.job.text[sec.byte_range.clone()] == " +1")
            .expect("+1");
        assert_eq!(plus.leading_space, 0.0);
        assert!(texts(&shapes).iter().any(|t| t == "Retry all"));
    }

    #[test]
    fn a_long_name_is_cut_at_the_name_width_and_the_count_and_separators_stay() {
        let theme = theme();
        let long = "release-pipeline-watch-logs-eu-west-with-a-very-long-tab-title";
        let (_, shapes) = render(460.0, &view(&[long, "build", "tests"], false));
        let (_, line) = text_shape(&shapes, |t| t.starts_with("3 surfaces"));
        let shown = colored(&line, theme.text_secondary_raised().to_egui());
        assert!(shown[0].ends_with('\u{2026}'), "{shown:?}");
        assert!(shown[0].len() < long.len());
        assert_eq!(shown[1], "build");
        assert!(line.text().ends_with(" +1"), "{}", line.text());
    }

    #[test]
    fn names_shrink_by_their_width_down_to_the_floor_then_the_line_overflows() {
        // 다 들어가면 본래 폭을 160 까지로 자른 값이다. 짧은 이름은 하한 40 을 채우지 않는다.
        assert_eq!(
            shrink_names(&[24.0, 300.0], 40.0, 160.0, 400.0),
            [24.0, 160.0]
        );
        // 하한보다 짧은 이름은 줄어들 때도 제 폭에서 멈추고, 나머지 칸이 부족분을 받는다.
        let s = shrink_names(&[24.0, 300.0], 40.0, 160.0, 100.0);
        assert!(
            (s[0] - 24.0).abs() < 1e-3 && (s[1] - 76.0).abs() < 1e-3,
            "{s:?}"
        );
        // 넘친 60을 본래 폭 비율(100:200)로 나눠 줄인다.
        let s = shrink_names(&[100.0, 200.0], 40.0, 160.0, 200.0);
        assert!(
            (s[0] - 80.0).abs() < 1e-3 && (s[1] - 120.0).abs() < 1e-3,
            "{s:?}"
        );
        // 비율대로면 45 칸이 33 으로 줄어 하한 40 에 멈추고, 나머지 칸이 남은 부족분을 받는다.
        let s = shrink_names(&[45.0, 160.0], 40.0, 160.0, 150.0);
        assert!(
            (s[0] - 40.0).abs() < 1e-3 && (s[1] - 110.0).abs() < 1e-3,
            "{s:?}"
        );
        // 모두 하한이면 더 줄지 않고 줄이 넘친다(그리는 쪽이 잘라 낸다).
        assert_eq!(
            shrink_names(&[100.0, 200.0], 40.0, 160.0, 50.0),
            [40.0, 40.0]
        );
    }

    #[test]
    fn equal_names_collapse_into_one_entry_with_a_muted_count() {
        let theme = theme();
        let (_, shapes) = render(460.0, &view(&["Shell", "Shell"], false));
        let (_, line) = text_shape(&shapes, |t| t.starts_with("2 surfaces"));
        assert_eq!(line.text(), "2 surfaces — Shell \u{d7}2");
        assert_eq!(
            colored(&line, theme.text_secondary_raised().to_egui()),
            ["Shell"]
        );
        assert_eq!(
            colored(&line, theme.banner_body_fg().to_egui()),
            ["2 surfaces — ", " \u{d7}2"]
        );
        assert!(texts(&shapes).iter().any(|t| t == "Retry all"));
    }

    #[test]
    fn the_count_is_surfaces_and_the_rest_is_entries() {
        let names = ["Shell", "build", "Shell", "tests", "Shell"];
        let (_, shapes) = render(460.0, &view(&names, false));
        let (_, line) = text_shape(&shapes, |t| t.starts_with("5 surfaces"));
        assert_eq!(line.text(), "5 surfaces — Shell \u{d7}3, build +1");
    }

    #[test]
    fn the_title_wraps_without_a_row_cap() {
        let v = AttachSizeSyncBannerView {
            title: "Couldn't sync the terminal size with the remote, and this title keeps going \
                    so that it needs more than three rows in a narrow card",
            narrow: true,
            ..view(&["build"], false)
        };
        let (_, shapes) = render(200.0, &v);
        let (_, title) = text_shape(&shapes, |t| t.starts_with("Couldn't"));
        assert!(title.rows.len() > 3, "{} rows", title.rows.len());
        assert!(!title.text().contains('\u{2026}'));
    }

    #[test]
    fn the_fixed_copy_wraps_and_is_never_cut() {
        let v = AttachSizeSyncBannerView {
            narrow: true,
            ..view(&["build"], false)
        };
        let (_, shapes) = render(200.0, &v);
        let (_, hint) = text_shape(&shapes, |t| t == HINT);
        assert!(hint.rows.len() > 1, "the hint did not wrap");
        assert!(!hint.text().contains('\u{2026}'));
    }

    /// 프레임마다 주어진 입력을 넣고 그 프레임의 클릭과 마지막 프레임의 도형을 돌려준다.
    fn run_frames(
        v: &AttachSizeSyncBannerView<'_>,
        frames: &[Vec<egui::Event>],
    ) -> (Vec<AttachSizeSyncBannerClicks>, Vec<egui::Shape>) {
        let theme = theme();
        let ctx = egui::Context::default();
        let mut clicks = Vec::new();
        let mut shapes = Vec::new();
        for (i, events) in frames.iter().enumerate() {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(460.0, 400.0),
                )),
                time: Some(i as f64 * 0.1),
                events: events.clone(),
                ..Default::default()
            };
            let mut frame = AttachSizeSyncBannerClicks::default();
            let out = ctx.run(input, |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        frame = attach_size_sync_banner(ui, &theme, v);
                    });
            });
            clicks.push(frame);
            shapes = out.shapes.into_iter().map(|c| c.shape).collect();
        }
        (clicks, shapes)
    }

    /// 라벨 글자를 감싸는 가장 작은 채움 사각형 — Retry 버튼의 rect.
    fn retry_button_rect(shapes: &[egui::Shape]) -> egui::Rect {
        let label = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text() == "Retry" => {
                    Some(s.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("Retry label");
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) if r.rect.contains_rect(label) => Some(r.rect),
                _ => None,
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("Retry button rect")
    }

    fn click_at(pos: egui::Pos2) -> Vec<Vec<egui::Event>> {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![
            vec![],
            vec![egui::Event::PointerMoved(pos)],
            vec![button(true)],
            vec![button(false)],
        ]
    }

    #[test]
    fn retrying_keeps_the_label_and_draws_the_spinner_inside_the_button() {
        let theme = theme();
        let (_, shapes) = run_frames(&view(&["build"], true), &[vec![]]);
        assert!(texts(&shapes).iter().any(|t| t == "Retry"));
        let button = retry_button_rect(&shapes);
        let label = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.text() == "Retry" => {
                    Some(s.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("Retry label");
        // Spinner 는 지름이 Spinner 크기인 바탕 고리와 그 위를 도는 indicator 색 호로 그린다.
        let ink = theme.spinner_indicator().to_egui();
        let size = theme.icon_glyph_size_sm.value();
        let arcs: Vec<egui::Rect> = shapes
            .iter()
            .filter(|s| {
                matches!(s, egui::Shape::Path(p) if p.stroke.color == egui::epaint::ColorMode::Solid(ink))
            })
            .map(|s| s.visual_bounding_rect())
            .collect();
        // 고리 둘레는 호와 같은 반지름이라 bounding box 가 Spinner 크기보다 조금 작다.
        let ring = shapes
            .iter()
            .filter(|s| matches!(s, egui::Shape::Circle(c) if c.fill == egui::Color32::TRANSPARENT))
            .map(|s| s.visual_bounding_rect())
            .find(|r| {
                r.width() <= size + 0.5
                    && r.width() >= size * 0.75
                    && arcs.iter().any(|a| r.expand(0.5).contains_rect(*a))
            })
            .unwrap_or_else(|| panic!("no spinner ring around the arcs {arcs:?}"));
        assert!(button.contains_rect(ring), "{ring:?} outside {button:?}");
        assert!(
            ring.right() <= label.left(),
            "{ring:?} is not before {label:?}"
        );
    }

    #[test]
    fn retrying_does_not_report_a_click_on_retry() {
        // 상태마다 버튼 폭이 달라 그 상태에서 잰 Retry 가운데를 누른다.
        let press = |retrying: bool| {
            let v = view(&["build"], retrying);
            let (_, shapes) = run_frames(&v, &[vec![]]);
            run_frames(&v, &click_at(retry_button_rect(&shapes).center())).0
        };
        assert!(
            press(false).iter().any(|c| c.retry),
            "the click does not reach Retry, so the retrying case proves nothing"
        );
        assert!(press(true).iter().all(|c| !c.retry && !c.dismiss));
    }

    #[test]
    fn a_narrow_banner_puts_the_buttons_under_the_body_at_its_left_edge() {
        let v = AttachSizeSyncBannerView {
            narrow: true,
            ..view(&["build"], false)
        };
        let body_of = |shapes: &[egui::Shape]| {
            shapes
                .iter()
                .find_map(|s| match s {
                    egui::Shape::Text(t) if t.galley.text() == HINT => {
                        Some(s.visual_bounding_rect())
                    }
                    _ => None,
                })
                .expect("body")
        };
        let (_, shapes) = render(360.0, &v);
        let body = body_of(&shapes);
        let button = retry_button_rect(&shapes);
        assert!(
            button.top() >= body.bottom(),
            "{button:?} not under {body:?}"
        );
        assert!(
            (button.left() - body.left()).abs() <= 0.5,
            "{button:?} not at the body edge {body:?}"
        );
        let (_, wide) = render(460.0, &view(&["build"], false));
        let (beside, wide_body) = (retry_button_rect(&wide), body_of(&wide));
        assert!(
            beside.left() >= wide_body.right() && beside.top() < wide_body.bottom(),
            "the wide banner wrapped too"
        );
    }

    #[test]
    fn a_narrow_banner_keeps_the_close_button_in_the_top_right_corner() {
        let theme = theme();
        let v = AttachSizeSyncBannerView {
            narrow: true,
            ..view(&["build"], false)
        };
        let (_, shapes) = run_frames(&v, &[vec![]]);
        let card = shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) if r.blur_width == 0.0 => Some(r.rect),
                _ => None,
            })
            .max_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("card");
        let close = theme.icon_button_size_sm().value();
        let corner = egui::pos2(
            card.right() - theme.spacing_md.value() - close * 0.5,
            card.top() + theme.spacing_sm.value() + close * 0.5,
        );
        let clicks = run_frames(&v, &click_at(corner)).0;
        assert!(clicks.iter().any(|c| c.dismiss), "no × at {corner:?}");
        assert!(clicks.iter().all(|c| !c.retry));
        // Retry 는 그 아래 줄로 내려가 × 와 같은 줄에 있지 않다.
        assert!(retry_button_rect(&shapes).top() > corner.y + close * 0.5);
    }

    #[test]
    fn a_narrow_banner_stays_inside_its_width() {
        let names = ["release-pipeline-watch-logs-eu-west", "build", "tests"];
        let v = AttachSizeSyncBannerView {
            narrow: true,
            ..view(&names, true)
        };
        let (rect, shapes) = render(280.0, &v);
        for s in &shapes {
            let card_ink = match s {
                egui::Shape::Text(_) => true,
                egui::Shape::Rect(r) => r.blur_width == 0.0,
                _ => false,
            };
            if !card_ink {
                continue;
            }
            let b = s.visual_bounding_rect();
            if b.is_positive() {
                assert!(b.right() <= rect.right() + 0.5, "{b:?} past {rect:?}");
            }
        }
    }
}
