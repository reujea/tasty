//! 입력한 텍스트로 후보를 좁히는 자동 완성 필드.
//! Substring·Prefix는 대소문자를 무시하고 None은 모든 후보를 보여준다.
//! 후보는 가운데를 줄여 파일명 끝을 보존하며 높이를 넘으면 내부 스크롤을 사용한다.
//! 키보드 선택은 호버보다 우선한다. 확정하면 필터된 목록의 문자열을 직접 반환한다.
//! 방향키·Enter·Esc를 처리하지만 플러그인의 키 전달과 취소 후 버퍼 복원은 호출자가 맡는다.

use tasty_type_appearance::theme::Theme;

use crate::icon_button::IconPainter;
use crate::keyboard_cursor;

/// 후보 필터 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// 후보 라벨 어디든 질의가 부분 문자열로 포함(기본 — 경로친화).
    Substring,
    /// 후보 라벨이 질의로 시작.
    Prefix,
    /// 필터 없이 전체 후보를 보여준다.
    None,
}

/// AutoComplete 한 프레임의 사용자 행위.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoCompleteAction {
    /// 아무 일도 없음(키보드 커서 이동만 포함 — 그건 `active` 갱신으로 반영됨).
    None,
    /// 입력 버퍼 텍스트가 바뀜(질의 변경).
    Edited,
    /// 필터된 후보에서 확정한 문자열. 원본 인덱스로 다시 변환할 필요가 없다.
    Pick(String),
    /// active 행 없이 Enter — 현재 버퍼를 그대로 확정(navigate).
    Submit,
    /// Esc — 드롭다운 닫기 + 버퍼 원복(원복은 호출측 책임).
    Cancel,
}

/// 동작과 입력 필드 응답. 호출자는 응답의 포커스 상태를 읽거나 포커스를 해제할 수 있다.
pub struct AutoCompleteResponse {
    /// 이번 프레임의 사용자 행위.
    pub action: AutoCompleteAction,
    /// 트리거(편집 필드) 응답.
    pub response: egui::Response,
}

/// AutoComplete 빌더. 프레젠테이션 설정만 담고, 상태(`buf`/`entries`/`active`)는 `show` 인자.
pub struct AutoComplete<'a> {
    id_salt: &'a str,
    placeholder: &'a str,
    empty_label: &'a str,
    mono: bool,
    enabled: bool,
    /// 트리거·드롭다운 폭. `None` 이면 가용 폭.
    width: Option<f32>,
    /// 트리거 leading 아이콘(선택).
    icon: Option<IconPainter<'a>>,
    /// 후보 행 공통 leading 아이콘(선택 — 히스토리 dropdown 은 파일 아이콘).
    row_icon: Option<IconPainter<'a>>,
    /// 트리거 텍스트 색 override(선택). 미지정 시 `input_fg`(text-primary).
    trigger_text_color: Option<egui::Color32>,
    /// 필터 모드. 기본 `Substring`.
    match_mode: MatchMode,
    /// 매치 구간 highlight 여부. 기본 `true`.
    highlight: bool,
    /// 드롭다운 최대 높이 override. `None` 이면 `theme.autocomplete_max_height()`(220).
    max_dropdown_height: Option<f32>,
    /// 트리거의 포커스 테두리 유지(`Input::focus_look`).
    focus_look: bool,
    /// 트리거 뒤 칸(`Input::trailing`).
    trailing: Option<(IconPainter<'a>, f32)>,
    idle_text: Option<&'a str>,
}

