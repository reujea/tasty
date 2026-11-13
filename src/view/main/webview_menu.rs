//! plugin webview 페이지의 우클릭 메뉴. 페이지는 OS 우클릭을 소유 plugin 에 넘기고, plugin 이
//! `webview.context_menu` 로 이 메뉴를 연다(`src/app/dispatch/plugin_webview_menu.rs`).
//! 항목은 surface ID 복사 하나다. 명세: docs/dev-guide/context-menu.md.
//!
//! 복사가 되면 host 토스트를 띄우지 않는다. 토스트 카드가 겹친 webview 는 토스트가 닫힐 때까지 통째로
//! 숨겨지기 때문이다(`webview_sync::hidden_under_toast`). 대신 고른 결과를 소유 plugin 에 보내 페이지
//! 안에서 알리게 한다. 클립보드 쓰기가 실패하면 그 오류는 host 토스트로 알린다.
//! 쓰기는 공통 복사 문(`clipboard::copy::copy_text_reporting_failure`)을 거친다.

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
            let chosen = engine.has_surface(surface_id) && result == Some(ITEM_COPY_SURFACE_ID);
            let payload = chosen.then(|| {
                // 성공 토스트는 띄우지 않는다 — 토스트 카드가 겹친 webview 는 통째로 숨겨져
                // 복사한 그 페이지를 가린다. 실패만 토스트로 알린다.
                let copied = crate::clipboard::copy::copy_text_reporting_failure(
                    &mut this.clipboard,
                    &mut this.state.toasts,
                    &surface_id.to_string(),
                    crate::adapters::ui::ToastScope::Surface(surface_id),
                );
                selected_payload(surface_id, copied, page_x, page_y)
            });
            // 고르지 않고 닫아도 진행 중 표시는 반드시 푼다 — 아니면 그 plugin 은
            // 다시는 메뉴를 열지 못한다.
            crate::shortcuts::send_app_event(
                &this.proxy,
                crate::AppEvent::WebviewMenuClosed { plugin_id, payload },
            );
        });
    }

    /// 무대(fullscreen stage)가 떠 요청을 버린다. 버린 것이 plugin webview 의 요청이면 조절기를
    /// 풀어 그 plugin 이 다시는 메뉴를 열지 못하는 상태로 남지 않게 한다.
    pub(super) fn drop_pending_menu_for_stage(&mut self) {
        if let Some(PendingNativeMenu::WebviewSurface { plugin_id, .. }) =
            self.state.dialogs.pending_native_menu.take()
        {
            crate::shortcuts::send_app_event(
                &self.proxy,
                crate::AppEvent::WebviewMenuClosed {
                    plugin_id,
                    payload: None,
                },
            );
        }
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
