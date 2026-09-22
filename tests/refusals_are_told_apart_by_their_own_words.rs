//! 게이트의 **거절 자리마다 자기 말이 있고, 그 말을 단정하는 시험이 있는가**를 묻는다.
//!
//! 왜 종료 코드로는 이 물음에 답할 수 없나. 이 계열 게이트의 거절은 공용 `die()` 하나를
//! 여러 곳에서 부르고, 그 함수는 **전부 같은 rc** 로 나간다. 그래서 `die` 의 **정의**를
//! 통과로 바꾸면 시험 여럿이 한꺼번에 죽지만, **호출 지점 하나**를 폴백으로 바꾸면
//! 뒤에 오는 다른 호출 지점이 같은 rc 를 대신 내주어 아무 시험도 안 죽는다. rc 는 그
//! 자리들을 **못 가른다** — 가르는 채널은 그 자리가 찍는 **문구**뿐이다.
//!
//! 실측 2026-09-08 이 이 물음을 만들었다. `check-frozen-sum-ratchet.sh` 의 호출 지점 열에
//! 완화 변이를 하나씩 넣으니 넷만 죽고 여섯이 살아남았고, `check-plugin-version-bump.sh`
//! 의 열여섯에서는 넷만 죽었다. `CLAUDE.md` 는 그 게이트에 대해 "판정 불가는 통과가 아니라
//! 실패다" 라고 적는데, 그 문장이 이름을 부른 셋 밖에서는 아직 안 걸린다.
//!
//! **이름을 부르는 표를 만들지 않는다.** 좌변을 `scripts/` 에서 세므로, 거절 자리를 하나
//! 더 만드는 사람이 이 파일을 안 고쳐도 그 자리가 이 물음을 받는다. 상수로 목록을 복사해
//! 두면 새로 들어오는 것이 안 세어진다 — 형제 `gates_pin_their_judge_absence.rs` 와 같은
//! 규율이다.
//!
//! ── 이 가드가 묻는 것과 **안 묻는 것** ──────────────────────────────────
//!
//! 묻는 것: "이 거절 자리의 문구 중, **다른 거절과 구별되는 조각**을 단정하는 시험
//! 리터럴이 있는가." 안 묻는 것: 그 자리가 **닿을 수 있는가**(도달성), 그 단정이 그
//! 게이트를 실제로 돌려 나온 출력에 걸렸는가.
//!
//! 그래서 이 가드는 변이 census 와 **같은 물음이 아니다.** 실측으로 둘을 나란히 재
//! 봤다(2026-09-08, 두 게이트 26 자리): 변이가 죽인 자리 13, 이 가드가 "자기 말 있음"
//! 이라 한 자리 6. **17 자리에서 답이 같고 7 에서 갈렸다.**
//!
//! **갈린 일곱은 전부 한 방향이다** — 변이는 죽는데 자기 말이 없는 자리. 그 자리를 죽인
//! 시험은 **rc 만 단정한다**(예: `a_sibling_without_scan_dirs_is_undecidable` 은
//! `assert_eq!(rc, 2)` 뿐이다). rc 2 는 열여섯 자리가 함께 쓰는 값이라 그 초록은 어느
//! 자리였는지 못 말하고, 그래서 그 자리를 완화해도 다른 자리가 대신 답한다. 반대 방향
//! (자기 말이 있는데 변이가 안 죽는 자리)은 **0 이다.** 즉 이 가드는 변이 census 의
//! 부분집합을 세되, 세는 것이 **가르는 힘이 있는 초록**뿐이다.
//!
//! 그 "0" 은 처음부터 0 이 아니었다. 증거를 "판정문 안에 있는 조각" 으로만 잡았을 때는
//! `Cargo.toml` 처럼 그 게이트 어디에나 있는 일반 토큰이 증거로 섰다. 그래서 증거는
//! **그 게이트 안에서 그 판정문 밖에는 안 나오는 조각**이어야 하고, **다른 거절의
//! 판정문에도 없어야** 한다.
//!
//! ── 왜 잔여 0 이 아니라 상한 래칫인가 ───────────────────────────────────
//!
//! 지금 못 무는 자리가 스무 언저리다. 그 전부에 사유를 붙여 예외 목록에 넣으면 그것이
//! 곧 사람이 유지하는 표가 되고, "아직 안 물렸다" 는 사유가 아니라 **빚**이다. 래칫은
//! 그 빚을 값으로 들고 있으면서 늘지 못하게 한다 — 그리고 **줄어도 실패한다**(상한을
//! 같이 내리라는 뜻이다: 남는 여유가 곧 안 보는 구간이다). 선례는
//! `scripts/check-allow-reason.sh` 와 `scripts/check-shared-walk-ratchet.sh` 다.
//!
//! **`cfg` 로 자기를 빼지 않는다.** 형제들과 달리 이 가드는 셸을 부르지 않고 파일만
//! 읽으므로, `bash` 가 없는 플랫폼에서도 같은 답을 낸다. 경로 구분자는 공용 순회가 이미
//! `/` 로 폈다.
//!
//! 거절을 종료 코드가 아니라 자기 문구로 가르고 래칫으로 드는 결정의 근거·대안:
//! `docs/adr/0206-a-refusal-is-told-apart-by-its-own-words.md`.

