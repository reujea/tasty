use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::*;

fn view() -> HtmlAddressView<'static> {
    HtmlAddressView {
        address: "https://example.com/current",
        committed_address: "https://example.com/current",
        kind: HtmlAddressKind::Web,
        can_back: false,
        can_forward: true,
        loading: false,
        disconnected: false,
        skipped: None,
        remote_marker: None,
        focus: false,
        labels: HtmlAddressLabels {
            placeholder: "Path or URL",
            back: "Back",
            forward: "Forward",
            reload: "Reload",
            disconnected: "Disconnected",
        },
    }
}
fn frame(
    ctx: &egui::Context,
    state: &mut HtmlAddressState,
    view: &HtmlAddressView<'_>,
    width: f32,
    events: Vec<egui::Event>,
) -> HtmlAddressOutput {
    let mut result = None;
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 300.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                result = Some(html_address_bar(
                    ui,
                    &tasty_themes::mocha_fallback(),
                    egui::Id::new("address"),
                    LogicalPx(width),
                    state,
                    view,
                ));
            });
        },
    ));
    result.unwrap()
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
fn click(
    ctx: &egui::Context,
    state: &mut HtmlAddressState,
    v: &HtmlAddressView<'_>,
    p: egui::Pos2,
) -> HtmlAddressOutput {
    frame(
        ctx,
        state,
        v,
        460.0,
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    frame(
        ctx,
        state,
        v,
        460.0,
        vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    )
}
#[test]
fn geometry_threshold_field_floor_and_error_row() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let v = view();
    for (width, forward) in [(440.0, true), (439.0, false), (150.0, false)] {
        let out = frame(&ctx, &mut s, &v, width, vec![]);
        assert_eq!(out.rect.height(), 40.0);
        assert_eq!(out.forward_rect.is_some(), forward);
        assert!(out.field_rect.unwrap().width() >= 96.0);
        if width == 150.0 {
            assert_eq!(out.field_rect.unwrap().width(), 96.0);
        }
        assert_eq!(out.field_rect.unwrap().height(), 28.0);
        assert_eq!(out.reload_rect.unwrap().width(), 24.0);
        assert_eq!(
            out.field_rect.unwrap().left() - out.reload_rect.unwrap().right(),
            8.0
        );
    }
    s.reject("bad".into());
    assert_eq!(frame(&ctx, &mut s, &v, 460.0, vec![]).rect.height(), 64.0);
}
#[test]
fn agent_commit_preserves_draft_then_escape_uses_latest_commit() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.focus = true;
    frame(&ctx, &mut s, &v, 460.0, vec![]);
    v.focus = false;
    frame(
        &ctx,
        &mut s,
        &v,
        460.0,
        vec![egui::Event::Text("draft".into())],
    );
    assert_eq!(s.buffer, "draft");
    v.address = "https://example.com/agent";
    v.committed_address = v.address;
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    assert!(out.has_keyboard_focus);
    assert_eq!(s.buffer, "draft");
    let out = frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Escape)]);
    assert_eq!(out.action, HtmlAddressAction::Revert);
    assert_eq!(s.buffer, v.address);
    assert!(!out.has_keyboard_focus);
}
#[test]
fn rejected_enter_keeps_input_and_focus_then_typing_clears_error() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.focus = true;
    frame(&ctx, &mut s, &v, 460.0, vec![]);
    v.focus = false;
    frame(
        &ctx,
        &mut s,
        &v,
        460.0,
        vec![egui::Event::Text("/missing".into())],
    );
    let out = frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Enter)]);
    assert_eq!(out.action, HtmlAddressAction::Open("/missing".into()));
    assert!(out.has_keyboard_focus);
    s.reject("missing".into());
    frame(&ctx, &mut s, &v, 460.0, vec![]);
    assert_eq!(s.buffer, "/missing");
    assert!(s.error.is_some());
    frame(&ctx, &mut s, &v, 460.0, vec![egui::Event::Text("x".into())]);
    assert_eq!(s.buffer, "/missingx");
    assert!(s.error.is_none());
}
#[test]
fn mirror_is_read_only_and_placeholder_has_no_band() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.kind = HtmlAddressKind::Mirror;
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    assert!(out.forward_rect.is_none());
    click(&ctx, &mut s, &v, out.field_rect.unwrap().center());
    let out = frame(
        &ctx,
        &mut s,
        &v,
        460.0,
        vec![egui::Event::Text("replace".into()), key(egui::Key::Enter)],
    );
    assert_eq!(s.buffer, v.address);
    assert_eq!(out.action, HtmlAddressAction::None);
    v.kind = HtmlAddressKind::MirrorPlaceholder;
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    assert_eq!(out.rect.height(), 0.0);
    assert!(out.field_rect.is_none());
    assert!(out.reload_rect.is_none());
}
#[test]
fn loading_reload_is_not_clickable() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.loading = true;
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    let p = out.reload_rect.unwrap().center();
    assert_eq!(click(&ctx, &mut s, &v, p).action, HtmlAddressAction::None);
    v.loading = false;
    frame(&ctx, &mut s, &v, 460.0, vec![]);
    assert_eq!(click(&ctx, &mut s, &v, p).action, HtmlAddressAction::Reload);
    v.disconnected = true;
    assert_eq!(click(&ctx, &mut s, &v, p).action, HtmlAddressAction::None);
}
#[test]
fn inset_stack_has_one_top_margin_and_gaps_between_and_after() {
    let theme = tasty_themes::mocha_fallback();
    assert_eq!(
        html_address_content_inset(&theme, LogicalPx(40.0), &[]),
        LogicalPx(40.0)
    );
    assert_eq!(
        html_address_content_inset(&theme, LogicalPx(64.0), &[LogicalPx(60.0), LogicalPx(80.0)]),
        LogicalPx(228.0)
    );
}

