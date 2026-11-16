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

/// 문서 생성 스크립트 등록의 순서 정리. 상태가 바뀔 때마다 새 스크립트를 등록하고 앞 것을 지운다.
/// 등록 완료는 비동기로 오므로, 마지막 요청의 등록만 남기고 나머지 id는 지울 목록으로 돌려준다.
#[cfg(any(windows, test))]
#[derive(Debug, Default)]
pub(crate) struct ScriptRegistrations {
    latest: u64,
    held: Vec<String>,
}

#[cfg(any(windows, test))]
impl ScriptRegistrations {
    /// 새 등록을 요청한다. 돌려준 번호를 완료 때 [`Self::added`]에 넘긴다.
    pub(crate) fn begin(&mut self) -> u64 {
        self.latest += 1;
        self.latest
    }

    /// `generation` 요청의 등록이 `id`로 끝났다. 지울 id 목록을 돌려준다.
    pub(crate) fn added(&mut self, generation: u64, id: String) -> Vec<String> {
        if generation == self.latest {
            std::mem::replace(&mut self.held, vec![id])
        } else {
            vec![id]
        }
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

    #[test]
    fn only_the_latest_registration_is_kept() {
        let mut regs = ScriptRegistrations::default();
        let a = regs.begin();
        assert!(regs.added(a, "a".into()).is_empty());
        // 앞 요청의 완료가 뒤 요청보다 늦게 오면 그 등록을 바로 지운다.
        let b = regs.begin();
        let c = regs.begin();
        assert_eq!(regs.added(c, "c".into()), vec!["a".to_string()]);
        assert_eq!(regs.added(b, "b".into()), vec!["b".to_string()]);
        let d = regs.begin();
        assert_eq!(regs.added(d, "d".into()), vec!["c".to_string()]);
    }
}
