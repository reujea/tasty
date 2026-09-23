//! `PathField` — 주소창용 편집형 경로 필드(Explorer / Markdown 공용).
//!
//! 디자인 `PathField`(gallery `plugins.jsx`)의 소스 조립: [`crate::AutoComplete`] 트리거
//! (Input 언어 + 후보 드롭다운) + 우측 Go [`crate::IconButton`]. 두 surface 가 **같은 필드 +
//! 같은 후보 드롭다운 + 같은 편집/이동 계약**을 공유한다.
//!
//! 계약:
//! - **상태 호출측 소유**: `buffer`(편집 텍스트) / `editing`(포커스=편집모드) / `active`
//!   (keyboard-active 행)를 프레임마다 `&mut` 로 대여받아 갱신한다. 위젯은 글로벌 상태 없음.
//! - **아이콘 주입**: leading / Go 아이콘은 [`IconPainter`] 로 주입한다(위젯 내부 아이콘 상수
//!   금지 — 본체=glyph, 플러그인=baked 벡터를 흡수).
//! - **idle=secondary / editing=primary**: 비편집 시 mono 경로를 text-secondary 로 낮추고
//!   (AutoComplete `trigger_text_color`), 편집 진입 시 Input 기본 text-primary.
//! - **경로 문자열만 emit**: file vs directory 해석은 소비처 몫. 확정 시 [`PathFieldOutcome::Navigate`]
//!   가 경로 문자열만 담는다.
//! - **id_salt 필수**: host 다중 surface/tab 충돌 방지(각 필드 고유 id).
//!
//! 결정 로직([`decide`])은 순수 함수라 단위테스트로 격리한다.

use tasty_type_appearance::theme::Theme;

use crate::AutoCompleteAction;
use crate::control::ControlSize;
use crate::icon_button::IconPainter;

/// `PathField::show` 한 프레임의 확정 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathFieldOutcome {
    /// 확정 없음(편집 중 / 무입력).
    None,
    /// 경로 확정 — 이동해야 할 경로 문자열. active 후보 행 Enter/클릭이면 그 경로, 아니면
    /// 현재 버퍼. Go 클릭도 현재 버퍼를 확정한다.
    Navigate(String),
    /// 원복(Esc 또는 확정 없는 포커스 이탈) — 위젯이 `buffer` 를 `current_path` 로 되돌렸다.
    Revert,
}

/// 편집/이동 결정(내부). `decide` 가 산출한다.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Decision {
    None,
    /// 현재 버퍼로 이동(active 행 없이 Enter, 또는 Go 클릭).
    NavigateBuffer,
    /// 선택된 후보 경로로 이동(행 클릭 / Enter-on-active).
    NavigatePick(String),
    /// 원복(Esc, 또는 확정 없는 포커스 이탈).
    Revert,
}

/// AutoComplete 행위 + Go 클릭 + 포커스 이탈을 이동/원복/무동작으로 매핑한다.
///
/// 우선순위: **Esc(Cancel) > 행 확정(Pick) > 버퍼 확정(Submit) > Go 클릭 > 확정 없는
/// blur(원복) > None**. Go 클릭은 blur-원복보다 앞선다 — Go 클릭이 트리거 포커스를 뺏어
/// 같은 프레임에 `lost_focus` 를 유발하지만, 그건 이동 확정이지 취소가 아니기 때문이다.
fn decide(action: &AutoCompleteAction, lost_focus: bool, go_clicked: bool) -> Decision {
    match action {
        AutoCompleteAction::Cancel => Decision::Revert,
        AutoCompleteAction::Pick(path) => Decision::NavigatePick(path.clone()),
        AutoCompleteAction::Submit => Decision::NavigateBuffer,
        _ if go_clicked => Decision::NavigateBuffer,
        AutoCompleteAction::None | AutoCompleteAction::Edited if lost_focus => Decision::Revert,
        AutoCompleteAction::None | AutoCompleteAction::Edited => Decision::None,
    }
}

/// PathField 빌더. 프레젠테이션 설정만 담고, 상태(`buffer`/`editing`/`active`)는 `show` 인자.
pub struct PathField<'a> {
    id_salt: &'a str,
    placeholder: &'a str,
    empty_label: &'a str,
    /// 트리거·드롭다운 폭(Go 버튼 포함 총폭). `None` 이면 가용 폭.
    width: Option<f32>,
    /// 후보 필터 모드. 기본 `Substring`(디자인 typeahead).
    match_mode: crate::MatchMode,
    /// 매치 구간 highlight. 기본 `true`.
    highlight: bool,
    /// 드롭다운 최대 높이 override. `None` 이면 `theme.autocomplete_max_height()`.
    max_dropdown_height: Option<f32>,
    /// 트리거 leading 아이콘(per-surface: folderOpen / file).
    leading_icon: Option<IconPainter<'a>>,
    /// 후보 행 공통 leading 아이콘(미지정 시 `leading_icon` 재사용).
    row_icon: Option<IconPainter<'a>>,
    /// Go(arrow-right) 아이콘(per-surface baked/glyph 주입).
    go_icon: Option<IconPainter<'a>>,
    /// Go 버튼 hover tooltip(디자인 aria-label 대체). `None` 이면 tooltip 없음.
    go_tooltip: Option<&'a str>,
}