use std::fs;
use std::path::{Path, PathBuf};

use tasty_doc_guards::floored_walk::{Descend, Floor, Walked, walk_with_floor};

/// `scripts/` 의 `.sh` 순회 하한.
///
/// 순회가 죽어 거절 자리를 하나도 못 모으면 **안 물린 자리 0 으로** 나가는데, 이 가드는
/// 양방향이라 그 상태는 상한 미달로 빨개진다. 그래도 하한을 따로 두는 이유는 실패문이
/// 다르기 때문이다 — "래칫을 조여라" 가 아니라 "순회가 죽었다" 라고 말해야 한다.
const SCRIPT_FLOOR: Floor = Floor {
    min: 16,
    measured: 27,
    measured_on: "2026-09-20",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "eea00637f + 이 커밋 — `scripts/` 아래 `.sh` 를 재귀로 세면 그 트리에서 26 이고 \
         이 커밋이 하나를 더해 27 이다. 앞선 24 는 **안 잰 값**이었다(`NEVER_COUNTED`). \
         같은 좌변을 `gates_pin_their_judge_absence` 도 잰다 — 두 선언이 같은 값을 든다.",
    ),
    why_this_gap: "게이트·러너·빌드 스크립트는 회차마다 하나씩 늘고 가끔 하나가 접힌다. \
                   여유 11 은 그 폭을 견디되, 순회가 `scripts/lib`·`scripts/bench` 만 보거나 \
                   아예 안 내려간 상태는 잡는다. 이 여유는 아무도 고른 적이 없다 — 하한을 \
                   정할 때는 8 이었고 좌변이 자란 만큼 벌어졌다",
};

/// 루트 `tests/` 한 겹의 순회 하한 — 증거 리터럴을 여기서 모은다.
const ROOT_TESTS_FLOOR: Floor = Floor {
    min: 21,
    // 좌변의 사실은 `tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS` 하나가 갖는다. 이 자리에 값을 직접
    // 적었을 때 그 값은 **내 base 에서 센 것**이라 병합된 트리와 어긋났다 — 이제 어느
    // 트리에서 잰 값인지는 `counted_on` 이 말한다.
    measured: tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS.measured,
    measured_on: tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS.measured_on,
    counted_on: tasty_doc_guards::floored_walk::populations::ROOT_TEST_TARGETS.counted_on,
    why_this_gap: "루트 통합 테스트 타깃은 대개 하나씩 늘지만 접힐 때는 한 \
                   번에 여럿 빠진다 — 도달 가능한 창에서 다시 실측(`fdca139c0`..`91ca7d37d`, \
                   558 커밋, 직선): 감소 3 회, 폭은 -1 이 둘 · **-16 이 하나**. ★ 여유 22(좌변 \
                   43, 2026-09-22) 는 그 최대 감소 하나를 넘는 폭이다(6 이 남는다) — 두 번 \
                   겹치면(32) 짖는다. 앞선 판은 `8bdbf1bdb` 직전 1215 \
                   커밋에서 감소 10 회 · 최대 -6 으로 재고 여유를 그 세 배로 잡았는데, 그 \
                   좌표는 지금 main 에서 도달 불가다. 이 시험은 거부문을 자기 말로 갈라 보므로, 타깃이 절반만 보이면 갈릴 \
                   것이 없어 초록으로 끝난다 — 하한이 그 부분 사망을 잡는 유일한 자리다",
};

