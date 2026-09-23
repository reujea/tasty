//! `file_picker` specimen — Native file picker (local & remote), gallery-first 1단계.
//!
//! 권위 원본: `gallery/overlays-shared.jsx` `FilePickerFrame`/`FpRow`/`FpCrumbs`/
//! `FpHostBadge`, `gallery/overlays-windows.jsx` `#filepicker` Section(스펙 3개).
//! 매핑: `design-gallery-mapping.md` "file_picker".
//!
//! 640×480 단일 컴포넌트가 로컬/원격 두 모드를 겸한다 — 차이는 헤더 host indicator와
//! 브레드크럼 root뿐, 레이아웃은 불변. 원격 표시는 디자인의 열린 결정(A 배지 / B 글리프 /
//! C 프레임보더) 중 **A 배지가 사용자 확정**되어 이 specimen 은 A만 반영한다 —
//! B/C 는 미채택 대안이라 코드화하지 않는다.
//!
//! **본체 구현**: `src/adapters/ui/popup/file_picker.rs`(`PopupDef` id
//! `"file_picker"`, Tools 메뉴 트리거). 이 specimen 은 mock 데이터로 독립 렌더 —
//! 본체와 코드 공유는 하지 않는다(`file_handler_picker` 갤러리 specimen과 동일
//! 관례). 원격 채널 설계는 `docs/adr/0053-native-file-picker-remote-attach-channel.md`.
//!
//! `draw` = 개요(로컬/원격 loaded 나란히). `draw_states` = loading·empty·
//! permission-denied·connection-lost·multi-select 5상태. 키보드 focus-ring 은
//! loaded 프레임의 `pipeline.yaml` 행에 상시 표시(selection 과 시각 구분).
//! `draw_gesture_table` = 폴더를 고른 두 모드(저장: 대상이 아니다 · 열기: 들어간다).
//! `draw_save_mode` = 저장 모드 4상태(새 이름 · 기존 파일 선택 → 덮어쓰기 · 선택 후 이름 수정 ·
//! 깊은 경로의 가운데 생략). 저장 모드의 확정 수단은 footer 의 primary 버튼 **하나**이고,
//! 목록 행 선택은 확정이 아니라 이름 칸을 채운다.
//!
//! **긴 경로 축소 규칙**(열기 모드에도 적용): 넘침은 path bar 가 흡수한다 — breadcrumb 이
//! 유일한 가변 자식(`flex:1; min-width:0; overflow:hidden`)이고 깊으면 가운데를 접는다
//! (root + `…` + 마지막 두 성분). footer 의 라벨·버튼은 줄지 않고 이름 칸만 준다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::{
    CENTER_BLOCK_H_SPECIMEN as EMPTY_BLOCK_H, CENTER_GLYPH_SIZE as EMPTY_GLYPH,
};
use tasty_ui_widgets::{Button, ButtonVariant, IconButton, IconButtonVariant, Spinner, checkbox};

use crate::catalog::icons::{self, MockGlyph};
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

mod footer;
mod path_bar;

// ── 프레임 고정 치수 (디자인 raw px 근사 — 화면 전용 고정값, token-policy §c) ──
const FRAME_W: LogicalPx = LogicalPx(640.0);
const FRAME_H: LogicalPx = LogicalPx(480.0);
const HEADER_H: LogicalPx = LogicalPx(44.0); // padding ~8/8(디자인 10/10 근사) + content(host badge 22 최대)
const HEADER_PAD_L: LogicalPx = LogicalPx(14.0); // 디자인 L14
const PATH_H: LogicalPx = LogicalPx(36.0); // padding ~6/6 + refresh IconButton(sm)
const LIST_HEAD_H: LogicalPx = LogicalPx(26.0); // caption row — loaded/multi 상태만
const FOOTER_H: LogicalPx = LogicalPx(84.0); // name row(28) + gap(8) + action row(28) + padding 10/10 근사 — 덮어쓰기 경고 줄은 `footer_height` 가 더한다
const ROW_H: LogicalPx = LogicalPx(28.0); // FpRow padding 6/space-md + content 16
const SIZE_COL_W: LogicalPx = LogicalPx(68.0);
const MOD_COL_W: LogicalPx = LogicalPx(108.0);
const FOOTER_LABEL_W: LogicalPx = LogicalPx(64.0); // 디자인 "File name" 라벨 고정폭
/// 브레드크럼 한 성분의 최대 폭(디자인 `FpCrumbs` span `maxWidth:180`, 넘치면 말줄임).
/// 대응 Theme 토큰이 없는 구조 폭이다.
const CRUMB_MAX_W: LogicalPx = LogicalPx(180.0);
const FOOTER_CHIP_W: LogicalPx = LogicalPx(92.0); // "All files ▾" 타입필터 칩
/// 폴더 선택 갈래에서 고른 행 — 디자인 seed(`overlays-shared.jsx` `folderSel`).
const FOLDER_SEL: &str = "configs";