#[test]
fn failed_attempt_does_not_become_escape_revert_address() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.address = "https://offline.example";
    v.focus = true;
    frame(&ctx, &mut s, &v, 460.0, vec![]);
    v.focus = false;
    assert_eq!(s.buffer, v.address);
    let out = frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Escape)]);
    assert_eq!(out.action, HtmlAddressAction::Revert);
    assert_eq!(s.buffer, v.committed_address);
}

#[test]
fn input_length_overrides_preserve_existing_defaults() {
    use std::cell::RefCell;
    let ctx = egui::Context::default();
    let theme = tasty_themes::mocha_fallback();
    let widths = RefCell::new(Vec::new());
    let icon = |_: &mut egui::Ui, r: egui::Rect, _: egui::Color32| {
        widths.borrow_mut().push(r.width());
    };
    let output = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            Input::new()
                .mono(true)
                .icon(&icon)
                .show(ui, &theme, &mut "default-input".into());
            Input::new()
                .mono(true)
                .font_size(theme.input_font_size())
                .icon_size(theme.icon_glyph_size_sm)
                .icon(&icon)
                .show(ui, &theme, &mut "html-input".into());
        });
    });
    assert_eq!(
        *widths.borrow(),
        vec![
            theme.icon_glyph_size_md.value(),
            theme.icon_glyph_size_sm.value()
        ]
    );
    let sizes: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) if ["default-input", "html-input"].contains(&t.galley.text()) => {
                Some(t.galley.job.sections[0].format.font_id.size)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        sizes,
        vec![
            theme.font_size_caption.value(),
            theme.input_font_size().value()
        ]
    );
}

fn latte() -> tasty_type_appearance::theme::Theme {
    let file = tasty_themes::ThemeFile::parse(tasty_themes::LATTE_TOML_TEXT).expect("latte.toml");
    let (partial, is_light) = file.to_partial();
    let mut colors = tasty_themes::mocha_fallback_colors();
    colors.apply_partial(&partial);
    tasty_type_appearance::theme::Theme::with_colors(colors, is_light.unwrap_or(true))
}

/// The ink of the painted address text, read from the frame's text shapes.
fn address_ink(
    theme: &tasty_type_appearance::theme::Theme,
    state: &mut HtmlAddressState,
    v: &HtmlAddressView<'_>,
) -> egui::Color32 {
    let ctx = egui::Context::default();
    let mut found = None;
    for _ in 0..2 {
        let out = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 300.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    html_address_bar(
                        ui,
                        theme,
                        egui::Id::new("address"),
                        LogicalPx(800.0),
                        state,
                        v,
                    );
                });
            },
        );
        found = out.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == v.address => {
                Some(t.override_text_color.unwrap_or(t.fallback_color))
            }
            _ => None,
        });
    }
    found.expect("address text painted")
}

