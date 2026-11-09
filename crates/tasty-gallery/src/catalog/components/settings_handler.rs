//! 확장자 연결, 파일 감지기, 파일 핸들러, 훅 설정의 예제.
//! 실제 레지스트리를 읽지 않고 준비된 데이터로 표시한다.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, Ground, IconButton, IconButtonVariant, Input, TagVariant,
    switch, tag,
};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

/// 디자인 settings 콘텐츠 컬럼(1100 - L2 200 - 패딩) 근사 프레임 폭.
const WIDTH: LogicalPx = LogicalPx(560.0);
/// jsx `HookRow` line 2 "Shell cmd:" 라벨 폭 (`width: 74`).
const HOOK_CMD_LABEL_W: LogicalPx = LogicalPx(74.0);
/// jsx add-draft 카드 필드 라벨 폭 (`width: 100`).
const HOOK_ADD_LABEL_W: LogicalPx = LogicalPx(100.0);

/// jsx `Mono` — font-label 10 uppercase letter-spacing caps, text-muted.
fn mono_head(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .family(tasty_ui_widgets::label_font_family())
            .size(theme.font_size_micro.value())
            .extra_letter_spacing(theme.letter_spacing_caps(theme.font_size_micro).value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 행 하단 1px separator (jsx `borderBottom`).
fn row_separator(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
}

/// 시안 Spec 패널의 바깥 폭 `--tasty-size-360`(시안은 border-box라 padding·border 포함).
/// 공개 역할 토큰이 없어 갤러리 무대 치수로 둔다.
const EXT_PANEL_WIDTH: LogicalPx = LogicalPx(360.0);

/// 시안 `ExtMapG` seed의 `.md` detector 순서 — 먼저 맞는 detector가 이긴다.
const EXT_MD_ROWS: &[(&str, bool)] = &[
    ("Markdown viewer", true),
    ("Editor", true),
    ("html-preview", false),
];
const EXT_LOG_ROWS: &[(&str, bool)] = &[("Log viewer", true)];

/// 시안 `ExtMapG` prop — `custom`은 `.md`에 사용자 순서가 있어 Reset이 보이는 갈래,
/// `missing`은 미설치 `.ipynb` 묶음을 더하는 갈래, `long`은 긴 번역 문구 갈래다.
/// `pending`은 시안 `pendingRemove`·`pendingReset` — Remove·Reset을 누른 뒤 Save 전 초안 상태다.
#[derive(Clone, Copy, Default)]
struct ExtMapSeed {
    custom: bool,
    missing: bool,
    long: bool,
    pending: bool,
}

impl ExtMapSeed {
    /// (Reset, Remove, not installed) 문구. `long`은 시안의 긴 번역 표본이다.
    fn labels(self) -> (&'static str, &'static str, &'static str) {
        if self.long {
            (
                "Restablecer orden",
                "Entfernen",
                "インストールされていません",
            )
        } else {
            ("Reset", "Remove", "not installed")
        }
    }

    /// (Undo, removed on save, reset on save) 문구 — 시안 `P`.
    fn pending_labels(self) -> (&'static str, &'static str, &'static str) {
        if self.long {
            ("Rückgängig", "保存時に削除", "Se restablece al guardar")
        } else {
            ("Undo", "removed on save", "reset on save")
        }
    }
}

/// 확장자 머리줄 한 줄의 내용.
#[derive(Clone, Copy, Default)]
struct ExtHeader<'a> {
    /// 오른쪽 끝 ghost Button sm 의 문구(Reset · Remove · Undo).
    button: Option<&'a str>,
    /// `.ext` 뒤 Tag disabled 의 문구(not installed · removed on save · reset on save).
    tag: Option<&'a str>,
    /// 켜진 detector 가 없다 — `.ext` text-disabled · 아래 구분선.
    not_installed: bool,
    /// Save 하면 지워진다 — `.ext` 취소선(색은 그대로).
    struck: bool,
}

thread_local! {
    // 여섯 패널 짝(빈 입력 · ".toml" · custom+missing · custom+missing+long · pending · pending+long)의
    // Mocha·Latte 입력 버퍼.
    static EXT_DRAFTS: RefCell<[String; 12]> = RefCell::new([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        ".toml".to_string(),
        ".toml".to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ]);
}

/// 확장자 머리줄 — `.ext` · (Tag disabled) · 빈 칸 · 오른쪽 끝 ghost Button sm.
/// 위·아래 space-xs 여백 안의 높이는 button-height-sm 이상이라 Reset이 나타나도 행이 움직이지 않는다.
/// 미설치 묶음은 머리줄만 남고 그 아래에 구분선 하나를 둔다.
fn ext_group_header(ui: &mut egui::Ui, theme: &Theme, ext: &str, head: ExtHeader<'_>) {
    let head_y = theme.spacing_xs.value().round() as i8;
    let resp = egui::Frame::new()
        .inner_margin(egui::Margin {
            top: head_y,
            bottom: head_y,
            ..egui::Margin::ZERO
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(theme.button_height_sm().value());
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                let ext_fg = if head.not_installed {
                    theme.text_muted()
                } else {
                    theme.text_secondary()
                };
                let mut label = egui::RichText::new(ext)
                    .monospace()
                    .size(theme.font_size_caption.value())
                    .color(ext_fg.to_egui());
                if head.struck {
                    label = label.strikethrough();
                }
                ui.label(label);
                if let Some(tag_label) = head.tag {
                    tag(ui, theme, tag_label, TagVariant::Default, false);
                }
                if let Some(label) = head.button {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        Button::new(label)
                            .variant(ButtonVariant::Ghost)
                            .size(ControlSize::Sm)
                            .show(ui, theme);
                    });
                }
            });
        })
        .response;
    if head.not_installed {
        row_separator(ui, theme, resp.rect);
    }
}

