use super::*;

/// 설정으로 고를 수 있는 UI 배율(small · medium · large).
const ZOOMS: [f32; 3] = [0.85, 1.0, 1.2];
/// 본체 popup 기본 폭(배율 전, 시안 `size-360`).
const POPUP_W: f32 = 360.0;

fn mocha(zoom: f32) -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom)
}

fn latte(zoom: f32) -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte.toml");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors_and_zoom(colors, is_light.unwrap_or(true), zoom)
}

/// 배율 × 테마 × 대상별 시안 조합.
fn themes() -> Vec<(String, Theme, RenamePopupLayout)> {
    ZOOMS
        .iter()
        .flat_map(|&z| [("mocha", mocha(z)), ("latte", latte(z))])
        .flat_map(|(name, theme)| {
            LAYOUTS.map(|layout| (format!("{name} {layout:?}"), theme.clone(), layout))
        })
        .collect()
}

const LAYOUTS: [RenamePopupLayout; 2] = [RenamePopupLayout::Dialog, RenamePopupLayout::Explorer];

fn view<'a>(error: Option<&'a str>, waiting: Option<&'a str>) -> RenamePopupView<'a> {
    view_in(RenamePopupLayout::Dialog, error, waiting)
}

fn view_in<'a>(
    layout: RenamePopupLayout,
    error: Option<&'a str>,
    waiting: Option<&'a str>,
) -> RenamePopupView<'a> {
    RenamePopupView {
        layout,
        save: "Save",
        cancel: "Cancel",
        error,
        save_enabled: error.is_none(),
        waiting,
        autofocus: true,
        keys: true,
    }
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

struct Frame {
    /// 콘텐츠 전체(여백 포함). 본체 popup 은 이 높이만큼 콘텐츠 영역을 잡는다.
    content: egui::Rect,
    /// 프레임마다 돌려받은 동작.
    actions: Vec<RenamePopupAction>,
    output: egui::FullOutput,
}

impl Frame {
    fn action(&self) -> &RenamePopupAction {
        self.actions.last().expect("at least one frame")
    }
}

/// popup 콘텐츠 폭에서 `frames` 의 이벤트를 한 컨텍스트에 한 프레임씩 넣고 마지막 프레임을 돌려준다.
fn run(
    theme: &Theme,
    view: &RenamePopupView<'_>,
    buffer: &mut String,
    frames: Vec<Vec<egui::Event>>,
) -> Frame {
    let ctx = egui::Context::default();
    let width = (POPUP_W * theme.ui_zoom).round();
    let mut frame = Frame {
        content: egui::Rect::NOTHING,
        actions: Vec::new(),
        output: egui::FullOutput::default(),
    };
    for events in frames {
        let raw = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut content = egui::Rect::NOTHING;
        let mut action = RenamePopupAction::None;
        frame.output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let room = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(width, 600.0));
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(room));
                let drawn = ui.scope(|ui| rename_popup(ui, theme, view, buffer));
                content = drawn.response.rect;
                action = drawn.inner;
            });
        });
        frame.content = content;
        frame.actions.push(action);
    }
    frame
}

fn settle(theme: &Theme, view: &RenamePopupView<'_>, buffer: &str) -> Frame {
    let mut buffer = buffer.to_owned();
    run(theme, view, &mut buffer, vec![vec![], vec![]])
}

