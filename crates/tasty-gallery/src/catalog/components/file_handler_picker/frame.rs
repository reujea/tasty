//! 핸들러 선택 예제의 행 데이터와 프레임 구성 요소.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::file_handler as fh_model;
use tasty_ui_widgets::tokens::{
    FH_EDGE_PAD_X, FH_EMPTY_PAD_Y, FH_FRAME_WIDTH, FH_GAP_SM, FH_HEADER_PAD_BOTTOM,
    FH_HEADER_PAD_TOP, FH_ID_LINE_GAP, FH_LIST_FADE_HEIGHT, FH_LIST_MAX_HEIGHT, FH_LIST_PAD,
    FH_RECENT_DIM_OPACITY, FH_ROW_GAP, FH_ROW_PAD_X, STRUCT_GAP_2,
};
use tasty_ui_widgets::{Button, ButtonVariant, ControlSize, TagVariant, tag, tag_mono, tag_width};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::widgets::dialog as kit;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Host,
    User,
    Plugin,
}

impl Owner {
    /// 디자인 `FH_OWNER_WORD` — 출처를 사용자 낱말로 옮긴다.
    fn word(self) -> &'static str {
        match self {
            Self::Host => "built-in",
            Self::User => "you",
            Self::Plugin => "plugin",
        }
    }
}

#[derive(Clone, Copy)]
struct Row {
    id: &'static str,
    owner: Owner,
    /// handler action 이 여는 surface kind. 행 아이콘을 정할 때 쓴다.
    kind: &'static str,
    /// 선언된 표시 이름. `None` 이면 id 의 마지막 `/` 뒤 조각을 고정폭 글꼴로 쓴다.
    name: Option<&'static str>,
    /// plugin 이 매니페스트로 선언한 글리프 이름. `icons.json` 에 있는 이름일 때만 우선한다.
    icon: Option<&'static str>,
    /// Recent 행의 마지막 사용 시각(상대 표기).
    when: Option<&'static str>,
    /// 이 형식의 현재 기본 핸들러.
    dflt: bool,
}

const fn row(id: &'static str, owner: Owner, kind: &'static str) -> Row {
    Row {
        id,
        owner,
        kind,
        name: None,
        icon: None,
        when: None,
        dflt: false,
    }
}

const fn named(id: &'static str, owner: Owner, kind: &'static str, name: &'static str) -> Row {
    Row {
        name: Some(name),
        ..row(id, owner, kind)
    }
}

/// 서피스 종류에 대응하는 기본 아이콘.
fn kind_icon(kind: &str) -> Option<MockGlyph> {
    Some(match kind {
        "markdown" => icons::MARKDOWN,
        "html" => icons::HTML,
        "image" => icons::IMAGE,
        "directory" => icons::FOLDER,
        "editor" => icons::EDIT,
        "pager" | "terminal" => icons::TERMINAL,
        "log" => icons::LIST,
        "table" => icons::COLUMNS,
        "binary" => icons::LAYERS,
        _ => return None,
    })
}

/// 매니페스트가 선언한 이름 → 글리프. `icons.json` 에 없는 이름은 선언이 없는 것과 같다.
fn declared_icon(name: &str) -> Option<MockGlyph> {
    match name {
        "markdown" => Some(icons::MARKDOWN),
        "html" => Some(icons::HTML),
        "image" => Some(icons::IMAGE),
        "folder" => Some(icons::FOLDER),
        "edit" => Some(icons::EDIT),
        "terminal" => Some(icons::TERMINAL),
        "listView" => Some(icons::LIST),
        "columns" => Some(icons::COLUMNS),
        "layers" => Some(icons::LAYERS),
        "file" => Some(icons::FILE),
        _ => None,
    }
}

/// 선언한 아이콘, 서피스 종류의 기본 아이콘, file 순서로 선택한다.
fn row_icon(r: &Row) -> MockGlyph {
    r.icon
        .and_then(declared_icon)
        .or_else(|| kind_icon(r.kind))
        .unwrap_or(icons::FILE)
}

/// 선언한 이름이 없으면 ID의 마지막 경로 부분을 사용한다.
fn row_name(r: &Row) -> &'static str {
    match r.name {
        Some(n) => n,
        None => match r.id.rfind('/') {
            Some(i) => &r.id[i + 1..],
            None => r.id,
        },
    }
}

