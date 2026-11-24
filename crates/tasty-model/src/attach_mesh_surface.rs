//! 원격 egui-mesh surface의 로컬 표시 모델. plugin_id는 표시용이며 로컬 플러그인을 조회하지 않는다.
//! mesh 바이트는 attach 세션이 받아 호스트의 별도 프레임 저장소에 보관한다.

use std::path::PathBuf;

use super::SurfaceId;
use super::surface_trait::Surface;

/// attach로 받은 mesh를 표시할 surface. 렌더링은 호스트가 처리한다.
pub struct AttachMeshSurface {
    pub id: SurfaceId,
    /// 원격 kind. 실제 지원 여부는 호스트의 attach 검증에서 결정한다.
    pub kind_static: &'static str,
    /// 원격 plugin_id. 로컬 PluginManager 조회에 사용하지 않는다.
    pub plugin_id: String,
    pub display_name: String,
    /// 서버의 소유 plugin이 이 kind의 바탕을 host 테마 값으로 칠한다고 선언했는지(`bed_focus`).
    /// 거짓이면 client는 포커스를 leaf 위 띠로 보인다. 이 값을 싣지 않는 옛 서버도 거짓이다.
    pub bed_focus: bool,
}

impl AttachMeshSurface {
    pub fn new(
        id: SurfaceId,
        kind: &str,
        plugin_id: String,
        display_name: String,
        bed_focus: bool,
    ) -> Self {
        Self {
            id,
            kind_static: intern_known_kind(kind),
            plugin_id,
            display_name,
            bed_focus,
        }
    }
}

/// 알려진 kind를 고정 문자열로 변환하고 나머지는 mesh로 표시한다.
fn intern_known_kind(kind: &str) -> &'static str {
    match kind {
        // markdown은 현재 서버의 mesh 허용 목록에서 제외돼 있다.
        "markdown" => "markdown",
        "image" => "image",
        "mesh_demo" => "mesh_demo",
        // 알 수 없는 원격 kind는 일반 mesh 이름으로 보관한다.
        _ => "mesh",
    }
}

impl Surface for AttachMeshSurface {
    crate::impl_surface_any!();

    fn kind(&self) -> &'static str {
        self.kind_static
    }

    fn type_name(&self) -> &'static str {
        "AttachMesh"
    }

    fn surface_id(&self) -> Option<SurfaceId> {
        Some(self.id)
    }

    /// 원격 파일이라 로컬 파일시스템에 대응 경로가 없다 — cwd 상속 대상이 아니다.
    fn source_cwd(&self) -> Option<PathBuf> {
        None
    }

    fn display_name(&self) -> String {
        self.display_name.clone()
    }

    fn to_tree_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind_static,
            "type": "AttachMesh",
            "id": self.id,
            "plugin_id": self.plugin_id,
            "display_name": self.display_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_basics() {
        let s = AttachMeshSurface::new(
            7,
            "markdown",
            "com.tasty.markdown".into(),
            "Readme".into(),
            false,
        );
        assert_eq!(s.kind(), "markdown");
        assert_eq!(s.type_name(), "AttachMesh");
        assert_eq!(s.surface_id(), Some(7));
        assert_eq!(s.display_name(), "Readme");
        assert_eq!(s.source_cwd(), None);
    }

    #[test]
    fn unknown_kind_falls_back_without_panicking() {
        let s = AttachMeshSurface::new(
            1,
            "future_kind",
            "com.tasty.future".into(),
            "X".into(),
            false,
        );
        assert_eq!(s.kind(), "mesh");
    }

    #[test]
    fn tree_json_shape() {
        let s = AttachMeshSurface::new(3, "image", "com.tasty.image".into(), "Pic".into(), true);
        let j = s.to_tree_json();
        assert_eq!(j["kind"], "image");
        assert_eq!(j["type"], "AttachMesh");
        assert_eq!(j["id"], 3);
        assert_eq!(j["plugin_id"], "com.tasty.image");
    }
}
