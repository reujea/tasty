//! 글꼴 행의 오류 줄을 실제 격자로 잰다. 파일을 읽는 판정과 미리보기 등록까지 호출 경로 그대로 지난다.

use super::*;
use crate::adapters::ui::font_judge::fixtures::{
    RENAMED_FAMILY, VALID, negative_height, not_a_font, renamed, without_table,
};

fn file_text() -> &'static str {
    t("settings.appearance.font.custom_font_rejected")
}

fn family_text() -> &'static str {
    t("settings.appearance.font.family_rejected")
}

struct Drawn {
    texts: Vec<String>,
    family: egui::Response,
    custom: egui::Response,
    preview_top: f32,
}

impl Drawn {
    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|t| t == text)
    }
}

struct Harness {
    ctx: egui::Context,
    settings: Settings,
    fonts: PreviewFonts,
    dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let harness = Self {
            ctx: egui::Context::default(),
            settings: Settings::default(),
            fonts: PreviewFonts::default(),
            dir: tempfile::tempdir().unwrap(),
        };
        // 첫 pass 에는 글꼴 상태가 없다.
        drop(harness.ctx.run(egui::RawInput::default(), |_| {}));
        harness
    }

    fn file(&self, bytes: &[u8]) -> String {
        let path = self.dir.path().join("x.ttf");
        std::fs::write(&path, bytes).unwrap();
        path.display().to_string()
    }

    /// 기본 글꼴 격자와 미리보기를 한 프레임 그린다.
    fn base(&mut self, events: Vec<egui::Event>) -> Drawn {
        let col = crate::theme::theme().settings_label_width();
        let raw = raw_input(events);
        let (settings, fonts) = (&mut self.settings, &mut self.fonts);
        let mut seen = None;
        let out = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    seen = Some(default_font_section(
                        ui,
                        font_setting_rows(),
                        col,
                        settings,
                        &mut None,
                        fonts,
                    ));
                });
        });
        let (family, custom, preview_top) = seen.expect("frame ran");
        Drawn {
            texts: drawn_texts(&out),
            family,
            custom,
            preview_top,
        }
    }

    /// 대상의 override 격자와 미리보기를 한 프레임 그리고 글자를 돌려준다.
    fn surface(&mut self, target: SurfaceFontTarget<'_>, events: Vec<egui::Event>) -> Vec<String> {
        let raw = raw_input(events);
        let (settings, fonts) = (&mut self.settings, &mut self.fonts);
        let out = self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    draw_surface_font_section(ui, settings, &mut None, fonts, target);
                });
        });
        drawn_texts(&out)
    }

    /// 판정과 등록이 끝나도록 입력 없이 두 프레임 그린 뒤의 글자.
    fn surface_settled(&mut self, target: SurfaceFontTarget<'_>) -> Vec<String> {
        self.surface(target, Vec::new());
        self.surface(target, Vec::new());
        self.surface(target, Vec::new())
    }

    fn base_settled(&mut self) -> Drawn {
        self.base(Vec::new());
        self.base(Vec::new());
        self.base(Vec::new())
    }

    fn click_base(&mut self, pos: egui::Pos2) {
        self.base(vec![egui::Event::PointerMoved(pos), press(pos, true)]);
        self.base(vec![press(pos, false)]);
    }
}

fn raw_input(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(620.0, 800.0),
        )),
        focused: true,
        events,
        ..Default::default()
    }
}

fn drawn_texts(out: &egui::FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn typed(text: &str) -> Vec<egui::Event> {
    vec![egui::Event::Text(text.to_owned())]
}

const TERMINAL: SurfaceFontTarget<'static> = SurfaceFontTarget::Terminal;
const EXPLORER: SurfaceFontTarget<'static> = SurfaceFontTarget::Plugin {
    storage_key: "explorer",
};

/// 터미널 행과 플러그인 행에 파일을 넣었을 때 줄이 보이는지.
fn line_shown(target: SurfaceFontTarget<'_>, bytes: &[u8]) -> bool {
    let mut h = Harness::new();
    let path = h.file(bytes);
    let mut app = std::mem::take(&mut h.settings.appearance);
    target.override_mut(&mut app).custom_font_path = Some(path);
    h.settings.appearance = app;
    h.surface_settled(target).iter().any(|t| t == file_text())
}

#[test]
fn a_saved_bad_file_shows_the_line_without_input() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.custom_font_path = h.file(&not_a_font());
    assert!(h.base_settled().has(file_text()));
}