#[test]
fn idle_editing_and_mirror_values_use_their_own_ink_roles() {
    let theme = latte();
    // In Latte the idle value role differs from the old text-secondary ink.
    assert_ne!(theme.html_address_value_fg(), theme.text_secondary());
    let mut idle = HtmlAddressState::default();
    assert_eq!(
        address_ink(&theme, &mut idle, &view()),
        theme.html_address_value_fg().to_egui()
    );
    let mut editing = HtmlAddressState {
        buffer: "https://example.com/current".into(),
        editing: true,
        error: None,
    };
    assert_eq!(
        address_ink(&theme, &mut editing, &view()),
        theme.input_fg().to_egui()
    );
    let mirror = HtmlAddressView {
        kind: HtmlAddressKind::Mirror,
        ..view()
    };
    assert_eq!(
        address_ink(&theme, &mut HtmlAddressState::default(), &mirror),
        theme.input_readonly_fg().to_egui()
    );
}

/// 읽기 전용 mirror의 긴 주소는 말줄임 라벨이 아니라 원문 전체를 첫 포커스 프레임부터 복사한다.
/// 붙여넣기·Enter는 값을 바꾸거나 이동하지 않는다.
#[test]
fn a_long_mirror_address_copies_the_full_original_from_the_first_click() {
    let address = "https://example.com/문서/아주/긴/경로/with%20space/and/more/segments/report.html?q=한국어&x=1#section-끝";
    let mut v = view();
    v.kind = HtmlAddressKind::Mirror;
    v.address = address;
    v.committed_address = address;
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    let p = out.field_rect.unwrap().center();
    let copied = |events: Vec<egui::Event>, s: &mut HtmlAddressState| {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 300.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    html_address_bar(
                        ui,
                        &tasty_themes::mocha_fallback(),
                        egui::Id::new("address"),
                        LogicalPx(460.0),
                        s,
                        &v,
                    );
                });
            },
        );
        output
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
    };
    let press = |pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    assert_eq!(
        copied(vec![egui::Event::PointerMoved(p), press(true)], &mut s),
        None
    );
    let select_all = egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    assert_eq!(
        copied(vec![press(false), select_all, egui::Event::Copy], &mut s).as_deref(),
        Some(address)
    );
    let out = frame(
        &ctx,
        &mut s,
        &v,
        460.0,
        vec![
            egui::Event::Paste("https://other.example".into()),
            egui::Event::Cut,
            key(egui::Key::Enter),
        ],
    );
    assert_eq!(s.buffer, address);
    assert_eq!(out.action, HtmlAddressAction::None);
}
/// Tab과 Shift+Tab은 띠 안의 사용 가능한 버튼과 주소 입력만 돈다. 버튼에 포커스가 있는 동안에도 띠가
/// 키보드를 가지며 Enter·Space가 그 버튼을 누른다. 비활성 뒤로 버튼은 건너뛴다.
#[test]
fn tab_cycles_through_enabled_buttons_and_the_field() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    let field = html_address_field_id(egui::Id::new("address"));
    let focused = |ctx: &egui::Context| ctx.memory(|m| m.focused());
    let tab = |shift: bool| egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        },
    };
    v.focus = true;
    frame(&ctx, &mut s, &v, 460.0, vec![]);
    v.focus = false;
    assert_eq!(focused(&ctx), Some(field));
    frame(
        &ctx,
        &mut s,
        &v,
        460.0,
        vec![egui::Event::Text("draft".into())],
    );
    assert!(s.editing);

    let out = frame(&ctx, &mut s, &v, 460.0, vec![tab(true)]);
    assert!(out.has_keyboard_focus, "버튼 포커스도 띠의 포커스다");
    assert!(!s.editing, "주소 입력을 떠나면 편집 표시를 끈다");
    assert_eq!(
        s.buffer, v.committed_address,
        "거절되지 않은 초안은 주소로 돌아간다"
    );
    let reload = focused(&ctx).expect("Shift+Tab은 새로고침 버튼으로 간다");
    assert_ne!(reload, field);
    let out = frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Enter)]);
    assert_eq!(out.action, HtmlAddressAction::Reload);

    frame(&ctx, &mut s, &v, 460.0, vec![tab(true)]);
    let forward = focused(&ctx).expect("앞으로 버튼");
    assert_ne!(forward, reload);
    let out = frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Space)]);
    assert_eq!(out.action, HtmlAddressAction::Forward);

    let out = frame(&ctx, &mut s, &v, 460.0, vec![tab(true)]);
    assert_eq!(
        focused(&ctx),
        Some(field),
        "비활성 뒤로를 건너뛰고 주소로 돈다"
    );
    assert!(out.has_keyboard_focus);
    frame(&ctx, &mut s, &v, 460.0, vec![tab(false)]);
    assert_eq!(
        focused(&ctx),
        Some(forward),
        "주소에서 Tab은 첫 버튼으로 돈다"
    );
    frame(&ctx, &mut s, &v, 460.0, vec![tab(false)]);
    assert_eq!(focused(&ctx), Some(reload));
    frame(&ctx, &mut s, &v, 460.0, vec![tab(false)]);
    assert_eq!(focused(&ctx), Some(field));

    frame(&ctx, &mut s, &v, 460.0, vec![tab(true)]);
    let out = frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Escape)]);
    assert!(!out.has_keyboard_focus, "Escape는 버튼 포커스도 놓는다");
}
/// 띠 위아래에 다른 host 버튼이 있는 프레임. 띠 밖으로 포커스가 새는지 본다.
fn framed_with_neighbors(
    ctx: &egui::Context,
    state: &mut HtmlAddressState,
    view: &HtmlAddressView<'_>,
    events: Vec<egui::Event>,
) -> HtmlAddressOutput {
    let mut result = None;
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 300.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                drop(ui.button("above"));
                result = Some(html_address_bar(
                    ui,
                    &tasty_themes::mocha_fallback(),
                    egui::Id::new("address"),
                    LogicalPx(460.0),
                    state,
                    view,
                ));
                drop(ui.button("below"));
            });
        },
    ));
    result.unwrap()
}
fn with_mods(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}
/// 버튼이나 주소 입력에 포커스가 있을 때 방향키가 띠 밖 host 위젯으로 포커스를 옮기지 않는다.
/// Ctrl·Alt·Cmd를 함께 누른 Tab은 띠를 돌지 않는다(macOS의 Ctrl은 command가 아니다).
#[test]
fn arrows_and_modified_tab_keep_focus_where_it_is() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.can_back = true;
    let field = html_address_field_id(egui::Id::new("address"));
    let focused = |ctx: &egui::Context| ctx.memory(|m| m.focused());
    v.focus = true;
    framed_with_neighbors(&ctx, &mut s, &v, vec![]);
    v.focus = false;
    framed_with_neighbors(&ctx, &mut s, &v, vec![]);
    for mods in [
        egui::Modifiers::CTRL,
        egui::Modifiers::ALT,
        egui::Modifiers::MAC_CMD,
        egui::Modifiers::COMMAND,
    ] {
        framed_with_neighbors(&ctx, &mut s, &v, vec![with_mods(egui::Key::Tab, mods)]);
        framed_with_neighbors(&ctx, &mut s, &v, vec![]);
        assert_eq!(focused(&ctx), Some(field), "{mods:?}+Tab은 돌지 않는다");
    }
    framed_with_neighbors(
        &ctx,
        &mut s,
        &v,
        vec![with_mods(egui::Key::Tab, egui::Modifiers::SHIFT)],
    );
    framed_with_neighbors(&ctx, &mut s, &v, vec![]);
    let reload = focused(&ctx).unwrap();
    assert_ne!(reload, field);
    for arrow in [
        egui::Key::ArrowUp,
        egui::Key::ArrowDown,
        egui::Key::ArrowLeft,
        egui::Key::ArrowRight,
    ] {
        let out = framed_with_neighbors(&ctx, &mut s, &v, vec![key(arrow)]);
        framed_with_neighbors(&ctx, &mut s, &v, vec![]);
        assert_eq!(focused(&ctx), Some(reload), "{arrow:?}");
        assert!(out.has_keyboard_focus);
    }
}
/// 미러는 버튼과 읽기 전용 주소 사이를 돌지만 주소로 돌아와도 편집 상태가 되지 않는다.
#[test]
fn mirror_cycles_without_editing() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.kind = HtmlAddressKind::Mirror;
    let field = html_address_field_id(egui::Id::new("address"));
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    click(&ctx, &mut s, &v, out.field_rect.unwrap().center());
    assert_eq!(ctx.memory(|m| m.focused()), Some(field));
    let shift_tab = with_mods(egui::Key::Tab, egui::Modifiers::SHIFT);
    let out = frame(&ctx, &mut s, &v, 460.0, vec![shift_tab.clone()]);
    assert!(out.has_keyboard_focus);
    assert_ne!(ctx.memory(|m| m.focused()), Some(field));
    frame(&ctx, &mut s, &v, 460.0, vec![key(egui::Key::Tab)]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(field));
    assert!(!s.editing);
    assert_eq!(s.buffer, v.address);
}

