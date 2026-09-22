//! 액션 문(`dispatch_action_by_id`)에 누가 들어오는가를 명부로 붙든다.
//!
//! ## 무엇을 지키는가
//!
//! `src/adapters/ui/input/shortcuts/dispatch.rs` 의 `dispatch_action_by_id` 는 액션 id
//! (`KeybindingSettings` 의 필드 이름 그대로)를 받아 단축키와 같은 효과를 내는 **평평한
//! 문**이다. 문이 하나라 새 호출자가 붙기 쉽고, 붙는 순간 그 호출자는 사용자 액션 전부에
//! 한 번에 닿는다.
//!
//! 그래서 이 가드는 **호출자 집합**을 명부와 집합 동등으로 묶는다. 새 문이 생기면 빨개지고,
//! 그때 사람이 `docs/identity.md` 의 원칙 1(사용자 행동 ↔ 에이전트 행동 분리)·원칙 3
//! (포커스 독립성)과 대조한다. **그 대조는 이 가드가 안 한다.**
//!
//! ## 두 홉을 다 본다
//!
//! 직접 호출자만 보면 새는 자리가 있다. 오늘의 유일한 호출자는 팔레트의 지연 실행
//! (`dispatch_pending_command_palette`)이고, 그것은 `state.command_palette.pending_run`
//! 에서 꺼낸다. 즉 **`pending_run` 에 쓰는 자리도 이 문의 입구다.** 앞 홉만 붙들면 새
//! 기입자가 조용히 들어온다.
//!
//! ## 이 가드가 **안 묻는 것** — 원칙 2.2 부채
//!
//! 원칙 2.2 는 "에이전트 기능인데 GUI 에만 있는 것" 을 금지한다. 그 물음은 **액션마다
//! 답이 다르고**(`focus_pane_next` 는 원칙 3 이 에이전트 쪽에 없기를 **요구**한다),
//! 어떤 액션이 에이전트 기능인지는 측정이 아니라 **소유자의 판정**이다. 그래서 여기서는
//! 안 묻는다 — 모수만 도출해 두고 판정 칸은 비운다(아래 `the_action_set_is_derived_not_written`).
//!
//! ⇒ 2026-09-07 현재: **모수 71, 판정 0.** 이 수를 "부채 0" 으로 읽지 마라. 안 센 것이다.
//!
//! ## 문이 액션 전부를 열지는 않는다
//!
//! "단일 진입점" 이라는 말이 "모든 액션이 이 문을 지난다" 로 읽히기 쉬운데 아니다.
//! 실측 2026-09-07: 필드 71 · 문의 arm 42. 나머지는 다른 자리에서 처리된다(확대/축소는
//! `zoom.rs`, 복사/붙여넣기는 `copy_paste.rs`, modifier·슬롯 키 필드는 애초에 액션이 아니다).
//! 그래서 이 가드는 `arms ⊆ fields` 만 단정한다 — 반대 방향은 참이 아니고, 참이어야 할
//! 이유도 없다.

// 이유: 이 파일은 합성 트리를 만들어 걷기와 술어를 재는 양성 대조를 갖는다. 그 정리
// 코드(`let _ = remove_dir_all`)는 실패해도 할 일이 없다 — 이전 실행 잔여물이 없으면
// `NotFound` 가 정상 경로다. 그리고 전수 가드
// (`crates/tasty-doc-guards/tests/let_underscore_documented.rs`)는 테스트 본문을
// 제외하므로 여기서 나는 경고는 정책상 조치 대상이 아니다 —
// `docs/dev-guide/error-handling.md`.
#![allow(clippy::let_underscore_must_use)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use tasty_doc_guards::match_arms::{Source, matching_close};
use tasty_doc_guards::source_text::{mask_non_code, rust_sources};
use tasty_doc_guards::temp_scratch::Scratch;

