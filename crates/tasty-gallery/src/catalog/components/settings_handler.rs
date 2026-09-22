//! Settings › Handler 탭의 L2 서브탭 **콘텐츠** specimen 4 종.
//!
//! 전사 원본: `ui_kits/terminal/overlays/settings_window.jsx` `body()` 의
//! FileHandler 분기(File Extension Mapping / File Detectors / File Handlers,
//! jsx:910-964) + `HookHandlers`/`HookRow` 컴포넌트(jsx:442-545).
//!
//! `settings` specimen(창 셸)은 L2 를 탐색할 수 없어 이 서브탭 콘텐츠들이
//! 카탈로그에서 누락돼 있었다(ADR-0510 갤러리 완전성 갭) — 여기서 서브탭별
//! Spec 으로 노출한다. 갤러리는 본체 registry 에 의존할 수 없으므로 jsx 의
//! seed 데이터를 그대로 쓴다. 본체 대응: `src/view/settings/ui/file_handler_tab/`.

use std::cell::RefCell;
use tasty_type_geometry::length::LogicalPx;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Input, TagVariant, select,
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

/// jsx `Mono` — mono 10 uppercase letter-spacing caps, text-muted.
fn mono_head(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .monospace()
            .size(theme.font_size_micro.value())
            .color(theme.text_muted().to_egui()),
    );
}

/// 행 하단 1px separator (jsx `borderBottom`).
fn row_separator(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect) {
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(theme.border_width.value(), theme.separator.to_egui()),
    );
}

// ── File Extension Mapping (jsx:911-928) ─────────────────────────────────

const EXT_HANDLERS: &[&str] = &[
    "Image viewer",
    "Log viewer",
    "Editor",
    "Hex viewer",
    "External app",
];
/// jsx seed: (ext cluster, 기본 handler index in EXT_HANDLERS).
const EXT_ROWS: &[(&str, usize)] = &[
    ("*.png  *.jpg  *.svg", 0),
    ("*.log  *.txt", 1),
    ("*.json  *.yaml  *.toml", 2),
    ("*.bin  *.hex  *.o", 3),
];

thread_local! {
    static EXT_STATE: RefCell<Vec<usize>> =
        RefCell::new(EXT_ROWS.iter().map(|(_, h)| *h).collect());
}

pub fn draw_extension_mapping(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // Settings 창 안의 L2 서브탭 **콘텐츠**다 — 떠 있는 표면이 아니라 창 셸에
        // 얹힌 패널이므로 lift 가 없다(ADR-0254 세 번째 갈래).
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                // 헤더 행 — Mono 헤드 좌 + "Add mapping" ghost sm 우 (jsx:914-917).
                ui.horizontal(|ui| {
                    mono_head(ui, theme, "Extension → handler");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // specimen 은 상태가 없다 — 클릭 응답을 받아 처리할 곳이 없다.
                        let _ = Button::new("Add mapping")
                            .variant(ButtonVariant::Ghost)
                            .size(ControlSize::Sm)
                            .show(ui, theme);
                    });
                });
                EXT_STATE.with(|s| {
                    let sel = &mut *s.borrow_mut();
                    for (i, (ext, _)) in EXT_ROWS.iter().enumerate() {
                        let resp = ui.horizontal(|ui| {
                            ui.set_min_height(theme.settings_row_min_height().value());
                            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                            ui.label(
                                egui::RichText::new(*ext)
                                    .monospace()
                                    .size(theme.font_size_term_sm.value())
                                    .color(theme.text_secondary().to_egui()),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    select(
                                        ui,
                                        theme,
                                        &format!("gallery_ext_map_{i}"),
                                        &mut sel[i],
                                        EXT_HANDLERS,
                                        theme.field_width_md.value(),
                                        true,
                                    );
                                    ui.label(
                                        egui::RichText::new("→")
                                            .color(theme.text_muted().to_egui()),
                                    );
                                },
                            );
                        });
                        // jsx: 마지막 행은 borderBottom 없음.
                        if i + 1 < EXT_ROWS.len() {
                            row_separator(ui, theme, resp.response.rect);
                        }
                    }
                });
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("row", "ext(mono 12) 좌 · → · Select(150) 우"),
            ("row height", "settings-row-min-height"),
            ("divider", "1px separator · 마지막 행 없음"),
        ],
        &[
            TokenChip::new(
                "text-secondary",
                "ext cluster",
                theme.text_secondary().to_egui(),
            ),
            TokenChip::new("text-muted", "→ glyph", theme.text_muted().to_egui()),
            TokenChip::new("separator", "row divider", theme.separator.to_egui()),
        ],
    );
}

// ── File Detectors (jsx:929-947) ─────────────────────────────────────────

/// jsx seed: (name, desc, on).
const DETECTOR_ROWS: &[(&str, &str, bool)] = &[
    (
        "Extension match",
        "Match the file extension against the mapping table.",
        true,
    ),
    (
        "Path exists",
        "Only treat a token as a file when the path resolves on disk.",
        true,
    ),
    (
        "Content sniff",
        "Inspect magic bytes for files with no extension.",
        false,
    ),
    ("MIME type", "Fall back to the OS MIME database.", false),
];