const SUGGESTED: &[Row] = &[
    Row {
        dflt: true,
        ..named(
            "com.tasty.markdown/preview",
            Owner::Host,
            "markdown",
            "Markdown preview",
        )
    },
    named(
        "com.tasty.text/editor",
        Owner::Host,
        "editor",
        "Text editor",
    ),
    named(
        "com.tasty.pager/less",
        Owner::Host,
        "pager",
        "Terminal (less)",
    ),
    row("dev.git-helper.diff/viewer", Owner::Plugin, "markdown"),
];

/// 상대 시각 다섯 가지와 날짜를 한 행씩 표시해 열 너비를 비교한다.
const RECENT: &[Row] = &[
    Row {
        when: Some("just now"),
        ..named(
            "com.tasty.markdown/preview",
            Owner::Host,
            "markdown",
            "Markdown preview",
        )
    },
    Row {
        when: Some("12m ago"),
        ..named(
            "com.tasty.pager/less",
            Owner::Host,
            "pager",
            "Terminal (less)",
        )
    },
    Row {
        when: Some("2h ago"),
        ..named(
            "com.tasty.text/editor",
            Owner::Host,
            "editor",
            "Text editor",
        )
    },
    Row {
        when: Some("yesterday"),
        ..row("io.binview.hex/viewer", Owner::Plugin, "binary")
    },
    Row {
        when: Some("4d ago"),
        ..named("com.tasty.log/viewer", Owner::Host, "log", "Log viewer")
    },
    Row {
        when: Some("2026-09-13"),
        ..row("dev.git-helper.diff/viewer", Owner::Plugin, "markdown")
    },
];

const ALL: &[Row] = &[
    named(
        "com.tasty.text/editor",
        Owner::Host,
        "editor",
        "Text editor",
    ),
    named(
        "com.tasty.pager/less",
        Owner::Host,
        "pager",
        "Terminal (less)",
    ),
    named(
        "com.tasty.markdown/preview",
        Owner::Host,
        "markdown",
        "Markdown preview",
    ),
    named("com.tasty.log/viewer", Owner::Host, "log", "Log viewer"),
    row("io.binview.hex/viewer", Owner::Plugin, "binary"),
];

/// 표시 이름·아이콘의 유무와 긴 ID를 섞은 예제.
const MIXED: &[Row] = &[
    row("com.tasty.image/viewer", Owner::Host, "image"),
    Row {
        icon: Some("image"),
        dflt: true,
        ..named(
            "dev.imgview.raster/preview",
            Owner::Plugin,
            "image",
            "Raster preview",
        )
    },
    named(
        "com.tasty.text/editor",
        Owner::User,
        "editor",
        "Text editor",
    ),
    row(
        "net.example.enterprise.documents.attachments/inline-preview-handler",
        Owner::Plugin,
        "html",
    ),
];

const LONG_EXTRA: &[Row] = &[
    named("com.tasty.log/viewer", Owner::Host, "log", "Log viewer"),
    row("io.binview.hex/viewer", Owner::Plugin, "binary"),
    named(
        "dev.imgview.raster/preview",
        Owner::Plugin,
        "image",
        "Image preview",
    ),
    named("dev.tabular.csv/table", Owner::Plugin, "table", "CSV table"),
    named(
        "com.tasty.shell/editor-env",
        Owner::Host,
        "editor",
        "Open in $EDITOR",
    ),
    named(
        "com.tasty.tree/reveal",
        Owner::Host,
        "directory",
        "Reveal in file tree",
    ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FrameState {
    Default,
    Recent,
    Fallback,
    Empty,
    Long,
    Mixed,
}

impl FrameState {
    /// 헤더 mono 경로 — long/fallback 은 앞에서 잘린 긴 경로를 보인다.
    fn path(self) -> &'static str {
        match self {
            Self::Long | Self::Fallback => "…/federation/screens.tsx",
            _ => "docs/architecture.md",
        }
    }
}

/// 한 줄 말줄임 — 가용 폭을 넘으면 `…` 로 끝낸다.
fn paint_truncated(
    ui: &egui::Ui,
    pos: egui::Pos2,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_w: f32,
) -> f32 {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(0.0));
    let galley = ui.fonts(|f| f.layout_job(job));
    let w = galley.rect.width();
    ui.painter().galley(pos, galley, color);
    w
}

