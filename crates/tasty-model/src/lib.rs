#![forbid(unsafe_code)]

//! Tasty workspace / pane / tab / surface 도메인 모델.
//!
//! popup_kind / toast_kind 등은 본 바이너리의
//! gui 컴포넌트 API surface. headless 빌드 (`tasty --no-default-features`) 에선
//! 호출자가 cfg(gui) 로 차단되지만, 공개 표면은 library crate 의 `dead_code` 판정
//! 대상이 아니라 crate 단위 억제가 필요 없다.

/// `Surface::as_any` / `as_any_mut` 구현을 한 줄로 채우는 매크로.
///
/// ```ignore
/// impl Surface for MyPanel {
///     tasty_model::impl_surface_any!();
///     // ... 다른 메서드들 ...
/// }
/// ```
#[macro_export]
macro_rules! impl_surface_any {
    () => {
        fn as_any(&self) -> &dyn ::std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
            self
        }
    };
}

// LogicalPx / PhysicalPx 는 별 leaf crate `tasty-type-geometry` 에 정의되어 있다.
// 본 바이너리의 group import 호환성을 위해 재수출 유지.
// 새 코드는 `tasty_type_geometry::length::*` 직접 import 권장.
pub use tasty_type_geometry::direction::{FocusDirection, SplitDirection};
pub use tasty_type_geometry::length::{LogicalPx, PhysicalPx};
pub use tasty_type_geometry::rect::{DividerInfo, LogicalRect, PhysicalRect};

// 식별자 alias 는 tasty-utils::id 로 이전됨. 본 모듈은 호환을 위해 재수출 유지.
// 새 코드는 `tasty_utils::id::*` 직접 import 권장.
pub use tasty_utils::id::{
    NORMAL_CATEGORY_ID, PaneId, SurfaceId, TabId, WorkspaceCategoryId, WorkspaceId,
};

/// 분할된 pane 사이의 간격(보이는 보더로 렌더된다).
///
/// **논리 길이다.** 디자인이 치수를 정하는 시각 요소이므로 배율을 따라 커진다 —
/// 배율 2 화면에서 4 물리 px 로 그려진다. 물리로 두면 고해상도에서 선이 절반
/// 두께로 보이는데, 그것은 "얇은 선" 이 아니라 **디자인이 정한 두께가 안 지켜지는
/// 것**이다. 근거·대안·재검토 조건:
/// `docs/adr/0148-physical-px-constants-are-split-by-what-they-are-for.md`.
///
/// 비교 좌표계(물리)로 내리는 것은 `BinaryTree::border_width` 한 곳이다.
pub const PANE_BORDER_WIDTH: LogicalPx = LogicalPx(2.0);

/// 한 탭 안에서 분할된 surface 사이의 간격.
///
/// **물리 길이다 — 그리고 그것이 의도다.** 이 선은 hairline 이다: 화면 밀도와
/// 무관하게 **언제나 1 device px** 로 남아야 하는 구획선이라, 배율을 따라 커지면
/// 안 된다. 그래서 `PANE_BORDER_WIDTH` 와 값의 성격이 다르고, 둘을 한 덩어리로
/// 다루지 않는다(같은 ADR-0148).
///
/// ★ **hairline 이라는 판단은 추론이다.** 이 값이 물리 1 px 인 것이 hairline
/// 의도인지 단순 누락인지를 말해 주는 디자인 문서를 찾지 못했다. 추론의 근거는
/// "1 물리 px 는 배율을 곱하면 정수 device px 를 벗어나 흐려진다" 뿐이다.
/// 디자인 쪽에서 hairline 명시가 확인되면 그때 확정으로 바꾼다.
pub const SURFACE_BORDER_WIDTH: PhysicalPx = PhysicalPx(1.0);

/// Compute the terminal area rectangle (everything right of the sidebar, below the
/// titlebar) in physical pixels.
///
/// `top_inset` reserves space at the top for the custom titlebar (CSD). It is `0` until
/// the titlebar is actually drawn, making the inset a no-op.
///
/// `bottom_inset` reserves space at the bottom of the work column for the StatusBar
/// (`adapters::ui::status_bar`). Like `top_inset`, a value of `0` makes it a no-op.
///
/// This is the single canonical implementation. Both `main.rs` and `gpu.rs` should use this.
pub fn compute_terminal_rect(
    surface_width: PhysicalPx,
    surface_height: PhysicalPx,
    sidebar_width: LogicalPx,
    top_inset: PhysicalPx,
    bottom_inset: PhysicalPx,
    scale_factor: f32,
) -> PhysicalRect {
    let sw = sidebar_width
        .to_physical(scale_factor)
        .min(surface_width - PhysicalPx(1.0));
    PhysicalRect {
        x: sw,
        y: top_inset,
        width: (surface_width - sw).max(PhysicalPx(1.0)),
        height: (surface_height - top_inset - bottom_inset).max(PhysicalPx(1.0)),
    }
}

mod attach_mapping;
mod attach_mesh_surface;
pub mod banner_kind;
mod binary_tree;
pub mod closed_item;
mod dag_graph_surface;
mod empty_surface;
mod explorer_panel;
mod nav_state;
mod pane;
mod pane_tree;
pub mod popup_kind;
mod surface_layout;
mod surface_trait;
mod tab;
mod terminal_surface;
pub mod toast_kind;
mod workspace;
mod workspace_category;

pub use attach_mapping::{WorkspaceAttachMapping, WorkspaceAttachTarget};
pub use attach_mesh_surface::AttachMeshSurface;
pub use binary_tree::BinaryTree;
pub use closed_item::{ClosedItem, ClosedItemStore};
pub use dag_graph_surface::*;
pub use empty_surface::*;
pub use explorer_panel::*;
pub use nav_state::NavState;
pub use pane::*;
pub use pane_tree::*;
pub use surface_trait::Surface;
pub use tab::Tab;
pub use terminal_surface::*;
pub use workspace::*;
pub use workspace_category::*;

#[cfg(test)]
mod tests;
