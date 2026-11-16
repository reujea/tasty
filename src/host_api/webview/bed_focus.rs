//! 바탕 kind 페이지에 바탕 포커스 상태를 알리는 문자열. 속성 이름은 페이지와 함께 쓰는 계약이라
//! `tasty_plugin_protocol::bed_focus`에 있다. 계약과 이유는 `docs/design/systems/webview.md`의
//! "바탕 포커스 상태" 절에 있다.

use tasty_plugin_protocol::bed_focus::BED_FOCUSED_PROPERTY;

/// 상태를 만드는 고정 스타일시트. 계약 속성을 빈 값으로 정의하는 것뿐이다. 호스트는 이 문자열 말고는
/// 페이지에 넣지 않는다.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn bed_focused_sheet() -> String {
    format!(":root{{{BED_FOCUSED_PROPERTY}: ;}}")
}

/// 사용자 스타일시트가 없는 백엔드가 두 상태 모두 넣는 규칙. focus면 속성을 빈 값으로, 아니면
/// `initial`(무효 값)로 둔다. 페이지가 스스로 속성을 선언해도 덮도록 `:root`보다 구체성이 높다.
#[cfg(any(windows, test))]
fn bed_state_rule(focused: bool) -> String {
    let value = if focused { " " } else { "initial" };
    format!(":root:root{{{BED_FOCUSED_PROPERTY}:{value};}}")
}

/// [`bed_state_rule`]을 문서의 adopted stylesheet 하나로 두는 스크립트. 같은 문서에서 다시 실행하면
/// 그 시트의 규칙만 바꾼다. 문서 생성 시점에도 실행할 수 있도록 `document` 말고는 읽지 않는다.
#[cfg(any(windows, test))]
pub(crate) fn bed_state_script(focused: bool) -> String {
    let rule = serde_json::Value::String(bed_state_rule(focused));
    format!(
        "(()=>{{const k=Symbol.for('tasty.bedFocus');let s=document[k];\
         if(!s){{s=new CSSStyleSheet();Object.defineProperty(document,k,{{value:s}});\
         document.adoptedStyleSheets=[...document.adoptedStyleSheets,s];}}\
         s.replaceSync({rule});}})();"
    )
}

/// 스크립트로 상태를 두는 백엔드의 전달 기록. 문서 생성 스크립트 등록은 완료가 비동기로 오므로,
/// 마지막 요청의 등록이 끝나기 전에는 호스트 탐색을 붙잡아 두었다가 끝나면 내보낸다. 그래서 그 뒤
/// 생기는 문서는 첫 렌더부터 요청한 값이다. 마지막 요청의 어느 단계든 실패하면 기록을 지우고
/// [`Self::take_failed`]로 알려, 상위층이 같은 값을 다시 보내게 한다. `T`는 붙잡은 탐색이다.
#[cfg(any(windows, test))]
pub(crate) struct BedFocusDelivery<T> {
    /// 마지막으로 요청한 값. 실패하면 지운다.
    sent: Option<bool>,
    latest: u64,
    /// 마지막 요청의 문서 생성 스크립트 등록이 아직 끝나지 않았다.
    registering: bool,
    /// 남겨 둔 등록 id. 마지막 요청의 것 하나만 남긴다.
    held: Vec<String>,
    /// 등록이 끝나기를 기다리는 탐색. 나중 요청이 앞 것을 대신한다.
    deferred: Option<T>,
    failed: bool,
}

#[cfg(any(windows, test))]
impl<T> Default for BedFocusDelivery<T> {
    fn default() -> Self {
        Self {
            sent: None,
            latest: 0,
            registering: false,
            held: Vec::new(),
            deferred: None,
            failed: false,
        }
    }
}

#[cfg(any(windows, test))]
impl<T> BedFocusDelivery<T> {
    /// 값을 요청한다. 이미 그 값이면 `None`, 아니면 이 요청의 세대 번호다. 세대마다 문서 생성 스크립트를
    /// 새로 등록하고 지금 문서에 실행한 뒤, 완료를 같은 번호로 알린다.
    pub(crate) fn request(&mut self, focused: bool) -> Option<u64> {
        if self.sent == Some(focused) {
            return None;
        }
        self.sent = Some(focused);
        self.latest += 1;
        self.registering = true;
        self.failed = false;
        Some(self.latest)
    }

