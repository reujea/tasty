use super::*;

fn theme() -> Theme {
    Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0)
}

/// 폭 `width` 인 Ui 에 `draw` 를 그리고 차지한 사각형을 돌려준다.
fn drawn_rect(width: f32, mut draw: impl FnMut(&mut egui::Ui, &Theme)) -> egui::Rect {
    let theme = theme();
    let ctx = egui::Context::default();
    let mut out = None;
    for _ in 0..2 {
        drop(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let inner = ui.allocate_ui(egui::vec2(width, f32::INFINITY), |ui| {
                    ui.set_max_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    draw(ui, &theme);
                });
                out = Some(inner.response.rect);
            });
        }));
    }
    out.expect("drawn")
}

#[test]
fn the_action_bar_is_as_tall_as_its_height_function() {
    let theme = theme();
    let rect = drawn_rect(540.0, |ui, theme| {
        plugin_detail_bar(
            ui,
            theme,
            &PluginDetailBarView {
                enabled: true,
                enabled_label: "Enabled",
                disabled_label: "Disabled",
                configure: "Configure",
                uninstall: "Uninstall",
            },
        );
    });
    assert_eq!(rect.height(), plugin_detail_bar_height(&theme));
}

#[test]
fn a_command_row_is_one_settings_row_tall_including_its_bottom_line() {
    let theme = theme();
    for keys in [Some("Ctrl+Shift+V"), None] {
        let rect = drawn_rect(380.0, |ui, theme| {
            plugin_command_row(
                ui,
                theme,
                "Open clipboard viewer",
                keys,
                &PluginKeycapStyle::default(),
            );
        });
        assert_eq!(
            rect.height(),
            theme.settings_row_min_height().value(),
            "keys {keys:?}"
        );
    }
}

#[test]
fn a_long_command_title_stays_inside_the_column() {
    let rect = drawn_rect(200.0, |ui, theme| {
        plugin_command_row(
            ui,
            theme,
            "a very long command title that cannot fit in the detail column at all",
            Some("Ctrl+Alt+G"),
            &PluginKeycapStyle::default(),
        );
    });
    assert!(rect.width() <= 200.0 + 0.5, "width {}", rect.width());
}

/// 그린 글자와 그 사각형. 버튼 라벨 위치로 바 안 배치를 잰다.
fn text_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Rect)> {
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            egui::Shape::Text(t) => out.push((
                t.galley.text().to_string(),
                t.galley.rect.translate(t.pos.to_vec2()),
            )),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for c in shapes {
        walk(&c.shape, &mut out);
    }
    out
}

#[test]
fn the_action_buttons_end_at_the_bar_inset_and_follow_screen_order() {
    let theme = theme();
    let ctx = egui::Context::default();
    let width = 540.0;
    let mut bar_rect = egui::Rect::NOTHING;
    let mut output = None;
    for _ in 0..2 {
        output = Some(ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    let inner = ui.allocate_ui(egui::vec2(width, f32::INFINITY), |ui| {
                        ui.set_max_width(width);
                        plugin_detail_bar(
                            ui,
                            &theme,
                            &PluginDetailBarView {
                                enabled: true,
                                enabled_label: "Enabled",
                                disabled_label: "Disabled",
                                configure: "Configure",
                                uninstall: "Uninstall",
                            },
                        );
                    });
                    bar_rect = inner.response.rect;
                });
        }));
    }
    let texts = text_rects(&output.expect("drawn").shapes);
    let find = |label: &str| {
        texts
            .iter()
            .find(|(t, _)| t == label)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{label} not drawn"))
    };
    let enabled = find("Enabled");
    let configure = find("Configure");
    let uninstall = find("Uninstall");
    assert!(enabled.right() < configure.left());
    assert!(configure.right() < uninstall.left());
    let pad_x = ControlSize::Md.pad_x(&theme);
    // 응답 rect 는 내용 폭으로 줄어들 수 있어 바에 준 폭을 기준으로 잰다.
    let expected_right = bar_rect.left() + width - PLUGIN_ADD_INSET.value();
    assert!(
        (uninstall.right() + pad_x - expected_right).abs() <= 1.0,
        "uninstall button ends at {} but the bar inset is at {expected_right}",
        uninstall.right() + pad_x
    );
}