/// `crates/` 아래 통합 테스트 타깃의 순회 하한.
const CRATE_TESTS_FLOOR: Floor = Floor {
    min: 80,
    measured: tasty_doc_guards::floored_walk::populations::CRATE_TEST_TARGETS.measured,
    measured_on: tasty_doc_guards::floored_walk::populations::CRATE_TEST_TARGETS.measured_on,
    counted_on: tasty_doc_guards::floored_walk::populations::CRATE_TEST_TARGETS.counted_on,
    why_this_gap: "같은 창(1215 커밋)에서 이 모수는 34 에서 101 까지 단조로 \
                   늘었고 감소가 한 번도 관측되지 않았다 — 여유를 진폭에서 뽑을 수 없다. 그래서 \
                   아직 안 일어난 사건의 크기에 건다: 이 가드가 사는 `tasty-doc-guards`(82 \
                   타깃)를 빼면 한 크레이트의 최대가 8 이다(2026-09-20 재측정 — 앞선 판은 \
                   75 와 7 이었다). ★ 여유 35 는 그 4.4 배인데 그 배수는 **아무도 고른 적이 \
                   없다** — 하한 80 을 정할 때의 여유는 21(사건 크기 7 의 세 배)이었고, 그 뒤 \
                   좌변이 파생이라 혼자 자라는 동안 이 문장만 21 에 남아 있었다. 폭을 다시 \
                   고르려면 하한을 움직여야 하고 그것은 판단이라 여기서 안 한다. 루트 쪽 \
                   여유(19)와 값이 비슷했던 것은 우연이고 지금은 갈렸다 — 저쪽은 관측된 \
                   진폭에서, 이쪽은 미관측 사건의 크기에서 나왔다",
};

// ── ★ `measured` 는 **낡아도 아무도 안 말한다** ─────────────────────────
//
// `Floor` 가 검사하는 것은 `min <= measured` 와 날짜 형식뿐이다. `measured` 가 지금
// 트리의 실제 수와 맞는지는 **어느 가드도 안 묻는다** — `min` 이 여유를 크게 잡아 놨으니
// 실제가 움직여도 rc 는 계속 0 이고, 낡는 것은 판정이 아니라 `measured`/`measured_on`
// 이라는 **문서값**이다. 하필 그 값이 자기 갱신 시점을 스스로 주장한다.
//
// 그리고 그 낡음은 **같은 회차 안에서** 난다. 여러 lane 이 병렬로 일하면 각자 자기 base
// 에서 세는데, 병합된 트리에는 다른 lane 이 만든 타깃이 더 있다. 실측 2026-09-08: 이
// 파일을 처음 낼 때 루트 38 · crates 97 로 적었고, 병합된 트리에서는 39 · 101 이었다
// (+1 은 이 파일 자신, +4 는 같은 회차의 다른 lane 이 만든 `crates/*/tests/` 타깃).
//
// 그래서 위 두 수는 **병합된 트리에서 다시 센 값**이다. 세는 법(체크아웃 없이):
//
//     git ls-tree -r --name-only <병합 지점> \
//       | python3 -c "import sys;p=[l.strip() for l in sys.stdin]; \
//           print(sum(1 for x in p if x.count('/')==1 and x.startswith('tests/') and x.endswith('.rs')))"
//
// 이 환경의 `grep` 은 ugrep 이라 `-E` 의 일부 교체를 조용히 빈 결과로 낸다 — 세는 데
// 쓰지 마라.
//
// **이것을 말하는 채널을 만드는 것은 다음 회차 후보다.** 여기서 짓지 않는 이유는 그것이
// 이 가드의 물음이 아니기 때문이다 — `Floor` 를 쓰는 모든 자리에 걸리는 물음이라, 이
// 파일 안에 세우면 나머지 자리들은 여전히 조용하다.

/// 좌변에서 빼는 스크립트 — **게이트가 아닌 것**. 이름과 **사유**를 함께 둔다.
///
/// 파일 단위로 빼는 이유: 그 스크립트의 거절은 판정이 아니라 **사용법 오류**라, 거기에
/// "시험이 그 문구를 단정하라" 는 실재하지 않는 결함에 대한 처방이 된다.
const NOT_A_GATE: &[(&str, &str)] = &[(
    "survey-guard-move.sh",
    "게이트가 아니라 손으로 부르는 조사 도구다. 거절 넷은 전부 인자를 잘못 준 사람에게 \
     쓰는 법을 알려주는 자리이고, CI·훅 어디서도 안 불린다 — 그 문구를 시험이 단정해야 \
     할 이유가 없다.",
)];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 자기 말을 아직 못 가진 거절 자리의 **상한**. 양방향이다.
///
/// 실측 2026-09-08: 게이트의 거절 자리 26 중 자기 말이 있는 것 6, 없는 것 20.
/// 2026-09-14: `check-plugin-version-bump.sh` 의 의존 폐포 거절(`cargo tree 가 실패했다`)이
/// 자기 말을 가져 19. 그 문구를 단정하는 시험은 `tests/plugin_version_bump_channel.rs` 의
/// `an_unreadable_dependency_closure_is_undecidable_and_says_why`.
const CAP: usize = 19;

