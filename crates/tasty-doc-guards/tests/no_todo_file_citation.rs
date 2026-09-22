//! 휘발성 로컬 문서 인용 재유입 가드 — 커밋되는 파일이 git 에 올라가지 않는
//! 로컬 작업 문서를 인용하면 fail 한다.
//!
//! 배경: `CLAUDE.md` "소스 주석의 TODO 파일 및 디자인 changelog 인용 금지" 와
//! `docs/adr/template.md` "비-git 경로 참조 금지" 가 이미 규정한 것을 실제로
//! 강제한다. 로컬 작업 폴더는 `.gitignore` 대상이라 커밋되지 않고 완료된 항목은
//! 관례상 파일 자체가 삭제되므로, 그 번호·경로는 **로컬 세션에서만 유효한
//! 휘발성 식별자**다. 저장소를 새로 clone 한 사람에게 그 좌표는 존재한 적이 없다.
//!
//! 실제로 번호 재사용이 일어나 인용이 *무관한 문서* 로 해석되는 사례까지 나왔다 —
//! 죽은 참조를 넘어 오도하는 참조가 된다.
//!
//! **대체 수단(위반 시 이 중 하나를 쓴다)**:
//! 1. 이유가 자명하면 — 번호/경로 대신 이유를 주석에 직접 서술
//! 2. 설계 결정이 크면 — `docs/adr/` 에 ADR 을 쓰고 그 경로를 인용
//! 3. 기능 동작 설명이면 — `docs/`(dev-guide / features / plugins) 문서를 참조
//!
//! **탐지 패턴 7 종** (하나만 잡는 정규식으로는 절반도 못 거른다):
//! - P1 번호 인용 — `todo`(대소문자 무시) + 구분자 런(공백·탭·`/`, 0 개 이상) +
//!   선택적 하이픈 + 숫자.
//!   **어순 양방향**: 한국어 문장에서는 번호가 앞에 온다(`<숫자>번 TODO`). 뒤 어순만
//!   보던 시절 그 형태가 소스에 두 건 살아 있었다 — 같은 죽은 좌표인데 어순 하나로
//!   가드를 통과했다. **괄호 묶음도 잡는다**(`TODO(<숫자>)` · `TODO[<숫자>]` ·
//!   `TODO(<숫자>번)`). 벌거벗은 숫자만 보던 시절 그 형태가 문서에 두 건 살아
//!   있었다 — 규칙 본문이 예시로 드는 붙여쓴 형태와 같은 죽은 좌표인데 괄호
//!   하나로 가드를 통과했다.
//! - P2 conductor 번호 인용 — `todo-conductor`(대소문자 무시) + 구분자 런 + 숫자
//! - P3 경로 인용 — 로컬 작업 폴더 + `todo` / `todo-conductor` / `plans` / `conductor`
//! - P4 디자인 changelog slug — `YYYY-MM-DD-<slug>`. 원격 Claude Design 프로젝트
//!   내부에만 존재해 로컬 파일시스템에 흔적조차 없으므로 더 휘발적이다.
//! - P5 앵커 슬러그 변형 — 마크다운 앵커(`#...`) 안에 굳은 번호(`-todo-<숫자>`).
//!   제목에 번호를 달면 그 번호가 앵커의 일부가 되어 링크·주석으로 퍼지고, 나중에
//!   제목에서 번호를 떼는 순간 그 참조가 **전부 깨진다** (실제로 한 번 발생했다 —
//!   제목 하나를 고치자 링크 1곳과 주석 15줄이 죽은 좌표가 됐다). P1 이 대문자만
//!   보던 시절에는 표기 자체가 이것을 별도 패턴으로 만들었다. 지금은 겹치는 부분이
//!   있지만 여전히 독립적이다 — P1 의 구분자는 하이픈 **한 개**까지라 하이픈 런
//!   (`-todo--<숫자>`)을 놓치고, 무엇보다 **처방이 다르다**: P1 은 번호를 지우라고
//!   하지만 앵커는 제목의 번호를 떼고 그 제목을 가리키던 참조까지 함께 고쳐야 한다.
//! - P6 로컬 폴더 언급 — 하위 경로가 무엇이든, 아예 없든 잡는다. 폴더 이름 단독
//!   언급도 금지 대상이라는
//!   [ADR-0105](../docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md) 의
//!   결정을 강제한다. P3 가 네 개 하위 디렉토리로 좁혀 놓았던 것을 넓힌 형태다.
//! - P7 산문 언급 — 번호가 붙지 않은 `TODO`. P1 은 **숫자가 붙어야만** 잡는데, 실제로
//!   새는 형태는 번호 없이 티켓 자체를 가리키는 산문이다(`이 TODO 는 순수 구조
//!   이관이라` · `TODO 문서 초기 서술과 달리` · `a separate TODO if needed`). 번호가
//!   없다고 덜 죽은 참조가 되는 것이 아니다 — 갓 클론한 사람에게 그 문서는 존재한
//!   적이 없다. **할 일 표시는 통과시킨다**(판정선은 [`todo_marker_end`]), 그리고
//!   **마크다운은 범위 밖이다**(근거는 [`out_of_scope`]).
//!
//! **매칭은 구분자 개수·대소문자로 회피되지 않아야 한다.** 구분자를 한 개만 소비하는
//! 매처는 공백 두 개만 넣어도 통과하고, 원문 그대로 비교하는 매처는 대문자 표기로
//! 통과한다. 그래서 **P1~P6 은 전부** 구분자를 런으로 소비하고 소문자로 비교한다.
//! **예외는 대소문자 축이 아니라 구분자 축에만 있다** — P1 은 공백 런, `/`, **짝 맞는
//! 괄호 묶음**까지만 넓힌다. 임의 문장부호
//! (`:` · `#` · `.`)를 구분자로 허용하면 "TODO. 40" 같은 평범한 문장이 걸린다.
//! 괄호와 `/` 는 그 논거의 반례가 아니다: 괄호는 여는 괄호 뒤 숫자에 **닫는 괄호가
//! 짝으로 따라올 것**을 함께 요구하므로 문장이 아니라 번호를 감싼 묶음만 걸리고
//! ([`bracketed_number_end`]), `/` 는 문장 부호가 아니라 **경로 구분자**라 낱말과
//! 숫자 사이에서 문장을 만들지 않는다. 티켓 인용은 공백·하이픈·`/`·괄호로 쓰이지
//! 벌거벗은 문장부호로 쓰이지 않는다.
//!
//! **스캔 대상 정의 — denylist 전수 순회.** ADR-0105 의 규칙 범위가 "git 이 추적하는
//! 모든 파일" 이므로, 확장자·디렉토리 화이트리스트로 "볼 파일" 을 열거하지 않는다.
//! 순회가 닿는 모든 파일을 대상으로 삼고 바이너리 확장자만 뺀다. 화이트리스트 방식은
//! 스크립트·CI 설정·루트 문서·`site/` 를 통째로 놓치는 사각지대를 만들었고, 항목을
//! 추가해도 다음 사각지대가 또 생긴다. `git ls-files` 로 추적 집합을 직접 묻는 방법도
//! 있으나, 테스트가 git 바이너리와 저장소 메타데이터의 존재에 의존하게 되어 tarball
//! 빌드에서 깨지고
//! [ADR-0096](../docs/adr/0096-unit-tests-isolated-from-user-environment.md)
//! 의 "테스트는 환경을 읽지 않는다" 와도 어긋난다. gitignored 산출물은 `PRUNE_DIRS`
//! 가지치기로 덮이고, 남는 것(서명 파일 등)은 애초에 인용을 담지 않는다.
//!
//! **오탐 회피 — 홈 경로는 그 자리 직전 문맥으로 가른다.** 로컬 지침 폴더는 사용자
//! 홈에도 같은 이름이 있고, 홈 쪽은 ADR-0105 가 범위 밖으로 확정한 항목이다. 판정을
//! *줄 전체* 에서 홈 표기를 찾는 식으로 하면, 정당한 홈 경로가 한 번 나오는 줄에
//! 섞인 진짜 레포 로컬 참조까지 통과한다 — 그래서 **occurrence 직전** 만 본다
//! ([`home_context_before`]). 로컬 작업 폴더 쪽은 홈에 존재할 수 없어 예외가 없다.
//! reverse-DNS plugin id 는 이름 앞 글자가 식별자 문자라 경로 시작이 아니므로 애초에
//! 걸리지 않는다.
//!
//! 이 판정은 **휴리스틱**이다 — 직전 창에서 홈 표기를 낱말로 찾는 방식이라,
//! `let home = ...; home.join(".claude")` 처럼 변수 이름이 `home` 인 관용구에 기대고
//! 있다. 같은 뜻의 다른 이름(`user_root` · `profile_dir` 등)으로 쓰면 정당한 홈 경로가
//! 위반으로 잡힌다. 그때는 [`HOME_NEARBY`] 에 그 이름을 추가하거나, 소스를 `~` 표기로
//! 바꿔 접두 판정(①)에 걸리게 한다. 창을 넓히거나 부분문자열 매칭으로 되돌리는 것은
//! 답이 아니다 — 그러면 이 절이 막으려는 "줄 전체 면제" 로 되돌아간다.
//!
//! 선례: `crates/tasty-doc-guards/tests/no_emoji_in_source.rs`(구조 템플릿) · `crates/tasty-doc-guards/tests/design_token_adherence.rs`.

// 이유: 이 타깃은 전부 테스트다. 테스트의 `let _ =` 는 정책이 사유를 요구하지
// 않으므로 `clippy::let_underscore_must_use` 명부(프로덕션 전용)에 섞이면 안 된다
// — docs/dev-guide/error-handling.md.
#![allow(clippy::let_underscore_must_use)]
use std::path::{Path, PathBuf};

/// 예외 목록 — (repo-relative 경로, 그 파일에서만 허용하는 패턴 id).
///
/// **파일 통째가 아니라 패턴 단위**로 면제한다. 파일 전체를 빼면 그 파일이 *다른*
/// 형태의 위반을 새로 들여도 영영 잡히지 않는다 — 규칙 본문을 담은 파일일수록
/// 그렇게 되기 쉽다. 등록 기준은
/// [ADR-0105](../docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md) 가 정한
/// 그대로다: *그 파일의 본질이 그 형태를 담는 것인가*. "고치기 번거롭다" 는 사유가
/// 아니다.
/// - `CLAUDE.md`: 규칙 본문이 번호 인용(P1)과 changelog slug(P4)를 **예시로** 든다.
///   예시를 지우면 규칙이 무엇을 금지하는지 알 수 없게 된다. 경로 인용(P3/P6)은
///   면제하지 않는다 — 규칙을 설명하는 데 실제 경로가 필요하지 않다.
/// - `.gitignore`: 제외 항목을 적는 것이 그 파일의 정의다(ADR-0105 범위 밖 4항).
/// - 이 파일 자신(P7): 모듈 머리말·패턴 doc·단위 테스트가 `TODO` 를 산문으로 쓴다 —
///   무엇을 잡고 무엇을 통과시키는지 적는 것이 이 파일의 일이라, 금지 형태를 담는
///   것이 본질이다. P7 은 숫자를 요구하지 않아 `fx!` 로 판정 지점을 끊을 수도 없다
///   (끊을 구분자가 없다 — `TODO` 라는 낱말 자체가 판정 대상이다).
/// - `scripts/check-allow-reason.sh`(P7): 그 스크립트 주석이 ADR-0037 의 규칙 본문
///   (**빈 사유·"TODO" 금지**)을 인용한다. 인용을 지우면 그 게이트가 무엇을 강제하는지
///   알 수 없게 된다 — `CLAUDE.md` 가 P1·P4 를 면제받는 것과 같은 이유다.
///
/// **면제는 여전히 패턴 단위다.** 위 둘도 P7 만 면제이고, 같은 파일에 P1·P3·P6 을
/// 심으면 잡힌다. 순회 입력으로 폴더 이름이 필요한 곳은 조각으로 조립하고
/// ([`ws_dir`]), 번호가 붙는 패턴의 픽스처는 그대로 `fx!` 로 판정 지점을 끊어 쓴다.
const ALLOWLIST: &[(&str, &[&str])] = &[
    ("CLAUDE.md", &["P1", "P4"]),
    (".gitignore", &["P6"]),
    (
        "crates/tasty-doc-guards/tests/no_todo_file_citation.rs",
        &["P7", "P8", "P9", "P10"],
    ),
    ("scripts/check-allow-reason.sh", &["P7"]),
];

