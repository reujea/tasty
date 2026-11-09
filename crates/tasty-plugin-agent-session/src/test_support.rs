//! 시험이 함께 쓰는 plugin 도구 — 가짜 호스트를 든 plugin 과, 배선 시험이 읽는 제품 소스.

use crate::application::plugin::SupervisorPlugin;

/// 시험이 쓰는 plugin — 워커로 옮길 수 있는 가짜 호스트와, 표를 처음 쓸 때 여는 실물 데몬 핸들.
pub(crate) type TestPlugin = SupervisorPlugin<
    crate::application::surface_link::tests_support::SharedHost,
    crate::adapters::driven::local::LocalSupervisor,
>;

impl Default for TestPlugin {
    fn default() -> Self {
        Self::new(crate::adapters::driven::local::LocalSupervisor::new())
    }
}

/// 파일에서 시험 모듈(`#[cfg(test)]` 바로 뒤의 `mod 이름 {`)이 처음 시작하는 자리 — 없으면 끝.
///
/// 선언만 하는 `#[cfg(test)] mod 이름;` 이나 시험 전용 `use` 에서 자르면 그 뒤의 제품 코드가 빠진다.
fn test_modules_start(s: &str) -> usize {
    let mut from = 0;
    while let Some(at) = s[from..].find("#[cfg(test)]\nmod ") {
        let at = from + at;
        let line_end = s[at + 13..].find('\n').map_or(s.len(), |e| at + 13 + e);
        if s[at..line_end].trim_end().ends_with('{') {
            return at;
        }
        from = line_end;
    }
    s.len()
}

/// 탭 · IPC · 워커 결과를 잇는 제품 소스 — 배선 시험이 부르는 자리를 찾는다. 파일마다 시험 모듈 앞까지만 잇는다.
pub(crate) fn wiring_source() -> &'static str {
    static SRC: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SRC.get_or_init(|| {
        [
            include_str!("main.rs"),
            include_str!("application/plugin.rs"),
            include_str!("adapters/driving/navigation.rs"),
            include_str!("application/turn_done.rs"),
            include_str!("adapters/driving/ipc.rs"),
        ]
        .iter()
        // 남은 표지(시험 전용 선언 · `use`)는 지운다 — 배선 시험이 표지에서 자르면 그 뒤가 빠진다.
        .map(|s| s[..test_modules_start(s)].replace("#[cfg(test)]", ""))
        .collect::<Vec<_>>()
        .join("\n")
    })
}

/// 입력 하나를 에이전트에게 보내고 대화에 붙인다(**동기**).
///
/// 회수된 세션이면 그쪽이 자동 재개하므로 여기서 `resume` 을 먼저 부르지
/// 않는다 — 같은 검사를 두 곳에 두면 한쪽이 죽은 조건이 된다.
///
/// ⚠️ **화면 경로는 이것을 안 쓴다**(0.1.23 부터 워커가 맡는다) — 그래서
/// `cfg(test)` 다. 제품에 두면 `-D dead-code` 가 잡고, 그 린트를 끄면
/// 진짜 죽은 코드가 묻힌다.
///
/// 남기는 이유: `begin_turn` → 왕복 → `finish_turn` 이 **한 줄로** 맞물리는지
/// 보는 자리가 있어야 한다. 워커 경로는 스레드라 시험이 그 순서를 못 본다.
pub(crate) fn send<S: crate::ports::supervisor::Supervisor>(
    sup: &S,
    convo: &mut crate::domain::convo::Convo,
    text: &str,
) {
    let Some(session) = convo.session_id.clone() else {
        convo.turns.push(crate::domain::convo::Turn {
            sent: text.to_string(),
            got: String::new(),
            error: Some(crate::domain::i18n::t("sup.err.tab_has_no_session")),
            pending: false,
            at: None,
        });
        return;
    };
    let at = crate::domain::convo::begin_turn(convo, text);
    let got = sup
        .send_input(&session, text)
        .map(|v| crate::domain::convo::answer_of(&v))
        .map_err(|e| e.to_string());
    crate::domain::convo::finish_turn(convo, at, got);
}
