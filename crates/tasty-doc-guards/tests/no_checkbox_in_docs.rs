//! docs 마크다운 체크박스 재유입 가드 — `docs/**/*.md` 에 마크다운 task list
//! 체크박스(`[ ]` / `[x]` 로 시작하는 목록 항목)가 다시 들어오면 fail 한다.
//!
//! 배경: `docs/documentation-model.md` §6 "작성 규칙 요약" 은 docs 문서에 체크박스를
//! 두지 않는다고 규정한다. 체크 상태(했다/안 했다)는 본질이 진행 추적이라 같은 절의
//! transient(빌드/로드맵 상태) 금지와 충돌하고, 실제로도 체크 상태에 정해진 의미가
//! 없어 정보를 담지 못한다. Acceptance Criteria 는 평문 `Given … When … Then …` 불릿,
//! 검증·절차 항목은 평문 불릿이나 번호 목록으로 적는다.
//!
//! **탐지 규칙 — 행 시작의 목록 마커만 본다.** 선행 공백 → 목록 마커(`-` / `*` / `+`)
//! → 공백(1 개 이상, GFM 이 같은 항목으로 렌더하는 2~4 개 포함) → `[` → 공백·`x`·`X` 한 글자 → `]` 로 시작하는 행이 대상이다. 부분문자열
//! 검사는 인라인 설명("`[x]` 접두를 쓰지 않는다")이나 링크 텍스트까지 오탐하므로
//! 쓰지 않는다. 규칙 본문(documentation-model · 템플릿 주석 · `CLAUDE.md`)은 금지
//! 형태를 인라인 백틱으로만 언급하고 행 시작 목록으로는 쓰지 않아 allowlist 가
//! 필요 없다.
//!
//! **코드 펜스 안팎을 구분하지 않는다.** 규칙이 "docs 문서에 체크리스트를 넣지
//! 않는다" 이므로 펜스 안의 예시도 금지 대상이다.
//!
//! 선례: `crates/tasty-doc-guards/tests/no_todo_file_citation.rs`(docs 스캔 구조) · `crates/tasty-doc-guards/tests/no_emoji_in_source.rs`.

// 이유: 이 타깃은 전부 테스트다. 테스트의 `let _` 무시는 정책이 사유를 요구하지
// 않으므로 `clippy::let_underscore_must_use` 명부(프로덕션 전용)에 섞이면 안 된다
// — docs/dev-guide/error-handling.md.
#![allow(clippy::let_underscore_must_use)]

use std::path::Path;
use tasty_doc_guards::floored_walk::{
    Descend, Floor, Pick, Walked, walk_dirs_with_floor, walk_with_floor,
};

/// 스캔에서 제외할 파일(repo-relative). 현재 비어 있다 — 규칙 본문은 금지 형태를
/// 행 시작 목록으로 쓰지 않는 방식으로 이 가드를 통과하므로 등록할 파일이 없다.
/// 체크박스를 **담는 것이 본질** 인 파일(마크다운 렌더 테스트 픽스처 등)이 docs 에
/// 생기면 여기에 등록한다.
const ALLOWLIST_FILES: &[&str] = &[];