/// 탐지 패턴 표 — (id, 설명, 판정 함수). 한 줄에 대해 **전부** 돌린다.
type Finder = fn(&str) -> Option<String>;
const PATTERNS: &[(&str, &str, Finder)] = &[
    ("P1", "번호 인용", find_p1),
    ("P2", "conductor 번호 인용", find_p2),
    ("P3", "경로 인용", find_p3),
    ("P4", "디자인 changelog slug", find_p4),
    ("P5", "앵커 슬러그 번호", find_p5),
    ("P6", "로컬 폴더 언급", find_p6),
    ("P7", "산문 TODO 언급", find_p7),
    ("P8", "작업 분할 번호", find_p8),
    ("P9", "작업 계획 좌표", find_p9),
    ("P10", "회차 번호", find_p10),
];

/// 순회에서 통째로 가지치기할 **이름**. 빌드 산출물·워크트리·VCS·의존성 +
/// vendored 서드파티 번들, 그리고 레포 루트의 에이전트 지침 파일.
///
/// **이름이지 디렉토리가 아니다.** [`is_pruned`] 는 종류를 묻기 전에 이름으로 자르고
/// (그 이유는 그쪽 주석에 있다 — worktree 의 `.git` 은 파일이다), 그래서 이 목록은
/// 처음부터 파일을 담아 왔다. 상수 이름이 좁은 것은 역사다.
const PRUNE_DIRS: &[&str] = &[
    "target",
    "dist",
    "_site",
    ".worktree",
    ".git",
    ".idea",
    "node_modules",
    // 레포 루트의 에이전트 지침 파일. **추적되지 않는데 이름을 옮길 수 없다** —
    // 그 도구가 레포 루트에서 이 이름을 찾고, 이 파일의 일은 커밋되지 않는 로컬
    // 전용 지침을 **이름으로 가리켜 거기로 보내는 것**이라 P3/P6 를 피할 길이 없다
    // (위임하려면 위임 대상을 이름으로 불러야 한다). 그 로컬 지침 자신은 폴더
    // 이름으로 쳐내지는데, 그것을 가리키는 루트 파일만 그물에 남는다 — 같은 처리를
    // 파일 이름으로 한다.
    //
    // 여기에 두는 이유(다른 두 자리가 아니라): `ALLOWLIST` 는
    // `allowlist_entries_point_at_things_that_exist` 가 실재를 요구해 그 파일이 없는
    // 갓 클론한 트리에서 죽은 인용이 된다. `git check-ignore` 로 묻는 것은 모듈
    // 머리말이 기각한 축이다(ADR-0096 "테스트는 환경을 읽지 않는다"). 이름 상수는
    // 환경을 안 읽고, 그 이름이 없는 트리에서는 아무것도 안 맞아 무해하다.
    "AGENTS.md",
];

/// vendored 서드파티 번들 — **파일 단위로 열거한다.**
///
/// 이유는 비용과 오탐이다. 수 MB 짜리 minified 번들을 산문 패턴으로 훑는 것이 순수
/// 비용이고, 라이브러리를 갱신했을 때 그 안의 문자열이 우연히 P4 형태를 띠면
/// **무관한 이유로 CI 가 빨개진다.**
///
/// **`assets` 라는 디렉토리 이름으로 면제하던 것을 파일 열거로 바꿨다.** 이름은 덮는
/// 범위가 열려 있어서, 그 디렉토리에 우리 콘텐츠가 들어오면 조용히 안 봐진다. 그것이
/// 가정이 아니라 이미 벌어진 상태였다 — 실측(2026-09-05) 이름 면제가 덮고 있던 것에
/// 우리가 쓴 `assets/linux/tasty.desktop` · `assets/icons/*.svg` ·
/// `crates/tasty-plugin-markdown/assets/NOTICE.md` 가 포함돼 있었다. 폰트·이미지는
/// 이름이 아니라 정본 [`tasty_doc_guards::is_binary_artifact_ext`] 가 이미 덮고 있어서,
/// 이름 면제가 실제로 더 덮던 것은
/// **우리 파일들뿐이었다.**
///
/// 열거는 새로 들어온 것을 안 덮는다는 점에서 이름과 강도가 다르다. 목록이 실재와
/// 어긋나면 [`the_vendored_list_matches_what_is_there`] 가 빨개진다.
const VENDORED_FILES: &[&str] = &[
    "crates/tasty-plugin-markdown/assets/highlight.min.js",
    "crates/tasty-plugin-markdown/assets/katex.min.css",
    "crates/tasty-plugin-markdown/assets/katex.min.js",
    "crates/tasty-plugin-markdown/assets/mermaid.min.js",
];

/// gitignored 로컬 폴더 이름의 조각. 이 파일 자신이 P6 에 걸리지 않도록 나눠 둔다.
const LOCAL_HEAD: &str = "claude";
const LOCAL_TAIL: &str = "-workspace";

/// 로컬 작업 폴더 이름(선행 `.` 없음).
fn ws_dir() -> String {
    format!("{LOCAL_HEAD}{LOCAL_TAIL}")
}

/// P6 가 잡는 로컬 폴더 — (이름, 선행 `.` 이 필요한가 = 홈에도 같은 이름이 있는가).
///
/// **긴 이름을 먼저 본다** — 짧은 쪽이 접두라, 순서를 바꾸면 로컬 작업 폴더를 보고도
/// 지침 폴더 이름으로 보고한다.
///
/// 로컬 작업 폴더는 홈에 존재할 수 없으므로 선행 `.` 유무와 무관하게 잡는다 — 점을
/// 뺀 표기(`<폴더>/temp`)로 쓰는 것이 가장 흔한 회피 형태다. 지침 폴더는 사용자 홈에도
/// 같은 이름이 있어 선행 `.` 과 직전 문맥으로 가른다.
fn local_dirs() -> Vec<(String, bool)> {
    vec![(ws_dir(), false), (LOCAL_HEAD.to_string(), true)]
}

/// 순회 가지치기 대상인지 — `PRUNE_DIRS` + gitignored 로컬 폴더(선행 `.`).
///
/// 로컬 폴더는 worktree 에 **심볼릭 링크**로 걸려 있을 수 있다. `is_dir()` 은 링크를
/// 따라가므로, 가지치기하지 않으면 순회가 레포 밖 실제 경로까지 새어나간다.
fn is_pruned(name: &str) -> bool {
    PRUNE_DIRS.contains(&name)
        || name
            .strip_prefix('.')
            .is_some_and(|rest| rest == LOCAL_HEAD || rest == ws_dir())
}

/// 이름으로 걸리거나, **디렉토리 자신이 빌드 캐시라고 밝히거나**.
///
/// 이름만 볼 때는 `CARGO_TARGET_DIR` 로 만든 다른 이름의 빌드 디렉토리가 통째로
/// 모수에 들어왔다 — 실측(2026-09-05) 이 가드가 1.30s → 86.30s 가 됐다.
/// 판정 근거와 후보 비교는 [`tasty_doc_guards::is_build_cache_dir`].
fn is_pruned_dir(path: &Path, name: &str) -> bool {
    is_pruned(name) || tasty_doc_guards::is_build_cache_dir(path)
}

/// 금지되는 하위 디렉토리 — 이 넷 뒤에 오는 경로만 P3 가 잡는다.
const FORBIDDEN_SUBDIRS: &[&str] = &["todo-conductor", "todo", "plans", "conductor"];

/// 이름 **바로 앞** 에 붙는 홈 경로 접두. 경로 구분자 한 겹은 벗기고 본다.
const HOME_PREFIXES: &[&str] = &["~", "$home", "%userprofile%"];

/// 이름 직전 짧은 창 안에 **단어로** 있으면 홈 문맥으로 보는 표기(코드/산문).
///
/// 단어 경계를 요구하는 이유: 부분문자열로 보면 `Homebrew` 나 `renderHome()` 같은
/// 무관한 낱말이 그 줄의 진짜 위반을 면제시킨다. 영숫자가 앞뒤에 붙으면 다른 낱말로
/// 본다(`home_dir` 는 `_` 가 경계라 `home` 으로 잡힌다).
const HOME_NEARBY: &[&str] = &["home", "claude_config_dir", "홈의"];

/// 직전 문맥을 보는 창 크기(문자 수).
const HOME_WINDOW: usize = 32;

/// 구분자 런을 소비한다 — 개수를 제한하면 공백 하나만 더 넣어도 회피된다.
fn skip_run(bytes: &[u8], mut i: usize, allowed: &[u8]) -> usize {
    while i < bytes.len() && allowed.contains(&bytes[i]) {
        i += 1;
    }
    i
}

/// `bytes[i..]` 가 숫자로 시작하면 그 숫자열의 끝 인덱스.
fn digits_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return None;
    }
    let mut end = i;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    Some(end)
}

/// 한국어 서수 접미사 — 번호 뒤에 붙는다(`21번`).
const KOREAN_ORDINAL: &str = "번";

/// 여는 괄호로 감싼 번호(`(11)` · `[7]` · `(21번)`)의 끝 인덱스 — 닫는 괄호 다음.
///
/// **짝을 요구하는 것이 오탐 방지의 핵심이다.** 여는 괄호만 보고 숫자를 받으면
/// `TODO (2026 년부터` 같은 평범한 문장이 걸린다. 그래서 여는 괄호에 맞는 닫는
/// 괄호가 숫자(+ 선택적 `번`) 바로 뒤에 와야만 인정한다 — 이 형태는 문장부호가
/// 아니라 **번호를 감싸는 묶음**이라, 모듈 머리말이 `:` · `#` · `.` 를 구분자로
/// 받지 않기로 한 근거(평범한 문장의 오탐)가 여기에는 적용되지 않는다.
fn bracketed_number_end(line: &str, i: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let close = match bytes.get(i)? {
        b'(' => b')',
        b'[' => b']',
        _ => return None,
    };
    let num_start = skip_run(bytes, i + 1, b" \t");
    let after_digits = digits_end(bytes, num_start)?;
    // 한국어 서수(`21번`)까지가 번호다 — 괄호 안에서만 보므로 `이번` 류와 섞이지 않는다.
    let after_ordinal = match line[after_digits..].strip_prefix(KOREAN_ORDINAL) {
        Some(_) => after_digits + KOREAN_ORDINAL.len(),
        None => after_digits,
    };
    let end = skip_run(bytes, after_ordinal, b" \t");
    (bytes.get(end) == Some(&close)).then_some(end + 1)
}