/// 확장자 아래 detector 한 행 — 순번 · 이름 · (비후보면 Tag disabled "off") · ▲ · ▼.
/// ▲▼는 숨기지 않고 disabled로 두어 행마다 같은 자리를 지킨다.
fn ext_detector_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    index: usize,
    name: &str,
    candidate: bool,
    last_candidate: bool,
) {
    let resp = ui.horizontal(|ui| {
        ui.set_min_height(theme.settings_row_min_height().value());
        ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
        let (num_rect, _) = ui.allocate_exact_size(
            egui::vec2(theme.spacing_lg.value(), theme.font_size_caption.value()),
            egui::Sense::hover(),
        );
        ui.painter().text(
            num_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            (index + 1).to_string(),
            egui::FontId::monospace(theme.font_size_caption.value()),
            theme.text_muted().to_egui(),
        );
        let name_fg = if candidate {
            theme.text_secondary()
        } else {
            theme.text_muted()
        };
        ui.label(
            egui::RichText::new(name)
                .size(theme.font_size_body.value())
                .color(name_fg.to_egui()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
            IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .enabled(candidate && !last_candidate)
                .show(ui, theme, &|ui, rect, c| {
                    icons::CHEVRON_DOWN
                        .image(rect.width(), c)
                        .paint_at(ui, rect);
                });
            IconButton::new()
                .variant(IconButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .enabled(candidate && index > 0)
                .show(ui, theme, &|ui, rect, c| {
                    icons::CHEVRON_UP.image(rect.width(), c).paint_at(ui, rect);
                });
            if !candidate {
                tag(ui, theme, "off", TagVariant::Default, false);
            }
        });
    });
    row_separator(ui, theme, resp.response.rect);
}

/// 시안 `ExtMapG` 패널 — Input + Add, 그 아래 확장자별 detector 순서 목록.
fn ext_map_panel(ui: &mut egui::Ui, theme: &Theme, draft: &mut String, seed: ExtMapSeed) {
    let frame = egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_frame().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8));
    // 바깥 폭이 시안 값이 되도록 padding·border를 뺀 폭을 콘텐츠에 준다.
    let content_w = EXT_PANEL_WIDTH.value() - frame.total_margin().sum().x;
    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(content_w);
            ui.spacing_mut().item_spacing.y = theme.spacing_md.value();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 이 예제에는 detector가 설치돼 있다 — 입력이 비었을 때만 막힌다.
                    Button::new("Add")
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Sm)
                        .enabled(!draft.trim().is_empty())
                        .show(ui, theme);
                    Input::new()
                        .mono(true)
                        .placeholder(crate::i18n::t(
                            "settings.file_handler.extension_mapping.add_placeholder",
                        ))
                        .width(ui.available_width())
                        .show(ui, theme, draft);
                });
            });
            let (reset, remove, not_installed) = seed.labels();
            let (undo, removed_on_save, reset_on_save) = seed.pending_labels();
            // 누른 버튼은 같은 자리의 Undo 가 되고, Tag 가 Save 가 할 일을 적는다.
            let md_head = match (seed.custom, seed.pending) {
                (false, _) => ExtHeader::default(),
                (true, false) => ExtHeader {
                    button: Some(reset),
                    ..ExtHeader::default()
                },
                (true, true) => ExtHeader {
                    button: Some(undo),
                    tag: Some(reset_on_save),
                    ..ExtHeader::default()
                },
            };
            ext_group(ui, theme, ".md", EXT_MD_ROWS, md_head);
            if seed.missing {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let head = if seed.pending {
                        ExtHeader {
                            button: Some(undo),
                            tag: Some(removed_on_save),
                            not_installed: true,
                            struck: true,
                        }
                    } else {
                        ExtHeader {
                            button: Some(remove),
                            tag: Some(not_installed),
                            not_installed: true,
                            struck: false,
                        }
                    };
                    ext_group_header(ui, theme, ".ipynb", head);
                });
            }
            ext_group(ui, theme, ".log", EXT_LOG_ROWS, ExtHeader::default());
        });
    });
}

