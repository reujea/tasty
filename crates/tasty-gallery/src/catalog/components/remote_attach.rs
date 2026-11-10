//! Attach 프로필과 원격 워크스페이스를 선택하는 정적 예제.
//! 원격 목록이 비어도 새 워크스페이스 행은 남기며 푸터에서 선택을 확인한다.

mod new_row;
mod panes;
mod rows;

pub use new_row::draw_new_row;

use panes::{left_pane, right_pane};
use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::{Button, ButtonVariant, IconButton, IconButtonVariant};

use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};
use crate::catalog::widgets::dialog as kit;

// ── 프레임 고정 치수 (디자인 raw px — 화면 전용 고정값) ──
const FRAME_W: LogicalPx = LogicalPx(680.0);
const FRAME_H: LogicalPx = LogicalPx(460.0);
const LEFT_W: LogicalPx = LogicalPx(240.0);
const HEADER_H: LogicalPx = LogicalPx(47.0); // padding 10/10 + content 27
const HEADER_PAD_L: LogicalPx = LogicalPx(14.0); // 디자인 L14 (size-14)
const FOOTER_H: LogicalPx = LogicalPx(49.0);
const BODY_H: LogicalPx = FRAME_H.minus(HEADER_H).minus(FOOTER_H);
const PROFILE_ROW_H: LogicalPx = LogicalPx(50.0); // name(2 lines) + padding sm
const WS_ROW_H: LogicalPx = LogicalPx(34.0);
const STRIP_W: LogicalPx = LogicalPx(440.0); // 새 행 상태 specimen 의 pane 폭

/// 두 열의 caps 헤더("ATTACH PROFILES" / "REMOTE WORKSPACES") 행 높이 —
/// padding 12/12 + micro caps 한 줄. 4px 그리드 밖이고 대응 Theme 토큰이 없다.
const CAPS_HEADER_H: LogicalPx = LogicalPx(30.0);

/// 우측 pane 상태. `Loaded` / `Empty` 는 같은 목록 렌더 경로를 타고 ws 목록의
/// 길이만 다르다. `EmptyPlanA` 는 채택하지 않은 비교안(가운데 상태 + 생성 버튼)이다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RaState {
    Initial,
    Connecting,
    Error,
    Loaded,
    Empty,
    EmptyPlanA,
}

impl RaState {
    fn empty(self) -> bool {
        matches!(self, RaState::Empty | RaState::EmptyPlanA)
    }
}

/// "+ New workspace" 행의 시각 상태 — 디자인 `RaNewWsRow` 의 `phase`(rest/creating/
/// failed)에 포인터/선택 상태를 합친 것. 이 행은 버튼이 아니라 **목록 행**이라
/// 이웃 ws 행과 같은 select-then-confirm 계약을 따른다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum NewRow {
    Rest,
    Hover,
    Selected,
    Creating,
    Failed,
}

impl NewRow {
    fn creating(self) -> bool {
        self == NewRow::Creating
    }
    fn failed(self) -> bool {
        self == NewRow::Failed
    }
    fn selected(self) -> bool {
        self == NewRow::Selected
    }
}

/// 생성 실패 시 행 하단에 인라인으로 붙는 원격 메시지(specimen seed).
const CREATE_ERROR: &str = "The remote refused workspace.create — the instance is read-only.";

struct Prof {
    name: &'static str,
    label: &'static str,
    target: &'static str,
    inactive: bool,
}

const PROFILES: &[Prof] = &[
    Prof {
        name: "prod-web",
        label: "us-east",
        target: "deploy@10.0.4.12",
        inactive: false,
    },
    Prof {
        name: "gb10",
        label: "",
        target: "→ prod-web",
        inactive: false,
    },
    Prof {
        name: "edge-direct",
        label: "",
        target: "root@edge.example.com",
        inactive: false,
    },
    Prof {
        name: "media-nas",
        label: "lab",
        target: "→ nas.local",
        inactive: false,
    },
    Prof {
        name: "legacy-attach",
        label: "",
        target: "→ legacy-box",
        inactive: true,
    },
];

struct Ws {
    name: &'static str,
    panes: u32,
    busy: bool,
    attached: bool,
    /// 원격 쪽에서도 다른 기계의 mirror인 workspace. 보이지만 고를 수 없다(R-58 체인).
    chain: bool,
}