impl<'a> AutoComplete<'a> {
    pub fn new(id_salt: &'a str) -> Self {
        Self {
            id_salt,
            placeholder: "",
            empty_label: "",
            mono: false,
            enabled: true,
            width: None,
            icon: None,
            row_icon: None,
            trigger_text_color: None,
            match_mode: MatchMode::Substring,
            highlight: true,
            max_dropdown_height: None,
            focus_look: false,
            trailing: None,
            idle_text: None,
        }
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub(crate) fn idle_text(mut self, text: Option<&'a str>) -> Self {
        self.idle_text = text;
        self
    }

    /// 후보 0개일 때 드롭다운에 표시할 비상호작용 라벨(예: "No matching path").
    pub fn empty_label(mut self, empty_label: &'a str) -> Self {
        self.empty_label = empty_label;
        self
    }

    /// 경로 변형 — 입력·후보 텍스트 monospace + caption(11).
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// 트리거 leading 아이콘 painter(text-muted 색으로 호출됨).
    pub fn icon(mut self, icon: IconPainter<'a>) -> Self {
        self.icon = Some(icon);
        self
    }

    /// 후보 행 공통 leading 아이콘 painter(text-muted 색으로 호출됨).
    pub fn row_icon(mut self, row_icon: IconPainter<'a>) -> Self {
        self.row_icon = Some(row_icon);
        self
    }

    /// 입력 글자색. 기본은 input_fg이며 다른 색도 Theme에서 가져와야 한다.
    pub fn trigger_text_color(mut self, color: egui::Color32) -> Self {
        self.trigger_text_color = Some(color);
        self
    }

    /// 필터 모드 — 기본 `Substring`. `None` 이면 필터 없이 전체 후보 노출.
    pub fn match_mode(mut self, match_mode: MatchMode) -> Self {
        self.match_mode = match_mode;
        self
    }

    /// 매치 구간 highlight — 기본 `true`. 필터를 쓰지 않는(=`None`) 용례는 꺼도 좋다.
    pub fn highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    /// 드롭다운 최대 높이(logical px) override. 미지정 시 `autocomplete_max_height`(220).
    pub fn max_dropdown_height(mut self, max_height: f32) -> Self {
        self.max_dropdown_height = Some(max_height);
        self
    }

    /// 포커스가 없어도 트리거에 포커스 테두리를 그린다.
    pub fn focus_look(mut self, focus_look: bool) -> Self {
        self.focus_look = focus_look;
        self
    }

    /// 트리거 뒤에 변 길이 `size` 인 칸을 두고 `paint` 로 그린다.
    pub fn trailing(mut self, paint: IconPainter<'a>, size: f32) -> Self {
        self.trailing = Some((paint, size));
        self
    }

    /// 입력 필드와 포커스 중의 후보 팝오버를 그린다.
    /// buf는 검색할 입력값, entries는 원본 후보, active는 필터된 목록의 키보드 인덱스다.
    /// 팝오버는 주변 레이아웃을 밀지 않는다.
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        buf: &mut String,
        entries: &[&str],
        active: &mut Option<usize>,
    ) -> AutoCompleteResponse {
        let width = self.width.unwrap_or_else(|| ui.available_width());

        let mut trigger = crate::Input::new()
            .idle_text(self.idle_text)
            .placeholder(self.placeholder)
            .mono(self.mono)
            .enabled(self.enabled)
            .focus_look(self.focus_look)
            .width(width);
        if let Some((paint, size)) = self.trailing {
            trigger = trigger.trailing(paint, size);
        }
        if let Some(icon) = self.icon {
            trigger = trigger.icon(icon);
        }
        if let Some(color) = self.trigger_text_color {
            trigger = trigger.text_color(color);
        }
        let resp = trigger.show(ui, theme, buf);

        let mut action = if resp.changed() {
            AutoCompleteAction::Edited
        } else {
            AutoCompleteAction::None
        };

        // Enter/Esc가 포커스를 해제하는 프레임에도 확정·취소를 처리해야 한다.
        let engaged = self.enabled && (resp.has_focus() || resp.lost_focus());
        if !engaged {
            return AutoCompleteResponse {
                action,
                response: resp,
            };
        }

        let area_id = ui.make_persistent_id(("tasty_autocomplete", self.id_salt));
        retain_focus_for_dropdown_press(&resp, area_id);
        let focused = resp.has_focus();
        let query = buf.clone();
        let filtered = filter_entries(entries, &query, self.match_mode);

        let (down, up, enter, esc) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let n = filtered.len();
        // active 는 필터된 가시 목록 기준. 필터가 좁혀지면 범위를 벗어날 수 있어 clamp.
        if let Some(i) = *active {
            *active = if n == 0 {
                Option::None
            } else {
                Some(i.min(n - 1))
            };
        }
        // 포커스가 있을 때만 공용 키보드 커서 규칙으로 이동한다. 비활성 후보는 없다.
        if focused {
            if down {
                *active = keyboard_cursor::step_active(*active, n, None, true);
            }
            if up {
                *active = keyboard_cursor::step_active(*active, n, None, false);
            }
        }

        let clicked = if focused {
            let origin = resp.rect.left_bottom() + egui::vec2(0.0, theme.spacing_xs.value());
            let max_height = self
                .max_dropdown_height
                .unwrap_or_else(|| theme.autocomplete_max_height().value());
            egui::Area::new(area_id)
                .order(egui::Order::Foreground)
                .fixed_pos(origin)
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    ui.set_width(width);
                    autocomplete_dropdown(
                        ui,
                        theme,
                        self.id_salt,
                        &filtered,
                        self.empty_label,
                        self.mono,
                        self.row_icon,
                        *active,
                        &query,
                        self.highlight,
                        max_height,
                    )
                })
                .inner
        } else {
            Option::None
        };

        // 행위 우선순위: Esc > Enter/click > Edited.
        if esc {
            action = AutoCompleteAction::Cancel;
        } else if enter {
            action = active
                .and_then(|i| filtered.get(i))
                .map(|s| AutoCompleteAction::Pick((*s).to_string()))
                .unwrap_or(AutoCompleteAction::Submit);
        } else if let Some(s) = clicked.and_then(|i| filtered.get(i)) {
            action = AutoCompleteAction::Pick((*s).to_string());
            resp.surrender_focus();
        }
        AutoCompleteResponse {
            action,
            response: resp,
        }
    }
}

