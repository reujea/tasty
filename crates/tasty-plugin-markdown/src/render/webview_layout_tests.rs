//! `render_document` 의 HTML 을 실제 WebKitGTK 에 띄워 주소창 배치를 잰다(Linux 전용).
//!
//! 생성 CSS 문자열 시험은 선언이 있는지만 본다. 여기서는 엔진이 그 CSS 로 주소창을 문서 어느
//! 위치에서도 상단에 붙이는지를 `getBoundingClientRect()` 로 판정한다. GTK3 는 디스플레이가 있어야
//! 하므로 `#[ignore]` 로 두고, 디스플레이를 주는 CI 단계가 이름으로 부른다
//! (`docs/dev-guide/ci-gates.md`). 실행:
//! `xvfb-run -a cargo test -p tasty-plugin-markdown -- --ignored --exact render::webview_layout_tests::address_bar_stays_on_top_in_webkitgtk`

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use javascriptcore::ValueExt as _;
use tasty_plugin_sdk::Translator;
use tasty_type_appearance::theme::Theme;
use webkit2gtk::{LoadEvent, WebView, WebViewExt as _};

use super::{AddrFailure, AddrFailureKind, DocumentInput, RemoteView, render_document};

/// 측정 뷰포트. 회귀를 처음 잰 값과 같게 둔다.
const VIEW_W: i32 = 1000;
const VIEW_H: i32 = 800;
/// 엔진 응답 한 번을 기다리는 최대 시간. 넘기면 엔진이 멈춘 것으로 보고 실패한다.
const WAIT: Duration = Duration::from_secs(20);

fn document(source: &str) -> String {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let tr = Translator::default();
    render_document(DocumentInput {
        find_bindings: &[],
        address_bindings: &[],
        theme: &theme,
        tr: &tr,
        file_path: "/a/current.md",
        source,
        load_error: None,
        base_dir: Some(Path::new("/a")),
        recent: &[],
        remote: None,
        large_deferred: false,
    })
}

/// 화면보다 몇 배 긴 문서. 가운데 제목은 앵커 이동 목적지다.
fn long_source() -> String {
    let mut s = String::new();
    for i in 0..120 {
        s.push_str(&format!(
            "## Section {i}\n\nParagraph {i} with enough words to wrap across the line once or twice.\n\n"
        ));
    }
    s
}

fn spin_until(done: impl Fn() -> bool, what: &str) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < WAIT,
            "{what}: WebKitGTK 가 {WAIT:?} 안에 답하지 않았다"
        );
        gtk::main_iteration_do(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}

struct Page {
    _window: gtk::OffscreenWindow,
    view: WebView,
}

impl Page {
    fn load(html: &str) -> Self {
        Self::load_sized(html, VIEW_W)
    }

    fn load_sized(html: &str, width: i32) -> Self {
        let window = gtk::OffscreenWindow::new();
        window.set_default_size(width, VIEW_H);
        let view = WebView::new();
        window.add(&view);
        window.show_all();
        let finished = Rc::new(Cell::new(false));
        let flag = finished.clone();
        view.connect_load_changed(move |_, event| {
            if event == LoadEvent::Finished {
                flag.set(true);
            }
        });
        view.load_html(html, None);
        spin_until(|| finished.get(), "문서 적재");
        Page {
            _window: window,
            view,
        }
    }

    /// 스크립트의 값을 JSON 문자열로 돌려받아 숫자로 읽는다.
    fn number(&self, script: &str) -> f64 {
        let value = self.json(script);
        value
            .as_f64()
            .unwrap_or_else(|| panic!("{script}: 숫자가 아니다: {value}"))
    }

    /// 스크립트의 값을 문자열로 읽는다.
    fn string(&self, script: &str) -> String {
        let value = self.json(script);
        value
            .as_str()
            .unwrap_or_else(|| panic!("{script}: 문자열이 아니다: {value}"))
            .to_string()
    }

    fn json(&self, script: &str) -> serde_json::Value {
        let out: Rc<RefCell<Option<Result<String, String>>>> = Rc::new(RefCell::new(None));
        let slot = out.clone();
        let wrapped = format!("JSON.stringify((() => {{ {script} }})())");
        self.view.evaluate_javascript(
            &wrapped,
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            move |r| {
                *slot.borrow_mut() =
                    Some(r.map(|v| v.to_str().to_string()).map_err(|e| e.to_string()));
            },
        );
        spin_until(|| out.borrow().is_some(), script);
        let text = out
            .borrow_mut()
            .take()
            .expect("filled above")
            .unwrap_or_else(|e| panic!("{script}: 스크립트 실패: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{script}: {text}: {e}"))
    }

