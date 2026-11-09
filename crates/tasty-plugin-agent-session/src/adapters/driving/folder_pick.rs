//! 폴더 고르기 — host `file_picker` popup 을 빌려 쓴다(`docs/features/native-file-picker/index.md`
//! § 플러그인의 피커 요청).
//!
//! `mode: "folder"` 로 열면 확정 대상이 고른 하위 폴더, 없으면 **지금 보고
//! 있는 폴더**이고, 파일 행은 보이되 고를 수 없다. 응답 `paths[0]` 이 그
//! 폴더 경로다. host 는 모르는 키를 조용히 버리므로 모드 이름이 틀리면
//! 파일 피커로 열린다.
//!
//! 단추 클릭이 낸 navigation URL 을 `user_navigation_url` 로 돌려준다. host 가
//! 그것을 사용자 조작으로 확인해야 피커가 포커스를 가진 popup 으로 열린다.
//! 없으면 passive popup 이 되고, 키를 받는 이 webview 가 피커를 덮는다.
//!
//! popup instance 는 안 싣는다 — `owner_popup_instance` 는 선택이고,
//! "popup 밖(surface 위젯 등)에서 호출하면 생략한다"가 host 의 계약이다.
//! 우리는 webview surface 에서 부르므로 부모 popup 이 없다.

use serde_json::{Value, json};

/// host 가 확정/취소를 밀어 주는 이벤트 이름.
/// `src/app/dispatch/file_picker.rs::FILE_PICKER_RESULT_EVENT` 와 같은 값 —
/// 공유 crate 가 없어 markdown plugin 과 같은 방식으로 복제한다.
pub const FILE_PICKER_RESULT_EVENT: &str = "file_picker.result";

/// `file_picker.result` payload — 호스트가 확정 · 취소 모두에 채우는 세 필드.
///
/// 손으로 읽는다: 이 크레이트는 `serde_json` 만 들고 derive 용 `serde` 는
/// 안 든다. 필드가 셋뿐이라 의존을 늘릴 값이 없다.
pub struct ResultWire {
    pub request_id: u64,
    pub paths: Vec<String>,
    pub cancelled: bool,
}

impl ResultWire {
    /// 모양이 틀리면 `None` — host 가 보낸 것이라도 믿고 풀지 않는다.
    /// `request_id` 가 없으면 **어느 요청의 답인지 모른다**: 그것만은
    /// 기본값으로 때울 수 없어 실패로 돌린다.
    pub fn parse(v: &Value) -> Option<Self> {
        Some(Self {
            request_id: v.get("request_id")?.as_u64()?,
            paths: v
                .get("paths")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            // 없으면 확정으로 본다 — 취소는 반드시 실려 온다(호스트는
            // 확정·취소 **둘 다** 세 필드를 채운다).
            cancelled: v.get("cancelled").and_then(Value::as_bool).unwrap_or(false),
        })
    }
}

/// `file_picker.trigger` 파라미터. 호출과 떼어 두어 시험이 wire 모양을 본다.
pub fn trigger_params(surface_id: u32, start_dir: Option<&str>, nav_url: &str) -> Value {
    json!({
        "mode": "folder",
        "user_navigation_url": nav_url,
        // 필터를 **안 건다** — 우리가 쓰는 것은 파일이 아니라 그 폴더다.
        "filters": Vec::<String>::new(),
        "start_dir": start_dir,
        "origin_surface_id": surface_id,
    })
}