const WORKSPACES: &[Ws] = &[
    Ws {
        name: "agents-prod",
        panes: 3,
        busy: true,
        attached: false,
        chain: false,
    },
    Ws {
        name: "api-gateway",
        panes: 2,
        busy: false,
        attached: true,
        chain: false,
    },
    Ws {
        name: "scratch",
        panes: 1,
        busy: false,
        attached: false,
        chain: false,
    },
    Ws {
        name: "build-farm",
        panes: 2,
        busy: false,
        attached: false,
        chain: true,
    },
];

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        ra_card(ui, theme, RaState::Loaded);
    });

    spec::meta(
        ui,
        theme,
        &[
            ("frame", "680×460 · bg-panel · headless header"),
            ("shadow", "shadow-modal — occupies the viewport (centered)"),
            ("left", "240px bg-sidebar · attach profiles (single select)"),
            ("right", "flex · 4 states off left selection"),
            (
                "new row",
                "first row · plus glyph in dot slot · accent label · 'on remote'",
            ),
            (
                "ws row",
                "StatusDot · name · panes · busy / in-use / mirror",
            ),
            (
                "mirror (chain)",
                "a remote workspace that is itself a mirror of another machine: shown, not \
                 selectable, name text-muted, idle dot, neutral Tag remote glyph + mirror, \
                 tooltip remote.row.reason.chain",
            ),
            ("selected", "surface-active + 2px accent left bar"),
            ("footer", "Connect (primary, conditional) · Cancel (ghost)"),
        ],
        &[
            TokenChip::new("bg-sidebar", "left pane", theme.bg_sidebar().to_egui()),
            TokenChip::new(
                "surface-active",
                "selected row",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "new-row glyph+label / select bar / Connect",
                theme.accent_primary().to_egui(),
            ),
            TokenChip::new(
                "separator",
                "1px rule closing the new-row group",
                theme.separator.to_egui_premultiplied(),
            ),
            TokenChip::new(
                "tag-attached-fg",
                "in-use Tag",
                theme.tag_attached_fg().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "The picker consumes the tasty-attach profiles edited in remote_tool — one store, \
         listed here. A remote workspace already attached elsewhere shows an attached \
         'in use' Tag and can't be selected (prevents a double-mirror.)",
    );

    spec::note(
        ui,
        theme,
        "'+ New workspace' is a list row, not a button: it flows through the same \
         single-select state as every remote workspace, so confirming it goes through the \
         footer like any other choice. The footer button then reads 'Create & connect' — \
         it has to say which of the two things it will do. It creates the workspace with \
         the remote's own default name and cwd; nothing is asked for.",
    );
}

pub fn draw_states(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        for (caption, state) in [
            ("initial — nothing picked", RaState::Initial),
            ("connecting", RaState::Connecting),
            ("error — retry", RaState::Error),
            (
                "empty — reachable, no workspaces (list path)",
                RaState::Empty,
            ),
        ] {
            spec::cluster(ui, theme, caption, |ui| ra_card(ui, theme, state));
        }
    });

    spec::meta(
        ui,
        theme,
        &[
            ("initial", "remote glyph + 'Select an attach profile'"),
            (
                "connecting",
                "Spinner + 'Connecting…' + SSH note; footer ghost becomes Stop",
            ),
            ("error", "danger warn glyph + reason + Retry"),
            (
                "empty",
                "caps header + pre-selected '+ New workspace' row + one muted line · Create & connect enabled",
            ),
            ("center", "flex-centered, gap sm, padding xl/lg"),
        ],
        &[
            TokenChip::new(
                "accent-danger",
                "error glyph",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::without_color("spinner-track", "connecting spinner"),
            TokenChip::new(
                "text-placeholder",
                "initial glyph",
                theme.text_placeholder().to_egui(),
            ),
            TokenChip::new("text-muted", "prompt copy", theme.text_muted().to_egui()),
        ],
    );

    spec::do_(
        ui,
        theme,
        "Model all four states explicitly — the connect (SSH tunnel + list) can take \
         seconds; never leave the right pane blank while it resolves.",
    );

    spec::note(
        ui,
        theme,
        "The host checks the connection wait on each frame. With no result after 20 seconds, it requests cancellation and shows an error with Retry. Stop requests cancellation and returns to the initial state without closing the picker.",
    );

    spec::dont(
        ui,
        theme,
        "Don't give the empty remote its own centered state with a create button. That \
         would make the same action confirm two different ways — a button that fires on \
         click when the remote is empty, a row that waits for the footer when it isn't — \
         off a condition the user can't see coming. Empty is the same list with one row in \
         it, and that row starts selected so the footer is live the moment the pane paints.",
    );
}

