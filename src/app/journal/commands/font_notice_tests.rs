//! 저장이 적용 경로를 지나 글꼴 거절 카드가 되는 연결을 잰다. 메인 창은 GPU 가 있어야 만들어지므로 창에 쌓인
//! 카드 대신 `toast_settings_result` 가 창이 없을 때 남기는 경고를 센다. 다른 시험이 같은 콜사이트를 구독자
//! 없이 치면 "never" 로 캐시되고, 적용 경로가 전역 Theme 도 설치하며, winit `EventLoop` 는 프로세스에 하나뿐이므로
//! 새 프로세스에서 혼자 돈다. `App` 을 만드는 데 X11 디스플레이가 필요하다.

use super::*;
use crate::adapters::ui::font_judge::fixtures::not_a_font;
use crate::app::App;
use crate::app::font_rejection::SETTINGS_SAVE_MENU;
use crate::app::settings_change::test_support::{ProbeStorage, core_with};
use crate::intent::{IntentOrigin, UserSource};
use std::sync::Mutex;
use winit::event_loop::EventLoop;

const NO_WINDOW: &str = "settings result without a main window";

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Logs {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }

    fn cards(&self) -> usize {
        self.text().matches(NO_WINDOW).count()
    }

    fn cards_of(&self, message: &str) -> usize {
        self.text().matches(message).count()
    }

    fn capture(&self, run: impl FnOnce()) {
        let sink = self.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .without_time()
            .with_writer(move || sink.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            run();
        });
    }
}

fn save() -> IntentOrigin {
    IntentOrigin::User {
        source: UserSource::Menu(SETTINGS_SAVE_MENU),
    }
}

fn event_loop() -> EventLoop<crate::AppEvent> {
    crate::x11_test_loop::x11_event_loop()
}

fn app_on(event_loop: &EventLoop<crate::AppEvent>, fail_saves: bool) -> App {
    let mut app = crate::app::preset_shutdown_tests::make_gui(event_loop.create_proxy());
    app.services = core_with(Arc::new(ProbeStorage {
        fail: fail_saves,
        ..Default::default()
    }));
    app
}

struct BadFont {
    _dir: tempfile::TempDir,
    path: String,
}

impl BadFont {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.ttf");
        std::fs::write(&path, not_a_font()).unwrap();
        let path = path.display().to_string();
        Self { _dir: dir, path }
    }

    fn card(&self) -> String {
        crate::i18n::t_fmt_fit("toast.font_rejected", &self.path)
    }
}

#[test]
fn a_save_on_the_apply_path_is_one_card() {
    if crate::test_support::rerun_alone(module_path!(), "a_save_on_the_apply_path_is_one_card") {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let font = BadFont::new();
    let mut next = crate::settings::Settings::default();
    next.appearance.default_font.custom_font_path = font.path.clone();
    let event_loop = event_loop();
    let mut app = app_on(&event_loop, false);
    let logs = Logs::default();
    logs.capture(|| {
        app.cascade_settings_updated(next.clone(), &save(), None);
        app.cascade_settings_updated(next, &save(), None);
    });
    assert_eq!(logs.cards_of(&font.card()), 1, "{}", logs.text());
    assert_eq!(logs.cards(), 1, "{}", logs.text());
}

/// 카테고리를 끄는 저장은 journal 이 끝난 뒤에 적용된다. 그때 같은 출처로 카드 한 장이 나온다.
#[test]
fn a_save_deferred_by_a_category_reset_is_one_card_after_the_journal() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_save_deferred_by_a_category_reset_is_one_card_after_the_journal",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let font = BadFont::new();
    let (mut session, journal) = boot();
    session
        .runtime
        .settings
        .general
        .workspace_categories_enabled = true;
    let mut next = session.runtime.settings.clone();
    next.general.workspace_categories_enabled = false;
    next.appearance.default_font.custom_font_path = font.path.clone();

    let event_loop = event_loop();
    let mut app = app_on(&event_loop, false);
    app.journal = journal;
    let (state, _fixture) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(vec![]));
    let id = app.engines.park_for_test(state, session);
    let logs = Logs::default();
    let pump = |app: &mut App| {
        let mut sessions: Vec<_> = app.engines.all_sessions_mut().collect();
        app.journal.poll_bootstrap(&mut sessions, None).unwrap();
        app.resolve_journal_requests();
    };
    logs.capture(|| {
        app.cascade_settings_updated(next, &save(), None);
        let applied = |app: &App| {
            !app.engines
                .get(id)
                .unwrap()
                .runtime
                .settings
                .general
                .workspace_categories_enabled
        };
        assert!(!applied(&app), "the reset holds the save back");
        assert_eq!(logs.cards(), 0, "{}", logs.text());
        let mut stall = StallBudget::new(&app.journal);
        while !applied(&app) {
            pump(&mut app);
            stall.nap("deferred settings save");
        }
        for _ in 0..3 {
            pump(&mut app);
        }
    });
    assert_eq!(logs.cards_of(&font.card()), 1, "{}", logs.text());
    assert_eq!(logs.cards(), 1, "{}", logs.text());
}

/// 저장 실패 카드와 글꼴 거절 카드는 서로 다른 카드다.
#[test]
fn a_failed_save_and_a_rejected_font_are_two_cards() {
    if crate::test_support::rerun_alone(
        module_path!(),
        "a_failed_save_and_a_rejected_font_are_two_cards",
    ) {
        return;
    }
    let _home = crate::test_support::IsolatedHome::new();
    let font = BadFont::new();
    let mut next = crate::settings::Settings::default();
    next.appearance.default_font.custom_font_path = font.path.clone();
    let event_loop = event_loop();
    let mut app = app_on(&event_loop, true);
    let logs = Logs::default();
    logs.capture(|| {
        app.cascade_settings_updated(next, &save(), None);
        let deadline = Instant::now() + Duration::from_secs(10);
        while logs.cards() < 2 {
            assert!(Instant::now() < deadline, "{}", logs.text());
            app.services.poll_file_writer();
            app.settle_settings_saves();
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    assert_eq!(logs.cards_of(&font.card()), 1, "{}", logs.text());
    assert_eq!(logs.cards(), 2, "{}", logs.text());
}