#[test]
fn the_description_wraps_within_measure_lg_in_a_wide_column() {
    let theme = theme();
    let long =
        "A plugin description that is much longer than one line of the detail column. ".repeat(4);
    let rect = drawn_rect(900.0, |ui, theme| {
        plugin_detail_description(ui, theme, &long);
    });
    assert!(
        rect.width() <= theme.measure_lg.value() + 0.5,
        "description is {} wide",
        rect.width()
    );
    assert!(
        rect.height() > theme.font_size_body.value() * 2.0,
        "it wraps"
    );
}

/// 제거 확인 바는 높이 함수가 말한 만큼 차지한다. 짧은 문구는 평소 바와 같은 높이이고, 줄바꿈하는
/// 긴 문구에서는 글 열만큼 커진다.
#[test]
fn the_confirm_bar_is_as_tall_as_its_height_function() {
    let theme = theme();
    let short = PluginUninstallConfirmView {
        title: "Uninstall git?",
        note: "Its files are removed.",
        cancel: "Cancel",
        uninstall: "Uninstall",
        focus_cancel: false,
        busy: None,
        wrap: false,
    };
    let long_note = "Its files are removed. Settings stay until you delete them. ".repeat(4);
    let long = PluginUninstallConfirmView {
        note: &long_note,
        ..short
    };
    for (view, grows) in [(&short, false), (&long, true)] {
        for width in [380.0, 540.0] {
            let mut expected = 0.0;
            let rect = drawn_rect(width, |ui, theme| {
                expected = plugin_uninstall_confirm_bar_height(ui, theme, view, width);
                plugin_uninstall_confirm_bar(ui, theme, view);
            });
            assert_eq!(rect.height(), expected, "width {width}");
            if grows {
                assert!(expected > plugin_detail_bar_height(&theme), "width {width}");
            } else {
                assert_eq!(expected, plugin_detail_bar_height(&theme), "width {width}");
            }
            assert!(
                rect.width() <= width,
                "the bar overflows {width}: {}",
                rect.width()
            );
        }
    }
}

#[test]
fn keycaps_are_split_trimmed_and_title_cased() {
    for (chord, caps) in [
        ("ctrl + shift + h", "Ctrl+Shift+H"),
        ("Ctrl+Alt+G", "Ctrl+Alt+G"),
        ("ALT+pageup", "Alt+Pageup"),
        ("ctrl++", "Ctrl++"),
        ("  ", ""),
    ] {
        assert_eq!(plugin_keycaps(chord), caps, "{chord:?}");
    }
}

#[test]
fn the_homepage_link_drops_only_the_web_scheme() {
    assert_eq!(
        homepage_display("https://github.com/zilhak/tasty"),
        "github.com/zilhak/tasty"
    );
    assert_eq!(homepage_display("http://example.com"), "example.com");
    assert_eq!(homepage_display("HTTPS://Example.com/A"), "Example.com/A");
    assert_eq!(homepage_display("Http://example.com"), "example.com");
    // 접두가 멀티바이트 문자 경계에 걸려도 그대로 돌려준다.
    assert_eq!(homepage_display("https:/é"), "https:/é");
    assert_eq!(homepage_display("example.com/x"), "example.com/x");
}

/// Attention 바는 Installed 바와 같은 틀이라 높이도 같다. 버튼이 있든 없든 같다.
#[test]
fn the_attention_bar_shares_the_installed_bar_height() {
    let theme = theme();
    let color = theme.accent_warning().to_egui();
    for action in [
        None,
        Some(PluginAttentionBarAction::Reapprove("Re-approve")),
        Some(PluginAttentionBarAction::Configure("Configure")),
    ] {
        let rect = drawn_rect(540.0, |ui, theme| {
            plugin_attention_bar(
                ui,
                theme,
                &PluginAttentionBarView {
                    status: "Needs review",
                    dot: color,
                    ink: color,
                    action,
                },
            );
        });
        assert_eq!(rect.height(), plugin_detail_bar_height(&theme));
    }
}