    /// 찾기 막대에 검색어를 넣고 건수 표시가 기대값이 될 때까지 기다린다.
    /// 입력 뒤 검색은 타이머로 예약되므로 첫 읽기가 이전 값을 볼 수 있다.
    fn find_count_becomes(&self, query: &str, expected: &str) {
        let set = format!(
            "const i = document.getElementById('tasty-find-input'); i.value = {}; \
             i.dispatchEvent(new Event('input', {{bubbles: true}})); return 0;",
            serde_json::to_string(query).expect("문자열 직렬화")
        );
        self.number(&set);
        let start = Instant::now();
        loop {
            let shown =
                self.string("return document.getElementById('tasty-find-count').textContent;");
            if shown == expected {
                return;
            }
            assert!(
                start.elapsed() < WAIT,
                "검색어 {query}: 건수가 {expected} 이 아니라 {shown} 이다"
            );
            gtk::main_iteration_do(false);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

const BAR_TOP: &str =
    "return document.getElementById('tasty-addr-bar').getBoundingClientRect().top;";

/// 주소창은 긴 문서의 끝과 앵커 이동 뒤에도 뷰포트 맨 위(`rect.top == 0`)에 있고,
/// 짧은 문서에서는 `body` 상자가 뷰포트 높이를 채운다.
///
/// `body{height:100%}` 로 되돌리면 첫 단언이, `html` 의 확정 높이를 빼면 마지막 단언이 실패한다.
#[test]
#[ignore = "WebKitGTK 를 띄울 디스플레이가 필요하다 — CI 단계가 xvfb-run 안에서 이름으로 실행한다"]
fn address_bar_stays_on_top_in_webkitgtk() {
    let display = std::env::var("DISPLAY").unwrap_or_default();
    assert!(
        !display.is_empty(),
        "DISPLAY 가 없다. `xvfb-run -a cargo test -p tasty-plugin-markdown -- --ignored --exact \
         render::webview_layout_tests::address_bar_stays_on_top_in_webkitgtk` 로 실행한다"
    );
    // 사용자 세션의 WAYLAND_DISPLAY 가 있어도 지정한 X 디스플레이에 띄운다.
    gtk::gdk::set_allowed_backends("x11");
    gtk::init().expect("GTK 초기화");

    let long = Page::load(&document(&long_source()));
    let viewport = long.number("return window.innerHeight;");
    assert_eq!(viewport, f64::from(VIEW_H), "뷰포트 높이");
    let scroll_height = long.number("return document.scrollingElement.scrollHeight;");
    assert!(
        scroll_height > 4.0 * viewport,
        "문서가 화면보다 충분히 길어야 끝까지 스크롤한 결과가 의미 있다: {scroll_height}"
    );

    let scrolled = long.number(
        "const s = document.scrollingElement; s.scrollTop = s.scrollHeight; return s.scrollTop;",
    );
    assert!(
        scrolled > scroll_height - 2.0 * viewport,
        "문서 끝까지 스크롤되지 않았다: scrollTop {scrolled}, scrollHeight {scroll_height}"
    );
    assert_eq!(
        long.number(BAR_TOP),
        0.0,
        "문서 끝에서 주소창이 상단에 붙어 있지 않다 (scrollTop {scrolled})"
    );

    let anchored = long.number(
        "document.scrollingElement.scrollTop = 0; \
         document.getElementById('section-60').scrollIntoView(); \
         return document.scrollingElement.scrollTop;",
    );
    assert!(
        anchored > viewport,
        "앵커 이동이 일어나지 않았다: scrollTop {anchored}"
    );
    assert_eq!(
        long.number(BAR_TOP),
        0.0,
        "앵커 이동 뒤 주소창이 상단에 붙어 있지 않다 (scrollTop {anchored})"
    );

    let short = Page::load(&document("# Short\n\nOne line."));
    assert_eq!(
        short.number("return document.body.getBoundingClientRect().height;"),
        f64::from(VIEW_H),
        "짧은 문서의 body 가 뷰포트 높이를 채우지 않는다"
    );

    remote_address_band_layout();
    callout_label_stays_out_of_find_and_selection();
}

/// 실제 영어 번역표로 문서를 그린다. 기본 라벨이 키가 아니라 문구로 나와야 찾기를 잴 수 있다.
fn document_en(source: &str) -> String {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let tr = Translator::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("lang"), "en");
    render_document(DocumentInput {
        find_bindings: &[],
        address_bindings: &[],
        theme: &theme,
        tr: &tr,
        file_path: "/a/current.md",
        source,
        load_error: None,
        base_dir: Some(Path::new("/a")),
        recent: &[],
        remote: None,
        large_deferred: false,
    })
}

const SELECT_BODY: &str = "const s = getSelection(); \
     s.selectAllChildren(document.getElementById('tasty-md-body')); return s.toString();";
const SELECT_QUOTE: &str = "const s = getSelection(); \
     s.selectAllChildren(document.querySelector('blockquote')); return s.toString();";

/// 접지 않는 콜아웃의 라벨은 보이기만 하는 글자다. 찾기가 잡지 않고 선택·복사 문자열에도 없다.
/// 접는 콜아웃의 라벨은 실제 글자 노드라 찾기가 잡고 선택 문자열에도 있다.
/// 문자열은 라벨 구조를 바꾸기 전의 값이다. 복사는 같은 선택 문자열을 쓴다.
fn callout_label_stays_out_of_find_and_selection() {
    let page = Page::load(&document_en(
        "Before para.\n\n> [!NOTE]\n> Quoted body.\n\nAfter para.",
    ));
    page.find_count_becomes("Quoted", "1/1");
    page.find_count_becomes("Note", "0/0");
    assert_eq!(page.string(SELECT_QUOTE), "Quoted body.\n\n", "콜아웃 선택");
    assert_eq!(
        page.string(SELECT_BODY),
        "Before para.\n\nQuoted body.\n\nAfter para.",
        "본문 전체 선택"
    );

    let folded = Page::load(&document_en(
        "Before para.\n\n> [!NOTE]+\n> Quoted body.\n\nAfter para.",
    ));
    folded.find_count_becomes("Note", "1/1");
    assert_eq!(
        folded.string(SELECT_BODY),
        "Before para.\n\nNote\nQuoted body.\n\nAfter para.",
        "접는 콜아웃의 본문 전체 선택"
    );
}

fn remote_document(failure: Option<AddrFailure>) -> String {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let tr = Translator::default();
    render_document(DocumentInput {
        find_bindings: &[],
        address_bindings: &[],
        theme: &theme,
        tr: &tr,
        file_path: "/home/deploy/agents-prod/docs/very/long/folder/names/RUNBOOK.md",
        source: "# Remote\n\nBody.",
        load_error: None,
        base_dir: None,
        recent: &[],
        remote: Some(RemoteView {
            addr_failure: failure,
            ..RemoteView::default()
        }),
        large_deferred: false,
    })
}

const GO_SHOWN: &str = "return getComputedStyle(document.getElementById('tasty-addr-go-tip')).display === 'none' ? 0 : 1;";
const FIELD_W: &str =
    "return document.getElementById('tasty-addr-field').getBoundingClientRect().width;";
const BAR_H: &str =
    "return document.getElementById('tasty-addr-bar').getBoundingClientRect().height;";

/// 원격 주소 띠: 넓은 폭에서는 Go 가 보이고, 필드가 160 아래로 줄어들 폭에서는 Go 만 숨는다.
/// 필드는 하한 96 을 지킨다. 오류 행이 있으면 띠 높이가 `--md-addr-bar-h` 재정의와 같다.
/// 지목 실행이 한 이름만 고르므로 위 시험이 이어서 부른다.
fn remote_address_band_layout() {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);

    let wide = Page::load(&remote_document(None));
    assert_eq!(wide.number(GO_SHOWN), 1.0, "넓은 폭에서 Go 가 숨었다");
    assert!(wide.number(FIELD_W) >= f64::from(theme.md_address_go_hide_below().value()));
    assert_eq!(
        wide.number(BAR_H),
        f64::from(super::address::bar_height_px(&theme, false)),
        "띠 높이(40 + 경계선 1)"
    );

    // 원격 Go 경계 폭(padding·Folder·Reload·간격·필드 하한 160·Go)보다 4px 좁은 폭.
    let narrow_w = super::address::go_hide_width_px(&theme, true) as i32 - 4;
    let narrow = Page::load_sized(&remote_document(None), narrow_w);
    assert_eq!(narrow.number(GO_SHOWN), 0.0, "좁은 폭에서 Go 가 남았다");
    assert!(
        narrow.number(FIELD_W) >= f64::from(theme.md_address_field_min_width().value()),
        "필드가 하한보다 줄었다"
    );
    assert_eq!(
        narrow.number(
            "return document.getElementById('tasty-refresh').getBoundingClientRect().width;"
        ),
        f64::from(theme.icon_button_size_sm().value()),
        "Reload 는 줄거나 숨지 않는다"
    );

    let failed = Page::load(&remote_document(Some(AddrFailure {
        typed: "/home/deploy/agents-prod/RUNBOK.md".into(),
        kind: AddrFailureKind::NotFound,
    })));
    assert_eq!(
        failed.number(BAR_H),
        f64::from(super::address::bar_height_px(&theme, true)),
        "오류 행을 더한 띠 높이"
    );
    assert_eq!(
        failed.number(
            "return parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--md-addr-bar-h'));"
        ),
        f64::from(super::address::bar_height_px(&theme, true)),
        "검색 바와 앵커 여백이 읽는 높이"
    );
}
