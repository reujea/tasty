//! IPC 라우터의 dispatch 팔이 전부 권한 표에 등재돼 있는지 검증한다.
//!
//! `method_meta()` 가 `None` 을 반환하는 메서드는 plugin/agent 호출자에게
//! `UnknownMethod` 로 거부된다. 라우터에 분기가 있는데 표에 없으면 그 거부가
//! **정책인지 등재 누락인지 구분되지 않는다** — 코드를 읽는 쪽에서도, 나중에
//! 권한을 재검토하는 쪽에서도. local caller 전용으로 두려는 의도라면
//! `local_only()` 로 명시 등재해 의도를 표에 남긴다.
//!
//! 이 가드가 없던 동안 `agent.task_set_result` · `debug.close_workspace` /
//! `debug.switch_workspace` / `debug.switch_tab` · `plugin.upgrade_builtins`
//! 5 종이 형제 메서드가 전부 등재된 상태에서 조용히 빠져 있었다. 정책·근거
//! 본문은 [`docs/dev-guide/api-conventions.md`] · [`docs/dev-guide/debug-ipc.md`].
//!
//! **[`ROUTER_SOURCES`] 에 파일을 빠뜨리면 그만큼 사각지대가 그대로 남는다** —
//! 실제로 `src/app/` 쪽 3 파일이 처음 목록에서 빠져 `plugin.upgrade_builtins`
//! 하나가 가드를 통과했다. 새 dispatch match 를 다른 파일에 만들면 여기에
//! 추가한다.
//!
//! release 빌드에서는 `DEBUG_METHODS` 가 설계상 비어 있어(`debug.*` 는 release
//! IPC 표면에서 완전히 사라진다) 이 대조가 성립하지 않는다. 따라서 debug
//! 빌드에서만 돈다 — `cargo test` 의 기본 프로필이 debug 다. 이 타깃의
//! 자동 실행은 **헤드리스 조합**(`check-headless` 의 전체 스위트)에서만 일어난다
//! (기본 조합 잡은 `--lib --bins` 라 통합 타깃을 못 본다 — `docs/dev-guide/ci-gates.md`).
//! 테스트를 `--release` 로 돌리면 이 파일은 통째로 컴파일에서 빠진다.
#![cfg(debug_assertions)]

use std::collections::BTreeSet;
use std::path::Path;

use tasty_doc_guards::match_arms::{Source, matching_close};
use tasty_ipc::method_meta::method_meta;

/// 라우터 dispatch 팔이 있는 고정 소스([`ROUTER_DIRS`] 로 걷지 않는 위치).
/// 각 파일에서 `"<method>" =>` 팔과 `… .method == "<method>"` 비교를 모두 읽는다.
const ROUTER_SOURCES: &[&str] = &[
    "src/adapters/ipc/handler.rs",
    "src/adapters/ipc/handler/ime.rs",
    "src/adapters/ipc/handler/debug_plugin.rs",
    "src/app/dispatch/list_global.rs",
    "src/boot/headless_dispatch.rs",
];

/// 디렉토리째 걷는 라우터 소스 위치(`*.rs`, 비재귀). `src/app/ipc/` 는 dispatch
/// 스텝이 모여 있어 파일이 늘어도 사람이 [`ROUTER_SOURCES`] 에 손으로 추가하는 걸
/// 잊으면 사각지대가 생긴다 — `window_required.rs` 가 실제로 그렇게 빠져 6 메서드가
/// 가드를 통과했다. 이 디렉토리는 통째로 걸어 새 파일이 자동으로 감시망에 들게 한다.
/// 라우팅을 안 하는 파일(caller_gate·routing)은 매치가 0 이라 무해하다.
const ROUTER_DIRS: &[&str] = &["src/app/ipc"];

/// `    "foo.bar" => ...` 형태의 match 팔에서 메서드 이름만 뽑는다.
///
/// 팔 문법(`"..." =>`)으로 좁히는 게 핵심이다 — `method.starts_with("agent.rate_limit_")`
/// 같은 판정 헬퍼의 문자열 리터럴을 구조적으로 배제하므로 예외 allowlist 가 필요 없다.
fn arm_method(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t.strip_prefix('"')?;
    let (name, after) = rest.split_once('"')?;
    if !after.trim_start().starts_with("=>") {
        return None;
    }
    // 메서드 이름 형태(`ns.method`)만 — 문자열 매칭 팔 전반이 아니라.
    if !is_method_name(name) {
        return None;
    }
    Some(name)
}