/// 고른 **폴더**가 작업 디렉토리다(`mode: "folder"`).
///
/// 취소·빈 결과는 `None`. 빈 문자열도 `None` — 그것을 cwd 로 넘기면
/// *"설정을 따른다"* 와 구분되지 않는다.
///
/// **옛 host 는 파일 경로를 준다**(`folder` 모드를 모르는 판). 그때도
/// 망가지지 않게 **폴더가 아니면 부모로 접는다** — 종전 우회와 같은
/// 동작이라 옛 host 에 붙어도 사용자는 차이를 못 느낀다.
pub fn dir_from_result(r: &ResultWire) -> Option<String> {
    if r.cancelled {
        return None;
    }
    let raw = r.paths.first()?;
    let p = std::path::Path::new(raw);
    // **`is_dir()` 로 가르지 않는다**(2026-09-28 정정).
    //
    // 우리는 `mode: "folder"` 로 요청했고 host 는 그 답으로 **폴더
    // 경로**를 준다 — 그것이 계약이다. 그런데 `is_dir()` 은 이 프로세스
    // 에서 stat 을 때리는 것이라, 실패하면(권한·경합·원격 경로) 조용히
    // false 가 되어 **부모로 한 칸 올라간다**. 사용자는 고른 폴더가
    // 아니라 그 위가 들어가는 것을 보고 *"저장이 안 된다"* 고 읽는다.
    //
    // 원격 브라우징이면 이 경로는 **이 기계에 아예 없다** — stat 이
    // 반드시 실패하고, 그때 부모로 접는 것은 명백히 틀린 동작이다.
    //
    // 옛 host(파일 경로를 주는 판) 폴백은 **끝이 파일처럼 보일 때만**
    // 쓴다. 확장자가 있으면 파일로 보고 부모를 쓴다 — 폴더 이름에 점이
    // 있는 경우를 위해 stat 은 **보조로만** 본다.
    let looks_like_file = p.extension().is_some_and(|e| !e.is_empty()) && !p.is_dir();
    let chosen = if looks_like_file { p.parent()? } else { p };
    let s = chosen.to_str()?;
    (!s.is_empty()).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(paths: &[&str], cancelled: bool) -> ResultWire {
        ResultWire {
            request_id: 1,
            paths: paths.iter().map(|s| (*s).to_string()).collect(),
            cancelled,
        }
    }

    /// **없는 경로여도 고른 폴더 그대로 쓴다**(2026-09-28 회귀).
    ///
    /// 종전에는 `is_dir()` 로 갈랐는데, 그 stat 이 실패하면(원격 경로·
    /// 권한·경합) 조용히 **부모로 한 칸 올라갔다**. 사용자에게는 고른
    /// 폴더가 안 잡히는 것으로 보인다 — *"저장이 안 된다"*.
    #[test]
    fn a_directory_that_does_not_exist_here_is_still_taken_as_chosen() {
        // 이 기계에 없는 경로 — 원격 브라우징이 정확히 이 모양이다.
        let p = "/nowhere-on-this-host/deep/project";
        assert_eq!(
            dir_from_result(&wire(&[p], false)),
            Some(p.to_string()),
            "없는 폴더라고 부모로 올라갔다 — 고른 것을 그대로 써야 한다"
        );
    }

    /// 확장자가 붙은 것은 **옛 host 의 파일 답**으로 보고 부모를 쓴다.
    #[test]
    fn an_old_host_file_answer_still_falls_back_to_the_parent() {
        assert_eq!(
            dir_from_result(&wire(&["/w/proj/a.md"], false)),
            Some("/w/proj".to_string())
        );
    }

    /// 고른 파일의 **부모 폴더**가 작업 디렉토리다.
    #[test]
    fn the_parent_of_the_chosen_file_is_the_directory() {
        assert_eq!(
            dir_from_result(&wire(&["/w/proj/README.md"], false)),
            Some("/w/proj".into())
        );
    }

    /// 취소하면 **아무것도 안 바꾼다** — 빈 값으로 덮으면 사용자가 이미
    /// 쳐 둔 경로가 사라진다.
    #[test]
    fn cancelling_yields_nothing() {
        assert_eq!(dir_from_result(&wire(&["/w/proj/a.md"], true)), None);
    }

    /// 아무것도 안 고르면 `None`. 취소 플래그가 없어도 마찬가지다.
    #[test]
    fn an_empty_selection_yields_nothing() {
        assert_eq!(dir_from_result(&wire(&[], false)), None);
    }

    /// 부모가 없는 경로는 `None` — 빈 문자열을 cwd 로 넘기면
    /// *"agent 설정을 따른다"* 와 구분되지 않는다.
    #[test]
    fn a_path_without_a_usable_parent_yields_nothing() {
        // **`/` 는 이제 정상이다**(folder 모드). 종전에는
        // *"파일을 고르고 부모를 쓴다"* 였으므로 부모가 없는 `/` 가 결함
        // 이었는데, 지금은 `/` 를 **고를 수 있는 폴더**로 받는다.
        assert_eq!(dir_from_result(&wire(&["/"], false)), Some("/".to_string()));
        // 폴더도 아니고 부모도 없는 것은 여전히 거른다(옛 host 폴백 경로).
        assert_eq!(dir_from_result(&wire(&["bare.txt"], false)), None);
    }

    /// wire 를 손으로 읽는다 — 셋 다 제자리에 온다.
    #[test]
    fn a_well_formed_payload_parses() {
        let v = json!({"request_id": 9, "paths": ["/w/a.md"], "cancelled": false});
        let r = ResultWire::parse(&v).expect("못 읽었다");
        assert_eq!(r.request_id, 9);
        assert_eq!(r.paths, vec!["/w/a.md".to_string()]);
        assert!(!r.cancelled);
    }

    /// `request_id` 가 없으면 **버린다** — 어느 요청의 답인지 모르면
    /// 엉뚱한 탭의 폴더를 바꿀 수 있다.
    #[test]
    fn a_payload_without_a_request_id_is_refused() {
        assert!(ResultWire::parse(&json!({"paths": ["/w/a.md"]})).is_none());
        assert!(
            ResultWire::parse(&json!({"request_id": "9"})).is_none(),
            "문자열 id 가 읽혔다"
        );
    }

    /// 나머지는 빠져도 읽는다 — 취소 플래그가 없으면 확정이다.
    #[test]
    fn missing_optional_fields_fall_back() {
        let r = ResultWire::parse(&json!({"request_id": 3})).expect("못 읽었다");
        assert!(r.paths.is_empty());
        assert!(!r.cancelled);
    }

    /// 여럿 고르면 **첫 번째**를 쓴다 — 폴더는 하나만 정해진다.
    #[test]
    fn with_several_files_the_first_one_decides() {
        let r = wire(&["/w/a/1.md", "/w/b/2.md"], false);
        assert_eq!(dir_from_result(&r), Some("/w/a".into()));
    }

    /// **확장자 필터를 걸지 않는다.** 걸면 그 확장자 파일이 없는
    /// 폴더를 고를 수 없다 — 고르려는 것이 파일이 아니라 폴더라서다.
    #[test]
    fn no_extension_filter_is_requested() {
        let p = trigger_params(7, Some("/w"), "");
        assert_eq!(p["filters"].as_array().map(Vec::len), Some(0));
        assert_eq!(p["start_dir"], "/w");
        assert_eq!(p["origin_surface_id"], 7);
    }

    /// popup 부모를 **안 신고한다** — 우리는 surface 에서 부른다.
    /// 없는 부모를 신고하면 host 가 세울 스택이 틀어진다.
    #[test]
    fn no_owner_popup_is_claimed() {
        let p = trigger_params(7, None, "");
        assert!(
            p.get("owner_popup_instance").is_none(),
            "없는 popup 부모를 신고했다"
        );
        assert!(p["start_dir"].is_null());
    }
}

