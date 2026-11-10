//! 터미널 hover 링크의 우클릭 메뉴: 선택, 복사, 연결 동작.
//! 드래그 선택이 아닌 LinkSpan을 사용하며 포커스를 옮기지 않는다.
//! 명세: docs/features/terminal-link/index.md.

use crate::runtime::engine_read::EngineRead;
use winit::event::ElementState;

use super::MainView;
use crate::selection::{SelectionMode, SelectionPoint, TextSelection};
use crate::state::{PendingNativeMenu, TerminalLinkMenu};
use crate::terminal_link::LinkSegment;
use crate::view::ui::View;
use tasty_type_geometry::length::PhysicalPx;

/// 메뉴 항목 id.
const ITEM_SELECT: u32 = 1;
const ITEM_COPY: u32 = 2;
const ITEM_OPEN_WITH: u32 = 3;

impl MainView {
    /// 우클릭한 surface의 hover 링크를 복사해 메뉴 대상으로 보관한다.
    /// 우클릭 뒤 항목을 명시적으로 고르므로 LinkModifier::None도 허용한다.
    /// 메뉴를 열어도 터미널 포커스는 옮기지 않는다.
    pub(super) fn terminal_link_menu_target(
        &self,
        engine: &EngineRead<'_>,
        surface_id: u32,
    ) -> Option<TerminalLinkMenu> {
        // Keep the modifier/overlay eligibility established by hover handling,
        // but resolve the target again at the click content cut.
        self.hovered_link.as_ref()?;
        let fresh = self.compute_hovered_link(engine);
        let hovered = fresh.as_ref();
        if !link_menu_gate(
            hovered.map(|h| h.surface_id),
            surface_id,
            engine.live.occupancy.is_hard_occupied(surface_id),
        ) {
            return None;
        }
        let hovered = hovered?;
        let (start, end, text) = hovered_link_text(engine, hovered)?;
        // 자식 PTY 가 없는 terminal 은 원격 attach mirror 다 — 화면 경로가 원격 호스트 경로.
        let is_mirror =
            engine.terminals.contains(surface_id) && !engine.terminals.has_pty(surface_id);
        let (open_with, remote_path) = link_open_target(&hovered.uri, is_mirror);
        Some(TerminalLinkMenu {
            surface_id,
            start,
            end,
            text,
            open_with,
            remote_path,
        })
    }

    /// copy_link 단축키: 마우스 포인터 아래 링크를 복사한다. 링크 클릭 수식키는 요구하지 않는다.
    /// 메뉴·팝업·배너가 포인터를 덮고 있거나 링크가 없으면 false를 반환해 키를 다음 경로로 넘긴다.
    /// 키보드에서 실행했으므로 toast에 단축키 hint를 붙이지 않는다.
    pub(crate) fn run_copy_link(&mut self, engine: &EngineRead<'_>) -> bool {
        if self.mouse_overlay_open() || self.state.popup_hovered || self.state.banner_hovered {
            return false;
        }
        let Some(hovered) = self.compute_hovered_link(engine) else {
            return false;
        };
        let Some((_, _, text)) = hovered_link_text(engine, &hovered) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }
        self.copy_text(
            &text,
            false,
            "toast.copied",
            Vec::new(),
            crate::adapters::ui::ToastScope::Surface(hovered.surface_id),
        );
        self.mark_dirty();
        true
    }

    /// 링크 메뉴에 사용한 press와 release를 모두 로컬에서 소비한다.
    /// 한쪽만 앱에 보내면 짝이 없는 버튼 이벤트가 된다.
    pub(super) fn queue_terminal_link_menu(
        &mut self,
        link: TerminalLinkMenu,
        button_state: ElementState,
        x: f32,
        y: f32,
    ) {
        if button_state == super::mouse::terminal_menu_open_state() {
            let sf = self.base.gpu.scale_factor();
            self.state.dialogs.pending_native_menu = Some(PendingNativeMenu::TerminalLink {
                link,
                // 네이티브 메뉴 좌표는 logical — 물리 마우스 좌표를 변환 API 로 내린다.
                x: PhysicalPx(x).to_logical(sf).value(),
                y: PhysicalPx(y).to_logical(sf).value(),
            });
        }
        self.mark_dirty();
    }

    pub(super) fn handle_terminal_link_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        link: TerminalLinkMenu,
        x: f32,
        y: f32,
    ) {
        use crate::platform::native_menu::MenuItem;
        let items: Vec<MenuItem> = link_menu_items(link.open_with.is_some())
            .into_iter()
            .map(|(id, key)| MenuItem::new(id, crate::i18n::t(key)))
            .collect();
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // Linux 는 continuation 이 여러 프레임 뒤다 — 그 사이 surface 가 닫혔을 수 있다.
            if !engine.has_surface(link.surface_id) {
                return;
            }
            match result {
                Some(ITEM_SELECT) => this.apply_link_selection(engine, &link),
                Some(ITEM_COPY) => {
                    if link.text.is_empty() {
                        return;
                    }
                    this.copy_text(
                        &link.text,
                        false,
                        "toast.copied",
                        crate::adapters::ui::toast::binding_hint(engine.settings, "copy_link"),
                        crate::adapters::ui::ToastScope::Surface(link.surface_id),
                    );
                }
                Some(ITEM_OPEN_WITH) => this.open_link_handler_picker(engine, &link),
                _ => {}
            }
        });
    }

    /// 메뉴를 연 뒤에도 같은 범위의 문자가 같을 때만 링크를 선택한다.
    /// 출력·스크롤백 정리·크기 변경으로 내용이 달라졌으면 선택하지 않는다.
    fn apply_link_selection(&mut self, engine: &EngineRead<'_>, link: &TerminalLinkMenu) {
        let sel = link_selection(link.surface_id, link.start, link.end);
        let Some(terminal) = engine.as_ref().visible_terminal(link.surface_id) else {
            return;
        };
        if crate::selection::extract_selected_text(terminal, &sel) != link.text {
            return;
        }
        self.text_selection = Some(sel);
        self.mark_dirty();
    }

    /// 자동 실행이나 detector 조회 없이 전체 핸들러 선택창을 연다.
    /// 원격 경로에는 원격에 열 수 있는 핸들러만 표시한다.
    fn open_link_handler_picker(
        &mut self,
        engine: &crate::runtime::engine_read::EngineRead<'_>,
        link: &TerminalLinkMenu,
    ) {
        let Some(target) = link.open_with.clone() else {
            return;
        };
        match target {
            crate::file::dispatch::DispatchTarget::File(file) if link.remote_path => {
                crate::file::dispatch::remote::open_remote_link_picker(
                    &mut self.state,
                    engine,
                    file,
                    link.surface_id,
                );
            }
            target => {
                let all = engine.file_handler.all_handlers();
                crate::file::dispatch::open_picker_from_read(
                    &mut self.state,
                    engine,
                    target,
                    None,
                    all,
                    true,
                    crate::file::dispatch::FileDispatchPolicy {
                        dispatch_origin: crate::file::dispatch::FileDispatchOrigin::User,
                        ignore_size_limit: false,
                        run: None,
                    },
                );
            }
        }
        self.mark_dirty();
    }
}

