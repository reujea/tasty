//! 저장된 글꼴 값이 거절되면 사용자에게 한 번 알린다. 알린 값을 기억해 같은 값을 다시 저장해도 카드가
//! 늘지 않게 한다. 판정은 등록 경로와 별개다. 등록은 `plugin_font_overrides` 키가 있는 종류만 지나므로
//! 기본 글꼴만 저장한 가장 흔한 경우를 놓친다.

use std::collections::HashSet;

use crate::adapters::ui::font_judge::{RejectedValue, judge_saved_fonts, saved_font_targets};
use crate::intent::{IntentOrigin, UserSource};
use crate::settings::AppearanceSettings;

/// 설정 창 Save 의 사용자 메뉴 출처.
pub(crate) const SETTINGS_SAVE_MENU: &str = "settings_save";

/// 이번 프로세스에서 사용자에게 알린 거절 값들.
#[derive(Default)]
pub(crate) struct FontRejectionNotices {
    reported: HashSet<RejectedValue>,
}

impl FontRejectionNotices {
    /// 이번 판정 결과를 반영하고 처음 거절된 값을 돌려준다. 통과한 값은 잊어 다시 거절되면 또 알린다.
    fn take_new(&mut self, rejected: Vec<RejectedValue>) -> Vec<String> {
        self.forget_passing(&rejected);
        rejected
            .into_iter()
            .filter(|value| self.reported.insert(value.clone()))
            .map(|value| value.value)
            .collect()
    }

    fn forget_passing(&mut self, rejected: &[RejectedValue]) {
        self.reported.retain(|value| rejected.contains(value));
    }

    pub(crate) fn at_boot(&mut self, appearance: &AppearanceSettings) -> Vec<String> {
        let mut lookup = Default::default();
        self.take_new(judge_saved_fonts(appearance, &mut lookup))
    }

    pub(crate) fn after_apply(
        &mut self,
        origin: &IntentOrigin,
        prev: Option<&AppearanceSettings>,
        next: &AppearanceSettings,
    ) -> Vec<String> {
        self.after_apply_with(origin, prev, next, |appearance| {
            let mut lookup = Default::default();
            judge_saved_fonts(appearance, &mut lookup)
        })
    }

    /// 사용자 출처면 처음 거절된 값을 돌려준다. 그 밖의 출처는 로그만 남기고 집합에 넣지 않는다.
    /// 파일을 읽는 판정은 글꼴 값이 바뀌었을 때와 설정 창 Save 에서만 돈다. Save 는 값이 같아도 판정해
    /// 파일을 고친 값을 잊는다.
    fn after_apply_with(
        &mut self,
        origin: &IntentOrigin,
        prev: Option<&AppearanceSettings>,
        next: &AppearanceSettings,
        judge: impl FnOnce(&AppearanceSettings) -> Vec<RejectedValue>,
    ) -> Vec<String> {
        let changed = prev.is_none_or(|prev| saved_font_targets(prev) != saved_font_targets(next));
        if !changed && !is_settings_save(origin) {
            return Vec::new();
        }
        let rejected = judge(next);
        if origin.is_user() {
            return self.take_new(rejected);
        }
        self.forget_passing(&rejected);
        for value in &rejected {
            tracing::warn!(
                "settings applied a font that cannot be used: {}",
                value.value
            );
        }
        Vec::new()
    }
}