/// egui는 입력 밖의 pointer-down에서 먼저 blur한다. 자기 후보 영역의 누름만
/// 입력 포커스로 되돌려 호출자의 원복과 팝오버 소실을 막는다. 외부 누름·Enter·Esc는 제외한다.
fn retain_focus_for_dropdown_press(response: &egui::Response, area_id: egui::Id) {
    let at = response.ctx.input(|i| {
        (i.pointer.primary_pressed()
            && !i.key_pressed(egui::Key::Enter)
            && !i.key_pressed(egui::Key::Escape))
        .then(|| i.pointer.interact_pos())
        .flatten()
    });
    let layer = egui::LayerId::new(egui::Order::Foreground, area_id);
    if at.is_some_and(|p| response.ctx.layer_id_at(p) == Some(layer)) {
        response.request_focus();
    }
}

/// 현재 위치에 후보 목록을 그린다. entries와 반환 인덱스는 필터된 목록 기준이다.
/// 키보드 선택이 호버보다 우선하며 max_height를 넘으면 내부 스크롤을 사용한다.
#[allow(clippy::too_many_arguments)]
pub fn autocomplete_dropdown(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    entries: &[&str],
    empty_label: &str,
    mono: bool,
    row_icon: Option<IconPainter<'_>>,
    active: Option<usize>,
    query: &str,
    highlight: bool,
    max_height: f32,
) -> Option<usize> {
    let shadow = theme.shadow_popover().to_egui();
    let mut clicked = None;
    egui::Frame::new()
        .fill(theme.surface_raised().to_egui())
        .stroke(egui::Stroke::new(
            theme.border_width.value(),
            theme.border_default().to_egui(),
        ))
        .corner_radius(theme.corner_radius.value())
        .shadow(shadow)
        .inner_margin(egui::Margin::same(theme.spacing_xs.value() as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if entries.is_empty() {
                empty_row(ui, theme, empty_label);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt(("tasty_autocomplete_list", id_salt))
                    .max_height(max_height)
                    .auto_shrink([true, true])
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        for (i, entry) in entries.iter().enumerate() {
                            if candidate_row(
                                ui,
                                theme,
                                entry,
                                mono,
                                row_icon,
                                active == Some(i),
                                query,
                                highlight,
                            )
                            .clicked()
                            {
                                clicked = Some(i);
                            }
                        }
                    });
            }
        });
    clicked
}