/// 행 하나 — 글리프 · 이름 · (출처 · id · 언제) · default Tag.
fn fh_row(ui: &mut egui::Ui, theme: &Theme, r: &Row, sel: bool, dim: bool) {
    let plugin = r.owner == Owner::Plugin;
    let name_font = if r.name.is_some() {
        egui::FontId::proportional(theme.font_size_body.value())
    } else {
        egui::FontId::monospace(theme.font_size_body.value())
    };
    let id_font = egui::FontId::monospace(theme.font_size_caption.value());
    let meta_font = egui::FontId::proportional(theme.font_size_caption.value());
    let name_h = ui.fonts(|f| f.row_height(&name_font));
    let id_h = ui.fonts(|f| f.row_height(&id_font));
    let h = theme.spacing_sm.value() * 2.0 + name_h + id_h;

    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());

    if sel {
        ui.painter().rect_filled(
            rect,
            theme.corner_radius_sm.value(),
            theme.surface_active().to_egui(),
        );
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme.listctrl_selected_bar_width().value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, theme.listctrl_selected_bar().to_egui());
    }

    let glyph_color = if sel {
        if plugin {
            theme.accent_agent_on_active().to_egui()
        } else {
            theme.text_on_active().to_egui()
        }
    } else if plugin {
        theme.accent_agent().to_egui()
    } else {
        let c = theme.text_muted().to_egui();
        if dim {
            c.gamma_multiply(FH_RECENT_DIM_OPACITY)
        } else {
            c
        }
    };
    let gs = theme.icon_glyph_size_md.value();
    let glyph_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.left() + FH_ROW_PAD_X.value(),
            rect.center().y - gs / 2.0,
        ),
        egui::vec2(gs, gs),
    );
    row_icon(r).image(gs, glyph_color).paint_at(ui, glyph_rect);

    // 우측 default Tag 를 먼저 자리잡아 텍스트 가용 폭에서 뺀다.
    let tag_w = if r.dflt {
        tag_width(ui, theme, "default") + FH_ROW_GAP.value()
    } else {
        0.0
    };
    let text_left = glyph_rect.right() + FH_ROW_GAP.value();
    let text_w = rect.right() - FH_ROW_PAD_X.value() - tag_w - text_left;

    let top = rect.top() + theme.spacing_sm.value();
    paint_truncated(
        ui,
        egui::pos2(text_left, top),
        row_name(r),
        name_font,
        if sel {
            theme.text_on_active()
        } else {
            theme.text_primary()
        }
        .to_egui(),
        text_w,
    );

    // 둘째 줄: 출처 · id · (언제). id 만 말줄임, 나머지는 고정 폭.
    let mut x = text_left;
    let y = top + name_h;
    // 선택 행은 surface-active 위이므로 둘째 줄 조각도 한 글자색이다. 본체 행과 같다.
    let on_active = theme.text_on_active().to_egui();
    let sep_color = if sel {
        on_active
    } else {
        theme.text_disabled().to_egui()
    };
    let muted = if sel {
        on_active
    } else {
        theme.text_muted().to_egui()
    };
    let owner_color = if plugin && !sel {
        theme.accent_agent().to_egui()
    } else {
        muted
    };
    x += paint_truncated(
        ui,
        egui::pos2(x, y),
        r.owner.word(),
        meta_font.clone(),
        owner_color,
        text_w,
    ) + FH_ID_LINE_GAP.value();
    x += paint_truncated(
        ui,
        egui::pos2(x, y),
        "·",
        meta_font.clone(),
        sep_color,
        text_w,
    ) + FH_ID_LINE_GAP.value();
    // 시각 문자열이 바뀌어도 ID 영역의 폭이 흔들리지 않도록 열 너비를 고정한다.
    let when_w = if r.when.is_some() {
        let dot = ui.fonts(|f| f.layout_no_wrap("·".into(), meta_font.clone(), sep_color));
        dot.rect.width() + theme.fh_when_width().value() + FH_ID_LINE_GAP.value() * 2.0
    } else {
        0.0
    };
    let id_avail = (text_left + text_w - x - when_w).max(0.0);
    x += paint_truncated(
        ui,
        egui::pos2(x, y),
        &fh_model::elide_id_front(r.id),
        id_font,
        muted,
        id_avail,
    ) + FH_ID_LINE_GAP.value();
    if let Some(when) = r.when {
        x += paint_truncated(
            ui,
            egui::pos2(x, y),
            "·",
            meta_font.clone(),
            sep_color,
            when_w,
        ) + FH_ID_LINE_GAP.value();
        paint_truncated(
            ui,
            egui::pos2(x, y),
            when,
            meta_font,
            muted,
            theme.fh_when_width().value(),
        );
    }

    if r.dflt {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    egui::pos2(rect.right() - tag_w, rect.top()),
                    rect.max,
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        child.add_space(FH_ROW_PAD_X.value());
        tag(&mut child, theme, "default", TagVariant::Accent, false);
    }
}