#[cfg(test)]
mod manifest_tests {
    /// **`file_picker.trigger` 는 `fs.read` 를 요구한다** — 선언이 없으면
    /// 호스트가 **조용히 거절한다**(실측 2026-09-22).
    ///
    /// # 왜 이 가드인가
    ///
    /// 거절은 로그에만 남는다:
    ///
    /// ```text
    /// ipc permission denied: 'com.tasty.agent-session' missing permission
    /// 'fs.read' for method 'file_picker.trigger'
    /// ```
    ///
    /// **plugin 쪽에는 아무 신호가 없다** — `host.call` 이 오류를 내지만
    /// 사용자에게는 *"단추가 안 먹는다"* 로만 보인다. 코드가 아무리 맞아도
    /// 매니페스트 한 줄이 빠지면 기능이 통째로 죽는다.
    ///
    /// 이름과 달리 우리는 **파일을 읽지 않는다** — 쓰는 것은 고른 파일의
    /// 경로(그 부모)뿐이다. 호스트가 피커 전체를 이 권한으로 게이트한다.
    #[test]
    fn the_manifest_declares_the_permission_the_picker_needs() {
        let m = include_str!("../../../tasty-plugin.toml");
        // 주석에도 이 문자열이 있으므로 **선언부**를 본다.
        let decl: Vec<&str> = m
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .collect();
        assert!(
            decl.iter().any(|l| l.starts_with("\"fs.read\"")),
            " `fs.read` 선언이 없다 — `file_picker.trigger` 가 조용히 거절된다"
        );
    }
}

