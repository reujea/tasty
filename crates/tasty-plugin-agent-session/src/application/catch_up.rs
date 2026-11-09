//! 복원된 대화 탭의 **빠진 답을 대화 기록 꼬리로 채운다**.
//!
//! # 왜 있나
//!
//! 턴이 도는 동안 plugin 이 교체되면(재sync · 재기동) 그 턴의 답을 받을 프로세스가 사라진다. 새
//! 프로세스는 스냅샷으로 탭을 되살리지만 스냅샷에는 도는 턴의 답이 없고, 다시 붙은 구독은 그 뒤에 나온
//! 출력만 "에이전트가 이어서" 턴으로 붙인다 — 답의 앞부분이 빠진다(실측 2026-10-21, agent-web 탭).
//!
//! # 언제 읽나
//!
//! 복원된 탭마다 한 번, 활동 폴링이 그 세션을 한가하다고 본 때다 — 복원 순간 도는 턴이었으면 그 답은
//! 턴이 끝나야 기록에 다 있다. 스냅샷이 비어 온 탭도 같다(도는 턴은 스냅샷에 안 남는다).
//!
//! # 어떻게 채우나 — 덧붙이지 않고 갈아 끼운다
//!
//! 구독이 이미 뒷부분을 붙였을 수 있으므로 덧붙이면 같은 답이 두 번 보인다. 그래서 탭에서 **보낸 말이
//! 있는 마지막 턴**을 기준점으로 기록에서 같은 말을 뒤에서부터 찾고, 그 지점부터 기록을 정본으로 탭의
//! 꼬리를 바꾼다. 기록 쪽 답 글이 화면보다 짧으면 바꾸지 않는다 — 화면에 있는 글을 잃지 않는다.
use serde_json::{Value, json};

use crate::adapters::driven::store::turn_archive::Limits;
use crate::domain::convo::Convo;
use crate::domain::convo::Convos;
use crate::domain::convo::Turn;
use crate::ports::host::SendHost;

/// 워커가 기록을 다 읽고 되던지는 자기 호출.
pub const METHOD: &str = "agent_session.history_caught_up";

fn answer_len(turns: &[Turn]) -> usize {
    turns.iter().map(|t| t.got.trim().chars().count()).sum()
}

/// 탭의 꼬리를 기록으로 맞춘다 — 바꿨으면 `true`.
pub fn merge_tail(tab: &mut Vec<Turn>, hist: &[Turn]) -> bool {
    let Some(a) = tab.iter().rposition(|t| !t.sent.is_empty()) else {
        return false;
    };
    let Some(h) = hist.iter().rposition(|t| t.sent == tab[a].sent) else {
        return false;
    };
    let canon = &hist[h..];
    let cur = &tab[a..];
    let same = cur.len() == canon.len()
        && cur
            .iter()
            .zip(canon)
            .all(|(x, y)| x.sent == y.sent && x.got.trim() == y.got.trim());
    if same || answer_len(canon) < answer_len(cur) {
        return false;
    }
    tab.truncate(a);
    tab.extend(canon.iter().cloned());
    true
}

