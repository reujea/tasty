//! 내보내기 실패 블록의 잉크와 버튼 바탕을 그려진 화면에서 확인한다.

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

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// Export 행(raised 카드) 안에 미분류 실패 블록을 그린 뒤 도형을 모은다.
fn draw_export_failure(th: &Theme) -> Vec<egui::Shape> {
    let failure = ExportFailure {
        path: PathBuf::from("/tmp/tasty-keybindings.toml"),
        reason: ExportFailReason::Unknown("os error 28: No space left on device".to_string()),
    };
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(720.0, 480.0));
    let mut output = None;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        output = Some(ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut notice = |ui: &mut egui::Ui| {
                    notices::export_failure(ui, th, &failure);
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
            });
        }));
    }
    let mut shapes = Vec::new();
    for clipped in output.expect("drawn").shapes {
        flatten(&clipped.shape, &mut shapes);
    }
    shapes
}

fn texts(shapes: &[egui::Shape]) -> Vec<PaintedText> {
    shapes
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
        .collect()
}

fn find<'a>(texts: &'a [PaintedText], needle: &str) -> &'a PaintedText {
    texts
        .iter()
        .find(|t| t.text.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} was not painted"))
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

/// 그려진 블록의 제목은 strong, 본문과 OS 줄은 text-on-active다. 버튼은 tint 아래 raised 카드에
/// 놓이고 ghost 라벨은 raised 바탕의 잉크다. Mocha는 panel·raised ghost 잉크가 같은 값이라
/// 바탕의 차이는 두 값이 갈리는 Latte가 가른다.
#[test]
fn export_failure_paints_canon_inks_and_puts_actions_on_the_raised_card() {
    let latte = latte();
    assert_ne!(
        latte.button_ghost_raised_fg(),
        latte.button_ghost_fg(),
        "Latte must separate the raised ghost ink from the panel one"
    );
    for th in themes() {
        assert_ne!(
            th.text_on_active(),
            th.text_secondary(),
            "body ink must be distinguishable from the old one"
        );
        let shapes = draw_export_failure(&th);
        let painted = texts(&shapes);

        let title = find(&painted, t("settings.keybindings.ie_export_failure_title"));
        assert_eq!(title.color, th.accent_danger_strong().to_egui(), "title");
        let body = find(&painted, "/tmp/tasty-keybindings.toml");
        assert_eq!(body.color, th.text_on_active().to_egui(), "body");
        let os = find(&painted, "os error 28");
        assert_eq!(os.color, th.text_on_active().to_egui(), "OS line");

        let tint_fill = th
            .accent_danger()
            .to_egui()
            .gamma_multiply(th.tint_fill_alpha());
        let tint = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if r.fill == tint_fill => Some(r.rect),
                _ => None,
            })
            .expect("danger tint painted");
        assert!(tint.contains_rect(title.rect), "title inside the tint");

        let retry = find(&painted, t("settings.keybindings.ie_export_retry"));
        let other = find(&painted, t("settings.keybindings.ie_export_choose_another"));
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
            other.color,
            th.button_ghost_raised_fg().to_egui(),
            "ghost label on the raised card"
        );
    }
}