/// 원격 host 배지 칩의 높이(디자인 size-22). 4px 그리드 밖이고 대응 Theme 토큰이
/// 없다 — 칩 하나의 구조 높이라 spacing 리듬 값이 아니다.
const HOST_BADGE_H: LogicalPx = LogicalPx(22.0);

/// 브레드크럼 구분자·타입필터 칩 화살표의 글리프 한 변. **아이콘 스케일 밖이다** —
/// Theme 은 12(xs) · 14(sm) · 15(row-action) · 16(md) 만 갖는데 디자인은 여기 13 을
///쓴다. 조용히 12/14 로 반올림하지 않고 값을 보존한 채 이름만 붙였다.
const CRUMB_GLYPH: LogicalPx = LogicalPx(13.0);

/// 그 블록 본문 텍스트의 최대 폭 — 한 줄이 너무 길어지지 않게 잡는 값.
const EMPTY_BODY_MAX_W: LogicalPx = LogicalPx(340.0);
const HOST: &str = "deploy@10.0.4.12";

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
    // 디자인 seed 1:1 (overlays-shared.jsx `saveName`).
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
    multi: bool,
    mode: Mode,
    /// 깊은 경로 — breadcrumb 가운데 생략.
    deep: bool,
    /// 고른 행이 **폴더**다 — 디자인 `FilePickerFrame folderSel`. 두 모드에서 뜻이 다르다:
    /// 저장은 "이것은 저장 대상이 아니다", 열기는 "확정하면 들어간다".
    folder_sel: bool,
}

impl Variant {
    const fn open(state: FpState, remote: bool, multi: bool) -> Self {
        Self {
            state,
            remote,
            multi,
            mode: Mode::Open,
            deep: false,
            folder_sel: false,
        }
    }

    const fn save(save: SaveState, deep: bool) -> Self {
        Self {
            state: FpState::Loaded,
            remote: false,
            multi: false,
            mode: Mode::Save(save),
            deep,
            folder_sel: false,
        }
    }

