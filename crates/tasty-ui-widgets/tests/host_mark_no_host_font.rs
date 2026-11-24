//! 프로필 이름이 없는 attach(빈 호스트)의 탐색기 원격 호스트 표지가 낱말을 `font-label`로 그리는지 본다.
//! en에서는 `font-label` 가족이 고정폭이라 이름 글꼴과 구별되지 않으므로, 가족이 갈리는 ja로 이 시험
//! 파일을 새 프로세스에서 다시 실행한다(`font-label` 조회는 프로세스 환경을 한 번 읽어 캐시한다).

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{HostMark, OpStatusProps, QueueHeader, op_queue_header, op_status_line};

const PROBE: &str = "HOST_MARK_NO_HOST_FONT_PROBE";

/// 진행 줄과 대기열 머리를 실제 호출로 그리고, 칠한 글자마다 (문자열, 글꼴 가족)을 모은다.
fn painted(name: &str) -> Vec<(String, egui::FontFamily)> {
    let theme = Theme::with_colors_and_zoom(tasty_themes::mocha_fallback_colors(), false, 1.0);
    let mark = HostMark {
        name,
        empty_label: "Remote",
        paint_glyph: &|_, _, _| {},
    };
    let ctx = egui::Context::default();
    let mut out = Vec::new();
    // 첫 pass에서 글꼴이 준비되므로 두 번 돈다.
    for _ in 0..2 {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (line, _) =
                    ui.allocate_exact_size(egui::vec2(600.0, 28.0), egui::Sense::hover());
                op_status_line(
                    ui,
                    &theme,
                    line,
                    &OpStatusProps {
                        show_bar: true,
                        fraction: Some(0.5),
                        waiting: false,
                        text: "Copying",
                        bytes: None,
                        queued: None,
                        show_label: "Show",
                        cancel_tip: "Cancel",
                        host: Some(mark),
                    },
                    &|_, _, _| {},
                );
                ui.allocate_ui(egui::vec2(288.0, 40.0), |ui| {
                    op_queue_header(
                        ui,
                        &theme,
                        &QueueHeader {
                            before: "On",
                            host: mark,
                            after: "· one at a time",
                        },
                    );
                });
            });
        });
        out = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Remote" || t.galley.text() == name => {
                    let font = &t.galley.job.sections.first()?.format.font_id;
                    Some((t.galley.text().to_owned(), font.family.clone()))
                }
                _ => None,
            })
            .collect();
    }
    out
}

#[test]
fn the_no_host_word_uses_the_label_font_in_the_progress_line_and_the_queue_header() {
    if std::env::var(PROBE).is_ok() {
        let label = egui::FontFamily::Name(egui::epaint::text::MONO_LINE_SANS_FAMILY.into());
        assert_eq!(tasty_ui_widgets::label_font_family(), label);
        // 진행 줄과 대기열 머리 두 곳 모두 낱말을 label 가족으로 그린다.
        assert_eq!(painted(""), vec![("Remote".to_owned(), label.clone()); 2]);
        // 이름이 있으면 두 곳 모두 고정폭 이름이다.
        assert_eq!(
            painted("build-eu"),
            vec![("build-eu".to_owned(), egui::FontFamily::Monospace); 2]
        );
        return;
    }
    let exe = std::env::current_exe().expect("test binary path");
    let out = std::process::Command::new(&exe)
        .args([
            "--exact",
            "the_no_host_word_uses_the_label_font_in_the_progress_line_and_the_queue_header",
        ])
        .env(PROBE, "1")
        .env("TASTY_LOCALE", "ja")
        .output()
        .expect("run the test binary again");
    assert!(
        out.status.success(),
        "TASTY_LOCALE=ja: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