#[test]
fn only_http_and_https_homepages_are_links() {
    for url in [
        "https://github.com/zilhak/tasty",
        "http://example.com",
        "HTTPS://Example.com",
    ] {
        assert!(is_web_homepage(url), "{url:?}");
    }
    for url in [
        "",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "ftp://example.com",
        "example.com",
        "https://",
        "mailto:a@b.c",
    ] {
        assert!(!is_web_homepage(url), "{url:?}");
    }
}

/// `draw` 를 그리고, `text` 로 그려진 글자 위를 누르고 뗀다. 그 사이 `draw` 가 true 를 돌려준 적이
/// 있으면 true. 글자가 밑줄을 가졌는지도 함께 돌려준다.
pub(crate) fn click_text(
    text: &str,
    mut draw: impl FnMut(&mut egui::Ui, &Theme) -> bool,
) -> (bool, bool) {
    let theme = theme();
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0));
    let mut clicked = false;
    let mut target = None;
    let mut underlined = false;
    let mut frame = |events: Vec<egui::Event>, clicked: &mut bool| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| *clicked |= draw(ui, &theme));
            },
        )
    };
    for _ in 0..2 {
        let out = frame(Vec::new(), &mut clicked);
        for c in &out.shapes {
            if let egui::Shape::Text(t) = &c.shape
                && t.galley.text() == text
            {
                target = Some(t.galley.rect.translate(t.pos.to_vec2()).center());
                underlined = t
                    .galley
                    .job
                    .sections
                    .iter()
                    .any(|s| s.format.underline.width > 0.0);
            }
        }
    }
    let pos = target.unwrap_or_else(|| panic!("{text:?} was not drawn"));
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    drop(frame(vec![egui::Event::PointerMoved(pos)], &mut clicked));
    drop(frame(vec![button(true)], &mut clicked));
    drop(frame(vec![button(false)], &mut clicked));
    drop(frame(Vec::new(), &mut clicked));
    (clicked, underlined)
}

/// 메타 줄은 웹 주소만 누를 수 있는 밑줄 링크로 그리고, 다른 scheme 은 눌러도 열리지 않는 평문이다.
#[test]
fn the_meta_line_links_only_web_homepages() {
    let meta = |homepage: &'static str| {
        move |ui: &mut egui::Ui, theme: &Theme| {
            plugin_detail_meta(
                ui,
                theme,
                &PluginMetaView {
                    authors: "ann",
                    id: "com.a.b",
                    homepage,
                    wrap_long: false,
                },
            )
        }
    };
    assert_eq!(
        click_text("example.com/x", meta("https://example.com/x")),
        (true, true)
    );
    assert_eq!(
        click_text("javascript:alert(1)", meta("javascript:alert(1)")),
        (false, false)
    );
}

/// 폭 `width` 에 메타 줄을 그리고 글자 조각마다 (글자, 사각형, 줄 수) 를 그린 순서대로 돌려준다.
fn meta_texts(width: f32, view: &PluginMetaView<'_>) -> Vec<(String, egui::Rect, usize)> {
    let theme = theme();
    let ctx = egui::Context::default();
    let mut out = Vec::new();
    for _ in 0..2 {
        let full = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.allocate_ui(egui::vec2(width, f32::INFINITY), |ui| {
                    ui.set_max_width(width);
                    plugin_detail_meta(ui, &theme, view);
                });
            });
        });
        out.clear();
        for c in &full.shapes {
            if let egui::Shape::Text(t) = &c.shape {
                out.push((
                    t.galley.text().to_string(),
                    t.galley.rect.translate(t.pos.to_vec2()),
                    t.galley.rows.len(),
                ));
            }
        }
    }
    out
}

