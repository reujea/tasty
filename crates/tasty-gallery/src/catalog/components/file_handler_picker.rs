//! 파일 핸들러 선택 화면의 정적 예제. 추천·최근 목록, 전체 목록, 빈 상태를 보여준다.
//! 본체 뷰와 코드를 공유하지 않으며 FH_* 치수와 경로 생략 함수는 공용 크레이트에서 쓴다.
//! 선택은 한 번 열기에만 적용하고 기본 연결을 저장하지 않는다.

mod frame;

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use frame::{FrameState, RemoteHead, fh_footer, frame, header_card, remote_frame};
use tasty_ui_widgets::file_handler as fh_model;
use tasty_ui_widgets::tokens::FH_FRAME_WIDTH;

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Default, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("width", "420px"),
            ("header", "title + format Tag · path (mono 11)"),
            ("rows", "icon · name · origin"),
            ("default", "Tag accent"),
            ("selected", "2px accent left bar"),
            ("footer", "Cancel (ghost) · Open (primary)"),
        ],
        &[
            TokenChip::new(
                "surface-active",
                "selected",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "select bar",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "accent-agent",
                "plugin handler",
                theme.accent_agent().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Plugin handlers are marked with the mauve agent accent — you always know a third \
         party is handling the open.",
    );
}

pub fn draw_format(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "detected", |ui| {
            frame(ui, theme, FrameState::Default, true);
        });
        spec::cluster(ui, theme, "not detected → fallback", |ui| {
            frame(ui, theme, FrameState::Fallback, true);
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("place", "title row, right-aligned"),
            ("detected", "Tag accent"),
            ("unknown", "Tag default (outline)"),
            ("path", "mono 11 · text-muted"),
            (
                "long path",
                "elide at the FRONT in the model, then render LTR",
            ),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "format Tag",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::without_color("font-mono", "path"),
            TokenChip::new("text-muted", "path ink", theme.text_muted().to_egui()),
        ],
    );
    spec::do_(
        ui,
        theme,
        "Do cut long paths at the front — the filename is the tail and the tail identifies \
         the file. Elide in the model (drop leading segments, prefix …/) and render the \
         result left-to-right.",
    );
}

pub fn draw_recent(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Recent, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("groups", "Suggested → rule → Recent"),
            ("group label", "11px uppercase, text-secondary"),
            ("count", "mono, next to the label"),
            ("caption", "11px muted, one line"),
            ("recent meta", "origin · relative time"),
            ("selection", "one row across BOTH groups"),
        ],
        &[
            TokenChip::new(
                "separator",
                "group rule",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "text-secondary",
                "group label",
                theme.text_secondary().to_egui(),
            ),
            TokenChip::new("text-muted", "caption / time", theme.text_muted().to_egui()),
            TokenChip::without_color("fh-when-width", "when column reserve"),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't split the frame into two side-by-side lists. Two 200px columns lose the origin \
         line, and the eye has to compare across a gutter to pick one thing.",
    );
}

pub fn draw_when(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Recent, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("< 60 s", "just now"),
            ("< 60 min", "{n}m ago"),
            ("< 24 h", "{n}h ago"),
            ("24–48 h", "yesterday"),
            ("2–7 d", "{n}d ago — ceiling at 7"),
            ("≥ 7 d", "YYYY-MM-DD — no phrase key"),
            ("n", "integer floor, never rounded up"),
            ("column", "reserved at fh-when-width (56px)"),
            ("recompute", "every frame — midnight flips it live"),
        ],
        &[
            TokenChip::new("text-muted", "the whole slot", theme.text_muted().to_egui()),
            TokenChip::without_color("fh-when-width", "when column"),
        ],
    );
    spec::note(
        ui,
        theme,
        "The date is deliberately not a translated phrase. Past a week the number stops helping \
         you choose a handler, and a bare YYYY-MM-DD is bounded at ten characters, reads the same \
         in every locale, and has no plural rule to get wrong. That bound is what the column \
         reserves — which is why the id beside it never reflows when a row crosses a boundary.",
    );
    spec::dont(
        ui,
        theme,
        "Don't add {n}w ago or {n}mo ago, and don't shrink or drop the time to make room. A \
         partially cut timestamp reads as a different timestamp; the id is the piece that gives \
         way, and it already elides from the front.",
    );
}

