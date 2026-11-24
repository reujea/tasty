//! 띠 kind WebView의 페이지 위치. 포커스 띠 행·주소 띠 아래에서 시작하고, 띠 행이 본문을 다 쓰면
//! 창을 드러내지 않는다. 바탕 kind는 띠 행을 비우지 않는다.

use super::page_geometry::{edge_reserved, page_bounds};
use crate::model::{PhysicalPx, PhysicalRect};
use crate::plugin_bridge::remote_surface::RemoteSurface;
use crate::runtime::surface_registry::KindCapabilities;
use crate::runtime::surface_registry::webview_kind;

fn leaf(y: f32, height: f32) -> PhysicalRect {
    PhysicalRect {
        x: PhysicalPx(100.0),
        y: PhysicalPx(y),
        width: PhysicalPx(400.0),
        height: PhysicalPx(height),
    }
}

/// 띠 행 아래, 주소 띠 아래에서 시작하고 아래·좌우 여백은 그대로다.
#[test]
fn the_page_starts_below_the_edge_row_and_the_chrome() {
    let page = page_bounds(leaf(60.0, 300.0), [1.0, 2.0, 3.0], 2.0, 40.0).expect("page");
    assert_eq!(page.y, 60.0 + 2.0 + 40.0);
    assert_eq!(page.x, 101.0);
    assert_eq!(page.width, 397.0);
    assert_eq!(page.y + page.height, 60.0 + 300.0 - 3.0, "the bottom stays");
    let bed = page_bounds(leaf(60.0, 300.0), [0.0; 3], 0.0, 40.0).expect("page");
    assert_eq!(bed.y, 100.0, "a bed kind starts right below its chrome");
}

/// 띠 행이 남은 본문을 다 쓰면 창을 드러내지 않는다. 띠 없이도 본문이 없던 배치는 예전대로 1px 창이다.
#[test]
fn an_edge_that_takes_the_whole_body_hides_the_page() {
    assert!(page_bounds(leaf(0.0, 3.0), [0.0; 3], 4.0, 0.0).is_none());
    assert!(page_bounds(leaf(0.0, 42.0), [0.0; 3], 2.0, 40.0).is_none());
    let left = page_bounds(leaf(0.0, 43.0), [0.0; 3], 2.0, 40.0).expect("one row is left");
    assert_eq!(left.height, 1.0);
    let no_edge = page_bounds(leaf(0.0, 30.0), [0.0; 3], 0.0, 40.0).expect("old behavior");
    assert_eq!(no_edge.height, 1.0);
    let chrome_only = page_bounds(leaf(0.0, 30.0), [0.0; 3], 2.0, 40.0).expect("old behavior");
    assert_eq!(
        chrome_only.height, 1.0,
        "the chrome alone already took the body"
    );
}

/// 띠를 예약하는 webview kind만 띠 행 높이를 돌려준다. 바탕을 선언한 kind와 WebView가 아닌 kind는 0이다.
#[test]
fn only_an_edge_webview_reserves_the_row() {
    let decl: tasty_plugin_manifest::SurfaceKindDecl = serde_json::from_value(serde_json::json!({
        "kind": "edge-web",
        "display_name_i18n_key": "surface.kind.html",
        "rendering": "webview",
    }))
    .expect("decl");
    let _g = webview_kind::WEBVIEW_KIND_TEST_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    webview_kind::register_webview_kind("com.example.edge-web", &decl, &[]);
    let at = leaf(100.75, 300.0);
    let edge = RemoteSurface::new(7, "edge-web", "com.example.edge-web".into(), "web".into());
    assert_eq!(edge_reserved(&edge, at, 1.25), 103.0 - 100.75);
    assert_eq!(edge_reserved(&edge, leaf(60.0, 300.0), 2.0), 4.0);
    let bed = RemoteSurface::new(8, "edge-web", "com.example.edge-web".into(), "web".into())
        .with_capabilities(KindCapabilities {
            bed_focus: true,
            ..KindCapabilities::default()
        });
    assert_eq!(edge_reserved(&bed, at, 1.25), 0.0);
    let not_web = RemoteSurface::new(9, "not-a-web-kind", "com.example.x".into(), "x".into());
    assert_eq!(edge_reserved(&not_web, at, 1.25), 0.0);
    assert_eq!(
        edge_reserved(&crate::model::TerminalSurface { id: 3 }, at, 1.25),
        0.0
    );
}