    /// 목록에서 고른 것이 폴더인 갈래 — 두 모드 모두 단일 클릭이 선택이므로 둘 다 성립한다.
    const fn folder_selected(mut self) -> Self {
        self.folder_sel = true;
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
            TokenChip::new(
                "bg-sidebar",
                "path / list-header bar",
                theme.bg_sidebar().to_egui(),
            ),
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
        "Tasty's own \"Open file\" dialog — replaces the OS-native picker so it can browse a \
         remote attach host over the same SSH mechanism as attach (the OS picker only ever \
         sees the local disk). Local and remote are the same component; they differ only in \
         the header host indicator and the breadcrumb root, never the layout. Not the Explorer \
         surface (free-roam tab) — this is a select-and-confirm dialog. Not yet wired to the \
         host app: the remote directory-listing channel architecture is still undecided.",
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
            ("empty", "folderOpen glyph + muted line"),
            ("error", "danger glyph · title · reason · action"),
            ("perm vs. conn", "Retry vs. Reconnect (resumes)"),
            ("multi", "checkbox col · joined names · N selected"),
            ("Open", "disabled while loading / error / empty"),
        ],
        &[
            TokenChip::new(
                "accent-danger",
                "error glyph",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new(
                "text-placeholder",
                "empty glyph",
                theme.text_placeholder().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Row focus ring (keyboard nav) is shown on pipeline.yaml in the loaded frames above — \
         a 1px accent-primary outline, distinct from the filled selection background so focus \
         and selection never merge visually. Multi-select is spec'd for the future \
         (checkbox column, comma-joined name field, \"N selected\" count); single-select ships \
         by default. Favorites/recent locations are deferred, not designed this pass.",
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
        spec::cluster(ui, theme, "deep path — middle-elided breadcrumb", |ui| {
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
            ("breadcrumb", "root + … + last two segments"),
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
            TokenChip::new("separator", "footer rule", theme.separator.to_egui()),
        ],
    );

    spec::note(
        ui,
        theme,
        "Long-path shrink rule (fixes a defect in open mode too). Overflow is absorbed in the \
         path bar, never by the footer: the breadcrumb is the only flexible child \
         (flex:1; min-width:0) and elides in the middle — root + … + the last two segments, \
         since the current folder and its parent are what orient you; the … lists the hidden \
         ancestors on click. Footer label, filter chip and both buttons are flex:none; only the \
         name input shrinks. No horizontal scroll, and Cancel / Open / Save can never be clipped.",
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
                "tone",
                "the folder line has none — it is a fact, not a warning",
            ),
        ],
        &[
            TokenChip::new(
                "text-muted",
                "folder-target line",
                theme.text_muted().to_egui(),
            ),
            TokenChip::new(
                "text-secondary",
                "the button named in the open line",
                theme.text_secondary().to_egui(),
            ),
            TokenChip::new(
                "overlay-active",
                "selected row bed",
                theme.overlay_active().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Save and open are the same view, so every branch of the gesture table had to hold in \
         both. Single click selects and double click descends, for both kinds in both modes. A \
         folder is never a save target: the footer button still reads the name field only, so a \
         selected folder cannot change what gets written — and the muted line says so where the \
         reading happens, rather than disabling something and leaving the cause off-screen.",
    );

    spec::dont(
        ui,
        theme,
        "Don't confirm on a file's double-click in save mode. The double-click writes the name, \
         and that name is what raises the overwrite warning — confirming in the same gesture \
         would write over a file before its warning could be read.",
    );
}

// ════════════════════════════════════════════════════════════════════════
/// 640×480 카드 한 장.
fn card(ui: &mut egui::Ui, theme: &Theme, v: Variant) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .shadow(theme.shadow_modal().to_egui())
        .show(ui, |ui| {
            ui.set_width(FRAME_W.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                ui.set_width(FRAME_W.value());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                header(ui, theme, v);
                path_bar::path_bar(ui, theme, v);
                // footer 를 먼저 재고 남은 높이를 본문에 준다 — 덮어쓰기 경고 줄이 footer 를
                // 키우면 본문이 줄지 footer 가 밀려나지 않는다.
                let footer_h = footer::footer_height(ui, theme, v);
                body(ui, theme, v, FRAME_H - HEADER_H - PATH_H - footer_h);
                footer::footer(ui, theme, v, footer_h);
            });
        });
}

fn header(ui: &mut egui::Ui, theme: &Theme, v: Variant) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FRAME_W.value(), HEADER_H.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(theme.border_width.value(), theme.separator.to_egui()),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(
            rect.left() + HEADER_PAD_L.value(),
            rect.top() + theme.spacing_sm.value(),
        ),
        egui::pos2(
            rect.right() - theme.spacing_sm.value(),
            rect.bottom() - theme.spacing_sm.value(),
        ),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    // A 배지 채택: 헤더 글리프는 원격/로컬 공통 FILE (glyph 후보 B 의 remote 스왑은 미반영).
    kit::icon(
        &mut child,
        icons::FILE,
        theme.icon_glyph_size_md,
        theme.text_muted().to_egui(),
    );
    kit::title(
        &mut child,
        theme,
        match v.mode {
            Mode::Open => "Open file",
            Mode::Save(_) => "Save file",
        },
    );
    if v.remote {
        host_badge(&mut child, theme, HOST);
    }
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .show(ui, theme, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
            });
    });
}

/// 원격 host 배지(§6.1 A안, 채택) — mono `user@host` 칩, `accent-info` 축.
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
    // info 배지의 채움/테두리 짝 — `tint-fill-alpha` / `tint-border-alpha`.
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
    let pad = theme.spacing_md;
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

fn list_header(ui: &mut egui::Ui, theme: &Theme, multi: bool) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FRAME_W.value(), LIST_HEAD_H.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(theme.border_width.value(), theme.separator.to_egui()),
    );
    let c = cols(rect, theme, multi);
    let font = egui::FontId::monospace(theme.font_size_micro.value());
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

