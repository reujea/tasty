//! 본체와 같은 단일 카드 그리기 함수로 토스트 종류와 쌓이는 순서를 비교한다.
//! 치수·색 계산을 공유하며 수명·페이드·중복 합치기는 실행하지 않는다.

use tasty_type_appearance::theme::Theme;

use crate::catalog::spec::{StageVariant, TokenChip, dont, meta, note, stage};
use crate::catalog::toast_card::{self, ToastKind};

struct ToastCardProps {
    kind: ToastKind,
    message: &'static str,
    hint: Option<HintKeys>,
}

/// 예제 hint의 키. 시안의 Kbd 규칙대로 macOS는 기호, 그 밖은 Ctrl+Shift+ 표기다.
#[derive(Clone, Copy)]
enum HintKeys {
    /// 시안 `⌘⇧C`.
    CopyPath,
    /// 시안 `⌘C`.
    Copy,
}

impl HintKeys {
    fn keys(self) -> Vec<String> {
        let keys: &[&str] = match (self, cfg!(target_os = "macos")) {
            (HintKeys::CopyPath, true) => &["⌘⇧C"],
            (HintKeys::CopyPath, false) => &["Ctrl", "Shift", "C"],
            (HintKeys::Copy, true) => &["⌘C"],
            (HintKeys::Copy, false) => &["Ctrl", "C"],
        };
        keys.iter().map(|k| (*k).to_owned()).collect()
    }
}

fn draw_toast_card(ui: &mut egui::Ui, theme: &Theme, props: &ToastCardProps, alpha: f32) {
    let hint = props.hint.map(HintKeys::keys).unwrap_or_default();
    toast_card::draw_single_card(ui, theme, props.kind, props.message, &hint, alpha);
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    // 시안 Toast Spec의 카드 여섯 장. 같은 문구로 hint 유무를 나란히 비교한다.
    let cards = [
        ToastCardProps {
            kind: ToastKind::Success,
            message: "Path copied",
            hint: Some(HintKeys::CopyPath),
        },
        ToastCardProps {
            kind: ToastKind::Success,
            message: "Path copied",
            hint: None,
        },
        ToastCardProps {
            kind: ToastKind::Success,
            message: "Copied 3 lines from the selection to the clipboard",
            hint: Some(HintKeys::Copy),
        },
        ToastCardProps {
            kind: ToastKind::Info,
            message: "Copied (OSC 52)",
            hint: None,
        },
        ToastCardProps {
            kind: ToastKind::Warning,
            message: "Held by another client (readonly)",
            hint: None,
        },
        ToastCardProps {
            kind: ToastKind::Error,
            message: "Force detach — connection dropped",
            hint: None,
        },
    ];

    stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for card in &cards {
            draw_toast_card(ui, theme, card, 1.0);
        }
        // 값이 든 Warning 카드. 값은 본문과 같은 글꼴이고 끊을 곳이 없으면 카드 안에서 줄을 바꾼다.
        let font_rejected = crate::i18n::t_fmt("toast.font_rejected", "~/fonts/notes.txt");
        toast_card::draw_single_card(ui, theme, ToastKind::Warning, &font_rejected, &[], 1.0);
    });

    note(
        ui,
        theme,
        "A long unbroken value such as a path wraps inside the card. The font rejected toast is on Overlays › Windows, under Settings.",
    );

    meta(
        ui,
        theme,
        &[
            ("rail", "toast-accent-width (2) left"),
            ("radius", "4"),
            ("fill", "surface-raised"),
            ("max-width", "toast-max-width"),
            (
                "boxes",
                "rail · body (flex 1) · hint (flex none) · gap space-sm · pad space-sm / space-md",
            ),
            (
                "hint",
                "Kbd of the action's binding · menu / mouse origin only · none when unbound",
            ),
            (
                "hint position",
                "right end, first line · never truncated — body wraps first",
            ),
            (
                "platform",
                // 갤러리 UI 글꼴에 ⌥가 없어 기호 이름으로 적는다.
                "Kbd rule: Cmd / Shift / Option symbols on macOS, Ctrl+Shift+ on Windows / Linux",
            ),
            ("agent", "catalog only — host does not emit"),
            ("icon", "catalog only — host card has no icon"),
        ],
        &[
            TokenChip::new(
                "accent-success",
                "ok rail",
                egui::Color32::from(theme.accent_success()),
            ),
            TokenChip::without_color("toast-hint-font-size", "hint · micro mono"),
            TokenChip::new(
                "surface-raised",
                "card fill",
                egui::Color32::from(theme.surface_raised()),
            ),
        ],
    );
}