/// 설치된 확장자 한 묶음 — 머리줄과 detector 순서 목록.
fn ext_group(
    ui: &mut egui::Ui,
    theme: &Theme,
    ext: &str,
    rows: &[(&str, bool)],
    head: ExtHeader<'_>,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ext_group_header(ui, theme, ext, head);
        let last = rows.iter().rposition(|(_, c)| *c);
        for (i, (name, candidate)) in rows.iter().enumerate() {
            ext_detector_row(ui, theme, i, name, *candidate, Some(i) == last);
        }
    });
}

/// 시안 `ThemePair` — 같은 패널을 Mocha·Latte로 나란히 둔다.
/// `pair`는 짝 번호다. 입력 버퍼 내용이 아니라 고정 번호로 id를 구분해야 입력 중에도 포커스가 유지된다.
fn ext_map_theme_pair(
    ui: &mut egui::Ui,
    theme: &Theme,
    pair: usize,
    drafts: &mut [String],
    seed: ExtMapSeed,
) {
    // 갤러리 배율을 따르도록 두 팔레트에 현재 zoom을 입힌다.
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        ("Mocha", with_zoom(tasty_themes::mocha_fallback())),
        ("Latte", with_zoom(crate::host_shell::latte_theme())),
    ];
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        for ((label, th), draft) in themes.iter().zip(drafts.iter_mut()) {
            egui::Frame::new()
                .fill(th.bg_app().to_egui())
                .corner_radius(th.corner_radius.value())
                .inner_margin(egui::Margin::same(th.spacing_md.value() as i8))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = th.spacing_sm.value();
                        ui.label(
                            egui::RichText::new(*label)
                                .size(th.font_size_caption.value())
                                .color(th.text_muted().to_egui()),
                        );
                        ui.push_id(("ext_map", pair, *label), |ui| {
                            ext_map_panel(ui, th, draft, seed);
                        });
                    });
                });
        }
    });
}