thread_local! {
    static DETECTOR_STATE: RefCell<Vec<bool>> =
        RefCell::new(DETECTOR_ROWS.iter().map(|(_, _, on)| *on).collect());
}

pub fn draw_detectors(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // Settings 창 안의 L2 서브탭 **콘텐츠**다 — 떠 있는 표면이 아니라 창 셸에
        // 얹힌 패널이므로 lift 가 없다(ADR-0254 세 번째 갈래).
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                mono_head(ui, theme, "Detection passes (priority order)");
                DETECTOR_STATE.with(|s| {
                    let on = &mut *s.borrow_mut();
                    for (i, (name, desc, _)) in DETECTOR_ROWS.iter().enumerate() {
                        let resp = ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                switch(ui, theme, &mut on[i], None, true);
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.label(
                                        egui::RichText::new(*name)
                                            .size(theme.font_size_body.value())
                                            .color(theme.text_secondary().to_egui()),
                                    );
                                    ui.label(
                                        egui::RichText::new(*desc)
                                            .size(theme.font_size_term_sm.value())
                                            .color(theme.text_muted().to_egui()),
                                    );
                                });
                            });
                        });
                        ui.add_space(theme.spacing_sm.value());
                        row_separator(
                            ui,
                            theme,
                            resp.response
                                .rect
                                .expand2(egui::vec2(0.0, theme.spacing_xs.value())),
                        );
                    }
                });
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("row", "name+desc 좌(2줄) · Switch 우"),
            ("desc", "12 text-muted, marginTop 2"),
            ("divider", "1px separator · paddingBottom 8"),
        ],
        &[
            TokenChip::new(
                "text-secondary",
                "pass name",
                theme.text_secondary().to_egui(),
            ),
            TokenChip::new("text-muted", "description", theme.text_muted().to_egui()),
            TokenChip::new(
                "accent-primary",
                "switch on",
                theme.accent_primary().to_egui(),
            ),
        ],
    );
}

// ── File Handlers (jsx:950-964) ──────────────────────────────────────────

/// jsx seed: (name, kind tag, on).
const HANDLER_ROWS: &[(&str, &str, bool)] = &[
    ("Image viewer", "image", true),
    ("Log viewer", "text", true),
    ("Hex viewer", "binary", false),
    ("External app", "fallback", false),
];

thread_local! {
    static HANDLER_STATE: RefCell<Vec<bool>> =
        RefCell::new(HANDLER_ROWS.iter().map(|(_, _, on)| *on).collect());
}

pub fn draw_file_handlers(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        // Settings 창 안의 L2 서브탭 **콘텐츠**다 — 떠 있는 표면이 아니라 창 셸에
        // 얹힌 패널이므로 lift 가 없다(ADR-0254 세 번째 갈래).
        kit::frame_card_flat(ui, theme, WIDTH, kit::panel_fill(theme), |ui| {
            kit::region_sym(ui, theme.spacing_md, theme.spacing_sm, |ui| {
                mono_head(ui, theme, "Registered file handlers");
                HANDLER_STATE.with(|s| {
                    let on = &mut *s.borrow_mut();
                    for (i, (name, kind, _)) in HANDLER_ROWS.iter().enumerate() {
                        let resp = ui.horizontal(|ui| {
                            ui.set_min_height(theme.settings_row_min_height().value());
                            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
                            ui.label(
                                egui::RichText::new(*name)
                                    .size(theme.font_size_body.value())
                                    .color(theme.text_secondary().to_egui()),
                            );
                            tag(ui, theme, kind, TagVariant::Default, false);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    switch(ui, theme, &mut on[i], None, true);
                                },
                            );
                        });
                        if i + 1 < HANDLER_ROWS.len() {
                            row_separator(ui, theme, resp.response.rect);
                        }
                    }
                });
            });
        });
    });
    spec::meta(
        ui,
        theme,
        &[
            ("row", "name · Tag(kind) 좌 · Switch 우(marginLeft auto)"),
            ("row height", "settings-row-min-height"),
            ("divider", "1px separator · 마지막 행 없음"),
        ],
        &[
            TokenChip::new(
                "text-secondary",
                "handler name",
                theme.text_secondary().to_egui(),
            ),
            TokenChip::new("tag", "kind chip", theme.tag_fg().to_egui()),
            TokenChip::new(
                "accent-primary",
                "switch on",
                theme.accent_primary().to_egui(),
            ),
        ],
    );
}