pub fn draw_path_cut(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (label, raw) in [
            "65 chars — fits exactly, no cut",
            "93 → 57 chars — leading segments dropped at a boundary",
            "one 72-char segment → 65 — mid-segment, front",
        ]
        .into_iter()
        .zip(PATH_SAMPLES)
        {
            spec::cluster(ui, theme, label, |ui| {
                header_card(ui, theme, &fh_model::elide_target_front(raw, PATH_BUDGET));
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            ("line box", "420 − 1×2 border − 14×2 pad = 390px"),
            ("cell", "6px laid out — 5.5556px nominal, rounded up"),
            ("budget", "measured: 390px ÷ one laid-out cell"),
            ("fallback", "65 chars, only when it cannot measure"),
            ("first rule", "drop a whole leading segment, prefix …/"),
            (
                "second rule",
                "characters — only if one segment is too long",
            ),
            ("direction", "front, because the filename is the tail"),
        ],
        &[TokenChip::new(
            "text-muted",
            "mono path line",
            theme.text_muted().to_egui(),
        )],
    );
    spec::do_(
        ui,
        theme,
        "The host measures the available width with the actual font layout. A 65-character fallback corresponds to a 390px line and 6px cells with D2Coding at 11px. Other font metrics can produce a different measured limit.",
    );
    spec::dont(
        ui,
        theme,
        "Don't cut inside a directory name to save two characters. A half-written segment reads \
         as a directory that does not exist, which is worse than one fewer level of context.",
    );
}

/// 경로를 미리 잘라 저장하지 않고 공용 생략 함수에 넣어 비교한다.
/// 시안은 B·C 의 생략 결과만 보여 주므로, 원본은 그 결과와 라벨의 원래 글자 수(93·72)에 맞춰 앞 조각을 채운 것이다.
const PATH_SAMPLES: [&str; 3] = [
    "packages/design-system/src/components/navigation/federated/Bar.ts",
    "packages/design-system/src/components/navigation/federation/SidebarCategoryHeaderContrast.tsx",
    "navigation-federation-sidebar-category-header-contrast-exploration-v2.md",
];

/// 이 예제는 글꼴을 실측하지 않고 측정할 수 없을 때의 문자 수 상한을 사용한다.
const PATH_BUDGET: usize = tasty_ui_widgets::tokens::FH_TARGET_ELIDE_FALLBACK;

/// 원격 경로 표본 — 디자인 Spec `file-handler-remote-path` 의 줄. 호스트 글자는 attach 프로필 이름이다.
/// 넷째 칸은 이름으로 연 경로가 정한 형식이다(경로 링크는 None → `format unknown`).
const REMOTE_PATH_ROWS: [(&str, &str, &str, Option<&str>); 4] = [
    ("remote path — profile gx10", "gx10", "src/main.rs", None),
    (
        "remote path opened by name (plugin, mirror file open) — the format its name settled keeps the accent Tag",
        "gx10",
        "docs/architecture.md",
        Some("markdown"),
    ),
    (
        "93-char path beside mac-company-attach — whole segments drop, the filename stays",
        "mac-company-attach",
        PATH_SAMPLES[1],
        None,
    ),
    (
        "same path at the host cap — the one segment is cut at its front",
        "build-runner-eu-west-07-staging-attach",
        PATH_SAMPLES[1],
        None,
    ),
];

pub fn draw_remote_path(ui: &mut egui::Ui, theme: &Theme) {
    let empty_label = crate::i18n::t("attach.mirror_pill_label");
    let row = |ui: &mut egui::Ui, (label, host, path, format): (&str, &str, &str, Option<&str>)| {
        spec::cluster(ui, theme, label, |ui| {
            remote_frame(
                ui,
                theme,
                path,
                &RemoteHead {
                    mark: &tasty_ui_widgets::RemoteMark { host, empty_label },
                    format,
                },
            );
        });
    };
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "local path — no mark", |ui| {
            header_card(ui, theme, "docs/architecture.md");
        });
        for r in REMOTE_PATH_ROWS {
            row(ui, r);
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "where",
                "path line, before the path — never in the title row",
            ),
            ("mark", "remote glyph 12 + host · mono caption · no fill"),
            ("ink", "fh-remote-fg → accent-remote-raised"),
            (
                "gap",
                "glyph ↔ host fh-remote-gap 4 · mark ↔ path space-sm 8",
            ),
            (
                "host cap",
                "fh-remote-max-width 160 on the host text · end-elided with …",
            ),
            (
                "host text",
                "attach profile name, as it was at connect time",
            ),
            ("path box", "390 − mark − 8; mark = 12 + 4 + host text"),
            (
                "path measure",
                "front elision against the path box, at a segment boundary",
            ),
            ("path cap, unmeasured", "34 — the box left at the host cap"),
            (
                "no host text",
                "the word Remote (attach.mirror_pill_label) in the host slot, font-label",
            ),
            ("local path", "no mark — the line is the path alone"),
            (
                "format Tag",
                "remote target from a path link → format unknown, Tag default (outline) · remote target opened by name → Tag accent, the format its name settled",
            ),
            (
                "body",
                "the remote handler list, never the empty state (no All handlers group, no one-time strip) — with no remote handler the picker does not open and the download-and-run dialog opens instead",
            ),
        ],
        &[
            TokenChip::new("fh-remote-fg", "host mark", theme.fh_remote_fg().to_egui()),
            TokenChip::without_color("fh-remote-gap", "glyph ↔ host"),
            TokenChip::without_color("fh-remote-max-width", "host cap"),
            TokenChip::new("text-muted", "path ink", theme.text_muted().to_egui()),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't join host and path into one string (gx10:/src/main.rs). The path line elides at \
         the front, so the host would be the first thing cut.",
    );
}