/// 그룹 제목과 개수, 설명. 디자인에 별도 자간 값이 없어 기본 자간을 사용한다.
fn fh_group(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    count: usize,
    caption: Option<&str>,
    attention: bool,
) {
    kit::region(
        ui,
        egui::Margin {
            left: FH_ROW_PAD_X.value() as i8,
            right: FH_ROW_PAD_X.value() as i8,
            top: theme.spacing_sm.value() as i8,
            bottom: theme.spacing_xs.value() as i8,
        },
        |ui| {
            ui.spacing_mut().item_spacing.y = STRUCT_GAP_2.value();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = FH_GAP_SM.value();
                let label_color = if attention {
                    theme.accent_attention()
                } else {
                    theme.text_secondary()
                };
                ui.label(
                    egui::RichText::new(label.to_uppercase())
                        .size(theme.font_size_caption.value())
                        .color(label_color.to_egui()),
                );
                ui.label(
                    egui::RichText::new(count.to_string())
                        .size(theme.font_size_caption.value())
                        .monospace()
                        .color(theme.text_muted().to_egui()),
                );
            });
            if let Some(c) = caption {
                ui.label(
                    egui::RichText::new(c)
                        .size(theme.font_size_caption.value())
                        .color(theme.text_muted().to_egui()),
                );
            }
        },
    );
}

/// 프레임 자기 헤더 — 제목 + 형식 Tag, 그 아래 mono 경로.
fn fh_header(ui: &mut egui::Ui, theme: &Theme, state: FrameState, path: &str) {
    kit::region(
        ui,
        egui::Margin {
            left: FH_EDGE_PAD_X.value() as i8,
            right: FH_EDGE_PAD_X.value() as i8,
            top: FH_HEADER_PAD_TOP.value() as i8,
            bottom: FH_HEADER_PAD_BOTTOM.value() as i8,
        },
        |ui| {
            ui.spacing_mut().item_spacing.y = FH_GAP_SM.value();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                kit::title(ui, theme, "Open file with…");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if state == FrameState::Fallback {
                        tag(ui, theme, "format unknown", TagVariant::Default, false);
                    } else {
                        tag_mono(ui, theme, "markdown", TagVariant::Accent, false);
                    }
                });
            });
            // 파일명이 남도록 앞부분을 생략한 뒤 LTR 한 줄로 그린다.
            let path_font = egui::FontId::monospace(theme.font_size_caption.value());
            let path_h = ui.fonts(|f| f.row_height(&path_font));
            let path_w = ui.available_width();
            let (path_rect, _) =
                ui.allocate_exact_size(egui::vec2(path_w, path_h), egui::Sense::hover());
            paint_truncated(
                ui,
                path_rect.left_top(),
                path,
                path_font,
                theme.text_muted().to_egui(),
                path_w,
            );
        },
    );
    kit::hsep(ui, theme);
}

/// fallback 안내 띠 — 이 선택이 1회성이고 아무것도 등록하지 않는다는 사실.
fn fh_fallback_strip(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .inner_margin(egui::Margin {
            left: FH_EDGE_PAD_X.value() as i8,
            right: FH_EDGE_PAD_X.value() as i8,
            top: theme.spacing_sm.value() as i8,
            bottom: theme.spacing_sm.value() as i8,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                kit::icon(
                    ui,
                    icons::ALERT_TRIANGLE,
                    theme.icon_glyph_size_md,
                    theme.accent_attention().to_egui(),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "One-time choice — nothing is registered for this format. \
                             This screen shows again next time.",
                        )
                        .size(theme.font_size_caption.value())
                        .color(theme.text_secondary_raised().to_egui()),
                    )
                    .wrap(),
                );
            });
        });
    kit::hsep(ui, theme);
}

/// 등록된 핸들러가 없을 때 설정 화면으로 안내한다.
fn fh_empty(ui: &mut egui::Ui, theme: &Theme) {
    kit::region(
        ui,
        egui::Margin {
            left: FH_EDGE_PAD_X.value() as i8,
            right: FH_EDGE_PAD_X.value() as i8,
            top: FH_EMPTY_PAD_Y.value() as i8,
            bottom: FH_EMPTY_PAD_Y.value() as i8,
        },
        |ui| {
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                kit::icon(
                    ui,
                    icons::FILE,
                    theme.icon_glyph_size_md,
                    theme.text_disabled().to_egui(),
                );
                kit::body(ui, theme, crate::i18n::t("file_handler.picker.empty"));
                kit::caption(
                    ui,
                    theme,
                    crate::i18n::t("file_handler.picker.empty_hint"),
                    false,
                );
                ui.add_space(theme.spacing_xs.value());
                Button::new(crate::i18n::t("file_handler.picker.open_settings_button"))
                    .variant(ButtonVariant::Secondary)
                    .size(ControlSize::Sm)
                    .show(ui, theme);
            });
        },
    );
}

