//! Explorer 툴바 아래에 붙는 Find 바. 검색 아이콘이 있는 입력, Subfolders 체크박스(원격은 없음),
//! 오른쪽 상태 글자(mono caption), 닫기 버튼을 `explorer-search-bar-height` 높이 한 줄에 둔다.
//! 이름에서 찾은 부분을 칠하는 글자 배치도 여기 둔다. 본체와 갤러리가 같은 함수를 부른다.

use tasty_type_appearance::theme::Theme;

use crate::button::{Button, ButtonVariant};
use crate::control::ControlSize;
use crate::icon_button::IconButton;
use crate::input::Input;
use crate::spinner::Spinner;
use crate::toggle::checkbox;

/// 오른쪽 상태 자리.
pub enum ExplorerFindStatus<'a> {
    /// 개수 등 글자 하나.
    Text(&'a str),
    /// 하위 폴더 검색 중. Spinner · 글자 · Stop 버튼.
    Searching { text: &'a str, stop: &'a str },
    /// 끝난 검색에서 읽지 못한 폴더 수를 경고 색으로 덧붙인다. 툴팁에 그 폴더들을 적는다.
    Skipped {
        text: &'a str,
        skipped: &'a str,
        tooltip: &'a str,
    },
}

/// Find 바의 문구.
pub struct ExplorerFindLabels<'a> {
    pub placeholder: &'a str,
    pub subfolders: &'a str,
    pub close: &'a str,
}

/// Find 바에서 일어난 일.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplorerFindEvents {
    /// 입력이 바뀌었다.
    pub changed: bool,
    /// 입력에서 Esc. 호출자가 글자가 있으면 지우고 없으면 바를 닫는다.
    pub escape: bool,
    pub close: bool,
    pub stop: bool,
    /// Subfolders 체크박스를 눌렀다.
    pub subfolders_toggled: bool,
    /// 입력 칸에 포커스가 있다. 호출자가 이 동안 목록 타입어헤드를 끈다.
    pub field_focused: bool,
}

/// 처음 열 때 입력 칸에 포커스를 준다.
pub struct ExplorerFindBar<'a> {
    pub query: &'a mut String,
    /// `None` 이면 Subfolders 를 숨긴다(원격 explorer).
    pub subfolders: Option<&'a mut bool>,
    pub status: ExplorerFindStatus<'a>,
    pub labels: ExplorerFindLabels<'a>,
    pub focus: bool,
}

/// Find 바 높이.
pub fn explorer_find_bar_height(theme: &Theme) -> f32 {
    theme.explorer_search_bar_height().value()
}

/// 가용 폭 전체에 Find 바를 그린다.
pub fn explorer_find_bar(
    ui: &mut egui::Ui,
    theme: &Theme,
    bar: ExplorerFindBar<'_>,
) -> ExplorerFindEvents {
    let mut events = ExplorerFindEvents::default();
    let h = explorer_find_bar_height(theme);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, theme.bg_panel().to_egui());
    let bw = theme.border_width.value();
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - bw * 0.5,
        egui::Stroke::new(bw, theme.separator.to_egui_premultiplied()),
    );
    let gap = theme.spacing_sm.value();
    let inner = rect.shrink2(egui::Vec2::X * gap);
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    row.spacing_mut().item_spacing.x = gap;
    // 오른쪽부터 닫기 · 상태 · Subfolders 를 놓고 남은 폭을 입력에 준다.
    let close = IconButton::new()
        .size(ControlSize::Sm)
        .show(&mut row, theme, &|ui, r, c| {
            tasty_icons::CLOSE.image(r.height(), c).paint_at(ui, r)
        })
        .on_hover_text(bar.labels.close);
    events.close = close.clicked();
    status(&mut row, theme, &bar.status, &mut events);
    if let Some(sub) = bar.subfolders {
        events.subfolders_toggled = checkbox(
            &mut row,
            theme,
            sub,
            bar.labels.subfolders,
            true,
            crate::Ground::Panel,
        )
        .clicked();
    }
    let field_w = row.available_width();
    let resp = row
        .allocate_ui_with_layout(
            egui::vec2(field_w, theme.input_height().value()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                Input::new()
                    .placeholder(bar.labels.placeholder)
                    .icon(&|ui, r, c| tasty_icons::SEARCH.image(r.height(), c).paint_at(ui, r))
                    .width(field_w)
                    .show(ui, theme, bar.query)
            },
        )
        .inner;
    if bar.focus {
        resp.request_focus();
    }
    events.changed = resp.changed();
    events.field_focused = resp.has_focus();
    events.escape = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
    events
}