/// 증거로 인정할 리터럴의 최소 길이(문자 수).
///
/// 짧은 조각은 우연히 겹친다. 여덟 자면 `tokei 미설치`·`rustfmt 가 없다` 같은 실제
/// 단정은 지나가고, `Cargo.toml` 같은 일반 토큰은 아래 조건들에서 걸러진다.
const MIN_EVIDENCE: usize = 8;

/// 이 파일 자신 — 증거 풀에서 뺀다.
///
/// 아래 실패문이 게이트 이름을 인용하므로, 안 빼면 이 파일이 자기 물음의 증거 풀에 든다.
///
/// **오늘 이 줄은 안 물린다 — 그 사실을 적어 둔다.** 실측 2026-09-08: 이 상수를 없는
/// 파일 이름으로 바꾸는 변이가 **rc=0 으로 살아남았다.** 지금 이 파일의 리터럴 중 어떤
/// 판정문의 구별 조각이 되는 것이 없기 때문이다. 즉 이것은 방어이지 지금 서 있는 축이
/// 아니고, 실패문에 판정문 조각을 인용하는 순간 서게 된다. 안 걸리는 술어를 걸리는
/// 것처럼 적지 않으려고 여기 남긴다.
const SELF: &str = "refusals_are_told_apart_by_their_own_words.rs";

/// 거절 자리 하나: `(스크립트 상대경로, 줄 번호, 판정문)`.
struct Refusal {
    script: String,
    line: usize,
    message: String,
}

fn walk(dir: &Path, floor: &Floor, keep: &dyn Fn(&Walked) -> bool) -> Vec<Walked> {
    walk_with_floor(dir, dir, floor, Descend::Everything, keep)
        .unwrap_or_else(|why| panic!("{why}"))
}

/// `scripts/` 에서 `die "…"` 호출 줄을 센다.
///
/// 주석 줄과 `die()` 정의 줄은 뺀다 — 앞은 사용례이고 뒤는 자리가 아니라 그 자리들이
/// 공유하는 함수다.
fn refusals() -> Vec<Refusal> {
    let dir = root().join("scripts");
    let mut out = Vec::new();
    for w in walk(&dir, &SCRIPT_FLOOR, &|w: &Walked| w.rel.ends_with(".sh")) {
        if NOT_A_GATE.iter().any(|(name, _)| w.rel.ends_with(name)) {
            continue;
        }
        let body = fs::read_to_string(&w.path).unwrap_or_default();
        for (i, line) in body.lines().enumerate() {
            if line.trim_start().starts_with('#') || line.contains("die()") {
                continue;
            }
            let Some(rest) = line.split_once("die \"") else {
                continue;
            };
            // 여는 따옴표 앞이 식별자 문자면 `die` 가 아니라 다른 낱말의 꼬리다.
            if rest
                .0
                .chars()
                .last()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let Some((msg, _)) = rest.1.split_once('"') else {
                continue;
            };
            out.push(Refusal {
                script: w.rel.clone(),
                line: i + 1,
                message: msg.to_string(),
            });
        }
    }
    out
}

/// 한 줄에서 러스트 문자열 리터럴을 뽑는다. `\"` 를 건너뛴다.
///
/// 줄 단위인 것이 중요하다 — 파일 전체에 정규식을 걸면 `\"` 를 담은 리터럴이 뒤따르는
/// 따옴표를 삼켜 여러 리터럴이 한 덩어리로 붙는다(실측으로 밟았다: 그 상태에서 이
/// 물음의 답이 물림 5 로 나왔고, 줄 단위로 고치니 12 였다).
fn literals_in(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut buf = String::new();
        let mut closed = false;
        while let Some(d) = chars.next() {
            if d == '\\' {
                if let Some(e) = chars.next() {
                    buf.push(e);
                }
                continue;
            }
            if d == '"' {
                closed = true;
                break;
            }
            buf.push(d);
        }
        if closed {
            out.push(buf);
        }
    }
    out
}

