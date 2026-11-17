//! `tasty.tree()`는 마지막 발행값을 읽고, 메인은 진행 중 job이 없으면 트리를 발행하지 않는다.
//! 그래서 Lua job은 송신 직전에 트리를 발행하는 `LuaSend::engine()`을 거쳐 보내야 한다.
//! 엔진을 직접 꺼내 job을 보내면 새 job이 오래전 트리를 읽는다.
//!
//! 이 검사는 `src/`의 제품 코드(시험 파일과 파일 끝 `#[cfg(test)] mod … {` 뒤를 뺀 범위)에서 다음을 본다.
//! - `lua_engine` 필드를 쓰는 파일이 허용 목록 안에 있다(새 송신 지점이 엔진을 직접 꺼내지 않는다).
//! - 엔진 전용 송신 메서드는 `.engine().`을 거치거나, `hooks::lua::fire`가 넘긴 엔진만 쓰는 자동실행 파일에 있다.
//! - 자동실행 송신은 `hooks::lua::fire`에서만 부른다.
//! - about_to_wait는 진행 중 job이 있을 때만 갱신하는 함수만 부르고 직접 발행하지 않는다.
//!
//! 허용 파일 안의 새 직접 송신, 이름이 다른 송신 메서드, 실제 실행 순서는 보지 않는다.
//! 송신 직전 발행의 동작은 `hooks::lua`와 `dispatch::lua_commands` 시험이 잰다.

use super::{fn_body, mask_non_code, repo_root};

/// `lua_engine` 필드를 쓰는 제품 파일. 송신이 아닌 쓰임(필드 선언, 결과·명령 회수, 깨우기 등록,
/// headless 통로)이거나 `lua_send!`를 정의하는 파일이다.
const ENGINE_FIELD_FILES: &[&str] = &[
    "src/app.rs",
    "src/app/boot_machine.rs",
    "src/app/dispatch/lua_commands.rs",
    "src/boot.rs",
    "src/boot/headless_dispatch.rs",
];
/// `LuaEngine`에만 있는 송신 메서드. `fire`는 다른 타입에도 있어 이름으로 고를 수 없다.
const SEND_METHODS: &[&str] = &[".submit_script(", ".run_script(", ".reset_hooks("];
const AUTOFIRE: &str = "src/host_api/hooks/autofire.rs";
const AUTOFIRE_DISPATCH: &str = "autofire::dispatch(";
const HOOKS_LUA: &str = "src/host_api/hooks/lua.rs";

fn production_sources() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (path, text) in tasty_doc_guards::source_text::rust_sources(&repo_root(), &["src"]) {
        let rel = path.to_string_lossy().replace('\\', "/");
        if rel.ends_with("_tests.rs") || rel.ends_with("/tests.rs") || rel.contains("/tests/") {
            continue;
        }
        out.push((rel, without_test_module(&mask_non_code(&text))));
    }
    assert!(out.len() > 100, "src 제품 파일을 {}개만 읽었다", out.len());
    out
}

/// 파일 안에 정의한 시험 모듈 뒤를 잘라 낸다. `#[path]`로 읽는 시험 파일은 이름으로 따로 뺀다.
fn without_test_module(masked: &str) -> String {
    let mut from = 0;
    while let Some(at) = masked[from..].find("#[cfg(test)]") {
        let at = from + at;
        let rest = masked[at + "#[cfg(test)]".len()..].trim_start();
        if let Some(after_mod) = rest.strip_prefix("mod ")
            && after_mod
                .split_once(|c: char| !(c.is_alphanumeric() || c == '_'))
                .is_some_and(|(_, tail)| tail.trim_start().starts_with('{'))
        {
            return masked[..at].to_string();
        }
        from = at + 1;
    }
    masked.to_string()
}

/// 송신 메서드를 엔진에 직접 부르는 줄. `.engine().`을 거친 줄은 뺀다.
fn direct_sends(code: &str) -> Vec<String> {
    code.lines()
        .filter(|line| SEND_METHODS.iter().any(|m| line.contains(m)))
        .filter(|line| !line.contains(".engine()."))
        .map(|line| line.trim().to_string())
        .collect()
}

#[test]
fn lua_jobs_are_sent_through_the_publishing_path() {
    let sources = production_sources();
    let mut field_users: Vec<&str> = sources
        .iter()
        .filter(|(_, code)| code.contains("lua_engine"))
        .map(|(rel, _)| rel.as_str())
        .collect();
    field_users.sort_unstable();
    assert_eq!(
        field_users, ENGINE_FIELD_FILES,
        "`lua_engine`을 쓰는 파일이 바뀌었다. 새 송신 지점이면 `lua_send!`로 보내고, 송신이 아니면 목록에 이유와 함께 더한다."
    );
    let mut violations = Vec::new();
    for (rel, code) in &sources {
        if rel != AUTOFIRE {
            violations.extend(
                direct_sends(code)
                    .into_iter()
                    .map(|l| format!("{rel}: {l}")),
            );
        }
        if rel != HOOKS_LUA && code.contains(AUTOFIRE_DISPATCH) {
            violations.push(format!("{rel}: {AUTOFIRE_DISPATCH}"));
        }
    }
    assert!(
        violations.is_empty(),
        "Lua job을 송신 직전 발행 없이 보낸다: {violations:#?}"
    );
}

#[test]
fn the_event_loop_publishes_only_for_pending_jobs() {
    let src = std::fs::read_to_string(repo_root().join("src/app/event_handler.rs")).unwrap();
    let body = mask_non_code(&fn_body(&src, "fn about_to_wait").expect("about_to_wait 본문"));
    assert!(body.contains("self.refresh_lua_snapshot_for_pending_jobs()"));
    for direct in ["publish_snapshot(", "lua_tree("] {
        assert!(
            !body.contains(direct),
            "about_to_wait가 진행 중 job과 관계없이 `{direct}`로 발행한다"
        );
    }
}

#[test]
fn the_judgment_flags_a_direct_send() {
    assert_eq!(
        direct_sends(
            "let r = engine.submit_script(s, None, None);\nlua.engine().run_script(s, None);"
        ),
        ["let r = engine.submit_script(s, None, None);"]
    );
    let cut =
        without_test_module("fn a() {}\n#[cfg(test)]\nmod tests {\n engine.run_script(x);\n}");
    assert!(direct_sends(&cut).is_empty());
    let kept = without_test_module("#[cfg(test)]\nmod probe;\nfn a() { engine.run_script(x); }");
    assert_eq!(direct_sends(&kept).len(), 1);
}