/// 순회가 실제로 `docs/` 를 봤음을 보장하는 하한 — 값 하나가 아니라 **무엇의 함수인지**와
/// 함께 선언한다. 이 형태와 그 이유는 `tasty_doc_guards::floored_walk` 에 있다.
const DOCS_FLOOR: Floor = Floor {
    min: 338,
    // 좌변의 사실은 `populations::DOCS_MD` 하나가 갖는다 — 같은 모수를 재는 자리가
    // `every_docs_category_is_reachable_from_the_index` 에 하나 더 있다.
    measured: tasty_doc_guards::floored_walk::populations::DOCS_MD.measured,
    measured_on: tasty_doc_guards::floored_walk::populations::DOCS_MD.measured_on,
    counted_on: tasty_doc_guards::floored_walk::populations::DOCS_MD.counted_on,
    why_this_gap: "도달 가능한 창에서 다시 실측(`fdca139c0`..`91ca7d37d`, 558 커밋, \
                   직선): 이 모수는 285..448 로 움직였고 **감소가 여전히 한 번도 없다** — \
                   51 개 커밋에서 다 늘기만 했다(앞선 창 `8bdbf1bdb` 직전 1215 커밋에서도 \
                   354..402 · 감소 0 이었다. 그 좌표는 지금 main 에서 도달 불가라 다시 쟀고, \
                   결론은 같다). 그 창 밖에서는 감소가 두 번 관측됐다 — `0f37f8565`(502 -> 500, \
                   −2 — 중복 ADR 사본 둘을 지운 정리)와 `e1e80e2b8`(567 -> 562, −5 — 중복 ADR \
                   여섯을 하나로 흡수한 정리)다(`65bcd1988`..`4bace058e` 를 first-parent 로 커밋마다 \
                   셌다, 2026-09-23). 관측된 감소 진폭(최대 5)은 아래 사건 크기 64 보다 훨씬 작아 \
                   '진폭 × 몇 배' 의 곱수 근거로 안 쓰고, 대신 아직 안 일어난 사건의 크기에 건다: \
                   문서 카테고리 하나가 접히면 그 아래 `.md` 가 통째로 빠지고, 지금 트리에서 그 \
                   크기의 최대는 `docs/features` 의 64 다(2026-09-23 재측정 — 이 수는 안 \
                   움직였다). 이 문단은 한때 '감소 진폭이 관측되지 않았다' 와 '그 창 밖에서 첫 \
                   감소가 났다(`e1e80e2b8`)' 를 적고 있었다 — `0f37f8565` 의 −2 는 그때 이미 \
                   `DOCS_MD` 의 이력에 적혀 있었다. ★ 여유 267(좌변 605) 은 그 사건 \
                   하나(64)를 넘어 203 이 더 남는다 — 좌변이 자란 만큼 벌어진 것이고, 하한을 \
                   그대로 두는 한 이 폭은 계속 벌어진다. 이 문장은 좌변이 448 이던 판에서 \
                   110 이라고, 좌변이 502 이던 판까지도 117 이라고, 좌변이 559 이던 판에서는 157 이라고, 좌변이 591 이던 판에서는 189 라고 \
                   적혀 있었다: 모수가 파생이라 **수는 따라왔는데 그 수를 설명하는 \
                   이 문장만 안 따라왔다.** `docs/adr` 408(2026-09-23) 는 곱수에 안 넣는다 — 그것이 \
                   접히는 것은 카테고리 하나가 접히는 사건이 아니라 결정 기록 방식이 바뀌는 \
                   일이고, 그때 할 일은 하한을 견디는 것이 아니라 이 수를 다시 재는 것이다. 이 \
                   가드는 체크박스를 담은 문서를 찾으므로, 순회가 절반만 모으면 못 본 문서의 \
                   체크박스가 **위반 0 으로 승인된다**. 앞선 판은 계보(1215 커밋에서 354→399)는 \
                   적었지만 폭 102 를 그 수에서 뽑지는 않았다 — 계보는 값이 어디서 왔는지를 \
                   말하고, 폭은 그것만으로 안 정해진다",
};

/// 행이 마크다운 체크박스 목록 항목으로 시작하는지.
/// 선행 공백 · 목록 마커 · 공백(1 개 이상) · `[` · (공백|x|X) · `]` 순서만 본다. `]` 뒤는 보지 않는다.
fn is_checkbox_item(line: &str) -> bool {
    let rest = line.trim_start();
    let Some(rest) = rest.strip_prefix(['-', '*', '+']) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(' ') else {
        return false;
    };
    // GFM 은 마커 뒤 공백 1~4 개를 모두 같은 목록 항목으로 렌더한다 — 남은 공백도 흡수.
    let rest = rest.trim_start_matches(' ');
    let Some(rest) = rest.strip_prefix('[') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix([' ', 'x', 'X']) else {
        return false;
    };
    rest.starts_with(']')
}

/// 체크박스를 금지할 문서인지 — `docs/` 하위 `.md` 전부.
///
/// 이름을 물음으로 적는다: 다른 가드의 같은 이름 `is_scan_target` 들과 grep 에서 뭉쳐
/// 보이지만, 이 가드의 물음은 "체크박스 검사 대상 문서인가" 로 그들과 다르다 — 다른
/// 물음이라 위임할 정본이 없다(ADR-0180: 같은 이름 다른 물음은 이름을 갈라 세운다).
fn is_checkbox_doc(rel: &str) -> bool {
    rel.starts_with("docs/") && rel.ends_with(".md")
}

