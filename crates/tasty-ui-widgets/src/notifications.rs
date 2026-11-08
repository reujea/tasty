//! Shared notification channel views. State, translation and navigation belong to the caller.
mod item;
mod panel;
mod summary;

pub use panel::{NotificationAction, NotificationPanelState, notification_panel};
pub use summary::{NotificationSummary, notification_summary};

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationTone {
    Error,
    Warning,
    Info,
}

impl NotificationTone {
    fn index(self) -> usize {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
    fn icon(self) -> tasty_icons::Icon {
        match self {
            Self::Error => tasty_icons::ALERT_CIRCLE,
            Self::Warning => tasty_icons::ALERT_TRIANGLE,
            Self::Info => tasty_icons::INFO,
        }
    }
    fn color(self, th: &Theme) -> egui::Color32 {
        match self {
            Self::Error => th.notification_badge_error_bg(),
            Self::Warning => th.notification_badge_warning_bg(),
            Self::Info => th.notification_badge_info_bg(),
        }
        .into()
    }
    fn glyph_color(self, th: &Theme) -> egui::Color32 {
        match self {
            Self::Error => th.notification_item_error_glyph(),
            Self::Warning => th.notification_item_warning_glyph(),
            Self::Info => th.notification_item_info_glyph(),
        }
        .into()
    }
    fn ink(self, th: &Theme) -> egui::Color32 {
        match self {
            Self::Error => th.notification_badge_error_fg(),
            Self::Warning => th.notification_badge_warning_fg(),
            Self::Info => th.notification_badge_info_fg(),
        }
        .into()
    }
}

pub struct NotificationLabels<'a> {
    pub levels: [&'a str; 3],
    pub no_unread: &'a str,
    pub empty: &'a str,
    pub mark_all: &'a str,
    pub mark_read: &'a str,
    pub jump: &'a str,
    pub closed: &'a str,
    pub source_closed: &'a str,
    pub more: &'a str,
    pub less: &'a str,
}

pub struct NotificationRow<'a> {
    pub id: u64,
    pub tone: NotificationTone,
    pub unread: bool,
    pub title: &'a str,
    pub body: &'a str,
    pub source: &'a str,
    pub source_closed: bool,
    pub time: &'a str,
}

/// 알림 한 건을 패널 없이 펼쳐 그린다. 다른 화면의 Spec 이 알림 문구를 보일 때 쓴다.
pub fn notification_item_expanded(
    ui: &mut egui::Ui,
    th: &Theme,
    row: &NotificationRow<'_>,
    labels: &NotificationLabels<'_>,
) -> Option<NotificationAction> {
    let width = LogicalPx(ui.available_width());
    let plan = item::plan(ui, th, row, width, true, false);
    item::draw(ui, th, row, labels, &plan, true)
}

fn counts(rows: &[NotificationRow<'_>]) -> [usize; 3] {
    let mut counts = [0; 3];
    for row in rows.iter().filter(|row| row.unread) {
        counts[row.tone.index()] += 1;
    }
    counts
}

// The design caps the displayed count, not the stored or accessible count.
fn count_label(count: usize) -> String {
    if count > 99 {
        "99+".into()
    } else {
        count.to_string()
    }
}

fn badge_size(ui: &egui::Ui, th: &Theme, count: usize) -> egui::Vec2 {
    let text = ui.painter().layout_no_wrap(
        count_label(count),
        egui::FontId::monospace(th.badge_font_size().value()),
        th.text_primary().into(),
    );
    egui::vec2(
        text.size().x
            + th.notification_badge_glyph_size().value()
            + th.notification_badge_glyph_gap().value()
            + th.badge_padding_x().value() * 2.0,
        th.badge_size().value(),
    )
}

fn badges_size(ui: &egui::Ui, th: &Theme, counts: [usize; 3], vertical: bool) -> egui::Vec2 {
    let sizes: Vec<_> = counts
        .into_iter()
        .filter(|n| *n > 0)
        .map(|n| badge_size(ui, th, n))
        .collect();
    let gaps = sizes.len().saturating_sub(1) as f32;
    if vertical {
        egui::vec2(
            sizes.iter().map(|s| s.x).fold(0.0, f32::max),
            sizes.iter().map(|s| s.y).sum::<f32>()
                + gaps * th.notification_badge_stack_gap().value(),
        )
    } else {
        egui::vec2(
            sizes.iter().map(|s| s.x).sum::<f32>()
                + gaps * th.notification_badge_group_gap().value(),
            sizes.iter().map(|s| s.y).fold(0.0, f32::max),
        )
    }
}

fn paint_badges(
    ui: &egui::Ui,
    th: &Theme,
    rect: egui::Rect,
    counts: [usize; 3],
    labels: [&str; 3],
    vertical: bool,
) {
    let mut pos = rect.min;
    for tone in [
        NotificationTone::Error,
        NotificationTone::Warning,
        NotificationTone::Info,
    ] {
        let n = counts[tone.index()];
        if n == 0 {
            continue;
        }
        let size = badge_size(ui, th, n);
        let start = if vertical {
            egui::pos2(rect.center().x - size.x * 0.5, pos.y)
        } else {
            pos
        };
        let badge = egui::Rect::from_min_size(start, size);
        ui.painter()
            .rect_filled(badge, th.badge_size().value() * 0.5, tone.color(th));
        let glyph = th.notification_badge_glyph_size().value();
        let icon = egui::Rect::from_center_size(
            egui::pos2(
                badge.left() + th.badge_padding_x().value() + glyph * 0.5,
                badge.center().y,
            ),
            egui::vec2(glyph, glyph),
        );
        tone.icon().image(glyph, tone.ink(th)).paint_at(ui, icon);
        ui.painter().text(
            egui::pos2(
                icon.right() + th.notification_badge_glyph_gap().value(),
                badge.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            count_label(n),
            egui::FontId::monospace(th.badge_font_size().value()),
            tone.ink(th),
        );
        ui.interact(
            badge,
            ui.id().with(("notification_badge", tone.index())),
            egui::Sense::hover(),
        )
        .on_hover_text(format!("{}: {n}", labels[tone.index()]));
        if vertical {
            pos.y += size.y + th.notification_badge_stack_gap().value();
        } else {
            pos.x += size.x + th.notification_badge_group_gap().value();
        }
    }
}

fn text_job(
    ui: &egui::Ui,
    th: &Theme,
    text: &str,
    size: LogicalPx,
    color: egui::Color32,
    width: LogicalPx,
    rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        egui::FontId::proportional(size.value()),
        color,
        width.value().max(0.0),
    );
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = false;
    for section in &mut job.sections {
        section.format.line_height = Some(size.value() * th.line_height_ui);
    }
    ui.fonts(|fonts| fonts.layout_job(job))
}
