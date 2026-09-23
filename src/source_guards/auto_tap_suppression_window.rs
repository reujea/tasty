//! 즉시-tap 억제 구간이 **들어간 자리에서 반드시 닫히는지** 못 박는다.
//!
//! # 계약
//!
//! `execute_forwarded_structural_op` 는 도메인 실행 함수(`structural_exec::split` ·
//! `structural_exec::create_tab`)를 부르기 직전에 `set_auto_tap_suppressed(true)` 로
//! 즉시-tap 을 끄고 직후에 `false` 로 되돌린다. 그 사이에 함수를 빠져나가면 플래그는 **켜진 채로
//! 남는다** — `OccupancyRegistry` 는 프로세스 수명 동안 살아 있으므로 이후 모든
//! `tap_new_workspace_member` 가 영구히 tap 을 건너뛴다. 증상은 점유된 workspace 에
//! 새로 생긴 터미널이 그냥 스트리밍되지 않는 것뿐이고, 패닉도 로그도 없다.
//!
//! `src/core/attach.rs` 의 필드 doc 이 그 안전성을 **산문으로만** 진술하고 있었다:
//! *"핸들러 호출 자체는 `Result` 를 반환하지 않아 `?` 로 건너뛸 수 없다."* 참인
//! 진술인데, 참으로 유지된다는 보장이 아무 데도 없었다. 지금 구간이 감싸는 도메인 실행은
//! `Result` 를 **반환한다** — 그 산문의 전제는 사라졌고, 구간이 안전한 이유는 결과를
//! 바인딩만 하고 구간을 닫은 뒤에 판정하기 때문이다. 그것을 지키는 것이 이 가드다.
//!
//! # 왜 이 자리가 정적으로만 판정 가능한가
//!
//! 런타임 시험으로는 못 묻는다. "두 줄 사이에 탈출 경로가 없다" 를 실행으로 관측하려면
//! 감싼 실행을 실제로 실패시켜야 하고, 그러려면 `Core` · `AppState` · `CoreState` 를 전부
//! 세워야 한다 — 이 세 개는 앱 그 자체다. 반면 플래그 자신은 `bool` 이라 그것만 따로
//! 시험하면 아무것도 증명하지 못한다(무엇을 뽑아내도 계약이 안 따라온다).
//! 그래서 여기서 묻는 것은 **그 구간의 소스 모양**이다.
//!
//! # 초록이 뜻하는 것
//!
//! **여는 호출마다 다음 setter 가 닫는 호출이고, 그 사이 텍스트에 `?` · `return` ·
//! `break` · `continue` 가 없고, 사이가 비어 있지 않다.** 그리고 **플래그를 읽는 자리가
//! 아직 있고 그것이 tap 경로다.**
//!
//! 초록이 뜻하지 **않는** 것:
//!
//! - **패닉 unwind 로는 안 닫힌다.** `?` 가 없어도 감싼 실행이 패닉하면 플래그는 켜진 채
//!   남는다. 그것을 원천 봉쇄하려면 RAII 가드로 바꿔야 하고, 그건 이 판정기가 아니라
//!   설계 변경이다.
//! - **구간이 옳은 것을 감싼다는 뜻이 아니다.** 사이에 호출이 하나라도 있으면 통과한다.
//!   그것이 도메인 실행 함수인지는 안 본다.
//! - **닫는 호출이 실행된다는 뜻이 아니다.** 조건 분기 뒤에 있어도 텍스트로는 통과한다.
//! - **RAII 로 바꾸면 이 판정기는 좌변이 비어 빨개진다.** 조용히 초록이 되지 않게
//!   비공허를 따로 단정한다 — 좌변이 0 이 되는 것은 "지켜졌다" 가 아니라 "이 가드가
//!   낡았다" 는 뜻이고, 그 둘은 같은 색으로 보이면 안 된다.
//!
//! # 이 가드가 **혼자** 지키는 축과, 남과 겹치는 축
//!
//! rc 만 보면 변이가 죽은 이유를 잘못 귀속한다. 실측으로 갈랐다.
//!
//! - **구간이 안 닫히는 축**은 이 가드가 혼자 지킨다. 구간 안에 `?` 를 넣거나 닫는
//!   호출을 지우면 `every_suppression_window_is_closed_before_any_way_out` 이 자기
//!   좌표를 찍고 죽는다.
//! - **읽는 쪽이 사라지는 축은 겹친다.** `is_auto_tap_suppressed` **선언을 지우면**
//!   `-D dead-code` 가 먼저 죽이고 이 가드는 **한 마디도 안 한다**(실측 2026-09-08:
//!   rc=101 인데 `test result:` 줄 자체가 없다 — 컴파일이 죽은 것이다). 그래서 그
//!   축을 재려면 선언을 지우는 대신 **읽기를 다른 파일의 헬퍼 뒤로 옮겨야** 한다.
//!   그때 비로소 `the_flag_still_has_a_reader_and_it_is_the_tap_path` 가 답한다.
//!   술어를 어긴 것이 아니라 **좌변에서 나간 것**이고, 이 파일만
//!   보는 좌변에서는 그 이동이 곧 부재다.
//!
//! 그래서 rc 는 증거가 아니다. **어느 시험이 무슨 좌표를 찍고 죽었는지**가 증거다.
//!
//! # 왜 인라인이 아니라 여기인가
//!
//! `src/core/attach.rs` 에 같은 성질의 인라인 가드가 이미 있다(두 pump 의 호출 순서).
//! 그 배치는 코드 옆이라 읽기는 좋은데 찾기가 나쁘다 — 자매 가드
//! [`super::frame_draw_order`] 가 그 실측을 근거로 이 디렉토리를 택했고, 같은 이유로
//! 따른다.
//!
//! 관측(단정이 아니다): 2026-09-08 기준 여는 호출 3 · 닫는 호출 3 · 읽는 자리 1.
//! 그 셋을 품는 함수 본문에는 `?` 가 5 개, `return` 이 4 개 있다 — **전부 구간 밖**이고,
//! 그 사실이 곧 이 가드가 지키는 것이다.

