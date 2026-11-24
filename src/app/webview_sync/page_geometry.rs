//! 띠 kind WebView의 native 페이지 위치. 포커스 띠 행과 주소 띠·배너 아래에서 시작한다.

use crate::plugin_bridge::remote_surface::RemoteSurface;

/// 띠 kind WebView가 leaf 위에 비워 두는 높이(물리 px). 바탕 kind와 WebView가 아닌 surface는 0이다.
pub(super) fn edge_reserved(
    surface: &dyn crate::model::Surface,
    leaf: crate::model::PhysicalRect,
    scale_factor: f32,
) -> f64 {
    let reserves = surface
        .as_any()
        .downcast_ref::<RemoteSurface>()
        .is_some_and(|remote| {
            crate::state::surface_focus::webview_reserves_edge(
                remote.kind_static,
                remote.capabilities,
            )
        });
    if !reserves {
        return 0.0;
    }
    let below = crate::state::surface_focus::below_edge(&crate::theme::theme(), leaf, scale_factor);
    f64::from((below.y - leaf.y).value())
}

/// leaf 안 native 페이지의 물리 사각형. `insets`는 좌·우·아래 여백, `edge`는 포커스 띠 행 높이,
/// `content_top`은 주소 띠·배너 높이다. 페이지는 `edge + content_top` 아래에서 시작한다.
/// 띠 행이 남은 본문을 다 쓰면 `None`이다. 그 창은 드러내지 않는다. 띠가 없어도 본문이 없는 경우는
/// 예전대로 1px 창을 둔다(띠 예약과 무관한 기존 동작).
pub(super) fn page_bounds(
    leaf: crate::model::PhysicalRect,
    [left, right, bottom]: [f64; 3],
    edge: f64,
    content_top: f64,
) -> Option<crate::webview::PhysicalWebViewBounds> {
    let top = edge + content_top;
    let height = f64::from(leaf.height.value()) - bottom - top;
    if edge > 0.0 && height < 1.0 && height + edge >= 1.0 {
        return None;
    }
    Some(crate::webview::PhysicalWebViewBounds {
        x: f64::from(leaf.x.value()) + left,
        y: f64::from(leaf.y.value()) + top,
        width: (f64::from(leaf.width.value()) - left - right).max(1.0),
        height: height.max(1.0),
    })
}