/// P1 — 번호와 `TODO` 가 붙어 있는 형태를 **양쪽 어순 모두** 잡는다.
/// 뒤 어순은 `todo` + 구분자 런(공백·탭·`/`) + (선택적 하이픈 + 숫자 | 괄호로 감싼 숫자),
/// 앞 어순은 숫자 + `번` + 공백 런 + `todo`(한국어 문장의 자연스러운 순서).
/// 번호 없는 평범한 `TODO:` 주석은 대상이 아니다(금지 대상은 *파일 번호 인용*
/// 이지 할 일 표시가 아니다).
///
/// **대소문자를 가리지 않는다** — 모듈 머리말의 "구분자 개수·대소문자로 회피되지
/// 않아야 한다" 가 이 패턴에도 걸린다. 넓히는 쪽이 오탐을 늘리지 않는 이유는 이
/// 패턴이 **숫자가 바로 붙을 것**을 요구하기 때문이다: 영단어·식별자로서의 `todo`
/// 는 뒤에 숫자가 아니라 글자나 `_` 가 온다(`todos` · `todo_marker_end`). 실측
/// 2026-09-20 — 추적 파일에 소문자 `todo` 가 **123** 자리 있었고 그중 숫자가
/// 뒤따르는 것은 **8** 자리였다. 그 8 은 전부 죽은 좌표(3)이거나 이 파일 자신의
/// 픽스처(5)였고 **산문·식별자 오탐은 0** 이었다. 오탐 위험이 사는 축은 대소문자가
/// 아니라 **구분자**이고, 그쪽은 모듈 머리말이 이미 좁혀 두었다.
///
/// **`/` 는 구분자로 받는다.** 티켓 번호는 경로 표기로도 인용되고(`todo/<숫자>`), 임의
/// 문장부호를 뺀 근거("`TODO. 40` 같은 평범한 문장이 걸린다")가 `/` 에는 서지 않는다 —
/// 낱말과 숫자 사이의 `/` 는 문장 부호가 아니라 경로 구분자다. 실측 2026-09-20 —
/// 추적 파일에서 `todo/` 형태는 7 자리였고, 그중 산문은 둘(`TODO/changelog` ·
/// 콜아웃 종류 `note/todo/abstract`)인데 **둘 다 뒤에 숫자가 없어** 이 패턴에 걸리지
/// 않는다. 숫자를 요구하는 것이 여기서도 방벽이다.
fn find_p1(line: &str) -> Option<String> {
    // `to_ascii_lowercase` 는 바이트 길이를 보존하므로 인덱스가 `line` 과 같다 —
    // 반환하는 인용문은 **원문 슬라이스**로 돌려준다(보고에 원문이 보여야 한다).
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("todo") {
        let start = from + pos;
        let after_space = skip_run(bytes, start + 4, b" \t/");
        let mut i = after_space;
        if i < bytes.len() && bytes[i] == b'-' {
            i += 1;
        }
        if let Some(end) = digits_end(bytes, i) {
            return Some(line[start..end].to_string());
        }
        if let Some(end) = bracketed_number_end(line, after_space) {
            return Some(line[start..end].to_string());
        }
        if let Some(num_start) = korean_ordinal_start(line, start) {
            return Some(line[num_start..start + 4].to_string());
        }
        from = start + 4;
    }
    None
}

/// 앞 어순 판정 — `line[..todo_at]` 의 꼬리가 `숫자+번` + 공백 런인지 본다.
/// 맞으면 숫자열이 시작하는 바이트 인덱스.
fn korean_ordinal_start(line: &str, todo_at: usize) -> Option<usize> {
    let head = line[..todo_at].trim_end_matches([' ', '\t']);
    let digits = head.strip_suffix(KOREAN_ORDINAL)?;
    let num_start = digits.len()
        - digits
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .count();
    (num_start < digits.len()).then_some(num_start)
}

/// P2 — `todo-conductor`(대소문자 무시) + 구분자 런 + 숫자.
fn find_p2(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let needle = "todo-conductor";
    let mut from = 0;
    while let Some(pos) = lower[from..].find(needle) {
        let start = from + pos;
        let i = skip_run(bytes, start + needle.len(), b" \t/_-");
        if let Some(end) = digits_end(bytes, i) {
            return Some(line[start..end].to_string());
        }
        from = start + needle.len();
    }
    None
}

/// P3 — 로컬 작업 폴더 + 금지 하위 디렉토리. 대소문자·슬래시 개수로 회피되지 않는다.
fn find_p3(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let dir = ws_dir();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(&dir) {
        let start = from + pos;
        from = start + dir.len();
        let after = skip_run(bytes, from, b"/\\");
        if after == from {
            continue; // 폴더 이름 뒤에 경로 구분자가 없다 — 하위 인용이 아니다.
        }
        if let Some(sub) = FORBIDDEN_SUBDIRS
            .iter()
            .find(|s| lower[after..].starts_with(**s))
        {
            return Some(format!("{dir}/{sub}"));
        }
    }
    None
}

/// P4 — 디자인 changelog 판정 slug(`YYYY-MM-DD-<slug>`). 대소문자를 가리지 않는다.
fn find_p4(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let is_d = |i: usize| i < bytes.len() && bytes[i].is_ascii_digit();
    let is_dash = |i: usize| i < bytes.len() && bytes[i] == b'-';
    for start in 0..bytes.len() {
        // 앞 글자가 숫자면 연도 4 자리의 시작이 아니다(더 긴 숫자열의 중간).
        if start > 0 && bytes[start - 1].is_ascii_digit() {
            continue;
        }
        if !(is_d(start) && is_d(start + 1) && is_d(start + 2) && is_d(start + 3)) {
            continue;
        }
        if !(is_dash(start + 4) && is_d(start + 5) && is_d(start + 6)) {
            continue;
        }
        if !(is_dash(start + 7) && is_d(start + 8) && is_d(start + 9)) {
            continue;
        }
        if !is_dash(start + 10) {
            continue;
        }
        let mut end = start + 11;
        if !(end < bytes.len() && bytes[end].is_ascii_alphabetic()) {
            continue;
        }
        while end < bytes.len() && (bytes[end].is_ascii_alphabetic() || bytes[end] == b'-') {
            end += 1;
        }
        return Some(line[start..end].to_string());
    }
    None
}

/// 앵커 슬러그 안의 번호(`todo-<숫자>`)를 찾는다. 슬러그 시작이거나 `-` 뒤에 와야
/// 한다 — `todo-conductor` 는 뒤가 숫자가 아니라 걸리지 않는다. 호출부가 소문자로
/// 낮춘 슬러그를 넘긴다.
fn slug_todo_number(slug: &str) -> Option<String> {
    let bytes = slug.as_bytes();
    let needle = "todo-";
    let mut from = 0;
    while let Some(pos) = slug[from..].find(needle) {
        let start = from + pos;
        // `start - 1` 이 멀티바이트 연속 바이트여도 `-`(ASCII) 와는 절대 같지 않다.
        let at_boundary = start == 0 || bytes[start - 1] == b'-';
        let digits_start = skip_run(bytes, start + needle.len(), b"-");
        from = start + needle.len();
        if !at_boundary {
            continue;
        }
        if let Some(end) = digits_end(bytes, digits_start) {
            return Some(slug[start..end].to_string());
        }
    }
    None
}

/// P5 — 마크다운 앵커(`#<슬러그>`) 안에 굳은 번호.
fn find_p5(line: &str) -> Option<String> {
    let mut from = 0;
    while let Some(pos) = line[from..].find('#') {
        let start = from + pos;
        let rest = &line[start + 1..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, ')' | '`' | ',' | '"' | '\''))
            .map(|i| start + 1 + i)
            .unwrap_or(line.len());
        let slug = &line[start + 1..end];
        if let Some(hit) = slug_todo_number(&slug.to_ascii_lowercase()) {
            return Some(format!("#{slug} ({hit})"));
        }
        from = start + 1;
    }
    None
}

/// 이름 직전 문맥이 사용자 홈을 가리키는가.
///
/// **줄 전체가 아니라 그 자리 직전만** 본다. 줄 전체를 훑으면 정당한 홈 경로가 한 번
/// 나오는 줄에 섞인 진짜 레포 로컬 참조까지 통과한다. `at` 은 선행 `.` 의 인덱스다.
fn home_context_before(lower: &str, at: usize) -> bool {
    let head = &lower[..at];
    // ① 경로 접두가 바로 앞에 붙은 형태 — 구분자는 **런으로** 벗긴다(소스에서
    //    이스케이프된 `\\` 나 중복 `/` 로 회피되지 않게).
    let trimmed = head.trim_end_matches(['/', '\\']);
    if HOME_PREFIXES.iter().any(|p| trimmed.ends_with(p)) {
        return true;
    }
    // ② 코드/산문 문맥이 직전 짧은 창 안에 낱말로 있는 형태.
    let window = head
        .char_indices()
        .rev()
        .take(HOME_WINDOW)
        .last()
        .map_or(head, |(i, _)| &head[i..]);
    HOME_NEARBY.iter().any(|a| contains_word(window, a))
}