/// 그린 글자 `text` 의 보이는 사각형(clip 적용)과 clip.
fn text_rect(f: &Frame, text: &str) -> (egui::Rect, egui::Rect) {
    f.output
        .shapes
        .iter()
        .find_map(|c| match &c.shape {
            egui::Shape::Text(t) if t.galley.text() == text => {
                Some((t.visual_bounding_rect().intersect(c.clip_rect), c.clip_rect))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{text:?} is drawn"))
}

/// 글자를 감싸는 가장 작은 테두리 사각형 = 입력 상자 바깥.
fn field_rect(f: &Frame, text: &str) -> egui::Rect {
    let at = text_rect(f, text).0.center();
    f.output
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Rect(r) if r.stroke.width > 0.0 && r.rect.contains(at) => Some(r.rect),
            _ => None,
        })
        .min_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("the field box is drawn")
}

fn box_colors(f: &Frame, text: &str) -> (Vec<egui::Color32>, Vec<egui::Color32>) {
    let at = text_rect(f, text).0.center();
    let rects = || {
        f.output.shapes.iter().filter_map(move |c| match &c.shape {
            egui::Shape::Rect(r) if r.rect.contains(at) => Some(r),
            _ => None,
        })
    };
    (
        rects().map(|r| r.fill).collect(),
        rects()
            .filter(|r| r.stroke.width > 0.0)
            .map(|r| r.stroke.color)
            .collect(),
    )
}

/// 그린 모든 모양이 콘텐츠 사각형 안에 있다 = popup 이 이 높이를 잡으면 잘리는 것이 없다.
fn assert_all_inside(f: &Frame, label: &str) {
    let bound = f.content.expand(0.5);
    for c in &f.output.shapes {
        if matches!(c.shape, egui::Shape::Noop) {
            continue;
        }
        let r = c.shape.visual_bounding_rect().intersect(c.clip_rect);
        // 패널 바탕처럼 콘텐츠 전체를 덮는 뒤 배경은 콘텐츠가 아니다.
        if !r.is_positive() || r.contains_rect(f.content) {
            continue;
        }
        assert!(
            bound.contains_rect(r),
            "{label}: {r:?} spills out of the content {:?}",
            f.content
        );
    }
}

/// 입력 글자는 상자 테두리와 좌우 padding 안쪽에 있고 세로 가운데다.
fn assert_text_inset(theme: &Theme, f: &Frame, text: &str, label: &str) {
    let field = field_rect(f, text);
    let bw = theme.border_width.value();
    let pad = theme.input_padding_x().value();
    assert_eq!(
        field.height(),
        theme.input_height().value(),
        "{label}: field height"
    );
    let (glyphs, clip) = text_rect(f, text);
    // TextEdit 는 caret 이 보이도록 clip 을 글자 칸보다 1px 넓힌다. 가로로 밀린 긴 이름의 글자는 그
    // clip 끝까지 보이지만 테두리에는 닿지 않는다.
    let room = field.shrink2(egui::vec2(pad - 1.0, bw)).expand(0.5);
    assert!(
        room.contains_rect(glyphs),
        "{label}: glyphs {glyphs:?} cross the padding/border of {field:?}"
    );
    assert!(
        room.contains_rect(clip.intersect(field)),
        "{label}: text clip {clip:?} reaches the border of {field:?}"
    );
    assert!(
        (glyphs.center().y - field.center().y).abs() <= 2.0,
        "{label}: glyphs {glyphs:?} are not centred in {field:?}"
    );
}

#[test]
fn the_field_keeps_its_padding_and_height_at_every_ui_scale_and_theme() {
    for (name, theme, layout) in themes() {
        let label = format!("{name} x{}", theme.ui_zoom);
        let f = settle(&theme, &view_in(layout, None, None), "Planning");
        assert_text_inset(&theme, &f, "Planning", &label);
        assert_all_inside(&f, &label);
        let k = settle(&theme, &view_in(layout, None, None), "기획 문서");
        assert_text_inset(&theme, &k, "기획 문서", &label);
    }
}

#[test]
fn the_base_height_is_the_drawn_content_so_the_first_frame_is_not_clipped() {
    for (name, theme, layout) in themes() {
        let f = settle(&theme, &view_in(layout, None, None), "Planning");
        assert_eq!(
            f.content.height(),
            rename_popup_base_height(&theme, layout).value(),
            "{name} x{}",
            theme.ui_zoom
        );
        // 버튼 글자 아래에 하단 여백이 남는다.
        for label in ["Save", "Cancel"] {
            let (r, _) = text_rect(&f, label);
            assert!(
                f.content.bottom() - r.bottom() >= theme.spacing_md.value(),
                "{name} x{}: {label} {r:?} has no bottom room in {:?}",
                theme.ui_zoom,
                f.content
            );
        }
    }
}

#[test]
fn long_error_and_waiting_lines_wrap_and_grow_the_content_and_go_away_after() {
    let ja = "このカテゴリ名はすでに使われています。別の名前を入力してください。長い説明が続く場合でも折り返して表示します。";
    let ko = "build-eu-very-long-remote-host-name에서 이름 바꾸는 중… 응답을 기다리고 있습니다";
    for (name, theme, layout) in themes() {
        let label = format!("{name} x{}", theme.ui_zoom);
        let base = rename_popup_base_height(&theme, layout).value();
        let e = settle(&theme, &view_in(layout, Some(ja), None), "Planning");
        assert!(e.content.height() > base, "{label}: error adds a line");
        let (err, _) = text_rect(&e, ja);
        assert!(
            err.height() > theme.font_size_caption.value() * 1.5,
            "{label}: the long error wraps: {err:?}"
        );
        assert_all_inside(&e, &label);
        assert_text_inset(&theme, &e, "Planning", &label);
        let w = settle(&theme, &view_in(layout, None, Some(ko)), "Planning");
        assert!(w.content.height() > base, "{label}: waiting adds a line");
        assert_all_inside(&w, &label);
        assert_text_inset(&theme, &w, "Planning", &label);
        let plain = settle(&theme, &view_in(layout, None, None), "Planning");
        assert_eq!(plain.content.height(), base, "{label}: no leftover room");
    }
}

#[test]
fn a_long_name_scrolls_inside_the_padding() {
    let long = "a-very-long-workspace-name-that-does-not-fit-the-rename-field-at-all";
    for (name, theme, layout) in themes() {
        let label = format!("{name} x{}", theme.ui_zoom);
        let f = settle(&theme, &view_in(layout, None, None), long);
        assert_text_inset(&theme, &f, long, &label);
        assert_all_inside(&f, &label);
    }
}

#[test]
fn escape_cancels_and_enter_confirms_the_focused_field() {
    let theme = mocha(1.0);
    let mut buffer = "hello".to_owned();
    let f = run(
        &theme,
        &view(None, None),
        &mut buffer,
        vec![vec![key(egui::Key::Escape)]],
    );
    assert_eq!(*f.action(), RenamePopupAction::Escape);

    let mut buffer = "hello".to_owned();
    let f = run(
        &theme,
        &RenamePopupView {
            keys: false,
            ..view(None, None)
        },
        &mut buffer,
        vec![vec![key(egui::Key::Escape)]],
    );
    assert_eq!(
        *f.action(),
        RenamePopupAction::None,
        "keys off ignores escape"
    );

    let mut buffer = "hello".to_owned();
    let f = run(
        &theme,
        &view(None, None),
        &mut buffer,
        vec![
            vec![],
            vec![egui::Event::Text("!".into())],
            vec![key(egui::Key::Enter)],
        ],
    );
    // 처음 포커스에 전체 선택이므로 입력이 이름을 바꾼다.
    assert_eq!(*f.action(), RenamePopupAction::Confirm("!".into()));
}

#[test]
fn an_invalid_name_blocks_enter_and_marks_the_field() {
    let theme = mocha(1.0);
    let mut buffer = "normal".to_owned();
    let f = run(
        &theme,
        &view(Some("“normal” is reserved."), None),
        &mut buffer,
        vec![vec![], vec![key(egui::Key::Enter)]],
    );
    assert_eq!(*f.action(), RenamePopupAction::None);
    let (_, strokes) = box_colors(&f, "normal");
    assert!(
        strokes.contains(&theme.input_border_invalid().to_egui()),
        "{strokes:?}"
    );
}

#[test]
fn a_waiting_rename_keeps_the_name_and_ignores_keys_until_the_remote_answers() {
    let theme = mocha(1.0);
    let mut buffer = "report-final.pdf".to_owned();
    let waiting = view(None, Some("Renaming on build-eu…"));
    let f = run(
        &theme,
        &waiting,
        &mut buffer,
        vec![
            vec![],
            vec![key(egui::Key::Escape)],
            vec![egui::Event::Text("x".into())],
            vec![key(egui::Key::Enter)],
        ],
    );
    assert!(
        f.actions.iter().all(|a| *a == RenamePopupAction::None),
        "{:?}",
        f.actions
    );
    assert_eq!(buffer, "report-final.pdf");
    let (fills, strokes) = box_colors(&f, "report-final.pdf");
    assert!(
        fills.contains(&theme.input_readonly_bg().to_egui()),
        "{fills:?}"
    );
    assert!(
        strokes.contains(&theme.input_readonly_border().to_egui())
            || strokes.contains(&theme.input_border_focus().to_egui()),
        "{strokes:?}"
    );
}

#[test]
fn an_editable_field_draws_the_input_box_with_the_focus_edge() {
    let theme = mocha(1.0);
    let f = settle(&theme, &view(None, None), "tasty-core");
    let (fills, strokes) = box_colors(&f, "tasty-core");
    assert!(fills.contains(&theme.input_bg().to_egui()), "{fills:?}");
    assert!(
        strokes.contains(&theme.input_border_focus().to_egui()),
        "{strokes:?}"
    );
}

#[test]
fn without_autofocus_the_field_stays_unfocused() {
    let theme = mocha(1.0);
    let mut v = view(None, None);
    v.autofocus = false;
    let f = settle(&theme, &v, "tasty-core");
    let (_, strokes) = box_colors(&f, "tasty-core");
    assert!(
        strokes.contains(&theme.input_border().to_egui()),
        "{strokes:?}"
    );
}

/// Given 같은 컨텍스트에 A·B 두 입력이 있고 A가 입력 포커스를 가졌을 때, When 키 소유가 A에서 B로 옮겨 간
/// 프레임에 글자가 들어오면, Then A 버퍼는 바뀌지 않는다.
#[test]
fn a_rename_that_loses_the_keys_drops_its_input_focus_before_reading_text() {
    let theme = mocha(1.0);
    let ctx = egui::Context::default();
    let mut buffers = ["alpha".to_owned(), "beta".to_owned()];
    // (A가 키를 가짐, B가 키를 가짐, 이벤트)
    let frames = [
        (true, false, vec![]),
        (true, false, vec![]),
        (false, true, vec![egui::Event::Text("x".into())]),
    ];
    for (a, b, events) in frames {
        let raw = egui::RawInput {
            events,
            ..Default::default()
        };
        drop(ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for (i, owns) in [a, b].into_iter().enumerate() {
                    ui.push_id(i, |ui| {
                        let v = RenamePopupView {
                            autofocus: owns,
                            keys: owns,
                            ..view(None, None)
                        };
                        rename_popup(ui, &theme, &v, &mut buffers[i]);
                    });
                }
            });
        }));
    }
    assert_eq!(buffers[0], "alpha");
}