/// 줄이 넘쳐도 구분점은 뒤 항목과 같은 줄에 있고, 항목은 한 줄로 그린다.
#[test]
fn a_meta_separator_wraps_with_the_item_after_it() {
    let view = PluginMetaView {
        authors: "tasty-labs",
        id: "com.example.plugin-meta-wrap",
        homepage: "https://example.com/plugin",
        wrap_long: false,
    };
    let texts = meta_texts(220.0, &view);
    let dots: Vec<usize> = (0..texts.len()).filter(|&i| texts[i].0 == "·").collect();
    assert_eq!(dots.len(), 2, "{texts:?}");
    for &i in &dots {
        let (_, dot, _) = &texts[i];
        let (item, rect, _) = texts.get(i + 1).expect("item after the dot");
        assert!(
            (dot.top() - rect.top()).abs() < 0.5 && rect.left() > dot.right(),
            "구분점이 뒤 항목 {item:?} 과 다른 줄에 있다: {texts:?}"
        );
    }
    for (text, _, rows) in &texts {
        assert_eq!(*rows, 1, "{text:?} 가 여러 줄로 갈렸다: {texts:?}");
    }
    let lines: std::collections::BTreeSet<i32> = texts
        .iter()
        .map(|(_, r, _)| r.top().round() as i32)
        .collect();
    assert!(
        lines.len() >= 2,
        "줄이 넘치지 않아 검사가 의미 없다: {texts:?}"
    );
}

/// 같은 homepage 를 가진 메타 줄 둘을 한 프레임에 그려도 링크 id 가 겹치지 않는다. 겹치면 egui 가
/// debug 빌드에서 경고 글을 그린다.
#[test]
fn two_meta_lines_with_one_homepage_do_not_share_a_link_id() {
    let theme = theme();
    let view = PluginMetaView {
        authors: "tasty",
        id: "com.tasty.git-viewer",
        homepage: "https://github.com/zilhak/tasty",
        wrap_long: false,
    };
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..2 {
        let full = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                plugin_detail_meta(ui, &theme, &view);
                plugin_detail_meta(ui, &theme, &view);
            });
        });
        texts = text_rects(&full.shapes)
            .into_iter()
            .map(|(t, _)| t)
            .collect::<Vec<_>>();
    }
    assert_eq!(
        texts
            .iter()
            .filter(|t| *t == "github.com/zilhak/tasty")
            .count(),
        2,
        "{texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t.contains("widget ID")),
        "link ids clash: {texts:?}"
    );
}

/// 바와 위 구분선은 버튼이 없어도 열 폭 전체를 쓴다. 서명 사유처럼 버튼이 없는 바가 내용 폭으로
/// 줄면 구분선이 열 중간에서 끊긴다.
#[test]
fn the_attention_bar_spans_the_column_without_a_button() {
    let theme = theme();
    let color = theme.accent_danger().to_egui();
    for action in [
        None,
        Some(PluginAttentionBarAction::Reapprove("Re-approve")),
    ] {
        let rect = drawn_rect(540.0, |ui, theme| {
            plugin_attention_bar(
                ui,
                theme,
                &PluginAttentionBarView {
                    status: "Not registered",
                    dot: color,
                    ink: color,
                    action,
                },
            );
        });
        assert_eq!(rect.width(), 540.0, "action {}", action.is_some());
    }
}

fn caps(chord: &str, style: &PluginKeycapStyle) -> Vec<PluginKeycap> {
    plugin_keycap_parts(chord, style)
}

fn text(t: &str) -> PluginKeycap {
    PluginKeycap::Text(t.to_owned())
}

/// Windows·Linux 는 매니페스트 순서 그대로 낱말 키캡이다. 표시 스타일 값이 있어도 따르지 않는다.
#[test]
fn other_platforms_keep_word_keycaps_in_manifest_order() {
    let symbols = PluginKeycapStyle::from_setting_names(false, "symbol", "symbol", "symbol");
    for style in [PluginKeycapStyle::default(), symbols] {
        assert_eq!(
            caps("ctrl + shift + h", &style),
            [text("Ctrl"), text("Shift"), text("H")]
        );
        assert_eq!(caps("alt + k", &style), [text("Alt"), text("K")]);
        assert_eq!(
            caps("shift+ctrl+h", &style),
            [text("Shift"), text("Ctrl"), text("H")]
        );
    }
}

