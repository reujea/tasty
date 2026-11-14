//! 매니페스트 검증과 egui-mesh 선언을 확인한다.

use std::path::PathBuf;
use tasty_plugin_manifest::{Manifest, SurfaceKindRendering};

#[test]
fn manifest_loads_and_validates() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let m = Manifest::load(&dir).expect("manifest should load + validate");
    assert_eq!(m.id, "com.tasty.mesh-demo");
    assert_eq!(m.api_version, "1");
    assert_eq!(m.surface_kinds.len(), 1);
    assert_eq!(m.surface_kinds[0].kind, "mesh_demo");
    assert_eq!(
        m.surface_kinds[0].rendering,
        SurfaceKindRendering::EguiMesh,
        "rendering must parse to EguiMesh (wire key 'egui-mesh')"
    );
    assert!(
        m.surface_kinds[0].egui_paste,
        "the demo takes format pastes to check the reply path"
    );
}