/// mirror surface 의 호스트 글자. 연결 정보가 아직 없으면 빈 글자(화면은 `Remote` 낱말)다.
pub(super) fn remote_host_text(
    engine: &crate::runtime::engine_read::EngineRead<'_>,
    surface_id: u32,
) -> String {
    engine
        .mirror_location_of(surface_id)
        .map(|location| location.host.clone())
        .unwrap_or_default()
}

/// hover 링크의 선택 양끝과 화면 문자열. 화면 내용이 hover 계산 뒤 바뀌었으면 None이다.
fn hovered_link_text(
    engine: &EngineRead<'_>,
    hovered: &super::HoveredLink,
) -> Option<(SelectionPoint, SelectionPoint, String)> {
    let surface_id = hovered.surface_id;
    let (start, end) = link_selection_bounds(&hovered.highlight.segments, hovered.highlight.epoch)?;
    let terminal = engine.visible_terminal(surface_id)?;
    let text = terminal.with_content(|view| {
        if view.cut().epoch != hovered.highlight.epoch || view.cut().revision != hovered.revision {
            return None;
        }
        Some(crate::selection::extract_selected_text_from_view(
            &view,
            &link_selection(surface_id, start, end),
        ))
    })?;
    Some((start, end, text))
}

/// hover 링크가 우클릭한 surface에 속하고 hard 점유 mirror가 아닐 때 허용한다.
/// hard 점유 화면은 지연된 스냅샷이므로 실제 PTY 상태와 다를 수 있다(docs/dev-guide/attach-behavior.md).
fn link_menu_gate(hovered_surface: Option<u32>, clicked_surface: u32, hard_occupied: bool) -> bool {
    hovered_surface == Some(clicked_surface) && !hard_occupied
}

/// 링크 세그먼트(화면 순서, `end_col` 포함)를 selection 양끝으로. 선택 범위 판정도 양끝
/// 포함이라 그대로 맞물린다. 소프트 wrap 링크는 첫 세그먼트 시작 ~ 마지막 세그먼트 끝.
fn link_selection_bounds(
    segments: &[LinkSegment],
    epoch: tasty_terminal::ContentEpoch,
) -> Option<(SelectionPoint, SelectionPoint)> {
    let first = segments.first()?;
    let last = segments.last()?;
    Some((
        SelectionPoint {
            epoch,
            col: first.start_col,
            absolute_row: first.absolute_row,
        },
        SelectionPoint {
            epoch,
            col: last.end_col,
            absolute_row: last.absolute_row,
        },
    ))
}

