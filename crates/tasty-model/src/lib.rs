#![forbid(unsafe_code)]

//! GUI에 의존하지 않는 workspace·pane·tab·surface 모델과 UI 분류 타입.

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

// 호환 import를 위한 재수출. 새 코드는 tasty_type_geometry::length를 직접 사용한다.
pub use tasty_type_geometry::direction::{FocusDirection, SplitDirection};
pub use tasty_type_geometry::length::{LogicalPx, PhysicalPx};
pub use tasty_type_geometry::rect::{DividerInfo, LogicalRect, PhysicalRect};

// 호환 import를 위한 재수출. 새 코드는 tasty_utils::id를 직접 사용한다.
pub use tasty_utils::id::{
    NORMAL_CATEGORY_ID, PaneId, SurfaceId, TabId, WorkspaceCategoryId, WorkspaceId,
};

/// pane 사이 간격의 논리 두께. BinaryTree::border_width에서 배율을 적용해 물리 두께로 바꾼다.
pub const PANE_BORDER_WIDTH: LogicalPx = LogicalPx(2.0);

/// surface 사이 간격의 물리 두께. 배율과 무관하게 1 device px을 유지한다.
/// 별도의 디자인 의도가 확인된 값이라는 뜻은 아니며 현재 좌표계 계약을 나타낸다.
pub const SURFACE_BORDER_WIDTH: PhysicalPx = PhysicalPx(1.0);

/// 분할 비율의 허용 범위. 분할 경계 드래그와 split 의 비율 보정이 같은 범위를 쓴다.
/// 첫째 칸이 이 비율 아래로 줄거나 둘째 칸이 `1 - DIVIDER_RATIO_MAX` 아래로 줄지 않는다.
pub const DIVIDER_RATIO_MIN: f32 = 0.1;
pub const DIVIDER_RATIO_MAX: f32 = 0.9;

/// Compute the terminal area rectangle (everything right of the sidebar, below the
/// titlebar) in physical pixels.
///
/// `top_inset` reserves space at the top for the custom titlebar (CSD). It is `0` until
/// the titlebar is actually drawn, making the inset a no-op.
///
/// `bottom_inset` reserves space at the bottom of the work column for the StatusBar
/// (`adapters::ui::status_bar`). Like `top_inset`, a value of `0` makes it a no-op.
///
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

mod attach_mesh_surface;
pub mod banner_kind;
mod binary_tree;
pub mod closed_item;
mod dag_graph_surface;
mod empty_surface;
mod explorer_panel;
pub mod html_script;
mod nav_state;
mod pane;
mod pane_tree;
pub mod popup_kind;
mod remote_binding;
mod surface_descriptor;
mod surface_layout;
pub use surface_descriptor::SurfaceDescriptor;
mod surface_trait;
mod tab;
mod terminal_surface;
pub mod toast_kind;
mod workspace;
mod workspace_category;

pub use attach_mesh_surface::AttachMeshSurface;
pub use binary_tree::{BinaryTree, TreeGeometry};
pub use closed_item::{ClosedItem, ClosedItemStore};
pub use dag_graph_surface::*;
pub use empty_surface::*;
pub use explorer_panel::*;
pub use nav_state::{
    NavState, WebViewNavInfo, committed_document_url, is_same_document_fragment_move,
};
pub use pane::*;
pub use pane_tree::*;
pub use remote_binding::RemoteBinding;
pub use surface_trait::Surface;
pub use tab::{SurfaceTitle, Tab};
pub use terminal_surface::*;
pub use workspace::*;
pub use workspace_category::*;

#[cfg(test)]
mod tests;

mod structure_presentation;
pub use structure_presentation::{
    RestoredPresentation, StructurePresentation, StructurePresentationSnapshot,
};

mod split_node_id;
pub use split_node_id::SplitNodeId;