/// `hay` 안에 `word` 가 **낱말로** 있는가 — 앞뒤가 영숫자면 다른 낱말의 일부다.
fn contains_word(hay: &str, word: &str) -> bool {
    let bytes = hay.as_bytes();
    let mut from = 0;
    while let Some(pos) = hay[from..].find(word) {
        let start = from + pos;
        from = start + word.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = bytes.get(from).is_none_or(|c| !c.is_ascii_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// P6 — 레포 로컬 폴더 언급. 하위 경로가 무엇이든, 아예 없든 잡는다.
///
/// P3 는 네 개 하위 디렉토리가 뒤따를 때만 잡았다. ADR-0105 가 폴더 이름 단독 언급
/// 까지 금지로 확정했으므로 그 범위를 여기서 강제한다.
fn find_p6(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for (name, needs_dot) in local_dirs() {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&name) {
            let start = from + pos;
            from = start + name.len();
            let dotted = start > 0 && bytes[start - 1] == b'.';
            if needs_dot && !dotted {
                continue; // 홈에도 있는 이름은 경로 표기일 때만 대상이다.
            }
            // 이름(또는 선행 `.`) 앞이 식별자 문자면 더 긴 이름의 일부다
            // (reverse-DNS plugin id, `tasty-plugin-<이름>` 등). 경로 시작이 아니다.
            let prev = if dotted {
                start.checked_sub(2)
            } else {
                start.checked_sub(1)
            };
            if let Some(p) = prev {
                let c = bytes[p];
                if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                    continue;
                }
            }
            // 뒤가 식별자 문자면 다른 이름이다. 긴 이름을 먼저 보므로 로컬 작업
            // 폴더는 이 검사에 걸리기 전에 잡힌다.
            if let Some(&c) = bytes.get(from)
                && (c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                continue;
            }
            if needs_dot && home_context_before(&lower, start - 1) {
                continue;
            }
            return Some(if dotted {
                format!(".{name}")
            } else {
                name.clone()
            });
        }
    }
    None
}

/// 할 일 표시(`TODO:` · `TODO(<범위>):`)의 끝 인덱스 — 콜론 다음.
///
/// **판정선이 콜론인 이유**: 티켓을 가리키는 산문은 `TODO` 를 문장 안의 명사로 쓰고
/// (`이 TODO 는` · `a separate TODO if needed`), 할 일 표시는 그 뒤에 곧바로 콜론을
/// 붙여 설명을 연다(`TODO: ...` · `TODO(<범위>): ...`). 괄호 형태는 **닫는 괄호 뒤에
/// 콜론이 와야** 인정한다 — 괄호만 보면 `TODO(…)` 로 끝나는 산문까지 통과한다.
/// 괄호 안은 무엇이든 좋다(담당자·기한·조건). P1 이 괄호 안 *숫자* 를 티켓 번호로
/// 잡는 것과 겹치지 않는다: 그쪽은 콜론이 없어 여기서 마커로 인정되지 않는다.
fn todo_marker_end(line: &str, after_todo: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let colon_at = match bytes.get(after_todo)? {
        b':' => return Some(after_todo + 1),
        b'(' => after_todo + 1 + line[after_todo + 1..].find(')')? + 1,
        _ => return None,
    };
    (bytes.get(colon_at) == Some(&b':')).then_some(colon_at + 1)
}

/// P7 — 번호가 붙지 않은 산문 `TODO`. 할 일 표시([`todo_marker_end`])만 통과한다.
///
/// **소문자 `todo` 는 보지 않는다 — P1 과 달리 여기는 숫자를 요구하지 않는다.**
/// 그 차이가 이 축을 가른다: P1 은 `todo` 뒤에 숫자가 붙을 것을 요구해 영단어·식별자
/// 와 저절로 갈리지만, 이 패턴은 `todo` 라는 낱말 하나가 곧 판정 대상이라 소문자로
/// 넓히면 `todos` 를 뺀 모든 식별자·URL·영단어가 그대로 들어온다. 티켓을 가리키는
/// 산문은 관례상 대문자로 쓰므로 그 선에서 멈춘다. 실측 2026-09-20 — 추적 파일의
/// 소문자 `todo` 123 자리 중 숫자가 뒤따르는 것은 8 자리뿐이라, 이 패턴을 같이
/// 넓히면 나머지 **115** 자리가 그대로 위반으로 들어온다.
fn find_p7(line: &str) -> Option<String> {
    let mut from = 0;
    while let Some(pos) = line[from..].find("TODO") {
        let start = from + pos;
        let after = start + 4;
        from = after;
        if todo_marker_end(line, after).is_none() {
            return Some(line[start..after].to_string());
        }
    }
    None
}

/// P8 — 괄호로 감싼 **작업 분할 번호**(`(01)`~`(09)`). 기능 하나를 여러 단계로 쪼갠
/// 작업 계획의 "몇 번째 항목" 이고, 그 계획은 커밋되지 않는 로컬 문서다. P1 이 잡는
/// 번호는 `TODO` 라는 낱말을 옆에 달고 있지만 이쪽은 번호만 홀로 선다 — 그래서 P1 의
/// 어느 어순으로도 안 걸린다.
///
/// 실측 2026-09-14: 이 형태가 180 자리에 있었고, 그중 한 갈래는 문서 어디에도 대응
/// 항목이 없었다(`(05)` — 그 작업 항목이 문서를 안 남겼다). 나머지도 "번호 = 뜻" 을
/// 세운 자리가 없어, 읽는 사람은 번호를 보고 갈 곳이 없다. 커밋 제목이 그 번호를 달고
/// 있던 것이 출처다(`... 갤러리 specimen (09)`).
///
/// **두 자리·앞자리 0 만 본다.** 한 자리(`(3)`)나 앞자리가 0 이 아닌 수(`(12)`)는 데이터
/// 쪽이 압도적으로 많다 — 각주·항목 번호·측정값. 앞자리 0 은 자리수를 맞춘 **일련번호**
/// 표기라 이 부류를 고르게 집는다. ADR 좌표는 네 자리(`ADR-0054`)라 걸리지 않는다.
///
/// **맨번호는 안 본다** — `06 bulk 채널` 처럼 괄호 없이 문장에 녹은 형태도 같은 부류지만,
/// 날짜·버전·측정값과 구분할 표지가 없어 세면 오탐이 본문을 덮는다. 그쪽은 사람이
/// 문장을 다시 써야 하고, 이 가드는 다시 스며드는 입구만 막는다.
fn find_p8(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut from = 0;
    while let Some(pos) = line[from..].find('(') {
        let start = from + pos;
        from = start + 1;
        let d0 = bytes.get(start + 1);
        let d1 = bytes.get(start + 2);
        if d0 == Some(&b'0')
            && d1.is_some_and(|c| c.is_ascii_digit() && *c != b'0')
            && bytes.get(start + 3) == Some(&b')')
        {
            return Some(line[start..start + 4].to_string());
        }
    }
    None
}

/// P9 — 점으로 이은 **작업 계획 좌표**(`D.3.C.B.1` · `D.3.C.G.3.c` · `D.3.C.B.10.1`).
/// 기능 하나를 단계로 쪼갠 계획의 마디 번호이고, 그 계획은 커밋되지 않는 로컬 문서다.
/// P8 이 잡는 `(0N)` 과 출처가 같다 — 2026-05 커밋 제목이 그 좌표를 달고 있었다
/// (`refactor(engine): ... (D.3.C.B.1 step 1)`).
///
/// 실측 2026-09-14: 이 형태가 53 자리에 있었고, 레포의 `.md` 어디에도 그 좌표를
/// 정의한 자리가 **0 곳**이었다. 읽는 사람은 번호를 보고 갈 곳이 없다.
///
/// **네 마디까지만 본다** — `<대문자>.<숫자>.<대문자>.<대문자>`. 뒤에 마디가 더 붙든
/// (`.19` · `.3.c`) 안 붙든 앞 네 마디가 이 부류를 고르게 집는다. 숫자만 점으로 이은
/// 것(버전 `1.2.3`)이나 문장 끝의 약어는 대문자와 숫자가 번갈아 오지 않아 안 걸린다.
/// 실측: 이 모양의 잔여가 0 인 트리에서 이 판정기가 잡는 자리도 0 이다.
fn find_p9(line: &str) -> Option<String> {
    let b = line.as_bytes();
    for i in 0..b.len() {
        // 앞이 단어 문자면 좌표의 머리가 아니다.
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.') {
            continue;
        }
        if !b[i].is_ascii_uppercase() || b.get(i + 1) != Some(&b'.') {
            continue;
        }
        let mut j = i + 2;
        let ds = j;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j == ds || b.get(j) != Some(&b'.') {
            continue;
        }
        j += 1;
        if !b.get(j).is_some_and(u8::is_ascii_uppercase) || b.get(j + 1) != Some(&b'.') {
            continue;
        }
        j += 2;
        if b.get(j).is_some_and(u8::is_ascii_uppercase) {
            return Some(line[i..=j].to_string());
        }
    }
    None
}

/// P10 — `R` + 두 자리 이상 숫자(`R56` · `R476` · `R1147`). 회차 분석 기록의 번호이고,
/// 그 기록은 커밋되지 않는 로컬 문서다. 번호만 홀로 서서 `TODO` 라는 낱말이 없으므로
/// P1 의 어느 어순으로도 안 걸린다.
///
/// 실측 2026-09-14: 이 형태가 414 자리·74 파일에 있었다. 소스 주석과 문서 본문 양쪽에
/// 퍼져 있었고, 절반 가까이는 `(R476)` 처럼 괄호 하나로 문장 끝에 달려 있었다 —
/// 그 괄호가 근거를 대신하고 있었으므로 빼면 근거가 사라지는 자리가 많았다. 그래서
/// 처방은 번호를 빼는 것이 아니라 **번호가 대신하던 명제를 그 자리에 적는 것**이다.
///
/// **두 자리부터 본다.** 한 자리 `R1` 은 데이터 쪽이 압도적이다(좌표축·순번). 두 자리
/// 이상은 이 레포에서 전부 회차 번호였다 — 실측으로 확인한 선이지 어림이 아니다.
///
/// ★ **이 술어의 좌변은 표기지 가리키는 것이 아니다.** 같은 회차를 우리말로 적은
/// `회차 94` 는 `R94` 와 완전히 같은 죽은 참조인데 여기 안 걸린다. 그 형태가 실제로
/// 있었고(실측 2026-09-20: 49 자리 · 19 파일), 이 술어를 세운 sweep 이 ascii 표기만
/// 보고 지나쳤다. 그 49 자리는 손으로 갈아 없앴다.
///
/// **그런데도 그 형태를 여기서 안 센다 — 판정문이 대다수 자리에서 틀리기 때문이다**
/// (`docs/adr/0243-not-building-a-judge-has-three-reasons-and-the-left-side-is-the-last-one.md`
/// 의 (ㄴ)). `회차` 는 번호를 붙이는 이름이기도 하지만 **세는 단위**이기도 하다 —
/// "끝난 회차 2" · "값으로 센 회차 41" · "앞 회차 817" 은 전부 수량이다. 갈라 낼 표지가
/// 낱말에 없어서, 지금 이 레포에서 그 패턴이 잡는 것은 **여섯 자리 전부가 정상 용법**이고
/// 위반은 0 이다. 그 상태로 세우면 실패문이 "번호를 빼라" 를 정상 문장에 시킨다.
/// 다시 셀 때는 `git grep -nIE '회차 [0-9]+'` 로 뽑아 사람이 갈랐다.
fn find_p10(line: &str) -> Option<String> {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'R' {
            continue;
        }
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
            continue;
        }
        let mut j = i + 1;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        if j - (i + 1) < 2 {
            continue;
        }
        if b.get(j)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            continue;
        }
        return Some(line[i..j].to_string());
    }
    None
}

/// 패턴의 **적용 범위** — 파일 종류로 갈리는 것만 여기서 뺀다.
///
/// [`ALLOWLIST`] 와 다른 축이다: 면제는 *그 파일 하나* 가 금지 형태를 담는 것이 본질일
/// 때 주는 것이고, 여기는 *확장자 전체* 에 대해 그 패턴이 애초에 물음이 아닌 경우다.
/// 면제로 흉내 내면 `.md` 파일 수만큼 항목이 늘고, 새 문서마다 항목을 더해야 한다.
///
/// **P7 은 마크다운을 안 본다.** 문서는 규칙 본문을 인용하는 것이 자기 일이고(이 가드가
/// 무엇을 금지하는지 적으려면 그 형태를 그대로 적어야 한다), 상류 라이브러리가 자기
/// 소스에 남긴 마커를 서술하는 자리도 문서다. 산문 언급이 거기서는 정상이다. 코드
/// 주석에는 그 일이 없다 — 티켓을 가리키는 것 말고 `TODO` 를 산문으로 쓸 이유가 없다.
fn out_of_scope(id: &str, rel: &str) -> bool {
    id == "P7" && rel.to_ascii_lowercase().ends_with(".md")
}