/// macOS 는 수식키를 ⌃ ⌥ ⇧ ⌘ 순서로 놓고 키를 맨 뒤에 둔다. 매니페스트 순서와 대소문자는 상관없다.
#[test]
fn macos_sorts_modifiers_in_apple_order() {
    let style = PluginKeycapStyle::from_setting_names(true, "alt", "option", "shift");
    assert_eq!(
        caps("Shift + ALT + k + ctrl + option", &style),
        [
            text("Ctrl"),
            text("Option"),
            text("Shift"),
            text("Alt"),
            text("K")
        ]
    );
    assert_eq!(caps("ctrl++", &style), [text("Ctrl"), text("+")]);
}

/// macOS 수식키는 사용자 표시 스타일을 따른다. Ctrl 은 어느 스타일에서도 낱말이다.
#[test]
fn macos_modifiers_follow_the_display_style_and_ctrl_stays_a_word() {
    let words = PluginKeycapStyle::from_setting_names(true, "cmd", "option", "shift");
    assert_eq!(
        caps("ctrl + shift + h", &words),
        [text("Ctrl"), text("Shift"), text("H")]
    );
    assert_eq!(caps("alt + k", &words), [text("Cmd"), text("K")]);

    let symbols = PluginKeycapStyle::from_setting_names(true, "symbol", "symbol", "symbol");
    assert_eq!(
        caps("ctrl + shift + h", &symbols),
        [
            text("Ctrl"),
            PluginKeycap::Icon(tasty_icons::SHIFT_KEY),
            text("H")
        ]
    );
    assert_eq!(
        caps("option + alt + k", &symbols),
        [
            PluginKeycap::Icon(tasty_icons::OPTION_KEY),
            PluginKeycap::Icon(tasty_icons::CMD_KEY),
            text("K")
        ]
    );
}

/// 모르는 설정 값은 키바인딩 표시처럼 낱말로 본다.
#[test]
fn unknown_style_names_fall_back_to_words() {
    let style = PluginKeycapStyle::from_setting_names(true, "bogus", "bogus", "");
    assert_eq!(style.alt, PluginKeycapAltStyle::Alt);
    assert_eq!(style.option, PluginKeycapWordStyle::Word);
    assert_eq!(style.shift, PluginKeycapWordStyle::Word);
}

/// 행이 그리는 키캡도 스타일을 따른다. macOS 기호 표기에서 낱말 `Cmd` 대신 아이콘 키캡이 그려진다.
#[test]
fn the_command_row_draws_the_styled_keycaps() {
    let theme = theme();
    let draw = |style: PluginKeycapStyle| {
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        for _ in 0..2 {
            let full = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    plugin_command_row(ui, &theme, "Open", Some("alt + k"), &style);
                });
            });
            texts = text_rects(&full.shapes)
                .into_iter()
                .map(|(t, _)| t)
                .collect::<Vec<_>>();
        }
        texts
    };
    let words = draw(PluginKeycapStyle::from_setting_names(
        true, "cmd", "option", "shift",
    ));
    assert!(words.iter().any(|t| t == "Cmd"), "{words:?}");
    let symbols = draw(PluginKeycapStyle::from_setting_names(
        true, "symbol", "option", "shift",
    ));
    assert!(
        !symbols.iter().any(|t| t == "Cmd" || t == "Alt"),
        "{symbols:?}"
    );
    assert!(symbols.iter().any(|t| t == "K"), "{symbols:?}");
    let other = draw(PluginKeycapStyle::default());
    assert!(other.iter().any(|t| t == "Alt"), "{other:?}");
}

fn reserved_confirm<'a>(title: &'a str, busy: Option<&'a str>) -> PluginUninstallConfirmView<'a> {
    PluginUninstallConfirmView {
        title,
        note: "Its folder is deleted from your plugins folder.",
        cancel: "Cancel",
        uninstall: "Remove",
        focus_cancel: false,
        busy,
        wrap: true,
    }
}

