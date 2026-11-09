//! 사람이 적은 세션 id 를 **데몬이 아는 실제 id** 로 푼다.
//!
//! # 왜 따로 있나
//!
//! 탭을 만들 때 `session_id` 칸에 적은 값은 종전에 확인 없이 그대로 썼다. 그래서
//! 시작 폼·목록이 보여 주는 앞 8자를 적으면 데몬에 없는 id 로 대화를 시도했고,
//! 전체 id 를 적어도 surface 에 잇지 않아(`surface_link::link`) CLI·재시작이 그
//! 탭을 세션 없는 탭으로 봤다(실측 2026-10-12). 탭 만들기와 `agent-session link-add`
//! 가 같은 물음("이 입력은 어느 세션인가")에 답하므로 판정을 여기 하나로 둔다.
//!
//! 전체 일치가 우선이고, 없으면 **하나뿐인** 앞부분 일치를 받는다. 둘 이상이면
//! 고르지 않는다 — 잘못 고른 세션에 말을 거는 것이 거절보다 나쁘다.

use crate::ports::host::HostCall;
use crate::ports::supervisor::{ClientError, SessionRow};

/// `input` 이 가리키는 세션의 전체 id. 못 정하면 사람에게 보일 사유.
pub fn resolve(rows: &[SessionRow], input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(crate::domain::i18n::t("sup.err.need_session_id_full"));
    }
    if let Some(r) = rows.iter().find(|r| r.id == input) {
        return Ok(r.id.clone());
    }
    let hits: Vec<&SessionRow> = rows.iter().filter(|r| r.id.starts_with(input)).collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(crate::domain::i18n::t1("sup.err.no_such_session", input)),
        many => Err(crate::domain::i18n::tn(
            "sup.err.ambiguous_session",
            &[input, &many.len().to_string()],
        )),
    }
}

/// 이어받기(`adopt`) 칸의 입력을 이미 있는 supervisor 기록과 대조한 결과.
#[derive(Debug, PartialEq, Eq)]
pub enum Adopt {
    /// 그 대화를 가진 기록이 없다 — 데몬에 이어받기를 맡긴다.
    New,
    /// 그 대화를 가진 기록이 있다 — 새로 띄우지 않고 **그 supervisor id 로** 잇는다. 연결돼 있으면
    /// 그 기록에 합류하고, 끊겨 있으면 데몬이 다음 입력에서 되살린다.
    Reuse { id: String, cwd: Option<String> },
}

/// 데몬에 붙은 자식 프로세스가 있다고 기록이 말하는 상태 — 그런 기록은 이미 연결돼 있다.
const CONNECTED: [&str; 2] = ["starting", "running"];