const DOOR: &str = "dispatch_action_by_id";
const DOOR_FILE: &str = "src/adapters/ui/input/shortcuts/dispatch.rs";
/// 슬롯에 **쓰는** 모양만 센다. `pending_run.take()` 는 읽기이고, 읽는 자리는 이미
/// 문의 호출자로 붙들려 있다 — 같은 자리를 두 명부에 넣으면 한쪽을 고칠 때 다른 쪽이
/// 조용히 낡는다.
const PALETTE_SLOT: &str = "pending_run =";
const KEYBINDINGS: &str = "crates/tasty-settings/src/keybindings.rs";

/// 문을 직접 부르는 자리 — (파일, 사유).
const DOOR_CALLERS: &[(&str, &str)] = &[(
    "src/view/main/redraw.rs",
    "Command Palette 의 지연 실행. 팝업이 닫힌 뒤에 액션을 쏘려고 한 프레임 미룬 자리이고, \
     들어오는 값은 `state.command_palette.pending_run` 에서 꺼낸 것뿐이다",
)];

/// 팔레트 슬롯에 **쓰는** 자리 — 문의 둘째 입구. (파일, 사유).
const SLOT_WRITERS: &[(&str, &str)] = &[(
    "src/adapters/ui/popup/command_palette.rs",
    "팝업에서 Enter 로 고른 항목을 슬롯에 넣는다. 사람이 팝업을 열어 고른 것이라 \
     사용자 입력 경로이고, release 에서 에이전트는 팝업을 강제로 못 연다(원칙 1)",
)];

/// 걷기가 죽으면 아래 집합 동등이 양쪽 빈 채로 성립한다.
const MIN_SOURCES: usize = 300;
/// 액션 필드 하한. 2026-09-07 실측 71.
const MIN_FIELDS: usize = 50;
/// 문의 arm 하한. 2026-09-07 실측 42.
const MIN_ARMS: usize = 30;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn sources() -> Vec<(PathBuf, String)> {
    sources_under(&repo_root(), MIN_SOURCES)
}

/// 위 걷기의 알맹이 — **뿌리와 하한을 인자로 받는다.**
///
/// 정본 값에 묶어 두면 이 걷기와 아래 술어를 합성 트리로 잴 길이 없다. 하한은 모수의
/// 성질이라 자리마다 다르다 — 레포의 `src` 파일 수와 합성 트리의 파일 수는 애초에 다른
/// 모수인데, 상수 하나가 둘을 다 판정하려 들면 둘 중 하나는 반드시 틀린다.
fn sources_under(root: &std::path::Path, floor: usize) -> Vec<(PathBuf, String)> {
    let out = rust_sources(root, &["src"]);
    assert!(
        out.len() >= floor,
        "`src` 에서 .rs 를 {} 개만 걷었다(하한 {floor}) — 걷기가 깨졌다. 모수가 비면 아래 \
         집합 동등은 양쪽이 비어 공짜로 성립한다",
        out.len()
    );
    out
}

/// `needle` 을 **코드에서** 쓰는 파일들 — 자기 정의 파일은 뺀다.
///
/// ★ **주석·문자열을 덮은 사본에 대고 묻는다.** 원문에 걸면 그 이름을 **설명하는 주석**이
/// 호출자로 세어진다. 실측으로 밟았다(2026-09-07): 원문 술어는 `src/view/main/keyboard.rs`
/// 를 문의 호출자로, `src/view/main/redraw.rs` 를 슬롯의 기입자로 셌는데 **둘 다 doc
/// 주석**이었다. 이 저장소는 그 답을 이미 갖고 있다 — [`mask_non_code`].
fn code_uses(needle: &str, own: &str) -> BTreeSet<String> {
    code_uses_in(&sources(), needle, own)
}

/// 위 술어의 알맹이 — **모수를 인자로 받는다.** 같은 이유다.
fn code_uses_in(sources: &[(PathBuf, String)], needle: &str, own: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (rel, text) in sources {
        let rel = rel.to_string_lossy().into_owned();
        if rel == own {
            continue;
        }
        if mask_non_code(text).contains(needle) {
            out.insert(rel);
        }
    }
    out
}

