//! Properties 의 동작: Open footer(목록의 Enter 와 같은 동작)와 권한 편집 결과 연결.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tasty_ui_widgets::{Button, ButtonVariant, ControlSize};

use super::ExplorerProperties;
use super::perm_edit::{self, PermAccess, PermEdit, Phase};
use crate::adapters::ui::surface::explorer::view::ExplorerView;
use crate::app::local_reads::{self, FolderCount, ItemKind, LinkTarget};
use crate::i18n::t;
use crate::runtime::engine_read::EngineRead;
use crate::state::MainViewState;
use crate::theme::Theme;

/// footer 의 Open. 한 항목에만 있고 끊긴 링크면 끈다.
#[derive(Debug, Clone)]
pub(super) enum Footer {
    None,
    Open(crate::explorer_ui::ExplorerAction),
    Disabled,
}

/// 권한 변경을 explorer 파일 작업 대기열에 넣는다. 받지 않았으면(대기열이 가득 참 등) 상자는
/// 편집 상태로 남고 이유는 그 칸이 알린다.
pub(super) fn apply_perm(
    state: &mut MainViewState,
    engine: &EngineRead<'_>,
    props: &mut ExplorerProperties,
    surface_id: u32,
    path: PathBuf,
    change: crate::app::file_jobs::permissions::PermChange,
) {
    let request = state.request_explorer_file_numbered(
        engine,
        surface_id,
        crate::app::explorer_files::Operation::SetPermissions { path, change },
        crate::intent::IntentOrigin::User {
            source: crate::intent::UserSource::Menu("explorer_properties_permissions"),
        },
    );
    if let (Some(request), Some(edit)) = (request, props.perm.as_mut()) {
        edit.phase = Phase::Applying(request);
    }
}

/// 한 항목의 footer. 그 항목을 목록에서 Enter 로 여는 동작을 쓴다. 지금 보는 폴더 자신(빈 곳에서 연
/// Properties)은 Enter 로 열 항목이 아니므로 footer 가 없다. 여러 항목·읽기 실패·읽는 중도 없다.
pub(super) fn footer(props: &ExplorerProperties, view: &ExplorerView) -> Footer {
    let [path] = props.paths.as_slice() else {
        return Footer::None;
    };
    if view.shown_dir() == Some(path.as_path()) {
        return Footer::None;
    }
    if let Some(entries) = &props.remote {
        return match entries.as_slice() {
            [entry] => {
                Footer::Open(crate::adapters::ui::surface::explorer::view::keys::open_action(entry))
            }
            _ => Footer::None,
        };
    }
    let Some(Ok(facts)) = &props.facts else {
        return Footer::None;
    };
    let [item] = facts.items.as_slice() else {
        return Footer::None;
    };
    if let ItemKind::Link {
        state: LinkTarget::Missing,
        ..
    } = item.kind
    {
        return Footer::Disabled;
    }
    // 목록에 보이는 항목이면 Enter 가 쓰는 항목 정보를 그대로 쓴다.
    if let Some(entry) = view.listed_entry(path) {
        return Footer::Open(
            crate::adapters::ui::surface::explorer::view::keys::open_action(entry),
        );
    }
    let folder = match &item.kind {
        ItemKind::Folder => true,
        ItemKind::Link {
            state: LinkTarget::Readable { is_dir, .. },
            ..
        } => *is_dir,
        _ => false,
    };
    Footer::Open(if folder {
        crate::explorer_ui::ExplorerAction::Navigate(path.clone())
    } else {
        crate::explorer_ui::ExplorerAction::OpenFile(path.clone())
    })
}

/// 구분선 위 오른쪽 정렬 secondary sm "Open". 누르면 true.
pub(super) fn footer_row(ui: &mut egui::Ui, th: &Theme, footer: &Footer) -> bool {
    let rule = ui.cursor().top();
    let left = ui.min_rect().left();
    let right = left + ui.available_width();
    ui.painter().hline(
        left..=right,
        rule + th.border_width.value() / 2.0,
        egui::Stroke::new(
            th.border_width.value(),
            th.explorer_props_footer_border().to_egui_premultiplied(),
        ),
    );
    let pad_x = th.explorer_props_padding_x().value();
    let mut clicked = false;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: pad_x as i8,
            right: pad_x as i8,
            top: (th.spacing_sm.value() + th.border_width.value()) as i8,
            bottom: th.spacing_sm.value() as i8,
        })
        .show(ui, |ui| {
            // 팝업 높이가 아직 이전 내용 높이면 남는 높이가 생기므로 버튼 한 줄 높이로 묶는다.
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ControlSize::Sm.height(th)),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let enabled = matches!(footer, Footer::Open(_));
                    let resp = Button::new(t("explorer.props.open"))
                        .variant(ButtonVariant::Secondary)
                        .size(ControlSize::Sm)
                        .enabled(enabled)
                        .show(ui, th);
                    let resp = if enabled {
                        resp
                    } else {
                        resp.on_hover_text(t("explorer.props.target_missing"))
                    };
                    clicked = enabled && resp.clicked();
                },
            );
        });
    clicked
}

impl ExplorerProperties {
    /// 읽은 한 로컬 항목의 권한 편집 상자를 연다.
    pub(super) fn open_perm(&mut self) {
        if self.remote.is_some() {
            return;
        }
        if let Some(Ok(facts)) = &self.facts
            && let [item] = facts.items.as_slice()
            && perm_edit::access(item) == PermAccess::Editable
        {
            self.perm = PermEdit::open(item);
        }
    }

    /// 편집 상자가 기다리는 파일 작업 요청 번호.
    pub(crate) fn perm_request(&self) -> Option<u64> {
        match self.perm.as_ref()?.phase {
            Phase::Applying(id) => Some(id),
            _ => None,
        }
    }

    /// 권한 변경이 끝났다. 성공이면 상자를 닫고, 실패면 이유를 보인다. 둘 다 실제 값을 다시 읽는다.
    /// `result` 가 None 이면 요청이 시작되지 않고 버려졌으므로 편집 상태로 돌린다.
    pub(crate) fn settle_perm(&mut self, request: u64, result: Option<Result<(), String>>) {
        if self.perm_request() != Some(request) {
            return;
        }
        match result {
            Some(Ok(())) => self.perm = None,
            Some(Err(reason)) => {
                if let Some(edit) = self.perm.as_mut() {
                    edit.phase = Phase::Failed(reason);
                }
            }
            None => {
                if let Some(edit) = self.perm.as_mut() {
                    edit.phase = Phase::Editing;
                }
            }
        }
        self.reread_keeping_perm();
    }

    /// 열린 편집 상자와 지금 값을 둔 채 다시 읽는다. 결과가 오면 [`poll`] 이 값과 상자를 맞춘다.
    fn reread_keeping_perm(&mut self) {
        if self.remote.is_some() {
            return;
        }
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.count = Arc::new(FolderCount::default());
        self.query = Some(local_reads::properties(
            self.paths.clone(),
            self.count.clone(),
            self.cancel.clone(),
        ));
    }
}