/// 드래그로 만든 선택과 구분되지 않는 selection.
fn link_selection(surface_id: u32, start: SelectionPoint, end: SelectionPoint) -> TextSelection {
    TextSelection {
        anchor: start,
        cursor: end,
        mode: SelectionMode::Normal,
        surface_id,
        dragging: false,
    }
}

/// 링크 URI → ("연결 동작" 대상, 원격 경로 여부). 경로 링크는 파일 대상, `http(s)` 는 URL
/// 대상, 그 밖의 scheme(mailto 등)은 열 핸들러가 없어 항목을 노출하지 않는다. URL 은
/// mirror 여부와 무관하다 — 원격 호스트 경로가 아니다.
fn link_open_target(
    uri: &str,
    is_mirror: bool,
) -> (Option<crate::file::dispatch::DispatchTarget>, bool) {
    use crate::file::dispatch::{DispatchTarget, LinkKind, parse_link};
    match parse_link(uri) {
        LinkKind::FileTarget(path) => (
            Some(DispatchTarget::File(crate::file::format::FileTarget::new(
                path,
            ))),
            is_mirror,
        ),
        LinkKind::External(uri) => (DispatchTarget::http_url(&uri), false),
    }
}

/// 메뉴 항목 (id, i18n 키). 기존 터미널 메뉴 항목은 싣지 않는다 — 링크 위 우클릭은 링크
/// 전용 메뉴로 대체된다.
fn link_menu_items(has_open_with: bool) -> Vec<(u32, &'static str)> {
    let mut items = vec![
        (ITEM_SELECT, "terminal_context_menu.link_select"),
        (ITEM_COPY, "terminal_context_menu.link_copy"),
    ];
    if has_open_with {
        items.push((ITEM_OPEN_WITH, "terminal_context_menu.link_open_with"));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::dispatch::DispatchTarget;

    fn seg(absolute_row: usize, start_col: usize, end_col: usize) -> LinkSegment {
        LinkSegment {
            absolute_row,
            start_col,
            end_col,
        }
    }

    fn pt(col: usize, absolute_row: usize) -> SelectionPoint {
        SelectionPoint {
            epoch: Default::default(),
            col,
            absolute_row,
        }
    }

    /// 좌클릭과 대칭 — 이 게이트는 tracking 모드를 보지 않는다. `handle_right_button` 은
    /// 이 판정을 `right_click_delegates_to_app` 보다 먼저 부른다.
    #[test]
    fn link_menu_wins_over_tracking_delegation() {
        assert!(link_menu_gate(Some(7), 7, false));
    }

    #[test]
    fn link_menu_skipped_when_hover_surface_differs() {
        assert!(!link_menu_gate(Some(8), 7, false));
        assert!(!link_menu_gate(None, 7, false));
    }

    #[test]
    fn link_menu_skipped_on_hard_occupied_mirror() {
        assert!(!link_menu_gate(Some(7), 7, true));
    }

    #[test]
    fn selection_from_single_segment_link_covers_exact_columns() {
        assert_eq!(
            link_selection_bounds(&[seg(5, 3, 20)], Default::default()),
            Some((pt(3, 5), pt(20, 5)))
        );
    }

    #[test]
    fn selection_from_wrapped_link_spans_first_to_last_segment() {
        assert_eq!(
            link_selection_bounds(&[seg(5, 70, 79), seg(6, 0, 12)], Default::default()),
            Some((pt(70, 5), pt(12, 6)))
        );
        assert_eq!(link_selection_bounds(&[], Default::default()), None);
    }

    #[test]
    fn menu_items_hide_open_with_when_no_target() {
        let ids = |v: Vec<(u32, &str)>| v.into_iter().map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(
            ids(link_menu_items(true)),
            vec![ITEM_SELECT, ITEM_COPY, ITEM_OPEN_WITH]
        );
        assert_eq!(ids(link_menu_items(false)), vec![ITEM_SELECT, ITEM_COPY]);
    }

    #[test]
    fn open_target_by_link_kind() {
        assert_eq!(
            link_open_target("https://example.com/page", false),
            (
                Some(DispatchTarget::Url("https://example.com/page".into())),
                false
            )
        );
        // URL 은 mirror 여도 원격 경로가 아니다.
        assert!(!link_open_target("https://example.com/page", true).1);
        assert_eq!(link_open_target("mailto:a@b.com", false), (None, false));
        let (target, remote) = link_open_target("file:///home/u/a.md", true);
        assert_eq!(
            target,
            Some(DispatchTarget::File(crate::file::format::FileTarget::new(
                "/home/u/a.md"
            )))
        );
        assert!(remote);
        assert!(!link_open_target("file:///home/u/a.md", false).1);
    }
}