/// 가장 새 토스트가 아래에 오도록 배치한다. 본체는 스코프당 다섯 개를 넘으면 가장 오래된 것을 지운다.
/// 나이에 따라 색이 흐려지지 않으므로 모든 예제의 불투명도는 1로 둔다.
pub fn draw_stack(ui: &mut egui::Ui, theme: &Theme) {
    // 위가 가장 오래된 것, 아래가 가장 새 것이다.
    let stack = [
        ToastCardProps {
            kind: ToastKind::Info,
            message: "Two notices while importing the bundle",
            hint: None,
        },
        ToastCardProps {
            kind: ToastKind::Warning,
            message: "Held by another client (readonly)",
            hint: None,
        },
        ToastCardProps {
            kind: ToastKind::Info,
            message: "Settings applied",
            hint: None,
        },
        ToastCardProps {
            kind: ToastKind::Success,
            message: "Path copied to clipboard",
            hint: Some(HintKeys::Copy),
        },
        ToastCardProps {
            kind: ToastKind::Error,
            message: "Force detach — connection dropped",
            hint: None,
        },
    ];

    stage(ui, theme, StageVariant::Solo, |ui| {
        egui::Frame::new()
            .fill(egui::Color32::from(theme.bg_app()))
            .inner_margin(egui::Margin::same(theme.spacing_xl.value() as i8))
            .show(ui, |ui| {
                ui.set_width(theme.measure_lg.value());
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    // 시안: 폭 toast-max-width 열, 카드는 자기 내용 폭으로 오른쪽 끝을 맞춘다.
                    ui.allocate_ui_with_layout(
                        egui::vec2(theme.toast_max_width.value(), 0.0),
                        egui::Layout::top_down(egui::Align::Max),
                        |ui| {
                            ui.set_width(theme.toast_max_width.value());
                            ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                            for card in &stack {
                                draw_toast_card(ui, theme, card, 1.0);
                            }
                        },
                    );
                });
            });
    });

    note(
        ui,
        theme,
        "Anchored bottom-right and stacked bottom-up: the newest card is at the bottom, \
         space-sm gap. At most 5 per scope; a 6th drops the oldest (topmost) immediately. \
         Cards past the scope's top edge are not drawn, and every resting card is fully \
         opaque — alpha is only for enter and exit. Each card keeps its own content width, \
         capped at toast-max-width, and right edges align to the anchor; there is no shared \
         stack width, so nothing re-flows when a card enters or leaves.",
    );

    meta(
        ui,
        theme,
        &[
            ("anchor", "bottom-right"),
            ("order", "newest bottom"),
            ("gap", "space-sm 8"),
            ("cap", "5 per scope → oldest dropped"),
            (
                "width",
                "content width per card, cap toast-max-width (320) · right edges align",
            ),
            (
                "window-scope offset (b2 · 2026-10-07)",
                "main window: toast-stack-offset-bottom (→ size-36 = status bar 24 + 12), OFF-SCALE like the status bar and the card dims · Settings window: toast-stack-offset-bottom-settings (→ size-64 = footer 52 + 12), ON-SCALE like the footer — the stack sits above Cancel / Save",
            ),
            (
                "scale",
                "every structural toast dim (scope margin 12, padding 12 × 8, gap 8, main offset) is OFF-SCALE; only the Settings offset follows the zoom",
            ),
            (
                "shared corner (2026-10-07)",
                "workspace- and pane-scope stacks that land on the same corner merge into ONE column, newest bottom, cap 5 across both",
            ),
            (
                "link menu Copy hint (b2)",
                "own action copy_link ('Copy link'), unbound by default → no hint until the user binds it; never borrow the copy binding",
            ),
            (
                "over a native WebView (b2)",
                "while a toast card's rect intersects a WebView, only THAT WebView is hidden for the card's life (its tile shows plain); others stay; keyboard focus is not reclaimed (a toast never takes focus). A WebView that is RECEIVING KEYS is not hidden, so the card stays under it — accepted (2026-10-07): typing must not be lost, and a toast is transient",
            ),
        ],
        &[
            TokenChip::without_color("space-sm", "stack gap"),
            TokenChip::new(
                "surface-raised",
                "card fill",
                egui::Color32::from(theme.surface_raised()),
            ),
        ],
    );

    dont(
        ui,
        theme,
        "Don't let the stack grow unbounded, and don't fold the tail into a \"+N more\" row \
         either. Hold the cap by dropping the oldest card.",
    );
}

