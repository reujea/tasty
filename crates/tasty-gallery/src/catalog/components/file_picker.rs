//! 로컬·원격 파일 선택과 저장 화면의 정적 예제. 본체의 디렉터리 조회를 실행하지 않는다.
//! 원격 대상은 호스트 배지로 구분한다. 선택·저장 상태와 긴 경로 표시를 비교한다.

mod filter_chip;
mod folder;
mod footer;
mod indicator;
mod path_bar;
mod path_fit;

pub use folder::draw_folder_mode;

pub use filter_chip::draw_filter_chip;
use indicator::Indicator;
pub use indicator::draw_remote_indicator;
use path_bar::PathKind;
pub use path_fit::draw_path_fit;

use tasty_egui_theme::overlay_edge::BoxEdge;
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{CenterState, ControlSize, IconButton, IconButtonVariant, checkbox};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

// 화면 전용 고정 치수. 대응 토큰이 없는 값은 디자인 값을 유지한다.
const FRAME_W: LogicalPx = LogicalPx(640.0);
const FRAME_H: LogicalPx = LogicalPx(480.0);
/// `…` 메뉴 예제 카드의 높이 — 디자인 `FilePickerFrame mode="save" deep crumbMenu w={320} h={420}`.
const CRUMB_MENU_H: LogicalPx = LogicalPx(420.0);
const SIZE_COL_W: LogicalPx = LogicalPx(68.0);
const MOD_COL_W: LogicalPx = LogicalPx(108.0);
/// 폴더 선택 갈래에서 고른 행 — 디자인 seed(`overlays-shared.jsx` `folderSel`).
const FOLDER_SEL: &str = "configs";

/// 원격 host 배지 칩의 높이(디자인 size-22). 4px 그리드 밖이고 대응 Theme 토큰이
/// 없다 — 칩 하나의 구조 높이라 spacing 리듬 값이 아니다.
const HOST_BADGE_H: LogicalPx = LogicalPx(22.0);

/// 디자인에서 정한 13px 아이콘. 대응 Theme 크기 토큰이 없어 그대로 사용한다.
const CRUMB_GLYPH: LogicalPx = LogicalPx(13.0);

const HOST: &str = "deploy@10.0.4.12";

/// 헤더 높이. 고정값이 없고 위아래 `fp-header-pad-y`와 sm IconButton으로 정해진다.
/// host 배지(22)는 이 안에 들어간다.
fn header_height(theme: &Theme) -> LogicalPx {
    let pad = theme.fp_header_pad_y();
    pad + LogicalPx(ControlSize::Sm.height(theme)) + pad
}

/// 경로 막대 높이. 위아래 `fp-path-pad-y`와 sm 새로고침 IconButton으로 정해진다.
fn path_bar_height(theme: &Theme) -> LogicalPx {
    let pad = theme.fp_path_pad_y();
    pad + LogicalPx(ControlSize::Sm.height(theme)) + pad
}

/// 목록 머리 높이. 위아래 `fp-list-head-pad-y`와 micro 라벨 한 줄(`line-height-ui`)로 정해진다.
fn list_head_height(theme: &Theme) -> LogicalPx {
    let pad = theme.fp_list_head_pad_y();
    pad + theme.font_size_micro.scaled(theme.line_height_ui) + pad
}