/// 이어받기 칸의 `input` 이 이미 기록에 있는가.
///
/// # 왜 있나
///
/// 시작 화면의 세션 id 칸은 종전에 확인 없이 `session.spawn` 의 `adopt` 로 갔다. 데몬은
/// 이어받기마다 supervisor 기록을 새로 만들므로, 같은 Claude 대화를 두 번 이어받으면 기록이
/// 둘이 되고(실측 2026-10-20: `625fccaa`·`f2e706b0` 가 같은 대화를 가리켰다) 둘이 함께 돌면
/// 한 대화 기록에 두 프로세스가 쓴다. 그래서 기록을 먼저 찾는다:
/// - 그 대화를 가진 기록이 **연결돼 있으면** 그 기록에 잇는다 — 같은 에이전트 세션 id 로 오면 같은
///   supervisor 세션이다. 프로세스를 새로 띄우지 않으므로 한 대화에 두 프로세스가 붙지 않는다
///   (데몬 protocol 16 의 `adopt` 합류와 같은 결과이고, 이쪽은 판과 무관하게 동작한다).
/// - **끊겨 있으면** 그 기록을 다시 잇는다 — 데몬은 프로세스 없는 기록에 입력이 오면 스스로 되살린다.
/// - 없으면 새로 이어받는다.
///
/// 입력은 에이전트 세션 id(사용자가 가진 것은 대개 이것이다)로도, supervisor id 로도 찾는다.
/// 전체 일치가 우선이고, 없으면 앞부분 일치를 받되 **서로 다른 대화 둘 이상**에 걸리면
/// 고르지 않는다(`resolve` 와 같은 규율).
pub fn adopt_target(rows: &[SessionRow], input: &str) -> Result<Adopt, String> {
    let input = input.trim();
    fn keys(r: &SessionRow) -> [Option<&str>; 2] {
        [Some(r.id.as_str()), r.agent_session_id()]
    }
    let mut hits: Vec<&SessionRow> = rows
        .iter()
        .filter(|r| keys(r).contains(&Some(input)))
        .collect();
    if hits.is_empty() {
        hits = rows
            .iter()
            .filter(|r| keys(r).iter().flatten().any(|k| k.starts_with(input)))
            .collect();
        // 한 대화 = 에이전트 세션 id. 그것이 없는 기록은 supervisor id 가 곧 좌표다.
        let mut convos: Vec<&str> = hits
            .iter()
            .map(|r| r.agent_session_id().unwrap_or(&r.id))
            .collect();
        convos.sort_unstable();
        convos.dedup();
        if convos.len() > 1 {
            return Err(crate::domain::i18n::tn(
                "sup.err.ambiguous_session",
                &[input, &convos.len().to_string()],
            ));
        }
    }
    let Some(first) = hits.first() else {
        return Ok(Adopt::New);
    };
    let convo = first.agent_session_id().unwrap_or(&first.id);
    // 연결된 기록(그 대화를 지금 쥔 쪽)이 먼저다 — 끊긴 기록이 함께 있어도 그쪽을 되살리면 한 대화에
    // 두 프로세스가 붙는다.
    let live = hits.iter().find(|r| CONNECTED.contains(&r.state.as_str()));
    match live.or_else(|| hits.iter().find(|r| r.resumable)) {
        Some(r) => Ok(Adopt::Reuse {
            id: r.id.clone(),
            cwd: r.cwd().map(str::to_string),
        }),
        None => Err(crate::domain::i18n::tn(
            "sup.err.adopt_not_resumable",
            &[&first.id, convo, &first.state],
        )),
    }
}

/// `input` 을 풀어 이 surface 에 잇는다. `listed` 는 데몬에 물은 세션 목록이다.
///
/// **데몬에 못 물었으면 거절하지 않는다** — 적힌 값을 그대로 잇는다. 같은 칸을
/// 재시작 복원도 쓰는데(스냅샷의 `session_id`), 데몬이 아직 안 떴다고 복원을
/// 거절하면 대화 탭이 세션을 잃는다. 판정할 근거가 없을 때의 기본값은 종전
/// 동작이다. 데몬이 답했는데 없거나 모호한 것만 거절한다.
pub fn attach<H: HostCall>(
    host: &H,
    listed: Result<Vec<SessionRow>, ClientError>,
    surface_id: u32,
    input: &str,
) -> Result<String, String> {
    let id = match listed {
        Ok(rows) => resolve(&rows, input)?,
        Err(e) => {
            tracing::warn!(
                "세션 목록을 못 물어 {} 를 확인 없이 잇는다: {e}",
                input.trim()
            );
            return link_saved(host, surface_id, input);
        }
    };
    crate::application::surface_link::link(host, surface_id, &id).map_err(|e| e.to_string())?;
    Ok(id)
}

/// **이 plugin 이 저장해 둔 id** 를 확인 없이 잇는다 — 재시작 복원이 쓴다.
///
/// 그 id 는 사람이 친 것이 아니라 우리가 스냅샷에 적은 전체 id 라 풀 것이 없다.
/// 데몬에 묻지 않는 이유는 비용이다: 복원은 plugin 루프에서 탭마다 돌고, 데몬이
/// 다른 턴에 묶여 있으면 묻는 데만 탭 하나에 5초(진단까지 10초)가 든다 — 탭이
/// 여럿이면 호스트가 plugin 을 응답 없음으로 죽이는 60초에 닿는다(실측 2026-10-13:
/// 탭 셋을 복원하는 동안 CLI 호출 하나가 46초를 기다렸다).
pub fn link_saved<H: HostCall>(host: &H, surface_id: u32, id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err(crate::domain::i18n::t("sup.err.need_session_id_full"));
    }
    crate::application::surface_link::link(host, surface_id, id).map_err(|e| e.to_string())?;
    Ok(id.to_string())
}