pub fn draw_extension_mapping(ui: &mut egui::Ui, theme: &Theme) {
    let custom_missing = ExtMapSeed {
        custom: true,
        missing: true,
        long: false,
        pending: false,
    };
    let custom_missing_long = ExtMapSeed {
        long: true,
        ..custom_missing
    };
    let pending = ExtMapSeed {
        pending: true,
        ..custom_missing
    };
    let pending_long = ExtMapSeed {
        pending: true,
        ..custom_missing_long
    };
    let seeds = [
        ExtMapSeed::default(),
        ExtMapSeed::default(),
        custom_missing,
        custom_missing_long,
        pending,
        pending_long,
    ];
    EXT_DRAFTS.with(|d| {
        let drafts = &mut *d.borrow_mut();
        let mut pairs = drafts.chunks_mut(2).zip(seeds).enumerate();
        // 시안은 기본 두 짝, Reset·Remove 두 짝, Save 전 초안 두 짝을 서로 다른 Stage에 둔다.
        for _ in 0..3 {
            spec::stage(ui, theme, StageVariant::Column, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
                for (pair, (buf, seed)) in pairs.by_ref().take(2) {
                    ext_map_theme_pair(ui, theme, pair, buf, seed);
                }
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "add",
                "Button secondary sm · disabled: empty input or no detector",
            ),
            ("order", "IconButton sm chevronUp / chevronDown"),
            ("top / last row", "▲ / ▼ disabled"),
            (
                "non-candidate row",
                "both disabled · name text-disabled · Tag disabled \"off\"",
            ),
            ("hide instead?", "no — slots stay put"),
            (
                "Reset",
                "ghost Button sm · header right end · only when the extension has a custom order · tooltip kept · draft only",
            ),
            (
                "not installed",
                "the group header alone: .ext text-disabled · Tag disabled · Remove ghost Button sm; no detector rows; separator under it",
            ),
            (
                "header",
                "min-height button-height-sm — Reset appearing never moves the rows",
            ),
            (
                "long copy",
                "Tag and button never truncate; .ext label is the shrinking item",
            ),
            ("confirm", "none — both edit the draft, Cancel reverts"),
            (
                "pending",
                "after Remove / Reset, before Save: the header stays; the pressed button becomes Undo (ghost sm, same slot); a Tag disabled says what Save does — \"removed on save\" / \"reset on save\"",
            ),
            (
                "pending remove",
                ".ext label line-through (text-disabled kept)",
            ),
            (
                "pending reset",
                "rows already show install order; Reset → Undo",
            ),
            (
                "Undo",
                "drops that one draft change; pressing it again is not a toggle back — Remove / Reset return",
            ),
            (
                "Save / Cancel",
                "Save applies (removed group disappears, Reset button disappears); Cancel reverts every pending header",
            ),
        ],
        &[
            TokenChip::new(
                "state-disabled-fg",
                "disabled ink",
                theme.state_disabled_fg().to_egui(),
            ),
            TokenChip::without_color("settings-row-min-height", "row"),
        ],
    );
    spec::note(
        ui,
        theme,
        "The body draws each detector row at settings-row-min-height with a separator rule under it, and a separator under a not-installed header (extension_mapping.rs); the kit Meta lists neither.",
    );
}

/// jsx `SEED_HOOKS` 미러 행.
#[derive(Clone)]
struct HookSeed {
    id: String,
    /// 출처 Tag 에 그대로 찍히는 글자 — `host` · `you` · **그 plugin 의 id**.
    origin: &'static str,
    /// 이 행이 사용자 것인가. 휴지통이 붙는 유일한 조건이다.
    user: bool,
    prio: i32,
    cmd: String,
    /// `IpcSequence` 행인가. 그러면 2 행이 편집 Input 이 아니라 **mono 한 줄 요약**이다.
    seq: bool,
    /// 저장된 method 를 한 줄 형식으로 쓸 수 없는 시퀀스인가. 그러면 Edit 대신 "Edit with CLI" + 복사다.
    cli: bool,
    on: bool,
}

struct HookState {
    hooks: Vec<HookSeed>,
    adding: bool,
    draft_id: String,
    draft_cmd: String,
}

fn seed_hooks() -> Vec<HookSeed> {
    vec![
        HookSeed {
            id: "push.received".into(),
            origin: "host",
            user: false,
            prio: 10,
            cmd: "tasty notify \"push → $TASTY_HOOK_REPO\"".into(),
            seq: false,
            cli: false,
            on: true,
        },
        HookSeed {
            id: "pr.opened".into(),
            origin: "git-helper",
            user: false,
            prio: 20,
            cmd: "git-helper pr open --id $TASTY_HOOK_PR".into(),
            seq: false,
            cli: false,
            on: true,
        },
        HookSeed {
            id: "deploy.finished".into(),
            origin: "you",
            user: true,
            prio: 30,
            cmd: "~/ops/on-deploy.sh $TASTY_HOOK_ENV".into(),
            seq: false,
            cli: false,
            on: false,
        },
        HookSeed {
            id: "alert.fired".into(),
            origin: "host",
            user: false,
            prio: 40,
            cmd: "ipc: window.focus → layout.save → surface.close".into(),
            seq: true,
            cli: false,
            on: true,
        },
        HookSeed {
            id: "ci-notify".into(),
            origin: "you",
            user: true,
            prio: 50,
            cmd: "ipc: system.info → … (2 steps)".into(),
            seq: true,
            cli: true,
            on: true,
        },
    ]
}

thread_local! {
    static HOOK_STATE: RefCell<HookState> = RefCell::new(HookState {
        hooks: seed_hooks(),
        adding: false,
        draft_id: String::new(),
        draft_cmd: String::new(),
    });
}

/// plugin 출처만 mauve(`accent-agent`)다 — `host` 와 `you` 는 기본 Tag.
///
/// plugin 행은 "plugin" 이 아니라 **그 plugin 의 id** 를 달므로 이름으로 못 가른다.
fn origin_variant(origin: &str) -> TagVariant {
    if origin == "host" || origin == "you" {
        TagVariant::Default
    } else {
        TagVariant::Agent
    }
}

pub fn draw_hook_handlers(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                HOOK_STATE.with(|s| {
                    let st = &mut *s.borrow_mut();
                    draw_hook_content(ui, theme, st);
                });
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("row", "2줄 — id·Tag·prio·Switch·(휴지통|자물쇠) / action"),
            ("origin tag", "host · you · plugin id(agent variant)"),
            ("remove", "user 행만 — 그 외는 자물쇠 글리프 + tooltip"),
            (
                "IpcSequence",
                "편집 Input 대신 mono 한 줄 요약(스텝을 → 로 이음)",
            ),
            (
                "sequence editing",
                "Edit (ghost sm) on EVERY IpcSequence row (host / plugin edits save as a user override, like ShellCommand) → opens the inline text editor below",
            ),
            (
                "not text-representable",
                "a stored method name the line format cannot carry → the row keeps caption \"Edit with CLI\" + copy; copies tasty hook-handler get --id <id>",
            ),
            (
                "switched off",
                "id in text-muted, no opacity; the Switch and a disabled command state it",
            ),
            ("add card", "surface-raised + border + radius · caps 헤드"),
            ("priority", "낮을수록 먼저 (레지스트리 규약)"),
        ],
        &[
            TokenChip::new("text", "handler id (mono)", theme.text_primary().to_egui()),
            TokenChip::new(
                "text-muted",
                "prio · intro",
                theme.text_muted_raised().to_egui(),
            ),
            TokenChip::new(
                "accent-agent",
                "plugin origin tag",
                theme.accent_agent().to_egui(),
            ),
            TokenChip::without_color("font-mono", "event · action · sequence"),
            TokenChip::new("glyph-dim", "자물쇠 글리프", theme.glyph_dim().to_egui()),
            TokenChip::new(
                "separator",
                "row divider",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "surface-raised",
                "add-draft card",
                theme.surface_raised().to_egui(),
            ),
        ],
    );
}