/// 목록 행 높이. 위아래 `fp-row-pad-y`와 아이콘·이름 줄(`line-height-ui`) 중 높은 쪽으로 정해진다.
fn row_height(theme: &Theme) -> LogicalPx {
    let pad = theme.fp_row_pad_y();
    let name = theme.font_size_body.scaled(theme.line_height_ui);
    pad + name.max(theme.icon_glyph_size_md) + pad
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FpState {
    Loaded,
    Loading,
    Empty,
    ErrorPerm,
    ErrorConn,
}

struct Row {
    folder: bool,
    name: &'static str,
    size: &'static str,
    modified: &'static str,
}

// 디자인 FilePickerFrame `files` seed 1:1 (overlays-shared.jsx).
const FILES: &[Row] = &[
    Row {
        folder: true,
        name: "configs",
        size: "—",
        modified: "Jul 12 09:14",
    },
    Row {
        folder: true,
        name: "logs",
        size: "—",
        modified: "Jul 14 22:03",
    },
    Row {
        folder: true,
        name: "node_modules",
        size: "—",
        modified: "Jul 02 11:40",
    },
    Row {
        folder: false,
        name: "README.md",
        size: "4.2 KB",
        modified: "Jul 15 08:21",
    },
    Row {
        folder: false,
        name: "package.json",
        size: "1.1 KB",
        modified: "Jul 15 08:21",
    },
    Row {
        folder: false,
        name: "pipeline.yaml",
        size: "3.8 KB",
        modified: "Jul 14 17:55",
    },
    Row {
        folder: false,
        name: "deploy.sh",
        size: "902 B",
        modified: "Jul 11 14:02",
    },
    Row {
        folder: false,
        name: ".env",
        size: "218 B",
        modified: "Jul 09 10:30",
    },
];

// 디자인 multi 상태 checked seed (README.md / package.json / pipeline.yaml).
const MULTI_PICKED: &[&str] = &["README.md", "package.json", "pipeline.yaml"];

/// 무엇을 하는 피커인가 — 디자인 `FilePickerFrame` `mode`/`save` prop.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Open,
    Save(SaveState),
    Folder {
        selected: bool,
        lang: folder::Language,
        long: bool,
    },
}

/// 저장 모드의 이름 칸 상태 — 디자인 `save="new"|"picked"|"edited"`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveState {
    /// 새 이름을 입력했다 — 목록 선택 없음, 버튼 Save.
    New,
    /// 목록에서 기존 파일을 골랐다 — 이름 칸에 그 이름, 곧 덮어쓰기 상태.
    Picked,
    /// 고른 뒤 이름을 고쳤다 — 선택이 풀리고 버튼이 Save 로 돌아온다.
    Edited,
}

impl SaveState {
    fn name(self) -> &'static str {
        match self {
            SaveState::Picked => "pipeline.yaml",
            SaveState::Edited => "pipeline-v2.yaml",
            SaveState::New => "keybindings-2026-09-14.toml",
        }
    }

    fn selected(self) -> Option<&'static str> {
        match self {
            SaveState::Picked => Some("pipeline.yaml"),
            _ => None,
        }
    }
}

/// 카드 한 장의 변형 — 디자인 `FilePickerFrame` prop 묶음.
#[derive(Clone, Copy)]
struct Variant {
    state: FpState,
    remote: bool,
    /// 원격 표시 방식 — 디자인 `FilePickerFrame indicator`. 로컬이면 쓰지 않는다.
    indicator: Indicator,
    multi: bool,
    mode: Mode,
    /// 경로 데이터 — 디자인 `remote` · `deep` · `pathKind`.
    path: PathKind,
    /// 접힌 조상의 `…` 메뉴를 펼친 상태 — 디자인 `crumbMenu`.
    crumb_menu: bool,
    /// 고른 행이 **폴더**다 — 디자인 `FilePickerFrame folderSel`. 두 모드에서 뜻이 다르다:
    /// 저장은 "이것은 저장 대상이 아니다", 열기는 "확정하면 들어간다".
    folder_sel: bool,
    /// 호출자가 넘긴 확장자 필터 — 디자인 `FilePickerFrame filters`. 비면 칩을 그리지 않는다.
    filters: &'static [&'static str],
    /// 카드 크기 — 디자인 `FilePickerFrame w`/`h`.
    w: LogicalPx,
    h: LogicalPx,
}

impl Variant {
    const fn open(state: FpState, remote: bool, multi: bool) -> Self {
        Self {
            state,
            remote,
            indicator: Indicator::Badge,
            multi,
            mode: Mode::Open,
            path: if remote {
                PathKind::Remote
            } else {
                PathKind::Local
            },
            crumb_menu: false,
            folder_sel: false,
            filters: &[],
            w: FRAME_W,
            h: FRAME_H,
        }
    }

