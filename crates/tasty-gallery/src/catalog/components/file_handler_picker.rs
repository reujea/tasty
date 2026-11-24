//! 파일 핸들러 선택 화면의 정적 예제. 추천·최근 목록, 전체 목록, 빈 상태를 보여준다.
//! 본체 뷰와 코드를 공유하지 않으며 FH_* 치수와 경로 생략 함수는 공용 크레이트에서 쓴다.
//! 선택은 한 번 열기에만 적용하고 기본 연결을 저장하지 않는다.

mod frame;

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;
use frame::{FrameState, RemoteHead, fh_footer, frame, header_card, remote_spec_frame};
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

/// 원격 경로 표본 한 줄 — 라벨, 호스트(None 이면 로컬 경로), 경로, 이름이 정한 형식, 프레임 상태.
struct RemotePathRow {
    label: &'static str,
    host: Option<&'static str>,
    path: &'static str,
    format: Option<&'static str>,
    state: FrameState,
    /// agent mirror 가 다시 붙는 중이다. 표지는 중립 잉크 + 상태 단어다.
    reconnecting: bool,
}

/// 디자인 Spec `file-handler-remote-path` 의 표본 줄, 정본 순서. 호스트 글자는 attach 프로필 이름이다.
/// 이름으로 연 경로(Ctrl+click·탐색기·파일 열기 도구·plugin)는 이름이 정한 형식의 accent Tag 이고,
/// 링크 메뉴의 열기 동작은 detector 가 없어 fallback(`format unknown`, 1 회성 띠, All handlers)이다.
const REMOTE_PATH_ROWS: [RemotePathRow; 6] = [
    RemotePathRow {
        label: "local path — no mark",
        host: None,
        path: "docs/architecture.md",
        format: None,
        state: FrameState::Default,
        reconnecting: false,
    },
    RemotePathRow {
        label: "remote path identified by name (path link Ctrl+click, mirror explorer, file open tool, plugin) — profile host, the format its name settled keeps the accent Tag · Ctrl+click opens it only when the first handler cannot open the file remotely and another can",
        host: Some("host"),
        path: "docs/architecture.md",
        format: Some("markdown"),
        state: FrameState::Default,
        reconnecting: false,
    },
    RemotePathRow {
        label: "remote path from the link menu's open action — no detector: format unknown, All handlers and the one-time strip, remote handlers only",
        host: Some("host"),
        path: "src/main.rs",
        format: None,
        state: FrameState::Fallback,
        reconnecting: false,
    },
    RemotePathRow {
        label: "remote path · agent mirror reconnecting (resync, or drop → cleanup) — neutral ink + status word, as the status bar",
        host: Some("work-laptop-attach"),
        path: "docs/architecture.md",
        format: Some("markdown"),
        state: FrameState::Default,
        reconnecting: true,
    },
    RemotePathRow {
        label: "93-char path beside work-laptop-attach (link menu) — 258px box, 43 chars: whole segments drop, the filename stays",
        host: Some("work-laptop-attach"),
        path: PATH_SAMPLES[1],
        format: None,
        state: FrameState::Fallback,
        reconnecting: false,
    },
    RemotePathRow {
        label: "same path at the host cap (link menu) — 206px box, 34 chars: …/ + the 33-char file name is 35, so only the slash drops — … + the whole file name",
        host: Some("build-runner-eu-west-07-staging-attach"),
        path: PATH_SAMPLES[1],
        format: None,
        state: FrameState::Fallback,
        reconnecting: false,
    },
];

/// 디자인 `ThemePair` — 같은 내용을 Mocha·Latte 열로 나란히(좁으면 줄을 바꿔) 그린다. 열마다 자기
/// bg-app 바탕이고, 갤러리 UI 배율을 따른다.
fn theme_pair(ui: &mut egui::Ui, theme: &Theme, id: &str, add: impl Fn(&mut egui::Ui, &Theme)) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing =
            egui::vec2(theme.spacing_lg.value(), theme.spacing_lg.value());
        for (label, th) in &themes {
            spec::wrap_item(ui, |ui| {
                egui::Frame::new()
                    .fill(th.bg_app().to_egui())
                    .corner_radius(th.corner_radius.value())
                    .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            // 열은 프레임 폭이다. 긴 라벨은 이 폭에서 줄을 바꾼다.
                            ui.set_width(FH_FRAME_WIDTH.value());
                            ui.spacing_mut().item_spacing.y = th.spacing_md.value();
                            ui.push_id((id, *label), |ui| add(ui, th));
                        });
                    });
            });
        }
    });
}

