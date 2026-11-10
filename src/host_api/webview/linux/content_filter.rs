//! Local WebView remote-content filter compilation and lifetime.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use webkit2gtk::UserContentManager;

/// GObject가 아닌 ref-counted filter의 소유권. 안전 바인딩이 없어 Drop에서 직접 unref한다.
pub(super) struct ContentFilter(pub(super) *mut webkit2gtk::ffi::WebKitUserContentFilter);

impl Drop for ContentFilter {
    fn drop(&mut self) {
        // SAFETY: 이 포인터는 `webkit_user_content_filter_store_save_finish` 가 준
        // full ref 이고 이 타입만이 소유한다. Drop 은 한 번만 돈다.
        unsafe { webkit2gtk::ffi::webkit_user_content_filter_unref(self.0) };
    }
}

/// http(s) 리소스를 막는 WebKit content-blocker 규칙. main frame 문서 요청은 빼고 탐색 정책이 정한다
/// (`remote_block`). 서브프레임 문서는 `child-frame`이라 첫 규칙에 남는다.
const REMOTE_BLOCK_RULES: &str = r#"[{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}},{"trigger":{"url-filter":"^https?://","resource-type":["document"],"load-context":["top-frame"]},"action":{"type":"ignore-previous-rules"}}]"#;

pub(super) const REMOTE_BLOCK_FILTER_ID: &str = "tasty-block-remote";

/// 비동기 콜백까지 Box로 보관하고 콜백이 소유권을 회수한다.
struct FilterSaveState {
    store: *mut webkit2gtk::ffi::WebKitUserContentFilterStore,
    ucm: UserContentManager,
    block_remote: Rc<Cell<bool>>,
    slot: Rc<RefCell<Option<ContentFilter>>>,
}

fn content_filter_store_dir() -> Option<String> {
    let Some(dir) = tasty_utils::path::tasty_home().map(|h| h.join("webkit-content-filters"))
    else {
        tracing::warn!("tasty 홈을 못 찾음 — 원격 서브리소스 차단이 안 걸린다");
        return None;
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(
            ?dir,
            "content filter 저장소 디렉토리 생성 실패: {e} — 차단이 안 걸린다"
        );
        return None;
    }
    Some(dir.to_string_lossy().into_owned())
}

/// 디스크 필터 저장소에 비동기 컴파일한다. 완료 전이나 실패 시 서브리소스 차단은 적용되지 않는다.
pub(super) fn compile_remote_block_filter(
    ucm: Option<UserContentManager>,
    block_remote: Rc<Cell<bool>>,
    slot: Rc<RefCell<Option<ContentFilter>>>,
) {
    use gtk::glib::translate::ToGlibPtr;

    let Some(ucm) = ucm else {
        tracing::warn!("WebKitGTK UserContentManager 없음 — 원격 서브리소스 차단이 안 걸린다");
        return;
    };
    let Some(dir_str) = content_filter_store_dir() else {
        return;
    };

    // SAFETY: 아래 세 포인터는 모두 이 블록이 소유한 값에서 나온다 —
    // `dir_str`/`REMOTE_BLOCK_FILTER_ID`/`rules` 의 stash 는 호출이 끝날 때까지 살아
    // 있고(변수로 묶어 둔다), 비동기 호출이 계속 필요로 하는 것(store)은 상태
    // 상자에 넣어 콜백이 되찾는다. 콜백은 GTK main loop 에서 돈다(= 이 스레드).
    unsafe {
        let store = webkit2gtk::ffi::webkit_user_content_filter_store_new(dir_str.to_glib_none().0);
        let state = Box::new(FilterSaveState {
            store,
            ucm,
            block_remote,
            slot,
        });
        let rules = gtk::glib::Bytes::from_static(REMOTE_BLOCK_RULES.as_bytes());
        let id = REMOTE_BLOCK_FILTER_ID.to_glib_none();
        let rules_ptr: *mut gtk::glib::ffi::GBytes = rules.to_glib_none().0;
        webkit2gtk::ffi::webkit_user_content_filter_store_save(
            store,
            id.0,
            rules_ptr,
            std::ptr::null_mut(),
            Some(remote_block_filter_saved),
            Box::into_raw(state) as gtk::glib::ffi::gpointer,
        );
    }
}

/// `webkit_user_content_filter_store_save` 완료 콜백.
///
/// # Safety
/// GIO 가 부르는 `GAsyncReadyCallback` 이다. `user_data` 는 위 함수가
/// `Box::into_raw` 로 넘긴 `FilterSaveState` 이고 이 호출이 유일한 소비자다.
unsafe extern "C" fn remote_block_filter_saved(
    _source: *mut gtk::glib::gobject_ffi::GObject,
    result: *mut gtk::gio::ffi::GAsyncResult,
    user_data: gtk::glib::ffi::gpointer,
) {
    use gtk::glib::translate::ToGlibPtr;

    // SAFETY: 위 주석의 계약. 포인터는 한 번만 되찾는다.
    let state = unsafe { Box::from_raw(user_data.cast::<FilterSaveState>()) };
    let mut err: *mut gtk::glib::ffi::GError = std::ptr::null_mut();
    // SAFETY: `state.store` 는 저장을 시작한 그 저장소이고, `result` 는 GIO 가 준
    // 이 호출의 결과다. 실패면 널을 주고 `err` 를 채운다.
    let filter = unsafe {
        webkit2gtk::ffi::webkit_user_content_filter_store_save_finish(state.store, result, &mut err)
    };
    // SAFETY: 저장소 참조는 `compile_remote_block_filter` 가 만든 것이고 여기가 끝이다
    // (비동기 작업은 자기 참조를 따로 잡는다).
    unsafe { gtk::glib::gobject_ffi::g_object_unref(state.store.cast()) };
    if filter.is_null() {
        // SAFETY: 실패 시 GIO 가 채워 준 GError — 메시지를 읽고 해제한다.
        let msg = unsafe {
            let m = if err.is_null() {
                String::from("(원인 미상)")
            } else {
                std::ffi::CStr::from_ptr((*err).message)
                    .to_string_lossy()
                    .into_owned()
            };
            if !err.is_null() {
                gtk::glib::ffi::g_error_free(err);
            }
            m
        };
        tracing::warn!("원격 차단 content filter 컴파일 실패: {msg} — 서브리소스 차단이 안 걸린다");
        return;
    }
    let filter = ContentFilter(filter);
    if state.block_remote.get() {
        // SAFETY: 방금 받은 유효한 필터를 이 webview 의 매니저에 붙인다. 매니저는
        // 상태 상자가 살아 있는 동안 유효하다.
        unsafe {
            webkit2gtk::ffi::webkit_user_content_manager_add_filter(
                state.ucm.to_glib_none().0,
                filter.0,
            )
        };
    }
    *state.slot.borrow_mut() = Some(filter);
}