    const fn save(save: SaveState, deep: bool) -> Self {
        Self {
            state: FpState::Loaded,
            remote: false,
            indicator: Indicator::Badge,
            multi: false,
            mode: Mode::Save(save),
            path: if deep {
                PathKind::Deep
            } else {
                PathKind::Local
            },
            crumb_menu: false,
            folder_sel: false,
            filters: &[],
            w: FRAME_W,
            h: FRAME_H,
        }
    }

    /// 목록에서 고른 것이 폴더인 갈래 — 두 모드 모두 단일 클릭이 선택이므로 둘 다 성립한다.
    const fn folder_selected(mut self) -> Self {
        self.folder_sel = true;
        self
    }

    const fn filtered(mut self, filters: &'static [&'static str]) -> Self {
        self.filters = filters;
        self
    }

    const fn path(mut self, path: PathKind) -> Self {
        self.path = path;
        self
    }

    const fn crumb_menu_open(mut self) -> Self {
        self.crumb_menu = true;
        self
    }

    const fn indicated(mut self, indicator: Indicator) -> Self {
        self.indicator = indicator;
        self
    }

    /// 원격 표시가 프레임 테두리(C안)인가.
    fn border_mode(self) -> bool {
        self.remote && self.indicator == Indicator::Border
    }

    const fn sized(mut self, w: LogicalPx, h: LogicalPx) -> Self {
        self.w = w;
        self.h = h;
        self
    }

    fn overwrite(self) -> bool {
        matches!(self.mode, Mode::Save(SaveState::Picked))
    }
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "local", |ui| {
            card(ui, theme, Variant::open(FpState::Loaded, false, false));
        });
        spec::cluster(ui, theme, "remote (host badge)", |ui| {
            card(ui, theme, Variant::open(FpState::Loaded, true, false));
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "640×480 · PopupDef · bg-panel"),
            ("header", "glyph · title · host badge(remote) · ✕"),
            ("path bar", "breadcrumb + refresh · bg-sidebar"),
            ("row", "checkbox? · icon · name · size · modified"),
            ("footer", "name field · type filter · Cancel / Open"),
            ("open", "enabled only with a selection"),
            ("dismiss", "×/Cancel/Esc · Open = Enter"),
        ],
        &[
            TokenChip::new("bg-panel", "frame", theme.bg_panel().to_egui()),
            TokenChip::new("bg-sidebar", "path bar", theme.bg_sidebar().to_egui()),
            TokenChip::new(
                "surface-active",
                "selected row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "selected bar · folder glyph · Open · crumb link",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "accent-info",
                "remote host badge",
                theme.accent_info().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Local and remote file pickers share the layout and use a host badge to distinguish the target. The host has an attach-based remote directory-listing path. This gallery renders fixed example data and does not query either filesystem.",
    );
}

pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "loading (remote)", |ui| {
            card(ui, theme, Variant::open(FpState::Loading, true, false));
        });
        spec::cluster(ui, theme, "empty folder", |ui| {
            card(ui, theme, Variant::open(FpState::Empty, false, false));
        });
        spec::cluster(ui, theme, "permission denied (local)", |ui| {
            card(ui, theme, Variant::open(FpState::ErrorPerm, false, false));
        });
        spec::cluster(ui, theme, "connection lost (remote)", |ui| {
            card(ui, theme, Variant::open(FpState::ErrorConn, true, false));
        });
        spec::cluster(ui, theme, "multi-select (remote)", |ui| {
            card(ui, theme, Variant::open(FpState::Loaded, true, true));
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("loading", "Spinner · reads dir (remote: over SSH)"),
            ("empty", "folderOpen glyph · CenterState"),
            (
                "error",
                "danger glyph · title · reason · action below the block",
            ),
            ("perm vs. conn", "Retry vs. Reconnect (resumes)"),
            ("multi", "checkbox col · joined names · N selected"),
            ("Open", "disabled while loading / error / empty"),
        ],
        &[
            TokenChip::new(
                "center-state-error-fg",
                "error glyph",
                theme.center_state_error_fg().to_egui(),
            ),
            TokenChip::new(
                "center-state-glyph-fg",
                "empty glyph · spinner",
                theme.center_state_glyph_fg().to_egui(),
            ),
            TokenChip::without_color("spinner-track", "loading spinner"),
        ],
    );

    spec::note(
        ui,
        theme,
        "The loaded example marks pipeline.yaml with a keyboard focus ring, separate from the selected row background. The multi-select example shows checkboxes, joined names and a selected count; it does not perform file operations.",
    );
}