use super::{fn_body, line_of, mask_non_code, repo_root};

/// 구간이 사는 파일과 함수.
const HOME: &str = "src/core/attach_runtime.rs";
const WINDOW_FN: &str = "fn execute_forwarded_structural_op";
/// 구간을 여는 호출과 닫는 호출.
const OPEN: &str = "set_auto_tap_suppressed(true)";
const CLOSE: &str = "set_auto_tap_suppressed(false)";
/// 플래그를 읽는 자리와 그것이 살아야 하는 함수.
const READ: &str = "is_auto_tap_suppressed()";
const READER_FN: &str = "fn tap_new_workspace_member";
/// 구간 안에 있으면 플래그가 켜진 채 새는 탈출 형태.
const ESCAPES: &[&str] = &["?", "return", "break", "continue"];

/// `HOME` 을 마스킹해 읽는다 — 주석 속 같은 이름을 호출로 세지 않기 위해서다
/// (실측: 마스킹 없이 세면 읽는 자리가 1 이 아니라 2 로 나온다).
fn masked_home() -> String {
    let path = repo_root().join(HOME);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{HOME} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n");
    mask_non_code(&src)
}

fn body_of(masked: &str, signature: &str) -> String {
    fn_body(masked, signature).unwrap_or_else(|| {
        panic!(
            "`{signature}` 를 {HOME} 에서 자르지 못했다 — 이름이 바뀌었거나 자리를 \
             옮겼다. 못 자른 상태의 초록은 통과가 아니라 미측정이니 이 가드의 상수를 \
             함께 고쳐라"
        )
    })
}

/// `body` 안에서 `needle` 이 나타나는 모든 위치.
fn all_positions(body: &str, needle: &str) -> Vec<usize> {
    body.match_indices(needle).map(|(i, _)| i).collect()
}