fn status(
    ui: &mut egui::Ui,
    theme: &Theme,
    status: &ExplorerFindStatus<'_>,
    events: &mut ExplorerFindEvents,
) {
    let font = crate::label_font(theme.font_size_caption.value());
    let muted = theme.text_muted().to_egui();
    // right_to_left 줄이라 화면 왼쪽에 올 것을 나중에 둔다.
    match status {
        ExplorerFindStatus::Text(text) => {
            ui.label(egui::RichText::new(*text).font(font).color(muted));
        }
        ExplorerFindStatus::Searching { text, stop } => {
            if Button::new(stop)
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Sm)
                .show(ui, theme)
                .clicked()
            {
                events.stop = true;
            }
            ui.label(egui::RichText::new(*text).font(font).color(muted));
            Spinner::new()
                .size(theme.icon_glyph_size_sm.value())
                .show(ui, theme);
        }
        ExplorerFindStatus::Skipped {
            text,
            skipped,
            tooltip,
        } => {
            ui.label(
                egui::RichText::new(*skipped)
                    .font(font.clone())
                    .color(theme.accent_warning_raised().to_egui()),
            )
            .on_hover_text(*tooltip);
            ui.label(egui::RichText::new("·").font(font.clone()).color(muted));
            ui.label(egui::RichText::new(*text).font(font).color(muted));
        }
    }
}

/// 이름 글자 배치. `query` 와 대소문자 없이 처음 맞는 부분을 `explorer-match-fg` 로 칠한다.
/// `selected` 행은 surface-active 위라 일치 구간을 `explorer-match-fg-active` 글자와 밑줄로 구분한다.
pub fn explorer_match_job(
    theme: &Theme,
    name: &str,
    query: &str,
    font: egui::FontId,
    color: egui::Color32,
    selected: bool,
) -> egui::text::LayoutJob {
    let match_fg = if selected {
        theme.explorer_match_fg_active()
    } else {
        theme.explorer_match_fg()
    }
    .to_egui();
    let match_underline = if selected {
        egui::Stroke::new(theme.autocomplete_match_underline_width().value(), match_fg)
    } else {
        egui::Stroke::NONE
    };
    let (name, range) = crate::display_text::highlighted(name, match_range(name, query));
    let mut job = egui::text::LayoutJob::default();
    let plain = |job: &mut egui::text::LayoutJob, s: &str, c: egui::Color32| {
        job.append(s, 0.0, egui::TextFormat::simple(font.clone(), c));
    };
    match range {
        Some(range) => {
            plain(&mut job, &name[..range.start], color);
            job.append(
                &name[range.clone()],
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: match_fg,
                    underline: match_underline,
                    ..Default::default()
                },
            );
            plain(&mut job, &name[range.end..], color);
        }
        None => plain(&mut job, &name, color),
    }
    job
}

/// `name` 에서 `query` 와 대소문자 없이 처음 맞는 바이트 범위. 빈 query 는 `None`.
pub fn match_range(name: &str, query: &str) -> Option<std::ops::Range<usize>> {
    if query.is_empty() {
        return None;
    }
    let lower: Vec<(usize, char)> = name
        .char_indices()
        .flat_map(|(i, c)| c.to_lowercase().map(move |l| (i, l)))
        .collect();
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.len() > lower.len() {
        return None;
    }
    (0..=lower.len() - needle.len()).find_map(|start| {
        let window = &lower[start..start + needle.len()];
        if window.iter().map(|(_, c)| *c).eq(needle.iter().copied()) {
            let begin = window[0].0;
            let last = window[needle.len() - 1].0;
            let end = last + name[last..].chars().next().map_or(0, char::len_utf8);
            Some(begin..end)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests;
