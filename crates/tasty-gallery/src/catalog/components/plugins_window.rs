//! 플러그인 관리 창의 Installed·Attention·Add plugin 예제.
//! 본체 뷰는 Context에 직접 패널을 붙이므로 여기서는 같은 구성을 주어진 영역에 그린다.

mod add;
mod attention;
mod installed;

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;
use tasty_ui_widgets::tokens::STRUCT_GAP_2;
use tasty_ui_widgets::{ControlSize, paint_dashed_outline};

use crate::catalog::icons::{CLOSE, PLUG};
use crate::catalog::spec::{self, StageVariant, TokenChip};

/// 본체 SEGMENT_TAB_LABEL_PRIMITIVE_12와 같은 12px 글꼴. 대응 semantic 토큰이 없다.
const SEGMENT_TAB_LABEL_PRIMITIVE_12: LogicalPx = LogicalPx(12.0);

/// 확인한 매니페스트를 보이는 Add 예제 창의 높이. 예제 무대 전용 값이다.
const ADD_VERIFIED_STAGE_H: LogicalPx = LogicalPx(760.0);

/// 세그먼트 탭 셋 — 본체 `PluginsUiState.tab`. 세 탭은 서로 다른 본문을 그린다.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    /// Installed 는 상세가 세 갈래다 — 무선택 / 선택 / uninstall 확인.
    Installed { detail: installed::Detail },
    /// Attention 은 대상이 0 이면 본문이 통째로 안내로 바뀌고 세그먼트 배지도 사라진다.
    /// `selected` 는 상세에 펼칠 `attention::ENTRIES` 의 위치다.
    Attention { empty: bool, selected: usize },
    /// `Add plugin` 은 상태가 둘이라 어느 쪽을 그릴지 함께 든다.
    Add { preview: bool },
}

/// 본체 Installed·Attention 목록과 같은 너비 접근자를 사용한다.
fn list_w(theme: &Theme) -> f32 {
    theme.plugins_side_panel_width().value()
}

/// 예제 창의 크기. 상세 정보를 한눈에 비교하도록 Installed 화면은 더 높게 잡는다.
fn stage_size(theme: &Theme, tab: Tab) -> egui::Vec2 {
    let h = match tab {
        // Attention 액션 바는 Installed 처럼 열 바닥에 붙으므로 사유 detail 이 바 위에 다 들 높이가 필요하다.
        Tab::Installed { .. } | Tab::Attention { empty: false, .. } => theme.measure_xl,
        // 경로 선택 블록 아래 매니페스트 카드와 신뢰 상자, fingerprint 줄까지 담아야 액션 바가
        // 무대 안에 든다.
        Tab::Add { preview: true } => ADD_VERIFIED_STAGE_H.scaled(theme.ui_zoom),
        _ => theme.measure_sm,
    };
    egui::vec2(list_w(theme) + theme.measure_md.value(), h.value())
}

/// 한 세그먼트 탭의 표시 상태 — 본체 `segment_tab` 의 뒤쪽 인자 4개를 묶은 것.
struct Segment<'a> {
    label: &'a str,
    count: Option<usize>,
    danger: bool,
    selected: bool,
}