/// 저장값은 입력 없이 줄이 나와야 한다. 설정 창은 dirty 일 때만 그리므로 판정 뒤 다음 프레임을 요청해야 한다.
#[test]
fn the_first_frame_asks_for_the_frame_that_shows_the_verdict() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.custom_font_path = h.file(&not_a_font());
    assert!(!h.base(Vec::new()).has(file_text()));
    assert!(h.ctx.has_requested_repaint());
    assert!(h.base(Vec::new()).has(file_text()));

    let mut h = Harness::new();
    h.settings.appearance.terminal_font.custom_font_path = Some(h.file(&not_a_font()));
    let first = h.surface(TERMINAL, Vec::new());
    assert!(!first.iter().any(|t| t == file_text()));
    assert!(h.ctx.has_requested_repaint());
    let next = h.surface(TERMINAL, Vec::new());
    assert!(next.iter().any(|t| t == file_text()));
}

#[test]
fn empty_values_show_no_line() {
    let mut h = Harness::new();
    let drawn = h.base_settled();
    assert!(!drawn.has(file_text()) && !drawn.has(family_text()));
    let mut h = Harness::new();
    h.settings.appearance.terminal_font.custom_font_path = Some(String::new());
    h.settings.appearance.terminal_font.font_family = Some(String::new());
    let texts = h.surface_settled(TERMINAL);
    assert!(!texts.iter().any(|t| t == file_text() || t == family_text()));
}

/// 두 경로가 갈리는 입력. 터미널 행은 UI 판정을 쓰지 않고 플러그인 행은 UI 판정을 쓴다.
#[test]
fn a_row_follows_the_renderer_it_configures() {
    let a3 = &VALID[..1024];
    let t1 = negative_height();
    let t3 = without_table(b"cmap");
    let n1 = without_table(b"hhea");
    let cases: [(&str, &[u8], bool, bool); 6] = [
        ("a1", &not_a_font(), true, true),
        ("a3", a3, false, true),
        ("t1", &t1, true, false),
        ("t3", &t3, false, true),
        ("n1", &n1, true, true),
        ("normal", VALID, false, false),
    ];
    let mut wrong = Vec::new();
    for (name, bytes, plugin, terminal) in cases {
        let got = (line_shown(EXPLORER, bytes), line_shown(TERMINAL, bytes));
        if got != (plugin, terminal) {
            wrong.push((name, got));
        }
    }
    assert!(wrong.is_empty(), "wrong (plugin, terminal): {wrong:?}");
}

/// 줄 높이가 음수인 파일이 미리보기까지 지나도 패닉하지 않는다. `Harness` 가 글자를 layout 까지 그린다.
#[test]
fn a_negative_height_font_reaches_layout_in_every_row() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.custom_font_path = h.file(&negative_height());
    assert!(h.base_settled().has(file_text()));
}

#[test]
fn use_default_shows_no_line_even_if_the_default_is_rejected() {
    let mut h = Harness::new();
    let path = h.file(&not_a_font());
    h.settings.appearance.default_font.custom_font_path = path;
    let texts = h.surface_settled(TERMINAL);
    assert!(!texts.iter().any(|t| t == file_text()));
    let texts = h.surface_settled(EXPLORER);
    assert!(!texts.iter().any(|t| t == file_text()));
}

#[test]
fn focus_hides_the_ink_and_keeps_the_rows_where_they_were() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.custom_font_path = h.file(&not_a_font());
    let before = h.base_settled();
    assert!(before.has(file_text()));

    h.click_base(before.custom.rect.center());
    let focused = h.base(Vec::new());
    assert!(focused.custom.has_focus());
    assert!(!focused.has(file_text()));
    assert_eq!(focused.preview_top, before.preview_top);
}

#[test]
fn typing_a_bad_value_shows_nothing_until_focus_leaves() {
    let mut h = Harness::new();
    let clean = h.base_settled();
    h.click_base(clean.custom.rect.center());
    let drawn = h.base(typed("/no/such/font.ttf"));
    assert_eq!(
        h.settings.appearance.default_font.custom_font_path,
        "/no/such/font.ttf"
    );
    assert!(!drawn.has(file_text()));
    let typing = h.base_settled();
    assert!(!typing.has(file_text()));
    assert_eq!(typing.preview_top, clean.preview_top);

    // 입력 밖을 누르면 그 값으로 줄이 생긴다.
    h.click_base(egui::pos2(600.0, 790.0));
    assert!(h.base_settled().has(file_text()));
}

