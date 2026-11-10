//! 대화 탭이 낸 navigation 과 호스트가 민 이벤트를 plugin 의 동작으로 옮긴다.
//!
//! webview 는 plugin 에게 직접 이벤트를 못 보낸다. 페이지가 navigation 을 시도하면 호스트가 그 URL 을
//! 알려 주고(`session_surface` 머리말), 여기서 URL 모양으로 갈래를 고른다.

use crate::adapters::driven::store::turn_archive;
use crate::adapters::driven::subagents;
use crate::adapters::driving::context_menu;
use crate::adapters::driving::folder_pick;
use crate::adapters::driving::ipc;
use crate::adapters::driving::ipc::delete_sessions;
use crate::adapters::driving::manage;
use crate::adapters::driving::statusline;
use crate::application::lazy_webview;
use crate::application::prompts;
use crate::application::session_surface;
use crate::application::surface_link;
use crate::application::sync_compact;
use crate::domain::overdue;
use crate::domain::send_queue;
use crate::ports::host::SendHost;
use serde_json::Value;

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 대화 탭의 navigation 하나를 처리한다 — 이 탭이 낸 것이 아니면 아무것도 안 한다.
    pub(crate) fn navigate(&mut self, surface_id: u32, url: &str) {
        // 우클릭 — 호스트 메뉴("터미널 ID 복사")를 연다. 매번 남기는 아래 기록에서 뺀다.
        if let Some(req) = context_menu::parse(url) {
            if let Some(host) = self.host.clone() {
                context_menu::open(&host, surface_id, req);
            }
            return;
        }
        // 쓰던 글(메모리 절약) — 아래 로그보다 먼저 거둔다. 치는 글이 300ms 마다 로그에 남으면 안 된다.
        if self.take_view_nav(surface_id, url) {
            return;
        }
        // 이 surface 가 낸 것이 아니면 무시한다 — 다른 링크까지 입력으로 읽으면
        // 사용자가 안 보낸 말이 에이전트에게 간다.
        // 실물 URL 을 남긴다 — 어떤 모양으로 오는지 추측하지 않는다(한 번
        // 틀린 모양을 가정해 시험까지 그 위에 세웠다).
        tracing::warn!(
            "[agent_session] surface={} navigation url = {}",
            surface_id,
            url
        );
        let sid = surface_id;
        let Some(host) = self.host.clone() else {
            return;
        };
        // 탭 조작(보내기 · 중지 · 비우기 · 폴더 · 응답 …)은 `#` 꼬리표로 온다 — 사람이 손댄 것이니 휴식
        // 시계를 민다. 우리가 부른 적재는 꼬리표가 없다.
        if sync_compact::is_tab_action(url)
            && let Some(c) = self.convos.get_mut(&sid)
        {
            c.sync.touch(std::time::Instant::now());
        }

        // **다시 싣기를 여기서 가르지 않는다**(0.1.54 · 0.1.50 되돌림).
        //
        // 0.1.50 이 맨 `about:blank` 를 *우클릭 Reload* 로 보고 다시 그렸다.
        // **그 전제가 틀렸다** — 통지에는 `url` 과 `surface_id` 뿐이고
        // **원인(`cause`)이 안 실린다**(`WebviewNavigationAttemptCtx`).
        // 그래서 *우리가 부른 적재* 와 *사용자 reload* 가 **같은 모양**으로
        // 온다. 결과는 `paint` → 재적재 → 그 통지 → 또 `paint` 였고,
        // 실측에서 세 surface 가 번갈아 **6회** 돌았다.
        //
        // ⚠️ **호스트가 막는 쪽(`NavigationCause::Reload`)은 그대로 둔다** —
        // 거기서는 `cause` 를 안다. 흰 화면은 그쪽이 이미 없앤다. 다만
        // *다시 그려 주는 것* 은 원인을 알 수 있을 때만 할 수 있고, 그
        // 수단이 생기기 전에는 **안 하는 것이 맞다**(틀린 자리에서 그리면
        // 멀쩡한 화면을 덮는다).

        // 대화 조작 — 중지(단추 · Esc, CLI `supervisor abort` 와 같은 처리기) · 이전 대화 펼치기.
        if self.tab_control(&host, sid, url) {
            return;
        }

        // 기존 세션에 붙기 — 새로 띄우지 않는다.
        if let Some(id) = crate::adapters::driving::url::parse_attach(url) {
            self.attach_from_form(&host, sid, &id);
            return;
        }

        // 폴더 찾기 — host 피커를 연다. 답은 `on_event` 로 비동기로 온다.
        if let Some(target) = crate::adapters::driving::url::parse_browse(url) {
            // 답을 어느 칸에 넣을지 기억한다 — 답에는 요청 id 만 온다. 연결 탭의 칸만 시작 폴더를 준다.
            let start = self.convos.get_mut(&sid).and_then(|c| {
                c.pick_target = target;
                (target == crate::domain::convo::PickField::Start && !c.cwd_pick.is_empty())
                    .then(|| c.cwd_pick.clone())
            });
            match host.call(
                "file_picker.trigger",
                folder_pick::trigger_params(sid, start.as_deref(), url),
            ) {
                Ok(v) => {
                    match v.get("request_id").and_then(serde_json::Value::as_u64) {
                        Some(rid) => {
                            // 어느 탭의 요청인지 기억한다 — 답에는 surface 가
                            // 안 실려 오고 `request_id` 만 온다.
                            self.pending_pick.insert(rid, sid);
                            // 성공도 남긴다 — 화면을 못 볼 때 *"열렸다"* 와
                            // *"안 눌렸다"* 를 로그만으로 갈라야 한다.
                            tracing::info!(
                                "[agent_session] 폴더 피커 열림 surface={sid} request_id={rid}"
                            );
                        }
                        // 호스트가 OK 를 냈는데 id 가 없다 — 답을 영영 못 받는다.
                        None => tracing::warn!(
                            "[agent_session] file_picker.trigger 응답에 request_id 가 없다: {v}"
                        ),
                    }
                }
                // 피커가 이미 열려 있으면 host 가 거부한다(단일 인스턴스).
                // 조용히 삼키지 않는다 — 단추가 안 먹은 것처럼 보인다.
                Err(e) => tracing::warn!("file_picker.trigger 실패: {e}"),
            }
            return;
        }

        // 폴더 단추 — **시작하지 않는다.** 값만 받아 폼을 다시 그린다.
        // 이 갈래가 없으면 webview 는 폼에서 떠난 채 아무것도 안 그려
        // 빈 화면이 된다(같은 축을 `action=""` 로 한 번 밟았다).
        if let Some(c) = crate::adapters::driving::url::parse_cwd_pick(url) {
            if let Some(convo) = self.convos.get_mut(&sid) {
                convo.cwd_pick = c;
            }
            if let Some(convo) = self.convos.get(&sid) {
                session_surface::push(&host, &self.sup, sid, convo, false);
            }
            return;
        }

        // **세션 삭제** — 되돌릴 수 없다. 확인을 거친 것만 실행한다.
        //
        // 첫 클릭(`confirmed=false`)은 **아무것도 하지 않는다** — 화면이
        // 그 행을 확인으로 바꾸는 일은 스크립트가 맡는다. 여기서 다시
        // 그리면 문서가 재적재되어 사용자가 방금 친 검색어가 날아간다.
        // **목록 새로고침** — 읽기라 확인 단계가 없다.
        //
        // 데몬이 정본이고 이 화면은 열 때 받은 값을 들고 있다. 다른
        // 곳에서 세션이 생기거나 사라져도 **이 탭은 모른다** — 그때
        // 쓰는 단추다.
        if crate::adapters::driving::url::parse_refresh(url) {
            self.refresh_attachable(sid);
            if let Some(c) = self.convos.get(&sid) {
                session_surface::push(&host, &self.sup, sid, c, false);
            }
            return;
        }

        if let Some((ids, confirmed)) = crate::adapters::driving::url::parse_delete_many(url) {
            if !confirmed {
                return;
            }
            delete_sessions(&self.sup, &ids);
            // 목록을 다시 그린다 — 지운 것이 사라져야 지워진 줄 안다.
            //
            // **붙을 수 있는 목록을 새로 받아야 한다.** `Convo` 의
            // `attachable` 은 폼을 그릴 때 담아 둔 값이라, 그대로 다시
            // 그리면 방금 지운 세션이 **그대로 남는다**.
            self.refresh_attachable(sid);
            if let Some(c) = self.convos.get(&sid) {
                session_surface::push(&host, &self.sup, sid, c, false);
            }
            return;
        }

        // 시작 폼의 관리 절(프로필 · 역할) — 고친 뒤 폼을 다시 그린다.
        if let Some(action) = manage::parse(url) {
            self.apply_manage(&host, sid, &action);
            return;
        }

        // 시작 폼 — 세션이 없는 탭은 이 폼을 그린다.
        if let Some(form) = crate::adapters::driving::url::parse_start(url) {
            self.start_from_form(&host, sid, &form);
            return;
        }

        let Some(text) = crate::adapters::driving::url::parse_input(url) else {
            return;
        };

        // **여기서 잠금 화면을 싣지 않는다**(0.1.17 · 실측으로 뒤집혔다).
        //
        // 종전에는 *"보내는 동안 폼을 잠근다"* 며 `push(busy=true)` 를 먼저
        // 불렀다. 그런데 `send` 가 **동기 블로킹**이라 그 사이 webview 는
        // 그려지지 않는다 — 실측(2026-09-29) 두 `push` 가 **3ms 차이**로
        // 함께 났고(55.685 → 55.688), 제출과는 3.7초 떨어져 있었다.
        //
        // 즉 잠금 화면은 **한 번도 보인 적이 없고**, 문서 전체 재적재를
        // 한 번 더 시켜 **깜빡임만 두 배**로 만들었다. `set_url` 이 유일한
        // 갱신 수단이라 한 번 실을 때마다 흰 바탕이 스친다.
        // **보낼 때 세션을 다시 찾는다.** 탭을 연 **뒤에** 이으면
        // (`link-add`) 열 때 캐시한 값은 비어 있다 — 실물에서 "이 탭에 이어진
        // 세션이 없다" 가 그것이었다. 잇기는 언제든 바뀔 수 있으니 **그때
        // 물어보는 것**이 맞다.
        if let Some(c) = self.convos.get_mut(&sid)
            && c.session_id.is_none()
        {
            c.session_id = surface_link::session_of_surface(&host, sid).ok();
        }
        //  **전송을 워커에 넘긴다**(0.1.23).
        //
        // `send_input` 은 모델이 답을 다 낼 때까지 안 돌아온다
        // (`client.rs` 의 `TURN_TIMEOUT`). 여기서 그대로 부르면 그 동안
        // plugin 루프가 **통째로 막혀** 두 번째 메시지를 못 받고, 60초를
        // 넘기면 호스트가 plugin 을 죽인다(실측 기록 — `client.rs` 주석).
        //
        // ⇒ 화면에 보일 것만 여기서 만들고(`begin_turn`), 실제 왕복은
        // 스레드가 한다. 답이 오면 `agent_session.turn_done` 으로 **정규
        // 경로에 되던져** 화면을 갱신한다 — 30초 자동 sync 가 쓰는 길과
        // 같은 형태다(`on_start` 참조).
        //  **턴이 도는 동안이면 쌓는다**(`send_queue`). 도는 세션에 또 보내면 데몬
        // 전체가 그 턴을 기다리며 선다(실측 2026-10-12).
        let staged = send_queue::parse_nonce(url);
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let busy = self.activity.turn_busy(c);
        match send_queue::take_or_queue(c, &text, busy) {
            Some(now) => self.send_turn(&host, sid, &now, staged),
            None => {
                tracing::info!(
                    "[agent_session] surface={sid}: 턴이 도는 중이라 쌓았다 ({}건 대기)",
                    c.queued.len()
                );
                session_surface::patch(
                    &host,
                    sid,
                    &send_queue::settle_msg(staged, &c.queued, None),
                );
            }
        }
    }

    /// 호스트가 민 이벤트 — 우클릭 메뉴 선택 · webview 생성 · 폴더 피커 결과.
    pub(crate) fn on_host_event(&mut self, key: &str, payload: &Value) {
        if key == context_menu::SELECTED_EVENT {
            if let Some(host) = self.host.clone() {
                context_menu::on_selected(&host, payload, |sid| self.convos.contains_key(&sid));
            }
            return;
        }
        if key == lazy_webview::WEBVIEW_CREATED_EVENT {
            if let Some(host) = self.host.clone() {
                self.webview_created(&host, payload);
            }
            return;
        }
        if key != folder_pick::FILE_PICKER_RESULT_EVENT {
            return;
        }
        let Some(r) = folder_pick::ResultWire::parse(payload) else {
            tracing::warn!("file_picker.result 모양이 틀리다");
            return;
        };
        // 수신 자체를 남긴다 — 피커는 떴는데 값이 안 실리는 경우와
        // 아예 안 뜬 경우를 로그로 가른다.
        tracing::info!(
            "[agent_session] 피커 결과 request_id={} cancelled={} paths={}",
            r.request_id,
            r.cancelled,
            r.paths.len()
        );
        let Some((sid, dir)) = resolve_pick(&mut self.pending_pick, &r) else {
            return;
        };
        tracing::info!("[agent_session] 작업 폴더 = {dir} (surface={sid})");
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        // 연결 탭의 칸은 다시 그릴 때마다 되싣는다(`cwd_pick`). 관리 탭의 칸은 한 번만 싣는다 —
        // 남겨 두면 다음에 다시 그릴 때마다 사용자가 고친 값을 덮는다.
        if c.pick_target == crate::domain::convo::PickField::Start {
            c.cwd_pick = dir;
            session_surface::push(&host, &self.sup, sid, c, false);
        } else {
            c.field_pick = Some((c.pick_target, dir));
            session_surface::push(&host, &self.sup, sid, c, false);
            c.field_pick = None;
        }
    }
}