/// 본체 `segment_tab` 전사 — 라벨 + (count 또는 danger 배지), selected 면 채운 배경.
fn segment_tab(
    ui: &mut egui::Ui,
    theme: &Theme,
    p: &egui::Painter,
    at: egui::Pos2,
    seg: &Segment<'_>,
) -> f32 {
    let Segment {
        label,
        count,
        danger,
        selected,
    } = *seg;
    let label_color = if selected {
        theme.text_on_active().to_egui()
    } else {
        theme.text_muted().to_egui()
    };
    let count_color = if selected {
        theme.text_on_active().to_egui()
    } else {
        theme.text_muted().to_egui()
    };
    let label_font = egui::FontId::proportional(SEGMENT_TAB_LABEL_PRIMITIVE_12.value());
    let label_galley = p.layout_no_wrap(label.to_string(), label_font, label_color);

    let badge = danger && count.is_some_and(|c| c > 0);
    let badge_galley = badge.then(|| {
        p.layout_no_wrap(
            count.unwrap_or(0).to_string(),
            egui::FontId::proportional(theme.badge_font_size().value()),
            theme.text_on_accent().to_egui(),
        )
    });
    let count_galley = (!danger)
        .then(|| {
            count.map(|c| {
                p.layout_no_wrap(
                    c.to_string(),
                    egui::FontId::monospace(theme.font_size_micro.value()),
                    count_color,
                )
            })
        })
        .flatten();

    let pad_x = theme.spacing_md.value();
    let gap = theme.spacing_sm.value();
    let height = theme.item_height_tab.value() + STRUCT_GAP_2.value();
    let badge_h = theme.spacing_lg.value();
    let badge_pad = theme.spacing_xs.value();

    let mut width = label_galley.size().x + pad_x * 2.0;
    if let Some(g) = &count_galley {
        width += gap + g.size().x;
    }
    if let Some(g) = &badge_galley {
        width += gap + (g.size().x + badge_pad * 2.0).max(badge_h);
    }

    let rect = egui::Rect::from_min_size(at, egui::vec2(width, height));
    if selected {
        p.rect(
            rect,
            theme.corner_radius.value(),
            theme.surface_active().to_egui(),
            egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
            egui::StrokeKind::Inside,
        );
    }

    let mut x = rect.min.x + pad_x;
    p.galley(
        egui::pos2(x, rect.center().y - label_galley.size().y * 0.5),
        label_galley.clone(),
        label_color,
    );
    x += label_galley.size().x;
    if let Some(g) = count_galley {
        x += gap;
        p.galley(
            egui::pos2(x, rect.center().y - g.size().y * 0.5),
            g,
            count_color,
        );
    }
    if let Some(g) = badge_galley {
        x += gap;
        let bw = (g.size().x + badge_pad * 2.0).max(badge_h);
        let brect = egui::Rect::from_min_size(
            egui::pos2(x, rect.center().y - badge_h * 0.5),
            egui::vec2(bw, badge_h),
        );
        p.rect_filled(
            brect,
            theme.corner_radius.value(),
            theme.accent_danger().to_egui(),
        );
        p.galley(
            egui::pos2(
                brect.center().x - g.size().x * 0.5,
                brect.center().y - g.size().y * 0.5,
            ),
            g,
            theme.text_on_accent().to_egui(),
        );
    }
    // 선택되지 않은 탭도 같은 폭을 차지한다 (본체 allocate_exact_size 와 동일).
    let _ = ui;
    width
}