fn check(found: BTreeSet<String>, roster: &[(&str, &str)], what: &str, why_it_matters: &str) {
    let listed: BTreeSet<String> = roster.iter().map(|(p, _)| (*p).to_string()).collect();
    let extra: Vec<&String> = found.difference(&listed).collect();
    let stale: Vec<&String> = listed.difference(&found).collect();
    assert!(
        extra.is_empty(),
        "{what} 에 명부에 없는 자리가 생겼다: {extra:?}\n{why_it_matters}\n\
         ★ 명부에 줄을 더해서 통과시키기 전에 `docs/identity.md` 원칙 1(사용자 행동 ↔ \
         에이전트 행동 분리)과 원칙 3(포커스 독립성)에 대고 읽어라. 그 대조는 이 가드가 \
         안 한다 — 여기서 멈추는 것이 그 대조를 하라는 뜻이다."
    );
    assert!(
        stale.is_empty(),
        "명부에 있는데 트리에 없는 {what} 자리다: {stale:?}\n\
         사라진 자리의 잔재는 다음 사람이 계속 검토하게 만든다 — 지워라."
    );
}

#[test]
fn the_action_door_has_only_the_registered_callers() {
    check(
        code_uses(DOOR, DOOR_FILE),
        DOOR_CALLERS,
        "액션 문의 호출자",
        "이 문은 액션 id 하나로 사용자 액션 전부에 닿는다. 새 호출자는 그 전부를 한 번에 \
         얻는다 — 그것이 에이전트 경로면 원칙 1 이 금지하는 '사용자 입력 재현' 이다.",
    );
}

#[test]
fn the_palette_slot_has_only_the_registered_writers() {
    check(
        code_uses(PALETTE_SLOT, "src/state/command_palette.rs"),
        SLOT_WRITERS,
        "팔레트 슬롯의 기입자",
        "슬롯에 쓰면 다음 프레임에 문이 열린다. 직접 호출자만 붙들면 이 둘째 입구로 \
         조용히 들어온다.",
    );
}

/// 액션 이름은 **도출한다 — 손으로 안 쓴다.**
///
/// 명부에 71 을 적으면 필드가 늘 때마다 낡는다(ADR-0139). 이름은 `KeybindingSettings`
/// 에서 읽고, 여기서 단정하는 것은 **관계**뿐이다: 문의 arm 은 전부 실재하는 액션 필드다.
///
/// 반대 방향(`fields ⊆ arms`)은 **단정하지 않는다.** 29 개 필드가 다른 자리에서 처리되고
/// (`zoom.rs` · `copy_paste.rs`), modifier·슬롯 키 필드는 애초에 액션이 아니다.
#[test]
fn the_action_set_is_derived_not_written() {
    let root = repo_root();
    let fields = keybinding_fields(&root);
    let arms = door_arms(&root);
    assert!(
        fields.len() >= MIN_FIELDS,
        "`KeybindingSettings` 필드를 {} 개만 뽑았다 (2026-09-07 실측 71) — 추출이 깨졌다",
        fields.len()
    );
    assert!(
        arms.len() >= MIN_ARMS,
        "액션 문의 arm 을 {} 개만 뽑았다 (2026-09-07 실측 42) — 추출이 깨졌다",
        arms.len()
    );
    let unknown: Vec<&String> = arms.difference(&fields).collect();
    assert!(
        unknown.is_empty(),
        "액션 문에 `KeybindingSettings` 에 없는 id 가 있다: {unknown:?}\n\
         그 arm 은 어떤 단축키로도 안 불리고 설정에도 안 나온다 — 오타이거나, 필드를 \
         지우면서 arm 을 안 지운 것이다."
    );
}

fn keybinding_fields(root: &std::path::Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(root.join(KEYBINDINGS))
        .unwrap_or_else(|e| panic!("{KEYBINDINGS} 를 읽지 못했다 — {e}"));
    let body = brace_body(&text, "pub struct KeybindingSettings")
        .expect("`KeybindingSettings` 선언을 못 찾았다 — 구조가 바뀌었으면 이 가드를 옮겨라");
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("pub ")
            && let Some(name) = rest.split(':').next()
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            out.insert(name.to_string());
        }
    }
    out
}

