//! 순회 결과가 비어도 조용히 통과하는 검사를 **만들 수 없게** 하는 공용 순회.
//!
//! 스캔 가드의 실패 형태는 둘인데 하나만 보인다. 위반을 찾아 빨개지는 것은 보이고,
//! 순회가 죽어 아무것도 안 보고 초록인 것은 안 보인다. 뒤쪽은 그 가드가 지키려던
//! 것이 깨진 바로 그 순간에도 초록이다.
//!
//! **막는 방법은 각자 하한을 박는 것이고, 실제로 다들 박았다 — 형태가 제각각으로.**
//! 수 하한 상수, 빈 결과 거부, 명부 대조, 판정을 함수로 뽑은 것, 지역 변수 비교…
//! 2026-09-06 에 그 형태들을 정적으로 열거해 "방어 없는 순회" 를 세려다 세 번 반례를
//! 맞았다. 방어는 임의의 코드라 열거가 끝나지 않는다. 열거로 셀 수 있는 것은 **이름을
//! 내가 소유한 것** 뿐이고, 그래서 세는 대신 자리를 하나 만든다.
//!
//! 이 모듈을 통과한 순회는 하한을 **빠뜨릴 수 없다.** 하한은 인자이고, 실패 메시지는
//! 소비자가 아니라 여기가 만든다 — 하한을 내려서 통과시키지 말라는 금지도 여기 있다.
//! 소비자마다 다시 쓰면 언젠가 한 곳이 빠뜨리고, 빠뜨린 쪽은 조용하다.
//!
//! 하한을 값 하나로 받지 않는 이유는 [`Floor`] 에 적었다.

use std::path::{Path, PathBuf};

