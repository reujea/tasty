//! 내보내기 실패 블록과 가져오기 파싱 실패 블록의 잉크·버튼 바탕을 그려진 화면에서 확인한다.

use super::*;

fn latte() -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors(colors, is_light.unwrap_or(true))
}

fn themes() -> [Theme; 2] {
    [tasty_themes::mocha_fallback(), latte()]
}

/// 그려진 글자 하나 — 문자열, 위치, 실제로 칠해진 색.
struct PaintedText {
    text: String,
    rect: egui::Rect,
    color: egui::Color32,
}

/// 한 프레임에 그려진 것.
struct Painted {
    shapes: Vec<egui::Shape>,
    texts: Vec<PaintedText>,
    /// 텍스처로 칠한 글리프(위치, tint).
    glyphs: Vec<(egui::Rect, egui::Color32)>,
}

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

fn painted(output: egui::FullOutput) -> Painted {
    let mut shapes = Vec::new();
    for clipped in output.shapes {
        flatten(&clipped.shape, &mut shapes);
    }
    let texts = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Text(t) => {
                let section = t.galley.job.sections.first()?.format.color;
                let color =
                    t.override_text_color
                        .unwrap_or(if section == egui::Color32::PLACEHOLDER {
                            t.fallback_color
                        } else {
                            section
                        });
                Some(PaintedText {
                    text: t.galley.text().to_string(),
                    rect: t.visual_bounding_rect(),
                    color,
                })
            }
            _ => None,
        })
        .collect();
    let glyphs = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_some() => Some((r.rect, r.fill)),
            _ => None,
        })
        .collect();
    Painted {
        shapes,
        texts,
        glyphs,
    }
}

impl Painted {
    fn text(&self, needle: &str) -> &PaintedText {
        self.texts
            .iter()
            .find(|t| t.text.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} was not painted"))
    }

    /// `inner`를 품는 가장 작은 채움 사각형의 채움색.
    fn fill_under(&self, inner: egui::Rect) -> egui::Color32 {
        self.shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r)
                    if r.brush.is_none()
                        && r.fill != egui::Color32::TRANSPARENT
                        && r.rect.contains_rect(inner) =>
                {
                    Some((r.rect.area(), r.fill))
                }
                _ => None,
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, fill)| fill)
            .unwrap_or_else(|| panic!("nothing filled under {inner:?}"))
    }

    fn rect_filled_with(&self, fill: egui::Color32) -> egui::Rect {
        self.shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.brush.is_none() && r.fill == fill => Some(r.rect),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no rect filled with {fill:?}"))
    }
}

/// 같은 Context로 프레임을 이어 그린다. 글리프는 SVG 그림이라 로더를 설치한다.
struct Scene {
    ctx: egui::Context,
}

impl Scene {
    fn new() -> Self {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        Self { ctx }
    }

    fn frame(&self, events: Vec<egui::Event>, draw: &mut dyn FnMut(&mut egui::Ui)) -> Painted {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(720.0, 480.0),
            )),
            events,
            ..Default::default()
        };
        painted(self.ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui));
        }))
    }

    /// 글꼴과 글리프 텍스처가 자리 잡도록 두 번 그린다.
    fn settle(&self, draw: &mut dyn FnMut(&mut egui::Ui)) -> Painted {
        self.frame(Vec::new(), draw);
        self.frame(Vec::new(), draw)
    }

    fn focused_rect(&self) -> Option<egui::Rect> {
        let id = self.ctx.memory(|m| m.focused())?;
        self.ctx.read_response(id).map(|r| r.rect)
    }
}

fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn export_failure() -> ExportFailure {
    ExportFailure {
        path: PathBuf::from("/tmp/tasty-keybindings.toml"),
        reason: ExportFailReason::Unknown("os error 28: No space left on device".to_string()),
    }
}

/// Export 행(raised 카드) 안의 미분류 실패 블록.
fn export_row(th: &Theme, failure: &ExportFailure) -> impl FnMut(&mut egui::Ui) {
    move |ui| {
        let mut notice = |ui: &mut egui::Ui| {
            notices::export_failure(ui, th, failure);
        };
        entry::action_row(
            ui,
            th,
            icons::DOWNLOAD,
            "Export",
            "desc",
            "Export…",
            ButtonVariant::Secondary,
            false,
            None,
            Some(&mut notice),
        );
    }
}

fn danger_tint(th: &Theme) -> egui::Color32 {
    th.accent_danger()
        .to_egui()
        .gamma_multiply(th.tint_fill_alpha())
}

/// 제목은 accent-danger-strong, 글리프는 accent-danger-raised다. 두 값은 두 테마 모두에서 갈린다.
#[test]
fn export_failure_title_and_glyph_read_separate_inks() {
    for th in themes() {
        let ink = notices::export_failure_ink(&th);
        assert_eq!(ink.title, th.accent_danger_strong().to_egui(), "title ink");
        assert_eq!(ink.glyph, th.accent_danger_raised().to_egui(), "glyph ink");
        assert_eq!(ink.tint, th.accent_danger().to_egui(), "tint");
        assert_ne!(ink.title, ink.glyph, "the two inks must differ");
    }
}