/// 액션 문의 `match action_id` 팔 이름.
///
/// 본문과 팔의 경계는 공용 판정기(`tasty_doc_guards::match_arms`)가 주석·문자열·문자 리터럴을
/// 덮은 사본에서 찾는다 — 손으로 센 중괄호는 본문 문자열의 `}`(format escape `"}}"` 가 평범한
/// 예다)에서 `match` 를 일찍 닫아 그 뒤의 팔을 조용히 놓쳤다. 이 가드의 명제는 부분집합(팔 ⊆
/// 필드)이라 놓친 팔은 실패가 아니라 초록으로 나간다. `fn dispatch_action_by_id` 정의가 `#[cfg]`
/// 로 둘 이상이면 전부 읽는다.
fn door_arms(root: &std::path::Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(root.join(DOOR_FILE))
        .unwrap_or_else(|e| panic!("{DOOR_FILE} 를 읽지 못했다 — {e}"));
    let src = Source::new(&text);
    let bodies = src.fn_bodies("dispatch_action_by_id");
    assert!(
        !bodies.is_empty(),
        "액션 문을 못 찾았다 — 이름이 바뀌었으면 이 가드도 함께 옮긴다"
    );
    let mut out = BTreeSet::new();
    for body in bodies {
        let block = src
            .code_slice(&body)
            .find("match action_id")
            .and_then(|k| {
                src.code[body.start + k..]
                    .find('{')
                    .map(|o| body.start + k + o)
            })
            .and_then(|open| matching_close(&src.code, open).map(|close| open..close + 1))
            .expect("`match action_id` 를 못 찾았다 — 디스패치 모양이 바뀌었다");
        let arms = src
            .match_arms(block)
            .unwrap_or_else(|e| panic!("액션 문의 팔을 못 읽었다 — {e}"));
        for arm in arms {
            for alt in src.alternatives(&arm.pattern) {
                if let Some(name) = src.plain_string(&alt)
                    && !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                {
                    out.insert(name.to_string());
                }
            }
        }
    }
    out
}

/// `marker` 뒤 첫 `{` 부터 짝이 맞는 `}` 까지 — 짝은 주석·리터럴을 덮은 사본에서 센다.
fn brace_body<'a>(src: &'a str, marker: &str) -> Option<&'a str> {
    let source = Source::new(src);
    let at = source.code.find(marker)?;
    let open = at + source.code[at..].find('{')?;
    let close = matching_close(&source.code, open)?;
    Some(&src[open + 1..close])
}

#[test]
fn every_registered_place_carries_a_reason() {
    let thin: Vec<&str> = DOOR_CALLERS
        .iter()
        .chain(SLOT_WRITERS.iter())
        .filter(|(_, why)| why.split_whitespace().count() < 8)
        .map(|(p, _)| *p)
        .collect();
    assert!(
        thin.is_empty(),
        "사유가 너무 짧다 — 다음 사람이 그 자리가 왜 정당한지 **재현**할 수 있어야 한다: {thin:?}"
    );
}

