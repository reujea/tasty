//! Host adapter for the shared notification widgets. Only explicit user actions acknowledge items.
use crate::i18n::{t, t_fmt};
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use tasty_ui_widgets::{
    NotificationAction, NotificationLabels, NotificationPanelState, NotificationRow,
    NotificationSummary, NotificationTone,
};

fn tone(level: crate::notification::NotificationLevel) -> NotificationTone {
    match level {
        crate::notification::NotificationLevel::Error => NotificationTone::Error,
        crate::notification::NotificationLevel::Warning => NotificationTone::Warning,
        crate::notification::NotificationLevel::Info => NotificationTone::Info,
    }
}

fn level_labels() -> [&'static str; 3] {
    [
        t("notification_panel.errors"),
        t("notification_panel.warnings"),
        t("notification_panel.info"),
    ]
}

fn elapsed(timestamp: std::time::Instant) -> String {
    let secs = timestamp.elapsed().as_secs();
    let (key, value) = if secs < 60 {
        ("notification_panel.elapsed_seconds", secs)
    } else if secs < 3600 {
        ("notification_panel.elapsed_minutes", secs / 60)
    } else {
        ("notification_panel.elapsed_hours", secs / 3600)
    };
    t_fmt(key, &value.to_string())
}

pub(crate) struct SummaryData {
    counts: [usize; 3],
    latest: Option<(NotificationTone, String, String)>,
    tooltip: String,
}
impl SummaryData {
    pub(crate) fn view(&self, open: bool) -> NotificationSummary<'_> {
        NotificationSummary {
            counts: self.counts,
            label: t("notification_panel.window_title"),
            tooltip: &self.tooltip,
            level_labels: level_labels(),
            latest: self
                .latest
                .as_ref()
                .map(|(tone, title, time)| (*tone, title.as_str(), time.as_str())),
            open,
        }
    }
}

pub(crate) fn summary_data(engine: &EngineRead<'_>) -> SummaryData {
    let n = engine.live.notifications.unread_counts(None);
    let counts = [n.error, n.warning, n.info];
    let latest = engine
        .live
        .notifications
        .all()
        .rev()
        .find(|n| !n.read)
        .map(|n| (tone(n.level), n.title.clone(), elapsed(n.timestamp)));
    let labels = level_labels();
    let tooltip = if counts.iter().all(|n| *n == 0) {
        format!(
            "{} — {}",
            t("notification_panel.window_title"),
            t("notification_panel.no_unread")
        )
    } else {
        format!(
            "{} — {}: {} · {}: {} · {}: {}",
            t("notification_panel.window_title"),
            labels[0],
            counts[0],
            labels[1],
            counts[1],
            labels[2],
            counts[2]
        )
    };
    SummaryData {
        counts,
        latest,
        tooltip,
    }
}

pub(crate) fn toggle_from_sidebar(state: &mut MainViewState, rect: egui::Rect) {
    let th = crate::theme::theme();
    let pos = egui::pos2(
        rect.right() + th.spacing_xs.value(),
        (rect.bottom() - th.notifications_popup_height().value()).max(th.spacing_sm.value()),
    );
    state.dispatch_intent(
        crate::intent::UiIntent::TogglePopup {
            id: "notifications",
            mode: crate::intent::OpenPopupMode::AtFocused(
                crate::model::popup_kind::PopupScope::Window,
                pos,
            ),
        }
        .from_user_menu("notifications.summary"),
    );
}

pub(crate) fn draw_notification_content_inner(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
) {
    draw_notification_content(ui, state, engine, false);
}

pub(crate) fn draw_notification_content(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    wide: bool,
) {
    let th = crate::theme::theme();
    let items: Vec<_> = engine.live.notifications.all().rev().collect();
    let sources: Vec<_> = items
        .iter()
        .map(|n| {
            let ws = engine
                .workspaces()
                .into_iter()
                .find(|ws| ws.id == n.source_workspace);
            let closed =
                ws.is_none() || (n.source_surface != 0 && !engine.has_surface(n.source_surface));
            let name = ws
                .map(|ws| ws.name.as_str())
                .unwrap_or(t("notification_panel.unknown_workspace"));
            let source = if n.source_surface == 0 {
                name.to_string()
            } else {
                format!("{name} › #{}", n.source_surface)
            };
            (source, closed, elapsed(n.timestamp))
        })
        .collect();
    let rows: Vec<_> = items
        .iter()
        .zip(&sources)
        .map(|(n, (source, closed, time))| NotificationRow {
            id: n.id,
            tone: tone(n.level),
            unread: !n.read,
            title: &n.title,
            body: &n.body,
            source,
            source_closed: *closed,
            time,
        })
        .collect();
    let labels = NotificationLabels {
        levels: level_labels(),
        no_unread: t("notification_panel.no_unread"),
        empty: t("notification_panel.empty_message"),
        mark_all: t("button.mark_all_read"),
        mark_read: t("notification_panel.mark_read"),
        jump: t("button.jump_to_workspace"),
        closed: t("notification_panel.closed"),
        source_closed: t("notification_panel.source_closed"),
        more: t("notification_panel.more"),
        less: t("notification_panel.less"),
    };
    let key = ui.id().with("notification_view_state");
    let mut panel = ui
        .data(|d| d.get_temp::<NotificationPanelState>(key))
        .unwrap_or_default();
    let actions = tasty_ui_widgets::notification_panel(ui, &th, &rows, &labels, &mut panel, wide);
    ui.data_mut(|d| d.insert_temp(key, panel));
    for action in actions {
        match action {
            NotificationAction::Read(id) => mark_read(state, id),
            NotificationAction::ReadAll => state.dispatch_intent(
                crate::app::command::DomainIntent::MarkAllNotificationsRead {
                    ids: items.iter().filter(|n| !n.read).map(|n| n.id).collect(),
                }
                .from_user_menu("notification_panel.mark_all_read"),
            ),
            NotificationAction::Jump(id) => {
                if let Some(n) = items.iter().find(|n| n.id == id) {
                    if n.source_surface != 0 {
                        state.reveal_surface(engine, n.source_surface);
                    } else if let Some(idx) = engine
                        .workspaces()
                        .into_iter()
                        .position(|ws| ws.id == n.source_workspace)
                    {
                        state.switch_workspace(engine, idx);
                    }
                    mark_read(state, id);
                }
            }
            NotificationAction::Toggle(_) => {}
        }
    }
}

fn mark_read(state: &mut MainViewState, id: u64) {
    state.dispatch_intent(
        crate::app::command::DomainIntent::MarkNotificationRead { id }
            .from_user_menu("notification_panel.mark_read"),
    );
}

pub fn notifications_popup_size() -> egui::Vec2 {
    let th = crate::theme::theme();
    egui::vec2(
        th.notifications_popup_width().value(),
        th.notifications_popup_height().value(),
    )
}
pub fn notifications_popup_sizer(_state: &MainViewState, _engine: &EngineRead<'_>) -> egui::Vec2 {
    notifications_popup_size()
}
pub fn draw_notification_popup(
    ui: &mut egui::Ui,
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    _cx: &mut crate::adapters::ui::popup::PopupCx<'_>,
) -> crate::adapters::ui::popup::PopupAction {
    draw_notification_content_inner(ui, state, engine);
    crate::adapters::ui::popup::PopupAction::None
}