/// footer — 확정된 두 버튼. Open 은 선택이 없으면 비활성.
pub(super) fn fh_footer(ui: &mut egui::Ui, theme: &Theme, can_open: bool) {
    kit::hsep(ui, theme);
    kit::region(
        ui,
        egui::Margin {
            left: FH_EDGE_PAD_X.value() as i8,
            right: FH_EDGE_PAD_X.value() as i8,
            top: theme.spacing_sm.value() as i8,
            bottom: theme.spacing_sm.value() as i8,
        },
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_sm.value();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    Button::new("Open")
                        .variant(ButtonVariant::Primary)
                        .enabled(can_open)
                        .show(ui, theme);
                    Button::new("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .show(ui, theme);
                });
            });
        },
    );
}

/// 공통 popup 타이틀바 — **기각된** 읽기의 표본에만 쓴다(경로가 두 번 잘린다).
fn fh_titlebar(ui: &mut egui::Ui, theme: &Theme) {
    egui::Frame::new()
        .fill(theme.bg_sidebar().to_egui())
        .inner_margin(egui::Margin {
            left: FH_ROW_PAD_X.value() as i8,
            right: STRUCT_GAP_2.value() as i8 * 3,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_min_height(theme.item_height_interactive.value());
                ui.label(
                    egui::RichText::new("Open with — …/screens.tsx")
                        .size(theme.font_size_body.value())
                        .color(theme.text_secondary().to_egui()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    kit::icon(
                        ui,
                        icons::CLOSE,
                        theme.icon_glyph_size_md,
                        theme.text_muted().to_egui(),
                    );
                });
            });
        });
    kit::hsep(ui, theme);
}

/// 목록 그룹 사이 구분선 — 좌우 10 안쪽, 위 8.
fn fh_group_rule(ui: &mut egui::Ui, theme: &Theme) {
    ui.add_space(theme.spacing_sm.value());
    kit::region(
        ui,
        egui::Margin {
            left: FH_ROW_PAD_X.value() as i8,
            right: FH_ROW_PAD_X.value() as i8,
            top: 0,
            bottom: 0,
        },
        |ui| kit::hsep(ui, theme),
    );
}

/// 상태 하나를 프레임 하나로 — 디자인 `FileHandlerFrame`.
pub(super) fn frame(ui: &mut egui::Ui, theme: &Theme, state: FrameState, headless: bool) {
    frame_with_path(ui, theme, state, headless, state.path());
}

/// 상태별 기본 경로 대신 지정한 경로로 프레임을 그린다.
pub(super) fn frame_with_path(
    ui: &mut egui::Ui,
    theme: &Theme,
    state: FrameState,
    headless: bool,
    path: &str,
) {
    kit::frame_card(ui, theme, FH_FRAME_WIDTH, kit::panel_fill(theme), |ui| {
        if !headless {
            fh_titlebar(ui, theme);
        }
        fh_header(ui, theme, state, path);
        if state == FrameState::Fallback {
            fh_fallback_strip(ui, theme);
        }
        if state == FrameState::Empty {
            fh_empty(ui, theme);
        } else {
            fh_list(ui, theme, state);
        }
        fh_footer(ui, theme, state != FrameState::Empty);
    });
}

/// 경로 생략 결과를 비교할 수 있도록 헤더만 그린다.
pub(super) fn header_card(ui: &mut egui::Ui, theme: &Theme, path: &str) {
    kit::frame_card(ui, theme, FH_FRAME_WIDTH, kit::panel_fill(theme), |ui| {
        fh_header(ui, theme, FrameState::Default, path);
    });
}

