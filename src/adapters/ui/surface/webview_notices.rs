//! host가 webview 위에 그리는 주소 띠와 안내 카드의 대상 surface. kind가 선언한 능력으로 정한다.
//!
//! 스크립트·원격 콘텐츠 안내는 그 허용을 집행하는 스크립트 게이트(`script_gate`)를 따른다. 게이트를
//! 붙이고 허용 수단을 주지 않는 조합을 만들지 않기 위해서다. 그 밖의 안내(받는 문서의 크기 안내)는
//! `inset_notices`가 맡는다. 주소 띠(`address_band`)와는 독립이고, 띠는 수신 표시만 그린다.

use crate::runtime::surface_registry::KindCapabilities;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WebviewChromeTargets {
    /// 주소 띠를 그릴 surface.
    pub band: Option<u32>,
    /// 스크립트·원격 콘텐츠 상태를 읽을 surface.
    pub script: Option<u32>,
    /// 받는 문서 상태(띠의 수신 표시·수신 화면)를 읽을 surface.
    pub receive: Option<u32>,
    /// 받는 문서의 크기 안내를 그릴 surface.
    pub size: Option<u32>,
    /// 안내 카드 묶음을 그릴 surface.
    pub notices: Option<u32>,
}

impl WebviewChromeTargets {
    pub(crate) fn of(surface_id: Option<u32>, capabilities: KindCapabilities) -> Self {
        let when = |on: bool| surface_id.filter(|_| on);
        Self {
            band: when(capabilities.address_band),
            script: when(capabilities.script_gate),
            receive: when(capabilities.address_band || capabilities.inset_notices),
            size: when(capabilities.inset_notices),
            notices: when(capabilities.script_gate || capabilities.inset_notices),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(address_band: bool, script_gate: bool, inset_notices: bool) -> KindCapabilities {
        KindCapabilities {
            address_band,
            script_gate,
            inset_notices,
            ..KindCapabilities::default()
        }
    }

    /// 안내 상태는 주소 띠가 없어도 그 안내를 맡은 능력을 선언한 surface에서 읽는다.
    #[test]
    fn notices_do_not_depend_on_the_address_band() {
        let gate_only = WebviewChromeTargets::of(Some(7), caps(false, true, false));
        assert_eq!(
            gate_only,
            WebviewChromeTargets {
                band: None,
                script: Some(7),
                receive: None,
                size: None,
                notices: Some(7),
            }
        );
        let band_and_gate = WebviewChromeTargets::of(Some(7), caps(true, true, false));
        assert_eq!(band_and_gate.script, Some(7));
        assert_eq!(
            band_and_gate.notices,
            Some(7),
            "스크립트 안내는 inset_notices 없이도 그린다"
        );
        let insets_only = WebviewChromeTargets::of(Some(7), caps(false, false, true));
        assert_eq!(
            insets_only,
            WebviewChromeTargets {
                band: None,
                script: None,
                receive: Some(7),
                size: Some(7),
                notices: Some(7),
            }
        );
        assert_eq!(
            WebviewChromeTargets::of(Some(7), KindCapabilities::default()),
            WebviewChromeTargets::default()
        );
        assert_eq!(
            WebviewChromeTargets::of(None, caps(true, true, true)),
            WebviewChromeTargets::default()
        );
    }

    /// 크기 안내는 `inset_notices`를 선언한 kind만 그린다. 주소 띠는 수신 상태만 읽는다.
    #[test]
    fn the_size_notice_follows_inset_notices_not_the_address_band() {
        let band_only = WebviewChromeTargets::of(Some(7), caps(true, false, false));
        assert_eq!(band_only.receive, Some(7), "띠의 수신 표시");
        assert_eq!(band_only.size, None, "주소 띠만으로는 크기 안내가 없다");
        let band_and_gate = WebviewChromeTargets::of(Some(7), caps(true, true, false));
        assert_eq!(
            band_and_gate.size, None,
            "스크립트 게이트도 크기 안내를 켜지 않는다"
        );
        let band_and_insets = WebviewChromeTargets::of(Some(7), caps(true, false, true));
        assert_eq!(band_and_insets.size, Some(7));
        assert_eq!(band_and_insets.notices, Some(7));
    }
}