pub fn draw_save_mode(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(ui, theme, "save — typed name (new file)", |ui| {
            card(ui, theme, Variant::save(SaveState::New, false));
        });
        spec::cluster(
            ui,
            theme,
            "save — existing file picked → Overwrite",
            |ui| {
                card(ui, theme, Variant::save(SaveState::Picked, false));
            },
        );
        spec::cluster(
            ui,
            theme,
            "save — name edited after the pick → selection cleared",
            |ui| {
                card(ui, theme, Variant::save(SaveState::Edited, false));
            },
        );
        spec::cluster(ui, theme, "deep path — fits at 640, nothing folds", |ui| {
            card(ui, theme, Variant::save(SaveState::New, true));
        });
    });

    spec::meta(
        ui,
        theme,
        &[
            ("title", "Save file (open mode: Open file)"),
            ("confirm", "footer primary only — one control"),
            ("labels", "Save · Overwrite when the name exists"),
            ("input", "editable · placeholder “Type a file name”"),
            ("list pick", "writes the name into the input → Overwrite"),
            ("selection", "clears as soon as the name diverges → Save"),
            ("overwrite", "11px warning line above the buttons"),
            ("disabled", "Save disabled while the name is empty"),
            (
                "breadcrumb",
                "folds one ancestor at a time only when it doesn't fit",
            ),
            ("footer", "never shrinks — the input absorbs it"),
        ],
        &[
            TokenChip::new(
                "accent-warning",
                "overwrite line",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new("input-bg", "name field", theme.input_bg().to_egui()),
            TokenChip::new(
                "text-placeholder",
                "empty name",
                theme.text_placeholder().to_egui(),
            ),
            TokenChip::new(
                "separator",
                "footer rule",
                theme.separator.to_egui_premultiplied(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "A path that does not fit folds its ancestors into a … menu one at a time, from the middle; a path that fits stays whole. The path field takes the remaining width after the refresh button. In the footer, the name field shrinks while the labels, filter and action buttons retain their width.",
    );
}

pub fn draw_gesture_table(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        spec::cluster(
            ui,
            theme,
            "save — folder selected · not a save target",
            |ui| {
                card(
                    ui,
                    theme,
                    Variant::save(SaveState::New, false).folder_selected(),
                );
            },
        );
        spec::cluster(
            ui,
            theme,
            "open — folder selected · Open enters it",
            |ui| {
                card(
                    ui,
                    theme,
                    Variant::open(FpState::Loaded, false, false).folder_selected(),
                );
            },
        );
        // 640 에서는 깊은 경로가 다 들어가 접히지 않는다 — 피커 바닥 폭(320)에서 조상이 접힌다.
        spec::cluster(
            ui,
            theme,
            "… menu open — the hidden ancestors (320 · fp-popup-min-width)",
            |ui| {
                card(
                    ui,
                    theme,
                    Variant::save(SaveState::New, true)
                        .sized(theme.fp_popup_min_width(), CRUMB_MENU_H)
                        .crumb_menu_open(),
                );
            },
        );
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "single click",
                "selects the row — file or folder, both modes",
            ),
            ("double click folder", "descends — both modes"),
            ("double click file", "open: confirms · save: selects only"),
            (
                "folder + Save",
                "never a target — muted footer line, button unchanged",
            ),
            ("folder + Open", "enters it — the keyboard route to descend"),
            ("name field", "open: empty while a folder is selected"),
            (
                "elision",
                "overflow-driven — one ancestor at a time, no depth threshold",
            ),
            (
                "… tooltip",
                "Show 3 hidden folders (singular: 1 hidden folder)",
            ),
            (
                "… menu",
                "content-measured, 180–320 band = BORDER-BOX outer width, path order",
            ),
            (
                "… menu rows",
                "shared MenuItem — 28 · pad-x 12 · folder 16 · menu-item-fg (hover menu-item-fg-hover)",
            ),
            (
                "specimen",
                "320 (fp-popup-min-width) — the width where it folds",
            ),
            (
                "tone",
                "the folder line has none — it is a fact, not a warning",
            ),
        ],
        &[
            TokenChip::new(
                "text-muted",
                "folder-target line",
                theme.text_muted_raised().to_egui(),
            ),
            TokenChip::new(
                "text-secondary",
                "the button named in the open line",
                theme.text_secondary_raised().to_egui(),
            ),
            TokenChip::new(
                "overlay-active",
                "selected row bed",
                theme.overlay_active().to_egui_premultiplied(),
            ),
            TokenChip::new("menu-bg", "… menu fill", theme.menu_bg().to_egui()),
            TokenChip::new("menu-border", "… menu edge", theme.menu_border().to_egui()),
            TokenChip::without_color("menu-item-padding-x", "row pad-x 12"),
            TokenChip::without_color("fp-crumb-menu-max-width", "320 — … menu ceiling (NEW)"),
            TokenChip::without_color("fp-crumb-menu-min-width", "180 — … menu floor (NEW)"),
            TokenChip::without_color("fp-crumb-max-width", "180 — one crumb's cap (NEW)"),
        ],
    );

    spec::note(
        ui,
        theme,
        "Single click selects a file or folder. Double click enters a folder in either mode. A selected folder is not a save target: Save uses the name field, while Open enters the selected folder. The footer explains this difference.",
    );

    spec::dont(
        ui,
        theme,
        "Don't confirm on a file's double-click in save mode. The double-click writes the name, \
         and that name is what raises the overwrite warning — confirming in the same gesture \
         would write over a file before its warning could be read.",
    );
}