fn marker(kind: HtmlRemoteMarkerKind) -> HtmlRemoteMarkerView<'static> {
    HtmlRemoteMarkerView {
        kind,
        tooltip: "Remote content blocked. Click for details.",
        force_hover: false,
        native: &[],
    }
}

/// 원격 표지는 주소 칸 끝(오른쪽 padding으로 space-xs 들어간 자리)의 20 칸이다.
/// 차단 표지를 누르면 클릭만 알리고 주소 편집을 시작하지 않는다.
#[test]
fn the_blocked_remote_marker_sits_at_the_field_end_and_clicks_without_editing() {
    let theme = tasty_themes::mocha_fallback();
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.kind = HtmlAddressKind::Local;
    v.remote_marker = Some(marker(HtmlRemoteMarkerKind::Blocked));
    let out = frame(&ctx, &mut s, &v, 460.0, vec![]);
    let field = out.field_rect.unwrap();
    let m = out.remote_marker_rect.unwrap();
    assert_eq!(m.width(), theme.html_remote_marker_hit().value());
    assert_eq!(m.height(), theme.html_remote_marker_hit().value());
    assert_eq!(
        m.right(),
        field.right() - theme.input_padding_x().value() + theme.spacing_xs.value()
    );
    assert_eq!(m.center().y, field.center().y);

    let out = click(&ctx, &mut s, &v, m.center());
    assert!(out.remote_marker_clicked);
    assert!(!s.editing, "a marker click must not start address editing");
    assert!(!out.has_keyboard_focus);
}