fn fh_list(ui: &mut egui::Ui, theme: &Theme, state: FrameState) {
    let long = state == FrameState::Long;
    let suggested: Vec<Row> = match state {
        FrameState::Mixed => MIXED.to_vec(),
        FrameState::Long => SUGGESTED.iter().chain(LONG_EXTRA).copied().collect(),
        _ => SUGGESTED.to_vec(),
    };
    let draw = |ui: &mut egui::Ui| {
        kit::region(ui, egui::Margin::same(FH_LIST_PAD.value() as i8), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if state == FrameState::Fallback {
                fh_group(
                    ui,
                    theme,
                    "All handlers",
                    ALL.len(),
                    Some(crate::i18n::t("file_handler.picker.fallback_caption")),
                    true,
                );
                for (i, r) in ALL.iter().enumerate() {
                    fh_row(ui, theme, r, i == 0, false);
                }
            } else {
                fh_group(ui, theme, "Suggested", suggested.len(), None, false);
                for (i, r) in suggested.iter().enumerate() {
                    fh_row(ui, theme, r, i == 0, false);
                }
                if state == FrameState::Recent {
                    fh_group_rule(ui, theme);
                    fh_group(
                        ui,
                        theme,
                        "Recent",
                        RECENT.len(),
                        Some("Recently used — not matched to this format."),
                        false,
                    );
                    for r in RECENT {
                        fh_row(ui, theme, r, false, true);
                    }
                }
            }
        });
    };

    if !long {
        draw(ui);
        return;
    }
    // 목록 높이를 먼저 확보해야 부모의 남은 높이에 눌려 한 행으로 줄어들지 않는다.
    let h = FH_LIST_MAX_HEIGHT.value();
    let top = ui.cursor().top();
    ui.allocate_ui(egui::vec2(ui.available_width(), h), |ui| {
        ui.set_min_height(h);
        egui::ScrollArea::vertical()
            .id_salt("fh_long_list")
            .max_height(h)
            .auto_shrink([false, false])
            .drag_to_scroll(false)
            .show(ui, draw);
    });
    let bottom = ui.cursor().top();
    let fade = egui::Rect::from_min_max(
        egui::pos2(
            ui.max_rect().left(),
            (bottom - FH_LIST_FADE_HEIGHT.value()).max(top),
        ),
        egui::pos2(ui.max_rect().right(), bottom),
    );
    paint_bottom_fade(ui, theme, fade);
}

/// 하단 페이드 — 투명 → `bg-panel`. egui 에 gradient 가 없어 얇은 가로 띠를 쌓아 만든다.
fn paint_bottom_fade(ui: &egui::Ui, theme: &Theme, rect: egui::Rect) {
    const STEPS: usize = 10;
    let base = theme.bg_panel().to_egui();
    let step_h = rect.height() / STEPS as f32;
    for i in 0..STEPS {
        let t = (i + 1) as f32 / STEPS as f32;
        let band = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.top() + step_h * i as f32),
            egui::vec2(rect.width(), step_h),
        );
        ui.painter().rect_filled(band, 0.0, base.gamma_multiply(t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_falls_back_to_the_id_segment_after_the_last_slash() {
        assert_eq!(
            row_name(&row("com.tasty.image/viewer", Owner::Host, "image")),
            "viewer"
        );
        assert_eq!(row_name(&row("legacy", Owner::User, "editor")), "legacy");
        assert_eq!(
            row_name(&named("x/y", Owner::Host, "editor", "Text editor")),
            "Text editor"
        );
    }

    /// 공용 ID 생략 함수가 앞부분을 줄이고 끝부분과 최대 문자 수를 지키는지 확인한다.
    #[test]
    fn the_id_is_elided_at_the_front_past_the_limit() {
        let short = "com.tasty.text/editor";
        assert_eq!(fh_model::elide_id_front(short), short);
        let long = "net.example.enterprise.documents.attachments/inline-preview-handler";
        let out = fh_model::elide_id_front(long);
        assert!(out.starts_with('…'), "front-elided: {out}");
        assert!(out.ends_with("handler"), "the tail survives: {out}");
        assert_eq!(
            out.chars().count(),
            tasty_ui_widgets::tokens::FH_ID_ELIDE_MAX
        );
    }

    #[test]
    fn a_declared_icon_wins_only_when_it_names_a_known_glyph() {
        let declared = Row {
            icon: Some("image"),
            ..row("a/b", Owner::Plugin, "binary")
        };
        assert_eq!(row_icon(&declared).uri, icons::IMAGE.uri);
        let bogus = Row {
            icon: Some("not-a-glyph"),
            ..row("a/b", Owner::Plugin, "binary")
        };
        assert_eq!(row_icon(&bogus).uri, icons::LAYERS.uri);
        assert_eq!(
            row_icon(&row("a/b", Owner::Host, "nope")).uri,
            icons::FILE.uri
        );
    }
}