fn is_settings_save(origin: &IntentOrigin) -> bool {
    matches!(
        origin,
        IntentOrigin::User {
            source: UserSource::Menu(SETTINGS_SAVE_MENU),
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ui::font_judge::fixtures::{
        VALID, negative_height, not_a_font, without_table,
    };
    use crate::intent::AgentSource;
    use std::cell::Cell;

    fn save() -> IntentOrigin {
        IntentOrigin::User {
            source: UserSource::Menu(SETTINGS_SAVE_MENU),
        }
    }

    fn other_user() -> IntentOrigin {
        IntentOrigin::User {
            source: UserSource::Shortcut("zoom"),
        }
    }

    fn agent() -> IntentOrigin {
        IntentOrigin::Agent {
            source: AgentSource::Cli,
        }
    }

    struct Files(tempfile::TempDir);

    impl Files {
        fn new() -> Self {
            Self(tempfile::tempdir().unwrap())
        }

        fn write(&self, name: &str, bytes: &[u8]) -> String {
            let path = self.0.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            path.display().to_string()
        }
    }

    fn with_default_file(path: &str) -> AppearanceSettings {
        let mut appearance = AppearanceSettings::default();
        appearance.default_font.custom_font_path = path.to_owned();
        appearance
    }

    fn with_terminal_file(path: &str) -> AppearanceSettings {
        let mut appearance = AppearanceSettings::default();
        appearance.terminal_font.custom_font_path = Some(path.to_owned());
        appearance
    }

    fn saved(appearance: &AppearanceSettings) -> usize {
        FontRejectionNotices::default()
            .after_apply(&save(), None, appearance)
            .len()
    }

    /// 부팅 카드는 첫 overlay 프레임보다 먼저 쌓인다. 저장된 수명이 빠지면 기본 수명으로 사라진다.
    #[test]
    fn boot_cards_live_as_long_as_the_saved_setting() {
        let mut settings = crate::settings::Settings::default();
        settings.overlay.toast_duration_ms = 10_000;
        settings.appearance = with_default_file("/no/such/dir/font.ttf");
        let mut toasts = crate::adapters::ui::ToastManager::new();
        crate::app::App::report_boot_notices(
            &mut toasts,
            &mut FontRejectionNotices::default(),
            &settings,
        );
        let lifetimes = toasts.card_lifetimes();
        assert!(!lifetimes.is_empty());
        assert!(
            lifetimes
                .iter()
                .all(|life| *life == std::time::Duration::from_millis(10_000)),
            "{lifetimes:?}"
        );
    }

    /// 등록 경로는 키가 있는 종류만 지난다. 키가 없는 기본 글꼴이 여기서 잡혀야 한다.
    #[test]
    fn a_default_font_without_any_override_key_is_reported_once() {
        let files = Files::new();
        let path = files.write("a.ttf", &not_a_font());
        let appearance = with_default_file(&path);
        assert!(appearance.plugin_font_overrides.is_empty());
        let values = FontRejectionNotices::default().after_apply(&save(), None, &appearance);
        assert_eq!(values, [path]);
    }

    /// 값이 종류마다 상속돼 판정이 여럿이어도 카드는 하나다.
    #[test]
    fn a_value_shared_by_many_targets_is_one_notice() {
        let files = Files::new();
        let path = files.write("a.ttf", &not_a_font());
        let mut appearance = with_default_file(&path);
        for kind in ["explorer", "markdown"] {
            appearance
                .plugin_font_overrides
                .insert(kind.to_owned(), Default::default());
        }
        assert_eq!(saved(&appearance), 1);
    }

    #[test]
    fn saving_the_same_value_twice_is_one_notice() {
        let files = Files::new();
        let appearance = with_default_file(&files.write("a.ttf", &not_a_font()));
        let mut notices = FontRejectionNotices::default();
        assert_eq!(notices.after_apply(&save(), None, &appearance).len(), 1);
        let again = notices.after_apply(&save(), Some(&appearance), &appearance);
        assert_eq!(again.len(), 0);
    }

    #[test]
    fn a_value_that_passed_and_breaks_again_is_a_new_notice() {
        let files = Files::new();
        let path = files.write("a.ttf", &not_a_font());
        let appearance = with_default_file(&path);
        let mut notices = FontRejectionNotices::default();
        assert_eq!(notices.after_apply(&save(), None, &appearance).len(), 1);
        files.write("a.ttf", VALID);
        let fixed = notices.after_apply(&save(), Some(&appearance), &appearance);
        assert_eq!(fixed.len(), 0);
        files.write("a.ttf", &not_a_font());
        let broken = notices.after_apply(&save(), Some(&appearance), &appearance);
        assert_eq!(broken.len(), 1);
    }

    #[test]
    fn two_values_are_two_notices() {
        let files = Files::new();
        let mut appearance = with_default_file(&files.write("a.ttf", &not_a_font()));
        appearance.terminal_font.custom_font_path = Some(files.write("b.ttf", &not_a_font()));
        assert_eq!(saved(&appearance), 2);
    }

    #[test]
    fn a_missing_file_is_one_notice() {
        let files = Files::new();
        let missing = files.0.path().join("none.ttf").display().to_string();
        assert_eq!(saved(&with_default_file(&missing)), 1);
    }

    #[test]
    fn an_empty_value_is_no_notice() {
        assert_eq!(saved(&AppearanceSettings::default()), 0);
    }

    #[test]
    fn a_family_that_is_gone_is_one_notice() {
        let mut appearance = AppearanceSettings::default();
        appearance.default_font.font_family = "Tasty No Such Family".to_owned();
        let values = FontRejectionNotices::default().after_apply(&save(), None, &appearance);
        assert_eq!(values, ["Tasty No Such Family"]);
    }

    /// 터미널 적용값은 터미널 판정만 받는다.
    #[test]
    fn terminal_values_get_the_terminal_judgement_only() {
        let files = Files::new();
        let t1 = with_terminal_file(&files.write("t1.ttf", &negative_height()));
        assert_eq!(saved(&t1), 0, "t1");
        let a3 = with_terminal_file(&files.write("a3.ttf", &VALID[..1024]));
        assert_eq!(saved(&a3), 1, "a3");
        let t3 = with_terminal_file(&files.write("t3.ttf", &without_table(b"cmap")));
        assert_eq!(saved(&t3), 1, "t3");
    }

    /// 기본 글꼴의 값은 UI 판정과 그 값을 상속한 터미널의 판정을 모두 받는다.
    #[test]
    fn a_default_value_is_judged_by_both_renderers() {
        let files = Files::new();
        let inherited = with_default_file(&files.write("a3.ttf", &VALID[..1024]));
        assert_eq!(saved(&inherited), 1, "only the terminal refuses it");
        let b1 = with_default_file(&files.write("b1.ttf", &negative_height()));
        assert_eq!(saved(&b1), 1, "only the ui refuses it");
    }

    #[test]
    fn an_agent_apply_logs_and_does_not_remember() {
        let files = Files::new();
        let appearance = with_default_file(&files.write("a.ttf", &not_a_font()));
        let mut notices = FontRejectionNotices::default();
        assert_eq!(notices.after_apply(&agent(), None, &appearance).len(), 0);
        assert_eq!(
            notices
                .after_apply(&IntentOrigin::System, None, &appearance)
                .len(),
            0
        );
        assert_eq!(notices.after_apply(&save(), None, &appearance).len(), 1);
    }

    /// 줌처럼 사용자 출처 적용이 잦은 경로에서 글꼴 값이 그대로면 파일을 다시 읽지 않는다.
    #[test]
    fn an_apply_with_the_same_font_values_reads_no_file() {
        let files = Files::new();
        let appearance = with_default_file(&files.write("a.ttf", &not_a_font()));
        let mut zoomed = appearance.clone();
        zoomed.default_font.font_size += 1.0;
        let judged = Cell::new(0);
        let judge = |_: &AppearanceSettings| {
            judged.set(judged.get() + 1);
            Vec::new()
        };
        let mut notices = FontRejectionNotices::default();
        notices.after_apply_with(&other_user(), Some(&appearance), &zoomed, judge);
        notices.after_apply_with(&agent(), Some(&appearance), &zoomed, judge);
        assert_eq!(judged.get(), 0);
        notices.after_apply_with(&save(), Some(&appearance), &zoomed, judge);
        assert_eq!(judged.get(), 1, "Save judges the same value");
    }

    #[test]
    fn a_user_apply_that_changes_a_font_value_is_judged() {
        let files = Files::new();
        let next = with_default_file(&files.write("a.ttf", &not_a_font()));
        let prev = AppearanceSettings::default();
        let mut notices = FontRejectionNotices::default();
        assert_eq!(
            notices.after_apply(&other_user(), Some(&prev), &next).len(),
            1
        );
    }
}