/// 피커 결과를 **어느 탭의 어떤 폴더**로 풀지 정한다.
///
/// host 호출과 떼어 둔다 — 이 판단(내 요청인가 · 취소인가 · 폴더가
/// 뽑히는가)이 셋 다 조용히 틀릴 수 있는 자리라 시험이 닿아야 한다.
///
/// ⚠️ **표에서 지우는 것은 취소여도 한다.** 안 지우면 취소된 요청이
/// 표에 영영 남는다(요청 id 는 재사용되지 않으니 샌다).
pub(crate) fn resolve_pick(
    pending: &mut std::collections::HashMap<u64, u32>,
    r: &folder_pick::ResultWire,
) -> Option<(u32, String)> {
    let sid = pending.remove(&r.request_id)?;
    // 취소는 **아무것도 안 바꾼다** — 쳐 둔 경로를 지우면 안 된다.
    Some((sid, folder_pick::dir_from_result(r)?))
}

impl<H: crate::ports::host::SendHost, S: crate::ports::supervisor::SupervisorHandle>
    crate::application::plugin::SupervisorPlugin<H, S>
{
    /// 대화 탭의 조작 링크(중지 · 비우기 · 이전 대화)면 처리하고 `true` — 아니면 `false`(다른 갈래가 본다).
    pub(crate) fn tab_control(&mut self, host: &impl SendHost, sid: u32, url: &str) -> bool {
        if crate::adapters::driving::url::parse_stop(url) {
            self.stop_from_tab(host, sid);
        } else if crate::adapters::driving::url::parse_clear_click(url) {
            self.clear_from_tab(host, sid, None);
        } else if self.add_dir_typed(host, sid, url) {
            // 입력칸에 친 `/add-dir` — 데몬의 세션별 추가 폴더로 간다.
        } else if self.clear_typed(host, sid, url) {
            // 입력칸에 친 `/clear` — 말이 아니라 비우기다.
        } else if crate::adapters::driving::url::parse_older(url) {
            if let Some(c) = self.convos.get_mut(&sid) {
                turn_archive::expand_older(host, &self.sup, sid, c);
            }
        } else if let Some(what) = subagents::parse(url) {
            // "하위 에이전트" 줄 · 패널 머리 — 패널을 열고 닫거나 다시 읽는다.
            if let Some(c) = self.convos.get_mut(&sid) {
                subagents::click(host, &self.sup, sid, c, what);
            }
        } else if crate::adapters::driving::links::open_clicked(
            host,
            sid,
            self.convos.get(&sid).and_then(statusline::full_cwd),
            url,
        ) {
            // 답 글의 링크 — URL 은 브라우저, 있는 경로는 호스트 파일 열기로 넘겼다.
        } else if let Some(c) = self.convos.get(&sid)
            && statusline::copy_cwd(host, sid, c, url)
        {
            // 상태줄의 작업 폴더 — 전체 경로를 복사했다.
        } else if let Some(c) = self.convos.get(&sid)
            && statusline::copy_session(host, sid, c, url)
        {
            // 상태줄의 대화 id — 전체 id 를 복사했다.
        } else if statusline::copy_surface(host, sid, url) {
            // 상태줄의 surface id — 그 값을 복사했다.
        } else if statusline::parse_toggle(url)
            && let Some(c) = self.convos.get_mut(&sid)
        {
            // 상태줄 접기 · 펴기 — 탭이 쥐고 다시 그린다.
            c.status_folded = !c.status_folded;
            self.activity.refresh_status(host, sid, c);
        } else if let Some(c) = self.convos.get_mut(&sid)
            && prompts::handle_click(host, &self.sup, sid, c, url)
        {
            // 물음(권한 확인 · 질문)의 답 단추 — 답을 보내고 카드를 거뒀다.
        } else {
            return false;
        }
        true
    }

    /// 대화 탭의 중지(단추 · Esc) — 이 surface 세션의 도는 턴만 멈춘다. 보낸 것을 기억해,
    /// 곧 실패로 닫힐 그 턴을 실패가 아니라 멈춘 턴으로 그리게 한다(`finish_turn`).
    pub(crate) fn stop_from_tab(&mut self, host: &impl SendHost, sid: u32) {
        let r = ipc::handle_abort(host, &self.sup, &serde_json::json!({ "surface_id": sid }));
        match r {
            Ok(_) => {
                if let Some(c) = self.convos.get_mut(&sid) {
                    c.stop_requested = true;
                }
                tracing::info!("[agent_session] surface={sid}: 중지를 보냈다");
            }
            Err(e) => tracing::warn!("[agent_session] surface={sid}: 중지 실패: {e:?}"),
        }
        // 데몬이 모르는 턴은 데몬 중지로 안 닫힌다 — 탭에서 닫는다(`overdue::should_force_stop`).
        let now = std::time::Instant::now();
        let Some(c) = self.convos.get_mut(&sid) else {
            return;
        };
        let active = self.activity.daemon_active(c);
        if overdue::should_force_stop(c, active, now) {
            let closed = overdue::force_stop(c);
            tracing::info!(
                "[agent_session] surface={sid}: 중지 — 탭이 기다리던 턴 {closed:?} 을 닫았다"
            );
            for at in closed {
                session_surface::patch(host, sid, &session_surface::stopped_msg(at));
            }
            if let Some(c) = self.convos.get(&sid) {
                self.activity.refresh_status(host, sid, c);
            }
            self.flush_queues();
        } else {
            c.last_stop_at = Some(now);
        }
    }
}
