//! 탐색기 내용 영역의 상태 화면 — 빈 폴더 · 권한 거부 · 불러오는 중 · 읽기 오류.
//! 배치는 공용 [`tasty_ui_widgets::state_screen`](시안 `ExpState`)이 맡고 여기서는 내용과 색조를 고른다.
//! 권한 거부는 글리프와 제목을 accent-warning, 읽기 오류는 explorer-error-fg 로 칠하고, 불러오는 중은
//! 글리프 자리에 Spinner 를 둔다. 읽기 오류는 OS 이유 문구를 번역하지 않고 보이며 Retry(같은 경로를
//! 다시 읽음)와 Go up(상위 폴더, 루트에서는 숨김)을 둔다. 바로 위 폴더도 사라졌으면 Go up 은 남아 있는
//! 가장 가까운 상위 폴더로 간다.
//! 시안의 패널 배경·테두리는 갤러리 전시 칸이고 본체에서는 내용 영역 자체가 그 자리다.
//! 낮은 내용 영역에서는 공용 위젯이 compact 한 줄로 바꾼다.

use std::path::Path;

use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{ButtonVariant, StateGlyph as WidgetGlyph, StateScreenView, state_screen};

use super::ExplorerAction;
use super::find::FindScreen;
use super::view::{ExplorerView, LoadState};
use crate::adapters::ui::icons::{self, Icon};
use crate::i18n::t;

/// 상태 화면의 글리프 — 아이콘 또는 Spinner.
#[derive(Clone, Copy)]
enum StateGlyph {
    Icon(Icon),
    Spinner,
}

/// 글리프와 제목의 색조. 중립은 글리프 text-muted · 제목 text-secondary 다.
#[derive(Clone, Copy)]
enum Tone {
    Neutral,
    Warning,
    Error,
}

/// 상태 화면 하나의 내용.
struct StateScreen<'a> {
    glyph: StateGlyph,
    tone: Tone,
    title: &'a str,
    sub: Option<&'a str>,
    /// 번역하지 않은 OS 이유 문구(mono).
    reason: Option<&'a str>,
    /// 읽기 오류의 Retry · Go up 버튼. Go up 은 상위 폴더가 있을 때만 있다.
    actions: Option<ReadErrorActions>,
}

#[derive(Clone, Copy)]
struct ReadErrorActions {
    go_up: bool,
}

/// 목록 대신 상태 화면을 그려야 하면 받은 영역에 그리고 true 를 돌려준다.
/// 읽기 오류 화면의 버튼을 누르면 `action` 에 Refresh 또는 GoUp 을 넣는다.
/// 빈 폴더라도 새 항목 이름을 입력하는 중이면 목록 영역이 그 이름 행을 그려야 하므로 false 다.
pub(super) fn show_for(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &ExplorerView,
    root: &Path,
    action: &mut Option<ExplorerAction>,
) -> bool {
    let screen = match &view.state {
        LoadState::NoPermission => StateScreen {
            glyph: StateGlyph::Icon(icons::LOCK),
            tone: Tone::Warning,
            title: t("explorer.state.no_permission"),
            sub: Some(t("explorer.state.no_permission_sub")),
            reason: None,
            actions: None,
        },
        // 읽을 수 없음을 빈 폴더로 떨어뜨리지 않는다(시안 ssh config Note 의 원칙).
        LoadState::Error(msg) => StateScreen {
            glyph: StateGlyph::Icon(icons::ALERT_TRIANGLE),
            tone: Tone::Error,
            title: t("explorer.state.read_error"),
            sub: None,
            reason: Some(msg),
            actions: Some(ReadErrorActions {
                go_up: root.parent().is_some(),
            }),
        },
        LoadState::Loading => StateScreen {
            glyph: StateGlyph::Spinner,
            tone: Tone::Neutral,
            title: t("explorer.state.loading"),
            sub: None,
            reason: None,
            actions: None,
        },
        LoadState::Ok if view.entries.is_empty() && view.create.is_none() => StateScreen {
            glyph: StateGlyph::Icon(icons::FOLDER_OPEN),
            tone: Tone::Neutral,
            title: t("explorer.state.empty"),
            sub: None,
            reason: None,
            actions: None,
        },
        LoadState::Ok => return false,
    };
    if let Some(clicked) = show(ui, theme, &screen)
        && action.is_none()
    {
        *action = Some(match clicked {
            ExplorerAction::GoUp => view.go_up_action(root),
            other => other,
        });
    }
    true
}

/// 거르기에 맞는 이름이 없을 때의 제목. 번역문은 이름 붙은 `{query}` 자리로 입력한 글자를 받는다.
pub(super) fn no_filter_matches_title(query: &str) -> String {
    t("explorer.find.none_filter").replace("{query}", query)
}

