use super::*;

fn root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("C:/work")
    } else {
        PathBuf::from("/work")
    }
}

fn file_url(path: &Path) -> String {
    match resolve_html_address(&path.to_string_lossy(), None, None, None) {
        Ok(t) => t.url.to_string(),
        Err(e) => panic!("{e:?}"),
    }
}

fn source<'a>(
    info: &'a tasty_model::WebViewNavInfo,
    cwd: Option<&'a Path>,
) -> HtmlAddressSource<'a> {
    HtmlAddressSource {
        nav: NavState::Done,
        info,
        host_url: None,
        cwd,
        home: None,
        mirror: false,
        received: None,
        remote_marker: None,
        web_page: false,
        native: &[],
    }
}

fn answered(status: LocalFileStatus) -> impl FnOnce(PathBuf) -> mpsc::Receiver<LocalFileStatus> {
    move |_| {
        let (tx, rx) = mpsc::channel();
        tx.send(status).expect("receiver alive");
        rx
    }
}

#[test]
fn relative_input_follows_the_document_then_the_cwd_and_otherwise_is_refused() {
    let doc = file_url(&root().join("x/index.html"));
    assert_eq!(
        plan_submit("b.html", Some(&doc), Some(&root().join("y")), None),
        Submit::Lookup(
            resolve_html_address(&root().join("x/b.html").to_string_lossy(), None, None, None)
                .unwrap()
        )
    );
    assert_eq!(
        plan_submit("b.html", Some("https://example.com/a/"), None, None),
        Submit::Navigate("https://example.com/a/b.html".into())
    );
    match plan_submit("c.html", None, Some(&root().join("y")), None) {
        Submit::Lookup(t) => assert_eq!(t.local_path, Some(root().join("y/c.html"))),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        plan_submit("c.html", None, None, None),
        Submit::Reject(AddressError::NoBase)
    );
    assert_eq!(plan_submit("   ", None, None, None), Submit::Nothing);
    assert_eq!(
        plan_submit("ftp://example.com/a", None, None, None),
        Submit::Reject(AddressError::UnsupportedScheme)
    );
}

#[test]
fn a_missing_file_is_refused_before_any_request_and_keeps_the_draft() {
    let info = tasty_model::WebViewNavInfo::default();
    let cwd = root();
    let src = source(&info, Some(&cwd));
    let mut band = Band::default();
    band.field.buffer = "missing.html".into();
    band.field.editing = true;
    assert_eq!(
        band.submit(
            "missing.html",
            None,
            &src,
            answered(LocalFileStatus::Missing)
        ),
        None
    );
    let result = band.poll_lookup().expect("lookup answered");
    assert_eq!(
        result,
        Err(AddressError::MissingFile(root().join("missing.html")))
    );
    assert!(band.lookup.is_none());
    assert_eq!(band.field.buffer, "missing.html");
}

#[test]
fn an_existing_file_is_opened_as_a_file_url() {
    let info = tasty_model::WebViewNavInfo::default();
    let cwd = root();
    let src = source(&info, Some(&cwd));
    let mut band = Band::default();
    band.field.buffer = "a.html".into();
    band.submit("a.html", None, &src, answered(LocalFileStatus::File));
    assert_eq!(
        band.poll_lookup(),
        Some(Ok(file_url(&root().join("a.html"))))
    );
}

#[test]
fn a_lookup_answer_for_an_edited_input_is_dropped() {
    let info = tasty_model::WebViewNavInfo::default();
    let cwd = root();
    let src = source(&info, Some(&cwd));
    let mut band = Band::default();
    band.field.buffer = "a.html".into();
    band.submit("a.html", None, &src, answered(LocalFileStatus::File));
    band.field.buffer = "a.htm".into();
    assert_eq!(band.poll_lookup(), None);
    assert!(band.lookup.is_none());
}