/// 넓으면 한 줄이고, 좁으면 버튼 묶음이 다음 줄로 내려가며 높이 함수도 같은 배치로 잰다.
#[test]
fn a_wrapping_confirm_bar_moves_its_buttons_down_when_narrow() {
    let theme = theme();
    let long = "Remove tasty-official-agent-stream-v2-preview-build?";
    for (title, width, grows) in [
        ("Remove x?", 900.0, false),
        ("Remove x?", 320.0, true),
        (long, 900.0, false),
        (long, 320.0, true),
    ] {
        let view = reserved_confirm(title, None);
        let mut expected = 0.0;
        let rect = drawn_rect(width, |ui, theme| {
            expected = plugin_uninstall_confirm_bar_height(ui, theme, &view, width);
            plugin_uninstall_confirm_bar(ui, theme, &view);
        });
        assert_eq!(rect.height(), expected, "{title} at {width}");
        assert_eq!(
            expected > plugin_detail_bar_height(&theme),
            grows,
            "{title} at {width}: {expected}"
        );
    }
}

/// 제거 중 표시는 글 열 폭에 들어간다. 넓은 바도 줄바꿈 없이 같은 높이를 유지한다.
#[test]
fn the_removing_state_keeps_the_height_function_honest() {
    for (busy, width) in [(Some("Removing…"), 900.0), (Some("Removing…"), 320.0)] {
        let view = reserved_confirm("Remove x?", busy);
        let mut expected = 0.0;
        let rect = drawn_rect(width, |ui, theme| {
            expected = plugin_uninstall_confirm_bar_height(ui, theme, &view, width);
            plugin_uninstall_confirm_bar(ui, theme, &view);
        });
        assert_eq!(rect.height(), expected, "at {width}");
    }
}

/// 제거 중에는 두 버튼이 눌리지 않는다.
#[test]
fn the_removing_state_disables_both_buttons() {
    let theme = theme();
    let view = reserved_confirm("Remove x?", Some("Removing…"));
    let ctx = egui::Context::default();
    let bar = std::cell::Cell::new(egui::Rect::NOTHING);
    let run = |events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 200.0),
            )),
            events,
            ..Default::default()
        };
        let mut clicks = PluginUninstallConfirmClicks::default();
        drop(ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                clicks = plugin_uninstall_confirm_bar(ui, &theme, &view);
                bar.set(ui.min_rect());
            });
        }));
        clicks
    };
    run(Vec::new());
    let bar = bar.get();
    let inset = theme.spacing_md.value() + 4.0;
    // 오른쪽 끝 버튼(Remove)의 안쪽을 누른다.
    let at = egui::pos2(bar.max.x - inset - 8.0, bar.center().y);
    let press = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    run(vec![egui::Event::PointerMoved(at)]);
    let down = run(vec![press(true)]);
    let up = run(vec![press(false)]);
    assert_eq!(down, PluginUninstallConfirmClicks::default());
    assert_eq!(up, PluginUninstallConfirmClicks::default());
}

/// 빈 버전은 `v` 태그를 만들지 않는다.
#[test]
fn an_empty_version_draws_no_tag() {
    let theme = theme();
    let texts = |version: &'static str| {
        let ctx = egui::Context::default();
        let mut shapes = 0;
        for _ in 0..2 {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    plugin_detail_name_row(ui, &theme, "folder", version, None);
                });
            });
            shapes = output.shapes.len();
        }
        shapes
    };
    assert!(texts("") < texts("1.0.0"));
}

/// 줄바꿈을 켠 메타 줄은 긴 id 를 잘라 내지 않고 여러 줄로 그린다. 끈 쪽은 한 줄이다.
#[test]
fn only_a_wrapping_meta_line_grows_for_a_long_id() {
    let id = "com.tasty.official.agent-stream.v2-preview.with.a.very.long.tail";
    let height = |wrap_long| {
        drawn_rect(120.0, |ui, theme| {
            plugin_detail_meta(
                ui,
                theme,
                &PluginMetaView {
                    authors: "",
                    id,
                    homepage: "",
                    wrap_long,
                },
            );
        })
        .height()
    };
    assert!(height(true) > height(false));
}