/// 표본 한 줄 — 라벨과 그 아래 프레임.
fn remote_path_row(
    ui: &mut egui::Ui,
    th: &Theme,
    row: &RemotePathRow,
    mark: Option<&tasty_ui_widgets::RemoteMark<'_>>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = th.spacing_xs.value();
        ui.add(
            egui::Label::new(
                egui::RichText::new(row.label)
                    .size(th.font_size_caption.value())
                    .color(th.text_muted().to_egui()),
            )
            .wrap(),
        );
        let head = mark.map(|mark| RemoteHead {
            mark,
            format: row.format,
        });
        remote_spec_frame(ui, th, row.path, head.as_ref(), row.state);
    });
}

pub fn draw_remote_path(ui: &mut egui::Ui, theme: &Theme) {
    let empty_label = crate::i18n::t("attach.mirror_pill_label");
    let word = crate::i18n::t("status_bar.remote_reconnecting");
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        theme_pair(ui, theme, "fh_remote_path", |ui, th| {
            for row in &REMOTE_PATH_ROWS {
                let mark = row.host.map(|host| tasty_ui_widgets::RemoteMark {
                    host,
                    empty_label,
                    reconnecting: row.reconnecting.then_some(word),
                });
                remote_path_row(ui, th, row, mark.as_ref());
            }
        });
        // 디자인의 숨은 View `file-handler-remote-no-host`. 카탈로그 Spec 에 View 칸이 없어 같은 Spec
        // 아래에 라벨을 달아 둔다.
        let no_host = RemotePathRow {
            label: "View · file-handler-remote-no-host — no profile name (--ssh or --target-port from the CLI or IPC): the word Remote in the host slot, label font, remote ink",
            host: Some(""),
            path: "docs/architecture.md",
            format: Some("markdown"),
            state: FrameState::Default,
            reconnecting: false,
        };
        theme_pair(ui, theme, "fh_remote_no_host", |ui, th| {
            let mark = tasty_ui_widgets::RemoteMark {
                host: "",
                empty_label,
                reconnecting: None,
            };
            remote_path_row(ui, th, &no_host, Some(&mark));
        });
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
            (
                "ink",
                "fh-remote-fg → accent-remote-raised · base 10.54 · 6.63",
            ),
            (
                "gap",
                "glyph ↔ host fh-remote-gap 4 · mark ↔ path space-sm 8",
            ),
            (
                "host cap",
                "fh-remote-max-width 160 on the host text (glyph and gap outside it) · end-elided with …",
            ),
            (
                "host text",
                "the host text rule: attach profile name, as it was at connect time (R-57) · set once when the picker opens, unchanged after a drop · no profile name (--ssh or --target-port from the CLI or IPC, a profile saved with an empty name) → the word Remote (next row) · never 127.0.0.1:<port>, the local end of the tunnel",
            ),
            (
                "no host",
                "attach.mirror_pill_label in the host slot · font-label (mono in en and ko, UI sans in ja and zh-Hans) · fh-remote-fg, glyph unchanged · never elides · for an attach with no profile name: --ssh (whatever its value, 127.0.0.1:<port> included) or --target-port from the CLI or IPC, a profile saved with an empty name · the app draws the same word for any empty host, also one from missing connection info (before the first location sync) — whether a picker opens in that span is not settled",
            ),
            (
                "no-host path box",
                "390 − (12 + 4 + word) − 8, measured · when the measure is not valid or not one character fits, the common cap 34, the same as an unknown host width — no per-language estimate",
            ),
            (
                "widths",
                "6px advance: 'host' 24 · 'work-laptop-attach' 108 · cap 160",
            ),
            ("path box", "390 − mark − 8; mark = 12 + 4 + host text"),
            (
                "path measure",
                "front elision against the path box: 1 whole leading segments drop, prefix …/ · 2 when …/ + the last segment does not fit but … + it does, drop the slash · 3 only when … + the last segment does not fit is the segment cut at its front by characters, prefix …",
            ),
            (
                "path cap, unmeasured",
                "floor((366 − min(6 × host chars, 160)) / 6): 'host' 57 · 'work-laptop-attach' 43 · at the cap 34 (also when the host width is unknown) · reconnecting: also subtract 4 + the status word's width from 366; when its width is not known, take 6 per mono cell in en and ko (a Hangul syllable is two cells: reconnecting 72, 재연결 중 54) and 12 per character in ja and zh-Hans, one 11px em rounded up (再接続中 48, 重新连接中 60)",
            ),
            (
                "reconnecting",
                "glyph glyph-dim · host text-muted · then status_bar.remote_reconnecting in font-label, text-muted · gap 4 · no spinner, no pulse — the status bar's reconnecting form",
            ),
            (
                "reconnecting path box",
                "the word is part of the mark: 390 − (12 + 4 + host + 4 + word) − 8, measured — the word is font-label, mono in en and ko but UI sans in ja and zh-Hans, so a character count does not give its width",
            ),
            (
                "state",
                "agent mirror: follows its session state while the picker is open · saved remote row that drops while it is open, into reconnecting too: the picker closes, is not left under the scrim and does not open again when the row connects again (decided, Q1-a)",
            ),
            (
                "disconnected",
                "no form (user decision, R-58 Q1: the scrim takes the pointer, drawn on Layouts › Saved remote workspace) — on a saved remote row that is not connected, reconnecting included, nothing under the scrim answers, so no link opens the picker, so it has no form here · an agent mirror is closed once it drops (R-58 decision 23-4), so it has no disconnected form",
            ),
            (
                "reconnecting form",
                "the agent mirror's, with no scrim: while it attaches again after a resync (whatever its anchor or origin), and in the short span between its drop and its cleanup · both short, not measured",
            ),
            (
                "format Tag",
                "identified by name (path link Ctrl+click, mirror explorer, file open tool, plugin) → Tag accent, the format its extension or file name settled, beside the host mark, over the remote handler list · the link menu's open action → no detector, so the fallback: the All handlers group and the one-time strip, over the remote handlers only, Tag format unknown (default, outline)",
            ),
            (
                "Ctrl+click opens",
                "the picker only when the first handler cannot open the file on the remote host and another candidate can, so its detector has always matched — no match → the download-and-run dialog, no picker · the first handler can open it remotely → it runs, no picker",
            ),
            ("local path", "no mark — the line is the path alone"),
            (
                "body",
                "the handler list, never the empty state — a remote path with no remote handler does not open the picker · every user route (path link, open by name) opens the download-and-run dialog instead · an outside request (CLI, agent) opens nothing and is logged",
            ),
            (
                "rows",
                "only the handlers that can open the file on the remote host (the app filters them) · no default Tag on any row — the app sets no default for a remote target",
            ),
            (
                "copy",
                "none new — the status word reuses status_bar.remote_reconnecting, the no-host word attach.mirror_pill_label",
            ),
        ],
        &[
            TokenChip::new("fh-remote-fg", "host mark", theme.fh_remote_fg().to_egui()),
            TokenChip::without_color("fh-remote-gap", "glyph ↔ host ↔ word"),
            TokenChip::new(
                "glyph-dim",
                "glyph, reconnecting",
                theme.glyph_dim().to_egui(),
            ),
            TokenChip::without_color("fh-remote-max-width", "host cap"),
            TokenChip::new(
                "text-muted",
                "path ink · host + word, reconnecting",
                theme.text_muted().to_egui(),
            ),
        ],
    );
    spec::dont(
        ui,
        theme,
        "Don't join host and path into one string (host:/src/main.rs). The path line elides at \
         the front, so the host would be the first thing cut. The title row is not the place either: \
         it already holds the format Tag, and a host there is not attached to the path it qualifies.",
    );
    spec::note(
        ui,
        theme,
        "The File Picker keeps its tinted host badge in the title row: there the host is the \
         context for a whole browsing window, here it qualifies one path.",
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
                theme.accent_attention_raised().to_egui(),
            ),
            TokenChip::new(
                "surface-raised",
                "notice strip",
                theme.surface_raised().to_egui(),
            ),
            TokenChip::new(
                "text-secondary",
                "strip copy",
                theme.text_secondary().to_egui(),
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

    /// 원격 경로 표본의 긴 경로 두 줄이 라벨대로 줄어든다 — 앱 글꼴(D2Coding)로 잰 표지가 남긴 상자에서
    /// work-laptop-attach 옆 상자는 43자(앞 조각을 통째로 버림), 상한 호스트 옆 상자는 34자(슬래시만 빠짐)다.
    #[test]
    fn the_remote_long_path_rows_cut_as_their_labels_say() {
        let ctx = egui::Context::default();
        crate::fonts::install(&ctx);
        let theme = crate::host_shell::latte_theme();
        let mut cuts = Vec::new();
        let _frame = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let font = egui::FontId::monospace(theme.font_size_caption.value());
                let w = |t: &str| {
                    ui.fonts(|f| {
                        f.layout_no_wrap(t.into(), font.clone(), egui::Color32::PLACEHOLDER)
                            .rect
                            .width()
                    })
                };
                let advance = tasty_type_geometry::length::LogicalPx(w("00") - w("0"));
                for row in &REMOTE_PATH_ROWS[4..] {
                    let mark = tasty_ui_widgets::RemoteMark {
                        host: row.host.expect("remote row"),
                        empty_label: crate::i18n::t("attach.mirror_pill_label"),
                        reconnecting: None,
                    };
                    let budget =
                        tasty_ui_widgets::fh_remote_path_budget(ui, &theme, &mark, advance, "en");
                    cuts.push((budget, fh_model::elide_target_front(row.path, budget)));
                }
            });
        });
        // 정본 표본의 생략 결과(overlays-dialogs 의 긴 경로 두 줄).
        assert_eq!(
            cuts,
            vec![
                (43, "…/SidebarCategoryHeaderContrast.tsx".to_owned()),
                (34, "…SidebarCategoryHeaderContrast.tsx".to_owned()),
            ]
        );
    }
}
