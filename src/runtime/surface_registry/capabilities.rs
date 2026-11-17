//! kind가 선언한 host 기능 능력. 등록 때 한 번 해석해 정의·메타데이터·surface에 싣는다.
//! 호출 지점은 kind 이름이나 plugin id가 아니라 이 값만 본다.

use tasty_plugin_manifest::SurfaceKindCapabilityDecl;

/// 해석한 능력. 키와 뜻은 [`SurfaceKindCapabilityDecl`]과 같다.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KindCapabilities {
    pub address_band: bool,
    pub inset_notices: bool,
    pub script_gate: bool,
    pub opens_url: bool,
    pub remote_http_serve: bool,
    pub mirror_viewer: bool,
    pub mirror_content: bool,
    pub bed_focus: bool,
}

impl KindCapabilities {
    /// plugin kind의 선언을 해석한다. 선언한 키는 그 값을 쓴다.
    ///
    /// 선언하지 않은 키는 이 능력들이 생기기 전에 host가 쓰던 이름 규칙으로 정한다. 배포된 공식
    /// plugin manifest가 아직 이 키들을 선언하지 않아, 선언을 받기 전까지 그 기능이 꺼지지 않게 한다.
    /// 이 규칙은 이 함수 밖에 두지 않는다.
    pub fn resolve(decl: &SurfaceKindCapabilityDecl, kind: &str, plugin_id: &str) -> Self {
        let html = kind == "html";
        let official_html = html && plugin_id == "com.tasty.html";
        let official_markdown = kind == "markdown" && plugin_id == "com.tasty.markdown";
        let official_image = kind == "image" && plugin_id == "com.tasty.image";
        Self {
            address_band: decl.address_band.unwrap_or(html),
            inset_notices: decl.inset_notices.unwrap_or(html),
            script_gate: decl.script_gate.unwrap_or(html),
            opens_url: decl.opens_url.unwrap_or(official_html),
            remote_http_serve: decl.remote_http_serve.unwrap_or(official_html),
            mirror_viewer: decl.mirror_viewer.unwrap_or(official_html),
            mirror_content: decl.mirror_content.unwrap_or(official_markdown),
            bed_focus: decl
                .bed_focus
                .unwrap_or(official_markdown || official_image),
        }
    }
}

#[cfg(test)]
impl KindCapabilities {
    /// 아무것도 선언하지 않은 kind가 등록 때 받는 능력. 시험 fixture가 등록 없이 surface를 만들 때 쓴다.
    pub fn undeclared(kind: &str, plugin_id: &str) -> Self {
        Self::resolve(&SurfaceKindCapabilityDecl::default(), kind, plugin_id)
    }
}

#[cfg(feature = "gui")]
impl super::SurfaceKindRegistry {
    /// 능력이 맞는 철회되지 않은 kind 중 이름 순으로 첫 kind.
    pub fn first_live_kind_with(
        &self,
        wanted: impl Fn(&KindCapabilities) -> bool,
    ) -> Option<&'static str> {
        self.metadata_snapshot()
            .into_iter()
            .filter(|(_, metadata)| wanted(&metadata.capabilities))
            .map(|(kind, _)| kind)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(toml: &str) -> SurfaceKindCapabilityDecl {
        let manifest: tasty_plugin_manifest::SurfaceKindDecl = toml::from_str(&format!(
            "kind = \"k\"\ndisplay_name_i18n_key = \"k\"\n{toml}"
        ))
        .expect("surface kind");
        manifest.capabilities
    }

    #[test]
    fn a_declared_key_wins_over_the_name() {
        let off = decl("address_band = false\nmirror_viewer = false");
        let caps = KindCapabilities::resolve(&off, "html", "com.tasty.html");
        assert!(!caps.address_band);
        assert!(!caps.mirror_viewer);
        assert!(caps.script_gate, "선언하지 않은 키는 이름 규칙을 따른다");

        let on = decl("address_band = true\nscript_gate = true");
        let caps = KindCapabilities::resolve(&on, "my-html", "com.example.my-html");
        assert!(caps.address_band);
        assert!(caps.script_gate);
        assert!(!caps.inset_notices);
        assert!(!caps.mirror_viewer);
    }

    /// 아무것도 선언하지 않은 공식 kind는 능력이 생기기 전과 같은 기능을 받는다.
    #[test]
    fn an_undeclared_official_kind_keeps_its_features() {
        let none = SurfaceKindCapabilityDecl::default();
        let html = KindCapabilities::resolve(&none, "html", "com.tasty.html");
        assert_eq!(
            html,
            KindCapabilities {
                address_band: true,
                inset_notices: true,
                script_gate: true,
                opens_url: true,
                remote_http_serve: true,
                mirror_viewer: true,
                mirror_content: false,
                bed_focus: false,
            }
        );
        let markdown = KindCapabilities::resolve(&none, "markdown", "com.tasty.markdown");
        assert_eq!(
            markdown,
            KindCapabilities {
                mirror_content: true,
                bed_focus: true,
                ..KindCapabilities::default()
            }
        );
        let image = KindCapabilities::resolve(&none, "image", "com.tasty.image");
        assert_eq!(
            image,
            KindCapabilities {
                bed_focus: true,
                ..KindCapabilities::default()
            }
        );
        // 공식 id가 아닌 같은 이름은 소유 확인이 필요한 능력을 받지 못한다.
        let other = KindCapabilities::resolve(&none, "html", "com.example.html");
        assert!(other.address_band && !other.remote_http_serve && !other.mirror_viewer);
        let other = KindCapabilities::resolve(&none, "markdown", "com.example.md");
        assert_eq!(other, KindCapabilities::default());
    }
}
