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

/// 스크립트로 상태를 두는 백엔드의 전달 기록. 두 상태를 따로 센다.
///
/// - 등록: 그 뒤 생기는 문서에 실행할 문서 생성 스크립트. 완료가 비동기로 오므로 값이 바뀔 때마다 새
///   세대를 세우고, 그 세대의 등록이 끝나거나 실패할 때까지 호스트 탐색을 붙잡아 둔다. 그래서 그 뒤
///   생기는 문서는 첫 렌더부터 요청한 값이다.
/// - 적용: 지금 문서에 실행한 스크립트. 값이 바뀔 때마다 번호를 올린다.
///
/// 어느 쪽이든 마지막 것이 실패하면 [`Self::take_failed`]로 알린다. 상위층이 같은 값을 다시 요청하면
/// 실패한 쪽만 다시 한다. 적용만 실패했으면 등록 세대를 올리지 않고 장벽도 다시 세우지 않는다. 등록이
/// 실패했으면 그 사이 풀린 탐색의 문서에도 값이 없으므로, 새 세대로 다시 등록해 장벽을 세우고 지금
/// 문서에도 다시 적용한다. 재시도 횟수는 상위층이 정한다. `T`는 붙잡은 탐색이다.
#[cfg(any(windows, test))]
pub(crate) struct BedFocusDelivery<T> {
    /// 마지막으로 요청한 값.
    value: Option<bool>,
    /// 마지막 등록 세대.
    latest: u64,
    /// 호스트 탐색이 `latest` 등록을 기다린다.
    barrier: bool,
    /// 남겨 둔 등록 id. 마지막으로 끝난 등록 하나만 남긴다.
    held: Vec<String>,
    /// 등록이 끝나기를 기다리는 탐색. 받은 순서대로 모두 보낸다.
    deferred: Vec<T>,
    /// 마지막 적용 번호.
    applied: u64,
    registration_failed: bool,
    apply_failed: bool,
    /// 상위층에 아직 알리지 않은 실패.
    failed: bool,
    /// 이미 경고를 남겼다. 그 뒤 실패는 debug로 남긴다.
    warned: bool,
    /// view가 사라졌다. 그 뒤 오는 요청·탐색·완료는 아무것도 하지 않는다.
    closed: bool,
    /// 상위층에 아직 알리지 않은 문서 탐색 완료.
    loaded: bool,
    /// 문서를 그리던 process가 끝나 아직 새 문서가 실리지 않았다. 그동안 등록 완료가 오지 않으므로
    /// 탐색을 붙잡지 않는다.
    renderer_gone: bool,
}

/// [`BedFocusDelivery::request`]가 정한 할 일.
#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BedFocusWork {
    /// 이 세대로 문서 생성 스크립트를 등록한다.
    pub(crate) register: Option<u64>,
    /// 이 번호로 지금 문서에 스크립트를 실행한다.
    pub(crate) apply: Option<u64>,
}

#[cfg(any(windows, test))]
impl<T> Default for BedFocusDelivery<T> {
    fn default() -> Self {
        Self {
            value: None,
            latest: 0,
            barrier: false,
            held: Vec::new(),
            deferred: Vec::new(),
            applied: 0,
            registration_failed: false,
            apply_failed: false,
            failed: false,
            warned: false,
            closed: false,
            loaded: false,
            renderer_gone: false,
        }
    }
}

#[cfg(any(windows, test))]
impl<T> BedFocusDelivery<T> {
    /// 값을 요청한다. 새 값이면 등록과 적용을 모두 하고 장벽을 세운다. 같은 값이면 실패한 쪽만 다시
    /// 한다. 할 일이 없거나 view가 사라졌으면 `None`이다.
    pub(crate) fn request(&mut self, focused: bool) -> Option<BedFocusWork> {
        if self.closed {
            return None;
        }
        if self.value != Some(focused) {
            self.value = Some(focused);
            self.latest += 1;
            self.barrier = !self.renderer_gone;
            self.applied += 1;
            self.registration_failed = false;
            self.apply_failed = false;
            self.failed = false;
            return Some(BedFocusWork {
                register: Some(self.latest),
                apply: Some(self.applied),
            });
        }
        let registration_failed = std::mem::take(&mut self.registration_failed);
        let register = registration_failed.then(|| {
            self.latest += 1;
            self.barrier = !self.renderer_gone;
            self.latest
        });
        let apply_failed = std::mem::take(&mut self.apply_failed);
        let apply = (apply_failed || registration_failed).then(|| {
            self.applied += 1;
            self.applied
        });
        (register.is_some() || apply.is_some()).then_some(BedFocusWork { register, apply })
    }

