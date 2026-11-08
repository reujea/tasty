//! plugin이 그린 egui mesh를 표시할 surface 모델. 실제 수신·렌더링은 gfx와 view 계층이 맡는다.
//! Surface::kind가 static 문자열을 요구하므로 등록부가 kind 문자열을 한 번 할당해 유지한다.

use std::path::PathBuf;

use crate::model::{Surface, SurfaceId};

pub struct EguiMeshSurface {
    pub id: SurfaceId,
    pub retirement_binding: crate::plugin_bridge::host_cmd::MeshBinding,
    pub kind_static: &'static str,
    pub plugin_id: String,
    pub display_name: String,
    /// 복원 때 plugin에 다시 넘길 파일 경로. 파일을 쓰지 않는 종류는 None이다.
    pub file: Option<String>,
}

impl EguiMeshSurface {
    pub fn new(
        id: SurfaceId,
        kind_static: &'static str,
        plugin_id: String,
        display_name: String,
        file: Option<String>,
    ) -> Self {
        Self {
            id,
            retirement_binding: Default::default(),
            kind_static,
            plugin_id,
            display_name,
            file,
        }
    }
}

impl Surface for EguiMeshSurface {
    tasty_model::impl_surface_any!();

    fn kind(&self) -> &'static str {
        self.kind_static
    }

    fn type_name(&self) -> &'static str {
        "EguiMesh"
    }

    fn surface_id(&self) -> Option<SurfaceId> {
        Some(self.id)
    }

    /// 절대 경로 파일이면 부모 폴더를 cwd 상속에 제공한다. 상대 경로는 기준을 모르므로 쓰지 않는다.
    fn source_cwd(&self) -> Option<PathBuf> {
        self.file
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }

    fn display_name(&self) -> String {
        self.display_name.clone()
    }

    fn to_tree_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind_static,
            "type": "EguiMesh",
            "id": self.id,
            "plugin_id": self.plugin_id,
            "display_name": self.display_name,
        })
    }

    /// 후보 정보만 반환한다. mesh 허용 목록 검사는 앱 계층에서 맡는다.
    fn attach_mesh_info(&self) -> Option<(&str, &str)> {
        Some((self.kind_static, &self.plugin_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_basics() {
        let docs = PathBuf::from(if cfg!(windows) { r"C:\docs" } else { "/docs" });
        let s = EguiMeshSurface::new(
            7,
            "markdown",
            "com.tasty.markdown".into(),
            "Readme".into(),
            Some(docs.join("readme.md").to_string_lossy().into_owned()),
        );
        assert_eq!(s.kind(), "markdown");
        assert_eq!(s.type_name(), "EguiMesh");
        assert_eq!(s.surface_id(), Some(7));
        assert_eq!(s.display_name(), "Readme");
        assert_eq!(s.source_cwd(), Some(docs));
    }

    #[test]
    fn source_cwd_none_for_a_relative_file() {
        for file in ["a.png", "docs/a.png"] {
            let s = EguiMeshSurface::new(
                8,
                "image",
                "com.tasty.image".into(),
                "A".into(),
                Some(file.into()),
            );
            assert_eq!(s.source_cwd(), None, "{file}");
        }
    }

    #[test]
    fn source_cwd_none_without_file() {
        let s = EguiMeshSurface::new(9, "mesh-demo", "com.tasty.demo".into(), "Demo".into(), None);
        assert_eq!(s.source_cwd(), None);
    }

    #[test]
    fn tree_json_shape() {
        let s = EguiMeshSurface::new(
            3,
            "markdown",
            "com.tasty.markdown".into(),
            "Doc".into(),
            None,
        );
        let j = s.to_tree_json();
        assert_eq!(j["kind"], "markdown");
        assert_eq!(j["type"], "EguiMesh");
        assert_eq!(j["id"], 3);
        assert_eq!(j["plugin_id"], "com.tasty.markdown");
    }
}
