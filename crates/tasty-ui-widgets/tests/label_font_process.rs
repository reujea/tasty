//! `font-label` 조회는 프로세스 환경을 한 번 읽어 캐시한다. 값마다 이 시험 파일을 새 프로세스로
//! 다시 실행해 플러그인처럼 `tasty_i18n::init`을 부르지 않은 프로세스의 결과를 잰다.

use tasty_ui_widgets::label_font_family;

const WANT_PROPORTIONAL: &str = "LABEL_FONT_PROBE_WANT_PROPORTIONAL";

#[test]
fn the_lookup_reads_the_locale_the_host_passed() {
    if let Ok(want) = std::env::var(WANT_PROPORTIONAL) {
        assert_eq!(tasty_i18n::current_language(), "en");
        let family = label_font_family();
        let ui_sans = want == "1";
        let want = if ui_sans {
            egui::FontFamily::Name(egui::epaint::text::MONO_LINE_SANS_FAMILY.into())
        } else {
            egui::FontFamily::Monospace
        };
        assert_eq!(family, want);
        assert_tag_widths_follow_the_variant(ui_sans);
        return;
    }
    let exe = std::env::current_exe().expect("test binary path");
    for (locale, want) in [
        (Some("zh-Hans"), "1"),
        (Some("ja"), "1"),
        (Some("en"), "0"),
        (Some("ko"), "0"),
        (None, "0"),
    ] {
        let mut child = std::process::Command::new(&exe);
        child
            .args(["--exact", "the_lookup_reads_the_locale_the_host_passed"])
            .env(WANT_PROPORTIONAL, want);
        match locale {
            Some(value) => child.env("TASTY_LOCALE", value),
            None => child.env_remove("TASTY_LOCALE"),
        };
        let out = child.output().expect("run the test binary again");
        assert!(
            out.status.success(),
            "TASTY_LOCALE={locale:?}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

/// 폭 측정이 그리는 변형과 같은 글꼴로 재는지 본다. 값 Tag는 언어와 관계없이 고정폭 폭이다.
fn assert_tag_widths_follow_the_variant(words_are_proportional: bool) {
    let theme = tasty_themes::mocha_fallback();
    let label = "Workspaces";
    let ctx = egui::Context::default();
    let mut widths = None;
    // 첫 pass에서 글꼴이 준비되므로 두 번 돈다.
    for _ in 0..2 {
        let _output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                widths = Some((
                    tasty_ui_widgets::tag_width(ui, &theme, label),
                    tasty_ui_widgets::tag_mono_width(ui, &theme, label),
                ));
            });
        });
    }
    let (word, mono) = widths.expect("the panel ran");
    assert_eq!(word != mono, words_are_proportional, "{word} / {mono}");
}

/// 새 내장 언어의 글자가 고정폭 글꼴에 없으면 라벨이 한 낱말 안에서 글꼴을 바꾼다. 언어 코드를 적지
/// 않고 내장 목록을 돌아서, 언어를 등록하면 이 시험이 저절로 그 언어를 본다.
#[test]
fn a_language_the_mono_face_cannot_hold_reads_the_ui_font() {
    let face = ttf_parser::Face::parse(tasty_font::D2CODING_REGULAR_TTF, 0).expect("D2Coding");
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lang");
    for code in tasty_i18n::BUILTIN_CODES {
        let path = format!("{root}/{code}.toml");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut value: toml::Value = toml::from_str(&text).expect("language file parses");
        if let Some(table) = value.as_table_mut() {
            table.remove("meta");
        }
        let mut map = std::collections::HashMap::new();
        tasty_i18n::flatten_catalog_toml("", &value, &mut map);
        assert!(!map.is_empty(), "{code}: 번역 문자열을 읽지 못했다");
        let lacking = map
            .values()
            .filter(|s| {
                s.chars()
                    .any(|c| !c.is_whitespace() && face.glyph_index(c).is_none())
            })
            .count();
        let family = tasty_ui_widgets::label_font_family_for(code);
        if family == egui::FontFamily::Monospace {
            assert_eq!(
                lacking, 0,
                "{code}: 고정폭 글꼴에 없는 글자가 든 문자열이 {lacking}개인데 라벨이 고정폭이다"
            );
        }
    }
}
