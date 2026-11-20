//! 그리기 경로(`src/adapters/ui`, `src/gfx`)에서 지금 포커스(`focused_surface_id`)와 지금 키 게이트
//! (`window_keys_to_overlay`·`keyboard_overlay_open`·`window_holds_terminal_keys`)를 읽는 자리를 명부로 고정한다.
//! 그리기는 끊어 둔 입력 몫 하나를 그리므로, 그 몫의 키를 받을지는 그 몫의 포커스 노드(`MainViewState::pass_key_surface`,
//! `pass_focus_node`)로 정한다. 지금 값은 뒤 몫의 누름이 이미 옮긴 값일 수 있다. 명부의 자리는 키 판정이 아니라
//! 표시·명령 대상·포커스 경로 계산이라 지금 값을 읽는다. 새 자리가 생기면 이 가드가 실패하고, 키 판정이면 몫 값으로
//! 바꾸고 표시용이면 이유와 함께 명부에 더한다.
//!
//! 창 사건 시점의 키 처리(`src/adapters/ui/input`)는 사건이 들어온 그때의 포커스가 맞으므로 범위 밖이다. 시험 파일과
//! 파일 안 `#[cfg(test)] mod` 블록도 범위 밖이다. 잡지 못하는 것: 같은 값을 다른 이름의 함수로 감싸 읽는 자리.

use std::collections::BTreeMap;

use super::{block_after, mask_non_code, rust_sources};

const READS: [&str; 5] = [
    ".focused_surface_id(",
    ".window_keys_to_overlay(",
    ".keyboard_overlay_open(",
    ".keyboard_overlay_open_besides_html_address(",
    ".window_holds_terminal_keys(",
];

/// 지금 값을 읽어도 되는 그리기 자리: 파일, 자리 수, 이유.
const ALLOWED: [(&str, usize, &str); 10] = [
    (
        "src/adapters/ui/egui_panels.rs",
        1,
        "포커스 바탕(bed_focused)과 외부 변경 확인 대상 표시",
    ),
    (
        "src/adapters/ui/file_open_tool.rs",
        1,
        "메뉴를 고른 시점의 명령 대상",
    ),
    (
        "src/adapters/ui/draw.rs",
        1,
        "새 workspace 의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/sidebar/full.rs",
        1,
        "새 workspace 의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/popup/rail_category.rs",
        1,
        "새 workspace 의 cwd 출처(명령 대상)",
    ),
    (
        "src/adapters/ui/tab_bar/apply.rs",
        1,
        "찾기 단추를 누른 시점의 검색 대상",
    ),
    (
        "src/adapters/ui/status_bar.rs",
        1,
        "상태 줄에 보일 surface 정보",
    ),
    (
        "src/adapters/ui/tutorial/mod.rs",
        2,
        "말풍선이 가리킬 surface 표시와 준비 명령의 cwd 출처",
    ),
    (
        "src/adapters/ui/popup/route.rs",
        1,
        "포커스 경로 계산(포커스 노드 정규화가 쓴다)",
    ),
    ("src/gfx/gpu.rs", 1, "레이아웃의 포커스 surface 표시"),
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

/// 그리기 경로에서 지금 포커스·키 게이트를 읽는 자리가 명부와 같다.
#[test]
fn draw_paths_read_the_live_focus_only_where_the_list_allows() {
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    for (path, text) in rust_sources() {
        let path = path.to_string_lossy().replace('\\', "/");
        if !in_scope(&path) {
            continue;
        }
        let code = without_test_modules(&mask_non_code(&text));
        let n: usize = READS.iter().map(|r| code.matches(r).count()).sum();
        if n > 0 {
            found.insert(path, n);
        }
    }
    let allowed: BTreeMap<String, usize> = ALLOWED
        .iter()
        .map(|(p, n, _)| ((*p).to_owned(), *n))
        .collect();
    assert_eq!(
        found, allowed,
        "그리기 경로가 지금 포커스·키 게이트를 읽는 자리가 명부와 다르다. 키를 받을지 정하는 판정이면 \
         `MainViewState::pass_key_surface`(몫의 포커스 노드)를 읽고, 표시·명령 대상이면 이유와 함께 명부에 더한다"
    );
}

/// 범위 판정과 시험 블록 제거가 실제로 자리를 세고 빼는지 확인한다.
#[test]
fn the_scan_counts_draw_code_and_skips_test_modules() {
    let src = "fn a(s: &S) { s.focused_surface_id(e); }\n#[cfg(test)]\nmod tests { fn b(s: &S) { s.window_keys_to_overlay(); } }\n";
    let code = without_test_modules(&mask_non_code(src));
    assert_eq!(
        READS.iter().map(|r| code.matches(r).count()).sum::<usize>(),
        1
    );
    assert!(in_scope("src/adapters/ui/egui_panels.rs"));
    assert!(in_scope("src/gfx/gpu.rs"));
    assert!(!in_scope("src/adapters/ui/input/explorer_ime.rs"));
    assert!(!in_scope("src/adapters/ui/popup/press_cut_tests.rs"));
}

/// 탐색기 본문의 키 판정은 그리는 몫의 포커스 노드에서 꺼낸 값이다. 그 값은 주소 띠를 꺼내기 전에 읽는다
/// (포커스 노드가 띠의 포커스를 읽는다). 동작은 `adapters::ui::popup::route::tests::pass_focus` 가 잰다.
#[test]
fn the_explorer_input_reads_the_pass_key_surface() {
    let src = super::strip_comments(
        &std::fs::read_to_string(super::repo_root().join("src/adapters/ui/egui_panels.rs"))
            .expect("egui_panels.rs"),
    );
    let code: String = src.chars().filter(|c| !c.is_whitespace()).collect();
    let read = "letpass_key_surface=state.pass_key_surface(engine);";
    let take = "letmuthtml_bands=std::mem::take(&mutstate.html_address);";
    let input = "takes_keys:info.surface_id.is_some()&&info.surface_id==pass_key_surface,";
    for needle in [read, take, input] {
        assert_eq!(
            code.matches(needle).count(),
            1,
            "`{needle}` 이 하나가 아니다"
        );
    }
    assert!(
        code.find(read) < code.find(take),
        "몫의 키 surface 를 주소 띠를 꺼낸 뒤에 읽는다"
    );
}