#[test]
fn a_failed_attempt_stays_visible_while_the_commit_keeps_the_previous_document() {
    let mut band = Band::default();
    let old = "https://example.com/old";
    band.observe(NavState::Done, Some(old), None);
    let nav = band.admit("https://nowhere.invalid/".into());
    assert_eq!(nav, WebViewUserNav::Open("https://nowhere.invalid/".into()));
    // backend가 아직 로드를 알리지 않은 동안에는 확정 주소를 보인다.
    band.observe(NavState::Done, Some(old), None);
    assert_eq!(band.shown(NavState::Done, Some(old)), Some(old));
    band.observe(NavState::Loading, Some(old), None);
    assert_eq!(
        band.shown(NavState::Loading, Some(old)),
        Some("https://nowhere.invalid/")
    );
    band.observe(NavState::Failed, Some(old), None);
    assert_eq!(
        band.shown(NavState::Failed, Some(old)),
        Some("https://nowhere.invalid/")
    );
    // 다음 로드가 끝나면 확정 주소로 돌아간다.
    band.observe(NavState::Loading, Some(old), None);
    band.observe(NavState::Done, Some(old), None);
    assert!(band.attempt.is_none());
    assert_eq!(band.shown(NavState::Done, Some(old)), Some(old));
}

#[test]
fn an_agent_host_load_is_the_attempt_and_a_fragment_move_clears_it_without_a_load() {
    let mut band = Band::default();
    band.observe(
        NavState::Done,
        Some("file:///a.html"),
        Some("file:///b.html"),
    );
    assert_eq!(
        band.shown(NavState::Loading, Some("file:///a.html")),
        Some("file:///b.html")
    );
    let mut band = Band::default();
    band.admit("file:///a.html#sec".into());
    band.observe(NavState::Done, Some("file:///a.html#sec"), None);
    assert!(band.attempt.is_none());
}

#[test]
fn local_addresses_show_as_paths_and_web_addresses_as_urls() {
    let home = root().join("home");
    let url = file_url(&home.join("docs/a b.html"));
    let (text, kind) = display(&url, Some(&home));
    assert_eq!(kind, HtmlAddressKind::Local);
    let relative = PathBuf::from("docs").join("a b.html");
    assert_eq!(text, format!("~/{}", relative.to_string_lossy()));
    assert_eq!(
        display("https://example.com/a/", None),
        ("https://example.com/a/".into(), HtmlAddressKind::Web)
    );
    assert_eq!(display_path(&home, Some(&home)), "~");
}

#[test]
fn the_page_taking_focus_ends_editing_but_a_late_release_does_not() {
    // 띠가 포커스를 새로 얻었고 페이지가 포커스를 가졌다: 회수한다.
    let mut bands = HtmlAddressBands {
        focused: Some((7, true)),
        ..Default::default()
    };
    assert_eq!(bands.sync_page_focus(|_| true), Some(7));
    // 회수가 늦게 반영되어 페이지 포커스가 남아 있어도 편집을 끝내지 않는다.
    bands.focused = Some((7, false));
    assert_eq!(bands.sync_page_focus(|_| true), None);
    assert_eq!(bands.drop_focus, None);
    assert_eq!(bands.sync_page_focus(|_| false), None);
    // 그 뒤 페이지를 누르면 편집을 끝낸다.
    assert_eq!(bands.sync_page_focus(|_| true), None);
    assert_eq!(bands.drop_focus, Some(7));
}

fn run(
    ctx: &egui::Context,
    bands: &mut HtmlAddressBands,
    src: &HtmlAddressSource<'_>,
    events: Vec<egui::Event>,
) -> BandFrame {
    let theme = tasty_themes::mocha_fallback();
    let mut out = None;
    drop(ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 400.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = ui.max_rect();
                out = Some(bands.draw(ui, &theme, 3, rect, src));
            });
        },
    ));
    out.expect("drawn")
}

#[test]
fn the_focus_shortcut_and_enter_queue_a_user_open_from_an_empty_tab() {
    let info = tasty_model::WebViewNavInfo::default();
    let src = source(&info, None);
    let ctx = egui::Context::default();
    let mut bands = HtmlAddressBands::default();
    let first = run(&ctx, &mut bands, &src, vec![]);
    assert_eq!(first.height.value(), 40.0);
    bands.request_focus(3);
    run(&ctx, &mut bands, &src, vec![]);
    assert_eq!(bands.focused.map(|(s, _)| s), Some(3));
    run(
        &ctx,
        &mut bands,
        &src,
        vec![egui::Event::Text("https://example.com/".into())],
    );
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let out = run(&ctx, &mut bands, &src, vec![enter]);
    assert_eq!(
        out.nav,
        Some(WebViewUserNav::Open("https://example.com/".into()))
    );
    // 연 뒤에는 편집이 끝나고 오류 행이 없다.
    let after = run(&ctx, &mut bands, &src, vec![]);
    assert_eq!(after.height.value(), 40.0);
    assert!(bands.focused.is_none());
}

