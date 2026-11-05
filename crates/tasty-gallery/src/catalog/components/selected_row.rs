//! 선택·키보드 활성 행 위의 글리프, Tag, 일치 구간, Kbd `+`. 쉬는 행과 활성 행을 나란히 그린다.
//! 색이 뜻을 나르는 글리프는 자기 `-on-active` 역할을, 모양이 뜻을 나르는 글리프는 행 글자색을 쓴다.

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{
    KbdKey, StatusKind, TagVariant, explorer_match_job, kbd_parts_at, status_dot, tag,
    tag_with_icon,
};

use super::sidebar::with_zoom;
use crate::catalog::icons;
use crate::catalog::spec::{self, StageVariant, TokenChip};

pub fn draw(ui: &mut egui::Ui, theme: &Theme) {
    let palettes = [
        ("Mocha", tasty_themes::mocha_fallback()),
        ("Latte", crate::host_shell::latte_theme()),
    ];
    for (palette, base) in &palettes {
        let th = with_zoom(base, theme.ui_zoom);
        spec::stage(ui, &th, StageVariant::Column, |ui| {
            spec::cluster(ui, &th, palette, |ui| {
                ui.vertical(|ui| {
                    for active in [false, true] {
                        rows(ui, &th, active);
                    }
                });
            });
        });
    }
    spec::meta(
        ui,
        theme,
        &[
            ("hue glyph", "own -on-active role, 3:1 on the row"),
            ("shape glyph", "text-on-active"),
            ("chip", "Tag recipe, unchanged on the row"),
            ("match run", "row ink with a 1px underline"),
            (
                "Kbd +",
                "kbd-plus-fg at rest, kbd-plus-fg-active on the row",
            ),
        ],
        &[
            TokenChip::new(
                "port-star-on-active",
                "favorite star",
                theme.port_star_on_active().to_egui(),
            ),
            TokenChip::new(
                "accent-agent-on-active",
                "plugin dot, glyph",
                theme.accent_agent_on_active().to_egui(),
            ),
            TokenChip::new(
                "workspace-mirror-fg-active",
                "mirror glyph",
                theme.workspace_mirror_fg_active().to_egui(),
            ),
            TokenChip::new(
                "status-dot-success-active",
                "running dot",
                theme.status_dot_success_active().to_egui(),
            ),
            TokenChip::new(
                "status-dot-attached-ring-active",
                "attached ring",
                theme.status_dot_attached_ring_active().to_egui(),
            ),
            TokenChip::new(
                "explorer-match-fg-active",
                "match run",
                theme.explorer_match_fg_active().to_egui(),
            ),
            TokenChip::new(
                "kbd-plus-fg-active",
                "+ between keycaps",
                theme.kbd_plus_fg_active().to_egui(),
            ),
        ],
    );
}