/// 빈 원격의 두 안 — 시안 "Empty remote — plan A (center-state + CTA) vs plan B (list path)".
pub fn draw_empty_plans(ui: &mut egui::Ui, theme: &Theme) {
    spec::stage(ui, theme, StageVariant::Wrap, |ui| {
        plan_cluster(
            ui,
            theme,
            "plan A — center-state + CTA",
            theme.text_muted().to_egui(),
            RaState::EmptyPlanA,
        );
        plan_cluster(
            ui,
            theme,
            "plan B — list path (recommended)",
            theme.accent_primary().to_egui(),
            RaState::Empty,
        );
    });

    spec::meta(
        ui,
        theme,
        &[
            ("recommendation", "plan B"),
            ("why", "one render path, one affordance, one confirm route"),
            (
                "plan B copy",
                "\u{201c}‹host› is reachable but has no workspaces yet.\u{201d}",
            ),
            (
                "footer",
                "Create & connect, enabled (the row is pre-selected)",
            ),
            (
                "plan A cost",
                "a second way to trigger the same action, confirmed differently",
            ),
        ],
        &[
            TokenChip::new(
                "text-muted",
                "explanatory line",
                theme.text_muted().to_egui(),
            ),
            TokenChip::new(
                "accent-primary",
                "row label",
                theme.accent_primary().to_egui(),
            ),
        ],
    );

    spec::note(
        ui,
        theme,
        "Recommended: plan B. Plan A's CTA is a button, so it fires immediately — the same \
         action would then be confirmed two different ways depending on whether the remote \
         happened to be empty, and the implementation would carry two render branches plus \
         two handlers. Plan B has one list, one row, one confirm route, and the empty case \
         degrades to \u{201c}the list has exactly one row.\u{201d} In plan B the row is \
         pre-selected on an empty remote, so Create & connect is live the moment the pane \
         loads — the dead end is gone without adding a control.",
    );
}

/// 열린 질문 여덟 개의 결론 — 시안 "Decisions — the eight open questions, resolved".
/// 예제 그림 없이 Note 세 개로 이뤄진 구현 명세다.
pub fn draw_decisions(ui: &mut egui::Ui, theme: &Theme) {
    spec::note(
        ui,
        theme,
        "1 · empty state — plan B (list path, one pre-selected row). One render branch, one \
         confirm route.\n\
         2 · distinction — plus glyph in the dot slot + accent label + 1px separator below \
         (4/4 margins). Weight stays 500, size stays 13 — the row must read as a peer of the \
         rows below it, not as a header.\n\
         3 · creating — inline in the row (glyph → Spinner, label → \u{201c}Creating \
         workspace…\u{201d}), list below stays readable, names in text-muted, and inert. A full-pane connecting \
         takeover would throw away the list for a 1–3s roundtrip.\n\
         4 · failure — inline under the row, message clamped to 3 lines (full string in \
         title) + Try again. The connect-error center-state is right for \u{201c}we never got \
         a list\u{201d}; here we have the list and the user's next move is usually to pick an \
         existing workspace instead — don't hide it.\n\
         5 · confirm (core) — select, then footer. The user chose a placement inside a list; \
         a single row that fires on click while its neighbours only select is the \
         inconsistency, and it also loses the reversible \u{201c}I clicked it, now what?\u{201d} \
         moment before a remote-mutating action. Cost accepted: the row carries a selected \
         state.\n\
         6 · height & sticky — 34px, not sticky. Same box as a ws row; sticky would stack a \
         second frozen band under the caps header for a list that rarely exceeds ~8 rows.\n\
         7 · footer label — \u{201c}Create & connect\u{201d} while the new row is selected, \
         \u{201c}Connect\u{201d} otherwise. Because §6-5 chose footer confirmation, the button \
         must say which of the two things it will do.\n\
         8 · tooltip — yes, on the row: \u{201c}Creates a workspace on the remote with its \
         default name and cwd — you won't be asked for a name — then mirrors it here.\u{201d} \
         Not asking for a name is the surprising part, so it gets said where the click \
         happens.",
    );

    spec::note(
        ui,
        theme,
        "Interaction contract. Arrow keys traverse the new row as row 1; Enter confirms the \
         selection (same as the footer). Changing the left-hand profile resets the selection \
         and the row's phase to rest. During creation the left profile list and the right \
         list are inert; Cancel stays live — it closes the popup and abandons the in-flight \
         request, and a workspace already created on the remote is not rolled back (a flash \
         message on close says so; no extra confirm). On success the popup closes straight \
         into attach — no interstitial \u{201c}created\u{201d} step. The caps header keeps its \
         wording: REMOTE WORKSPACES · ‹profile› still describes the group, and the new row's \
         own label says it is a creation.",
    );

    spec::note(
        ui,
        theme,
        "New icons: none. plus, alertTriangle and refresh already exist in icons/. New \
         tokens: none — the row is built from accent-primary, overlay-hover, \
         surface-active, separator, accent-danger and the existing \
         spacing steps.",
    );
}