/// `… .method == "foo.bar" …` 형태의 비교에서 메서드 이름을 **전부** 뽑는다.
///
/// `arm_method`(`"…" =>`)가 못 보는 `if cmd.request.method == "…"` 라우팅을 잡는다.
/// `||` 로 이어진 다중 비교(`window_required.rs` 의 `is_window_required = … || … == "…"`)도
/// 한 줄에 여러 개가 와도 각각 잡도록 줄 전체를 훑는다. `==` 왼쪽이 `method` 로 끝나는
/// 비교만 받아 무관한 문자열 동치 비교(`kind == "terminal"` 등)를 배제한다.
fn eq_methods(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(pos) = rest.find("== \"") {
        let before = rest[..pos].trim_end();
        let after = &rest[pos + 4..]; // `== "` 다음
        let Some((name, tail)) = after.split_once('"') else {
            break;
        };
        if before.ends_with("method") && is_method_name(name) {
            out.push(name);
        }
        rest = tail;
    }
    out
}

/// 메서드 이름으로 갈래를 치는 `match` 의 팔 이름을 **전부** 뽑는다.
///
/// [`arm_method`] 는 줄이 `"…" =>` 로 시작할 때만 보므로 `"a" | "b" =>` 와 guard 가 붙은
/// `"a" if … =>` 를 통째로 놓친다 — 이 가드의 명제는 부분집합(팔 ⊆ 표)이라 놓친 팔은
/// 실패가 아니라 초록으로 나간다. 그래서 scrutinee 에 `method` 가 든 `match` 마다 공용
/// 판정기(`tasty_doc_guards::match_arms`)로 팔을 떼어 `|` 로 나눈 조각을 다 본다. 판정기가
/// 못 읽는 블록은 여기서 실패한다(건너뛰면 그 안의 팔이 전부 빠진다).
fn match_arm_methods(src: &str) -> Vec<String> {
    let source = Source::new(src);
    let code = source.code.as_str();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(at) = code[from..].find("match ") {
        let at = from + at;
        from = at + "match ".len();
        let word_start = !code[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        let Some(open) = code[from..].find('{').map(|k| from + k) else {
            break;
        };
        let scrutinee = &code[from..open];
        if !word_start || !scrutinee.contains("method") || scrutinee.contains(';') {
            continue;
        }
        let close = matching_close(code, open)
            .unwrap_or_else(|| panic!("{}행의 `match` 가 닫히지 않는다", source.line_of(open)));
        let arms = source
            .match_arms(open..close + 1)
            .unwrap_or_else(|e| panic!("dispatch 팔을 못 읽었다 — {e}"));
        for arm in arms {
            for alt in source.alternatives(&arm.pattern) {
                if let Some(name) = source.plain_string(&alt)
                    && is_method_name(name)
                {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

/// `ns.method` 형태(소문자·`_`·`.`, 점 하나 이상)인지.
fn is_method_name(name: &str) -> bool {
    name.contains('.')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
}

#[test]
fn every_router_arm_is_registered_in_method_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut missing: Vec<String> = Vec::new();
    let mut arm_scanned = 0usize;
    let mut eq_scanned = 0usize;
    let mut judge_scanned = 0usize;
    let mut line_only: Vec<String> = Vec::new();

    // 고정 소스 + 디렉토리째 걷는 소스(`src/app/ipc/*.rs`)를 합친다.
    let mut sources: Vec<String> = ROUTER_SOURCES.iter().map(|s| s.to_string()).collect();
    for dir in ROUTER_DIRS {
        let abs = root.join(dir);
        let entries = std::fs::read_dir(&abs)
            .unwrap_or_else(|e| panic!("라우터 디렉토리를 읽을 수 없다: {dir}: {e}"));
        for entry in entries {
            let entry = entry.expect("디렉토리 엔트리");
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let name = path.file_name().and_then(|n| n.to_str()).expect("파일명");
                sources.push(format!("{dir}/{name}"));
            }
        }
    }
    sources.sort();
    sources.dedup();

    for rel in &sources {
        let path = root.join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("라우터 소스를 읽을 수 없다: {rel}: {e}"));
        let judged_arms = match_arm_methods(&src);
        for name in &judged_arms {
            judge_scanned += 1;
            if method_meta(name).is_none() {
                missing.push(format!("{rel}: {name}"));
            }
        }
        let judged: BTreeSet<&str> = judged_arms.iter().map(String::as_str).collect();
        for line in src.lines() {
            // `"…" =>` 팔과 `… .method == "…"` 비교를 모두 본다.
            if let Some(name) = arm_method(line) {
                arm_scanned += 1;
                if !judged.contains(name) {
                    line_only.push(format!("{rel}: {name}"));
                }
                if method_meta(name).is_none() {
                    missing.push(format!("{rel}: {name}"));
                }
            }
            for name in eq_methods(line) {
                eq_scanned += 1;
                if method_meta(name).is_none() {
                    missing.push(format!("{rel}: {name}"));
                }
            }
        }
    }

    // 하한은 판독기마다 따로 둔다 — 합산하면 한 판독기가 0 건이 돼도 나머지가 하한을
    // 채워 연기 검사가 못 본다(판정기 판독만으로 100 을 넘는다). 세 판독은 서로 겹치므로
    // (평범한 `"…" =>` 팔은 줄 판독과 판정기 판독이 둘 다 센다) 합이 팔 수도 아니다.
    // 실측 2026-09-23(이 테스트의 세 카운터를 `--nocapture` 로 찍어 셈): 줄 팔 311 ·
    // `method ==` 비교 46 · 판정기 팔 321. `method ==` 46 은 `grep -o 'method == "[a-z_.]*"'`
    // 을 위 소스 전부에 돌린 합과 같다(`app_methods.rs` 18 · `window_required.rs` 16 ·
    // `headless_dispatch.rs` 7 · `debug_methods.rs` 4 · `handler.rs` 1). 하한은 그 약 8 할이다.
    assert!(
        arm_scanned >= 250,
        "`\"…\" =>` 줄 팔을 {arm_scanned} 개밖에 못 찾았다(실측 311) — 줄 판독이 깨졌을 가능성이 크다"
    );
    assert!(
        eq_scanned >= 36,
        "`… .method == \"…\"` 비교를 {eq_scanned} 개밖에 못 찾았다(실측 46) — 비교 판독이 깨졌을 가능성이 크다"
    );
    assert!(
        judge_scanned >= 250,
        "판정기로 읽은 dispatch 팔을 {judge_scanned} 개밖에 못 찾았다(실측 321) — 판정기 판독이 깨졌을 가능성이 크다"
    );
    // 위 하한은 판독기가 **통째로** 죽는 것만 잡는다 — 파일 하나만 못 읽게 되면 나머지가
    // 하한을 채운다. 그래서 줄 판독이 본 팔은 전부 같은 파일의 판정기 판독도 봐야 한다는
    // 포함관계를 따로 단정한다: 판정기 판독이 파일 하나만큼 죽으면 그 파일의 줄 팔이 여기로
    // 떨어진다. 반대쪽(줄 판독이 파일 하나만큼 죽는 것)은 이 단정이 못 보지만, 그때도 그
    // 팔은 판정기 판독이 대조하므로 표 대조에서 빠지는 팔은 없다 — 이 문장은 구조적 보장이
    // 아니라 조건부다: 작업 트리에서 줄 팔 ⊆ 판정기 팔이 성립하기 때문에 참이고, 그 포함이
    // 깨지는 트리에서는 위 단정이 먼저 빨개진다. `method ==` 비교 판독의 파일 단위 부분 사망은
    // 이 단정 밖이다(판정기 판독과 겹치는 원소가 없다). 판정기 판독이 **한 모양만** 죽는 파일
    // 안 부분 사망(`|` 로 이은 조각이나 guard 붙은 팔만 못 읽는 것)은 이 단정이 **언제나** 못
    // 본다 — 줄 판독([`arm_method`])은 닫는 따옴표 바로 뒤가 `=>` 인 팔만 읽어 그 두 모양을
    // 애초에 안 읽으므로, 그 모양만 죽어도 줄 판독 쪽에 판정기가 놓친 원소가 생기지 않는다.
    // 관찰(2026-09-23 작업 트리): 라우터 소스에 메서드 이름 guard 팔(`"x.y" if`)은 0 건이라
    // 그 모양은 지금 죽을 팔이 없고, `|` 모양만 죽으면 표 대조에서까지 빠지는 이름은 `|` 로만
    // 나오는 `plugin.enable` · `plugin.disable` 둘이다(나머지 `|` 이름은 다른 자리에 평범한
    // 팔로도 나온다). 하한이나 기대 원소를 더하지 않는 것은 라우터 표기가 바뀔 때마다 그 값을
    // 옮기게 되기 때문이다.
    assert!(
        line_only.is_empty(),
        "줄 판독이 본 `\"…\" =>` 팔을 같은 파일의 판정기 판독이 못 봤다 — 판정기 판독이 그 \
         파일에서 깨졌거나, 그 파일에 `method` 를 안 보는 `match` 가 생겨 그 팔 이름이 줄 \
         판독에만 잡혔을 가능성이 크다:\n  {}",
        line_only.join("\n  ")
    );
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "라우터에 분기가 있는데 METHOD_TABLE/DEBUG_METHODS/PREFIX_RULES 어디에도 \
         등재되지 않은 메서드가 있다. plugin 에 열 것이면 plugin(&[..]) 으로, \
         local caller 전용으로 둘 것이면 local_only() 로 **명시 등재**하라 \
         (미등재는 UnknownMethod 거부라 의도와 구분되지 않는다):\n  {}",
        missing.join("\n  ")
    );
}