#[cfg(test)]
mod host_contract_tests {
    use super::*;

    /// 호스트 `FilePickerTriggerReq` 가 받는 필드 이름·타입과 맞는지 본다.
    ///
    /// 호스트 선언(`src/adapters/ipc/handler/file_picker.rs` 의 `FilePickerTriggerReq`):
    /// `filters: Vec<String>` · `owner_popup_instance: Option<u64>` ·
    /// `start_dir: Option<String>` · `origin_surface_id: Option<u32>` ·
    /// `mode: "open" | "save" | "folder"` · `user_navigation_url: Option<String>`.
    ///
    /// **이름이 하나 틀리면 그 필드는 조용히 기본값이 된다** — serde 는
    /// 모르는 키를 버리고 오류를 안 낸다. `start_dir` 를 `startDir` 로
    /// 적었다면 피커는 **늘 홈에서 열리고** 아무도 못 알아챈다.
    #[test]
    fn the_wire_matches_what_the_host_declares() {
        let p = trigger_params(7, Some("/w/proj"), "agent-browse=1");
        let o = p.as_object().expect("객체가 아니다");

        // 우리가 싣는 키는 다섯이다 — 호스트가 아는 이름이어야 한다.
        let mut keys: Vec<&str> = o.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "filters",
                "mode",
                "origin_surface_id",
                "start_dir",
                "user_navigation_url"
            ],
            "호스트가 모르는 키를 싣거나 이름이 틀렸다"
        );
        // **폴더 모드를 켜고 있나** — 이 칸이 빠지면 host 가 파일
        // 피커를 열고, 사용자는 다시 "아무 파일이나 고르세요" 로 돌아간다.
        assert_eq!(p["mode"], "folder");
        assert_eq!(
            p["user_navigation_url"], "agent-browse=1",
            "클릭 URL 이 빠지면 피커가 passive 로 열려 webview 에 덮인다"
        );

        // 타입도 맞아야 한다 — 문자열로 싣는 surface id 는 거절된다.
        assert!(p["filters"].is_array(), "filters 는 배열이어야 한다");
        assert!(p["start_dir"].is_string(), "start_dir 는 문자열이어야 한다");
        assert!(
            p["origin_surface_id"].is_u64(),
            "origin_surface_id 는 수여야 한다 — 문자열이면 파싱이 실패한다"
        );
    }

    /// `start_dir` 가 없으면 **null 이 아니라 빠져야** 하나 — 호스트는
    /// `Option<String>` + `serde(default)` 라 **둘 다 받는다**. 다만
    /// `null` 을 실으면 "홈에서 연다" 는 뜻이 된다(생략과 같다).
    #[test]
    fn an_absent_start_dir_is_null_and_that_means_home() {
        let p = trigger_params(7, None, "");
        assert!(p["start_dir"].is_null());
    }
}