pub fn draw_fallback(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Fallback, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("label", "All handlers · accent-attention"),
            ("strip", "8/14 on surface-raised"),
            ("strip icon", "alertTriangle, attention"),
            ("promise", "one-time dispatch, no registration"),
        ],
        &[
            TokenChip::new(
                "accent-attention",
                "fallback tone",
                theme.accent_attention().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "notice strip",
                theme.surface_raised().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Attention-peach, not warning-yellow: nothing is wrong, the system just has no opinion.",
    );
}

pub fn draw_empty(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Empty, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("block", "32/14 padding, centered, 8px gap"),
            ("glyph", "text-disabled"),
            ("action", "Button secondary sm → Settings › Handlers"),
            ("Open", "disabled"),
        ],
        &[
            TokenChip::new(
                "text-disabled",
                "empty glyph",
                theme.text_disabled().to_egui(),
            ),
            TokenChip::new("text-muted", "instruction", theme.text_muted().to_egui()),
        ],
    );
    spec::do_(
        ui,
        theme,
        "Do keep the frame the same width and the footer intact — the empty state is the same \
         dialog, not a different one.",
    );
}

pub fn draw_long(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Long, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("list max-height", "264px"),
            ("overflow", "auto, list area only"),
            ("fade", "20px → bg-panel"),
            ("count", "mono, in the group label"),
        ],
        &[
            TokenChip::new("bg-panel", "fade target", theme.bg_panel().to_egui()),
            TokenChip::new(
                "separator",
                "header / footer rules",
                theme.separator.to_egui_premultiplied(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Header and footer never scroll — the path, the format, and the actions stay put.",
    );
}

pub fn draw_headless(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "headless — one header", |ui| {
            frame(ui, theme, FrameState::Default, true);
        });
        spec::cluster(ui, theme, "titlebar + header — path twice", |ui| {
            frame(ui, theme, FrameState::Long, false);
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("mode", "headless: true"),
            ("header owner", "the frame"),
            ("dismiss", "Esc · Cancel · titlebar X — not scrim click"),
            ("precedent", "Transfer progress"),
        ],
        &[
            TokenChip::new(
                "bg-sidebar",
                "titlebar fill (the rejected option)",
                theme.bg_sidebar().to_egui(),
            ),
            TokenChip::new(
                "separator",
                "header rule",
                theme.separator.to_egui_premultiplied(),
            ),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't pair the common titlebar with an in-frame header. The path gets cut two \
         different ways in one dialog.",
    );
}

pub fn draw_footer(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card(ui, theme, FH_FRAME_WIDTH, kit::panel_fill(theme), |ui| {
            fh_footer(ui, theme, true);
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("footer", "Cancel (ghost) · Open (primary)"),
            ("persistence", "none — dispatch is one-time"),
            ("default binding", "lives in Settings › Handlers, set there"),
            ("Open", "disabled until a row is selected"),
        ],
        &[
            TokenChip::new(
                "separator",
                "footer rule",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new("accent-primary", "Open", theme.accent_primary().to_egui()),
        ],
    );
    spec::note(
        ui,
        theme,
        "The picker is a pure dispatcher: it opens one file, one time, and stores nothing. \
         If a persistent “always open with” ever ships, it belongs in Settings › Handlers as a \
         visible, removable row — not as a checkbox on a transient dialog.",
    );
}

pub fn draw_rows(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        frame(ui, theme, FrameState::Mixed, true);
    });
    spec::meta(
        ui,
        theme,
        &[
            ("icon", "from action surface kind → icons.json"),
            ("icon fallback", "file"),
            ("name", "id segment after the last “/”"),
            ("name (raw id)", "mono — signals “no declared name”"),
            ("id line", "origin · id (mono 11), front-elided at 34 chars"),
            ("origin words", "built-in · you · plugin"),
            ("plugin", "mauve accent-agent on the origin word + glyph"),
        ],
        &[
            TokenChip::new(
                "accent-agent",
                "plugin origin",
                theme.accent_agent().to_egui(),
            ),
            TokenChip::without_color("font-mono", "id + undeclared names"),
            TokenChip::new("text-muted", "id line", theme.text_muted().to_egui()),
        ],
    );
    spec::note(
        ui,
        theme,
        "The mauve stays on the origin word and the glyph, not on the name — a plugin's \
         handler is still named after what it does.",
    );
}

