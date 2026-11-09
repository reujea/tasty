//! plugin webview 페이지의 우클릭 메뉴. 페이지는 OS 우클릭을 소유 plugin 에 넘기고, plugin 이
//! `webview.context_menu` 로 이 메뉴를 연다(`src/app/dispatch/plugin_webview_menu.rs`).
//! 항목은 surface ID 복사 하나다. 명세: docs/dev-guide/context-menu.md.
//!
//! 복사가 되면 host 토스트를 띄우지 않는다. 토스트 카드가 겹친 webview 는 토스트가 닫힐 때까지 통째로
//! 숨겨지기 때문이다(`webview_sync::hidden_under_toast`). 대신 고른 결과를 소유 plugin 에 보내 페이지
//! 안에서 알리게 한다. 클립보드 쓰기가 실패하면 그 오류는 host 토스트로 알린다.

use crate::runtime::engine_read::EngineRead;
use crate::state::PendingNativeMenu;

use super::MainView;

const ITEM_COPY_SURFACE_ID: u32 = 1;

impl MainView {
    pub(super) fn handle_webview_native_menu(
        &mut self,
        engine: &EngineRead<'_>,
        pending: PendingNativeMenu,
    ) {
        use crate::platform::native_menu::MenuItem;
        let PendingNativeMenu::WebviewSurface {
            surface_id,
            plugin_id,
            page_x,
            page_y,
            x,
            y,
        } = pending
        else {
            return;
        };
        let items = [MenuItem::new(
            ITEM_COPY_SURFACE_ID,
            crate::i18n::t("terminal_context_menu.copy_surface_id"),
        )];
        self.open_native_menu(engine, x, y, &items, move |this, engine, result| {
            // 메뉴가 열려 있는 동안 대상 surface 가 닫혔을 수 있다.
            if !engine.has_surface(surface_id) || result != Some(ITEM_COPY_SURFACE_ID) {
                return;
            }
            let copied = match this.clipboard.set_text(&surface_id.to_string()) {
                Ok(()) => true,
                Err(e) => {
                    this.state.clipboard_error_toast(
                        &e,
                        crate::adapters::ui::ToastScope::Surface(surface_id),
                    );
                    false
                }
            };
            crate::shortcuts::send_app_event(
                &this.proxy,
                crate::AppEvent::WebviewMenuChosen {
                    plugin_id,
                    payload: selected_payload(surface_id, copied, page_x, page_y),
                },
            );
        });
    }
}

fn selected_payload(surface_id: u32, copied: bool, x: f32, y: f32) -> serde_json::Value {
    serde_json::json!({
        "surface_id": surface_id,
        "item": "copy_surface_id",
        "copied": copied,
        "x": x,
        "y": y,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_selection_names_the_item_and_returns_the_page_point() {
        assert_eq!(
            super::selected_payload(7, true, 12.0, 30.5),
            serde_json::json!({ "surface_id": 7, "item": "copy_surface_id", "copied": true, "x": 12.0, "y": 30.5 })
        );
    }
}
