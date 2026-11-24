//! 원격 File Picker 머리 줄의 호스트 배지가 그리는 글자와 글꼴.
use super::*;

/// `host` 배지를 한 프레임 그리고 칠한 글자마다 (문자열, 글꼴)을 모은다.
fn painted_badge(host: &str) -> Vec<(String, egui::FontId)> {
    let th = crate::theme::theme();
    let ctx = egui::Context::default();
    // 글리프는 SVG 그림이다. 로더가 없으면 글리프 자리에 경고 글자가 칠해진다.
    egui_extras::install_image_loaders(&ctx);
    let mut out = Vec::new();
    // 첫 프레임은 폰트가 확정되지 않아 galley 가 비는 경우가 있어 두 번 돈다.
    for _ in 0..2 {
        let full = ctx.run(egui::RawInput::default(), |ctx| {
            egui::Area::new(egui::Id::new("fp_host_badge_test"))
                // 등장 페이드가 색을 반투명하게 바꾸지 않도록 끈다.
                .fade_in(false)
                .show(ctx, |ui| host_badge(ui, &th, host));
        });
        out = full
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => {
                    let font = text.galley.job.sections.first()?.format.font_id.clone();
                    Some((text.galley.text().to_owned(), font))
                }
                _ => None,
            })
            .collect();
    }
    out
}

/// 프로필 이름이 없는 attach(빈 호스트)는 글리프만 남기지 않고 `attach.mirror_pill_label` 낱말을 label 글꼴로 그린다.
/// 호스트 이름은 지금처럼 mono 로 그린다.
#[test]
fn the_host_badge_draws_the_no_host_word_in_the_label_font() {
    let caption = crate::theme::theme().font_size_caption.value();
    let word = t("attach.mirror_pill_label").to_string();
    assert!(!word.is_empty());
    assert_eq!(
        painted_badge(""),
        vec![(word, tasty_ui_widgets::label_font(caption))]
    );
    assert_eq!(
        painted_badge("work-laptop-attach"),
        vec![(
            "work-laptop-attach".to_owned(),
            egui::FontId::monospace(caption)
        )]
    );
}

const JA_PROBE: &str = "FILE_PICKER_HOST_BADGE_JA_PROBE";

/// en에서는 label 글꼴 가족이 고정폭이라 빈 호스트 낱말을 mono로 되돌려도 위 시험이 지나간다. 가족이 갈리는 ja로
/// 이 시험을 새 프로세스에서 다시 실행해 낱말이 label 가족(UI sans)이고 이름은 고정폭인지 본다
/// (label 글꼴 조회는 프로세스 환경을 한 번 읽어 캐시한다).
#[test]
fn the_host_badge_no_host_word_takes_the_ja_label_family() {
    if std::env::var(JA_PROBE).is_ok() {
        let sans = egui::FontFamily::Name(egui::epaint::text::MONO_LINE_SANS_FAMILY.into());
        assert_eq!(tasty_ui_widgets::label_font_family(), sans);
        let word = painted_badge("");
        assert_eq!(word.len(), 1, "{word:?}");
        assert_eq!(word[0].1.family, sans);
        let named = painted_badge("work-laptop-attach");
        assert_eq!(named.len(), 1, "{named:?}");
        assert_eq!(named[0].1.family, egui::FontFamily::Monospace);
        return;
    }
    let exe = std::env::current_exe().expect("test binary path");
    let out = std::process::Command::new(&exe)
        .args([
            "--exact",
            "adapters::ui::popup::file_picker::host_badge_tests::the_host_badge_no_host_word_takes_the_ja_label_family",
        ])
        .env(JA_PROBE, "1")
        .env("TASTY_LOCALE", "ja")
        .output()
        .expect("run the test binary again");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "TASTY_LOCALE=ja: {stdout}");
    // 다시 실행한 프로세스가 이 시험 하나를 실제로 돌렸는지 본다(이름이 어긋나면 0개가 돌고 성공한다).
    assert!(stdout.contains("1 passed"), "TASTY_LOCALE=ja: {stdout}");
}