/// 이어받을 대화가 시작된 작업 폴더 — claude 프로필이면 그 대화 기록에서 읽는다.
///
/// 못 읽으면 `None` 이고, 그때는 종전대로 데몬이 폴더를 정한다(프로필의 cwd, 없으면 데몬의 폴더).
pub fn adopted_cwd(
    sup: &impl crate::ports::supervisor::Supervisor,
    agent: &str,
    session: &str,
) -> Option<String> {
    let profile = sup
        .agent_list()
        .ok()?
        .into_iter()
        .find(|p| p.name == agent && p.kind == "claude")?;
    let dir = crate::adapters::driven::store::budget::config_dir(&profile.env)?;
    let transcript = crate::adapters::driven::store::budget::find_transcript(&dir, session)?;
    crate::adapters::driven::store::budget::transcript_cwd(&transcript)
        .filter(|c| std::path::Path::new(c).is_dir())
}

/// 폼에 놓을 것을 고른다 — **이미 다른 탭이 든 세션은 뺀다**.
///
/// surface 는 세션 하나만 들 수 있어(`surface.meta` 가 1:1) 물린 것을 고르면
/// 앞 탭의 짝이 **조용히 끊긴다**. 순수 함수로 꺼낸 이유는 시험이 닿기
/// 위해서다 — 안에 두면 이 거르기가 검증 없이 남는다(변이가 통과했다).
/// 폼이 보여 줄 것을 고른다.
///
/// **물린 세션도 목록에 둔다**(2026-10-02). 종전에는 뺐는데, 그러면
/// 사용자에게는 *"다른 탭에 있다"* 와 *"없다"* 가 **같은 화면**이 된다
/// (사용자 보고). 대신 `taken_by` 로 어느 탭인지 싣고 화면이 못 고르게
/// 한다 — 어디 있는지 알면 그 탭으로 가면 된다.
pub fn pick_for_form(
    rows: &[crate::ports::supervisor::SessionRow],
    taken: &std::collections::BTreeMap<String, u32>,
) -> (Vec<String>, Vec<crate::domain::convo::Attachable>) {
    let mut agents: Vec<String> = rows.iter().filter_map(|r| r.agent_name.clone()).collect();
    agents.sort();
    agents.dedup();
    let all = rows
        .iter()
        .map(|r| crate::domain::convo::Attachable {
            id: r.id.clone(),
            state: r.state.clone(),
            agent: r.agent_name.clone().unwrap_or_default(),
            reclaimed: r.reclaimed,
            cwd: r.cwd().map(str::to_string),
            taken_by: taken.get(&r.id).copied(),
            agent_session_id: r.agent_session_id().map(str::to_string),
            agent_session_name: crate::adapters::driven::session_info::name_of_row(r.raw()),
            remaining: crate::adapters::driven::remaining::of_row(r.cwd(), r.raw()),
        })
        .collect();
    (agents, all)
}

