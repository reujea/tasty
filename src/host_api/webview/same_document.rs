//! WebView2 같은 문서 fragment 이동의 탐색 기록.
//! WebView2는 이 이동에 `NavigationStarting`을 올리지 않는다. `SourceChanged`는 fragment가 없던 문서의 첫
//! 이동에만 왔고, `HistoryChanged`는 이동마다 왔다(2026-11-08 Windows 11, WebView2 측정). 그래서
//! `HistoryChanged`마다 페이지에서 주소를 읽어, 같은 문서에서 fragment만 바뀐 경우를 기록한다.
//! `load_html`(NavigateToString) 문서는 `Source`가 비어 있어 주소를 `location.href`로 읽는다.
//! 규칙은 docs/design/systems/webview.md "탐색" 절에 있다.

use super::PendingNavigation;

/// 문서가 만들어질 때 그 문서의 첫 주소를 기억한다. 문서마다 새 전역이라 새 문서는 늘 이 값부터 시작한다.
pub(crate) const INIT_SCRIPT: &str = "window[Symbol.for('tasty.sameDocumentHref')]=location.href;";

/// 직전에 기억한 주소와 지금 주소를 비교해, 같은 문서(`#` 앞이 같음)에서 fragment만 바뀌었으면 지금 주소를,
/// 아니면 null을 돌려주고 지금 주소를 기억한다. 기억이 없으면(초기화 스크립트가 돌지 않은 문서) 판단하지
/// 않는다. 페이지가 기억한 값을 바꿔도 돌려주는 주소는 페이지가 덮어쓸 수 없는 `location`의 값이다.
pub(crate) const HREF_SCRIPT: &str = "(function(){\
var k=Symbol.for('tasty.sameDocumentHref'),p=window[k],h=location.href;window[k]=h;\
function b(u){var i=u.indexOf('#');return i<0?u:u.slice(0,i);}\
return typeof p==='string'&&p!==h&&b(p)===b(h)?h:null;})()";

/// `ExecuteScript(HREF_SCRIPT)`의 결과(JSON)를 탐색 기록으로 바꾼다. 문자열이 아니거나 비었으면 버린다.
/// 같은 주소가 이미 큐에 있으면 다시 쌓지 않는다.
pub(crate) fn from_href_result(
    json: &str,
    user_gesture: bool,
    queued: &[PendingNavigation],
) -> Option<PendingNavigation> {
    let url: String = serde_json::from_str(json).ok()?;
    if url.is_empty() || queued.iter().any(|nav| nav.url == url) {
        return None;
    }
    Some(PendingNavigation { url, user_gesture })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn href_result_becomes_a_navigation_with_the_click_gesture() {
        let json = r##""about:blank#tasty-nav:browse:1794108661521""##;
        assert_eq!(
            from_href_result(json, true, &[]),
            Some(PendingNavigation {
                url: "about:blank#tasty-nav:browse:1794108661521".into(),
                user_gesture: true,
            })
        );
        assert_eq!(
            from_href_result(r#""about:blank#x""#, false, &[]).map(|nav| nav.user_gesture),
            Some(false)
        );
    }

    #[test]
    fn non_string_or_empty_results_are_dropped() {
        for json in ["null", "", "42", r#""""#, "{\"href\":\"a\"}", "not json"] {
            assert_eq!(from_href_result(json, true, &[]), None, "{json}");
        }
    }

    #[test]
    fn an_already_queued_address_is_not_queued_twice() {
        let queued = [PendingNavigation {
            url: "about:blank#tasty-nav:browse:1".into(),
            user_gesture: true,
        }];
        assert_eq!(
            from_href_result(r#""about:blank#tasty-nav:browse:1""#, false, &queued),
            None
        );
        assert!(from_href_result(r#""about:blank#tasty-nav:browse:2""#, false, &queued).is_some());
    }

    /// 두 스크립트를 node 로 실행해 판정을 확인한다. 스크립트는 OS 와 무관하므로 한 OS 에서만 돌린다.
    /// node 는 Linux 시험 잡이 준비한다(docs/dev-guide/ci-gates.md "단위 시험의 node").
    /// 개발 환경에 node 가 없으면 건너뛰지 않고 실패한다.
    #[cfg(target_os = "linux")]
    #[test]
    fn href_script_reports_only_fragment_moves_within_one_document() {
        const DRIVER: &str = r#"
const [init, href, steps] = process.argv.slice(1);
globalThis.window = globalThis;
globalThis.location = { href: '' };
const out = [];
for (const s of JSON.parse(steps)) {
  if (s.doc !== undefined) {
    delete window[Symbol.for('tasty.sameDocumentHref')];
    location.href = s.doc;
    if (s.init) (0, eval)(init);
    continue;
  }
  location.href = s.go;
  out.push((0, eval)(href));
}
process.stdout.write(JSON.stringify(out));
"#;
        let steps = serde_json::json!([
            {"doc": "about:blank", "init": true},
            {"go": "about:blank"},
            {"go": "about:blank#tasty-nav:browse:1"},
            {"go": "about:blank#tasty-nav:browse:2"},
            {"go": "about:blank#tasty-nav:browse:2"},
            {"go": "https://example.test/#x"},
            {"doc": "about:blank", "init": false},
            {"go": "about:blank#tasty-nav:browse:3"},
        ]);
        let output = std::process::Command::new("node")
            .args(["-e", DRIVER, INIT_SCRIPT, HREF_SCRIPT, &steps.to_string()])
            .output()
            .expect("node 를 실행하지 못했다. 이 시험은 node 가 필요하다. 설치해 PATH 에 넣는다");
        assert!(
            output.status.success(),
            "node 실패: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let got: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON 출력");
        assert_eq!(
            got,
            serde_json::json!([
                null,
                "about:blank#tasty-nav:browse:1",
                "about:blank#tasty-nav:browse:2",
                null,
                null,
                null
            ]),
            "첫 주소·같은 주소·다른 문서·기억 없는 문서는 null, 같은 문서의 fragment 이동만 주소"
        );
    }

    #[test]
    fn both_scripts_share_one_document_key() {
        let key = "Symbol.for('tasty.sameDocumentHref')";
        assert!(INIT_SCRIPT.contains(key));
        assert!(HREF_SCRIPT.contains(key));
    }
}
