use std::any::Any;
use std::path::PathBuf;

use super::{PhysicalRect, SurfaceId};

/// Common behavior for all Surface types.
///
/// Each surface type (TerminalSurface, EmptySurface, ExplorerPanel,
/// RemoteSurface, EguiMeshSurface) implements this trait.
/// All methods have default implementations suitable for non-terminal surfaces.
///
/// `Send` supertrait: Surface 트리를 담는 `CoreState` 가 부팅 워커 스레드에서
/// 생성돼 채널로 메인에 전달되므로 (boot_machine 의 WaitingEngine), 모든 구현체는
/// 스레드 간 이동 가능해야 한다.
pub trait Surface: Any + Send {
    /// Stable identifier for this surface kind (lowercase, snake_case).
    /// 예: `"terminal"`, `"markdown"`. IPC/registry/플러그인이
    /// 식별자로 쓰며, 절대 변경되지 않는다.
    fn kind(&self) -> &'static str;

    /// Any-cast accessor. Used by the surface registry's render/snapshot/restore
    /// closures and other callers that need to recover the concrete surface type
    /// without a per-kind downcast method on the trait.
    /// 모든 구현체는 `crate::impl_surface_any!()` 매크로 한 줄로 채울 수 있다.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Display-only type name (e.g. "Terminal", "Markdown"). 사용자에게 보이는
    /// 라벨이며 식별 비교에는 `kind()`를 써야 한다. 향후 i18n 적용 가능.
    fn type_name(&self) -> &'static str;

    /// Get this surface's ID.
    fn surface_id(&self) -> Option<SurfaceId>;

    /// All surface IDs contained in this surface.
    fn all_surface_ids(&self) -> Vec<SurfaceId> {
        self.surface_id().into_iter().collect()
    }

    /// The focused surface ID.
    fn focused_surface_id(&self) -> Option<SurfaceId> {
        self.surface_id()
    }

    /// Whether this surface contains the given surface ID.
    fn contains_surface(&self, surface_id: SurfaceId) -> bool {
        self.all_surface_ids().contains(&surface_id)
    }

    /// attach mesh mirror 후보 판별(`docs/dev-guide/attach-behavior.md` "mesh mirror 채널").
    /// `Some((kind, plugin_id))` 를 반환하면 이 surface 는 plugin egui-mesh
    /// surface 라는 뜻 — `Workspace::classify_attach_surfaces` 가 이 신호로
    /// mesh 후보를 모으고, 실제 화이트리스트 판정(어떤 `(kind, plugin_id)` 조합이
    /// mirror 허용인지)은 이 crate 밖(앱 계층, `src/core/attach_runtime.rs`)의
    /// 책임으로 남긴다 — `tasty-model` 은 자신에 의존하는 상위 crate(`src/`)의
    /// `EguiMeshSurface` 구체 타입을 모른 채로 "mesh 기반이다" 라는 계약만 정의한다
    /// (trait 정의는 하위 crate, 구현은 상위 crate — crate 의존 방향 유지).
    ///
    /// 기본 구현은 `None`(mesh 아님) — 대다수 surface(터미널/explorer 등)에 영향 없음.
    fn attach_mesh_info(&self) -> Option<(&str, &str)> {
        None
    }

    /// attach content mirror 후보 판별(`docs/adr/0255-markdown-attach-mirror-forwards-content-not-pixels.md`).
    /// `Some((kind, plugin_id, file))` 를 반환하면 이 surface 는 **렌더 결과가 아니라
    /// 원문**을 attach 채널로 나를 수 있다는 뜻 — `Workspace::classify_attach_surfaces`
    /// 가 이 신호로 후보를 모으고, 실제 화이트리스트 판정(어떤 `(kind, plugin_id)`
    /// 조합이 이 채널을 타는지)은 [`Self::attach_mesh_info`] 와 **같은 이유로** 이
    /// crate 밖(앱 계층, `src/core/attach_runtime.rs`)의 책임으로 남긴다.
    ///
    /// `file` 은 그 surface 가 열고 있는 **원격 절대경로**다. 없으면(파일 없이 열린
    /// 빈 문서) `None` — 그 경우도 후보이긴 하다(빈 문서를 빈 문서로 mirror 한다).
    /// 반환 위치가 owned 인 이유는 구현체(`RemoteSurface`)가 이 값을 mutex 뒤의
    /// snapshot 캐시에서 꺼내 빌려줄 수 없기 때문이다.
    ///
    /// 기본 구현은 `None`(이 채널 대상 아님) — 대다수 surface 에 영향 없음.
    fn attach_content_info(&self) -> Option<(&str, &str, Option<PathBuf>)> {
        None
    }

    /// Resize-fitting hook. Layout 가 leaf 의 rect 를 알릴 때 호출. 기본 no-op.
    /// 현재 모든 구현 (TerminalSurface 포함) 이 default 만 — Terminal resize 는
    /// 별 PTY resize 경로로 분리. 본 메서드는 후속 surface kind 들의 옵션.
    fn resize_all(&mut self, _rect: PhysicalRect, _cell_width: f32, _cell_height: f32) {}

    /// The "source" working directory associated with this surface, if any.
    ///
    /// 단축키 등 사용자 행위로 새 surface(터미널/탭/워크스페이스 등)를 만들 때
    /// 이 값을 시작 cwd로 상속한다. **default 본문 없음** — 모든 Surface impl
    /// 이 의미를 명시적으로 결정해야 한다 (Surface cwd invariant —
    /// `docs/design/policies/cwd.md#surface-cwd-invariant`).
    ///
    /// - TerminalSurface: 터미널의 OSC 7 cwd (engine.terminals 경유 — trait 는
    ///   None 반환; host 의 `CoreState::surface_cwd` 가 분기)
    /// - EguiMeshSurface(markdown 등): 파일의 부모 디렉터리
    /// - EmptySurface: None (또는 carry 받은 cwd)
    /// - RemoteSurface: 호스트가 carry 한 cwd (None 또는 ctx.cwd 그대로)
    fn source_cwd(&self) -> Option<PathBuf>;

    /// Display name for tab title. Default: type_name.
    fn display_name(&self) -> String {
        self.type_name().to_string()
    }

    /// webview URL accessor. webview overlay 를 사용하는 surface kind 가 자신의
    /// URL 을 반환. host 의 `sync_webviews` 가 이 메서드로 surface 별 URL 을
    /// 식별. 일반 surface 는 default `None` 반환.
    ///
    /// `Option<String>` 시그니처는 plugin RemoteSurface 가 lock 으로 보관한 URL
    /// 캐시를 owned 로 cloning 해 반환할 수 있게 한다.
    fn webview_url(&self) -> Option<String> {
        None
    }

    /// Produce a JSON tree representation of this surface.
    fn to_tree_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "kind": self.kind(),
            "type": self.type_name(), // 호환성을 위한 별칭. 신규 코드는 `kind` 사용.
        });
        if let Some(id) = self.surface_id() {
            obj["id"] = serde_json::json!(id);
        }
        obj
    }
}