/// 카드 한 장. 크기는 변형이 정한다(기본 640×480).
fn card(ui: &mut egui::Ui, theme: &Theme, v: Variant) {
    // 일반 예제는 메뉴 사각형을 쓰지 않는다.
    let _menu = card_edge(ui, theme, v, BoxEdge::NORMAL);
}

/// 깊은 경로의 열기 카드에 `…` 메뉴를 펼친 한 장. WebView 페이지 위 예제가 쓴다.
/// 펼친 메뉴 상자의 사각형을 돌려준다.
pub fn deep_card_with_menu(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: LogicalPx,
    h: LogicalPx,
    edge: BoxEdge,
) -> Option<egui::Rect> {
    let v = Variant::open(FpState::Loaded, false, false)
        .path(PathKind::Deep)
        .crumb_menu_open()
        .sized(w, h);
    card_edge(ui, theme, v, edge)
}

fn card_edge(ui: &mut egui::Ui, theme: &Theme, v: Variant, edge: BoxEdge) -> Option<egui::Rect> {
    let mut menu = None;
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            if v.border_mode() {
                theme.accent_info().to_egui()
            } else {
                theme.border_strong().to_egui()
            },
        ))
        .corner_radius(edge.radius(theme, theme.corner_radius).value())
        .shadow(edge.shadow(theme, theme.shadow_modal()).to_egui())
        .show(ui, |ui| {
            ui.set_width(v.w.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                ui.set_width(v.w.value());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                let strip_h = indicator::top_strip(ui, theme, v);
                header(ui, theme, v);
                let hidden = path_bar::path_bar(ui, theme, v);
                let bar_bottom = ui.min_rect().bottom();
                // footer 를 먼저 재고 남은 높이를 본문에 준다 — 덮어쓰기 경고 줄이 footer 를
                // 키우면 본문이 줄지 footer 가 밀려나지 않는다.
                let footer_h = footer::footer_height(ui, theme, v);
                body(
                    ui,
                    theme,
                    v,
                    v.h - strip_h - header_height(theme) - path_bar_height(theme) - footer_h,
                );
                footer::footer(ui, theme, v, footer_h);
                // 메뉴는 본문 위에 떠야 하므로 본문·footer 다음에 그린다.
                if let Some(hidden) = hidden.filter(|_| v.crumb_menu) {
                    menu = Some(path_bar::crumb_menu(ui, theme, &hidden, bar_bottom, edge));
                }
            });
        });
    menu
}