fn draw_hook_content(ui: &mut egui::Ui, theme: &Theme, st: &mut HookState) {
    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();

    ui.horizontal_top(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if Button::new("Add handler")
                .variant(ButtonVariant::Secondary)
                .size(ControlSize::Sm)
                .leading_icon(&|ui, rect, c| {
                    icons::PLUS.image(rect.width(), c).paint_at(ui, rect);
                })
                .show(ui, theme)
                .clicked()
            {
                st.adding = true;
                st.draft_id.clear();
                st.draft_cmd.clear();
            }
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_max_width(theme.measure_md.value());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Handlers fired when the inbound-hook server receives a matching \
                             event. Includes core host defaults, plugin contributions, and your \
                             own user mappings. The webhook listener (bind / port / secret) is \
                             configured separately.",
                        )
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted().to_egui()),
                    )
                    .wrap(),
                );
            });
        });
    });

    if st.adding {
        match hook_add_card(ui, theme, &mut st.draft_id, &mut st.draft_cmd) {
            Some(HookCardAction::Add) => {
                let max_prio = st.hooks.iter().map(|h| h.prio).max().unwrap_or(0);
                st.hooks.push(HookSeed {
                    id: st.draft_id.trim().to_string(),
                    origin: "you",
                    user: true,
                    prio: max_prio + 10,
                    cmd: st.draft_cmd.trim().to_string(),
                    seq: false,
                    cli: false,
                    on: true,
                });
                st.adding = false;
            }
            Some(HookCardAction::Cancel) => st.adding = false,
            None => {}
        }
    }

    mono_head(
        ui,
        theme,
        crate::i18n::t("settings.file_handler.hook_handlers.section_head"),
    );
    let mut remove: Option<usize> = None;
    for i in 0..st.hooks.len() {
        draw_hook_row(ui, theme, st, i, &mut remove);
    }
    if let Some(i) = remove {
        st.hooks.remove(i);
    }
}