/// 허용 표지는 툴팁만 있어 클릭을 알리지 않는다. 표지가 없으면 칸도 없다.
#[test]
fn the_allowed_marker_does_not_click_and_no_marker_draws_no_cell() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.remote_marker = Some(marker(HtmlRemoteMarkerKind::Allowed));
    let m = frame(&ctx, &mut s, &v, 460.0, vec![])
        .remote_marker_rect
        .unwrap();
    assert!(!click(&ctx, &mut s, &v, m.center()).remote_marker_clicked);
    v.remote_marker = None;
    assert!(
        frame(&ctx, &mut s, &v, 460.0, vec![])
            .remote_marker_rect
            .is_none()
    );
}

/// mirror에서 건너뛴 파일 Tag와 함께 있으면 Tag 다음에 표지가 온다.
#[test]
fn the_skipped_tag_comes_before_the_remote_marker() {
    let ctx = egui::Context::default();
    let mut s = HtmlAddressState::default();
    let mut v = view();
    v.kind = HtmlAddressKind::Mirror;
    v.skipped = Some(("2 skipped", "Declined: hero-video.mp4"));
    let without = frame(&ctx, &mut s, &v, 460.0, vec![]);
    v.remote_marker = Some(marker(HtmlRemoteMarkerKind::Blocked));
    let with = frame(&ctx, &mut s, &v, 460.0, vec![]);
    let m = with.remote_marker_rect.unwrap();
    // The field keeps its width; the marker takes the end and the Tag moves left of it.
    assert_eq!(with.field_rect, without.field_rect);
    assert!(m.right() > with.field_rect.unwrap().right() - 16.0);
}