fn header(ui: &mut egui::Ui, theme: &Theme, v: Variant) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(v.w.value(), header_height(theme).value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + theme.fp_inset_start().value(),
            rect.top() + theme.fp_header_pad_y().value(),
        ),
        egui::pos2(
            rect.right() - theme.fp_inset_end().value(),
            rect.bottom() - theme.fp_header_pad_y().value(),
        ),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.fp_section_gap().value();
    let (glyph, glyph_fg) = indicator::header_glyph(theme, v);
    kit::icon(&mut child, glyph, theme.icon_glyph_size_md, glyph_fg);
    kit::title(
        &mut child,
        theme,
        match v.mode {
            Mode::Open => "Open file",
            Mode::Save(_) => "Save file",
            Mode::Folder { .. } => folder::props(v).confirm,
        },
    );
    indicator::after_title(&mut child, theme, v);
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .size(ControlSize::Sm)
            .show(ui, theme, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
            });
    });
}

/// 원격 user@host를 표시하는 배지.
fn host_badge(ui: &mut egui::Ui, theme: &Theme, host: &str) {
    let info = theme.accent_info().to_egui();
    let font = egui::FontId::monospace(theme.font_size_caption.value());
    let galley = ui
        .painter()
        .layout_no_wrap(host.to_owned(), font, egui::Color32::PLACEHOLDER);
    let glyph = theme.icon_glyph_size_xs.value();
    let gap = theme.spacing_xs.value();
    let pad_x = theme.spacing_sm.value();
    let h = HOST_BADGE_H.value();
    let w = pad_x * 2.0 + glyph + gap + galley.rect.width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let radius = theme.corner_radius.value();
    ui.painter()
        .rect_filled(rect, radius, info.gamma_multiply(theme.tint_fill_alpha()));
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(
            theme.border_width.value(),
            info.gamma_multiply(theme.tint_border_alpha()),
        ),
        egui::StrokeKind::Inside,
    );
    let gy = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pad_x, rect.center().y - glyph * 0.5),
        egui::vec2(glyph, glyph),
    );
    icons::REMOTE.image(glyph, info).paint_at(ui, gy);
    let pos = egui::pos2(
        rect.left() + pad_x + glyph + gap,
        rect.center().y - galley.rect.height() * 0.5,
    );
    ui.painter().galley(pos, galley, info);
}

/// 행 컬럼 x좌표 — list header 와 `row` 가 동일 레이아웃을 공유.
struct Cols {
    checkbox_x: Option<LogicalPx>,
    icon_x: LogicalPx,
    name_left: LogicalPx,
    /// name 컬럼 우측 한계(= size 컬럼 좌측 - gap) — 디자인 `flex:1; max-width:0;
    /// overflow:hidden; ellipsis` 흉내(긴 이름 말줄임)에 쓰인다.
    name_right: LogicalPx,
    size_right: LogicalPx,
    mod_right: LogicalPx,
}

fn cols(rect: egui::Rect, theme: &Theme, multi: bool) -> Cols {
    let pad = theme.fp_inset_start();
    let gap = theme.spacing_sm;
    let glyph = theme.icon_glyph_size_md;
    let mut x = LogicalPx(rect.left()) + pad;
    let checkbox_x = if multi {
        let cx = x;
        x += glyph + gap;
        Some(cx)
    } else {
        None
    };
    let icon_x = x;
    x += glyph + gap;
    let name_left = x;
    let mod_right = LogicalPx(rect.right()) - pad;
    let size_right = mod_right - MOD_COL_W - gap;
    let name_right = size_right - SIZE_COL_W - gap;
    Cols {
        checkbox_x,
        icon_x,
        name_left,
        name_right,
        size_right,
        mod_right,
    }
}