// ── Hook Handlers (jsx HookHandlers/HookRow, 442-545) ────────────────────

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
            on: true,
        },
        HookSeed {
            id: "pr.opened".into(),
            origin: "git-helper",
            user: false,
            prio: 20,
            cmd: "git-helper pr open --id $TASTY_HOOK_PR".into(),
            seq: false,
            on: true,
        },
        HookSeed {
            id: "deploy.finished".into(),
            origin: "you",
            user: true,
            prio: 30,
            cmd: "~/ops/on-deploy.sh $TASTY_HOOK_ENV".into(),
            seq: false,
            on: false,
        },
        HookSeed {
            id: "alert.fired".into(),
            origin: "host",
            user: false,
            prio: 40,
            cmd: "ipc: window.focus → layout.save → surface.close".into(),
            seq: true,
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
        // Settings 창 안의 L2 서브탭 **콘텐츠**다 — 떠 있는 표면이 아니라 창 셸에
        // 얹힌 패널이므로 lift 가 없다(ADR-0254 세 번째 갈래).
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
            ("disabled", "row 전체 opacity-disabled"),
            ("add card", "surface-raised + border + radius · caps 헤드"),
            ("priority", "낮을수록 먼저 (레지스트리 규약)"),
        ],
        &[
            TokenChip::new("text", "handler id (mono)", theme.text_primary().to_egui()),
            TokenChip::new("text-muted", "prio · intro", theme.text_muted().to_egui()),
            TokenChip::new(
                "accent-agent",
                "plugin origin tag",
                theme.accent_agent().to_egui(),
            ),
            TokenChip::new("glyph-dim", "자물쇠 글리프", theme.glyph_dim().to_egui()),
            TokenChip::new("separator", "row divider", theme.separator.to_egui()),
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

    // ── intro: 설명 paragraph(flex 1, measure-md) + "Add handler" 버튼 ──
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
                        .size(theme.font_size_term_sm.value())
                        .color(theme.text_muted().to_egui()),
                    )
                    .wrap(),
                );
            });
        });
    });

    // ── "Add handler" 인라인 draft 카드 (jsx `adding && …`) ──
    if st.adding {
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
                hook_field_row(
                    ui,
                    theme,
                    "Event id:",
                    "e.g. pipeline.done",
                    &mut st.draft_id,
                );
                hook_field_row(
                    ui,
                    theme,
                    "Shell command:",
                    "tasty notify \"$TASTY_HOOK_*\"",
                    &mut st.draft_cmd,
                );
                // Align::Min(상단) — 본체 hook_handlers.rs 와 동일 이유(Frame 안
                // 마지막 요소, Align::Center 는 잔여 세로 공간 전체로 확장됨).
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    let can_add = !st.draft_id.trim().is_empty();
                    if Button::new("Add handler")
                        .variant(ButtonVariant::Primary)
                        .size(ControlSize::Sm)
                        .enabled(can_add)
                        .show(ui, theme)
                        .clicked()
                    {
                        let max_prio = st.hooks.iter().map(|h| h.prio).max().unwrap_or(0);
                        st.hooks.push(HookSeed {
                            id: st.draft_id.trim().to_string(),
                            origin: "you",
                            user: true,
                            prio: max_prio + 10,
                            cmd: st.draft_cmd.trim().to_string(),
                            seq: false,
                            on: true,
                        });
                        st.adding = false;
                    }
                    if Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .size(ControlSize::Sm)
                        .show(ui, theme)
                        .clicked()
                    {
                        st.adding = false;
                    }
                });
            });
    }

    // ── Mono caps 섹션 헤드 + rows ──
    mono_head(ui, theme, "Registered hook handlers");
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
        if !on {
            ui.set_opacity(theme.opacity_disabled());
        }
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: theme.spacing_xs.value() as i8,
                right: theme.spacing_xs.value() as i8,
                top: theme.spacing_sm.value() as i8,
                bottom: theme.spacing_sm.value() as i8,
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_xs.value();
                // line 1 — id · origin Tag · prio · (우측) Switch + remove.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 출처가 그 자리를 정한다 — user 행만 휴지통, 나머지는 자물쇠.
                        // 레지스트리가 시작마다 host/plugin 기본값을 다시 심으므로
                        // 거기 지우기를 두면 시스템이 되돌리는 것을 약속하는 셈이다.
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
                                        .color(theme.text_primary().to_egui()),
                                )
                                .truncate(),
                            );
                            let origin = st.hooks[i].origin;
                            tag(ui, theme, origin, origin_variant(origin), false);
                            ui.label(
                                egui::RichText::new(format!("prio {}", st.hooks[i].prio))
                                    .monospace()
                                    .size(theme.font_size_micro.value())
                                    .color(theme.text_muted().to_egui()),
                            );
                        });
                    });
                });
                // line 2 — "Shell cmd:" 라벨(74) + mono Input (disabled 시 편집 불가).
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
                        // 여러 스텝을 설정 행 안에서 고칠 자리가 없다 — 한 줄 요약만
                        // 둔다(스텝은 `→` 로 잇는다).
                        ui.label(
                            egui::RichText::new(st.hooks[i].cmd.clone())
                                .monospace()
                                .size(theme.font_size_term_sm.value())
                                .color(theme.text_secondary().to_egui()),
                        );
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

/// add 카드의 라벨(100px) + mono Input 행.
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
                        .color(theme.text_secondary().to_egui()),
                );
            },
        );
        Input::new()
            .mono(true)
            .placeholder(placeholder)
            .show(ui, theme, buf);
    });
}
