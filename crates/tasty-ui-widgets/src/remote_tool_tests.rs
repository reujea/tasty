use super::*;

fn latte() -> Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    Theme::with_colors(colors, is_light.unwrap_or(true))
}

fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// `draw`를 두 번 그린 뒤(글꼴·글리프 텍스처가 자리 잡게) 경고 배지 채움 안에 칠해진 글리프의 tint를 모은다.
fn badge_glyph_inks(th: &Theme, mut draw: impl FnMut(&mut egui::Ui)) -> Vec<egui::Color32> {
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 글리프가 칠해지지 않는다.
    egui_extras::install_image_loaders(&ctx);
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        };
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| draw(ui));
        });
        shapes.clear();
        for clipped in out.shapes {
            flatten(&clipped.shape, &mut shapes);
        }
    }
    let badge_fill = th
        .accent_warning()
        .to_egui()
        .gamma_multiply(th.tint_fill_alpha());
    let badges: Vec<egui::Rect> = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_none() && r.fill == badge_fill => Some(r.rect),
            _ => None,
        })
        .collect();
    assert_eq!(badges.len(), 1, "one warning badge is painted");
    shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.brush.is_some() && badges[0].contains_rect(r.rect) => {
                Some(r.fill)
            }
            _ => None,
        })
        .collect()
}

/// 경고 배지 글리프는 base 행에서 accent-warning-glyph, raised 필터 메뉴에서 accent-warning-raised다.
/// Mocha는 두 값이 같아 Latte에서 갈리는지 먼저 본다.
#[test]
fn warn_badge_glyph_ink_follows_its_host() {
    let latte = latte();
    assert_ne!(
        latte.accent_warning_glyph(),
        latte.accent_warning_raised(),
        "Latte must separate the two glyph inks"
    );
    for th in [tasty_themes::mocha_fallback(), latte] {
        assert_eq!(
            warn_badge_glyph_ink(&th, false),
            th.accent_warning_glyph(),
            "base row glyph"
        );
        assert_eq!(
            warn_badge_glyph_ink(&th, true),
            th.accent_warning_raised(),
            "raised menu glyph"
        );
    }
}

/// 실제로 그린 목록 행 칩과 프로토콜 필터 메뉴의 배지 글리프 색. 호출부가 넘기는 바탕과 그리기가 그 잉크를
/// 쓰는지 함께 본다.
#[test]
fn painted_warn_badge_glyphs_read_their_host_ink() {
    for th in [tasty_themes::mocha_fallback(), latte()] {
        let row = badge_glyph_inks(&th, |ui| {
            remote_row_title(
                ui,
                &th,
                "legacy-box",
                None,
                false,
                &[RemoteRowChip::Warn {
                    text: "passkey missing",
                    tooltip: "hint",
                }],
                400.0,
            );
        });
        assert_eq!(row, vec![th.accent_warning_glyph().to_egui()], "base row");

        let labels = ProtocolFilterLabels {
            title: "Filter by protocol",
            select_all: "Select all",
            deselect_all: "Deselect all",
            reset: "Reset",
            apply: "Apply",
            unknown: "unknown",
            unknown_hint: "hint",
        };
        let items = [
            ProtocolFilterItem {
                name: "ssh",
                unknown: false,
            },
            ProtocolFilterItem {
                name: "smb",
                unknown: true,
            },
        ];
        let mut draft = HashSet::new();
        let menu = badge_glyph_inks(&th, |ui| {
            draw_protocol_filter_body(ui, &th, &items, &labels, &mut draft);
        });
        assert_eq!(
            menu,
            vec![th.accent_warning_raised().to_egui()],
            "raised filter menu"
        );
    }
}

/// SSH 설정 구역의 머리 줄과 빈 상태 줄은 위로 정본 `remote-ssh-line-pad-top` 여백을 둔다. 지원 배율에서 옛 값
/// (STRUCT_GAP_2)과 같아 그린 위치로는 옛 상수와 가를 수 없으므로 두 자리가 읽는 토큰 이름을 고정한다. 그린 위치는
/// 아래 시험이 잰다.
#[test]
fn the_ssh_header_and_empty_lines_read_remote_ssh_line_pad_top() {
    let source = include_str!("remote_tool.rs");
    assert_eq!(
        source
            .matches("vspace(ui, th.remote_ssh_line_pad_top());")
            .count(),
        2,
        "머리 줄과 빈 상태 줄의 위 여백이 remote_ssh_line_pad_top 을 읽지 않는다"
    );
}