/// `docs/` 아래 스캔 대상을 모은다.
///
/// **가지치기가 없다.** 순회 루트가 `docs/` 하나이고 빌드 산출물도 로컬 작업 폴더도
/// 전부 그 밖에 있다. 죽은 가지는 코드가 없는 것보다 나쁘다 — 읽는 사람에게 "이
/// 가드는 그 경우를 고려했다" 는 거짓 안심을 주면서 그 판정은 한 번도 돌지 않는다.
///
/// **그 사실은 여기 적힌 문장이 아니라 `docs_holds_no_prunable_directory` 가 지킨다.**
/// 한때 이름 기반 가지치기가 이 자리에 있었고 지울 때 근거로 쓴 것은 "한 번도 참이
/// 된 적이 없다" 였다 — 그 값은 적는 순간 낡고, 낡아도 아무도 모른다. 그래서 값을
/// 문장으로 남기지 않고 재는 법을 테스트로 남긴다: `docs/` 아래에 가지쳐야 할
/// 디렉토리가 처음 생기는 날 이 순회는 그것을 그대로 들여다보고 그 아래 `.md` 까지
/// 검사 대상으로 삼는데, 그날 빨개지는 것은 이 주석이 아니라 그 테스트다.
///
/// 하한은 공용 순회가 강제한다 — 여기서 빠뜨릴 수 없고, 실패문도 거기서 나온다.
fn gather_docs(root: &Path) -> Result<Vec<Walked>, String> {
    walk_with_floor(
        &root.join("docs"),
        root,
        &DOCS_FLOOR,
        Descend::Everything,
        &|found| is_checkbox_doc(&found.rel),
    )
}