/// 스캔에서 뺄 확장자 — 바이너리라 인용을 담을 수 없는 것. `read_to_string` 이
/// 비-UTF8 을 걸러 주지만, 여기서 먼저 쳐내 순회 비용을 줄인다.
/// 스캔 대상 파일인지 — repo-relative 경로 기준.
///
/// **denylist 전수 방식**: 순회가 닿은 파일은 기본적으로 전부 대상이고 바이너리
/// 확장자만 뺀다(정본 [`tasty_doc_guards::is_binary_artifact_ext`]). 확장자가 없는
/// 파일(`Justfile` · 훅 스크립트)도 대상이다. 근거는 모듈 주석 "스캔 대상 정의" 참조.
fn is_scan_target(rel: &str) -> bool {
    if VENDORED_FILES.contains(&rel) {
        return false;
    }
    let name = rel.rsplit('/').next().unwrap_or("");
    // 선행 `.` 은 확장자 구분자가 아니다 — dotfile 은 확장자 없음으로 본다.
    let ext = name
        .trim_start_matches('.')
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext {
        Some(e) => !tasty_doc_guards::is_binary_artifact_ext(&e),
        None => true,
    }
}

/// 스캔이 **트리의 큰 갈래마다 닿았는가** — 좌변이 조용히 줄어드는 것을 잡는다.
///
/// 이 가드의 좌변은 [`gather`] 가 모은 파일이고, 그 목록이 **줄어드는 것은 아무도 안 봤다.**
/// 실측 2026-09-08: [`is_pruned`] 에 `docs` 한 줄을 더하면 좌변이 **1952 → 1552** 로 400
/// 줄고 이 파일의 18 개가 **그대로 초록이었다.** P3(경로 인용)이 가장 많이 깨질 자리가
/// 통째로 빠졌는데 화면에는 아무 차이도 없다 — 0 이 "안 걸렸다" 인지 "안 쟀다" 인지
/// 안 갈리는 자리였다.
///
/// **모수 하한(`>= N`)을 안 쓴 이유** — ADR-0243 의 **(ㅁ) 순서** 칸이다. 그 `N` 의 여유를
/// 정하려면 이 모수의 증감 폭을 재야 하는데 **안 쟀다**. 안 잰 수를 하한 옆에 적으면 다음
/// 사람이 그것을 실측으로 읽는다.
///
/// **재진입 조건**: 이 좌변을 **두 시점 이상에서 세면** 그때 하한을 붙인다. 지금 점은
/// 1952(2026-09-08) 하나뿐이고, 점이 하나면 증감 폭이 아니라 값 하나다. 두 번째 점이
/// 생기는 자리는 이 파일이 빨개져 좌변 수가 실패문에 찍히는 때다 — 그 수를 여기 적어라.
/// 갈래 확인은 여유가 필요 없고, 순회가 통째로 죽는 것과 한 갈래만 빠지는 것을 **같은
/// 판정으로** 잡는다.
/// ## 이 형태가 하한 28 중 몇을 대체하는가 (실측 `integration-94` = `95732c018`, 2026-09-08)
///
/// 여기 쓴 것은 **갈래 확인**이다 — 여유가 없고, 순회가 통째로 죽는 것과 한 갈래만 빠지는
/// 것을 같은 판정으로 잡는다. 하한(`>= N`)과 경쟁하는 것이 아니라 **다른 물음**이다:
/// 하한은 "얼마나 줄면 이상한가", 갈래 확인은 "이 갈래에 하나라도 닿는가".
///
/// 트리의 `const X: Floor = Floor {` 선언 **28** 개에 대해, 그 순회가 실제로 담은 것을
/// `walk_with_floor`·`walk_dirs_with_floor` 에 훅을 심어 재고(공통 접두를 벗긴 **가장 얕은
/// 분기점**에서 갈래를 셌다), 갈래 중 **디렉토리인 것**의 수로 갈랐다.
///
/// | 갈래 | 수 | 판정 |
/// |---|---|---|
/// | 갈래 확인으로 **대체 가능** (디렉토리 갈래 ≥ 2, 갈래 > 5) | 15 | 갈래가 이름으로 안정적이고, 하나가 통째로 빠지는 것이 실제 사고 형태다 |
/// | **둘 다 필요** (디렉토리 갈래 ≥ 2, 갈래 ≤ 5) | 2 | 갈래가 적어 하나가 두껍다 — "하나라도"는 그 갈래 **안**의 절반 소실을 원리적으로 못 잡는다 |
/// | **하한만 가능** (디렉토리 갈래 ≤ 1) | 10 | 평평하거나 갈래가 하나다. 평평한 순회에서 갈래 확인은 **파일 명부**가 되어 다른 처방이 된다 |
/// | 미측정 | 1 | `guard_test_channels_stay_split::JOB_FLOOR` — 공용 순회를 안 거쳐 훅이 안 닿았다 |
///
/// 하한만 가능한 10 의 실물: `.github/workflows` 셋(11 개 `.yml` 평평) · `lang` 둘(`en/ja/ko.toml`)
/// · 루트 `tests` 둘(39 개 `.rs`) · `MARKER_FLOOR`(`target` 하나) · `DOCS_DIR_FLOOR`(디렉토리
/// 순회) · `PERSISTENCE_FLOOR`(`layout_persistence` 한 모듈).
///
/// **갈래별 파일 수는 안 쟀다** — "둘 다 필요" 를 갈래 수 5 로 근사한 것이 그래서다.
/// 한 갈래가 좌변의 절반을 넘는지 재면 그 칸의 술어가 수로 바뀐다.
///
const MUST_BE_SCANNED: &[&str] = &["docs/", "src/", "crates/", "scripts/", ".github/"];

/// `path` 하위를 재귀 순회하며 스캔 대상 파일을 모은다. `is_pruned` 는 가지치기.
///
/// 디렉토리를 읽지 못하면 **panic 한다.** 조용히 건너뛰면 가드가 도는 줄 알면서
/// 실제로는 그 하위를 통째로 안 보는 상태가 되고, 그건 위양성보다 나쁘다.
fn gather(path: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        let rel = rel_of(path, root);
        if is_scan_target(&rel) {
            out.push(path.to_path_buf());
        }
        return;
    }
    let entries = std::fs::read_dir(path)
        .unwrap_or_else(|e| panic!("스캔 대상 디렉토리를 읽지 못했다: {} — {e}", path.display()));
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|e| panic!("디렉토리 항목을 읽지 못했다: {} — {e}", path.display()));
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // ★ **이름으로 하는 가지치기는 종류를 묻지 않는다.** worktree 에서 `.git` 은
        // 디렉토리가 아니라 `gitdir:` 한 줄이 든 **파일**이다 — 종류를 먼저 물으면
        // 그 파일이 가지치기를 빠져나가 모집단에 들고, 같은 커밋이 worktree 와 메인
        // 체크아웃에서 서로 다른 파일을 보게 된다. 모집단이 환경을 읽으면 답도
        // 언젠가 환경을 읽는다.
        if is_pruned(name) {
            continue;
        }
        // 캐시 표식 판정은 디렉토리 **안** 을 읽는다 — 파일에는 뜻이 없다.
        if p.is_dir() && is_pruned_dir(&p, name) {
            continue;
        }
        gather(&p, root, out);
    }
}

/// `rel` 이 면제받는 패턴 id 들. `ALLOWLIST` 조회를 순회에서 분리한 것이라
/// 합성 경로로 면제 창을 직접 찌를 수 있다.
///
/// 경로는 **정확 일치**다 — 접두/접미로 넓히면 `CLAUDE.md` 하나를 면제한 것이
/// `docs/CLAUDE.md` 나 `CLAUDE.md.bak` 까지 덮는다.
fn allowed_patterns(rel: &str) -> &'static [&'static str] {
    ALLOWLIST
        .iter()
        .find(|(f, _)| *f == rel)
        .map_or(&[], |(_, pats)| *pats)
}

/// 한 줄에 대한 판정 — 면제 적용까지 포함한다. 순회(파일 열기·경로 처리)와 갈라 둔 이유는
/// 면제를 겨냥한 변이를 **합성 문자열**로 찌르기 위해서다. 판정이 순회 안에 인라인으로
/// 있으면 면제 창을 시험하려면 레포에 진짜 위반을 심는 수밖에 없고, 그건 느린 데다
/// 되돌리다 사고가 난다.
fn violations_in_line(rel: &str, line: &str) -> Vec<String> {
    let allowed = allowed_patterns(rel);
    PATTERNS
        .iter()
        .filter(|(id, _, _)| !allowed.contains(id) && !out_of_scope(id, rel))
        .filter_map(|(id, kind, find)| find(line).map(|m| format!("{id} {kind}: `{m}`")))
        .collect()
}