/// 헤더 밴드 (높이 48) — 본체 `TopBottomPanel::top("plugins_header")`.
/// 선택 세그먼트와 필터 입력의 유무가 `tab` 에 달려 있다.
fn header(ui: &mut egui::Ui, theme: &Theme, rect: egui::Rect, tab: Tab) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme.bg_sidebar().to_egui());
    p.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        egui::Stroke::new(
            theme.border_width.value(),
            theme.separator.to_egui_premultiplied(),
        ),
    );

    let cy = rect.center().y;
    let mut x = rect.min.x + theme.spacing_md.value();

    // 헤더 아이콘은 주의 환기가 아닌 장식용 색을 쓴다.
    let icon = theme.icon_glyph_size_md.value();
    PLUG.image(icon, theme.plugins_header_glyph().to_egui())
        .paint_at(
            ui,
            egui::Rect::from_center_size(egui::pos2(x + icon * 0.5, cy), egui::vec2(icon, icon)),
        );
    x += icon + theme.spacing_xs.value();

    let title_font = egui::FontId::proportional(theme.font_size_max.value());
    let title = p.layout_no_wrap(
        "Plugins".to_string(),
        title_font,
        theme.text_primary().to_egui(),
    );
    p.galley(
        egui::pos2(x, cy - title.size().y * 0.5),
        title.clone(),
        theme.text_primary().to_egui(),
    );
    x += title.size().x + theme.spacing_sm.value();

    let div_h = theme.spacing_lg.value() + theme.spacing_xs.value();
    p.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(x, cy - div_h * 0.5),
            egui::vec2(theme.border_width.value(), div_h),
        ),
        0.0,
        theme.separator.to_egui_premultiplied(),
    );
    x += theme.border_width.value() + theme.spacing_sm.value();

    let tab_y = cy - (theme.item_height_tab.value() + STRUCT_GAP_2.value()) * 0.5;
    x += segment_tab(
        ui,
        theme,
        &p,
        egui::pos2(x, tab_y),
        &Segment {
            label: "Installed",
            count: Some(installed::ROWS.len()),
            danger: false,
            selected: matches!(tab, Tab::Installed { .. }),
        },
    ) + STRUCT_GAP_2.value();
    x += segment_tab(
        ui,
        theme,
        &p,
        egui::pos2(x, tab_y),
        &Segment {
            label: "Attention",
            // 0개일 때 배지가 사라지는 규칙도 이 함수에서 확인한다.
            count: Some(match tab {
                Tab::Attention { empty: true, .. } => 0,
                _ => attention::ENTRIES.len(),
            }),
            danger: true,
            selected: matches!(tab, Tab::Attention { .. }),
        },
    ) + STRUCT_GAP_2.value();
    segment_tab(
        ui,
        theme,
        &p,
        egui::pos2(x, tab_y),
        &Segment {
            label: "Add plugin",
            count: None,
            danger: false,
            selected: matches!(tab, Tab::Add { .. }),
        },
    );

    let close = theme.item_height_interactive.value();
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.max.x - theme.spacing_sm.value() - close,
            cy - close * 0.5,
        ),
        egui::vec2(close, close),
    );
    CLOSE
        .image(
            theme.icon_glyph_size_md.value(),
            theme.text_secondary().to_egui(),
        )
        .paint_at(
            ui,
            egui::Rect::from_center_size(
                close_rect.center(),
                egui::Vec2::splat(theme.icon_glyph_size_md.value()),
            ),
        );

    if !matches!(tab, Tab::Installed { .. }) {
        return;
    }

    let filter_w = theme.field_width_lg.value();
    let filter_h = theme.item_height_interactive.value();
    let filter_rect = egui::Rect::from_min_size(
        egui::pos2(
            close_rect.min.x - theme.spacing_sm.value() - filter_w,
            cy - filter_h * 0.5,
        ),
        egui::vec2(filter_w, filter_h),
    );
    p.rect(
        filter_rect,
        theme.corner_radius.value(),
        theme.surface_raised().to_egui(),
        egui::Stroke::new(theme.border_width.value(), theme.border_default().to_egui()),
        egui::StrokeKind::Inside,
    );
    p.text(
        egui::pos2(
            filter_rect.min.x + theme.spacing_sm.value(),
            filter_rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        "Filter installed…",
        egui::FontId::proportional(theme.font_size_body.value()),
        theme.text_placeholder().to_egui(),
    );
}

fn window(ui: &mut egui::Ui, theme: &Theme, tab: Tab) {
    let (rect, _) = ui.allocate_exact_size(stage_size(theme, tab), egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        theme.corner_radius.value(),
        theme.bg_panel().to_egui(),
    );

    let header_h =
        theme.item_height_interactive.value() + theme.spacing_lg.value() + theme.spacing_xs.value();
    let header_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), header_h));
    header(ui, theme, header_rect, tab);

    let body_top = header_rect.bottom();
    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, body_top), rect.max);

    let panes = |body: egui::Rect| {
        let list_rect =
            egui::Rect::from_min_max(body.min, egui::pos2(body.min.x + list_w(theme), body.max.y));
        let detail_rect = egui::Rect::from_min_max(egui::pos2(list_rect.max.x, body_top), body.max);
        (list_rect, detail_rect)
    };
    let divider = |ui: &mut egui::Ui, x: f32| {
        ui.painter().vline(
            x,
            egui::Rangef::new(body_top, body.max.y),
            egui::Stroke::new(
                theme.border_width.value(),
                theme.separator.to_egui_premultiplied(),
            ),
        );
    };

    match tab {
        Tab::Installed { detail } => {
            let (l, d) = panes(body);
            installed::list_pane(ui, theme, l, detail);
            installed::detail_pane(ui, theme, d, detail);
            divider(ui, l.max.x);
        }
        Tab::Attention { empty, selected } => {
            let (l, d) = panes(body);
            if empty {
                attention::empty_list_pane(ui, theme, l);
                attention::empty_detail_pane(ui, theme, d);
            } else {
                attention::list_pane(ui, theme, l, selected);
                attention::detail_pane(ui, theme, d, selected);
            }
            divider(ui, l.max.x);
        }
        Tab::Add { preview } => add::form_pane(ui, theme, body, preview),
    }
}

