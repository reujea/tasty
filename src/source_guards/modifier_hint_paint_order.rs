//! modifier-hint의 불투명 콘텐츠가 테두리를 덮지 않도록 콘텐츠 뒤에 테두리를 그려야 한다.
//! draw_hint_popup에서 두 호출의 텍스트 순서를 비교한다. 조건 분기의 실제 실행 순서는 판단하지 않는다.
//! draw_shell은 본문 확인용으로만 찾으며 그 호출 순서는 이 검사에서 요구하지 않는다.

use super::{fn_body, repo_root, strip_comments};

const HOME: &str = "src/adapters/ui/modifier_hint_overlay.rs";
const DRAW_FN: &str = "fn draw_hint_popup";
const CONTENT: &str = "draw_content(";
const BORDER: &str = "draw_shell_border(";

fn draw_body() -> String {
    let path = repo_root().join(HOME);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{HOME} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    let body = fn_body(&src, DRAW_FN).unwrap_or_else(|| {
        panic!(
            "{HOME}에서 `{DRAW_FN}` 본문을 읽지 못했다. 함수 이름·이동 여부와 본문 추출을 확인한다."
        )
    });
    strip_comments(&body)
}

#[test]
fn the_shell_border_is_redrawn_after_the_content() {
    let body = draw_body();
    let content = body.find(CONTENT).unwrap_or_else(|| {
        panic!("`{DRAW_FN}` 안에 `{CONTENT}` 가 없다 — 콘텐츠 draw 가 사라졌다")
    });
    let border = body.find(BORDER).unwrap_or_else(|| {
        panic!(
            "`{DRAW_FN}` 안에 `{BORDER}` 가 없다 — 테두리 재도색이 사라졌다. 셸에 테두리를 \
             합쳤다면 스트립 배경이 그것을 덮는지부터 확인해라"
        )
    });
    assert!(
        content < border,
        "`{DRAW_FN}`에서 `{BORDER}`가 `{CONTENT}`보다 앞에 있다. 불투명 스트립이 테두리를 덮지 않도록 {HOME}의 호출 순서를 고친다."
    );
}

#[test]
fn the_guard_reads_the_overlay_draw_fn_and_not_some_other_function() {
    // 함수 추출을 확인할 이웃 호출이다. draw_shell의 순서는 검사하지 않는다.
    let body = draw_body();
    for neighbour in ["draw_shell(", "modhint_resize_grip(", "IconButton::new("] {
        assert!(
            body.contains(neighbour),
            "잘라 온 본문이 `{DRAW_FN}` 이 아니다 — `{neighbour}` 가 안 보인다"
        );
    }
}