/// 시작 폼이 보여 줄 것 — `(agent 이름들, 붙을 수 있는 세션들)`.
///
/// # 왜 함수인가
///
/// 두 곳이 쓴다: 탭을 열 때와 **세션을 지운 뒤**. 복사해 두면 한쪽만
/// 고쳐지고, 삭제 뒤 목록에 **지운 것이 그대로 남는** 형태로 나타난다.
pub fn form_choices(
    h: &impl crate::ports::host::HostCall,
    sup: &impl crate::ports::supervisor::Supervisor,
) -> (Vec<String>, Vec<crate::domain::convo::Attachable>) {
    // 이미 다른 탭이 든 세션은 뺀다 — surface 는 세션 하나만 들 수
    // 있어(`surface.meta` 가 1:1) 물린 것을 고르면 앞 탭의 짝이
    // 조용히 끊긴다.
    // 세션 id → 그것을 든 surface id. **빼려는 것이 아니라 표시하려고**
    // 모은다(`pick_for_form` 참조).
    let taken: std::collections::BTreeMap<String, u32> =
        crate::application::surface_link::surfaces_by_session(h)
            .map(|(pairs, _)| {
                pairs
                    .into_iter()
                    .map(|(surface, sess)| (sess, surface))
                    .collect()
            })
            .unwrap_or_default();
    let (agents, mut all) = sup
        .list()
        .map(|(rows, _)| pick_for_form(&rows, &taken))
        .unwrap_or_default();
    // 이름 칸 — 데몬이 안 낸 것은 대화 기록에서 읽는다. 명부를 못 물으면 그 칸만 빈다.
    if let Ok(profiles) = sup.agent_list() {
        crate::adapters::driven::session_info::fill_names(&mut all, &profiles);
    }
    (agents, all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::cell::RefCell;

    #[derive(Default)]
    struct MetaHost {
        linked: RefCell<Vec<Value>>,
    }

    impl HostCall for MetaHost {
        fn call(&self, method: &str, params: Value) -> Result<Value, String> {
            assert_eq!(method, "surface.meta.set");
            self.linked.borrow_mut().push(params["value"].clone());
            Ok(json!({}))
        }
    }

    /// 폼의 행이 에이전트 세션 id 와 데몬이 낸 이름을 들고 간다.
    #[test]
    fn form_rows_carry_the_agent_session_and_its_listed_name() {
        let rows = vec![crate::ports::supervisor::SessionRow::from_raw_for_test(
            json!({
                "id":"sup-1","state":"stopped","agent_name":"dev",
                "agent_session_id":"conv-1","name":"listed"
            }),
        )];
        let (_, all) = pick_for_form(&rows, &Default::default());
        assert_eq!(all[0].agent_session_id.as_deref(), Some("conv-1"));
        assert_eq!(all[0].agent_session_name.as_deref(), Some("listed"));
    }

    #[test]
    fn attaching_links_the_resolved_full_id() {
        let host = MetaHost::default();
        let got = attach(&host, Ok(rows(&["74d6d290-4bc6"])), 3, "74d6d290").unwrap();
        assert_eq!(got, "74d6d290-4bc6");
        assert_eq!(*host.linked.borrow(), vec![json!("74d6d290-4bc6")]);
    }

    #[test]
    fn an_unreachable_daemon_keeps_the_given_id_instead_of_refusing() {
        let host = MetaHost::default();
        let down = Err(ClientError::Unavailable("no socket".into()));
        assert_eq!(
            attach(&host, down, 3, "74d6d290-4bc6").unwrap(),
            "74d6d290-4bc6"
        );
        assert_eq!(host.linked.borrow().len(), 1);
    }

    #[test]
    fn a_saved_id_is_linked_as_is() {
        let host = MetaHost::default();
        assert_eq!(
            link_saved(&host, 3, " 74d6d290-4bc6 ").unwrap(),
            "74d6d290-4bc6"
        );
        assert_eq!(*host.linked.borrow(), vec![json!("74d6d290-4bc6")]);
        assert!(link_saved(&host, 3, "  ").is_err());
    }

    #[test]
    fn a_refused_id_is_not_linked() {
        let host = MetaHost::default();
        assert!(attach(&host, Ok(rows(&["aaaa"])), 3, "zzzz").is_err());
        assert!(host.linked.borrow().is_empty());
    }

    fn rows(ids: &[&str]) -> Vec<SessionRow> {
        ids.iter()
            .map(|id| SessionRow::from_raw_for_test(json!({ "id": id, "state": "stopped" })))
            .collect()
    }

    #[test]
    fn a_full_id_resolves_to_itself() {
        let r = rows(&["74d6d290-4bc6", "74d6d290-4bc6-x"]);
        // 다른 id 의 앞부분이기도 하지만 전체 일치가 이긴다.
        assert_eq!(resolve(&r, "74d6d290-4bc6").unwrap(), "74d6d290-4bc6");
    }

    #[test]
    fn a_unique_prefix_resolves_to_the_full_id() {
        let r = rows(&["74d6d290-4bc6", "97f2c2e1-0b69"]);
        assert_eq!(resolve(&r, " 74d6d290 ").unwrap(), "74d6d290-4bc6");
    }

    #[test]
    fn an_unknown_or_ambiguous_input_is_refused() {
        let r = rows(&["74d6d290-aaaa", "74d6d290-bbbb"]);
        assert!(resolve(&r, "74d6d290").is_err());
        assert!(resolve(&r, "ffff").is_err());
        assert!(resolve(&r, "  ").is_err());
    }

    fn row(id: &str, agent_session: &str, state: &str, resumable: bool) -> SessionRow {
        SessionRow::from_raw_for_test(json!({
            "id": id,
            "agent_session_id": agent_session,
            "state": state,
            "resumable": resumable,
            "cwd": "/w/home",
        }))
    }

    const CONVO: &str = "6d1d8650-fea4-4b93-8291-a73ba3b23d8b";

    /// 그 대화를 가진 기록이 없으면 새로 이어받는다.
    #[test]
    fn an_unknown_conversation_is_adopted_anew() {
        let rows = vec![row("aaaa1111", "bbbb2222", "stopped", true)];
        assert_eq!(adopt_target(&rows, CONVO), Ok(Adopt::New));
    }

    /// **연결된 기록이 있으면 그 기록에 잇는다** — 같은 에이전트 세션 id 는 같은 supervisor 세션이다.
    /// 끊긴(되살릴 수 있는) 기록이 함께 있어도 연결된 쪽이 이긴다 — 끊긴 쪽을 되살리면 한 대화에
    /// 두 프로세스가 붙는다(실측: 같은 대화의 기록이 둘이었다).
    #[test]
    fn a_connected_conversation_joins_the_record_that_holds_it() {
        let rows = vec![
            row("625fccaa", CONVO, "stopped", true),
            row("f2e706b0", CONVO, "running", false),
        ];
        assert_eq!(
            adopt_target(&rows, CONVO),
            Ok(Adopt::Reuse {
                id: "f2e706b0".into(),
                cwd: Some("/w/home".into())
            }),
            "연결된 기록 대신 다른 기록을 골랐다"
        );
        let starting = vec![row("f2e706b0", CONVO, "starting", false)];
        assert!(
            matches!(adopt_target(&starting, CONVO), Ok(Adopt::Reuse { ref id, .. }) if id == "f2e706b0"),
            "뜨는 중인 기록에 잇지 않았다"
        );
    }

    /// **끊긴 기록은 그 supervisor id 로 다시 잇는다** — 새 기록을 만들지 않는다.
    #[test]
    fn a_disconnected_conversation_reuses_its_record() {
        let rows = vec![
            row("625fccaa", CONVO, "stopped", false),
            row("f2e706b0", CONVO, "stopped", true),
        ];
        assert_eq!(
            adopt_target(&rows, CONVO),
            Ok(Adopt::Reuse {
                id: "f2e706b0".into(),
                cwd: Some("/w/home".into())
            }),
            "되살릴 수 있는 기록을 골라야 한다"
        );
        // 앞부분만 적어도 같다 — 같은 대화의 기록 둘은 하나로 친다.
        assert!(matches!(
            adopt_target(&rows, "6d1d8650"),
            Ok(Adopt::Reuse { ref id, .. }) if id == "f2e706b0"
        ));
        // supervisor id 로 적어도 찾는다.
        assert!(matches!(
            adopt_target(&rows, "f2e706b0"),
            Ok(Adopt::Reuse { ref id, .. }) if id == "f2e706b0"
        ));
    }

    /// 되살릴 수 없는 기록만 있으면 새로 만들지도 잇지도 않고 사유를 낸다.
    #[test]
    fn a_record_that_cannot_resume_is_reported() {
        let rows = vec![row("625fccaa", CONVO, "failed", false)];
        let err = adopt_target(&rows, CONVO).expect_err("되살릴 수 없는 기록을 이었다");
        assert!(err.contains("625fccaa") && err.contains("failed"), "{err}");
    }

    /// 앞부분이 **서로 다른 대화 둘**에 걸리면 고르지 않는다.
    #[test]
    fn a_prefix_over_two_conversations_is_refused() {
        let rows = vec![
            row("s1", "6d1d0000-aaaa", "stopped", true),
            row("s2", "6d1d9999-bbbb", "stopped", true),
        ];
        assert!(adopt_target(&rows, "6d1d").is_err(), "둘 중 하나를 골랐다");
    }
}