/// 띠가 키보드 포커스를 가진 동안에만 키·IME 게이트(`keyboard_overlay_open`)가 열린다. 포커스를 가진
/// 띠를 그리지 않은 프레임(탭 전환 등)에는 포커스를 놓아 터미널 입력을 막아 두지 않는다.
#[test]
fn the_keyboard_gate_follows_the_band_focus_and_closes_when_the_band_is_not_drawn() {
    let info = tasty_model::WebViewNavInfo::default();
    let src = source(&info, None);
    let ctx = egui::Context::default();
    let mut bands = HtmlAddressBands::default();
    run(&ctx, &mut bands, &src, vec![]);
    bands.finish_frame();
    assert!(!bands.has_keyboard_focus());
    bands.request_focus(3);
    run(&ctx, &mut bands, &src, vec![]);
    bands.finish_frame();
    assert!(bands.has_keyboard_focus());
    assert!(bands.has_keyboard_focus_on(3) && !bands.has_keyboard_focus_on(4));
    // 그 띠를 그리지 않은 프레임.
    bands.finish_frame();
    assert!(!bands.has_keyboard_focus());
}

#[test]
fn a_relative_input_without_a_base_shows_the_error_row_and_sends_nothing() {
    let info = tasty_model::WebViewNavInfo::default();
    let src = source(&info, None);
    let ctx = egui::Context::default();
    let mut bands = HtmlAddressBands::default();
    bands.request_focus(3);
    run(&ctx, &mut bands, &src, vec![]);
    run(
        &ctx,
        &mut bands,
        &src,
        vec![egui::Event::Text("c.html".into())],
    );
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let out = run(&ctx, &mut bands, &src, vec![enter]);
    assert_eq!(out.nav, None);
    let shown = run(&ctx, &mut bands, &src, vec![]);
    assert_eq!(shown.height.value(), 64.0);
    assert_eq!(bands.bands[&3].field.buffer, "c.html");
}

#[test]
fn the_mirror_placeholder_hides_the_band() {
    let info = tasty_model::WebViewNavInfo::default();
    let src = HtmlAddressSource {
        mirror: true,
        received: None,
        ..source(&info, None)
    };
    let ctx = egui::Context::default();
    let mut bands = HtmlAddressBands::default();
    assert_eq!(run(&ctx, &mut bands, &src, vec![]).height.value(), 0.0);
}

/// 차단 원격 표지를 누르면 안내 다시 보이기만 알리고 탐색이나 주소 편집을 시작하지 않는다.
#[test]
fn the_blocked_remote_marker_reshows_the_notice_without_navigating() {
    let theme = tasty_themes::mocha_fallback();
    let info = tasty_model::WebViewNavInfo::default();
    let src = HtmlAddressSource {
        remote_marker: Some(tasty_ui_widgets::HtmlRemoteMarkerKind::Blocked),
        ..source(&info, None)
    };
    let ctx = egui::Context::default();
    let mut bands = HtmlAddressBands::default();
    run(&ctx, &mut bands, &src, vec![]);
    // 표지 칸은 필드 오른쪽 padding으로 space-xs 들어간 자리의 20 칸이다(위젯 시험이 위치를 고정한다).
    let field_right = 800.0 - theme.html_address_bar_padding_x().value();
    let right = field_right - theme.input_padding_x().value() + theme.spacing_xs.value();
    let p = egui::pos2(
        right - theme.html_remote_marker_hit().value() / 2.0,
        theme.html_address_bar_height().value() / 2.0,
    );
    let press = |pressed| egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    run(
        &ctx,
        &mut bands,
        &src,
        vec![egui::Event::PointerMoved(p), press(true)],
    );
    let out = run(&ctx, &mut bands, &src, vec![press(false)]);
    assert!(out.remote_reshow);
    assert!(out.nav.is_none());
    assert!(
        !bands.bands[&3].field.editing,
        "a marker press must not start address editing"
    );
    // 편집이 시작됐다면 요청한 포커스가 다음 프레임에 띠로 온다.
    run(&ctx, &mut bands, &src, vec![]);
    assert!(!bands.has_keyboard_focus());
}