#[test]
fn every_suppression_window_is_closed_before_any_way_out() {
    let masked = masked_home();
    let body = body_of(&masked, WINDOW_FN);
    let opens = all_positions(&body, OPEN);
    let closes = all_positions(&body, CLOSE);

    assert!(
        !opens.is_empty(),
        "`{WINDOW_FN}` 안에 `{OPEN}` 이 하나도 없다. 억제를 RAII 가드로 바꿨다면 이 \
         판정기가 낡은 것이니 좌변을 그 타입으로 옮겨라 — 좌변이 빈 초록은 \"지켰다\" 가 \
         아니라 \"안 봤다\" 다"
    );
    assert_eq!(
        opens.len(),
        closes.len(),
        "억제 구간의 여닫이 수가 다르다(연 것 {} · 닫은 것 {}). 하나라도 안 닫히면 \
         플래그가 켜진 채 남아 이후 모든 `tap_new_workspace_member` 가 tap 을 \
         영구히 건너뛴다",
        opens.len(),
        closes.len(),
    );

    for open in opens {
        // 이 여는 호출 다음에 오는 첫 setter 가 닫는 호출이어야 한다.
        let after = open + OPEN.len();
        let next_close = body[after..].find(CLOSE).map(|p| p + after);
        let next_open = body[after..].find(OPEN).map(|p| p + after);
        let close = next_close.unwrap_or_else(|| {
            panic!(
                "{HOME}:{} 의 `{OPEN}` 뒤에 `{CLOSE}` 가 없다 — 구간이 안 닫힌다",
                line_of(&masked, open)
            )
        });
        if let Some(again) = next_open {
            assert!(
                close < again,
                "{HOME}:{} 의 구간이 닫히기 전에 {} 줄에서 다시 열린다 — 중첩된 억제는 \
                 안쪽이 닫힐 때 바깥 구간까지 함께 풀어 버린다",
                line_of(&masked, open),
                line_of(&masked, again),
            );
        }

        let span = &body[after..close];
        for escape in ESCAPES {
            let found = if *escape == "?" {
                span.contains('?')
            } else {
                span.split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|w| w == *escape)
            };
            assert!(
                !found,
                "{HOME}:{} 의 억제 구간 안에 `{escape}` 가 있다. 그 경로로 빠져나가면 \
                 `{CLOSE}` 를 지나지 않아 플래그가 켜진 채 남고, 이후 점유 workspace 에 \
                 생기는 모든 터미널이 조용히 tap 되지 않는다 — 패닉도 로그도 없다",
                line_of(&masked, open),
            );
        }
        assert!(
            span.contains('('),
            "{HOME}:{} 의 억제 구간이 비어 있다 — 아무것도 안 감싸는 억제는 켰다 끄기만 \
             하고 끝난다",
            line_of(&masked, open),
        );
    }
}

#[test]
fn the_flag_still_has_a_reader_and_it_is_the_tap_path() {
    let masked = masked_home();
    let reads = all_positions(&masked, READ);
    assert!(
        !reads.is_empty(),
        "`{READ}` 를 읽는 자리가 없다. 읽는 쪽이 사라지면 위 구간 셋은 켰다 끄기만 하는 \
         무동작이 되고, 이중 tap(문자 중복 echo)이 조용히 돌아온다"
    );
    let reader = body_of(&masked, READER_FN);
    let reader_at = masked
        .find(&reader)
        .expect("잘라 온 본문이 원본에 없다 — 자르기가 깨졌다");
    for read in reads {
        assert!(
            read >= reader_at && read < reader_at + reader.len(),
            "{HOME}:{} 의 `{READ}` 가 `{READER_FN}` 밖에 있다. 억제 구간은 그 한 경로만 \
             건너뛰라고 켜는 것이라, 다른 경로가 같은 플래그를 읽으면 그 경로도 \
             forward-op 중에 조용히 멈춘다",
            line_of(&masked, read),
        );
    }
}

#[test]
fn the_guard_cuts_the_two_functions_and_not_some_others() {
    // 공허 방지 — `fn_body` 가 엉뚱한 함수를 잘라도 위 이름들이 우연히 들어 있을 수 있다.
    let masked = masked_home();
    let window = body_of(&masked, WINDOW_FN);
    for neighbour in ["StructuralOp::SplitSurface", "exec::create_tab("] {
        assert!(
            window.contains(neighbour),
            "잘라 온 본문이 `{WINDOW_FN}` 이 아니다 — `{neighbour}` 가 안 보인다"
        );
    }
    let reader = body_of(&masked, READER_FN);
    for neighbour in ["add_workspace_member(", "tap_surface_for_stream("] {
        assert!(
            reader.contains(neighbour),
            "잘라 온 본문이 `{READER_FN}` 이 아니다 — `{neighbour}` 가 안 보인다"
        );
    }
    assert!(
        window.len() > reader.len(),
        "두 자름이 같은 본문을 가리킨다 — 자르기가 깨졌다"
    );
}