/// 안 이름 캡션과 카드 한 장. 권장안 캡션만 accent 로 칠한다(시안 캡션 색).
fn plan_cluster(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    color: egui::Color32,
    state: RaState,
) {
    spec::wrap_item(ui, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        ui.label(
            egui::RichText::new(label.to_uppercase())
                .size(theme.font_size_micro.value())
                .color(color),
        );
        ra_card(ui, theme, state);
    });
}

/// 680×460 카드 한 장.
fn ra_card(ui: &mut egui::Ui, theme: &Theme, state: RaState) {
    egui::Frame::new()
        .fill(theme.bg_panel().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_strong().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        // 본체와 같은 중앙 팝업이므로 modal 그림자를 적용한다.
        .shadow(theme.shadow_modal().to_egui())
        .show(ui, |ui| {
            ui.set_width(FRAME_W.value());
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.vertical(|ui| {
                ui.set_width(FRAME_W.value());
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                header(ui, theme);
                body(ui, theme, state);
                footer(ui, theme, state);
            });
        });
}

fn header(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FRAME_W.value(), HEADER_H.value()),
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
    // 디자인 padding 위 10 · 오른쪽 10 · 아래 10 · 왼쪽 14.
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
    kit::icon(
        &mut child,
        icons::REMOTE,
        theme.icon_glyph_size_md,
        theme.text_muted().to_egui(),
    );
    kit::title(&mut child, theme, crate::i18n::t("remote_attach.heading"));
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        IconButton::new()
            .variant(IconButtonVariant::Ghost)
            .show(ui, theme, &|ui, rect, c| {
                icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
            });
    });
}

fn body(ui: &mut egui::Ui, theme: &Theme, state: RaState) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FRAME_W.value(), BODY_H.value()),
        egui::Sense::hover(),
    );
    let left = egui::Rect::from_min_size(rect.min, egui::vec2(LEFT_W.value(), BODY_H.value()));
    let right = egui::Rect::from_min_max(
        egui::pos2(rect.left() + LEFT_W.value(), rect.top()),
        rect.max,
    );
    ui.painter()
        .rect_filled(left, 0.0, theme.bg_sidebar().to_egui());
    ui.painter().vline(
        left.right(),
        left.y_range(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    left_pane(ui, theme, left, state);
    right_pane(ui, theme, right, state);
}

fn footer(ui: &mut egui::Ui, theme: &Theme, state: RaState) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(FRAME_W.value(), FOOTER_H.value()),
        egui::Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + theme.spacing_lg.value(), rect.top()),
        egui::pos2(rect.right() - theme.spacing_lg.value(), rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = theme.spacing_sm.value();
    // 새 워크스페이스 행을 고르면 확인 버튼이 생성과 연결을 함께 알린다.
    Button::new(if state.empty() {
        "Create & connect"
    } else {
        "Connect"
    })
    .variant(ButtonVariant::Primary)
    .enabled(state == RaState::Loaded || state.empty())
    .show(&mut child, theme);
    // 연결 중에는 팝업 닫기 대신 조회 중단을 표시한다.
    Button::new(if state == RaState::Connecting {
        "Stop"
    } else {
        "Cancel"
    })
    .variant(ButtonVariant::Ghost)
    .show(&mut child, theme);
}