/// **값을 잰 트리.** 날짜(`measured_on`)만으로는 못 가른다 — 여러 lane 이 같은 날 각자의
/// base 에서 재면 날짜가 같고 값이 다르다. 실측 2026-09-08: 같은 술어를 재는 하한 둘이
/// 448 과 440 이었고, 차 8 은 거르개가 아니라 **잰 트리**였다(그 8 은 전부 다른 lane 의
/// 결정 기록이다). 그리고 그날 통합된 트리에서 그 술어는 453 이라 **둘 다 틀렸다.**
///
/// 갈래를 넷으로 두는 이유는 "안 쟀다" 와 "쟀는데 그 좌표로 못 간다" 가 다른 상태이기
/// 때문이다. 빈칸 하나로 두면 그 둘이 같아 보이고, 다음 사람이 둘 다 "적으면 된다" 로
/// 읽는다.
///
/// ★ **이 체계가 인용하는 2026-09-04~2026-09-08 좌표는 지금 `main` 에서 도달 불가다.**
/// 그 창의 이력은 `1998055b1` 에서 갈라진 뒤 버려졌고, 그 갈래의 커밋은 객체로 남아
/// 있어도 `HEAD` 의 조상이 아니다 — 선언에 적힌 것은 `12bc0f4b2` · `b134d28e3` ·
/// `8bdbf1bdb` · `671aa69b2` · `ee7a32349` 다. **값은 실측이고 좌표만 죽었다.** 그래서
/// 지우지 않는다 — 지우면 그 값이 어디서 왔는지가 사라지고, 다음 사람은 낡았다는 것조차
/// 못 본다. 산문에 같은 창의 해시가 나오면 전부 이 문단이 설명한다.
/// 재는 법: `git merge-base --is-ancestor <해시> HEAD` 가 참이어야 [`CountedOn::Tree`] 의
/// 계약이 선다. 실측·처방·판정기를 안 지은 이유는
/// `docs/adr/0296-an-unreachable-commit-coordinate-is-annotated-not-deleted.md` 에 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountedOn {
    /// `main` 에서 **도달 가능한** 커밋의 트리에서 쟀다. 짧은 해시로 시작하고 뒤에 서술이
    /// 붙어도 된다.
    ///
    /// ★ 위 이력 재작성 이전 좌표를 든 자리가 다섯 있다(`12bc0f4b2` 둘 · `b134d28e3` 셋).
    /// 이 갈래의 계약을 지금은 못 지키는 자리라, 각 문자열 안에 그 사실을 적어 두었다 —
    /// 고치는 길은 좌표를 바꾸는 것이 아니라 **지금 트리에서 다시 세는 것**이다.
    Tree(&'static str),
    /// lane 의 tip 에서 쟀고 **그 커밋이 `main` 에 없다.** 회차가 체리픽으로 착지하면 lane
    /// tip 은 조상이 안 되므로, 그 해시를 적어 둔 자리는 값은 실측인데 좌표로는 못 간다.
    /// 실측 2026-09-08: 그때 하한 문장에 적힌 lane tip 다섯이 전부 이 상태였다.
    /// 다음에 그 좌변을 재는 사람이 `Tree` 로 바꾼다.
    ///
    /// ★ 단 지금 선언에 남은 `LaneTip` 넷(`8bdbf1bdb` 둘 · `671aa69b2` · `ee7a32349`)은
    /// 그 길이 닫혔다 — 위 이력 재작성으로 버려진 갈래의 커밋이라 `main` 에 들어올 일이
    /// 없다. 그 넷은 `Tree` 로 **바꾸는** 것이 아니라 다시 재서 새 좌표를 적는다. 이
    /// 갈래는 그 자체로 "좌표로 못 간다" 를 뜻하므로 자리마다 덧붙이지 않는다.
    LaneTip(&'static str),
    /// 픽스처가 **자기가 방금 만든 합성 트리**를 잰다. 레포 좌변이 아니라 "어느 커밋에서
    /// 쟀나" 라는 물음 자체가 성립하지 않는다.
    SyntheticTree,
    /// **안 쟀다.** 이 값이 어느 트리에서 나왔는지 선언에 안 남아 있다. 왜 못 밝히는지를
    /// 적는다 — 빈칸과 "없다고 판단함" 은 다르다. 0 이나 오늘 날짜로 채우지 마라.
    Unmeasured(&'static str),
}

impl CountedOn {
    /// 2026-09-08 의 정정 대상(열하나) 밖이라 그 뒤로 아무도 다시 안 잰 자리. 자리마다 사유가
    /// 같으므로 문장을 복제하지 않고 여기 하나를 둔다 — 그러면 이 갈래의 **수**가 세어지고,
    /// 그 수가 다음 회차의 좌변이 된다.
    pub const NEVER_COUNTED: CountedOn = CountedOn::Unmeasured(
        "2026-09-08 의 정정 대상 밖이라 다시 안 쟀다 — 값의 출처가 선언에도 커밋문에도 안 남아 있다",
    );

    fn validate(&self) -> Result<(), String> {
        let hash_ok = |h: &str| {
            let head: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            head.len() >= 7
        };
        match self {
            CountedOn::Tree(h) | CountedOn::LaneTip(h) => {
                if hash_ok(h) {
                    Ok(())
                } else {
                    Err(format!(
                        "`counted_on` 이 커밋으로 안 읽힌다: {h:?}. 짧은 해시(7 자 이상)로 \
                         시작해야 한다 — 날짜는 `measured_on` 이 이미 갖고 있고, 날짜만으로는 \
                         같은 날 다른 base 에서 잰 두 값을 못 가른다."
                    ))
                }
            }
            CountedOn::SyntheticTree => Ok(()),
            CountedOn::Unmeasured(why) => {
                if why.split_whitespace().count() >= 5 {
                    Ok(())
                } else {
                    Err(format!(
                        "`counted_on` 이 `Unmeasured` 인데 사유가 너무 짧다({} 낱말). 안 잰 \
                         것을 안 쟀다고 적는 것은 옳지만, 왜 못 밝히는지가 없으면 다음 사람이 \
                         그 자리를 재야 할 자리로 안 본다. ★ 이 문턱을 내려서 통과시키지 마라.",
                        why.split_whitespace().count()
                    ))
                }
            }
        }
    }
}

/// 순회가 모아야 할 최소량 — 그리고 **그 값이 무엇의 함수인지**.
///
/// 값 하나만 받으면 그 값은 낡는다. 낡은 하한은 두 방향으로 틀리는데 둘 다 조용하다:
/// 너무 낮으면 순회가 절반 죽어도 통과하고, 너무 높으면 정상적인 감소에 빨개져서
/// 사람이 하한을 내리는 습관을 들인다. 그래서 셋을 함께 받는다 — 마지막으로 **센**
/// 값, 그것을 **잰 날**, 그리고 하한을 그보다 낮게 잡은 **이유**.
///
/// 셋째가 핵심이다. 간격의 크기는 모수가 얼마나 빨리 움직이는가의 판단이고, 그 판단은
/// 자리마다 다르다 — 통합 타깃 수처럼 천천히 느는 모수는 여유가 좁아야 하고(여유가
/// 넓으면 술어가 절반 죽어도 통과한다), 크레이트 분해로 한 번에 크게 움직이는 모수는
/// 넓어야 한다. 그 판단을 안 적으면 다음 사람이 간격만 보고 아무 방향으로나 고친다.
pub struct Floor {
    /// 이 아래로 모이면 순회가 죽은 것으로 본다. 0 은 받지 않는다.
    pub min: usize,
    /// 마지막으로 실제로 센 값.
    pub measured: usize,
    /// 그 값을 잰 날 (`YYYY-MM-DD`).
    pub measured_on: &'static str,
    /// **그 값을 잰 트리.** 날짜만으로는 못 가른다 — 갈래와 근거는 [`CountedOn`] 에 있다.
    pub counted_on: CountedOn,
    /// `min` 을 `measured` 보다 낮게 잡은 이유 — 이 모수가 무엇의 함수이고 얼마나 빨리
    /// 움직이는가.
    pub why_this_gap: &'static str,
}

/// 여러 가드가 **같은 좌변**을 잴 때, 그 좌변의 사실을 담는 한 자리.
///
/// [`Floor`] 는 두 가지를 한 구조체에 담는다 — 모수의 **사실**(`measured`·`measured_on`)과
/// 소비자의 **판단**(`min`·`why_this_gap`). 앞은 모수마다 하나여야 하고 뒤는 소비자마다
/// 다르다. 그 둘을 안 가르면 같은 좌변을 재는 두 선언이 서로를 안 보고 **각자 낡는다.**
///
/// 실측 2026-09-08(`b134d28e3`): `Floor` 선언 28 개 중 같은 좌변을 재는 짝이 넷이었고, 넷 다
/// 두 값이 서로 달랐다 — `docs/**/*.md` 를 200 과 380 으로, `src/**/*.rs` 를 591 과 598 로,
/// 루트 통합 타깃을 36 과 39 로, 크레이트 통합 타깃을 93 과 101 로. 한쪽을 갱신해도
/// 다른 쪽은 안 움직이므로 갱신이 언제나 반쪽만 된다.
///
/// 더 나쁜 것은 산문 쪽이었다. `docs/**/*.md` 를 재는 두 선언은 같은 모수의 동역학을
/// **정반대로** 단언했다 — 한쪽은 "회차마다 늘고 줄어서", 다른 쪽은 "단조 증가해 왔고
/// 사라지는 변경은 없었다". 둘 다 자기 여유의 근거였다.
///
/// ★ **그 28 을 낳는 술어를 함께 적는다** — 이름 붙은 `const` 선언만 센 값이다. 같은
/// 이름에 다른 술어를 대면 다른 수가 나오고, 넷 다 재현 가능하다: 이 타입의 리터럴
/// 전부(인라인 포함) 41 · `const` 선언만 28 · 41 에서 이 파일 자신을 뺀 것 35 ·
/// `walk_with_floor(` 호출 37(전부 2026-09-08 · `b134d28e3`). 아래 짝 넷은 **선언 28**
/// 위에서만 뜻이 있다 — 인라인 리터럴은 픽스처가 **합성 트리**용으로 짓는 것이라 레포
/// 좌변을 안 잰다. 그 자리에 레포 실측을 넣으면 컴파일도 시험도 초록인 채 픽스처의
/// 하한만 죽는다.
///
/// ★ 그리고 이 문단은 **자기가 말하는 술어 안에 안 들어가게** 썼다. 첫 판에서 술어를
/// 예시로 적었더니 그 문자열이 그대로 좌변에 앉아 41 이 43 이 됐다 — 수를 세는 문장이
/// 그 수를 움직였다.
pub struct Population {
    /// 마지막으로 실제로 센 값.
    pub measured: usize,
    /// 그 값을 잰 날 (`YYYY-MM-DD`).
    pub measured_on: &'static str,
    /// **그 값을 잰 트리.** 날짜만으로는 부족하다 — 여러 lane 이 같은 날 각자의 base 에서
    /// 재면 날짜가 같고 값이 다르다. 그리고 다음 사람은 오늘 날짜를 보고 그 값을 통합
    /// 트리의 값으로 읽는다. 실측 2026-09-08: 그 형태로 이 레포의 하한 셋이 어긋났다.
    pub counted_on: CountedOn,
    /// **그 수를 낳는 술어.** 수만 적으면 다음 사람이 다른 술어로 세고 다른 수를 얻는데,
    /// 두 값이 다 재현 가능해서 어느 쪽이 틀렸는지 값으로는 안 갈린다. 수를 지키는 것은
    /// 수가 아니라 그 수를 낳는 정의다.
    pub how: &'static str,
}

/// 이 레포의 좌변들 — 둘 이상의 가드가 재는 것만 여기 산다.
///
/// **이 모듈이 별도 파일이 아닌 이유가 있다.** 여기 담기는 값 중 하나가 저장소 전체의
/// `.rs` 파일 수이고, 새 파일을 하나 만들면 그 값이 그 자리에서 1 늘어난다. 값을 담을
/// 자리가 값을 바꾸는 것은 다음 사람이 못 읽는 형태다.
///
/// ★ **이 모듈은 [`Floor`] 의 유일한 출처가 아니다.** 둘이고, 둘 다 정상이다.
///
/// - 여기 있는 `Population` — **레포의 좌변**을 재는 자리. 둘 이상의 가드가 같은 것을
///   재면 값이 여기 하나로 산다.
/// - 선언 자리의 **인라인 리터럴** — 픽스처가 **합성 트리**의 하한을 지을 때 쓴다.
///   그 트리는 파일 몇 개짜리라 레포 실측과 아무 관계가 없다.
///
/// 이것을 안 적으면 다음 사람이 인라인 리터럴을 "아직 안 옮긴 것" 으로 읽고 여기로
/// 쓸어 담는다. 그러면 합성 트리의 하한이 레포 실측이 되어 **컴파일도 시험도 초록인
/// 채 그 픽스처의 하한만 죽는다.** 가르는 법은 순회 뿌리다 — 뿌리가 `repo_root()` 에서
/// 오면 레포 좌변이고, 임시 디렉토리에서 오면 합성 트리다.
pub mod populations {
    use super::Population;

    /// `src/` 아래 `.rs` 전부.
    pub const SRC_RS: Population = Population {
        measured: 633,
        measured_on: "2026-09-22",
        counted_on: super::CountedOn::Tree(
            "6c3c9a4fc — 다음 회차 lane 들이 착지한 main 트리에서 다시 셌다. 632 -> 633 이고 \
             더해진 것은 `src/app/window_lifecycle/register_focus_tests.rs` 하나(`af8da5089`, \
             에이전트 창 포커스 lane), 삭제는 0 이다. 그 lane 이 이 항을 안 건드려 push 가 \
             pre-push B.10 에서 막혔다. \
             이전 회차: 86d08cc85 — 여러 lane 이 착지한 main 트리에서 다시 셌다. 선언 611 이 이 트리의 \
             632 와 어긋나 push 가 pre-push B.10 에서 막혔다 — 각 lane 이 자기 base 에서 \
             이 항을 안 건드린 채 파일을 더한 형태다. `62f7a86d8` 의 추적 트리는 610 이고(611 \
             은 아래 `src/file/picker_caps.rs` 가 든 lane 트리의 값이다) 거기서 더해진 것 26 · \
             빠진 것 4 다. 빠진 넷은 `src/adapters/production/stream_hub.rs` · \
             `src/core/file_origin_tests.rs` · `src/plugin_bridge/egui_mesh_surface.rs`(같은 \
             이름이 `src/core/` 에 더해졌다) · `src/state/mark.rs` 다. \
             이전 회차: 62f7a86d8 — 같은 트리에서 다시 셌다. 610 -> 611 이고 더해진 것은 \
             `src/file/picker_caps.rs` 하나(component 층 토큰을 뷰 밖에서 읽는 자리), \
             삭제는 0 이다. 이전 회차: 5120fcda1 — 앞 회차 lane 착지(층 1·2)를 담은 트리에서 다시 셌다. 607 → 610, 신규 \
             파일 3 개(`src/app/attach_client/dispatch.rs` · 그 아래 `debug_completion.rs` · \
             `src/webhook/listener_body_tests.rs`)이고 삭제는 0 이다. 앞선 값 607 은 아래 \
             좌표에서 쟀고 그 트리에서는 맞았다. 그 앞의 이력은 \
             이어서 적는다 — afcb71f32 — 네 번째 리팩토링 물결 여덟 lane 이 착지한 트리에서 다시 셌다. \
             **두 번째 감소다**(623 → 607, −16). 내역이 한 갈래로 깨끗하다: `src/platform/` \
             의 `.rs` 17 개가 통째로 `crates/tasty-platform/` 으로 갔고(순수 삭제 0), \
             더해진 것은 `src/adapters/ipc/handler/idempotency.rs` 하나다. 17 − 1 = 16 이 \
             그대로 이 걸음이다. 앞선 값 623 은 `36599e609` 에서 쟀고 그 트리에서는 맞았다 \
             — **이 모수가 처음 줄었다**(645 → 623, −22). `src/` 에서 사라진 파일은 지워진 것이 아니라 \
             `crates/` 로 옮겨갔고 그쪽은 이 모수 밖이다 — 터미널 링크·포맷·webview 잎 \
             추출, 폰트 크레이트의 장치 경계 분할, platform 폴더의 App glue 상승이 그 \
             이동이며, 순수 삭제는 아무것도 컴파일하지 않던 `src/debug_info.rs` 사본 \
             하나뿐이다. 앞선 값 645 는 `9a0b6a1be` 에서 쟀고 그 트리에서는 맞았다 — \
             호스트 주입기를 `tasty-ipc` 로 옮긴 그 커밋이 같은 방향의 첫 걸음이었다. \
             그 앞의 646 은 `353038a3e` 에서 쟀고 그 트리에서는 \
             맞았다. 그 앞의 643 은 `91ca7d37d`, 606 은 `12bc0f4b2` 에서 쟀고 각자의 트리에서 \
             다시 세면 지금도 그 값이다. 즉 **낡은 값**이지 안 잰 값이 아니었고, 그래서 이 \
             모수를 쓰는 자리의 여유 근거는 갱신 대상이 아니다(수만 따라간다). ★ 그 옛 \
             좌표는 지금 **main 에서 도달 불가**다 — 이 레포의 이력이 `1998055b1` 뒤로 다시 \
             쓰였고, 그 뒤로 옛 좌표는 어느 조상도 아니다. [`CountedOn::Tree`] 는 도달 가능한 \
             커밋을 뜻하므로 그 갈래를 지키려면 좌표도 함께 옮겨야 한다.",
        ),
        how: "`src/` 를 뿌리로 `SkipBuildCaches` 순회하고 `rel` 이 `.rs` 로 끝나는 것 전부. \
              재는 법: `git ls-tree -r --name-only <rev>` 에서 `src/` 로 시작하고 `.rs` 로 \
              끝나는 줄을 센다 — 이 모수의 미추적 기여분은 0 이다(2026-09-22 재확인: 작업 \
              트리를 `find` 로 센 633 과 추적 트리 계수 633 이 같았다. 607 · 623 · 632 · 645 였을 \
              때도, 2026-09-08 에도 같았다). \
              같은 좌변을 `scripts/check-intent-discipline.sh` 가 자기 실패문에 \
              `좌변(src/) 의 .rs N개` 로 찍으므로 **대조가 공짜다** — 두 수가 갈리면 술어가 \
              갈린 것이다(2026-09-22: 둘 다 633).",
    };

    /// 루트 패키지의 통합 테스트 타깃 — `tests/` 바로 아래 한 겹.
    pub const ROOT_TEST_TARGETS: Population = Population {
        measured: 43,
        measured_on: "2026-09-22",
        counted_on: super::CountedOn::Tree(
            "86d08cc85 — 여러 lane 이 착지한 main 트리에서 다시 셌다. 42 -> 43 이고 더해진 \
             것은 `tests/attach_structure_sync_loopback.rs` 하나(`5a6c6b810`, attach 구조 동기화 \
             lane), 삭제는 0 이다. 그 lane 이 이 항을 안 건드려 push 가 pre-push B.10 에서 \
             막혔다. \
             이전 회차: 62f7a86d8 — 같은 트리에서 다시 셌다. 41 -> 42 이고 더해진 것은 \
             `tests/file_handler_when_fits_column.rs` 하나, 삭제는 0 이다. \
             이전 회차: 65bcd1988 — CLI 도움말 현지화 lane 이 착지한 트리에서 다시 셌다. 40 -> 41 이고 \
             더해진 것은 `tests/cli_help_locales.rs` 하나, 삭제는 0 이다. \
             이전 회차: 91ca7d37d — 여기서 다시 셌다. 앞선 값 39 는 `12bc0f4b2` 에서 쟀고 그 트리에서 \
             다시 세도 39 라 **낡은 값**이었다. 그 사이 늘어난 파일은 정확히 하나 \
             (`tests/attach_markdown_content_loopback.rs`). ★ 옛 좌표는 지금 **main 에서 \
             도달 불가**다(이력이 `1998055b1` 뒤로 다시 쓰였다) — 자세한 사정은 \
             [`SRC_RS`] 의 같은 자리에 적었다.",
        ),
        how: "`tests/` 바로 아래 `.rs` — **한 겹만**이다. 더 깊은 것은 타깃이 아니라 그 \
              타깃의 모듈이라 `cargo test` 가 따로 안 돌린다. 재는 법: 경로가 `tests/` 로 \
              시작하고 `/` 가 정확히 하나이며 `.rs` 로 끝나는 줄.",
    };

    /// 크레이트들의 통합 테스트 타깃 — `crates/<크레이트>/tests/` 바로 아래 한 겹.
    pub const CRATE_TEST_TARGETS: Population = Population {
        measured: 125,
        measured_on: "2026-09-23",
        counted_on: super::CountedOn::Tree(
            "f5c9893c4 + lane — 마지막 워크스페이스 닫기 레이스 lane 이 \
             `tasty-doc-guards/tests/close_request_consumed_in_place.rs` 하나를 더해 124 -> 125 \
             다. 삭제는 0 이다. **lane 트리에서 잰 값이라 구조적으로 낮다** — 같은 회차의 다른 \
             lane 이 통합 타깃을 더했으면 통합 트리에서는 더 크고, 최종 값은 병합하는 쪽이 거기서 \
             다시 잰다. \
             이전 회차: 153566489 + lane — 경계 가드 구멍 lane 이 \
             `tasty-doc-guards/tests/automation_runners_do_not_reach_inbound_adapters.rs` 하나를 \
             더해 123 -> 124 다. 삭제는 0 이다. **lane 트리에서 잰 값이라 구조적으로 낮다** — 같은 \
             회차의 다른 lane 이 통합 타깃을 더했으면 통합 트리에서는 더 크고, 최종 값은 병합하는 \
             쪽이 거기서 다시 잰다. \
             이전 회차: 17e2a7f56 + lane — 도메인 경계(RF10) lane 이 \
             `tasty-doc-guards/tests/domain_does_not_reach_up.rs` 하나를 더해 122 -> 123 이다. \
             삭제는 0 이다. **lane 트리에서 잰 값이라 구조적으로 낮다** — 같은 회차의 다른 lane 이 \
             통합 타깃을 더했으면 통합 트리에서는 더 크고, 최종 값은 병합하는 쪽이 거기서 다시 잰다. \
             이전 회차: 62f7a86d8 — 파일 핸들러 lane 이 착지한 트리에서 다시 셌다. 121 -> 122 이고 더해진 것은 \
             `tasty-gallery/tests/mono_metrics.rs` 하나, 삭제는 0 이다. \
             이전 회차: 4d307aeb3 — CLI 도움말 현지화 lane 이 착지한 트리에서 다시 셌다. 119 -> 121 이고 \
             더해진 둘은 `tasty-cli/tests/localized_help.rs` 와 \
             `tasty-cli/tests/plugin_help_catalog.rs` 다. 삭제는 0 이다. \
             이전 회차: dbf5481a4 + lane — site vendor lane 이 \
             `tasty-doc-guards/tests/design_token_census_has_one_source.rs` 하나를 더해 119 다. \
             **lane 트리에서 잰 값이라 구조적으로 낮다** — 같은 회차의 다른 lane 이 통합 \
             타깃을 더했으면 통합 트리에서는 더 크고, 최종 값은 병합하는 쪽이 거기서 다시 잰다. \
             이전 회차: c7887abcd — 그 트리에서 다시 셌다. 세 lane 이 각각 통합 타깃을 하나씩 \
             더했다: `tasty-doc-guards/tests/site_vendor_icons_match_the_app_transcription.rs` \
             와 `tasty-doc-guards/tests/drag_to_scroll_is_declared.rs`, 그리고 터미널 소거 lane 의 것. 셋 다 자기 base 에서 \
             116 이라 적었고 **그 값은 각자의 트리에서만 맞았다** — 같은 수가 서로 다른 두 \
             근거를 갖는 형태이고, 병합하는 쪽이 여기서 다시 잰 값이 이것이다. \
             이전 회차: 742b0dbf7 + lane — 그 트리의 추적 계수가 114 이고 그 lane 이 \
             `tasty-doc-guards/tests/githooks_are_pinned.rs` 하나를 더해 115 였다. \
             **lane 트리에서 잰 값이라 구조적으로 낮다** — 같은 회차의 다른 lane 이 \
             통합 타깃을 더했으면 통합 트리에서는 더 크고, 최종 값은 병합하는 쪽이 \
             거기서 다시 잰다. 바로 앞 회차가 그 형태의 실물이다(아래). \
             이전 회차: 5ce79fada + lane — **통합 트리에서 잰 값이다.** \
             두 lane 의 공통 base(`131f3d6eb`)의 추적 트리가 112 이고, 거기에 \
             **lane 둘이 각자 하나씩** 더해 \
             114 다 — main 에 먼저 착지한 `tasty-terminal/tests/erase_boundaries.rs` 와 \
             이 lane 의 `tasty-doc-guards/tests/\
             site_vendor_tokens_track_the_app_export.rs`. **그 둘이 같은 base 에서 각자 \
             113 을 적었다** — 더한 파일이 다른데 값이 같아, 둘 중 하나만 보면 어느 쪽도 \
             틀리지 않았고 차는 이 트리에서만 드러났다. 이 회차가 그 형태의 실물이다. \
             이전 회차: 1c6c5af8d + lane 작업 트리 112 — 그 base 의 추적 트리는 111 이고 \
             그 lane 이 `tasty-terminal/tests/region_autowrap.rs` 하나를 더했다. 같은 \
             회차의 다른 lane 이 통합 타깃을 더했으면 통합 트리에서는 더 크다 — \
             아래 f53234985 이력이 그 형태를 기록한 것이고, 최종 값은 병합하는 \
             쪽이 통합 트리에서 다시 잰다. 이전 회차: f53234985 — **통합 트리에서 \
             잰 값이다.** 앞선 108 은 한 lane 이 \
             자기 트리에서 잰 것이었고 그 값 자체는 맞았다 \
             (같은 값이 직전 push 지점에서도 108 이었다). 그 뒤 통합에서 lane 셋이 \
             각자 통합 타깃을 하나씩 더해 111 이 됐다 — `tasty-i18n` 의 \
             `font_validation` · `plugin_catalog`, `tasty-plugin-sdk` 의 \
             `plugin_user_overrides`. **lane 트리에서 잰 값은 구조적으로 낮다** — \
             한 lane 은 자기가 더한 하나만 보므로, 셋이 같은 base 에서 각자 +1 을 \
             적으면 셋 다 같은 값을 쓰고 차는 통합에서만 드러난다(앞선 회차 실측: \
             12bc0f4b2 + 그 통합 트리에서 base 102, 28 커밋 시점 104, 41 커밋 시점 \
             105). 최종 값은 병합하는 쪽이 통합 트리에서 다시 잰다.",
        ),
        how: "`crates/<크레이트>/tests/<파일>.rs` — 네 마디짜리 경로만. 재는 법: `/` 로 \
              쪼갠 마디가 넷이고 둘째 마디가 `tests` 이며 `.rs` 로 끝나는 줄.",
    };

    /// `docs/` 아래 `.md` 전부.
    pub const DOCS_MD: Population = Population {
        measured: 559,
        measured_on: "2026-09-22",
        counted_on: super::CountedOn::Tree(
            "6c3c9a4fc — 다음 회차 lane 들이 착지한 main 트리에서 다시 셌다. 558 -> 559 이고 \
             삭제는 0 이다. 더해진 것은 \
             `docs/adr/0497-an-agent-created-window-does-not-take-the-users-focus.md` \
             하나(`5a3cee3d5`, 에이전트 창 포커스 lane)다. 그 lane 이 이 항을 안 건드려 push \
             가 pre-push B.10 에서 막혔다 — 아래 이력의 형태가 열 번째로 났다. \
             이전 회차: 86d08cc85 — 여러 lane 이 착지한 main 트리에서 다시 셌다. 502 -> 558 이고 삭제는 \
             0 이라 **감소 0 이 이어졌다.** 더해진 56 중 55 가 `docs/adr/`(0350~0490 사이)이고 \
             하나가 `docs/dev-guide/app-state-ownership.md` 다. 아래 이력이 여덟 번 적은 형태가 \
             아홉 번째로 났다 — lane 마다 ADR 을 더하면서 아무도 자기 base 에서 이 항을 안 \
             건드렸고, 이번에는 pre-push B.10 이 병합된 트리에서 그것을 잡았다. \
             이전 회차: c81b7454b — headless 정의 경계 회차가 문서 둘을 들여왔다. 500 -> 502 이고 \
             삭제는 0 이다(`docs/adr/0346-headless-compiles-only-what-it-reaches.md` 와 \
             `docs/dev-guide/headless-build-boundaries.md`). ★ 아래 이력에 적힌 `0346` 은 \
             **다른 문서**다 — 그 번호를 달고 있던 것은 e2e 디스플레이 결정의 중복 사본이라 \
             지워졌고(정본은 0297), 비워진 번호를 이 회차가 새 결정에 다시 썼다. \
             이전 회차: 0f37f8565 — 마지막 lane 묶음이 착지한 트리에서 다시 셌다. 501 -> 500 이고 \
             **줄었다** — 마지막 묶음이 ADR 하나를 들여왔고, 같은 결정의 중복 사본 \
             둘을 함께 지웠다(그 셋은 본문이 같았다). 이전 회차: 65bcd1988 — 위 두 문서가 이 트리에 들었다(`docs/adr/0280-cli-help-localizes-presentation-not-protocol.md` \
             와 `docs/dev-guide/paired-agent-handlers.md`). 499 -> 501 이고 삭제는 0 이다. \
             이전 회차: 5120fcda1 — 같은 트리에서 다시 셌다. 493 → 499, 삭제 0 이라 단조 증가가 이어졌다. \
             내역이 이 모수의 함정을 그대로 보여 준다: 여섯 중 **다섯이 착지한 lane 이 들고 \
             온 ADR** 이고(0281 · 0284 · 0301 · 0302 · 0346) 하나만 이 회차가 새로 쓴 것이다. \
             앞선 값 493 은 아래 좌표에서 쟀고 그 트리에서는 맞았다. 그 앞의 이력은 \
             이어서 적는다 — afcb71f32 — 네 번째 물결 여덟 lane 이 착지한 트리에서 다시 셌다. 485 에서 8 이 \
             늘었고 **여덟 전부가 `docs/adr/` 이다**(0337~0344, lane 하나에 하나씩). \
             삭제는 0 이라 단조 증가가 또 이어졌다. 그리고 아래 ★ 가 예고한 형태가 \
             **여덟 번째로** 그대로 났다 — lane 마다 ADR 을 하나씩 더하면서 아무도 자기 \
             base 에서 이 항을 안 건드렸고, 선언이 485 인 채 여덟 lane 의 게이트가 전부 \
             통과했다. 착지 뒤에 재야 드러난다는 사실 자체는 이제 예외가 아니라 규칙이다. \
             앞선 값 485 는 `36599e609` 에서 쟀고 그 트리에서는 맞았다 — 455 에서 30 이 \
             늘었고 그 대부분이 `docs/adr/` 이었다. ★ 이 모수가 조용히 거짓이 되는 형태가 \
             여기서도 그대로 반복됐다 — lane 마다 ADR 을 하나씩 더하면서 아무도 자기 \
             base 에서 이 항을 안 건드렸고, 선언이 455 인 채로 모든 lane 의 게이트가 \
             통과했다. 착지 뒤 `--rev HEAD` 로 재야 드러난다. 앞선 값 455 는 `194c8653a` \
             에서 쟀고 그 트리에서는 맞았다. 앞선 값 454 는 `c7887abcd` 에서 쟀고 그 트리에서는 맞았다. 그 앞의 453 은 `a6aa0b4b3` 에서 쟀고 그 트리에서는 맞았다. 그 앞의 452 는 `194c8653a` 에서 쟀고 그 트리에서는 맞았다. 그 앞의 451 은 `353038a3e` 에서 쟀고 그 트리에서는 맞았다. 그 앞의 450 은 `4e379d048` 에서 쟀고 그 트리에서는 \
             맞았는데, 그 뒤 착지한 lane 이 문서를 더해 451 이 됐다. 그 앞의 449 는 `c15f504f4` 에서 쟀고 그 트리에서는 \
             맞았는데, 그 뒤 착지한 lane 이 ADR 하나를 더하면서 450 이 됐다. 그 lane 은 \
             자기 base 에서 이 항을 안 건드려 선언이 449 인 채 게이트를 전부 통과했다 — \
             **같은 형태가 연속 두 번** 났고, 두 번 다 원인은 `docs/adr/` 에 문서 하나가 \
             는 것이다. 이 모수가 조용히 거짓이 되는 형태의 일곱 번째 사례다. \
             `c15f504f4` 판의 기록: 셸 자산 lane 이 ADR 하나를 더했는데 자기 base 에서는 \
             이 항을 안 건드려 선언이 448 인 채 통과했다. 앞선 값 448 은 \
             `91ca7d37d` 에서 쟀고 그 트리에서는 맞았다. 그 앞의 407 은 `12bc0f4b2` 에서 \
             쟀고 ★ 그 좌표는 지금 **main 에서 도달 불가**다 — 사정은 [`SRC_RS`] 의 같은 \
             자리에 적었다.",
        ),
        how: "경로가 `docs/` 로 시작하고 `.md` 로 끝나는 것 전부 — 깊이 제한이 없다. \
              동역학도 함께 적는다: **감소가 여전히 한 번도 없다.** 앞선 창(1215 커밋, \
              2026-09-05~09-08)에서 354 에서 399 로 단조 증가했고 한 커밋 최대 이동이 1 \
              이었는데, 그 창의 좌표는 지금 도달 불가라 **도달 가능한 창에서 다시 쟀다**: \
              `fdca139c0`..`91ca7d37d` 의 558 커밋(직선 이력)에서 285 에서 448 로 가는 \
              동안 **감소 0 회** · 증가 51 회다. 다만 최대 증가가 +46 으로 앞선 창의 1 보다 \
              훨씬 크다 — 그 걸음들은 하루치를 묶은 `roll up` 커밋이라 내용의 증가가 아니라 \
              이력을 조립한 방식이다. 단조성은 두 창에서 같고 **진폭은 창마다 다르다.** \
              이 모수를 재는 두 가드가 한때 '회차마다 늘고 줄어서' 와 '단조 증가해 왔다' 로 \
              서로 반대되는 근거를 적고 있었다 — 뒤엣것이 맞다. 세 번째 창도 같은 방향이다: \
              `75911f249`..`36599e609` 의 165 커밋에서 455 에서 485 로 가는 동안 **감소 0 회** \
              · 증가 29 회 · 최대 증가 +2 다. 진폭이 앞 창(+46)보다 작은 것은 이 창이 \
              `roll up` 없이 커밋 하나에 ADR 하나씩 들어온 이력이기 때문이다.",
    };
}

/// 디렉토리를 내려갈지 정하는 방식.
///
/// 이름이 아니라 **성질**로 가른다. 빌드 산출물 디렉토리의 이름은 `CARGO_TARGET_DIR`
/// 하나로 무엇이든 될 수 있고, 이름 목록은 그것을 못 따라간다. cargo 가 만든
/// 디렉토리는 `CACHEDIR.TAG` 를 갖는다.
pub enum Descend {
    /// 전부 내려간다. 순회 루트 아래에 가지칠 것이 없다고 **판정한** 자리에 쓴다 —
    /// 그 판정 자체를 지키는 것은 이 모듈이 아니라 그 자리의 가드 몫이다.
    Everything,
    /// 빌드 캐시 디렉토리와 패키지 관리자의 의존 트리를 건너뛴다 —
    /// [`crate::is_build_cache_dir`] · [`crate::is_dependency_tree_dir`].
    ///
    /// 앞엣것은 표식(`CACHEDIR.TAG`)으로, 뒤엣것은 이름(`node_modules`)으로 가른다. 이름을
    /// 쓰는 근거는 그 함수 주석에 있다 — 그 이름은 도구가 박아 둔 것이라 자유롭지 않다.
    SkipBuildCaches,
    /// 위 둘에 더해 **점으로 시작하는 디렉토리**도 건너뛴다.
    ///
    /// 레포 루트부터 훑는 자리에 쓴다. 커밋되지 않는 로컬 작업 폴더는 clone·CI 에
    /// 없지만 **개발자의 작업 트리에는 있고**, 거기에는 문서 사본·게이트 결과물이
    /// 쌓인다. 그것을 세면 같은 커밋이 기계마다 다른 수를 낸다 — 아래 심볼릭 링크
    /// 주석이 적어 둔 것과 같은 병이고, 방향만 반대다(worktree 에서는 그 폴더가
    /// 링크라 안 세어지고, 원본 저장소에서는 실물이라 세어진다). 실측 2026-09-08:
    /// `cited_anchors_resolve` 의 좌변이 worktree 433 · 원본 874 로 갈렸다.
    ///
    /// 이름이 아니라 **형태**로 가른다 — 그 폴더 이름을 추적 소스에 적는 것은 다른
    /// 규율에 걸린다(`no_todo_file_citation` P6). `.git`·`.github` 도 이 규칙에
    /// 흡수되므로, `.github/` 아래를 봐야 하는 순회는 그 디렉토리를 **루트로** 넘겨라
    /// (루트 자신은 이 판정을 안 받는다).
    SkipBuildCachesAndDotDirs,
}

/// 순회가 찾은 파일 하나 — 절대 경로와 **정규화된** repo-relative 경로를 짝으로 낸다.
///
/// 짝으로 내는 이유가 있다. 소비자는 파일을 다시 열어야 해서 절대 경로가 필요하고,
/// 명부·면제 목록과 비교하려면 상대 경로가 필요하다. 하나만 내면 소비자가 나머지를
/// 자기 손으로 만들고, **그 "다시 만드는 자리" 가 없애려는 것 자체다.**
///
/// [`Walked::rel`] 의 구분자는 어느 플랫폼에서든 `/` 다. `strip_prefix` 의 결과는 그
/// 플랫폼 구분자를 그대로 물고 나오므로, 소비자가 그것을 펴서 소스에 박힌 `/` 리터럴과
/// 비교하면 Windows 에서 전부 빗나간다 — 그리고 **그 어긋남은 예외가 아니라 조용한 0** 이다.
/// 명부 조회가 모조리 실패하고 가드는 "명부에 없다" 고 보고한다.
///
/// 이 성질은 Linux 에서 잴 수 없다(여기서는 고치기 전에도 `/` 가 나온다). 그래서 Linux
/// 에서 도는 단정을 두지 않는다 — 대신 잴 수 있는 것을 잡는다: 소비자가 정규화를 자기
/// 손으로 하는 자리가 남아 있는지는 텍스트로 셀 수 있고,
/// `floored_walk_consumers_do_not_renormalize` 가 그것을 본다.
#[derive(Debug)]
pub struct Walked {
    /// 파일을 다시 열 때 쓴다.
    pub path: PathBuf,
    /// 비교에 쓴다. 구분자는 언제나 `/`.
    pub rel: String,
}

impl Floor {
    /// 선언 자체가 말이 되는지. 순회를 돌기 **전에** 본다 — 앞뒤가 안 맞는 하한으로
    /// 순회를 돌면 그 결과가 무엇을 뜻하는지 아무도 모른다.
    fn validate(&self) -> Result<(), String> {
        if self.min == 0 {
            return Err(
                "하한이 0 이다 — 아무것도 못 모아도 통과한다. 그것은 하한이 아니라 \
                 하한이 있다는 외양이다. 실제로 세서 그 절반쯤을 넣어라."
                    .to_string(),
            );
        }
        if self.min > self.measured {
            return Err(format!(
                "하한 {} 이 마지막 실측 {} 보다 크다 — 이 선언대로면 이 순회는 잰 그날에도 \
                 실패했어야 한다. 둘 중 하나가 낡았다: 다시 세서 `measured`·`measured_on` \
                 을 함께 갱신하거나, 하한이 과했던 것이면 이유와 함께 내려라.",
                self.min, self.measured
            ));
        }
        if self.measured_on.len() != 10 || self.measured_on.matches('-').count() != 2 {
            return Err(format!(
                "`measured_on` 이 `YYYY-MM-DD` 가 아니다: {:?}. 잰 날이 없으면 실측값은 \
                 언제 것인지 모르는 수가 되고, 그런 수는 갱신 대상으로 안 보인다.",
                self.measured_on
            ));
        }
        self.counted_on.validate()?;
        if self.why_this_gap.split_whitespace().count() < 10 {
            return Err(format!(
                "`why_this_gap` 이 너무 짧다({} 낱말) — 간격의 크기는 이 모수가 얼마나 빨리 \
                 움직이는가의 판단이고, 그 판단을 안 적으면 다음 사람이 간격만 보고 아무 \
                 방향으로나 고친다. 이 모수가 무엇의 함수인지, 무엇이 그것을 움직이는지 \
                 적어라. ★ 이 문턱을 내려서 통과시키지 마라.",
                self.why_this_gap.split_whitespace().count()
            ));
        }
        Ok(())
    }
}

/// 디렉토리를 만났을 때 무엇을 할지.
pub enum Pick {
    /// 안 모으고 그 아래로 계속 내려간다.
    Skip,
    /// 모으고 그 아래로도 계속 내려간다.
    Take,
    /// 모으고 그 아래로는 안 내려간다. 그 안이 이 물음의 답에 안 들어갈 때 쓴다 —
    /// 빌드 캐시를 찾는 순회가 캐시 내부를 훑으면 그 가드 자신이 사고의 규모를 재현한다.
    TakeAndStop,
}

/// `root` 아래를 재귀 순회해 `pick` 이 고르는 **디렉토리**를 모은다.
///
/// 파일 순회와 물음이 다르고, **하한을 거는 대상도 다르다.** 여기서 하한은 모은 수가
/// 아니라 **훑은 수**에 걸린다 — 조건에 맞는 디렉토리가 0 개인 것은 정상일 수 있지만
/// (찾는 것이 원래 없을 수 있다), 훑은 디렉토리가 0 이면 그것은 순회가 죽은 것이다.
/// 그 둘을 안 가르면 "없어서 0" 과 "못 봐서 0" 이 같은 초록이 된다.
#[must_use = "하한 미달은 이 Result 로만 나온다 — 버리면 순회가 죽어도 조용히 통과한다"]
pub fn walk_dirs_with_floor(
    root: &Path,
    rel_base: &Path,
    floor: &Floor,
    pick: &dyn Fn(&Walked) -> Pick,
) -> Result<Vec<Walked>, String> {
    floor.validate().map_err(|why| {
        format!(
            "순회 하한 선언이 앞뒤가 안 맞는다 ({}): {why}",
            root.display()
        )
    })?;

    let mut out = Vec::new();
    let mut walked = 0usize;
    collect_dirs(root, rel_base, pick, &mut out, &mut walked);
    out.sort_by(|a, b| a.rel.cmp(&b.rel));

    if walked < floor.min {
        return Err(format!(
            "{} 순회가 디렉토리를 {} 개만 훑었다(하한 {}) — 모은 것이 {} 개인데, 이 수가 \
             적다는 것은 찾는 것이 없다는 뜻이 아니라 아무 데도 안 가 봤다는 뜻이다.\n\
             마지막 실측은 {} 의 {} 개였고, 하한을 그보다 낮게 잡은 이유는 이렇다: {}\n\
             ★ 하한을 내려서 통과시키지 마라 — 순회가 죽은 것을 그대로 승인하는 것이다.\n\
             순서가 있다. (1) 순회 루트와 심볼릭 링크와 `read_dir` 실패를 먼저 확인한다. \
             (2) 트리가 정말 얕아진 것이면 실제 수를 다시 세서 `measured`·`measured_on` 을 \
             함께 갱신한다. (3) 그러고 나서 `why_this_gap` 이 여전히 맞는지 다시 읽는다.",
            root.display(),
            walked,
            floor.min,
            out.len(),
            floor.measured_on,
            floor.measured,
            floor.why_this_gap,
        ));
    }
    Ok(out)
}

fn collect_dirs(
    dir: &Path,
    rel_base: &Path,
    pick: &dyn Fn(&Walked) -> Pick,
    out: &mut Vec<Walked>,
    walked: &mut usize,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // 파일 순회와 같은 이유로 링크를 안 따라간다 — 레포 밖으로 새면 모수가 작업
        // 트리의 종류를 읽는다.
        if kind.is_symlink() || !kind.is_dir() {
            continue;
        }
        let path = entry.path();
        *walked += 1;
        let found = Walked {
            rel: normalized_rel(&path, rel_base),
            path,
        };
        match pick(&found) {
            Pick::Skip => collect_dirs(&found.path, rel_base, pick, out, walked),
            Pick::Take => {
                let next = found.path.clone();
                out.push(found);
                collect_dirs(&next, rel_base, pick, out, walked);
            }
            Pick::TakeAndStop => out.push(found),
        }
    }
}

/// `root` 아래를 재귀 순회해 `keep` 이 참인 파일을 모은다. 모인 수가 하한에 못 미치면
/// **결과 대신 이유를 돌려준다.**
///
/// 소비자는 이렇게 쓴다:
///
/// ```ignore
/// let files = walk_with_floor(&root.join("src"), &FLOOR, Descend::Everything, &|p| {
///     p.extension().is_some_and(|e| e == "rs")
/// })
/// .unwrap_or_else(|why| panic!("{why}"));
/// ```
///
/// `Result` 를 버리면 하한이 없는 것과 같아지므로 `#[must_use]` 를 붙였다.
#[must_use = "하한 미달은 이 Result 로만 나온다 — 버리면 순회가 죽어도 조용히 통과한다"]
pub fn walk_with_floor(
    root: &Path,
    rel_base: &Path,
    floor: &Floor,
    descend: Descend,
    keep: &dyn Fn(&Walked) -> bool,
) -> Result<Vec<Walked>, String> {
    floor.validate().map_err(|why| {
        format!(
            "순회 하한 선언이 앞뒤가 안 맞는다 ({}): {why}",
            root.display()
        )
    })?;

    let mut out = Vec::new();
    collect(root, rel_base, &descend, keep, &mut out);
    // `rel` 로 정렬한다 — 문자열이라 순서가 플랫폼에 안 흔들린다.
    out.sort_by(|a, b| a.rel.cmp(&b.rel));

    if out.len() < floor.min {
        return Err(format!(
            "{} 순회가 {} 개만 모았다(하한 {}) — 이 상태에서 뒤따르는 \"위반 0\" 은 위반이 \
             없다는 뜻이 아니라 아무것도 안 봤다는 뜻이다.\n\
             마지막 실측은 {} 의 {} 개였고, 하한을 그보다 낮게 잡은 이유는 이렇다: {}\n\
             ★ 하한을 내려서 통과시키지 마라 — 순회가 죽은 것을 그대로 승인하는 것이다.\n\
             순서가 있다. (1) 순회 루트와 가지치기와 `read_dir` 실패를 먼저 확인한다. \
             (2) 대상이 정말 줄어든 것이면 실제 수를 다시 세서 `measured`·`measured_on` 을 \
             함께 갱신한다. (3) 그러고 나서 `why_this_gap` 이 여전히 맞는지 다시 읽는다 — \
             모수가 움직이는 속도가 바뀌었으면 간격도 바뀐다.",
            root.display(),
            out.len(),
            floor.min,
            floor.measured_on,
            floor.measured,
            floor.why_this_gap,
        ));
    }
    Ok(out)
}

fn collect(
    dir: &Path,
    rel_base: &Path,
    descend: &Descend,
    keep: &dyn Fn(&Walked) -> bool,
    out: &mut Vec<Walked>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // **심볼릭 링크는 따라가지 않는다.** 따라가면 순회가 레포 밖 실제 경로로 새고,
        // 그 순간 모수가 작업 트리의 종류를 읽는다 — worktree 에는 레포 밖을 가리키는
        // 링크가 있고 보통의 clone 에는 없어서, 같은 커밋이 기계마다 다른 수를 낸다.
        // 실측 2026-09-06: 이 레포에서 따라가면 `.rs` 가 15 개 더 세어졌고, 그 15 는
        // 커밋되지 않는 로컬 작업 폴더의 것이라 어느 가드의 대상도 아니다.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if matches!(
                descend,
                Descend::SkipBuildCaches | Descend::SkipBuildCachesAndDotDirs
            ) && (crate::is_build_cache_dir(&path) || crate::is_dependency_tree_dir(&path))
            {
                continue;
            }
            if matches!(descend, Descend::SkipBuildCachesAndDotDirs)
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            collect(&path, rel_base, descend, keep, out);
        } else {
            let found = Walked {
                rel: normalized_rel(&path, rel_base),
                path,
            };
            if keep(&found) {
                out.push(found);
            }
        }
    }
}