/// 디자인 `ThemePair` — Mocha·Latte를 `bg-app` 바탕에 그린다. 예제 폭이 560이라 위아래로 쌓는다.
fn theme_pair(
    ui: &mut egui::Ui,
    theme: &Theme,
    mocha: &Theme,
    latte: &Theme,
    draw: impl Fn(&mut egui::Ui, &Theme),
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing_lg.value();
        for th in [mocha, latte] {
            egui::Frame::new()
                .fill(th.bg_app().to_egui())
                .inner_margin(egui::Margin::same(theme.spacing_lg.value() as i8))
                .show(ui, |ui| draw(ui, th));
        }
    });
}

pub use installed::draw_install_paths;

/// Add plugin의 안내 상자 자리 — 점선 빈 안내와 매니페스트 읽기 오류. Mocha·Latte 짝.
pub fn draw_hint_slot(ui: &mut egui::Ui, theme: &Theme) {
    let latte = crate::host_shell::latte_theme();
    let mocha = tasty_themes::mocha_fallback();
    spec::stage(ui, theme, StageVariant::Solo, |ui| {
        theme_pair(ui, theme, &mocha, &latte, |ui, th| {
            add::hint_slot(ui, th, th.measure_xl.value());
        });
    });
    spec::cluster(
        ui,
        theme,
        "dashed edge — radius vs square: dashes on the straight edges, solid corners",
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing_lg.value();
                for radius in [theme.corner_radius.value(), 0.0] {
                    let size =
                        egui::vec2(theme.field_width_lg.value(), ControlSize::Md.height(theme));
                    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    paint_dashed_outline(
                        ui.painter(),
                        theme,
                        rect,
                        radius,
                        theme.border_default().to_egui(),
                    );
                }
            });
        },
    );
    spec::meta(
        ui,
        theme,
        &[
            (
                "empty hint",
                "dashed 1px border-default · radius · pad 14 / 16",
            ),
            (
                "dash",
                "4 on / 4 off · border-dash · border-dash-gap · OFF-SCALE",
            ),
            (
                "corners",
                "solid arc; dashes on straight edges only, centred",
            ),
            (
                "read error",
                "same box · solid accent-danger edge · no fill",
            ),
            (
                "title",
                "Can't read tasty-plugin.toml · body · accent-danger",
            ),
            ("reason", "mono caption · text-muted · untranslated"),
            (
                "invalid",
                "read but fails validation (binary path · extras) — same box · title “tasty-plugin.toml is not valid” · reason = validation message",
            ),
            ("action", "none — fix the path above and Verify again"),
        ],
        &[
            TokenChip::without_color("border-dash", "→ size-4"),
            TokenChip::without_color("border-dash-gap", "→ size-4"),
            TokenChip::new(
                "border-default",
                "hint edge",
                theme.border_default().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "read error",
                theme.accent_danger().to_egui(),
            ),
        ],
    );
    spec::note(
        ui,
        theme,
        "The same dash pair draws the Scripts Add trigger… control (Misc › Scripts). The boxes are the shared paint_dashed_outline and plugin_add_read_error widgets the host calls. Strings: plugins.add_read_error “Can't read tasty-plugin.toml” · plugins.add_invalid “tasty-plugin.toml is not valid”.",
    );
}

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    for (label, tab) in [
        (
            "Installed",
            Tab::Installed {
                detail: installed::Detail::Selected(0),
            },
        ),
        (
            "Installed — health error",
            Tab::Installed {
                detail: installed::Detail::Selected(1),
            },
        ),
        (
            "Installed — no selection",
            Tab::Installed {
                detail: installed::Detail::None,
            },
        ),
        (
            "Installed — uninstall confirm",
            Tab::Installed {
                detail: installed::Detail::ConfirmUninstall(0),
            },
        ),
        (
            "Attention",
            Tab::Attention {
                empty: false,
                selected: attention::SELECTED,
            },
        ),
        (
            "Attention — signature invalid (description and homepage hidden)",
            Tab::Attention {
                empty: false,
                selected: attention::SIGNATURE_INVALID,
            },
        ),
        (
            "Attention — empty",
            Tab::Attention {
                empty: true,
                selected: attention::SELECTED,
            },
        ),
        ("Add plugin — before Verify", Tab::Add { preview: false }),
        (
            "Add plugin — verified manifest under the input (untrusted)",
            Tab::Add { preview: true },
        ),
    ] {
        spec::cluster(ui, theme, label, |ui| {
            spec::stage(ui, theme, StageVariant::Solo, |ui| window(ui, theme, tab));
        });
    }
    spec::cluster(
        ui,
        theme,
        "Installed detail — command keycaps per platform (macOS: Ctrl · Option · Shift · Command order, user display style, Ctrl stays a word)",
        |ui| {
            spec::stage(ui, theme, StageVariant::Solo, |ui| {
                installed::keycap_platforms(ui, theme);
            });
        },
    );
    let latte = crate::host_shell::latte_theme();
    let mocha = tasty_themes::mocha_fallback();
    spec::cluster(
        ui,
        theme,
        "Add plugin — action bar (trusted · permissions changed · 4 unverified · adding · already installed)",
        |ui| {
            theme_pair(ui, theme, &mocha, &latte, |ui, th| {
                add::add_bars(ui, th, th.measure_xl.value());
            });
        },
    );
    spec::cluster(
        ui,
        theme,
        "Add plugin — open values (homepage link · None · long fingerprint)",
        |ui| {
            theme_pair(ui, theme, &mocha, &latte, |ui, th| {
                add::open_values(ui, th, th.measure_xl.value());
            });
        },
    );
    spec::cluster(
        ui,
        theme,
        "Add plugin — trust judgment (6 kinds, 4 unverified share one danger box)",
        |ui| {
            theme_pair(ui, theme, &mocha, &latte, |ui, th| {
                add::trust_boxes(ui, th, th.measure_xl.value());
            });
        },
    );
    spec::cluster(ui, theme, "attention reasons (4)", |ui| {
        attention::reason_cards(ui, theme);
    });

    spec::meta(
        ui,
        theme,
        &[
            (
                "header",
                "높이 48 · plug + 타이틀 + 1px 구분선 + 세그먼트 3",
            ),
            (
                "segments",
                "Installed N(mono count) / Attention N(danger 배지) / Add plugin",
            ),
            (
                "list",
                "폭 `plugins_side_panel_width` · 아바타 32 · 이름 13 + 부제 10 muted",
            ),
            (
                "builtin",
                "이름 뒤 `•` · 상세는 버전 Tag 뒤 기본 Tag built-in",
            ),
            (
                "installed detail",
                "identity(이름 font-size-max 보통 굵기 · 메타 작성자 · id · homepage 링크) → 설명(line-height-ui) → (health 박스) → Permissions Tag → Command(키캡 규칙) → Surface kinds Tag(있을 때만) → 경로",
            ),
            (
                "uninstall confirm",
                "액션 바 자리 교체 · alertTriangle 16 accent-attention · 질문 body + 안내 caption muted · Cancel(ghost) · Uninstall(danger)",
            ),
            ("health error", "enabled + error 인 행만 우측 danger dot"),
            (
                "attention",
                "같은 2열 · 2번째 줄이 사유 라벨(severity 색) · 상세는 배너 + 사유 detail + 액션 바 · 서명 사유의 배너는 Add 탭과 같은 미인증 상자(fingerprint 줄 없음)이고 바 오른쪽이 Register anyway(danger) · 0 건이면 배지가 사라지고 본문이 안내로 바뀐다",
            ),
            (
                "add",
                "단일 열 · 제목 없이 경로 선택 블록(Plugin folder 머리글 · mono 입력 + Find folder…(secondary) + Verify(primary) · 설명 문단) 바로 아래에 확인 전 안내 상자 또는 매니페스트 카드(아바타 lg · 이름 + 버전 Tag · id · 작성자 · 설명 · Permissions/Surface kinds Tag, 비면 caption None · Source · Homepage 링크) + 신뢰 상자 · 액션 바는 왼쪽 Grants 문구 또는 막힌 이유(caption, text-muted) + 오른쪽 Cancel(ghost) + Add plugin(primary, 권한 변경이면 Trust & add), 미인증 네 가지는 Add anyway(danger, 모르는 키는 Trust key & add anyway)이고 처음 포커스는 Cancel, 추가 중에는 두 버튼 disabled + Spinner Adding…, 막힘은 이미 설치됨뿐, 확인 전에는 Cancel 만",
            ),
            (
                "copy fingerprint",
                "IconButton sm copy · after the value · absent without a fingerprint",
            ),
            (
                "attention bar",
                "no Details — the reason panel already shows the blurb + fingerprint",
            ),
            ("add — blocked", "disabled Add plugin · reason on the left"),
            (
                "reasons",
                "Already installed · Signed, but the publisher's public key file is missing · Signature check failed",
            ),
            (
                "untrusted + .pub",
                "Trust & add — Primary (agent is for AI-agent surfaces only)",
            ),
            (
                "grants",
                "No permissions · Grants 1 permission · Grants N permissions",
            ),
        ],
        &[
            TokenChip::new(
                "accent-decorative",
                "header plug",
                theme.plugins_header_glyph().to_egui(),
            ),
            TokenChip::new(
                "accent-danger",
                "attention badge · health dot",
                theme.accent_danger().to_egui(),
            ),
            TokenChip::new(
                "surface-active",
                "selected row · tab",
                theme.surface_active().to_egui(),
            ),
            TokenChip::new("bg-sidebar", "header · list", theme.bg_sidebar().to_egui()),
            TokenChip::without_color("tint-fill-alpha", "box fill"),
            TokenChip::without_color("tint-border-alpha", "box edge"),
            TokenChip::new(
                "accent-warning",
                "add + trust",
                theme.accent_warning().to_egui(),
            ),
            TokenChip::new(
                "accent-success",
                "trusted",
                theme.accent_success().to_egui(),
            ),
            TokenChip::new(
                "state-disabled-fg",
                "disabled ink",
                theme.state_disabled_fg().to_egui(),
            ),
            TokenChip::new("text-muted", "reason", theme.text_muted().to_egui()),
        ],
    );

    spec::note(
        ui,
        theme,
        "세 탭은 서로 다른 일을 한다 — Installed 는 설치된 plugin 의 상태·권한·명령을 보고, \
         Attention 은 서명·권한 변경으로 등록이 거부됐거나 런타임에서 실패한 plugin 만 모아 \
         이유와 필요한 조치를 보여주며, Add plugin 은 로컬 폴더 경로로 설치한다. 검색 입력은 \
         목록이 있는 Installed 탭에서만 나타난다. 사용자가 직접 끈 plugin 은 error 가 아니라 \
         정상 종료이므로 danger dot 이 붙지 않는다.",
    );

    spec::note(
        ui,
        theme,
        "Attention 의 severity 는 사유로 갈린다 — 서명 계열(신뢰 안 됨 · 무효)은 danger, \
         권한 변경과 런타임 오류는 warning 이다. 이것은 Installed 목록의 health dot 과 다른 \
         축이다: health dot 은 '실행 중 실패' 하나만 보고, Attention 은 등록 거부까지 함께 \
         모은다. Add 는 미신뢰 plugin 에 공개키가 있을 때만 Add 버튼이 살아 있다 — 공개키가 \
         없거나 서명 오류면 신뢰를 등록할 방법이 없어 버튼이 꺼진다.",
    );
}