    /// 호스트 탐색을 지금 보낼지 정한다. 등록을 기다리는 중이면 붙잡고 `None`을 돌려준다.
    pub(crate) fn navigate(&mut self, navigation: T) -> Option<T> {
        if self.registering {
            self.deferred = Some(navigation);
            None
        } else {
            Some(navigation)
        }
    }

    /// `generation` 요청의 등록이 `id`로 끝났다. 지울 id와, 마지막 요청이면 붙잡았던 탐색을 돌려준다.
    pub(crate) fn registered(&mut self, generation: u64, id: String) -> (Vec<String>, Option<T>) {
        if generation != self.latest {
            return (vec![id], None);
        }
        self.registering = false;
        (
            std::mem::replace(&mut self.held, vec![id]),
            self.deferred.take(),
        )
    }

    /// `generation` 요청의 등록이 실패했다. 마지막 요청이면 실패로 두고 붙잡았던 탐색을 돌려준다.
    pub(crate) fn registration_failed(&mut self, generation: u64) -> Option<T> {
        if generation != self.latest {
            return None;
        }
        self.registering = false;
        self.fail();
        self.deferred.take()
    }

    /// `generation` 요청을 지금 문서에 적용하지 못했다. 마지막 요청일 때만 실패로 둔다.
    pub(crate) fn apply_failed(&mut self, generation: u64) {
        if generation == self.latest {
            self.fail();
        }
    }

    fn fail(&mut self) {
        self.failed = true;
        self.sent = None;
    }

    /// 마지막 요청이 실패했는지 돌려주고 표시를 지운다.
    pub(crate) fn take_failed(&mut self) -> bool {
        std::mem::replace(&mut self.failed, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_defines_only_the_property_with_an_empty_value() {
        assert_eq!(bed_focused_sheet(), ":root{--tasty-bed-focused: ;}");
    }

    #[test]
    fn the_rule_defines_the_property_when_focused_and_voids_it_otherwise() {
        assert_eq!(bed_state_rule(true), ":root:root{--tasty-bed-focused: ;}");
        assert_eq!(
            bed_state_rule(false),
            ":root:root{--tasty-bed-focused:initial;}"
        );
    }

    #[test]
    fn the_script_carries_the_rule_as_a_string_literal() {
        for focused in [true, false] {
            let script = bed_state_script(focused);
            let literal = serde_json::Value::String(bed_state_rule(focused)).to_string();
            assert!(
                script.contains(&format!("s.replaceSync({literal})")),
                "{script}"
            );
            assert!(script.contains("adoptedStyleSheets"), "{script}");
        }
    }

    type Delivery = BedFocusDelivery<&'static str>;

    #[test]
    fn a_navigation_waits_for_the_latest_registration() {
        let mut d = Delivery::default();
        assert_eq!(d.navigate("before"), Some("before"));
        let g = d.request(false).unwrap();
        assert_eq!(d.navigate("first"), None);
        assert_eq!(d.navigate("second"), None);
        assert_eq!(d.registered(g, "a".into()), (vec![], Some("second")));
        assert_eq!(d.navigate("after"), Some("after"));
    }

    #[test]
    fn an_older_registration_does_not_release_the_navigation() {
        let mut d = Delivery::default();
        let a = d.request(true).unwrap();
        let b = d.request(false).unwrap();
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.registered(a, "a".into()), (vec!["a".to_string()], None));
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.registered(b, "b".into()), (vec![], Some("load")));
        let c = d.request(true).unwrap();
        assert_eq!(d.registered(c, "c".into()), (vec!["b".to_string()], None));
    }

    #[test]
    fn a_failed_registration_releases_the_navigation_and_reports_once() {
        let mut d = Delivery::default();
        let g = d.request(false).unwrap();
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.registration_failed(g), Some("load"));
        assert!(d.take_failed());
        assert!(!d.take_failed());
        // 실패한 값은 기록에서 지워져 같은 값을 다시 요청할 수 있다.
        assert!(d.request(false).is_some());
    }

    #[test]
    fn only_a_failure_of_the_latest_request_counts() {
        let mut d = Delivery::default();
        let a = d.request(true).unwrap();
        let b = d.request(false).unwrap();
        d.apply_failed(a);
        assert_eq!(d.registration_failed(a), None);
        assert!(!d.take_failed());
        assert_eq!(d.request(false), None);
        d.apply_failed(b);
        assert!(d.take_failed());
        assert!(d.request(false).is_some());
    }
}