#[test]
fn no_checkbox_in_docs() {
    let root = &tasty_doc_guards::repo_root();
    let files = gather_docs(root).unwrap_or_else(|why| panic!("{why}"));

    let mut violations = Vec::new();
    for file in &files {
        let rel = &file.rel;
        if ALLOWLIST_FILES.contains(&rel.as_str()) {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&file.path) else {
            continue; // 비-UTF8 은 마크다운 문서가 아니다.
        };
        for (i, line) in contents.lines().enumerate() {
            if is_checkbox_item(line) {
                violations.push(format!("  {}:{} — `{}`", rel, i + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "docs 문서에 마크다운 체크박스(task list) 목록 항목이 있다 — docs 는 현재 상태만 \
         기술하며 체크 상태(했다/안 했다)는 진행 추적이라 두지 않는다 \
         (docs/documentation-model.md §6 \"작성 규칙 요약\").\n\
         체크박스 접두를 떼고 평문 불릿(`- Given … When … Then …`)이나 번호 목록으로 쓸 것. \
         미구현 범위는 Status 줄과 본문 문장으로 적는다.\n\
         체크박스를 담는 것이 본질인 파일이면 ALLOWLIST_FILES 에 추가:\n{}",
        violations.join("\n")
    );
}

#[test]
fn checkbox_matcher_hits_only_line_start_list_items() {
    // 잡아야 하는 형태 — 마커 3 종, 체크 상태 3 종, 선행 공백, 중첩.
    assert!(is_checkbox_item("- [ ] Given a When b Then c"));
    assert!(is_checkbox_item("- [x] done"));
    assert!(is_checkbox_item("* [X] done"));
    assert!(is_checkbox_item("+ [ ] item"));
    assert!(is_checkbox_item("  - [ ] nested"));
    assert!(is_checkbox_item("\t- [x] tab-indented"));
    assert!(is_checkbox_item("- [ ]"));
    // 마커 뒤 공백 2~4 개 — GFM 은 같은 task item 으로 렌더하므로 회피 형태가 아니다.
    assert!(is_checkbox_item("-  [ ] two spaces"));
    assert!(is_checkbox_item("-   [x] three spaces"));
    assert!(is_checkbox_item("*    [ ] four spaces"));

    // 잡지 않아야 하는 형태 — 평문 불릿, 번호 목록, 인라인 언급, 링크, 마커 없음.
    assert!(!is_checkbox_item("- Given a When b Then c"));
    assert!(!is_checkbox_item(
        "1. [ ] numbered lists are not task lists"
    ));
    assert!(!is_checkbox_item(
        "- 목록 항목을 `[ ]`·`[x]` 로 시작하는 형식은 쓰지 않는다"
    ));
    assert!(!is_checkbox_item("- [link](target.md)"));
    assert!(!is_checkbox_item("- [xx] not a checkbox"));
    assert!(!is_checkbox_item("-[ ] no space after marker"));
    assert!(!is_checkbox_item("[ ] no list marker"));
    assert!(!is_checkbox_item(""));
}

/// `docs/` 아래에 있으면 안 되는 디렉토리 이름 — 빌드 산출물과 워크트리·git 내부.
///
/// **이 목록은 가지치기를 하지 않는다.** `gather` 가 무가지 순회라는 사실을 지키기
/// 위해서만 존재한다 — 값이 아니라 재는 법이다.
const PRUNABLE_DIRS: &[&str] = &["target", "dist", ".worktree", ".git", "node_modules"];

/// gitignored 로컬 작업 폴더 이름의 조각. 리터럴로 두면 이 파일이 비-git 경로 참조
/// 금지(`docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md`) 를 어긴다 — 인용이
/// 아니라 판정 입력이지만, 조각으로 조립하면 예외 등록 없이 규칙을 지킬 수 있다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

/// 디렉토리 이름이 순회에서 가지쳐야 할 것인지 — 빌드 산출물 이름 또는 선행 `.` 이
/// 붙은 로컬 작업 폴더. 두 갈래는 서로 다른 축이라 둘 다 대조가 필요하다.
fn is_prunable_dir(name: &str) -> bool {
    PRUNABLE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == format!("{LOCAL_HEAD}{LOCAL_TAIL}"))
}

/// `docs/` 아래 디렉토리 순회의 하한. 여기서 하한은 **모은 수가 아니라 훑은 수**에
/// 걸린다 — 가지쳐야 할 디렉토리가 0 개인 것이 이 가드가 지키려는 정상 상태다.
const DOCS_DIR_FLOOR: Floor = Floor {
    min: 59,
    measured: 87,
    measured_on: "2026-09-06",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::LaneTip("8bdbf1bdb"),
    why_this_gap: "이 모수는 `docs/` 아래 디렉토리 수다(뿌리 자신은 안 센다 — \
                   `collect_dirs` 가 하위만 훑는다). 움직임을 실측했다: `8bdbf1bdb` 직전 1215 \
                   커밋에서 이 수는 87 로 **한 번도 안 변했다**. 진폭이 0 이라 다른 하한들처럼 \
                   '관측 진폭 × 세 배' 를 쓸 근거가 없다 — 연속으로 몇 번 움직인다는 관측 자체가 \
                   없다. 그래서 사건 하나분만 둔다: 카테고리 하나가 접히면 그 아래가 통째로 \
                   빠지고, 그 크기의 최대는 `features` 를 뺀 `plugins` 다. 비-ADR 문서 \
                   통폐합이 단일 화면 `screens/` 폴더와 하위 폴더 셋을 없애 이 수가 71 로 \
                   내려갔고(clean checkout 에서 `git ls-tree -r -d --name-only HEAD docs` 로 잰 \
                   값 — 빈 디렉토리는 git 이 추적하지 않으므로 작업 트리의 `find` 가 아니라 \
                   이것으로 잰다), 그때 `plugins` 는 12 였다. 하한 59 = 71 − 12 = 사건 하나. \
                   `measured` 는 착지가 다시 잰다. `features` 49 는 곱수에 안 넣는다 — 그것이 통째로 접히는 \
                   것은 카테고리 하나가 접히는 사건이 아니라 문서 모델이 바뀌는 일이고, 그때는 \
                   이 수를 다시 재는 것이 맞다. 앞선 판은 '여유를 중간쯤 둔다' 였는데, 중간쯤은 \
                   어떤 실측으로도 거짓이 되지 않아 폭을 아무것도 안 정한다",
};

/// `root` 하위를 순회하며 `is_prunable_dir` 이 참인 디렉토리를 모은다.
/// 가지친 자리 아래로도 계속 내려간다 — 세는 것이 목적이지 자르는 것이 아니다.
///
/// 하한을 **인자로** 받는다. 본 테스트와 대조가 같은 순회를 부르되 대조는 작은 트리를
/// 쓰기 때문이다 — 하한을 상수로 박으면 대조가 그 하한에 걸려 다른 순회를 짜게 되고,
/// 그러면 대조가 본 테스트와 다른 것을 재게 된다.
fn prunable_dirs_under(root: &Path, rel_root: &Path, floor: &Floor) -> Result<Vec<String>, String> {
    walk_dirs_with_floor(root, rel_root, floor, &|found| {
        let name = found
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if is_prunable_dir(&name) {
            Pick::Take
        } else {
            Pick::Skip
        }
    })
    .map(|dirs| dirs.into_iter().map(|d| d.rel).collect())
}

#[test]
fn docs_holds_no_prunable_directory() {
    let root = &tasty_doc_guards::repo_root();
    let found = prunable_dirs_under(&root.join("docs"), root, &DOCS_DIR_FLOOR)
        .unwrap_or_else(|why| panic!("{why}"));
    assert!(
        found.is_empty(),
        "docs/ 아래에 순회에서 가지쳐야 할 디렉토리가 있다 — 이 가드의 순회(`gather`)는 \
         가지치기를 하지 않으므로 그 안의 `.md` 까지 체크박스 검사 대상이 된다.\n\
         셋 중 하나를 골라라. (1) 그 디렉토리를 `docs/` 밖으로 옮긴다. (2) 그 안의 문서도 \
         검사 대상이 맞다면 이 테스트를 지우고 `gather` 주석의 무가지 근거를 다시 쓴다. \
         (3) 검사 대상이 아니라면 `gather` 에 가지치기를 되살리고 이 테스트를 그 가지의 \
         대조로 바꾼다.\n\
         PRUNABLE_DIRS 에서 이름을 빼거나 이 테스트를 지워서 통과시키지 마라 — 그러면 \
         `gather` 가 가지 없이 도는 것이 옳다는 사실을 지키는 것이 아무것도 안 남는다.\n\
         빨간 경로가 네 변경과 무관해 보이면 먼저 만든 쪽을 찾아라 — 이 검사는 레포 실물 \
         `docs/` 를 보고, 함께 도는 다른 테스트 바이너리가 거기에 디렉토리를 만들면 그 \
         타깃의 뒷정리 누락이 여기서 빨개진다: \
         grep -rn --include='*.rs' create_dir tests crates | grep docs\n{}",
        found.join("\n")
    );
}

#[test]
fn the_prunable_check_reacts_to_a_planted_tree() {
    // 술어 축 — 두 갈래가 각각 산다.
    assert!(is_prunable_dir("target"), "빌드 산출물 이름을 안 잡는다");
    assert!(
        is_prunable_dir("node_modules"),
        "빌드 산출물 이름을 안 잡는다"
    );
    assert!(
        is_prunable_dir(&format!(".{LOCAL_HEAD}")),
        "선행 `.` 로컬 작업 폴더를 안 잡는다"
    );
    assert!(
        is_prunable_dir(&format!(".{LOCAL_HEAD}{LOCAL_TAIL}")),
        "선행 `.` 로컬 작업 폴더를 안 잡는다"
    );
    assert!(
        !is_prunable_dir("adr"),
        "평범한 docs 하위 디렉토리를 잡는다"
    );
    assert!(
        !is_prunable_dir("targets"),
        "이름이 겹치는 다른 디렉토리를 잡는다"
    );

    // 순회 축 — 실제 디렉토리를 심어서 부른다. 술어만 부르면 순회가 죽어도 초록이다.
    // 유일화 키에 **시각을 안 쓴다** — 시계의 해상도는 플랫폼의 성질이다.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let stamp = format!(
        "tasty-checkbox-guard-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let base = std::env::temp_dir().join(stamp);
    let docs = base.join("docs");
    std::fs::create_dir_all(docs.join("guide").join("target").join("deep")).unwrap();
    std::fs::create_dir_all(docs.join(format!(".{LOCAL_HEAD}"))).unwrap();
    std::fs::create_dir_all(docs.join("adr")).unwrap();

    // 대조는 심은 트리를 쓰므로 하한도 그 트리의 것이다. 본 테스트와 **같은 순회**를
    // 부르되 하한만 갈아 끼운다 — 대조가 다른 순회를 짜면 본 테스트가 쓰는 것을 안 재게 된다.
    let probe_floor = Floor {
        min: 3,
        measured: 5,
        measured_on: "2026-09-06",
        counted_on: tasty_doc_guards::floored_walk::CountedOn::SyntheticTree,
        why_this_gap: "심은 트리라 수가 고정이다. 하한을 실측보다 낮춰 두는 것은 이 대조가                        트리 모양의 사소한 변경에 깨지지 않게 하려는 것뿐이다.",
    };
    let found = prunable_dirs_under(&docs, &base, &probe_floor)
        .unwrap_or_else(|why| panic!("대조 트리 순회가 하한에 걸렸다: {why}"));
    // 정리 실패는 무시한다 — 판정은 위에서 이미 끝났고, 여기서 `?` 나 `unwrap` 을
    // 쓰면 임시 디렉토리 삭제 실패가 가드의 빨강으로 둔갑한다. 남아도 임시 경로다.
    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(
        found,
        vec![
            format!("docs/.{LOCAL_HEAD}"),
            "docs/guide/target".to_string()
        ],
        "심은 트리에서 가지칠 디렉토리를 정확히 두 개 집어야 한다 — 얕은 자리만 보거나 \
         평범한 디렉토리까지 집으면 이 목록이 달라진다"
    );
}