    /// 호스트 탐색을 지금 보낼지 정한다. 등록을 기다리는 중이면 붙잡고 `None`을 돌려준다. view가
    /// 사라졌으면 버린다.
    pub(crate) fn navigate(&mut self, navigation: T) -> Option<T> {
        if self.closed {
            return None;
        }
        if self.barrier {
            self.deferred.push(navigation);
            None
        } else {
            Some(navigation)
        }
    }

    /// `generation` 등록이 `id`로 끝났다. 지울 id와, 마지막 세대면 붙잡았던 탐색을 받은 순서대로
    /// 돌려준다. view가 사라졌으면 아무것도 돌려주지 않는다.
    pub(crate) fn registered(&mut self, generation: u64, id: String) -> (Vec<String>, Vec<T>) {
        if self.closed {
            return (Vec::new(), Vec::new());
        }
        if generation != self.latest {
            return (vec![id], Vec::new());
        }
        self.barrier = false;
        (
            std::mem::replace(&mut self.held, vec![id]),
            std::mem::take(&mut self.deferred),
        )
    }

    /// `generation` 등록이 실패했다. 마지막 세대면 실패로 두고 붙잡았던 탐색을 돌려준다.
    pub(crate) fn registration_failed(&mut self, generation: u64) -> Vec<T> {
        if self.closed || generation != self.latest {
            return Vec::new();
        }
        self.barrier = false;
        self.registration_failed = true;
        self.failed = true;
        std::mem::take(&mut self.deferred)
    }

    /// `number` 적용이 실패했다. 마지막 적용일 때만 실패로 둔다.
    pub(crate) fn apply_failed(&mut self, number: u64) {
        if self.closed || number != self.applied {
            return;
        }
        self.apply_failed = true;
        self.failed = true;
    }

    /// 마지막 요청이 실패했는지 돌려주고 표시를 지운다.
    pub(crate) fn take_failed(&mut self) -> bool {
        std::mem::replace(&mut self.failed, false)
    }

    /// 실패를 경고로 남길지. view마다 첫 실패만 참이다.
    pub(crate) fn first_failure(&mut self) -> bool {
        !std::mem::replace(&mut self.warned, true)
    }

    /// 문서 탐색이 성공으로 끝났다. 상위층이 다시 보내기를 멈춘 값을 한 번 더 보낼 기회다.
    /// process가 끝난 뒤 실린 문서라면, 그 문서에 값이 실행됐는지 알 수 없으므로 적용을 실패로 두어
    /// 상위층이 다시 보내게 한다.
    pub(crate) fn document_loaded(&mut self) {
        if self.closed {
            return;
        }
        self.loaded = true;
        if std::mem::take(&mut self.renderer_gone) && self.value.is_some() {
            self.apply_failed = true;
            self.failed = true;
        }
    }

    /// 문서를 그리던 process가 끝났다. 새 문서가 실릴 때까지 등록 완료가 오지 않으므로 장벽을 내리고
    /// 붙잡았던 탐색을 받은 순서대로 돌려준다. 그 뒤 실리는 문서가 복구 탐색이다.
    pub(crate) fn renderer_ended(&mut self) -> Vec<T> {
        if self.closed {
            return Vec::new();
        }
        self.renderer_gone = true;
        self.barrier = false;
        std::mem::take(&mut self.deferred)
    }

    /// 지난번 뒤로 문서 탐색이 성공으로 끝났는지 돌려주고 표시를 지운다.
    pub(crate) fn take_loaded(&mut self) -> bool {
        std::mem::replace(&mut self.loaded, false)
    }

    /// view가 사라진다. 붙잡은 탐색을 버리고, 그 뒤 오는 요청·탐색·완료를 무시한다. 늦게 온 등록
    /// 완료가 닫힌 view에 탐색을 보내지 않게 한다.
    pub(crate) fn close(&mut self) {
        self.closed = true;
        self.barrier = false;
        self.deferred.clear();
    }

    #[cfg(test)]
    fn held_navigations(&self) -> usize {
        self.deferred.len()
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

    fn work(register: Option<u64>, apply: Option<u64>) -> Option<BedFocusWork> {
        Some(BedFocusWork { register, apply })
    }

    #[test]
    fn a_new_value_registers_and_applies_and_holds_navigations() {
        let mut d = Delivery::default();
        assert_eq!(d.navigate("before"), Some("before"));
        assert_eq!(d.request(false), work(Some(1), Some(1)));
        assert_eq!(d.navigate("first"), None);
        assert_eq!(d.navigate("second"), None);
        assert_eq!(
            d.registered(1, "a".into()),
            (vec![], vec!["first", "second"])
        );
        assert_eq!(d.navigate("after"), Some("after"));
        // 값이 그대로면 할 일이 없다.
        assert_eq!(d.request(false), None);
    }

    #[test]
    fn an_older_registration_does_not_release_the_navigation() {
        let mut d = Delivery::default();
        assert_eq!(d.request(true), work(Some(1), Some(1)));
        assert_eq!(d.request(false), work(Some(2), Some(2)));
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.registered(1, "a".into()), (vec!["a".to_string()], vec![]));
        assert_eq!(d.navigate("reload"), None);
        assert_eq!(
            d.registered(2, "b".into()),
            (vec![], vec!["load", "reload"])
        );
        assert_eq!(d.request(true), work(Some(3), Some(3)));
        assert_eq!(d.registered(3, "c".into()), (vec!["b".to_string()], vec![]));
    }