/// Find 결과가 없거나 하위 폴더 검색이 실패했으면 목록 대신 상태 화면을 그리고 true 를 돌려준다.
/// 실패 화면의 Retry 는 새로고침이고, 새로고침은 하위 폴더 검색을 처음부터 다시 한다.
/// 새 항목 이름을 입력하는 중이면 목록 영역이 그 이름 행을 그리도록 어느 화면이든 false 다.
pub(super) fn show_find(
    ui: &mut egui::Ui,
    theme: &Theme,
    view: &ExplorerView,
    action: &mut Option<ExplorerAction>,
) -> bool {
    if view.create.is_some() {
        return false;
    }
    let title;
    let screen = match view.find_screen() {
        None => return false,
        Some(FindScreen::NoFilterMatches { query }) => {
            title = no_filter_matches_title(&query);
            StateScreen {
                glyph: StateGlyph::Icon(icons::SEARCH),
                tone: Tone::Neutral,
                title: &title,
                sub: None,
                reason: None,
                actions: None,
            }
        }
        Some(FindScreen::NoSearchMatches { folder }) => {
            title = crate::i18n::t_fmt("explorer.find.none", &folder);
            StateScreen {
                glyph: StateGlyph::Icon(icons::SEARCH),
                tone: Tone::Neutral,
                title: &title,
                sub: Some(t("explorer.find.none_sub")),
                reason: None,
                actions: None,
            }
        }
        Some(FindScreen::Failed(reason)) => {
            title = reason;
            StateScreen {
                glyph: StateGlyph::Icon(icons::ALERT_TRIANGLE),
                tone: Tone::Error,
                title: t("explorer.find.failed"),
                sub: None,
                reason: Some(&title),
                actions: Some(ReadErrorActions { go_up: false }),
            }
        }
    };
    if let Some(clicked) = show(ui, theme, &screen)
        && action.is_none()
    {
        *action = Some(clicked);
    }
    true
}

/// 받은 영역 전체를 차지하고 상태 화면을 그린다. 누른 버튼의 액션을 돌려준다.
/// Retry 는 Secondary, Go up 은 Ghost 이며 Go up 은 상위 폴더가 있을 때만 둔다.
fn show(ui: &mut egui::Ui, theme: &Theme, s: &StateScreen<'_>) -> Option<ExplorerAction> {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ui.available_height()),
        egui::Sense::hover(),
    );
    let (glyph_color, title_color) = tone_colors(theme, s.tone);
    let with_go_up = [
        (
            t("explorer.state.read_error_retry"),
            ButtonVariant::Secondary,
        ),
        (t("explorer.state.read_error_go_up"), ButtonVariant::Ghost),
    ];
    let actions: &[(&str, ButtonVariant)] = match s.actions {
        Some(ReadErrorActions { go_up: true }) => &with_go_up,
        Some(ReadErrorActions { go_up: false }) => &with_go_up[..1],
        None => &[],
    };
    let paint;
    let glyph = match s.glyph {
        StateGlyph::Icon(icon) => {
            paint = move |ui: &mut egui::Ui, rect: egui::Rect, c: egui::Color32| {
                icon.image(rect.height(), c).paint_at(ui, rect);
            };
            WidgetGlyph::Paint(&paint)
        }
        StateGlyph::Spinner => WidgetGlyph::Spinner,
    };
    let view = StateScreenView {
        glyph,
        glyph_color,
        title: s.title,
        title_color,
        sub: s.sub,
        reason: s.reason,
        actions,
    };
    match state_screen(ui, theme, rect, &view)? {
        0 => Some(ExplorerAction::Refresh),
        _ => Some(ExplorerAction::GoUp),
    }
}

/// (글리프 색, 제목 색). 중립은 글리프 text-muted · 제목 text-secondary 다.
fn tone_colors(theme: &Theme, tone: Tone) -> (egui::Color32, egui::Color32) {
    match tone {
        Tone::Neutral => (
            theme.text_muted().to_egui(),
            theme.text_secondary().to_egui(),
        ),
        Tone::Warning => {
            let c = theme.accent_warning_raised().to_egui();
            (c, c)
        }
        Tone::Error => {
            let c = theme.explorer_error_fg().to_egui();
            (c, c)
        }
    }
}

/// 미리보기 패널 본문의 상태. 시안 `YPreview` 가 목록 상태 화면(`ExpState`)을 그대로 쓴다.
pub(super) enum PreviewState<'a> {
    Plain(Icon, &'a str),
    /// 글리프 · 제목 · 보조 줄 · 선택 이유 줄(mono, 번역하지 않은 값).
    Sub(Icon, &'a str, &'a str, Option<&'a str>),
    Loading(&'a str),
    /// 제목과 번역하지 않은 OS 이유 문구.
    Error(&'a str, &'a str),
}

/// 받은 영역 가운데에 미리보기 상태 블록을 그린다. 버튼은 없다.
pub(super) fn show_preview_state(ui: &mut egui::Ui, theme: &Theme, state: PreviewState<'_>) {
    let screen = match state {
        PreviewState::Plain(glyph, title) => StateScreen {
            glyph: StateGlyph::Icon(glyph),
            tone: Tone::Neutral,
            title,
            sub: None,
            reason: None,
            actions: None,
        },
        PreviewState::Sub(glyph, title, sub, reason) => StateScreen {
            glyph: StateGlyph::Icon(glyph),
            tone: Tone::Neutral,
            title,
            sub: Some(sub),
            reason,
            actions: None,
        },
        PreviewState::Loading(title) => StateScreen {
            glyph: StateGlyph::Spinner,
            tone: Tone::Neutral,
            title,
            sub: None,
            reason: None,
            actions: None,
        },
        PreviewState::Error(title, reason) => StateScreen {
            glyph: StateGlyph::Icon(icons::ALERT_TRIANGLE),
            tone: Tone::Error,
            title,
            sub: None,
            reason: Some(reason),
            actions: None,
        },
    };
    // 버튼이 없으므로 돌려받을 액션도 없다.
    show(ui, theme, &screen);
}