fn row(
    ui: &mut egui::Ui,
    theme: &Theme,
    r: &Row,
    multi: bool,
    checked: bool,
    selected: bool,
    focus: bool,
) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, ROW_H.value()), egui::Sense::hover());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, theme.surface_active().to_egui());
        let bar = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(theme.tab_indicator_width.value(), rect.height()),
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
        checkbox(&mut child, theme, &mut chk, "", true);
    }
    let (icon_glyph, icon_color): (MockGlyph, egui::Color32) = if r.folder {
        (icons::FOLDER, theme.accent_primary().to_egui())
    } else {
        (icons::FILE, theme.text_muted().to_egui())
    };
    let ir = egui::Rect::from_min_size(
        egui::pos2(c.icon_x.value(), rect.center().y - glyph_size * 0.5),
        egui::vec2(glyph_size, glyph_size),
    );
    icon_glyph.image(glyph_size, icon_color).paint_at(ui, ir);
    let name_color = if selected {
        theme.text_primary()
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
    let muted = theme.text_muted().to_egui();
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
            list_header(ui, theme, multi);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(FRAME_W.value(), (body_h - LIST_HEAD_H).value()),
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
                        Mode::Open => (
                            !multi && f.name == "README.md",
                            !multi && f.name == "pipeline.yaml",
                        ),
                    }
                };
                let checked = multi && MULTI_PICKED.contains(&f.name);
                row(&mut col, theme, f, multi, checked, selected, focus);
            }
        }
        FpState::Loading => center(
            ui,
            theme,
            body_h,
            icons::FOLDER,
            theme.text_placeholder().to_egui(),
            true,
            "Loading folder…",
            theme.text_secondary().to_egui(),
            Some("Reading the directory over SSH."),
            None,
        ),
        FpState::Empty => center(
            ui,
            theme,
            body_h,
            icons::FOLDER_OPEN,
            theme.text_placeholder().to_egui(),
            false,
            "This folder is empty",
            theme.text_muted().to_egui(),
            None,
            None,
        ),
        FpState::ErrorPerm => center(
            ui,
            theme,
            body_h,
            icons::ALERT_TRIANGLE,
            theme.accent_danger().to_egui(),
            false,
            "Permission denied",
            theme.text_primary().to_egui(),
            Some(
                "You don't have permission to read this folder. Try a different folder or check access.",
            ),
            Some("Retry"),
        ),
        FpState::ErrorConn => center(
            ui,
            theme,
            body_h,
            icons::ALERT_TRIANGLE,
            theme.accent_danger().to_egui(),
            false,
            "Remote connection lost",
            theme.text_primary().to_egui(),
            Some("The SSH tunnel dropped. Reconnect to resume browsing from the last folder."),
            Some("Reconnect"),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn center(
    ui: &mut egui::Ui,
    theme: &Theme,
    body_h: LogicalPx,
    glyph: MockGlyph,
    glyph_color: egui::Color32,
    spinner: bool,
    heading: &str,
    heading_color: egui::Color32,
    body_text: Option<&str>,
    action: Option<&str>,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FRAME_W.value(), body_h.value()),
        egui::Sense::hover(),
    );
    let mut col = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    col.add_space(((body_h - LogicalPx(EMPTY_BLOCK_H)).max(LogicalPx(0.0)) * 0.5).value());
    col.spacing_mut().item_spacing.y = theme.spacing_sm.value();
    if spinner {
        Spinner::new().size(EMPTY_GLYPH).show(&mut col, theme);
    } else {
        kit::icon(&mut col, glyph, LogicalPx(EMPTY_GLYPH), glyph_color);
    }
    col.label(
        egui::RichText::new(heading)
            .size(theme.font_size_body.value())
            .strong()
            .color(heading_color),
    );
    if let Some(b) = body_text {
        col.set_max_width(EMPTY_BODY_MAX_W.value());
        col.label(
            egui::RichText::new(b)
                .size(theme.font_size_caption.value())
                .color(theme.text_muted().to_egui()),
        );
    }
    if let Some(label) = action {
        col.add_space(theme.spacing_xs.value());
        Button::new(label)
            .variant(ButtonVariant::Secondary)
            .leading_icon(&|ui, rect, c| icons::REFRESH.image(rect.height(), c).paint_at(ui, rect))
            .show(&mut col, theme);
    }
}
