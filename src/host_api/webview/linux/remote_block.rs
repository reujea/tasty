//! 원격(`http(s)`) 차단의 실효값을 탐색 정책과 content filter에 적용한다.
//!
//! 실효값은 `전역 설정 || 응답 결정 단계에서 정한 문서 단위 허용` 하나다(docs/surfaces/html/index.md
//! "원격 콘텐츠 차단"). 전역 설정과 게이트의 문서 값은 이 한 곳에서만 native에 쓴다. 문서 값은 게이트가
//! 상태를 바꾸는 지점(로드 시작·응답 결정·commit·종료)마다 다시 읽는다. entry mirror는 늘 막고, HTTP view는
//! endpoint가 정하므로 막지 않는다.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use webkit2gtk::{UserContentManager, UserContentManagerExt};

use super::super::script_gate::ScriptGate;
use super::content_filter::{ContentFilter, REMOTE_BLOCK_FILTER_ID};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// 로컬 html: 전역 || 문서 허용.
    Gate,
    /// entry mirror: 늘 막는다.
    Entry,
    /// HTTP view: endpoint가 정한다.
    Http,
}

/// 한 webview의 원격 차단 상태. 탐색 정책 콜백·load 신호 콜백이 복제해 쓴다.
#[derive(Clone)]
pub(super) struct RemoteBlock {
    mode: Mode,
    ucm: Option<UserContentManager>,
    /// 비동기 컴파일 완료 전에는 필터가 없다.
    filter: Rc<RefCell<Option<ContentFilter>>>,
    /// 지금 막는지. 탐색 정책과 필터 컴파일 완료 콜백이 읽는다.
    block: Rc<Cell<bool>>,
    global_allowed: Rc<Cell<bool>>,
    document_allowed: Rc<Cell<bool>>,
}

impl RemoteBlock {
    pub(super) fn new(entry: bool, http: bool, ucm: Option<UserContentManager>) -> Self {
        let mode = if http {
            Mode::Http
        } else if entry {
            Mode::Entry
        } else {
            Mode::Gate
        };
        Self {
            mode,
            ucm,
            filter: Rc::new(RefCell::new(None)),
            block: Rc::new(Cell::new(mode != Mode::Http)),
            global_allowed: Rc::new(Cell::new(false)),
            document_allowed: Rc::new(Cell::new(false)),
        }
    }

    pub(super) fn block_cell(&self) -> &Rc<Cell<bool>> {
        &self.block
    }

    pub(super) fn filter_slot(&self) -> &Rc<RefCell<Option<ContentFilter>>> {
        &self.filter
    }

    pub(super) fn ucm(&self) -> Option<UserContentManager> {
        self.ucm.clone()
    }

    /// 전역 "Allow remote content"를 바꾼다.
    pub(super) fn set_global(&self, allowed: bool) {
        self.global_allowed.set(allowed);
        self.apply();
    }

    /// 게이트의 문서 단위 값을 다시 읽어 적용한다. 게이트 상태가 바뀐 지점마다 부른다.
    pub(super) fn sync(&self, gate: &ScriptGate) {
        self.document_allowed.set(gate.document_remote());
        self.apply();
    }

    fn apply(&self) {
        let block = match self.mode {
            Mode::Http => false,
            Mode::Entry => true,
            Mode::Gate => !(self.global_allowed.get() || self.document_allowed.get()),
        };
        if self.block.replace(block) == block {
            return;
        }
        tracing::debug!("Linux WebKitGTK remote content block={block}");
        self.apply_filter();
    }

    /// 중복 설치를 피하려고 기존 ID의 필터를 제거한 뒤 필요하면 다시 붙인다.
    /// 컴파일 중이면 완료 콜백이 그때의 상태를 확인한다.
    fn apply_filter(&self) {
        use gtk::glib::translate::ToGlibPtr;

        let Some(ucm) = &self.ucm else { return };
        ucm.remove_filter_by_id(REMOTE_BLOCK_FILTER_ID);
        if self.block.get()
            && let Some(filter) = self.filter.borrow().as_ref()
        {
            // SAFETY: 컴파일이 끝난 유효한 필터이고 매니저는 이 webview가 소유한다.
            unsafe {
                webkit2gtk::ffi::webkit_user_content_manager_add_filter(
                    ucm.to_glib_none().0,
                    filter.0,
                )
            };
        }
    }
}
