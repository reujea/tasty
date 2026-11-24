//! 그리기 경로(`src/adapters/ui`, `src/gfx`)에서 지금 포커스(`focused_surface_id`)와 지금 키 게이트
//! (`window_keys_to_overlay`·`keyboard_overlay_open`·`window_holds_terminal_keys`)를 읽는 호출 자리를 명부로 고정한다.
//! 그리기는 끊어 둔 입력 몫 하나를 그리므로, 그 몫의 키를 받을지는 그 몫의 포커스 노드(`MainViewState::pass_key_node`,
//! `pass_focus_node`)로 정한다. 지금 값은 뒤 몫의 사건이 이미 옮긴 값일 수 있다. 명부의 자리는 키 판정이 아니라
//! 표시·명령 대상·포커스 경로 계산이라 지금 값을 읽는다.
//!
//! 자리 하나는 (파일, 그 호출을 감싼 `fn` 이름, 그 호출이 든 줄)이다. 줄은 공백을 하나로 줄여 비교한다. 승인 자리를
//! 바꾸거나 새 자리를 넣으면(같은 파일·같은 함수여도 줄이 다르면) 실패한다. 키 판정이면 몫 값으로 바꾸고, 표시용이면
//! 이유와 함께 명부에 더한다.
//!
//! 창 사건 시점의 키 처리(`src/adapters/ui/input`)는 사건이 들어온 그때의 포커스가 맞으므로 범위 밖이다. 시험 파일과
//! 파일 안 `#[cfg(test)] mod` 블록도 범위 밖이다. 잡지 못하는 것: 같은 값을 다른 이름의 함수로 감싸 읽는 자리, 승인
//! 줄과 글자까지 같은 줄을 같은 함수에 하나 더 넣는 자리(그 경우 자리 수가 늘어 실패한다).

use super::{block_after, mask_non_code, rust_sources};

const READS: [&str; 5] = [
    ".focused_surface_id(",
    ".window_keys_to_overlay(",
    ".keyboard_overlay_open(",
    ".keyboard_overlay_open_besides_html_address(",
    ".window_holds_terminal_keys(",
];

/// 지금 값을 읽어도 되는 그리기 자리: 파일, 감싼 함수, 호출이 든 줄(공백 줄임), 이유.
const ALLOWED: [(&str, &str, &str, &str); 12] = [
    (
        "src/adapters/ui/egui_panels.rs",
        "draw_egui_panels",
        "let focused_surface_id = state.focused_surface_id(engine);",
        "포커스 바탕(bed_focused)과 외부 변경 확인 대상 표시",
    ),
    (
        "src/adapters/ui/surface_focus_edge.rs",
        "collect",
        "let focused = state.focused_surface_id(engine);",
        "띠 kind leaf의 포커스 띠 표시(바탕 kind의 bed_focused와 같은 값)",
    ),
    (
        "src/adapters/ui/file_open_tool.rs",
        "open",
        "let surface = state.focused_surface_id(engine);",
        "메뉴를 고른 시점의 명령 대상",
    ),
    (
        "src/adapters/ui/draw.rs",
        "draw_ui",
        "cwd_source: state.focused_surface_id(engine),",
        "새 workspace 의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/sidebar/full.rs",
        "draw_full_sidebar",
        "cwd_source: state.focused_surface_id(engine),",
        "새 workspace 의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/popup/rail_category.rs",
        "draw_rail_category_popup",
        "cwd_source: state.focused_surface_id(engine),",
        "새 workspace 의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/tab_bar/apply.rs",
        "open_search_for_focused_terminal",
        "let focused = state.focused_surface_id(engine);",
        "찾기 단추를 누른 시점의 검색 대상",
    ),
    (
        "src/adapters/ui/status_bar.rs",
        "draw_status_bar",
        "let surface_id = state.focused_surface_id(engine);",
        "상태 줄에 보일 surface 정보",
    ),
    (
        "src/adapters/ui/tutorial/mod.rs",
        "draw_tutorial_overlay",
        "state.focused_surface_id(engine)",
        "말풍선이 가리킬 surface 표시",
    ),
    (
        "src/adapters/ui/tutorial/mod.rs",
        "draw_tutorial_overlay",
        "let cwd_source = state.focused_surface_id(engine);",
        "준비 명령의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/popup/route.rs",
        "path_of_focus",
        "let Some(surface) = state.focused_surface_id(core) else {",
        "포커스 경로 계산(포커스 노드 정규화가 쓴다)",
    ),
    (
        "src/gfx/gpu.rs",
        "prepare_layout",
        "let focused_surface_id = state.focused_surface_id(engine);",
        "레이아웃의 포커스 surface 표시",
    ),
];

fn in_scope(path: &str) -> bool {
    let draw = path.starts_with("src/adapters/ui/") || path.starts_with("src/gfx/");
    let file = path.rsplit('/').next().unwrap_or(path);
    draw && !path.starts_with("src/adapters/ui/input/")
        && !path.contains("/tests/")
        && !file.contains("test")
}

/// `#[cfg(test)] mod 이름 { … }` 블록을 지운 코드.
fn without_test_modules(masked: &str) -> String {
    let mut out = masked.to_owned();
    let mut from = 0;
    while let Some(at) = out[from..].find("#[cfg(test)]").map(|i| i + from) {
        let after = at + "#[cfg(test)]".len();
        let rest = out[after..].trim_start();
        let is_mod = rest.starts_with("mod ") || rest.starts_with("pub(crate) mod ");
        match (is_mod, block_after(&out, after)) {
            (true, Some((_, close))) => out.replace_range(at..=close, ""),
            _ => from = after,
        }
    }
    out
}