/// 시험 타깃마다 `(본문, 그 안의 리터럴)`.
///
/// 리터럴을 레포 전체에서 한 통으로 모으면 **다른 게이트의 시험이 증거로 선다.** 실측
/// 2026-09-08: `tests/frozen_sum_ratchet_gate.rs` 의 `"tokei 미설치"` 단정을 빗나가게
/// 바꾸는 변이가 **살아남았다** — 같은 리터럴이 `tests/file_sloc_gate_fails_loudly.rs`
/// 에도 있어 수가 안 움직였다. 그래서 자리마다 **그 게이트를 이름으로 부르는 시험**만
/// 증거 풀로 쓴다.
fn test_sources() -> Vec<(String, Vec<String>)> {
    let r = root();
    let root_tests = r.join("tests");
    let crate_tests = r.join("crates");

    let mut files = walk(&root_tests, &ROOT_TESTS_FLOOR, &|w: &Walked| {
        w.rel.ends_with(".rs") && !w.rel.contains('/')
    });
    files.extend(walk(&crate_tests, &CRATE_TESTS_FLOOR, &|w: &Walked| {
        let parts: Vec<&str> = w.rel.split('/').collect();
        parts.len() == 3 && parts[1] == "tests" && w.rel.ends_with(".rs")
    }));

    let mut out = Vec::new();
    for w in files {
        if w.rel.ends_with(SELF) {
            continue;
        }
        let body = fs::read_to_string(&w.path).unwrap_or_default();
        let mut lits: Vec<String> = body
            .lines()
            .flat_map(literals_in)
            .filter(|l| l.chars().count() >= MIN_EVIDENCE)
            .collect();
        lits.sort();
        lits.dedup();
        out.push((body, lits));
    }
    out
}

/// 그 게이트를 **이름으로 부르는** 시험들의 리터럴만 모은다.
fn pool_for(script: &str, sources: &[(String, Vec<String>)]) -> Vec<String> {
    let base = script.rsplit('/').next().unwrap_or(script);
    let mut out: Vec<String> = sources
        .iter()
        .filter(|(body, _)| body.contains(base))
        .flat_map(|(_, lits)| lits.iter().cloned())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// 그 자리의 **자기 말**을 단정하는 리터럴을 찾는다.
///
/// 셋을 모두 만족해야 증거다.
///   1. 그 판정문 안에 있다.
///   2. **다른 거절의 판정문에는 없다** — 있으면 어느 자리를 단정한 것인지 못 가른다.
///   3. 그 스크립트에서 **판정문들을 지운 나머지**에는 없다 — 있으면 그 게이트 어디에나
///      있는 일반 토큰이고, 그것을 담은 리터럴은 이 자리를 겨눈 단정이 아니다.
fn evidence<'a>(r: &Refusal, all: &[Refusal], lits: &'a [String], rest: &str) -> Option<&'a str> {
    lits.iter()
        .filter(|l| r.message.contains(l.as_str()))
        .filter(|l| {
            all.iter()
                .filter(|o| o.message.contains(l.as_str()))
                .count()
                == 1
        })
        .filter(|l| !rest.contains(l.as_str()))
        .max_by_key(|l| l.chars().count())
        .map(|l| l.as_str())
}

/// 그 스크립트에서 판정문들을 지운 나머지 본문.
fn body_without_messages(script: &str, all: &[Refusal]) -> String {
    let mut body = fs::read_to_string(root().join("scripts").join(script)).unwrap_or_default();
    for r in all.iter().filter(|r| r.script == script) {
        body = body.replace(&r.message, "");
    }
    body
}

/// 양성 대조(2026-09-08, 내 트리 · 스크립트를 읽는 루트 타깃 여덟, 기준선 75/0):
///
/// | 변이 | rc | 죽은 시험 |
/// |---|---|---|
/// | `MIN_EVIDENCE` 8 → 200 (증거 풀을 말린다) | 101 | 이것 + `refusals_without_words_of_their_own_stay_capped` |
/// | `die()` 판별 무력화 (선언과 호출을 안 가른다) | 101 | 같은 둘 |
///
/// **배타적이지 않다.** 좌변이 무너지면 상한 래칫이 양방향이라 미달 쪽으로 함께 짖기
/// 때문이다. 그래도 남기는 이유는 처방이다 — 래칫의 문장은 "상한을 같이 내려라" 이고,
/// 그것을 따르면 **좌변이 죽은 상태가 값으로 승인된다.** 이 시험의 문장만 "세는 줄이
/// 깨졌다" 를 가리킨다.
#[test]
fn the_left_side_is_counted_not_listed() {
    let all = refusals();
    assert!(
        !all.is_empty(),
        "scripts/ 에서 거절 자리를 하나도 못 셌다 — 세는 줄이 깨졌다. 그 상태에서 이 \
         가드는 아무것도 안 물은 채 초록이 된다"
    );
    let lits: Vec<String> = test_sources().into_iter().flat_map(|(_, l)| l).collect();
    assert!(
        lits.len() > 500,
        "시험 리터럴을 {}개밖에 못 모았다 — 리터럴 추출이 깨지면 모든 자리가 '자기 말 \
         없음' 으로 나와 이 가드가 재려던 것과 다른 것을 잰다",
        lits.len()
    );
}