fn rows(ui: &mut egui::Ui, th: &Theme, active: bool) {
    let ink = if active {
        th.text_on_active()
    } else {
        th.text_secondary()
    }
    .to_egui();
    let label = |ui: &mut egui::Ui, text: &str| {
        ui.label(
            egui::RichText::new(text)
                .size(th.font_size_body.value())
                .color(ink),
        );
    };
    row(ui, th, active, |ui| {
        glyph(
            ui,
            th,
            icons::STAR_FILL,
            if active {
                th.port_star_on_active()
            } else {
                th.port_star_on()
            }
            .to_egui(),
        );
        glyph(
            ui,
            th,
            icons::STAR,
            if active {
                th.port_star_off_active()
            } else {
                th.port_star_off()
            }
            .to_egui(),
        );
        label(ui, "127.0.0.1:5173");
    });
    row(ui, th, active, |ui| {
        let agent = if active {
            th.accent_agent_on_active()
        } else {
            th.accent_agent()
        }
        .to_egui();
        let d = th.status_dot_size().value();
        let (r, _) = ui.allocate_exact_size(egui::vec2(d, d), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), d * 0.5, agent);
        glyph(ui, th, icons::PLUG, agent);
        label(ui, "git-helper");
    });
    row(ui, th, active, |ui| {
        status_dot(
            ui,
            th,
            StatusKind::Running,
            "",
            false,
            true,
            row_ground(active),
        );
        glyph(
            ui,
            th,
            icons::REMOTE,
            if active {
                th.workspace_mirror_fg_active()
            } else {
                th.workspace_mirror_fg()
            }
            .to_egui(),
        );
        label(ui, "Project A");
        tag(ui, th, "remote", TagVariant::Info, false);
    });
    row(ui, th, active, |ui| {
        attached_dot(ui, th, active);
        label(ui, "build");
        tag(
            ui,
            th,
            crate::i18n::t("remote_attach.in_use"),
            TagVariant::Attached,
            false,
        );
        tag_with_icon(
            ui,
            th,
            crate::i18n::t("remote_attach.inactive"),
            TagVariant::Warning,
            icons::ALERT_TRIANGLE,
        );
    });
    row(ui, th, active, |ui| {
        let kind = if active {
            th.text_on_active()
        } else {
            th.text_muted()
        }
        .to_egui();
        glyph(ui, th, icons::FOLDER, kind);
        let image = if active {
            th.text_on_active()
        } else {
            th.accent_info()
        }
        .to_egui();
        glyph(ui, th, icons::IMAGE, image);
        ui.label(explorer_match_job(
            th,
            "diagram.png",
            "gram",
            egui::FontId::proportional(th.font_size_body.value()),
            ink,
            active,
        ));
    });
    let keys = row(ui, th, active, |ui| label(ui, "Split pane right"));
    kbd_parts_at(
        ui,
        th,
        &[
            KbdKey::Text(crate::i18n::t("settings.terminal.link_modifier_ctrl")),
            KbdKey::Text("D"),
        ],
        keys.right() - th.spacing_sm.value(),
        keys.center().y,
        row_ground(active),
    );
    row(ui, th, active, |ui| {
        for kind in [
            StatusKind::Running,
            StatusKind::Waiting,
            StatusKind::Error,
            StatusKind::Agent,
            StatusKind::NeedsInput,
            StatusKind::Completion,
            StatusKind::Idle,
        ] {
            status_dot(ui, th, kind, "", false, true, row_ground(active));
        }
    });
}

/// 행 하나. 활성이면 surface-active로 채우고 안쪽 가로 영역을 `content`에 넘긴다.
fn row(
    ui: &mut egui::Ui,
    th: &Theme,
    active: bool,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let width = th.measure_sm.value();
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(width, th.item_height_interactive.value()),
        egui::Sense::hover(),
    );
    if active {
        ui.painter().rect_filled(
            rect,
            th.corner_radius_sm.value(),
            th.surface_active().to_egui(),
        );
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::Vec2::X * th.spacing_sm.value()))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = th.spacing_sm.value();
    content(&mut child);
    rect
}

fn glyph(ui: &mut egui::Ui, th: &Theme, icon: tasty_icons::Icon, color: egui::Color32) {
    let size = th.icon_glyph_size_sm.value();
    let (r, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    icon.image(size, color).paint_at(ui, r);
}

/// 쉬는 점에 다른 클라이언트 점유 고리를 두른다.
fn attached_dot(ui: &mut egui::Ui, th: &Theme, active: bool) {
    let slot = th.workspace_dot_slot().value();
    let (r, _) = ui.allocate_exact_size(egui::vec2(slot, slot), egui::Sense::hover());
    let dot_r = th.status_dot_size().value() * 0.5;
    let ring_w = th.status_dot_attached_ring_width().value();
    let (dot, ring) = if active {
        (
            th.status_dot_idle_active(),
            th.status_dot_attached_ring_active(),
        )
    } else {
        (th.status_dot_idle(), th.status_dot_attached_ring())
    };
    ui.painter().circle_filled(r.center(), dot_r, dot.to_egui());
    ui.painter().circle_stroke(
        r.center(),
        dot_r + th.status_dot_attached_ring_offset().value() + ring_w * 0.5,
        egui::Stroke::new(ring_w, ring.to_egui()),
    );
}

/// 행이 선택됐으면 active 칸, 아니면 패널 칸이다.
fn row_ground(active: bool) -> tasty_ui_widgets::Ground {
    if active {
        tasty_ui_widgets::Ground::Active
    } else {
        tasty_ui_widgets::Ground::Panel
    }
}