#[test]
fn leaving_with_a_good_value_closes_the_line() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.custom_font_path = h.file(&not_a_font());
    let bad = h.base_settled();
    assert!(bad.has(file_text()));
    h.click_base(bad.custom.rect.center());
    h.settings.appearance.default_font.custom_font_path.clear();
    let held = h.base_settled();
    assert!(!held.has(file_text()));
    assert_eq!(
        held.preview_top, bad.preview_top,
        "the box stays while typing"
    );

    h.click_base(egui::pos2(600.0, 790.0));
    let left = h.base_settled();
    assert!(!left.has(file_text()));
    assert!(left.preview_top < bad.preview_top, "the box closes");
}

/// 터미널 판정은 파일을 읽으므로 입력 중에는 돌지 않고 포커스가 빠질 때 돈다.
#[test]
fn the_terminal_judgement_waits_until_focus_leaves() {
    let mut h = Harness::new();
    h.settings.appearance.terminal_font.custom_font_path = Some(String::new());
    h.surface_settled(TERMINAL);
    let judged = |h: &Harness| h.fonts.terminal_key().map(str::to_owned);
    let before = judged(&h);

    let id = egui::Id::new(("font_custom_field", "terminal"));
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.surface(TERMINAL, Vec::new());
    h.surface(TERMINAL, typed("/no/such/font.ttf"));
    let texts = h.surface_settled(TERMINAL);
    assert_eq!(judged(&h), before, "no judgement while typing");
    assert!(!texts.iter().any(|t| t == file_text()));

    h.ctx.memory_mut(|m| m.surrender_focus(id));
    let texts = h.surface_settled(TERMINAL);
    assert_ne!(judged(&h), before);
    assert!(texts.iter().any(|t| t == file_text()));
}

#[test]
fn a_family_that_is_gone_shows_the_family_line_in_both_renderers() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.font_family = "Tasty No Such Family".to_owned();
    let drawn = h.base_settled();
    assert!(drawn.has(family_text()));
    assert!(!drawn.has(file_text()));

    let mut h = Harness::new();
    h.settings.appearance.terminal_font.font_family = Some("Tasty No Such Family".to_owned());
    assert!(
        h.surface_settled(TERMINAL)
            .iter()
            .any(|t| t == family_text())
    );

    let mut h = Harness::new();
    h.settings
        .appearance
        .plugin_font_overrides
        .entry("explorer".to_owned())
        .or_default()
        .font_family = Some("Tasty No Such Family".to_owned());
    assert!(
        h.surface_settled(EXPLORER)
            .iter()
            .any(|t| t == family_text())
    );
}

#[test]
fn a_family_the_rows_file_provides_shows_no_line() {
    let mut h = Harness::new();
    let path = h.file(&renamed(VALID.to_vec()));
    h.settings.appearance.default_font.font_family = RENAMED_FAMILY.to_owned();
    h.settings.appearance.default_font.custom_font_path = path.clone();
    let drawn = h.base_settled();
    assert!(!drawn.has(family_text()) && !drawn.has(file_text()));

    let mut h = Harness::new();
    let path2 = h.file(&renamed(VALID.to_vec()));
    h.settings.appearance.terminal_font.font_family = Some(RENAMED_FAMILY.to_owned());
    h.settings.appearance.terminal_font.custom_font_path = Some(path2);
    let texts = h.surface_settled(TERMINAL);
    assert!(!texts.iter().any(|t| t == family_text() || t == file_text()));
}

/// 다른 행의 파일이 제공한 이름으로 통과하지 않는다. 터미널 행이 판정한 뒤 파일을 비워도 조회에 남지 않는다.
#[test]
fn a_family_from_another_rows_file_shows_the_line() {
    let mut h = Harness::new();
    let path = h.file(&renamed(VALID.to_vec()));
    h.settings.appearance.terminal_font.font_family = Some(RENAMED_FAMILY.to_owned());
    h.settings.appearance.terminal_font.custom_font_path = Some(path);
    h.surface_settled(TERMINAL);
    h.settings.appearance.terminal_font.custom_font_path = Some(String::new());
    assert!(
        h.surface_settled(TERMINAL)
            .iter()
            .any(|t| t == family_text())
    );
}

/// 콤보 목록이 열린 동안은 입력의 포커스와 같다. 닫으면 다시 보인다.
#[test]
fn an_open_family_list_hides_the_ink() {
    let mut h = Harness::new();
    h.settings.appearance.default_font.font_family = "Tasty No Such Family".to_owned();
    let closed = h.base_settled();
    assert!(closed.has(family_text()));

    let pos = closed.family.rect.center();
    h.click_base(pos);
    let open = h.base(Vec::new());
    assert!(!open.has(family_text()));
    assert_eq!(open.preview_top, closed.preview_top);
}