/// 기록 꼬리를 뒤에서 읽는다 — 결과는 [`METHOD`] 로 온다.
pub fn spawn<S: crate::ports::supervisor::Supervisor + Send + 'static>(
    host: impl SendHost,
    sup: S,
    surface_id: u32,
    session: String,
    limits: Limits,
) {
    let spawned = std::thread::Builder::new()
        .name(format!("agent-session-catchup-{surface_id}"))
        .spawn(move || {
            // 보관 파일에 다시 쓰지 않는다 — 탭이 이미 옮긴 턴이 겹친다. 꼬리만 필요하다.
            let limits = Limits {
                swap: false,
                ..limits
            };
            let Some(l) = crate::application::history::fill(&sup, &session, limits, None) else {
                return;
            };
            let params = json!({
                "surface_id": surface_id,
                "session": session,
                "snapshot": crate::domain::convo::snapshot_with_turns(Some(&session), &l.turns),
            });
            if let Err(e) = host.self_invoke(METHOD, params) {
                tracing::debug!("기록 꼬리를 넘기지 못했다(plugin 이 내려가는 중): {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("기록 꼬리를 읽을 워커를 못 띄웠다 — 빠진 답이 안 채워진다: {e}");
    }
}

/// 워커가 읽은 꼬리를 탭에 맞춘다 — 바꿨으면 다시 그릴 탭.
///
/// 그 사이 탭이 다른 세션으로 바뀌었거나 이 탭이 보낸 턴이 도는 중이면 손대지 않는다 — 도는 턴의 자리
/// (index)를 워커가 들고 있어 꼬리를 바꾸면 답이 엉뚱한 턴에 붙는다.
pub fn apply(convos: &mut Convos, params: &Value) -> Option<u32> {
    let field = |k: &str| {
        params
            .pointer(&format!("/params/{k}"))
            .or_else(|| params.get(k))
    };
    let sid = u32::try_from(field("surface_id")?.as_u64()?).ok()?;
    let session = field("session")?.as_str()?;
    let convo: &mut Convo = convos.get_mut(&sid)?;
    if convo.session_id.as_deref() != Some(session)
        || convo.turns.iter().any(|t| t.pending)
        || !convo.overdue.is_empty()
    {
        return None;
    }
    let hist = crate::domain::convo::turns_from_snapshot(field("snapshot")?);
    let changed = merge_tail(&mut convo.turns, &hist);
    if changed {
        tracing::info!("[agent_session] surface={sid}: 빠진 답을 대화 기록 꼬리로 채웠다");
    }
    changed.then_some(sid)
}

/// 탭 쪽 처리 — 대화 · 활동 · 구독을 함께 만지므로 plugin 의 메서드로 둔다.
impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 기록 꼬리를 다 읽었다 — 빠진 답을 채웠으면 다시 그린다.
    pub(crate) fn caught_up(&mut self, host: &impl SendHost, params: &Value) {
        if let Some(sid) = apply(&mut self.convos, params)
            && let Some(c) = self.convos.get(&sid)
        {
            crate::application::session_surface::push(
                host,
                &self.sup,
                sid,
                c,
                self.activity.is_busy(c),
            );
        }
    }

    /// 구독 이벤트를 반영한다.
    pub(crate) fn after_stream(&mut self, host: &impl SendHost, params: &Value) {
        for sid in self.streams.apply(host, &mut self.convos, params) {
            if let Some(c) = self.convos.get(&sid) {
                self.activity.refresh_status(host, sid, c);
            }
        }
        self.refresh_subagents(host);
    }

    /// 활동 폴링이 끝났다 — 복원된 탭 중 세션이 한가한 것은 기록 꼬리를 한 번 읽는다.
    ///
    /// 구독 이벤트로 재지 않는 이유: 구독은 다음 폴링에 다시 붙으므로, 교체 직후 끝난 턴은 구독이 못
    /// 본다(실측 2026-10-21 — 6초 턴 중 교체에서 끝 이벤트가 오지 않았다). 폴링은 턴이 끝난 뒤의
    /// 한가함을 늦게라도 반드시 본다.
    pub(crate) fn catch_up_idle(&mut self, host: &impl SendHost) {
        let Some(hh) = self.host.clone() else {
            return;
        };
        let due: Vec<(u32, String)> = self
            .convos
            .iter()
            .filter(|(_, c)| c.catch_up_on_finish && !self.activity.turn_busy(c))
            .filter_map(|(sid, c)| Some((*sid, c.session_id.clone()?)))
            .collect();
        for (sid, session) in due {
            if let Some(c) = self.convos.get_mut(&sid) {
                c.catch_up_on_finish = false;
            }
            spawn(
                hh.clone(),
                self.sup,
                sid,
                session,
                crate::adapters::driven::store::turn_archive::read_limits(host),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(sent: &str, got: &str) -> Turn {
        Turn {
            sent: sent.into(),
            got: got.into(),
            pending: false,
            at: None,
            error: None,
        }
    }

    /// 교체로 답의 앞부분을 잃고, 구독이 뒷부분만 "이어서" 로 붙인 탭 — 기록의 온전한 답으로 바뀐다.
    #[test]
    fn a_lost_answer_is_replaced_by_the_whole_one_without_doubling() {
        let mut tab = vec![t("a", "A"), t("b", ""), t("", "tail of B")];
        let hist = vec![t("a", "A"), t("b", "head of B\n\ntail of B")];
        assert!(merge_tail(&mut tab, &hist));
        assert_eq!(tab.len(), 2, "이어서 턴이 남아 같은 답이 두 번 보인다");
        assert_eq!(tab[1].got, "head of B\n\ntail of B");
    }

    #[test]
    fn turns_after_the_anchor_come_in_from_the_record() {
        let mut tab = vec![t("a", "A")];
        let hist = vec![t("a", "A"), t("c", "C")];
        assert!(merge_tail(&mut tab, &hist));
        assert_eq!(tab.last().map(|x| x.sent.as_str()), Some("c"));
    }

    #[test]
    fn nothing_changes_when_the_tab_already_matches() {
        let mut tab = vec![t("a", "A"), t("b", "B")];
        let hist = vec![t("a", "A"), t("b", " B ")];
        assert!(!merge_tail(&mut tab, &hist));
    }

    /// 기록이 더 짧으면(아직 다 안 써졌다) 화면의 글을 지우지 않는다.
    #[test]
    fn a_shorter_record_never_erases_what_the_tab_shows() {
        let mut tab = vec![t("b", "long answer")];
        let hist = vec![t("b", "long")];
        assert!(!merge_tail(&mut tab, &hist));
        assert_eq!(tab[0].got, "long answer");
    }

    /// 기준점이 기록에 없으면(다른 대화 · 형식 변화) 추측하지 않는다.
    #[test]
    fn an_anchor_missing_from_the_record_leaves_the_tab_alone() {
        let mut tab = vec![t("x", "")];
        assert!(!merge_tail(&mut tab, &[t("y", "Y")]));
        let mut empty: Vec<Turn> = vec![t("", "only auto")];
        assert!(!merge_tail(&mut empty, &[t("y", "Y")]));
    }

    #[test]
    fn a_running_turn_of_this_tab_is_never_touched() {
        let mut convos = Convos::default();
        convos.insert(
            3,
            Convo {
                session_id: Some("s".into()),
                turns: vec![Turn {
                    pending: true,
                    at: None,
                    ..t("b", "")
                }],
                ..Default::default()
            },
        );
        let snap = crate::domain::convo::snapshot_with_turns(Some("s"), &[t("b", "B")]);
        let p = json!({ "surface_id": 3, "session": "s", "snapshot": snap });
        assert_eq!(apply(&mut convos, &p), None);
        convos.get_mut(&3).unwrap().turns[0].pending = false;
        assert_eq!(apply(&mut convos, &p), Some(3));
        assert_eq!(convos[&3].turns[0].got, "B");
    }
}