/// 자물쇠 슬롯 — 휴지통 IconButton 과 같은 크기의 자리를 차지한다(행마다 우측 끝이
/// 어긋나지 않게). 누를 수 있는 것이 아니라 읽는 표시다.
fn lock_slot(ui: &mut egui::Ui, theme: &Theme) {
    let side = ControlSize::Sm.height(theme);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let glyph = ControlSize::Sm.icon_glyph(theme);
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(glyph, glyph));
    icons::LOCK
        .image(icon_rect.width(), theme.glyph_dim().into())
        .paint_at(ui, icon_rect);
    resp.on_hover_text("Provided by host — can't be removed");
}

/// jsx `HookRow` — 2줄 컬럼 + 하단 separator + disabled 시 row opacity.
fn draw_hook_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    st: &mut HookState,
    i: usize,
    remove: &mut Option<usize>,
) {
    let on = st.hooks[i].on;
    let resp = ui.scope(|ui| {
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: theme.spacing_xs.value() as i8,
                right: theme.spacing_xs.value() as i8,
                top: theme.spacing_sm.value() as i8,
                bottom: theme.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 호스트·플러그인 기본 항목은 재등록되므로 사용자 항목에만 삭제를 제공한다.
                        if st.hooks[i].user {
                            if IconButton::new()
                                .variant(IconButtonVariant::Ghost)
                                .size(ControlSize::Sm)
                                .show(ui, theme, &|ui, rect, c| {
                                    icons::TRASH.image(rect.width(), c).paint_at(ui, rect);
                                })
                                .clicked()
                            {
                                *remove = Some(i);
                            }
                        } else {
                            lock_slot(ui, theme);
                        }
                        switch(ui, theme, &mut st.hooks[i].on, None, true);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(st.hooks[i].id.clone())
                                        .monospace()
                                        .strong()
                                        .size(theme.font_size_body.value())
                                        .color(
                                            if on {
                                                theme.text_primary()
                                            } else {
                                                theme.text_muted()
                                            }
                                            .to_egui(),
                                        ),
                                )
                                .truncate(),
                            );
                            let origin = st.hooks[i].origin;
                            tag(ui, theme, origin, origin_variant(origin), false);
                            ui.label(
                                egui::RichText::new(format!("prio {}", st.hooks[i].prio))
                                    .family(tasty_ui_widgets::label_font_family())
                                    .size(theme.font_size_micro.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        });
                    });
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    let seq = st.hooks[i].seq;
                    ui.allocate_ui_with_layout(
                        egui::vec2(HOOK_CMD_LABEL_W.value(), theme.input_height().value()),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new(if seq { "Action:" } else { "Shell cmd:" })
                                    .size(theme.font_size_caption.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        },
                    );
                    if seq {
                        // 요약 한 줄 · 오른쪽 끝 Edit(편집기는 아래 Spec), 한 줄 형식으로 쓸 수 없으면
                        // "Edit with CLI"(툴팁 = 명령) + 복사.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if st.hooks[i].cli {
                                let copy = IconButton::new()
                                    .variant(IconButtonVariant::Ghost)
                                    .size(ControlSize::Sm)
                                    .show(ui, theme, &|ui, rect, c| {
                                        icons::COPY.image(rect.width(), c).paint_at(ui, rect);
                                    });
                                copy.on_hover_text("Copy edit command");
                                ui.label(
                                    egui::RichText::new("Edit with CLI")
                                        .size(theme.font_size_caption.value())
                                        .color(theme.text_muted().to_egui()),
                                )
                                .on_hover_text("tasty hook-handler get --id ci-notify");
                            } else {
                                Button::new("Edit")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ControlSize::Sm)
                                    .show(ui, theme);
                            }
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(st.hooks[i].cmd.clone())
                                                .monospace()
                                                .size(theme.font_size_term_sm.value())
                                                .color(theme.text_secondary().to_egui()),
                                        )
                                        .truncate(),
                                    );
                                },
                            );
                        });
                    } else {
                        Input::new()
                            .mono(true)
                            .enabled(on)
                            .show(ui, theme, &mut st.hooks[i].cmd);
                    }
                });
            });
    });
    row_separator(ui, theme, resp.response.rect);
}