/// 폭이 `max_w` 를 넘으면 문자 단위로 잘라 `…` 을 붙인다 (디자인 `text-overflow:
/// ellipsis` 흉내 — FpRow name 컬럼).
fn elide(ui: &egui::Ui, text: &str, font: egui::FontId, max_w: LogicalPx) -> String {
    let measure = |s: &str| {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .rect
            .width()
    };
    if max_w <= LogicalPx(0.0) || measure(text) <= max_w.value() {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let candidate: String = chars.iter().collect::<String>() + "…";
        if measure(&candidate) <= max_w.value() {
            return candidate;
        }
    }
    "…".to_owned()
}

fn list_header(ui: &mut egui::Ui, theme: &Theme, w: LogicalPx, multi: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(w.value(), list_head_height(theme).value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let c = cols(rect, theme, multi);
    let font = tasty_ui_widgets::label_font(theme.font_size_micro.value());
    let muted = theme.text_muted().to_egui();
    ui.painter().text(
        egui::pos2(c.name_left.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        "NAME",
        font.clone(),
        muted,
    );
    ui.painter().text(
        egui::pos2(c.size_right.value(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "SIZE",
        font.clone(),
        muted,
    );
    ui.painter().text(
        egui::pos2(c.mod_right.value(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "MODIFIED",
        font,
        muted,
    );
}

struct RowState {
    multi: bool,
    checked: bool,
    selected: bool,
    focus: bool,
    inert: bool,
    file_tooltip: &'static str,
}
fn row(ui: &mut egui::Ui, theme: &Theme, r: &Row, state: RowState) {
    let RowState {
        multi,
        checked,
        selected,
        focus,
        inert,
        file_tooltip,
    } = state;
    let w = ui.available_width();
    let h = row_height(theme);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h.value()), egui::Sense::hover());
    if inert {
        response.on_hover_text(file_tooltip);
    }
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme.selection_edge_width.value(), rect.height()),
        );
        ui.painter()
            .rect_filled(bar, 0.0, theme.accent_primary().to_egui());
    }
    if focus {
        // outlineOffset -1 흉내 — rect 안쪽 1px.
        ui.painter().rect_stroke(
            rect.shrink(theme.border_width.value() * 0.5),
            0.0,
            egui::Stroke::new(theme.border_width.value(), theme.accent_primary().to_egui()),
            egui::StrokeKind::Inside,
        );
    }
    let c = cols(rect, theme, multi);
    let glyph_size = theme.icon_glyph_size_md.value();
    if let Some(cx) = c.checkbox_x {
        let mut chk = checked;
        let cb_rect = egui::Rect::from_min_size(
            egui::pos2(cx.value(), rect.center().y - glyph_size * 0.5),
            egui::vec2(glyph_size, glyph_size),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cb_rect));
        checkbox(
            &mut child,
            theme,
            &mut chk,
            "",
            true,
            tasty_ui_widgets::Ground::Panel,
        );
    }
    let (icon_glyph, icon_color): (MockGlyph, egui::Color32) = if r.folder {
        (
            icons::FOLDER,
            if selected {
                theme.text_on_active()
            } else {
                theme.accent_primary()
            }
            .to_egui(),
        )
    } else {
        (
            icons::FILE,
            if inert {
                theme.fp_folder_file_fg()
            } else if selected {
                theme.text_on_active()
            } else {
                theme.text_muted()
            }
            .to_egui(),
        )
    };
    let ir = egui::Rect::from_min_size(
        egui::pos2(c.icon_x.value(), rect.center().y - glyph_size * 0.5),
        egui::vec2(glyph_size, glyph_size),
    );
    icon_glyph.image(glyph_size, icon_color).paint_at(ui, ir);
    let name_color = if inert {
        theme.fp_folder_file_fg()
    } else if selected {
        theme.text_on_active()
    } else {
        theme.text_secondary()
    };
    let name_font = egui::FontId::proportional(theme.font_size_body.value());
    let name_text = elide(ui, r.name, name_font.clone(), c.name_right - c.name_left);
    ui.painter().text(
        egui::pos2(c.name_left.value(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        name_text,
        name_font,
        name_color.to_egui(),
    );
    let mono_caption = egui::FontId::monospace(theme.font_size_caption.value());
    let muted = if inert {
        theme.fp_folder_file_fg()
    } else if selected {
        theme.text_on_active()
    } else {
        theme.text_muted()
    }
    .to_egui();
    ui.painter().text(
        egui::pos2(c.size_right.value(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        r.size,
        mono_caption.clone(),
        muted,
    );
    ui.painter().text(
        egui::pos2(c.mod_right.value(), rect.center().y),
        egui::Align2::RIGHT_CENTER,
        r.modified,
        mono_caption,
        muted,
    );
}

fn body(ui: &mut egui::Ui, theme: &Theme, v: Variant, body_h: LogicalPx) {
    let multi = v.multi;
    match v.state {
        FpState::Loaded => {
            list_header(ui, theme, v.w, multi);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(
                    v.w.value(),
                    (body_h - list_head_height(theme)).value(),
                ),
                egui::Sense::hover(),
            );
            let mut col = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            col.set_clip_rect(rect.intersect(ui.clip_rect()));
            col.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for f in FILES {
                let (selected, focus) = if v.folder_sel {
                    (f.name == FOLDER_SEL, false)
                } else {
                    match v.mode {
                        Mode::Save(save) => (save.selected() == Some(f.name), false),
                        Mode::Folder {selected, ..} => (selected && f.name == FOLDER_SEL, false),
                        Mode::Open => (
                            !multi && f.name == "README.md",
                            !multi && f.name == "pipeline.yaml",
                        ),
                    }
                };
                let checked = multi && MULTI_PICKED.contains(&f.name);
                row(&mut col, theme, f, RowState {multi, checked, selected, focus, inert: matches!(v.mode, Mode::Folder {..}) && !f.folder, file_tooltip: folder::file_tooltip(v)});
            }
        }
        FpState::Loading => center(
            ui,
            theme,
            v.w,
            body_h,
            CenterState::loading("Loading folder…").sub_line(Some(if matches!(v.mode, Mode::Folder { .. }) { crate::i18n::t("filepicker.loading_body_local") } else { "Reading the directory over SSH." })),
        ),
        FpState::Empty => center(
            ui,
            theme,
            v.w,
            body_h,
            CenterState::empty(icons::FOLDER_OPEN, if matches!(v.mode, Mode::Folder {..}) {folder::empty(v)} else {crate::i18n::t("explorer.state.empty")}),
        ),
        FpState::ErrorPerm => center(
            ui,
            theme,
            v.w,
            body_h,
            CenterState::error("Permission denied")
                .sub_line(Some(
                    "You don't have permission to read this folder. Try a different folder or check access.",
                ))
                .action("Retry", Some(icons::REFRESH)),
        ),
        FpState::ErrorConn => center(
            ui,
            theme,
            v.w,
            body_h,
            CenterState::error("Remote connection lost")
                .sub_line(Some(
                    "The SSH tunnel dropped. Reconnect to resume browsing from the last folder.",
                ))
                .action("Reconnect", Some(icons::REFRESH)),
        ),
    }
}

/// 목록 자리에 공용 CenterState 를 본체와 같은 폭으로 그린다.
fn center(
    ui: &mut egui::Ui,
    theme: &Theme,
    w: LogicalPx,
    body_h: LogicalPx,
    state: CenterState<'_>,
) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(w.value(), body_h.value()), egui::Sense::hover());
    state.show_in(ui, theme, rect);
}
