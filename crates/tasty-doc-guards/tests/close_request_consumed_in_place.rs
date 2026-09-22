//! `MainView::request_close` 가 세운 플래그를 **세운 그 App 경로 안에서** 치우는지
//! 소스 구조로 고정하는 가드.
//!
//! `request_close` 는 `close_requested` 플래그만 세운다. 그 플래그를 읽어 창을 치우는
//! 자리는 원래 `App::dispatch_window_event_to_view` 의 handler 직후 하나뿐이었고, 그래서
//! 창 이벤트 **밖**(`about_to_wait`)에서 MainView 가 마지막 워크스페이스를 닫으면
//! 플래그는 다음 창 이벤트까지 서 있었다. 그 사이 `engine.workspaces` 는 비어 있고, 다음
//! 창 이벤트가 `RedrawRequested` 면 렌더 경로(`forward_egui_mesh_context` →
//! `surface_regions` → `active_workspace`)가 빈 Vec 을 인덱싱해 죽는다. 다른 이벤트가
//! 먼저 오면 그 dispatch 가 치워서 살아남는다 — 확률로 죽는 레이스였다(Linux Xvfb 실측:
//! webview 에 포커스를 둔 채 `close_workspace` 단축키로 마지막 워크스페이스를 닫는 시나리오).
//!
//! 이 레이스는 GPU·창·OS 이벤트 순서가 있어야 재현되므로 단위 시험으로 못 잡는다. 그래서
//! 두 사실을 소스로 박는다.
//!
//! 1. **생산자 명부** — `.request_close()` 를 부르는 파일 집합이 [`PRODUCERS`] 와 같다.
//!    새 파일이 플래그를 세우기 시작하면 여기서 멈춰, 그 경로가 창 이벤트 안에서 도는지
//!    분류하게 한다.
//! 2. **App 진입점의 짝** — `src/app/` 에서 MainView 의 닫기 가능 진입점([`ENTRIES`])을
//!    부르는 자리마다, **같은 함수 안에서 그 뒤에** `self.close_self_requesting_windows()`
//!    가 온다. 창 이벤트 dispatch 경로(`w.handle_event(..)` 직후의 단일 창 정리)는 이
//!    진입점들을 직접 부르지 않으므로 이 규칙 밖이다.
//!
//! **이 가드가 못 보는 것**: 진입점 집합의 완전성. `ENTRIES` 는 [`PRODUCERS`] 의 호출부를
//! 손으로 거슬러 올라가 만든 목록이고(`handle_shortcut` · `dispatch_action_by_id` ·
//! `handle_double_tap_shortcut` · `poll_pending_native_menu`), 전이 도달 가능성을 기계로
//! 재는 채널은 없다. 생산자 명부가 바뀔 때 그 목록을 다시 거슬러 올라가라.
//!
//! **안 고른 대안**: `dispatch_window_event_to_view` 진입 시 선 플래그를 먼저 소비하는 것 —
//! 창 이벤트 앞은 막지만 같은 `about_to_wait` 뒤쪽·다음 `process_ipc` 가 빈 창을 볼 틈이
//! 남는다. **재검토 조건**: 창 이벤트 밖 MainView 진입점이 늘어 이 짝 규칙의 유지 비용이
//! 커지면, 짝 대신 App 이 `about_to_wait` 끝에서 한 번 전역 sweep 하는 형태로 바꾼다.

use std::path::PathBuf;
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};

/// `.request_close()` 를 코드로 부르는 파일 전부(레포 상대). 정의(`fn request_close`)가
/// 있는 `src/view/main.rs` 는 호출이 아니므로 안 든다.
const PRODUCERS: &[&str] = &[
    "src/adapters/ui/input/shortcuts/dispatch.rs",
    "src/adapters/ui/input/shortcuts/double_tap.rs",
    "src/view/main/redraw.rs",
];

/// 위 생산자에 닿는 MainView 진입점 — App 이 창 이벤트 밖에서 부를 수 있는 것.
const ENTRIES: &[&str] = &[
    ".handle_shortcut(",
    ".dispatch_action_by_id(",
    ".handle_double_tap_shortcut(",
    ".poll_pending_native_menu(",
];

const SWEEP: &str = "self.close_self_requesting_windows()";

/// 현재 `src/app/` 안의 진입점 호출 자리 수. 0 으로 떨어지면 규칙 2 가 아무것도 안 잰
/// 초록이 되므로 하한으로 박는다(실측: `webview_keys.rs` 의 `handle_shortcut` 하나 +
/// `event_handler.rs` 의 `poll_pending_native_menu` 하나).
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
        "`.request_close()` 를 부르는 파일 집합이 바뀌었다. 새 자리가 창 이벤트 dispatch \
         **밖**(about_to_wait · user_event · IPC)에서 돌 수 있으면, 그 App 경로가 같은 \
         함수 안에서 `close_self_requesting_windows()` 를 부르게 하고 ENTRIES 를 갱신하라. \
         그러지 않으면 워크스페이스가 빈 창이 다음 RedrawRequested 를 받는다."
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
        "src/app 의 진입점 호출 자리 수가 {sites} 다(기대 {ENTRY_SITES}). 늘었으면 새 자리도 \
         위 규칙을 지키는지 보고 상수를 맞춰라. 0 이면 이 시험은 아무것도 안 잰다."
    );
}