enum HookCardAction {
    Add,
    Cancel,
}

/// Hook Handlers add 카드. 카드는 surface-raised이고 라벨·Input·Cancel은 raised 바탕 규칙을 따른다.
fn hook_add_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    draft_id: &mut String,
    draft_cmd: &mut String,
) -> Option<HookCardAction> {
    let mut action = None;
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .inner_margin(egui::Margin::same(theme.spacing_md.value() as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
            mono_head(ui, theme, "New hook handler");
            hook_field_row(ui, theme, "Event id:", "e.g. pipeline.done", draft_id);
            hook_field_row(
                ui,
                theme,
                "Shell command:",
                crate::i18n::t("settings.file_handler.hook_handlers.placeholder_shell_command"),
                draft_cmd,
            );
            // 가운데 정렬이 남은 높이를 모두 차지하지 않도록 위쪽에 맞춘다.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                if Button::new("Add handler")
                    .variant(ButtonVariant::Primary)
                    .size(ControlSize::Sm)
                    .enabled(!draft_id.trim().is_empty())
                    .show(ui, theme)
                    .clicked()
                {
                    action = Some(HookCardAction::Add);
                }
                if Button::new("Cancel")
                    .variant(ButtonVariant::Ghost)
                    .size(ControlSize::Sm)
                    .ground(Ground::Raised)
                    .show(ui, theme)
                    .clicked()
                {
                    action = Some(HookCardAction::Cancel);
                }
            });
        });
    action
}

/// 열린 add 카드를 Mocha·Latte로 고정해 보인다. raised 라벨·Input·Cancel을 클릭 없이 확인한다.
pub fn draw_hook_add_card_raised(ui: &mut egui::Ui, theme: &Theme) {
    let with_zoom =
        |base: Theme| Theme::with_colors_and_zoom(base.to_colors(), base.is_light, theme.ui_zoom);
    let themes = [
        with_zoom(tasty_themes::mocha_fallback()),
        with_zoom(crate::host_shell::latte_theme()),
    ];
    spec::stage(ui, theme, StageVariant::Column, |ui| {
        for th in &themes {
            kit::frame_card_flat(ui, th, WIDTH, kit::panel_fill(th), |ui| {
                kit::region_sym(ui, th.spacing_md, th.spacing_sm, |ui| {
                    let mut id = "pipeline.done".to_string();
                    let mut cmd = String::new();
                    // 고정 예제라 버튼 결과를 쓰지 않는다.
                    let _action = hook_add_card(ui, th, &mut id, &mut cmd);
                });
            });
        }
    });
    spec::meta(
        ui,
        theme,
        &[
            (
                "card",
                "surface-raised + border-default + radius · caps head",
            ),
            ("labels", "text-secondary-raised"),
            (
                "inputs",
                "Input ground=\"raised\" → input-raised-bg / input-raised-border",
            ),
            (
                "Cancel",
                "Button ground=\"raised\" → button-ghost-raised-fg",
            ),
        ],
        &[
            TokenChip::new(
                "text-secondary-raised",
                "field labels",
                theme.text_secondary_raised().to_egui(),
            ),
            TokenChip::new(
                "input-raised-bg",
                "→ bg-panel",
                theme.input_raised_bg().to_egui(),
            ),
            TokenChip::new(
                "input-raised-border",
                "→ border-strong",
                theme.input_raised_border().to_egui(),
            ),
            TokenChip::new(
                "button-ghost-raised-fg",
                "→ text-secondary-raised",
                theme.button_ghost_raised_fg().to_egui(),
            ),
        ],
    );
}

/// add 카드의 라벨(100px) + mono Input 행. 카드가 surface-raised라 라벨은 text-secondary-raised,
/// Input은 raised 바탕이다.
fn hook_field_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    placeholder: &str,
    buf: &mut String,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
        ui.allocate_ui_with_layout(
            egui::vec2(
                HOOK_ADD_LABEL_W.value(),
                theme.settings_row_min_height().value(),
            ),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(theme.font_size_body.value())
                        .color(theme.text_secondary_raised().to_egui()),
                );
            },
        );
        Input::new()
            .ground(Ground::Raised)
            .mono(true)
            .placeholder(placeholder)
            .show(ui, theme, buf);
    });
}