    #[test]
    fn retrying_a_failed_apply_keeps_the_registration_that_holds_the_navigation() {
        let mut d = Delivery::default();
        assert_eq!(d.request(false), work(Some(1), Some(1)));
        assert_eq!(d.navigate("load"), None);
        d.apply_failed(1);
        assert!(d.take_failed());
        // 같은 값의 적용만 다시 한다. 등록 세대를 올리지 않는다.
        assert_eq!(d.request(false), work(None, Some(2)));
        assert_eq!(d.registered(1, "a".into()), (vec![], vec!["load"]));
        assert_eq!(d.request(false), None);
    }

    #[test]
    fn a_failed_registration_releases_the_navigation_and_is_retried_with_an_apply() {
        let mut d = Delivery::default();
        assert_eq!(d.request(false), work(Some(1), Some(1)));
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.registration_failed(1), vec!["load"]);
        assert_eq!(d.navigate("between"), Some("between"));
        assert!(d.take_failed());
        assert!(!d.take_failed());
        // 풀린 문서에도 값이 없으므로 다시 등록하며 지금 문서에도 다시 적용한다.
        assert_eq!(d.request(false), work(Some(2), Some(2)));
        assert_eq!(d.navigate("reload"), None);
        assert_eq!(d.registered(2, "b".into()), (vec![], vec!["reload"]));
        assert_eq!(d.request(false), None);
    }

    #[test]
    fn only_a_failure_of_the_latest_request_counts() {
        let mut d = Delivery::default();
        assert_eq!(d.request(true), work(Some(1), Some(1)));
        assert_eq!(d.request(false), work(Some(2), Some(2)));
        d.apply_failed(1);
        assert_eq!(d.registration_failed(1), Vec::<&str>::new());
        assert!(!d.take_failed());
        assert_eq!(d.request(false), None);
        d.apply_failed(2);
        assert!(d.take_failed());
        assert_eq!(d.request(false), work(None, Some(3)));
    }

    #[test]
    fn a_closed_view_drops_held_navigations_and_ignores_late_completions() {
        let mut d = Delivery::default();
        assert_eq!(d.request(false), work(Some(1), Some(1)));
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.navigate("reload"), None);
        d.close();
        assert_eq!(d.held_navigations(), 0);
        assert_eq!(d.registered(1, "a".into()), (vec![], vec![]));
        assert_eq!(d.registration_failed(1), Vec::<&str>::new());
        d.apply_failed(1);
        assert!(!d.take_failed());
        assert_eq!(d.navigate("late"), None);
        assert_eq!(d.held_navigations(), 0);
        assert_eq!(d.request(true), None);
    }

    #[test]
    fn a_completed_navigation_is_reported_once() {
        let mut d = Delivery::default();
        assert!(!d.take_loaded());
        d.document_loaded();
        d.document_loaded();
        assert!(d.take_loaded());
        assert!(!d.take_loaded());
        d.close();
        d.document_loaded();
        assert!(!d.take_loaded());
    }

    #[test]
    fn an_ended_renderer_releases_the_navigation_and_the_next_document_is_applied_again() {
        let mut d = Delivery::default();
        assert_eq!(d.request(false), work(Some(1), Some(1)));
        // process가 끝난 뒤 값이 바뀌어 등록이 끝나지 않은 채 남는다.
        assert_eq!(d.request(true), work(Some(2), Some(2)));
        assert_eq!(d.navigate("load"), None);
        assert_eq!(d.renderer_ended(), vec!["load"]);
        // 복구 탐색과 그 사이의 새 값은 붙잡지 않는다.
        assert_eq!(d.navigate("reload"), Some("reload"));
        assert_eq!(d.request(false), work(Some(3), Some(3)));
        assert_eq!(d.navigate("again"), Some("again"));
        // 새 문서가 실리면 적용을 다시 하게 하고, 그 뒤로는 다시 장벽을 세운다.
        d.document_loaded();
        assert!(d.take_failed());
        assert_eq!(d.request(false), work(None, Some(4)));
        assert_eq!(d.request(true), work(Some(4), Some(5)));
        assert_eq!(d.navigate("later"), None);
    }

    #[test]
    fn only_the_first_failure_of_a_view_is_a_warning() {
        let mut d = Delivery::default();
        assert!(d.first_failure());
        assert!(!d.first_failure());
        assert!(!d.first_failure());
    }
}