/// 후보 한 행을 그리고 클릭 응답을 반환한다.
#[allow(clippy::too_many_arguments)]
fn candidate_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    path: &str,
    mono: bool,
    row_icon: Option<IconPainter<'_>>,
    keyboard_active: bool,
    query: &str,
    highlight: bool,
) -> egui::Response {
    let height = theme.menu_item_height().value();
    let pad_x = theme.menu_item_padding_x().value();
    let gap = theme.spacing_sm.value();
    let radius = theme.menu_item_radius().value();
    let icon_glyph = theme.icon_glyph_size_md.value();
    let font = row_font(theme, mono);
    let width = ui.available_width();

    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    if keyboard_active {
        ui.painter()
            .rect_filled(rect, radius, theme.surface_active().to_egui());
    } else if resp.hovered() {
        ui.painter().rect_filled(
            rect,
            radius,
            theme.menu_item_bg_hover().to_egui_premultiplied(),
        );
    }

    let highlighted = keyboard_active || resp.hovered();
    let fg = if highlighted {
        theme.autocomplete_row_fg_active().to_egui()
    } else {
        theme.text_secondary().to_egui()
    };
    // 키보드 활성 행의 아이콘과 일치 구간은 surface-active 위 글자색이다.
    let (icon_fg, match_fg) = if keyboard_active {
        let part = theme.autocomplete_part_fg_active().to_egui();
        (part, part)
    } else {
        (
            theme.text_muted().to_egui(),
            theme.accent_primary().to_egui(),
        )
    };

    let mut x = rect.left() + pad_x;
    if let Some(paint) = row_icon {
        let irect = egui::Rect::from_center_size(
            egui::pos2(x + icon_glyph * 0.5, rect.center().y),
            egui::vec2(icon_glyph, icon_glyph),
        );
        paint(ui, irect, icon_fg);
        x += icon_glyph + gap;
    }

    let avail = (rect.right() - pad_x - x).max(0.0);
    let painter = ui.painter();
    let shown = elide_middle(path, avail, |s| {
        painter
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .rect
            .width()
    });

    // 말줄임 후 표시 문자열에서 첫 번째 일치 구간을 강조한다.
    let run = if highlight {
        match_run(&shown, query)
    } else {
        None
    };
    let center_y = rect.center().y;
    match run {
        Some((s, e)) => {
            let chars: Vec<char> = shown.chars().collect();
            let pre: String = chars[..s].iter().collect();
            let mid: String = chars[s..e].iter().collect();
            let post: String = chars[e..].iter().collect();
            for (seg, color) in [(pre, fg), (mid, match_fg), (post, fg)] {
                if seg.is_empty() {
                    continue;
                }
                let g = painter.layout_no_wrap(seg, font.clone(), egui::Color32::PLACEHOLDER);
                let pos = egui::pos2(x, center_y - g.rect.height() * 0.5);
                let w = g.rect.width();
                painter.galley(pos, g, color);
                x += w;
            }
        }
        None => {
            let g = painter.layout_no_wrap(shown, font, egui::Color32::PLACEHOLDER);
            let pos = egui::pos2(x, center_y - g.rect.height() * 0.5);
            painter.galley(pos, g, fg);
        }
    }

    resp
}

/// empty 행 — 비상호작용 muted 라벨(hover/선택 chrome 없음).
fn empty_row(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    let height = theme.menu_item_height().value();
    let pad_x = theme.menu_item_padding_x().value();
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let g = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(theme.font_size_body.value()),
        egui::Color32::PLACEHOLDER,
    );
    let pos = egui::pos2(rect.left() + pad_x, rect.center().y - g.rect.height() * 0.5);
    ui.painter().galley(pos, g, theme.text_muted().to_egui());
}

fn row_font(theme: &Theme, mono: bool) -> egui::FontId {
    if mono {
        egui::FontId::monospace(theme.font_size_caption.value())
    } else {
        egui::FontId::proportional(theme.font_size_body.value())
    }
}

/// 후보를 질의로 좁힌다(대소문자 무시). 질의가 비었거나 `None` 모드면 전체를 반환.
/// 순서는 원본 순서 보존. 반환은 **가시 목록**이며 `Pick` 은 이 목록의 문자열을 돌려준다.
fn filter_entries<'a>(entries: &[&'a str], query: &str, mode: MatchMode) -> Vec<&'a str> {
    if query.is_empty() || matches!(mode, MatchMode::None) {
        return entries.to_vec();
    }
    let q = query.to_lowercase();
    entries
        .iter()
        .copied()
        .filter(|e| {
            let l = e.to_lowercase();
            match mode {
                MatchMode::Prefix => l.starts_with(&q),
                MatchMode::Substring => l.contains(&q),
                MatchMode::None => true,
            }
        })
        .collect()
}

/// `text` 안에서 `query` 의 첫 대소문자 무시 매치 구간을 **char 인덱스** `(start, end)` 로
/// 반환. 질의가 비었거나 매치가 없으면 `None`. char 단위 비교라 유니코드 경계도 안전.
fn match_run(text: &str, query: &str) -> Option<(usize, usize)> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() {
        return None;
    }
    let t: Vec<char> = text.chars().collect();
    if q.len() > t.len() {
        return None;
    }
    for start in 0..=(t.len() - q.len()) {
        if (0..q.len()).all(|k| char_eq_ci(t[start + k], q[k])) {
            return Some((start, start + q.len()));
        }
    }
    None
}