/// `at` 앞에서 가장 가까운 `fn 이름` 의 이름. 없으면 빈 문자열.
fn enclosing_fn(code: &str, at: usize) -> String {
    let before = &code[..at];
    let mut end = before.len();
    while let Some(i) = before[..end].rfind("fn ") {
        let prev = before[..i].chars().next_back();
        if prev.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
            return before[i + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
        }
        end = i;
    }
    String::new()
}

/// 코드에서 읽는 자리들: (감싼 함수, 호출이 든 줄의 공백을 줄인 글).
fn sites(code: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for read in READS {
        for (at, _) in code.match_indices(read) {
            let start = code[..at].rfind('\n').map_or(0, |i| i + 1);
            let end = code[at..].find('\n').map_or(code.len(), |i| at + i);
            let line = code[start..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            out.push((enclosing_fn(code, at), line));
        }
    }
    out
}

/// 그리기 경로에서 지금 포커스·키 게이트를 읽는 자리가 명부와 같다.
#[test]
fn draw_paths_read_the_live_focus_only_where_the_list_allows() {
    let mut found: Vec<(String, String, String)> = Vec::new();
    for (path, text) in rust_sources() {
        let path = path.to_string_lossy().replace('\\', "/");
        if !in_scope(&path) {
            continue;
        }
        let code = without_test_modules(&mask_non_code(&text));
        for (func, line) in sites(&code) {
            found.push((path.clone(), func, line));
        }
    }
    found.sort();
    let mut allowed: Vec<(String, String, String)> = ALLOWED
        .iter()
        .map(|(p, f, l, _)| ((*p).to_owned(), (*f).to_owned(), (*l).to_owned()))
        .collect();
    allowed.sort();
    assert_eq!(
        found, allowed,
        "그리기 경로가 지금 포커스·키 게이트를 읽는 자리가 명부와 다르다. 키를 받을지 정하는 판정이면 \
         `MainViewState::pass_key_node`(몫의 포커스 노드)를 읽고, 표시·명령 대상이면 이유와 함께 명부에 더한다"
    );
}

/// 범위 판정과 시험 블록 제거가 실제로 자리를 세고 빼는지 확인한다.
#[test]
fn the_scan_counts_draw_code_and_skips_test_modules() {
    let src = "fn a(s: &S) { s.focused_surface_id(e); }\n#[cfg(test)]\nmod tests { fn b(s: &S) { s.window_keys_to_overlay(); } }\n";
    let code = without_test_modules(&mask_non_code(src));
    assert_eq!(
        sites(&code),
        [(
            "a".to_owned(),
            "fn a(s: &S) { s.focused_surface_id(e); }".to_owned()
        )]
    );
    // 승인 자리를 바꾸고 같은 함수에 다른 판정을 넣으면 자리 글이 달라진다.
    let swapped = "fn a(s: &S) { s.other(e); }\nfn b() {}\nfn a2(s: &S) { if s.window_keys_to_overlay() {} }\n";
    assert_eq!(
        sites(swapped),
        [(
            "a2".to_owned(),
            "fn a2(s: &S) { if s.window_keys_to_overlay() {} }".to_owned()
        )]
    );
    assert_eq!(
        sites("fn a(s: &S) {\n    s.keyboard_overlay_open_besides_html_address();\n}\n").len(),
        1,
        "긴 이름이 짧은 이름으로도 세이지 않는다"
    );
    assert!(in_scope("src/adapters/ui/egui_panels.rs"));
    assert!(in_scope("src/gfx/gpu.rs"));
    assert!(!in_scope("src/adapters/ui/input/explorer_ime.rs"));
    assert!(!in_scope("src/adapters/ui/popup/press_cut_tests.rs"));
}

/// 탐색기 본문과 html 주소 입력칸의 키 판정은 그리는 몫의 키 노드(`pass_key_node`) 하나에서 꺼낸다. 탐색기는 그 몫
/// 키들의 사건 시점 안내 포커스(`pass_notice_focus`)도 같은 판정 함수(`explorer_takes_keys`)로 읽는다. 그 값은 주소
/// 띠를 꺼내기 전에 읽는다(포커스 노드가 띠의 포커스를 읽는다). 동작은 `adapters::ui::popup::route::tests::pass_focus` 가 잰다.
#[test]
fn the_explorer_and_address_inputs_read_the_pass_key_node() {
    let src = super::strip_comments(
        &std::fs::read_to_string(super::repo_root().join("src/adapters/ui/egui_panels.rs"))
            .expect("egui_panels.rs"),
    );
    let code: String = src.chars().filter(|c| !c.is_whitespace()).collect();
    let read = "letpass_key_node=state.pass_key_node(engine);";
    let take = "letmuthtml_bands=std::mem::take(&mutstate.html_address);";
    let notice = "letpass_notice_focus=state.pass_notice_focus();";
    let input = "takes_keys:crate::state::focus_node::explorer_takes_keys(info.surface_id,pass_key_node,pass_notice_focus,),";
    let address = "&source,pass_key_node==Some(FocusNode::HtmlAddress(sid)),);";
    for needle in [read, notice, take, input, address] {
        assert_eq!(
            code.matches(needle).count(),
            1,
            "`{needle}` 이 하나가 아니다"
        );
    }
    assert!(
        code.find(read) < code.find(take),
        "몫의 키 노드를 주소 띠를 꺼낸 뒤에 읽는다"
    );
    // 안내 포커스는 그 몫 키들의 사건 시점 값만 읽는다. 그리기 때 다시 읽으면 앞 몫의 바깥 누름이 푼 값을 읽는다.
    assert!(
        !code.contains("restore_notice::has_keyboard_focus("),
        "탐색기 키 판정이 그리기 시점 안내 포커스를 읽는다"
    );
}