#[test]
fn refusals_without_words_of_their_own_stay_capped() {
    let all = refusals();
    let sources = test_sources();

    let mut scripts: Vec<String> = all.iter().map(|r| r.script.clone()).collect();
    scripts.sort();
    scripts.dedup();
    let rests: Vec<(String, String)> = scripts
        .iter()
        .map(|s| (s.clone(), body_without_messages(s, &all)))
        .collect();

    let mut naked = Vec::new();
    let mut spoken = 0usize;
    for r in &all {
        let rest = rests
            .iter()
            .find(|(s, _)| *s == r.script)
            .map(|(_, b)| b.as_str())
            .unwrap_or("");
        let pool = pool_for(&r.script, &sources);
        if evidence(r, &all, &pool, rest).is_some() {
            spoken += 1;
        } else {
            naked.push(format!("{}:{}  {}", r.script, r.line, r.message));
        }
    }

    assert!(
        spoken > 0,
        "자기 말이 있는 자리가 하나도 없다 — 술어가 죽었다. 좌변 {} 자리",
        all.len()
    );
    assert!(
        naked.len() <= CAP,
        "자기 말이 없는 거절이 상한({CAP})을 넘었다: {} 자리.\n\
         **상한을 올려서 통과시키지 마라.** 이 자리들은 종료 코드로 서로 구별되지 않아, \
         하나를 완화해도 다른 자리가 같은 rc 를 대신 내준다 — 초록이 뜨는 채로 안 걸린다.\n\
         그 자리의 문구를 단정하는 시험을 하나 지어라(형제 예: \
         tests/frozen_sum_ratchet_gate.rs 의 a_missing_tokei_is_undecidable).\n{}",
        naked.len(),
        naked.join("\n")
    );
    assert!(
        naked.len() >= CAP,
        "자기 말이 없는 거절이 상한({CAP})보다 적다({}). 좋은 방향이다 — 상한을 그 값으로 \
         **같이 내려라.** 남는 여유는 곧 아무도 안 보는 구간이다.\n    const CAP: usize = {};",
        naked.len(),
        naked.len()
    );
}

/// 양성 대조(2026-09-08, 내 트리 · 같은 여덟 타깃):
///
/// | 변이 | rc | 죽은 시험 |
/// |---|---|---|
/// | `NOT_A_GATE` 의 이름을 없는 스크립트로 | 101 | 이것 + `refusals_without_words_of_their_own_stay_capped` |
/// | `NOT_A_GATE` 의 사유를 40 자 미만으로 | 101 | **이것만** |
///
/// 두 단정이 서로 다른 것을 잡는다. 이름 쪽은 좌변을 움직여 래칫과 함께 죽고(그 자리가
/// 좌변으로 돌아온다), 사유 쪽은 거르기를 안 건드려 **이 시험만** 죽는다. 즉 사유 길이
/// 단정은 이 파일에서 유일하게 그 축을 보는 자리다.
#[test]
fn the_non_gate_list_names_real_scripts_with_reasons() {
    let dir = root().join("scripts");
    let scripts = walk(&dir, &SCRIPT_FLOOR, &|w: &Walked| w.rel.ends_with(".sh"));
    for (name, why) in NOT_A_GATE {
        assert!(
            scripts.iter().any(|w| w.rel.ends_with(name)),
            "NOT_A_GATE 의 '{name}' 이 scripts/ 에 없다 — 지워라. 안 지우면 빼둔 것의 수가 \
             실제보다 커 보인다"
        );
        assert!(
            why.chars().count() >= 40,
            "'{name}' 의 사유가 너무 짧다 — 왜 그 거절이 판정이 아닌지를 적어야 한다"
        );
    }
}