/// **양성 대조 — 걷기와 술어를 합성 트리로 건다.**
///
/// 이 가드의 수치 레버 셋(`MIN_SOURCES` · `MIN_FIELDS` · `MIN_ARMS`)은 전부 하한이라
/// **좁아지는 쪽만** 본다. 여기서 위험한 것은 반대쪽이다 — 술어가 넓어지면 명부에 없는
/// 자리가 `extra` 로 올라오고, 그 실패문은 "명부에 줄을 더하거나 그 호출을 없애라" 는
/// 처방을 낸다. 넓어진 술어가 짚은 자리는 **애초에 호출자가 아니므로** 그 처방은 없는
/// 위반에 대한 것이다. 이 저장소는 그 오탐을 실제로 밟았다(2026-09-07, doc 주석 둘).
///
/// ★ 바늘은 합성이다 — `DOOR`·`PALETTE_SLOT` 의 *값*을 안 쓴다. 그래서 이 대조는
/// 그 상수가 무엇인지가 아니라 **주석·문자열을 덮고 센다는 사실**을 잰다.
#[test]
fn the_predicate_counts_code_and_not_prose_on_a_substituted_tree() {
    let probe = Scratch::new("action-door-reader");
    let dir = probe.path();
    std::fs::create_dir_all(dir.join("src/nested")).expect("합성 트리를 만들지 못했다");
    // 걷기가 `src` 밖으로 안 나가는지 보려고 형제 디렉토리를 하나 둔다.
    std::fs::create_dir_all(dir.join("crates")).expect("합성 형제 트리를 만들지 못했다");

    let needle = "zeta_probe_call";
    // 진짜 호출 — 세어야 한다.
    std::fs::write(
        dir.join("src/real_caller.rs"),
        "fn go() {\n    zeta_probe_call(1);\n}\n",
    )
    .expect("합성 호출자 실패");
    // 하위 디렉토리의 진짜 호출 — 걷기가 내려가는지 함께 본다.
    std::fs::write(
        dir.join("src/nested/deep_caller.rs"),
        "fn deep() {\n    zeta_probe_call(2);\n}\n",
    )
    .expect("합성 하위 호출자 실패");
    // **주석에만** 있다 — 세면 안 된다. 이것이 2026-09-07 에 실제로 났던 오탐이다.
    std::fs::write(
        dir.join("src/only_comment.rs"),
        "/// zeta_probe_call 이 무엇인지 설명하는 doc 주석이다.\nfn unrelated() {}\n",
    )
    .expect("합성 주석 파일 실패");
    // **문자열 리터럴에만** 있다 — 역시 세면 안 된다.
    std::fs::write(
        dir.join("src/only_string.rs"),
        "fn name() -> &'static str {\n    \"zeta_probe_call\"\n}\n",
    )
    .expect("합성 문자열 파일 실패");
    // 정의가 사는 파일 — `own` 으로 넘겨서 뺀다.
    std::fs::write(
        dir.join("src/own_def.rs"),
        "pub fn zeta_probe_call(n: u8) {\n    let _ = n;\n}\n",
    )
    .expect("합성 정의 파일 실패");
    // `src` 밖 — 걷기에 안 들어와야 한다.
    std::fs::write(
        dir.join("crates/outside.rs"),
        "fn out() {\n    zeta_probe_call(9);\n}\n",
    )
    .expect("합성 바깥 파일 실패");

    // 하한은 이 합성 트리의 성질로 준다 — 정본 `MIN_SOURCES` 는 다른 모수의 것이다.
    let sources = sources_under(dir, 5);
    let rels: BTreeSet<String> = sources
        .iter()
        .map(|(p, _)| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        !rels.iter().any(|r| r.starts_with("crates/")),
        "걷기가 `src` 밖으로 나갔다 — 모수가 넓어졌다"
    );
    assert!(
        rels.contains("src/nested/deep_caller.rs"),
        "걷기가 하위 디렉토리로 안 내려갔다"
    );

    let found = code_uses_in(&sources, needle, "src/own_def.rs");
    assert_eq!(
        found,
        ["src/nested/deep_caller.rs", "src/real_caller.rs"]
            .iter()
            .map(|s| (*s).to_string())
            .collect::<BTreeSet<String>>(),
        "술어가 합성 트리에서 다른 답을 냈다"
    );
    assert!(
        !found.contains("src/only_comment.rs"),
        "주석에만 있는 이름을 호출자로 셌다 — 그 자리의 처방(\"호출을 없애거나 명부에 \
         등록해라\")은 주석에 대해 참이 아니다"
    );
    assert!(
        !found.contains("src/only_string.rs"),
        "문자열 리터럴에만 있는 이름을 호출자로 셌다 — 같은 이유다"
    );
    assert!(
        !found.contains("src/own_def.rs"),
        "정의가 사는 파일을 자기 호출자로 셌다"
    );
}
