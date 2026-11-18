//! MainView가 닫기를 요청한 뒤 같은 App 진입 경로에서 창을 정리하는지 소스 형태로 확인한다.
//! 다음 창 이벤트까지 미루면 빈 워크스페이스가 redraw나 IPC에 노출될 수 있다.
//! request_close 호출 파일과 창 이벤트 밖 진입점마다 뒤따르는 정리 호출을 대조한다.
//! 창 이벤트 dispatch의 단일 창 정리는 이 진입점 목록 밖이다.
//! 진입점의 완전성과 전이 호출은 판정하지 못한다. 생산자가 바뀌면 호출 경로를 직접 검토해야 한다.
//! 창 이벤트 밖 진입점이 늘어 목록 유지가 어려워지면 about_to_wait 끝에서 전체를 정리하는 방식을 재검토한다.

use std::path::PathBuf;
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};

/// request_close의 정의가 아닌 호출 파일을 등록한다.
const PRODUCERS: &[&str] = &[
    "src/adapters/ui/input/shortcuts/dispatch.rs",
    "src/adapters/ui/input/shortcuts/double_tap.rs",
    "src/view/main/redraw.rs",
];

/// 위 생산자에 닿는 MainView 진입점 — App 이 창 이벤트 밖에서 부를 수 있는 것.
const ENTRIES: &[&str] = &[
    ".handle_received_shortcut(",
    ".dispatch_action_by_id(",
    ".handle_double_tap_shortcut(",
    ".poll_pending_native_menu(",
];

const SWEEP: &str = "self.close_self_requesting_windows()";

/// App 진입점 수. 수집이 비어 정리 호출 검사가 생략되는 경우를 막는다.
const ENTRY_SITES: usize = 2;

fn sources(roots: &[&str]) -> Vec<(PathBuf, String)> {
    rust_sources(&tasty_doc_guards::repo_root(), roots)
        .into_iter()
        .map(|(p, text)| (p, mask_non_code(&text)))
        .collect()
}

#[test]
fn request_close_producers_are_the_known_set() {
    let mut found: Vec<String> = sources(&["src"])
        .into_iter()
        .filter(|(_, code)| code.contains(".request_close()"))
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut known: Vec<String> = PRODUCERS.iter().map(|s| s.to_string()).collect();
    known.sort();
    assert_eq!(
        found, known,
        "request_close 호출 파일이 달라졌다. 창 이벤트 밖에서 실행될 수 있다면 해당 App 함수가 뒤이어 close_self_requesting_windows를 호출하는지 확인하고 ENTRIES를 갱신한다."
    );
}

/// `offset` 을 품은 함수 본문의 끝 — 다음 4칸 들여쓰기 `fn` 선언(또는 파일 끝) 직전.
fn enclosing_fn_end(code: &str, offset: usize) -> usize {
    let rest = &code[offset..];
    [
        "\n    fn ",
        "\n    pub fn ",
        "\n    pub(crate) fn ",
        "\n    pub(super) fn ",
    ]
    .iter()
    .filter_map(|h| rest.find(h))
    .min()
    .map_or(code.len(), |i| offset + i)
}

#[test]
fn app_entry_calls_are_followed_by_the_sweep() {
    let mut sites = 0usize;
    let mut missing: Vec<String> = Vec::new();
    for (path, code) in sources(&["src/app"]) {
        for entry in ENTRIES {
            for (at, _) in code.match_indices(entry) {
                sites += 1;
                let end = enclosing_fn_end(&code, at);
                if !code[at..end].contains(SWEEP) {
                    let line = code[..at].lines().count();
                    missing.push(format!("{}:{line} `{entry}`", path.display()));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "창 이벤트 밖에서 MainView 의 닫기 가능 진입점을 부르고 같은 함수 안에서 \
         `{SWEEP}` 를 안 부르는 자리: {missing:?}"
    );
    assert_eq!(
        sites, ENTRY_SITES,
        "App 진입점 호출이 {sites}곳으로 기준 {ENTRY_SITES}와 다르다. 수집 누락과 새 호출 경로를 확인하고 기준값을 갱신한다."
    );
}