pub fn draw_default_tag(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(
            ui,
            theme,
            "ambiguous / explicit — default Tag shown",
            |ui| {
                frame(ui, theme, FrameState::Default, true);
            },
        );
        spec::cluster(ui, theme, "no match — no default", |ui| {
            frame(ui, theme, FrameState::Fallback, true);
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("single click", "select, frame stays"),
            ("double click", "select + open"),
            ("no selection", "Open disabled"),
            ("selection scope", "one row across all groups"),
            ("dismiss", "Esc · Cancel · titlebar X"),
            ("scrim click", "does NOT close"),
        ],
        &[
            TokenChip::new(
                "accent-primary",
                "default Tag fill",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "text-on-accent",
                "default Tag ink",
                theme.text_on_accent().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "Keep the Tag. It answers “what happens if I just press Enter” — the question the \
         picker exists to ask.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 예제 라벨에 적힌 원본·생략 결과의 문자 수를 실제 함수 결과와 비교한다.
    #[test]
    fn the_path_cut_labels_describe_what_the_specimen_draws() {
        let [fits, segment, one_piece] = PATH_SAMPLES;

        assert_eq!(fits.chars().count(), PATH_BUDGET);
        assert_eq!(fh_model::elide_target_front(fits, PATH_BUDGET), fits);

        assert_eq!(segment.chars().count(), 93);
        let cut = fh_model::elide_target_front(segment, PATH_BUDGET);
        assert_eq!(cut.chars().count(), 57);
        assert_eq!(
            cut,
            "…/navigation/federation/SidebarCategoryHeaderContrast.tsx"
        );

        assert_eq!(one_piece.chars().count(), 72);
        assert!(
            !one_piece.contains('/'),
            "한 조각이어야 문자 컷 갈래로 간다"
        );
        let cut = fh_model::elide_target_front(one_piece, PATH_BUDGET);
        assert_eq!(cut.chars().count(), 65);
        assert_eq!(
            cut,
            "…on-federation-sidebar-category-header-contrast-exploration-v2.md"
        );
    }
}