fn rel_of(file: &Path, root: &Path) -> String {
    file.strip_prefix(root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn no_todo_file_citation() {
    let root = &tasty_doc_guards::repo_root();
    let mut files = Vec::new();
    gather(root, root, &mut files);
    files.sort();

    let mut violations = Vec::new();
    for file in files {
        let rel = rel_of(&file, root);
        let Ok(contents) = std::fs::read_to_string(&file) else {
            continue; // 비-UTF8 은 인용을 담을 수 없다.
        };
        for (i, line) in contents.lines().enumerate() {
            for found in violations_in_line(&rel, line) {
                violations.push(format!("  {}:{} — {found}", rel, i + 1));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "커밋되는 파일이 git 에 올라가지 않는 경로(로컬 작업 폴더 · 로컬 지침 폴더)나 \
         그 안의 문서·디자인 changelog slug 를 인용했다 — 그 좌표는 clone 한 사람에게 \
         존재한 적이 없고, 번호는 재사용되어 무관한 문서로 해석된다. 규칙 전문과 범위 밖 \
         4 종은 `docs/adr/0105-no-nongit-path-refs-in-tracked-sources.md`.\n\
         대체 수단 3 가지 중 하나를 쓸 것: (1) 이유가 자명하면 번호 대신 이유를 직접 서술 \
         (2) 설계 결정이 크면 `docs/adr/` 에 ADR 을 쓰고 그 경로를 인용 \
         (3) 기능 동작 설명이면 `docs/`(dev-guide / features / plugins) 문서를 참조.\n\
         앵커(P5)면 제목에서 번호를 떼고 그 제목을 가리키던 참조도 함께 고칠 것 — \
         제목의 번호는 앵커로 굳어 링크·주석으로 퍼진다.\n\
         P6 면 위치를 적지 말고 \"커밋되지 않는 로컬 전용 지침이 정한다\" 로 위임할 것.\n\
         P7(산문 `TODO`)이면 티켓을 가리키던 말을 지우고 그 자리에서 이유를 한 줄로 \
         서술할 것 — 할 일 표시(`TODO:` · `TODO(<범위>):`)는 그대로 둬도 된다.\n\
         그 형태를 담는 것이 본질인 파일이면 ALLOWLIST 에 (경로, 허용 패턴) 으로 추가:\n{}",
        violations.join("\n")
    );
}

// ── 패턴 함수 단위 테스트 ────────────────────────────────────────────────
//
// 메인 스캔은 "지금 레포가 깨끗한가" 만 말해 준다. 패턴이 *무엇을 잡고 무엇을
// 통과시키는지* 는 레포 상태와 무관하게 고정돼야 한다 — 특히 오탐 회피 쪽은
// 레포에 그 형태가 남아 있지 않으면 메인 스캔이 영영 검증하지 못한다.

/// 픽스처 조립 — 이 파일은 ALLOWLIST 면제가 없으므로, 금지 형태를 그대로 적으면
/// 자기 스캔에 걸린다. 조각을 끊는 지점은 각 패턴의 **판정 지점**이라(구분자 앞,
/// 폴더 이름 중간) 조립 결과는 리터럴과 같고 소스에는 그 형태가 남지 않는다.
/// `concat!` 이라 런타임 비용도 없다.
macro_rules! fx {
    ($($p:literal),+ $(,)?) => { concat!($($p),+) };
}

#[test]
fn p1_catches_numbered_todo_citation_only() {
    assert_eq!(
        find_p1(fx!("see TODO", " 40")),
        Some(fx!("TODO", " 40").into())
    );
    assert_eq!(find_p1(fx!("(TODO", "18)")), Some(fx!("TODO", "18").into()));
    assert_eq!(
        find_p1(fx!("TODO", "-7 은 이미 닫혔다")),
        Some(fx!("TODO", "-7").into())
    );
    // 공백 런 — 하나만 소비하면 공백 두 개로 회피된다.
    assert_eq!(
        find_p1(fx!("see TODO", "  40")),
        Some(fx!("TODO", "  40").into())
    );
    assert_eq!(
        find_p1(fx!("see TODO", " \t 40")),
        Some(fx!("TODO", " \t 40").into())
    );
    assert_eq!(
        find_p1(fx!("see TODO", " -40")),
        Some(fx!("TODO", " -40").into())
    );
    // 번호 없는 평범한 할 일 표시는 대상이 아니다.
    assert_eq!(find_p1("// TODO: refactor this later"), None);
    assert_eq!(find_p1("TODOS 는 소문자 아님"), None);
    // 임의 문장부호는 구분자로 보지 않는다 — 평범한 문장의 오탐을 막는다.
    // 앞 어순(한국어) — 구분자 개수와 무관하게 잡는다.
    assert_eq!(
        find_p1(fx!("17번 ", "TODO", " — mesh mirror")),
        Some(fx!("17번 ", "TODO").into())
    );
    assert_eq!(
        find_p1(fx!("(18번", "TODO", ")")),
        Some(fx!("18번", "TODO").into())
    );
    // `번` 앞에 숫자가 없으면 티켓 인용이 아니다.
    assert_eq!(find_p1(fx!("이번 ", "TODO", " 는 크다")), None);
    assert_eq!(find_p1(fx!("번 ", "TODO")), None);

    assert_eq!(find_p1("TODO: 40"), None);
    assert_eq!(find_p1("TODO. 40"), None);
    assert_eq!(find_p1("TODO #40"), None);
    assert_eq!(find_p1("TODO_40"), None);
}

/// 대소문자 축 — 모듈 머리말의 "구분자 개수·대소문자로 회피되지 않아야 한다" 가 이
/// 패턴에도 걸린다. 넓힌 쪽이 할 일 표시와 식별자를 안 삼키는지 함께 고정한다.
#[test]
fn p1_is_not_evaded_by_case() {
    assert_eq!(
        find_p1(fx!("see todo", " 40")),
        Some(fx!("todo", " 40").into())
    );
    assert_eq!(find_p1(fx!("ToDo", "-7")), Some(fx!("ToDo", "-7").into()));
    assert_eq!(find_p1(fx!("(todo", "18)")), Some(fx!("todo", "18").into()));
    // 인용문은 **원문 표기**로 돌려준다 — 보고에서 그 자리를 찾을 수 있어야 한다.
    assert_eq!(
        find_p1(fx!("see Todo", " 40")).as_deref(),
        Some(fx!("Todo", " 40"))
    );
    // 넓힌 쪽이 할 일 표시를 삼키지 않는다 — 숫자를 요구하는 것이 그 방벽이다.
    assert_eq!(find_p1("// todo: refactor this later"), None);
    assert_eq!(find_p1("// todo(권한모델): 도입 후 대체"), None);
    assert_eq!(find_p1("let todos = todo_marker_end(line, 4);"), None);
    assert_eq!(find_p1("todo_40"), None);
}

/// `/` 구분자 축 — 티켓 번호는 경로 표기로도 인용된다. 낱말 사이의 `/` 와 가르는 것은
/// **뒤따르는 숫자**이고, 그 음성 둘은 레포에 실재하는 형태다.
#[test]
fn p1_takes_the_slash_as_a_separator() {
    assert_eq!(
        find_p1(fx!("(todo", "/52 R2)")),
        Some(fx!("todo", "/52").into())
    );
    assert_eq!(find_p1(fx!("TODO", "/7")), Some(fx!("TODO", "/7").into()));
    assert_eq!(
        find_p1(fx!("x/todo", "//3.md")),
        Some(fx!("todo", "//3").into())
    );
    // `/` 뒤에 숫자가 없으면 낱말 사이의 `/` 다 — 산문도 콜아웃 종류도 통과한다.
    assert_eq!(find_p1("TODO/changelog 어느 쪽 인용이든"), None);
    assert_eq!(find_p1("note/todo/abstract/quote"), None);
}

#[test]
fn p1_catches_bracketed_ticket_numbers() {
    assert_eq!(
        find_p1(fx!("원 TODO", "(11)는 close 계열을 다뤘다")),
        Some(fx!("TODO", "(11)").into())
    );
    assert_eq!(
        find_p1(fx!("구현 TODO", "(21번)가 정한다")),
        Some(fx!("TODO", "(21번)").into())
    );
    assert_eq!(
        find_p1(fx!("TODO", "[7] 참조")),
        Some(fx!("TODO", "[7]").into())
    );
    // 공백 런·괄호 안쪽 여백으로 회피되지 않는다.
    assert_eq!(
        find_p1(fx!("TODO", "  ( 40 )")),
        Some(fx!("TODO", "  ( 40 )").into())
    );

    // 닫는 괄호가 짝으로 따라오지 않으면 번호 묶음이 아니다 — 평범한 문장이다.
    assert_eq!(find_p1(fx!("TODO", " (2026 년부터 바뀐다")), None);
    assert_eq!(find_p1(fx!("TODO", "(11 참조")), None);
    // 여는 괄호와 닫는 괄호의 종류가 어긋나면 묶음으로 보지 않는다.
    assert_eq!(find_p1(fx!("TODO", "(11]")), None);
    // 괄호 안이 숫자가 아니면 대상이 아니다 — `TODO(name):` 코드 관례가 여기 걸린다.
    assert_eq!(find_p1("TODO(alice): 나중에 고친다"), None);
    assert_eq!(find_p1("TODO(fixme)"), None);
    assert_eq!(find_p1("TODO(v2): 캐시를 뺀다"), None);
    // 숫자 뒤에 다른 글자가 붙으면 번호가 아니다.
    assert_eq!(find_p1("TODO(3rd)"), None);
    assert_eq!(find_p1("TODO(0.1.59)"), None);
}

#[test]
fn p2_catches_conductor_ticket_numbers() {
    assert_eq!(
        find_p2(fx!("todo-conductor", "/12 참조")),
        Some(fx!("todo-conductor", "/12").into())
    );
    assert_eq!(
        find_p2(fx!("TODO-CONDUCTOR", " 3")),
        Some(fx!("TODO-CONDUCTOR", " 3").into())
    );
    // 구분자 런 — 개수로 회피되지 않는다.
    assert_eq!(
        find_p2(fx!("todo-conductor", "//12")),
        Some(fx!("todo-conductor", "//12").into())
    );
    assert_eq!(
        find_p2(fx!("todo-conductor", "  12")),
        Some(fx!("todo-conductor", "  12").into())
    );
    // 번호가 없으면 P2 는 잡지 않는다(폴더 이름 단독 언급은 P6 소관).
    assert_eq!(find_p2("todo-conductor 디렉토리"), None);
    assert_eq!(find_p2("todo-conductor#12"), None);
}

#[test]
fn p3_catches_workspace_subdir_paths() {
    assert_eq!(
        find_p3(fx!("claude", "-workspace/todo", "/3.md")),
        Some(fx!("claude", "-workspace/todo").into())
    );
    // 대소문자·슬래시 개수로 회피되지 않는다.
    assert_eq!(
        find_p3(fx!("claude", "-workspace/Todo", "/3.md")),
        Some(fx!("claude", "-workspace/todo").into())
    );
    assert_eq!(
        find_p3(fx!("claude", "-workspace//todo", "/3.md")),
        Some(fx!("claude", "-workspace/todo").into())
    );
    // 금지 하위가 아니면 P3 는 잡지 않는다(폴더 언급 자체는 P6 소관).
    assert_eq!(find_p3(fx!("claude", "-workspace/temp/x.png")), None);
}

#[test]
fn p4_catches_design_changelog_slug() {
    assert_eq!(
        find_p4(fx!("판정 slug 는 2026-07-03", "-spacing-offgrid 였다")),
        Some(fx!("2026-07-03", "-spacing-offgrid").into())
    );
    // 대문자 표기로 회피되지 않는다.
    assert_eq!(
        find_p4(fx!("2026-07-03", "-Spacing-Offgrid")),
        Some(fx!("2026-07-03", "-Spacing-Offgrid").into())
    );
    // 날짜만으로는 changelog slug 가 아니다.
    assert_eq!(find_p4("Date: 2026-09-04"), None);
    // 더 긴 숫자열의 중간은 연도가 아니다.
    assert_eq!(find_p4("id 120260-07-03-x"), None);
}

#[test]
fn p5_catches_anchor_slug_number() {
    assert!(find_p5(fx!("[링크](x.md#a-todo", "-12-b)")).is_some());
    // 대문자 앵커·하이픈 런으로 회피되지 않는다.
    assert!(find_p5(fx!("[링크](x.md#a-TODO", "-12-b)")).is_some());
    assert!(find_p5(fx!("[링크](x.md#a-todo", "--12)")).is_some());
    // `todo-conductor` 는 뒤가 숫자가 아니라 앵커 번호가 아니다.
    assert_eq!(find_p5("[링크](x.md#todo-conductor-notes)"), None);
    assert_eq!(find_p5(fx!("[링크](x.md#todo", "12)")), None);
    assert_eq!(find_p5("# 평범한 마크다운 제목"), None);
}

#[test]
fn p6_catches_local_workspace_mentions() {
    // 폴더 단독 언급 — P3 가 놓치던 형태.
    assert_eq!(
        find_p6(fx!("산출물은 .", "claude", "-workspace 아래")),
        Some(fx!(".", "claude", "-workspace").into())
    );
    assert_eq!(
        find_p6(fx!("스크린샷은 .", "claude", "-workspace/temp/ 에")),
        Some(fx!(".", "claude", "-workspace").into())
    );
    // 선행 `.` 을 뺀 표기 — 로컬 작업 폴더는 홈에 없으므로 그래도 위반이다.
    assert_eq!(
        find_p6(fx!("claude", "-workspace/temp 에 둔다")),
        Some(fx!("claude", "-workspace").into())
    );
    // 대문자 표기로 회피되지 않는다.
    assert_eq!(
        find_p6(fx!(".", "CLAUDE", "-WORKSPACE/temp")),
        Some(fx!(".", "claude", "-workspace").into())
    );
    // 레포 로컬 지침 폴더 — 상대 표기 변형 포함.
    assert_eq!(
        find_p6(fx!("설정은 .", "claude", "/CLAUDE.md 가 정한다")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("./.", "claude", "/x 를 읽는다")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!(r".\.", "claude", r"\x 를 읽는다")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("폴더는 .", "claude", " 하나뿐")),
        Some(fx!(".", "claude").into())
    );
}

#[test]
fn p6_home_exemption_is_adjacent_not_line_wide() {
    // 사용자 홈의 런타임 경로 — ADR-0105 가 범위 밖으로 확정한 항목.
    assert_eq!(find_p6("~/.claude/settings.json 을 머지한다"), None);
    assert_eq!(find_p6("$HOME/.claude/projects 아래를 훑는다"), None);
    assert_eq!(find_p6("%USERPROFILE%\\.claude\\settings.json"), None);
    assert_eq!(find_p6("Ok(base.home_dir().join(\".claude\"))"), None);
    assert_eq!(find_p6("아니면 홈의 `.claude/projects`."), None);
    assert_eq!(
        find_p6("$CLAUDE_CONFIG_DIR 미설정 시 .claude/projects"),
        None
    );
    // **같은 줄 어딘가의 홈 표기로는 면제되지 않는다.** 줄 전체를 보면 정당한 홈
    // 경로가 하나 있는 줄의 진짜 위반까지 통과한다.
    assert_eq!(
        find_p6(fx!("~/.tasty 와 .", "claude", "/CLAUDE.md 를 비교")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("Homebrew 설치 후 .", "claude", "/CLAUDE.md 수정")),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!(
            "renderHome() 은 .",
            "claude",
            "/settings.json 을 읽는다"
        )),
        Some(fx!(".", "claude").into())
    );
    assert_eq!(
        find_p6(fx!("홈 화면 설정은 .", "claude", "/CLAUDE.md 가 정한다")),
        Some(fx!(".", "claude").into())
    );
}

#[test]
fn p6_allows_identifiers_and_build_outputs() {
    // reverse-DNS plugin id — 경로가 아니다.
    assert_eq!(find_p6("id = \"com.tasty.claude\""), None);
    assert_eq!(find_p6("com.tasty.claude-design 은 제거됐다"), None);
    // 빌드 산출물·생성물 경로 — 범위 밖.
    assert_eq!(
        find_p6("crates/tasty-plugin-claude/tasty-plugin.toml.sig"),
        None
    );
    assert_eq!(find_p6("site/release.json 을 읽는다"), None);
    assert_eq!(find_p6("target/release/tasty-plugin-claude"), None);
}

#[test]
fn p7_catches_prose_todo_but_not_task_markers() {
    // 실제로 샜던 세 형태 — 번호가 없어 P1 이 통과시키던 것들.
    assert_eq!(
        find_p7("//! 이 TODO 는 순수 구조 이관이라"),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("// \"단일 `*` 만 지원\" 이라는 TODO 문서 초기 서술과 달리"),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("/// (conductor-scoped — a separate TODO if needed)."),
        Some("TODO".into())
    );
    // 상류 라이브러리가 자기 소스에 남긴 마커를 *서술* 하는 것도 산문이다.
    assert_eq!(
        find_p7("/// 자기 소스에 `TODO` 로 남겨 두었으므로"),
        Some("TODO".into())
    );
    // 문장 끝·구두점 앞.
    assert_eq!(
        find_p7("// tracked as a separate TODO."),
        Some("TODO".into())
    );
    assert_eq!(
        find_p7("// later TODOs route through the same dispatch"),
        Some("TODO".into())
    );

    // 할 일 표시는 통과 — 콜론이 바로 붙거나, 괄호 묶음 뒤에 콜론이 온다.
    assert_eq!(
        find_p7("// TODO: winit PR 머지 후 공식 버전으로 교체"),
        None
    );
    assert_eq!(
        find_p7("// TODO(권한모델): manifest 권한 도입 후 대체"),
        None
    );
    assert_eq!(find_p7("// TODO(emilk): upstream 이 정한다"), None);
    // 한 줄에 마커와 산문이 함께 있으면 산문 쪽이 잡힌다.
    assert_eq!(
        find_p7("// TODO: 이 TODO 는 아래와 이어진다"),
        Some("TODO".into())
    );

    // 콜론이 없으면 마커가 아니다 — 괄호만으로는 통과하지 못한다.
    assert_eq!(find_p7("// TODO(alice) 나중에"), Some("TODO".into()));
    assert_eq!(find_p7("// TODO 나중에 고친다"), Some("TODO".into()));
    // 소문자는 보지 않는다 — 식별자·영단어로 흔하다.
    assert_eq!(find_p7("let todo = 3; // todo list 를 만든다"), None);
    assert_eq!(find_p7("fn todo_marker() {}"), None);
}

#[test]
fn p9_catches_dotted_plan_coordinates_only() {
    assert_eq!(
        find_p9("refactor (D.3.C.B.1 step 1)"),
        Some("D.3.C.B".into())
    );
    assert_eq!(
        find_p9("계획 D.3.C.G.3.c 의 마지막"),
        Some("D.3.C.G".into())
    );
    // 마디가 더 붙어도 앞 넷으로 집는다.
    assert_eq!(find_p9("D.3.C.B.10.1 을 본다"), Some("D.3.C.B".into()));
    // 숫자만 점으로 이은 것은 버전이지 좌표가 아니다.
    assert_eq!(find_p9("tasty 0.9.31 릴리스"), None);
    assert_eq!(find_p9("1.2.3.4"), None);
    // 넷째 마디가 대문자가 아니면 좌표의 모양이 아니다.
    assert_eq!(find_p9("A.1.B.c 는 아니다"), None);
    // 둘째 마디에 숫자가 없으면 약어의 나열이다.
    assert_eq!(find_p9("U.S.A.B 형식"), None);
    // 앞이 단어 문자면 좌표의 머리가 아니다 — 파일명 안의 조각을 안 집는다.
    assert_eq!(find_p9("xD.3.C.B.1"), None);
}

#[test]
fn p10_catches_round_numbers_only() {
    assert_eq!(find_p10("위 R476 과 같은 부류다"), Some("R476".into()));
    assert_eq!(find_p10("(R56)"), Some("R56".into()));
    assert_eq!(find_p10("R1147 축"), Some("R1147".into()));
    // 한 자리는 데이터 쪽이 압도적이라 안 본다.
    assert_eq!(find_p10("R1 축과 R2 축"), None);
    // 뒤에 단어 문자가 이어지면 식별자의 조각이다.
    assert_eq!(find_p10("let R12x = 1;"), None);
    assert_eq!(find_p10("RGB12_FOO"), None);
    // 앞이 단어 문자면 이것도 식별자의 조각이다.
    assert_eq!(find_p10("VAR12"), None);
    assert_eq!(find_p10("xR476"), None);
    assert_eq!(find_p10("_R476"), None);
    // 소문자는 대상이 아니다.
    assert_eq!(find_p10("r476"), None);
}

#[test]
fn p7_is_out_of_scope_in_markdown_only() {
    let prose = "이 TODO 는 순수 구조 이관이다";
    // 코드·스크립트·CI 설정에서는 잡힌다.
    for rel in [
        "src/core/state/attention.rs",
        "scripts/bench/perf-10-surfaces.sh",
        ".github/workflows/test.yml",
        "site/vendor/gallery/components.jsx",
        "Justfile",
    ] {
        assert!(
            violations_in_line(rel, prose)
                .iter()
                .any(|v| v.starts_with("P7")),
            "P7 이 코드 자리에서 안 잡혔다: {rel}"
        );
    }
    // 마크다운은 범위 밖 — 규칙 본문 인용과 상류 마커 서술이 거기서는 정상이다.
    for rel in [
        "CLAUDE.md",
        "docs/dev-guide/ci-gates.md",
        "site/content/help/troubleshooting.md",
        "crates/tasty-plugin-markdown/assets/NOTICE.md",
    ] {
        assert!(
            violations_in_line(rel, prose).is_empty(),
            "마크다운이 P7 범위에 들어왔다: {rel}"
        );
    }
    // 범위 밖은 P7 하나뿐이다 — 같은 `.md` 에서 다른 패턴은 그대로 잡힌다.
    assert!(
        violations_in_line("docs/dev-guide/ci-gates.md", fx!("see TODO", " 40"))
            .iter()
            .any(|v| v.starts_with("P1"))
    );
}

#[test]
fn scan_target_covers_scripts_ci_and_root_docs() {
    // 예전 화이트리스트가 통째로 놓치던 사각지대.
    assert!(is_scan_target("scripts/bench/perf-10-surfaces.sh"));
    assert!(is_scan_target("CLAUDE.md"));
    assert!(is_scan_target("crates/tasty-design-tokens/README.md"));
    assert!(is_scan_target(".github/workflows/test.yml"));
    assert!(is_scan_target(".githooks/pre-commit"));
    assert!(is_scan_target("Justfile"));
    assert!(is_scan_target("site/content/help/troubleshooting.md"));
    // 바이너리는 제외.
    assert!(!is_scan_target("assets/icon.png"));
    assert!(!is_scan_target(
        "crates/tasty-plugin-claude/tasty-plugin.toml.sig"
    ));
}

/// **모집단이 환경을 읽으면 답도 환경을 읽는다.** worktree 에서 `.git` 은 파일이고
/// 메인 체크아웃에서는 디렉토리다 — 가지치기가 종류를 물으면 앞쪽에서만 그 파일이
/// 모집단에 들어 두 트리가 서로 다른 파일을 본다. 실재하는 레포를 상대로 시험하면
/// 이 회귀가 체크아웃 종류에 따라 조용히 사라지므로 임시 디렉토리로 형태를 짓는다.
#[test]
fn pruning_is_by_name_not_by_kind() {
    let dir = std::env::temp_dir().join(format!("tasty-prune-kind-{}", std::process::id()));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리");
    // worktree 의 형태 — 가지치기 이름을 가진 것이 디렉토리가 아니라 파일이다.
    std::fs::write(dir.join(".git"), "gitdir: elsewhere\n").expect("쓰기");
    std::fs::write(dir.join(".worktree"), "x\n").expect("쓰기");
    std::fs::write(dir.join("keep.md"), "x").expect("쓰기");

    let mut files = Vec::new();
    gather(&dir, &dir, &mut files);
    let mut seen: Vec<String> = files.iter().map(|f| rel_of(f, &dir)).collect();
    seen.sort();
    // 정리 실패는 무시한다 — 임시 디렉토리라 남아도 판정에 영향이 없다.
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        seen,
        vec!["keep.md".to_string()],
        "가지치기가 종류를 물었다 — 가지치기 이름을 가진 파일이 모집단에 들어왔다"
    );
}

/// **이름이 아닌 근거로도 가지치기된다.** 이 절이 없으면 `is_pruned_dir` 이 이름
/// 판정으로 퇴화해도 위 테스트가 전부 초록이라 — 다른 이름의 빌드 디렉토리가 모수에
/// 다시 들어온 것을 아무도 못 본다. 양극성으로 잡는다: 표식이 있으면 걸리고, 이름이
/// 같아 보여도 표식이 없으면 안 걸린다.
#[test]
fn a_build_dir_under_another_name_is_still_pruned() {
    let dir = std::env::temp_dir().join(format!("tasty-prune-{}", std::process::id()));
    // 정리 실패는 무시한다 — 임시 디렉토리라 남아도 판정에 영향이 없고, 여기서
    // 죽으면 진짜 실패가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리");

    assert!(
        !is_pruned_dir(&dir, "target-e2e-headless"),
        "표식이 없으면 이름이 빌드 디렉토리처럼 보여도 가지치기하지 않는다"
    );

    std::fs::write(
        dir.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .expect("표식 쓰기");
    assert!(
        is_pruned_dir(&dir, "target-e2e-headless"),
        "표식이 있으면 이름과 무관하게 가지치기한다"
    );

    // 정리 실패는 무시한다 — 임시 디렉토리라 남아도 판정에 영향이 없고, 여기서
    // 죽으면 진짜 실패가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&dir);
}

/// vendored 목록이 실재와 맞는가. **열거의 값은 여기서 나온다** — 목록이 이름 면제와
/// 다른 것은 "새로 들어온 것을 안 덮는다" 는 점뿐이고, 그것은 목록이 낡지 않을 때만
/// 성립한다.
///
/// 양방향으로 본다. 목록에 있는데 없는 파일(번들이 옮겨졌다)과, 같은 디렉토리에
/// 목록에 없는 minified 번들이 새로 들어온 것.
#[test]
fn the_vendored_list_matches_what_is_there() {
    let root = &tasty_doc_guards::repo_root();
    for rel in VENDORED_FILES {
        assert!(
            root.join(rel).is_file(),
            "vendored 목록이 없는 파일을 가리킨다: {rel} — 번들이 옮겨졌으면 목록도 옮겨라"
        );
    }

    let dir = root.join("crates/tasty-plugin-markdown/assets");
    let mut unlisted = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("vendored 디렉토리를 읽지 못했다") {
        let path = entry.expect("디렉토리 항목").path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.ends_with(".min.js") || name.ends_with(".min.css")) {
            continue;
        }
        let rel = format!("crates/tasty-plugin-markdown/assets/{name}");
        if !VENDORED_FILES.contains(&rel.as_str()) {
            unlisted.push(rel);
        }
    }
    assert!(
        unlisted.is_empty(),
        "목록에 없는 minified 번들이 있다 — 열거를 갱신해라: {unlisted:?}"
    );
}

/// **이름 면제가 덮던 우리 파일들이 이제 스캔된다.** 반대 극성이다 — 위 목록만 있고
/// 이름 면제가 남아 있으면 이 단언이 빨개진다.
#[test]
fn our_own_files_under_assets_are_scanned() {
    for rel in [
        "assets/linux/tasty.desktop",
        "assets/icons/tasty-melon.svg",
        "crates/tasty-plugin-markdown/assets/NOTICE.md",
    ] {
        assert!(
            is_scan_target(rel),
            "우리가 쓴 파일이 스캔 대상에서 빠졌다: {rel}"
        );
        let dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let name = dir.rsplit('/').next().unwrap_or("");
        assert!(
            !is_pruned(name),
            "그 파일이 든 디렉토리가 이름으로 가지치기된다: {dir}"
        );
    }
    // 대조 — vendored 번들은 여전히 빠진다.
    assert!(!is_scan_target(
        "crates/tasty-plugin-markdown/assets/katex.min.js"
    ));
}

#[test]
fn prunes_build_outputs_and_local_dirs_but_not_assets() {
    assert!(is_pruned("target"));
    assert!(is_pruned("node_modules"));
    // vendored 번들은 **디렉토리 이름이 아니라 파일 열거**로 뺀다 — 그 디렉토리에는
    // 우리 파일도 산다(`the_vendored_list_matches_what_is_there` 참조).
    assert!(!is_pruned("assets"));
    // gitignored 로컬 폴더(선행 `.`).
    assert!(is_pruned(fx!(".", "claude")));
    assert!(is_pruned(fx!(".", "claude", "-workspace")));
    // 점 없는 같은 이름은 일반 디렉토리다.
    assert!(!is_pruned(fx!("claude")));
    assert!(!is_pruned("src"));
}

// ── 면제(ALLOWLIST) 를 겨냥한 변이 ──────────────────────────────────────
//
// 면제를 하나 두면 그 면제만큼 구멍이다. 면제 창 **안쪽**에 진짜 위반을 심었을 때
// 잡히는지를 묻는 것이 아래 셋이고, 셋 다 `violations_in_line` 에 합성 입력을 먹인다 —
// 레포에 위반을 심어 보는 방식이 아니라 판정기에 영구히 붙는 형태다.

#[test]
fn allowlist_exempts_only_the_named_pattern_not_the_whole_file() {
    // `CLAUDE.md` 는 P1·P4 만 면제다. 같은 파일에 P3(경로 인용)을 심으면 잡혀야 한다 —
    // 이 단언이 깨지는 형태가 곧 "파일 통째 면제" 로의 회귀다.
    let planted = fx!("claude", "-workspace/todo", "/3.md");
    let found = violations_in_line("CLAUDE.md", planted);
    assert!(
        found.iter().any(|v| v.starts_with("P3")),
        "면제 파일에 심은 비면제 패턴이 통과했다: {found:?}"
    );

    // 면제된 쪽은 그대로 통과한다(면제가 실제로 동작하는지의 반대편).
    assert!(violations_in_line("CLAUDE.md", fx!("see TODO", " 40")).is_empty());
    // 같은 줄이 면제 없는 파일에서는 잡힌다 — 통과가 패턴 고장이 아니라 면제 때문임을 가른다.
    assert!(
        violations_in_line("src/main.rs", fx!("see TODO", " 40"))
            .iter()
            .any(|v| v.starts_with("P1"))
    );

    // 반대 방향 — 이 스크립트는 P7 만 면제이므로 P1 은 잡혀야 한다.
    let script = "scripts/check-allow-reason.sh";
    assert!(
        violations_in_line(script, fx!("(TODO", "18)"))
            .iter()
            .any(|v| v.starts_with("P1"))
    );
}

#[test]
fn allowlist_paths_match_exactly_not_by_prefix_or_suffix() {
    assert!(allowed_patterns("CLAUDE.md").contains(&"P1"));
    // 창은 경로 하나다 — 접두/접미가 겹치는 다른 파일로 새지 않는다.
    assert!(allowed_patterns("docs/CLAUDE.md").is_empty());
    assert!(allowed_patterns("CLAUDE.md.bak").is_empty());
    assert!(allowed_patterns("crates/x/CLAUDE.md").is_empty());
    assert!(allowed_patterns("").is_empty());
    // 면제되지 않은 패턴은 면제 파일에서도 목록에 없다.
    assert!(!allowed_patterns("CLAUDE.md").contains(&"P3"));
    assert!(!allowed_patterns("CLAUDE.md").contains(&"P6"));
}

/// **이 초록이 뜻하는 것은 "면제가 아직 필요하다" 가 아니다.** 면제가 실재하는 파일과
/// 실재하는 패턴 id 를 가리킨다는 것뿐이다 — 참조 무결성이다.
///
/// 그 파일이 더 이상 그 패턴을 담지 않아 면제가 아무 일도 안 하게 된 상태는 여기서
/// 안 잡힌다. 그것을 잡으려면 항목을 빼고 가드를 돌려 빨개지는지 봐야 하고, 그 판정은
/// 가드 안에서 할 수 없다. 재는 절차는 `docs/dev-guide/guard-population.md`.
#[test]
fn allowlist_entries_point_at_things_that_exist() {
    // 경로가 썩으면 가드가 그 파일을 다시 잡아 **시끄럽게** 실패하지만, 패턴 id 가 썩으면
    // (오탈자·패턴 개명) 의도한 면제가 조용히 사라진 채 아무도 모른다. 뒤쪽을 여기서 잡는다.
    let root = &tasty_doc_guards::repo_root();
    let ids: Vec<&str> = PATTERNS.iter().map(|(id, _, _)| *id).collect();
    for (rel, pats) in ALLOWLIST {
        assert!(
            root.join(rel).exists(),
            "면제 항목이 가리키는 파일이 없다 — 옮겼거나 지웠으면 항목도 지워라: {rel}"
        );
        assert!(
            !pats.is_empty(),
            "빈 면제 목록은 항목을 지우라는 뜻이다: {rel}"
        );
        for pat in *pats {
            assert!(
                ids.contains(pat),
                "면제가 없는 패턴 id 를 가리킨다(오탈자·개명): {rel} → {pat}"
            );
        }
    }
    // 목록이 통째로 비면 위 루프가 아무것도 검사하지 않고 초록이 된다.
    //
    // ★ 여기 숫자를 두지 않는다. 전에는 `>= 3` 이었는데 그 3 은 어디서도 안 나온 값이고,
    // **여유가 0 이었다** — 실측 항목 수가 정확히 3 이다(2026-09-07). 문턱과 실측이 같으면
    // 면제를 하나 줄이는 **옳은** 커밋이 그 자리에서 빨개지고, 그때 가장 싼 초록화는
    // **죽은 면제를 하나 남겨 두는 것**이다. 그런데 이 파일의 doc 이 바로 그 상태
    // ("면제가 아무 일도 안 하게 된 상태")를 결함으로 적는다. 즉 문턱이 이 가드가
    // 지키려는 것의 반대를 보상하고 있었다.
    //
    // 이 자리가 물어야 하는 것은 "몇 개인가" 가 아니라 "루프가 돌 것이 있는가" 하나다.
    // 그 물음에는 도출된 답이 있고, 그것을 그대로 적는다 — 실측도 날짜도 필요 없다.
    assert!(
        !ALLOWLIST.is_empty(),
        "면제 목록이 비었다 — 위 루프가 한 번도 안 돌아 이 시험은 아무것도 안 본다. \
         면제가 정말 다 없어졌으면 이 시험도 함께 지워라(빈 명부를 지키는 시험은 \
         초록이 뜻을 잃는다)"
    );
}

/// [`MUST_BE_SCANNED`] 의 판정. 실패문이 **좌변 수를 함께 찍는다** — 갈래가 빠진 것과
/// 순회가 죽은 것을 그 수로 가른다.
#[test]
fn the_scan_reaches_every_major_branch_of_the_tree() {
    let root = &tasty_doc_guards::repo_root();
    let mut files = Vec::new();
    gather(root, root, &mut files);
    let rels: Vec<String> = files.iter().map(|f| rel_of(f, root)).collect();
    let missing: Vec<&str> = MUST_BE_SCANNED
        .iter()
        .copied()
        .filter(|d| !rels.iter().any(|r| r.starts_with(d)))
        .collect();
    assert!(
        missing.is_empty(),
        "스캔이 이 갈래에 한 파일도 안 닿았다: {missing:?} (좌변 {} 개)\n\
         가지치기(`is_pruned` · `is_scan_target` · `VENDORED_FILES`)가 넓어졌거나 순회가 \
         죽었다. 이 가드는 위반 수 0 을 결론으로 내므로, 좌변이 줄면 **더 조용히** 초록이 \
         된다 — 상한을 올려 통과시키듯 이 명부에서 갈래를 빼서 통과시키지 마라.",
        rels.len()
    );
}