/// SSH 설정 구역을 `zoom` 배율로 두 번 그린 뒤 (구분선 y, 머리 줄 글자 위, 빈 상태 줄 글자 위, 머리 블록이 커서를
/// 내리는 높이, 머리 블록 위에서 글자 위까지)를 돌려준다. 뒤 둘은 같은 머리 블록(안쪽 여백 + 가로 줄 + 두 라벨)을
/// 커서 위치를 재며 한 번 더 그려 얻는다.
fn ssh_line_positions(th: &Theme) -> (f32, f32, f32, f32, f32) {
    let ctx = egui::Context::default();
    let mut shapes = Vec::new();
    let mut block = (0.0, 0.0);
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        };
        let out = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let data = LocalSshSectionData {
                    heading: "From ssh config",
                    path: "~/.ssh/config",
                    in_profiles_tag: "In profiles",
                    add_label: "Add profile",
                    empty_message: "No hosts found",
                    hosts: &[],
                };
                draw_local_ssh_section(ui, th, &data);
                let top = ui.cursor().top();
                ssh_inset(ui, th, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = th.spacing_sm.value();
                        selectable_label_tracked(
                            ui,
                            "REFERENCE",
                            th.text_secondary(),
                            th.font_size_caption,
                            false,
                            th.letter_spacing_caps(th.font_size_caption),
                        );
                        selectable_label(
                            ui,
                            "reference",
                            th.text_muted(),
                            th.font_size_caption,
                            true,
                        );
                    });
                });
                block = (top, ui.cursor().top());
            });
        });
        shapes.clear();
        for clipped in out.shapes {
            flatten(&clipped.shape, &mut shapes);
        }
    }
    let text_top = |needle: &str| {
        shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.job.text == needle => Some(t.pos.y),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{needle:?} was not painted"))
    };
    let divider = shapes
        .iter()
        .find_map(|s| match s {
            egui::Shape::LineSegment { points, stroke }
                if points[0].y == points[1].y && stroke.color == th.border_frame().to_egui() =>
            {
                Some(points[0].y)
            }
            _ => None,
        })
        .expect("the section divider is painted");
    (
        divider,
        text_top("FROM SSH CONFIG"),
        text_top("No hosts found"),
        block.1 - block.0,
        text_top("REFERENCE") - block.0,
    )
}

/// SSH 설정 구역의 머리 줄은 구분선 아래 `spacing_sm` 다음에, 빈 상태 줄은 머리 블록 아래 `spacing_xs` 다음에 각각
/// 위 여백 `remote_ssh_line_pad_top` 을 더 두고 그려진다(배율 0.85·1·1.2).
#[test]
fn the_ssh_header_and_empty_lines_sit_remote_ssh_line_pad_top_lower() {
    for zoom in [0.85_f32, 1.0, 1.2] {
        let th = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, zoom);
        let (divider, header, empty, block, inset) = ssh_line_positions(&th);
        let pad = th.remote_ssh_line_pad_top().value();
        let header_expected = divider + th.spacing_sm.value() + pad + inset;
        assert!(
            (header - header_expected).abs() < 0.01,
            "배율 {zoom}: 머리 줄 글자 위 {header} 가 구분선 {divider} + spacing_sm + remote_ssh_line_pad_top {pad} \
             + 안쪽 {inset} = {header_expected} 이 아니다"
        );
        let empty_expected = header + block + th.spacing_xs.value() + pad;
        assert!(
            (empty - empty_expected).abs() < 0.01,
            "배율 {zoom}: 빈 상태 줄 글자 위 {empty} 가 머리 줄 {header} + 머리 블록 {block} + spacing_xs \
             + remote_ssh_line_pad_top {pad} = {empty_expected} 이 아니다"
        );
    }
}