/// 시안 `toast-display-backend`: 네이티브 Wayland에서 다른 앱과 복사·붙여넣기를 못 할 때의 warning 카드.
/// 본체와 같은 키를 네 언어로 그린다. 카드는 hint·아이콘·버튼 없이 기존 카드 그대로다.
pub fn draw_display_backend(ui: &mut egui::Ui, theme: &Theme) {
    const KEYS: [&str; 6] = [
        "toast.clipboard_wayland_unsupported",
        "toast.clipboard_wayland_x_failed",
        "toast.clipboard_files_wayland_copy",
        "toast.clipboard_files_wayland_paste",
        "toast.clipboard_files_wayland_x_failed_copy",
        "toast.clipboard_files_wayland_x_failed_paste",
    ];
    stage(ui, theme, StageVariant::Column, |ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
        for key in [KEYS[0], KEYS[2], KEYS[3]] {
            toast_card::draw_single_card(
                ui,
                theme,
                ToastKind::Warning,
                crate::i18n::t_lang("en", key),
                &[],
                1.0,
            );
        }
    });
    stage(ui, theme, StageVariant::Wrap, |ui| {
        for key in KEYS {
            let name = format!("toast-db-{}", key["toast.".len()..].replace('_', "-"));
            crate::catalog::spec::cluster(ui, theme, &name, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing_sm.value();
                    for lang in ["en", "ko", "ja", "zh-Hans"] {
                        toast_card::draw_single_card(
                            ui,
                            theme,
                            ToastKind::Warning,
                            crate::i18n::t_lang(lang, key),
                            &[],
                            1.0,
                        );
                    }
                });
            });
        }
    });
    meta(
        ui,
        theme,
        &[
            ("variant", "warning (rail accent-warning)"),
            (
                "variant change",
                "the clipboard toasts on native Wayland are warning; the X11 failure toasts toast.clipboard_files_internal_copy / _paste stay info",
            ),
            (
                "card",
                "the Toast card as it is · no icon, no hint, no button, no link",
            ),
            (
                "body",
                "what is off · cause · fix · one string per key, at most 200 characters in every language",
            ),
            ("life", "the global toast setting, like every other toast"),
        ],
        &[
            TokenChip::new("accent-warning", "rail", theme.accent_warning().to_egui()),
            TokenChip::new("surface-raised", "card", theme.surface_raised().to_egui()),
            TokenChip::without_color("toast-max-width", "card width cap (320)"),
        ],
    );
    note(
        ui,
        theme,
        "These notices are long for a toast, but their life is the global toast setting like every other toast; none of them has a life of its own. Menus raise none: where the native menu cannot open, Tasty opens its fallback menu.",
    );
}