impl<'a> PathField<'a> {
    pub fn new(id_salt: &'a str) -> Self {
        Self {
            id_salt,
            placeholder: "",
            empty_label: "",
            width: None,
            match_mode: crate::MatchMode::Substring,
            highlight: true,
            max_dropdown_height: None,
            leading_icon: None,
            row_icon: None,
            go_icon: None,
            go_tooltip: None,
        }
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// 후보 0개일 때 드롭다운에 표시할 비상호작용 라벨(예: "No matching path").
    pub fn empty_label(mut self, empty_label: &'a str) -> Self {
        self.empty_label = empty_label;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// 후보 필터 모드 — 기본 `Substring`. 소비처가 v1(필터 없음)을 원하면 `None`.
    pub fn match_mode(mut self, match_mode: crate::MatchMode) -> Self {
        self.match_mode = match_mode;
        self
    }

    /// 매치 구간 highlight — 기본 `true`.
    pub fn highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    /// 드롭다운 최대 높이(logical px) override.
    pub fn max_dropdown_height(mut self, max_height: f32) -> Self {
        self.max_dropdown_height = Some(max_height);
        self
    }

    /// 트리거 leading 아이콘 painter(text-muted/icon-fg 색으로 호출됨).
    pub fn leading_icon(mut self, icon: IconPainter<'a>) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    /// 후보 행 공통 leading 아이콘 painter. 미지정 시 `leading_icon` 을 재사용한다.
    pub fn row_icon(mut self, row_icon: IconPainter<'a>) -> Self {
        self.row_icon = Some(row_icon);
        self
    }

    /// Go(arrow-right) 아이콘 painter.
    pub fn go_icon(mut self, go_icon: IconPainter<'a>) -> Self {
        self.go_icon = Some(go_icon);
        self
    }

    /// Go 버튼 hover tooltip(i18n 라벨). egui 엔 웹 aria 가 없어 tooltip 으로 노출한다.
    /// 미지정 시 tooltip 없음(기존 호출 무변경).
    pub fn go_tooltip(mut self, go_tooltip: &'a str) -> Self {
        self.go_tooltip = Some(go_tooltip);
        self
    }

    /// 트리거(AutoComplete) + Go 버튼을 한 행에 그리고 편집/이동/원복 결정을 반환한다.
    ///
    /// - `buffer`: 편집 버퍼(트리거 텍스트). 원복 시 위젯이 `current_path` 로 되돌린다.
    /// - `editing`: 편집모드(=트리거 포커스). 위젯이 매 프레임 갱신한다.
    /// - `active`: keyboard-active 행 index(필터된 가시 목록 기준). 닫히면 리셋된다.
    /// - `candidates`: 후보 경로(최신순). 빈 슬라이스면 드롭다운은 empty 행만.
    /// - `current_path`: 원복 대상(캐노니컬 현재 경로).
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        self,
        ui: &mut egui::Ui,
        theme: &Theme,
        buffer: &mut String,
        editing: &mut bool,
        active: &mut Option<usize>,
        candidates: &[&str],
        current_path: &str,
    ) -> PathFieldOutcome {
        let total_w = self.width.unwrap_or_else(|| ui.available_width());
        let gap = theme.spacing_sm.value();
        let go_side = ControlSize::Sm.height(theme);
        let field_w = (total_w - go_side - gap).max(0.0);

        // leading/row 아이콘 — row 는 미지정 시 leading 재사용(양 주소창 관례).
        let leading = self.leading_icon;
        let row = self.row_icon.or(self.leading_icon);

        let mut outcome = PathFieldOutcome::None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;

            // 트리거 = AutoComplete(mono 경로 + 후보 드롭다운). idle 은 secondary 로 낮춘다.
            let mut ac = crate::AutoComplete::new(self.id_salt)
                .mono(true)
                .match_mode(self.match_mode)
                .highlight(self.highlight)
                .placeholder(self.placeholder)
                .empty_label(self.empty_label)
                .width(field_w);
            if let Some(icon) = leading {
                ac = ac.icon(icon);
            }
            if let Some(icon) = row {
                ac = ac.row_icon(icon);
            }
            if let Some(mh) = self.max_dropdown_height {
                ac = ac.max_dropdown_height(mh);
            }
            if !*editing {
                ac = ac.trigger_text_color(theme.text_secondary().to_egui());
            }
            let out = ac.show(ui, theme, buffer, candidates, active);

            // 편집모드 = 트리거의 egui memory 포커스(단일 진실). `has_focus()` 는
            // viewport focused(RawInput.focused)에 게이트돼 있어 쓰지 않는다 — 편집모드는
            // 위젯 포커스 상태 머신이고, viewport focused 는 렌더 게이트(커서 깜빡임·
            // 드롭다운 표시)로만 작용해야 한다. has_focus 를 쓰면 focused=false 프레임
            // (plugin 재-paint / surface blur)마다 editing 이 false 로 떨어져 진입 재감지
            // 루프(주소창 진동)를 만든다. blur-원복은 memory 기반 lost_focus 가 담당.
            *editing = out.response.ctx.memory(|m| m.has_focus(out.response.id));

            // Go 버튼 — arrow-right IconButton(sm). 클릭 = 현재 버퍼 확정.
            // tooltip 은 버튼 response 에 붙인다(값 있을 때만 — 디자인 aria-label 대체).
            let go_clicked = if let Some(go) = self.go_icon {
                let mut resp = crate::IconButton::new()
                    .size(ControlSize::Sm)
                    .show(ui, theme, go);
                if let Some(tip) = self.go_tooltip {
                    resp = resp.on_hover_text(tip);
                }
                resp.clicked()
            } else {
                false
            };

            outcome = match decide(&out.action, out.response.lost_focus(), go_clicked) {
                Decision::None => PathFieldOutcome::None,
                Decision::NavigateBuffer => PathFieldOutcome::Navigate(buffer.clone()),
                Decision::NavigatePick(path) => {
                    *buffer = path.clone();
                    PathFieldOutcome::Navigate(path)
                }
                Decision::Revert => {
                    // Esc / 확정 없는 포커스 이탈 → 원래 경로 원복.
                    *buffer = current_path.to_string();
                    out.response.surrender_focus();
                    PathFieldOutcome::Revert
                }
            };
        });

        // 닫히면 keyboard-active 커서 리셋(다음 오픈은 active 없음부터).
        if !*editing {
            *active = None;
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_submit_navigates_buffer() {
        // active 행 없이 Enter → 현재 버퍼 이동.
        assert_eq!(
            decide(&AutoCompleteAction::Submit, true, false),
            Decision::NavigateBuffer
        );
    }

    #[test]
    fn decide_pick_navigates_row_path() {
        // active 행 Enter / 행 클릭 → 그 후보 경로 이동(확정 문자열 직접).
        assert_eq!(
            decide(
                &AutoCompleteAction::Pick("/docs/readme.md".to_string()),
                true,
                false
            ),
            Decision::NavigatePick("/docs/readme.md".to_string())
        );
    }

    #[test]
    fn decide_cancel_reverts() {
        // Esc → 원복(포커스 유지/이탈 무관).
        assert_eq!(
            decide(&AutoCompleteAction::Cancel, false, false),
            Decision::Revert
        );
        assert_eq!(
            decide(&AutoCompleteAction::Cancel, true, false),
            Decision::Revert
        );
        // Esc 는 Go 클릭보다도 우선.
        assert_eq!(
            decide(&AutoCompleteAction::Cancel, true, true),
            Decision::Revert
        );
    }

    #[test]
    fn decide_blur_without_confirm_reverts() {
        // 확정 없이 포커스만 잃으면 편집 취소 → 원복.
        assert_eq!(
            decide(&AutoCompleteAction::None, true, false),
            Decision::Revert
        );
        assert_eq!(
            decide(&AutoCompleteAction::Edited, true, false),
            Decision::Revert
        );
    }

    #[test]
    fn decide_typing_is_none() {
        // 포커스 유지 중 편집/무입력 → 무동작(드롭다운 열림 유지).
        assert_eq!(
            decide(&AutoCompleteAction::None, false, false),
            Decision::None
        );
        assert_eq!(
            decide(&AutoCompleteAction::Edited, false, false),
            Decision::None
        );
    }

    #[test]
    fn decide_go_click_navigates_buffer_over_blur_revert() {
        // Go 클릭은 같은 프레임 blur 를 유발하지만 이동 확정이다(원복 아님).
        assert_eq!(
            decide(&AutoCompleteAction::None, true, true),
            Decision::NavigateBuffer
        );
        assert_eq!(
            decide(&AutoCompleteAction::Edited, true, true),
            Decision::NavigateBuffer
        );
        // 행 확정(Pick)이 있으면 Go 보다 그 경로가 우선.
        assert_eq!(
            decide(&AutoCompleteAction::Pick("/a".to_string()), true, true),
            Decision::NavigatePick("/a".to_string())
        );
    }
}