/// `rel_base` 기준 상대 경로를 만들고 구분자를 `/` 로 편다.
///
/// **정규화는 여기 한 곳에서만 한다.** 소비자마다 하면 언젠가 한 곳이 빠뜨리고, 빠뜨린
/// 쪽은 예외가 아니라 조용한 0 을 낸다 — 소스에 박힌 `/` 리터럴과의 비교가 모조리
/// 빗나가고, 가드는 "명부에 없다" 고 보고한다.
///
/// [`walk_with_floor`] 가 파일마다 이것을 부르지만, 디렉토리를 모으거나 경로 하나를
/// 다루는 자리는 순회를 안 거친다. 그런 자리도 자기 손으로 펴지 말고 이것을 불러라 —
/// 그래야 "정규화하는 자리" 가 저장소에 하나로 남고, 그 하나만 맞으면 전부 맞는다.
///
/// **재는 채널**: 이 함수가 옳은지는 `crossplatform-check` 의 `check-windows` 만 잰다
/// (Linux 에서는 고치기 전에도 `/` 가 나온다). 소비자가 이것을 안 부르고 자기 손으로
/// 펴는 자리가 남아 있는지는 `floored_walk_consumers_do_not_renormalize` 가 잰다.
pub fn normalized_rel(path: &Path, rel_base: &Path) -> String {
    path.strip_prefix(rel_base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 이 판정들이 무엇에 반응하는지 보이려면 실제 트리가 필요하다. 소스에 심은 문자열로
    /// 대신하면 순회가 죽어도 통과한다 — 그것이 바로 이 모듈이 막으려는 형태다.
    struct Tree(PathBuf);

    impl Tree {
        fn new(files: &[&str], cache_dirs: &[&str]) -> Self {
            // 유일성 키에 **시각을 안 쓴다.** 시각의 해상도는 플랫폼의 성질이라,
            // 같은 코드가 어떤 OS 에서는 유일하고 어떤 OS 에서는 겹친다 — 겹치면 두
            // 픽스처가 같은 디렉토리를 쓰고, 먼저 끝난 쪽의 `Drop` 이 다른 쪽이 순회
            // 중인 트리를 지운다. 그 결과는 "파일이 하나 모자란다" 라서 **가지치기
            // 결함처럼 보인다** — 실제로 2026-09-08 에 macOS 러너에서
            // `build_caches_are_skipped_only_when_asked` 가 그 모습으로 죽었고,
            // Linux 에서는 안 죽었다.
            //
            // 기전은 이 항을 상수로 바꿔 재현했다(2026-09-08 실측: 여덟 시험이 한꺼번에
            // 죽는다 — 부분 충돌은 그중 하나만 죽인다). 그래서 단조 카운터로 바꾼다:
            // 해상도가 없는 값이라 플랫폼을 안 읽는다.
            //
            // ★ 이 레포는 이미 그것을 알고 적어 뒀다 — `temp_path` 모듈이 유니크화
            //   성분을 등급으로 나누며 시간 nonce 를 **"가장 약하다"** 로 분류하고
            //   "새 코드는 다른 둘을 쓰는 게 낫다" 고 말한다. 이 픽스처는 그 권고가
            //   생기기 전에 쓰였고, 예고된 사고가 그대로 났다. 판정은 안 바뀐다 —
            //   `process::id` 가 같은 창에 있어 그 가드에는 계속 유니크화로 보인다.
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let base = std::env::temp_dir().join(format!(
                "tasty-floored-walk-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            for rel in files {
                let path = base.join(rel);
                std::fs::create_dir_all(path.parent().expect("파일에 부모가 없다")).unwrap();
                std::fs::write(&path, b"x").unwrap();
            }
            for rel in cache_dirs {
                let dir = base.join(rel);
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    dir.join("CACHEDIR.TAG"),
                    b"Signature: 8a477f597d28d172789f06886806bc55",
                )
                .unwrap();
            }
            Self(base)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            // 정리 실패는 무시한다 — 판정은 이미 끝났고, 여기서 `unwrap` 을 쓰면 임시
            // 디렉토리 삭제 실패가 테스트의 빨강으로 둔갑한다. 남아도 임시 경로다.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn floor(min: usize, measured: usize) -> Floor {
        Floor {
            min,
            measured,
            measured_on: "2026-09-06",
            counted_on: CountedOn::SyntheticTree,
            why_this_gap: "시험용 고정값이다 — 이 모수는 테스트 안에서만 살고 아무것도 \
                           따라가지 않으므로 간격에 뜻이 없다",
        }
    }

    fn all(_: &Walked) -> bool {
        true
    }

    #[test]
    fn a_walk_that_meets_its_floor_returns_what_it_found() {
        let t = Tree::new(&["a.rs", "sub/b.rs", "sub/deep/c.rs"], &[]);
        let got = walk_with_floor(&t.0, &t.0, &floor(3, 3), Descend::Everything, &all)
            .expect("셋을 모았으면 통과해야 한다");
        assert_eq!(got.len(), 3, "재귀가 죽으면 얕은 자리만 모은다");
    }

    #[test]
    fn a_dead_walk_returns_why_instead_of_an_empty_list() {
        let t = Tree::new(&["a.rs"], &[]);
        let why = walk_with_floor(&t.0, &t.0, &floor(3, 10), Descend::Everything, &all)
            .expect_err("하한 미달인데 통과했다");
        assert!(
            why.contains("1 개만 모았다") && why.contains("하한 3"),
            "실패문이 실제로 몇 개를 모았는지 말하지 않는다: {why}"
        );
        assert!(
            why.contains("하한을 내려서 통과시키지 마라"),
            "금지가 빠졌다 — 이 문장이 없으면 다음 사람의 최소 수선은 하한을 내리는 것이다: {why}"
        );
    }

    #[test]
    fn the_keep_predicate_actually_filters() {
        let t = Tree::new(&["a.rs", "b.md", "c.md"], &[]);
        let got = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::Everything, &|w| {
            w.rel.ends_with(".md")
        })
        .expect("둘을 모았으면 통과해야 한다");
        assert_eq!(
            got.len(),
            2,
            "`keep` 을 안 보고 전부 모으거나 아무것도 안 모은다"
        );
    }

    /// 가지치기는 **양방향으로** 재야 무정보가 아니다. 한쪽만 보면 "건너뛴다" 와
    /// "애초에 그 파일이 없다" 가 구별되지 않는다.
    #[test]
    fn build_caches_are_skipped_only_when_asked() {
        let t = Tree::new(&["keep.rs", "cache/inside.rs"], &["cache"]);
        let skipped = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::SkipBuildCaches, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("바깥 파일 하나는 남는다");
        assert_eq!(skipped.len(), 1, "빌드 캐시 안을 들여다봤다");

        let everything = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::Everything, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("둘 다 모인다");
        assert_eq!(
            everything.len(),
            2,
            "`Everything` 인데도 건너뛴다 — 그러면 위 검사의 초록은 가지치기의 증거가 아니다"
        );
    }

    /// 위와 같은 양방향 대조 — 의존 트리 쪽. 표식이 아니라 이름으로 가르는 갈래라
    /// 위 시험은 이 자리를 안 덮는다(`CACHEDIR.TAG` 를 안 만든다).
    #[test]
    fn dependency_trees_are_skipped_only_when_asked() {
        let t = Tree::new(&["keep.rs", "node_modules/dep/inside.rs"], &[]);
        let skipped = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::SkipBuildCaches, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("바깥 파일 하나는 남는다");
        assert_eq!(skipped.len(), 1, "`node_modules` 안을 들여다봤다");

        let everything = walk_with_floor(&t.0, &t.0, &floor(1, 1), Descend::Everything, &|w| {
            w.rel.ends_with(".rs")
        })
        .expect("둘 다 모인다");
        assert_eq!(
            everything.len(),
            2,
            "`Everything` 인데도 건너뛴다 — 그러면 위 검사의 초록은 가지치기의 증거가 아니다"
        );
    }

    /// 결과 순서는 소비자가 기대는 성질이다 — 실패문에 목록을 싣는 가드는 순서가 흔들리면
    /// 같은 결함을 매번 다른 모습으로 보고한다. 파일을 스무 개 만드는 것은 그 때문이다:
    /// `read_dir` 이 돌려주는 순서는 파일시스템이 정하고, 셋만 만들면 그것이 우연히
    /// 정렬 순서와 같아 정렬을 지워도 이 대조가 안 죽는다.
    #[test]
    fn the_walk_returns_a_sorted_list() {
        let names: Vec<String> = (0..20).rev().map(|i| format!("f{i:02}.rs")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let t = Tree::new(&refs, &[]);
        let got = walk_with_floor(&t.0, &t.0, &floor(20, 20), Descend::Everything, &all)
            .expect("스무 개를 모았으면 통과해야 한다");
        assert!(
            got.windows(2).all(|p| p[0].rel <= p[1].rel),
            "순회 결과가 정렬돼 있지 않다 — 순서를 파일시스템에 맡기면 같은 결함이 완주마다 \
             다른 순서로 보고된다: {:?}",
            got.iter().map(|w| &w.rel).collect::<Vec<_>>()
        );
    }

    /// 링크를 안 따라간다는 것은 **양방향으로** 재야 한다. 링크 너머 파일이 안 세어지는
    /// 것만 보면 "링크를 안 따라갔다" 와 "거기 파일이 없다" 가 구별되지 않는다.
    /// unix 전용이다 — Windows 의 `symlink_dir` 은 개발자 모드나 관리자 권한을 요구해서
    /// CI 러너에서 권한 오류로 죽는다. 그것은 "링크를 따라갔다" 와 다른 실패라 여기서
    /// 재면 판정이 흐려진다. 이 성질을 Windows 에서 재는 채널은 **없다** — 순회가 링크를
    /// 안 따라간다는 것은 unix 에서만 확인된다.
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_out_of_the_tree() {
        let outside = Tree::new(&["beyond.rs"], &[]);
        let inside = Tree::new(&["here.rs"], &[]);
        std::os::unix::fs::symlink(&outside.0, inside.0.join("link")).unwrap();

        let got = walk_with_floor(
            &inside.0,
            &inside.0,
            &floor(1, 1),
            Descend::Everything,
            &all,
        )
        .expect("트리 안의 파일 하나는 모인다");
        let rels: Vec<&str> = got.iter().map(|w| w.rel.as_str()).collect();
        assert_eq!(
            rels,
            vec!["here.rs"],
            "링크 너머로 순회가 샜다 — 그러면 모수가 작업 트리의 종류를 읽는다"
        );

        // 반대편: 링크가 아니라 진짜 디렉토리였으면 그 파일이 세어진다. 이것이 없으면
        // 위 초록은 "링크를 건너뛰었다" 가 아니라 "거기 아무것도 없었다" 일 수 있다.
        let plain = Tree::new(&["here.rs", "sub/beyond.rs"], &[]);
        let got = walk_with_floor(&plain.0, &plain.0, &floor(1, 1), Descend::Everything, &all)
            .expect("둘 다 모인다");
        assert_eq!(got.len(), 2, "평범한 하위 디렉토리까지 건너뛴다");
    }

    /// 디렉토리 순회의 하한은 **훑은 수**에 걸린다. 모은 수에 걸면 "찾는 것이 원래
    /// 없다" 는 정상 상태가 실패가 된다.
    #[test]
    fn the_directory_walk_floors_what_it_visited_not_what_it_kept() {
        let t = Tree::new(&["a/x.rs", "b/y.rs", "c/z.rs"], &[]);
        // 아무것도 안 고르지만 셋을 훑었으니 통과한다.
        let got = walk_dirs_with_floor(&t.0, &t.0, &floor(3, 3), &|_| Pick::Skip)
            .expect("모은 것이 0 이어도 훑은 것이 하한을 넘으면 통과해야 한다");
        assert!(got.is_empty(), "아무것도 안 골랐는데 모았다");

        // 훑은 것이 모자라면 거부된다.
        let thin = Tree::new(&["a/x.rs"], &[]);
        let why = walk_dirs_with_floor(&thin.0, &thin.0, &floor(3, 3), &|_| Pick::Skip)
            .expect_err("훑은 것이 하한에 못 미치는데 통과했다");
        assert!(
            why.contains("디렉토리를 1 개만 훑었다"),
            "실패문이 훑은 수를 안 말한다: {why}"
        );
    }

    /// `TakeAndStop` 이 실제로 멈추는지 — 그리고 `Take` 는 안 멈추는지. 한쪽만 재면
    /// "멈췄다" 와 "거기 아무것도 없다" 가 구별되지 않는다.
    #[test]
    fn take_and_stop_prunes_while_take_keeps_descending() {
        let t = Tree::new(&["hit/deep/x.rs", "plain/y.rs"], &[]);
        let stop = walk_dirs_with_floor(&t.0, &t.0, &floor(1, 4), &|w| {
            if w.rel.ends_with("hit") {
                Pick::TakeAndStop
            } else {
                Pick::Skip
            }
        })
        .expect("셋 이상 훑는다");
        assert_eq!(
            stop.iter().map(|w| w.rel.as_str()).collect::<Vec<_>>(),
            vec!["hit"],
            "`TakeAndStop` 인데 그 아래로 내려갔다"
        );

        let go = walk_dirs_with_floor(&t.0, &t.0, &floor(1, 4), &|w| {
            if w.rel.contains("hit") {
                Pick::Take
            } else {
                Pick::Skip
            }
        })
        .expect("셋 이상 훑는다");
        assert_eq!(
            go.iter().map(|w| w.rel.as_str()).collect::<Vec<_>>(),
            vec!["hit", "hit/deep"],
            "`Take` 인데 그 아래로 안 내려갔다 — 그러면 위 검사의 초록은 가지치기의 증거가 아니다"
        );
    }

    #[test]
    fn a_floor_of_zero_is_refused_before_the_walk_runs() {
        let t = Tree::new(&[], &[]);
        let why = walk_with_floor(&t.0, &t.0, &floor(0, 10), Descend::Everything, &all)
            .expect_err("하한 0 을 받아들였다");
        assert!(
            why.contains("하한이 0 이다"),
            "0 을 다른 이유로 거부했다: {why}"
        );
    }

    #[test]
    fn a_floor_above_its_own_measurement_is_refused() {
        let t = Tree::new(&["a.rs"], &[]);
        let why = walk_with_floor(&t.0, &t.0, &floor(11, 10), Descend::Everything, &all)
            .expect_err("실측보다 높은 하한을 받아들였다");
        assert!(why.contains("보다 크다"), "다른 이유로 거부했다: {why}");
    }

    #[test]
    fn an_undated_measurement_is_refused() {
        let t = Tree::new(&["a.rs"], &[]);
        let bad = Floor {
            measured_on: "얼마 전",
            counted_on: CountedOn::SyntheticTree,
            ..floor(1, 10)
        };
        let why = walk_with_floor(&t.0, &t.0, &bad, Descend::Everything, &all)
            .expect_err("날짜 없는 실측을 받아들였다");
        assert!(why.contains("YYYY-MM-DD"), "다른 이유로 거부했다: {why}");
    }

    #[test]
    fn a_gap_without_a_reason_is_refused() {
        let t = Tree::new(&["a.rs"], &[]);
        let bad = Floor {
            why_this_gap: "적당히",
            ..floor(1, 10)
        };
        let why = walk_with_floor(&t.0, &t.0, &bad, Descend::Everything, &all)
            .expect_err("사유 없는 간격을 받아들였다");
        assert!(why.contains("너무 짧다"), "다른 이유로 거부했다: {why}");
    }
}