/// 그려진 블록의 제목 글자는 strong, 제목 줄 글리프는 raised, 본문과 OS 줄은 text-on-active다.
/// 버튼은 tint 아래 raised 카드에 놓이고 Try again 채움은 surface-on-raised, ghost 라벨은 raised 바탕의 잉크다.
/// Mocha는 panel·raised ghost 잉크가 같은 값이라 바탕의 차이는 두 값이 갈리는 Latte가 가른다.
#[test]
fn export_failure_paints_canon_inks_and_puts_actions_on_the_raised_card() {
    let latte = latte();
    assert_ne!(
        latte.button_ghost_raised_fg(),
        latte.button_ghost_fg(),
        "Latte must separate the raised ghost ink from the panel one"
    );
    assert!(
        themes()
            .iter()
            .any(|th| th.surface_on_raised() != th.button_secondary_bg()),
        "one theme must separate the raised secondary fill from the panel one"
    );
    for th in themes() {
        assert_ne!(
            th.text_on_active(),
            th.text_secondary(),
            "body ink must be distinguishable from the old one"
        );
        let failure = export_failure();
        let scene = Scene::new();
        let p = scene.settle(&mut export_row(&th, &failure));

        let title = p.text(t("settings.keybindings.ie_export_failure_title"));
        assert_eq!(title.color, th.accent_danger_strong().to_egui(), "title");
        let body = p.text("/tmp/tasty-keybindings.toml");
        assert_eq!(body.color, th.text_on_active().to_egui(), "body");
        let os = p.text("os error 28");
        assert_eq!(os.color, th.text_on_active().to_egui(), "OS line");

        let tint = p.rect_filled_with(danger_tint(&th));
        assert!(tint.contains_rect(title.rect), "title inside the tint");
        let title_glyphs: Vec<egui::Color32> = p
            .glyphs
            .iter()
            .filter(|(r, _)| tint.contains_rect(*r))
            .map(|(_, c)| *c)
            .collect();
        assert_eq!(
            title_glyphs,
            vec![th.accent_danger_raised().to_egui()],
            "the one glyph in the tint is the title glyph in the raised ink"
        );

        let retry = p.text(t("settings.keybindings.ie_export_retry"));
        let other = p.text(t("settings.keybindings.ie_export_choose_another"));
        for button in [retry, other] {
            assert!(
                button.rect.top() >= tint.bottom(),
                "{:?} must sit below the tint ({:?} vs {:?})",
                button.text,
                button.rect,
                tint
            );
        }
        assert_eq!(
            p.fill_under(retry.rect),
            th.surface_on_raised().to_egui(),
            "Try again fill on the raised card"
        );
        assert_eq!(
            other.color,
            th.button_ghost_raised_fg().to_egui(),
            "ghost label on the raised card"
        );
    }
}

/// Choose another location… 를 포인터로 누르고 있는 동안과, 포커스를 두고 Space를 누르고 있는 동안
/// 모두 글자는 raised ghost 의 눌림 잉크(button-ghost-raised-fg-hover)다.
#[test]
fn export_failure_ghost_press_reads_the_raised_hover_ink_from_pointer_and_keyboard() {
    for th in themes() {
        assert_ne!(
            th.button_ghost_raised_fg_hover(),
            th.button_ghost_raised_fg(),
            "pressed ink must be distinguishable from the resting one"
        );
        let pressed_ink = th.button_ghost_raised_fg_hover().to_egui();
        let label = t("settings.keybindings.ie_export_choose_another");
        let failure = export_failure();

        // 포인터 누름.
        let scene = Scene::new();
        let mut draw = export_row(&th, &failure);
        let at = scene.settle(&mut draw).text(label).rect.center();
        scene.frame(vec![egui::Event::PointerMoved(at)], &mut draw);
        scene.frame(
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut draw,
        );
        let held = scene.frame(Vec::new(), &mut draw);
        assert_eq!(held.text(label).color, pressed_ink, "pointer press");

        // 키보드 누름: Tab으로 포커스를 옮긴 뒤 포인터 없이 Space를 누르고 있는다.
        let scene = Scene::new();
        let mut draw = export_row(&th, &failure);
        let target = scene.settle(&mut draw).text(label).rect;
        let mut focused = false;
        for _ in 0..8 {
            scene.frame(vec![key(egui::Key::Tab, true)], &mut draw);
            scene.frame(vec![key(egui::Key::Tab, false)], &mut draw);
            if scene
                .focused_rect()
                .is_some_and(|r| r.contains_rect(target))
            {
                focused = true;
                break;
            }
        }
        assert!(focused, "Tab must reach the ghost button");
        let resting = scene.frame(Vec::new(), &mut draw);
        assert_eq!(
            resting.text(label).color,
            th.button_ghost_raised_fg().to_egui(),
            "focused but not pressed"
        );
        scene.frame(vec![key(egui::Key::Space, true)], &mut draw);
        let held = scene.frame(Vec::new(), &mut draw);
        assert_eq!(held.text(label).color, pressed_ink, "keyboard press");
    }
}

/// 가져오기 파싱 실패 본문은 내보내기 실패와 달리 text-secondary 그대로다.
#[test]
fn parse_failure_body_stays_text_secondary() {
    for th in themes() {
        let failure = Failure {
            path: "/tmp/settings.json".to_string(),
            line: Some(1),
        };
        let scene = Scene::new();
        let p = scene.settle(&mut |ui: &mut egui::Ui| {
            notices::parse_failure(ui, &th, &failure);
        });
        assert_eq!(
            p.text("/tmp/settings.json").color,
            th.text_secondary().to_egui(),
            "parse failure body"
        );
    }
}