/// 두 char 를 대소문자 무시 비교. ASCII 밖(유니코드)도 `to_lowercase` 폴딩으로 처리.
fn char_eq_ci(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// `max_w` 폭에 맞도록 문자열 가운데를 `…` 로 접는다(머리/꼬리 보존, 꼬리 우선).
/// `measure` 는 문자열 폭(px). 전체가 들어가면 원문 그대로.
fn elide_middle(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n <= 1 {
        return text.to_string();
    }
    // keep = 유지할 원문 문자 수(가운데 `…` 제외). 꼬리에 여분을 줘 파일명을 더 보존.
    for keep in (1..n).rev() {
        let head = keep / 2;
        let tail = keep - head;
        let mut s = String::with_capacity(keep + 3);
        s.extend(&chars[..head]);
        s.push('…');
        s.extend(&chars[n - tail..]);
        if measure(&s) <= max_w {
            return s;
        }
    }
    "…".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elide_middle_keeps_head_and_tail() {
        // 각 문자 폭 1 로 측정.
        let measure = |s: &str| s.chars().count() as f32;
        // 충분히 넓으면 원문.
        assert_eq!(elide_middle("abcdef", 10.0, measure), "abcdef");
        // 좁으면 가운데 `…`, 머리·꼬리 보존(꼬리 우선).
        let out = elide_middle("/home/user/notes/readme.md", 9.0, measure);
        assert!(out.contains('…'), "must contain ellipsis: {out}");
        assert!(measure(&out) <= 9.0, "must fit: {out} ({})", measure(&out));
        assert!(out.starts_with('/'), "head preserved: {out}");
        assert!(out.ends_with("md"), "tail (filename) preserved: {out}");
    }

    #[test]
    fn elide_middle_degenerate() {
        let measure = |s: &str| s.chars().count() as f32;
        // 0 폭이라도 최소 `…`.
        assert_eq!(elide_middle("abcdef", 0.0, measure), "…");
        // 단일 문자는 접지 않음.
        assert_eq!(elide_middle("x", 0.0, measure), "x");
    }

    #[test]
    fn filter_substring_prefix_none() {
        let items = [
            "~/Downloads",
            "~/work/tasty",
            "~/work/tasty-ui/src",
            "~/.config/tasty",
        ];
        // substring: 어디든 포함.
        assert_eq!(
            filter_entries(&items, "tasty", MatchMode::Substring),
            vec!["~/work/tasty", "~/work/tasty-ui/src", "~/.config/tasty"]
        );
        // prefix: 시작만.
        assert_eq!(
            filter_entries(&items, "~/work", MatchMode::Prefix),
            vec!["~/work/tasty", "~/work/tasty-ui/src"]
        );
        // none: 항상 전체.
        assert_eq!(
            filter_entries(&items, "tasty", MatchMode::None),
            items.to_vec()
        );
        // 빈 질의: 전체.
        assert_eq!(
            filter_entries(&items, "", MatchMode::Substring),
            items.to_vec()
        );
        // 대소문자 무시.
        assert_eq!(
            filter_entries(&items, "DOWN", MatchMode::Substring),
            vec!["~/Downloads"]
        );
    }

    #[test]
    fn pick_returns_visible_string_not_original_index() {
        // 필터된 인덱스와 원본 인덱스가 달라도 선택 문자열을 그대로 반환해야 한다.
        let items = ["alpha", "beta", "gamma", "delta"];
        let filtered = filter_entries(&items, "l", MatchMode::Substring);
        // "l" 포함: alpha, delta (beta·gamma 는 제외).
        assert_eq!(filtered, vec!["alpha", "delta"]);
        // keyboard-active = 가시 목록의 1번(delta) — 원본에선 3번.
        let active = 1usize;
        let picked = filtered[active].to_string();
        assert_eq!(picked, "delta");
        assert_ne!(picked, items[active]);
    }

    #[test]
    fn match_run_case_insensitive_first_run() {
        // 기본 매치 — char 인덱스. "readme.md" 의 "me" 는 4..6.
        assert_eq!(match_run("readme.md", "me"), Some((4, 6)));
        // 대소문자 무시.
        assert_eq!(match_run("README.md", "me"), Some((4, 6)));
        // 첫 매치만.
        assert_eq!(match_run("ababab", "ab"), Some((0, 2)));
        // 없음 / 빈 질의.
        assert_eq!(match_run("readme", "zzz"), None);
        assert_eq!(match_run("readme", ""), None);
        // 질의가 더 길면 없음.
        assert_eq!(match_run("ab", "abc"), None);
    }

    #[test]
    fn match_run_unicode_safe() {
        // 멀티바이트 경계에서도 char 인덱스로 안전.
        assert_eq!(match_run("경로/readme", "readme"), Some((3, 9)));
    }
}
