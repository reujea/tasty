//! 창 소유 자원의 `list` 가 단일 engine 만 보지 않고 모든 main + parked engine 을
//! 순회해 결과를 합치도록 호스트 레벨에서 special-case 처리한다.
//! CLAUDE.md "list 명령은 전체 워크스페이스를 순회" 원칙.
//!
//! **여기 없는 list 는 포커스된 창의 것만 답한다** — 에러 없이. 실측(2026-09-05, 창 둘):
//! 창1 에서 만든 headless pty 가 창2 포커스에서 `pty.list` 에 안 나오는데
//! `pty.read {id}` 는 그 pty 를 읽었다. **조작할 수 있는데 볼 수 없는** 상태이고,
//! 답이 틀렸다는 신호가 없다. 창 소유 자원의 list 를 새로 만들면 여기에 등록한다.
//!
//! 합산이 옳으려면 **id 가 engine 을 건너 유일해야 한다**(`IdGenerator`) — 안 그러면
//! 합친 목록에 같은 id 가 둘 들어가 호출자가 어느 쪽도 지목할 수 없다.

use serde_json::json;

use crate::app::App;
use crate::ipc as host_ipc;
use crate::ipc::handler::{
    attach, hooks, image, notification, output, pane, pty, surface, workspace, workspace_category,
};
use crate::ipc::protocol::JsonRpcResponse;

impl App {
    /// list 류 메서드면 모든 engine 결과를 합쳐 반환. 그 외는 None 반환해
    /// caller 가 일반 routing 계속.
    pub(crate) fn dispatch_list_global(
        &mut self,
        request: &host_ipc::protocol::JsonRpcRequest,
    ) -> Option<JsonRpcResponse> {
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);
        match request.method.as_str() {
            "notification.list" => {
                // A row outside its engine's newest 50 cannot be in the global top 50.
                let rows = one(self.merge_fields(
                    &id,
                    |_c, _s, e, id| notification::handle_notification_list(e, id),
                    &[],
                ));
                Some(JsonRpcResponse::success(
                    id,
                    json!(notification::latest_notifications(rows)),
                ))
            }
            "workspace.list" => Some(self.collect_list(id, |_c, s, e, id| {
                workspace::handle_workspace_list(s, e, id)
            })),
            "surface.list" => {
                Some(self.collect_list(id, |_c, _s, e, id| surface::handle_surface_list(e, id)))
            }
            "pane.list" => {
                Some(self.collect_list(id, |_c, _s, e, id| pane::handle_pane_list(e, id)))
            }
            // `tree` 는 이름이 `*.list` 가 아니라서 이 집합을 이름 모양으로 훑는
            // 눈에 오래 안 보였다. 성질은 같다 — 창 소유 컬렉션을 순회하고, 대상 인자가
            // 없어 포커스된 창으로 떨어졌다. 실측(창 둘, 창1 비포커스): `list tree` 가
            // 창2 의 워크스페이스만 냈고 창1 의 것은 `list panes`·`list workspaces`
            // 에는 보이는데 여기서만 사라졌다. 워크스페이스 id 는 창을 건너 유일하므로
            // (`IdGenerator` 공유) 이어 붙이면 그대로 키가 된다.
            "tree" => Some(self.collect_list(id, |_c, s, e, id| {
                JsonRpcResponse::success(id, json!(host_ipc::handler::build_engine_tree(s, e)))
            })),
            // 아래 둘은 결과가 맨 배열이 아니라 **이름 붙은 배열**이라 합산 함수가
            // 필드를 알아야 한다. 막힌 것은 함수의 거처가 아니라 결과 모양이었다.
            "pty.list" => {
                Some(self.collect_field(id, "ptys", |_c, _s, e, id| pty::handle_list(e, id)))
            }
            "output.observe_list" => Some(self.collect_field(id, "observers", |c, _s, e, id| {
                output::handle_observe_list(c, e, id)
            })),
            // `image.list` 도 `engine.workspaces` 를 순회한다 — 창 소유인데 여기 없었다.
            //
            // **이 자리에 요청이 어떻게 닿는지가 다른 list 와 다르다.** `image` 는
            // 번들 plugin 이 점유한 namespace 라 외부 호출은 step 5 의 namespace
            // forward 에서 plugin 으로 넘어가고, 그 forward 는 이 합산보다 **먼저**
            // 돈다. plugin 은 `image.list` 를 자기가 답하지 않고 trampoline 으로
            // host 에 되돌린다(`host.call`) — 그 되돌림은 `dispatch_checked` 로
            // 들어오고 거기서는 forward 단계가 없어 이 합산을 지난다. 즉 host 가
            // 합산해야 plugin 을 거쳐 온 답도 전 창을 본다.
            //
            // id 가 창을 건너 유일하다: 항목의 키는 `surface_id` 이고 surface id 는
            // `IdGenerator` 공유다(`surface.list` 가 합산인 근거와 같다).
            "image.list" => {
                Some(self.collect_field(id, "entries", |_c, _s, e, id| image::handle_list(e, id)))
            }
            "workspace_category.list" => Some(self.collect_categories(id)),
            // 두 hook 표면은 **id 공간이 공유로 바뀐 뒤에야** 합산이 뜻을 갖는다. 그 전에는
            // 두 창의 훅이 똑같이 id 1 을 받아, 합친 목록에 같은 id 가 둘 실려 호출자가
            // 어느 쪽도 지목하지 못했다(`IdGenerator` 의 hook · global_hook 카운터).
            //
            // `hook.list` 의 `surface_id` 는 **대상이 아니라 필터**다 — 그것으로 주인 창이
            // 정해지지 않으므로 여기서 합산한다. 필터는 각 engine 에 그대로 넘긴다.
            "hook.list" => {
                let params = request.params.clone();
                Some(self.collect_list(id, move |_c, _s, e, id| {
                    hooks::handle_hook_list(e, id, &params)
                }))
            }
            "global_hook.list" => {
                Some(self.collect_list(id, |_c, _s, e, id| hooks::handle_global_hook_list(e, id)))
            }
            // 점유 레지스트리(`OccupancyRegistry`)는 engine 마다 하나다 — 이름이 attach 라
            // 창 밖의 것처럼 읽히지만 저장소가 `CoreState` 에 산다. 그래서 창이 둘일 때
            // 다른 창의 점유는 **보이지 않고**, 그 목록을 믿고 free 라고 판단한 호출자가
            // 이미 점유된 surface 를 집으러 간다.
            //
            // 두 배열의 키가 창을 건너 유일하다: `surface_id` 와 `workspace_id` 는 둘 다
            // `IdGenerator` 의 공유 카운터에서 나온다(`surface.list`·`workspace.list` 가
            // 합산인 근거와 같다). 그래서 이어 붙이면 그대로 키가 된다.
            "attach.list" => Some(self.collect_fields(
                id,
                ("attached", "workspaces"),
                |_c, _s, e, id| attach::handle_list(e, id),
            )),
            _ => None,
        }
    }

    /// 카테고리 목록을 합치되 예약 카테고리 `normal` 은 **한 줄로 접는다.**
    ///
    /// 카테고리 id 는 이미 창을 건너 유일하다(`IdGenerator.category` 가 공유 카운터이고
    /// 1 부터 발급한다). 겹치는 것은 **모든 engine 에 상수로 존재하는 `normal`(id 0)**
    /// 하나뿐이라, 그것만 접으면 합친 목록의 id 가 다시 키가 된다.
    ///
    /// **접어도 지목을 잃지 않는다** — `normal` 은 rename · delete · move 가 전부
    /// 거부하는 예약 항목이라(`CategoryOpError::IsNormal`, move 는 index 0 고정) 애초에
    /// 어떤 요청의 대상이 아니다. 그래서 "어느 창의 normal 인가" 라는 물음이 생기지
    /// 않는다.
    fn collect_categories(&mut self, id: serde_json::Value) -> JsonRpcResponse {
        let rows = self.merge_fields(
            &id,
            |_c, _s, e, id| workspace_category::handle_list(e, id),
            &[],
        );
        let rows = one(rows);
        JsonRpcResponse::success(id, json!(fold_normal(rows)))
    }

    /// 결과가 **맨 배열**인 list 를 합친다.
    fn collect_list<F>(&mut self, id: serde_json::Value, f: F) -> JsonRpcResponse
    where
        F: FnMut(
            &crate::core::Core,
            &mut crate::state::AppState,
            &mut crate::core::CoreState,
            serde_json::Value,
        ) -> JsonRpcResponse,
    {
        let merged = one(self.merge_fields(&id, f, &[]));
        JsonRpcResponse::success(id, json!(merged))
    }

    /// 결과가 `{ "<field>": [...] }` 인 list 를 합쳐 같은 모양으로 되돌린다.
    fn collect_field<F>(&mut self, id: serde_json::Value, field: &str, f: F) -> JsonRpcResponse
    where
        F: FnMut(
            &crate::core::Core,
            &mut crate::state::AppState,
            &mut crate::core::CoreState,
            serde_json::Value,
        ) -> JsonRpcResponse,
    {
        let merged = one(self.merge_fields(&id, f, &[field]));
        JsonRpcResponse::success(id, json!({ field: merged }))
    }

    /// 결과가 **이름 붙은 배열 둘**인 list 를 합쳐 같은 모양으로 되돌린다.
    ///
    /// 필드마다 `merge_fields` 를 따로 부르면 engine 당 핸들러가 두 번 돌고, 그러면
    /// 한 응답의 두 배열이 **서로 다른 시점의 스냅샷**이 된다. 한 번 순회하며 둘 다
    /// 꺼낸다.
    fn collect_fields<F>(
        &mut self,
        id: serde_json::Value,
        fields: (&str, &str),
        f: F,
    ) -> JsonRpcResponse
    where
        F: FnMut(
            &crate::core::Core,
            &mut crate::state::AppState,
            &mut crate::core::CoreState,
            serde_json::Value,
        ) -> JsonRpcResponse,
    {
        let (a, b) = fields;
        let mut merged = self.merge_fields(&id, f, &[a, b]);
        let second = merged.pop().unwrap_or_default();
        let first = merged.pop().unwrap_or_default();
        JsonRpcResponse::success(id, json!({ a: first, b: second }))
    }

    /// 모든 main + parked engine 을 **한 번씩** 돌며 배열을 잇는다. `fields` 가 비면
    /// 결과 자체가 배열이라고 보고 통 하나를 돌려주고, 이름이 있으면 결과 객체의 그
    /// 필드마다 통을 하나씩 돌려준다 — 통의 수와 순서는 `fields` 와 같다.
    ///
    /// 필드가 여럿일 때 필드마다 따로 순회하지 않는 이유는 시점이다: 핸들러가 engine
    /// 당 두 번 돌면 한 응답의 두 배열이 서로 다른 스냅샷이 된다.
    fn merge_fields<F>(
        &mut self,
        id: &serde_json::Value,
        mut f: F,
        fields: &[&str],
    ) -> Vec<Vec<serde_json::Value>>
    where
        F: FnMut(
            &crate::core::Core,
            &mut crate::state::AppState,
            &mut crate::core::CoreState,
            serde_json::Value,
        ) -> JsonRpcResponse,
    {
        let take = |resp: JsonRpcResponse, out: &mut Vec<Vec<serde_json::Value>>| {
            let result = resp.result;
            for (i, slot) in out.iter_mut().enumerate() {
                let arr = match fields.get(i) {
                    Some(k) => result.as_ref().and_then(|v| v.get(k)),
                    None => result.as_ref(),
                }
                .and_then(|v| v.as_array());
                if let Some(arr) = arr {
                    slot.extend(arr.iter().cloned());
                }
            }
        };
        // `pty.list` 는 목록을 만들기 전에 idle/종료분을 걷어내므로 engine 이 `&mut` 다.
        // 그래서 필드를 쪼개 빌린다 — `&mut self.view` 와 `&self.core` 가 겹치지 않는다.
        let Self {
            view,
            parked_states,
            core,
            ..
        } = self;
        let mut combined: Vec<Vec<serde_json::Value>> = vec![Vec::new(); fields.len().max(1)];
        for w in view.views.values_mut() {
            if let Some(m) = w.as_main_mut() {
                take(
                    f(core, &mut m.state, &mut m.core_state, id.clone()),
                    &mut combined,
                );
            }
        }
        for (s, e) in parked_states.iter_mut() {
            take(f(core, s, e, id.clone()), &mut combined);
        }
        combined
    }
}

/// 통 하나짜리 순회 결과에서 그 통을 꺼낸다. `merge_fields` 는 `fields` 가 비거나
/// 이름이 하나면 통을 정확히 하나 돌려주므로 여기서 갈래가 생기지 않는다.
fn one(mut buckets: Vec<Vec<serde_json::Value>>) -> Vec<serde_json::Value> {
    buckets.pop().unwrap_or_default()
}

/// 여러 engine 에서 온 카테고리 행에서 `normal` 을 하나로 접는다.
///
/// 접힌 줄의 각 필드가 무엇을 뜻하는지 정한다 — 지어내지 않는다.
/// - `workspace_count`: 전 창 **합**. 각 창의 normal 이 담은 워크스페이스 전부다.
/// - `index`: `0`. normal 은 모든 engine 에서 위치가 고정이라(`move` 가 index 0 을
///   거부한다) 창을 안 골라도 참이다.
/// - `collapsed`: **모든 창에서 접혀 있을 때만** `true`. 한 창을 골라 그 값을 쓰면
///   나머지 창에 대해 거짓이 되므로, 집합의 성질로 답한다.
///
/// normal 이 아닌 행은 그대로 둔다 — id 가 창을 건너 유일해서 그 자체로 키다. 다만
/// `index` 는 **그 행이 온 창 안에서의 위치**라 합친 목록에서는 값이 반복될 수 있다.
fn fold_normal(rows: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut count: u64 = 0;
    let mut collapsed = true;
    let mut seen_normal = false;
    let mut rest: Vec<serde_json::Value> = Vec::new();
    let mut name = "normal".to_string();
    for row in rows {
        if row.get("is_normal").and_then(|v| v.as_bool()) != Some(true) {
            rest.push(row);
            continue;
        }
        seen_normal = true;
        count += row
            .get("workspace_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        collapsed &= row
            .get("collapsed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if let Some(n) = row.get("name").and_then(|v| v.as_str()) {
            name = n.to_string();
        }
    }
    if !seen_normal {
        return rest;
    }
    let mut out = Vec::with_capacity(rest.len() + 1);
    out.push(json!({
        "id": 0,
        "name": name,
        "index": 0,
        "collapsed": collapsed,
        "is_normal": true,
        "workspace_count": count,
    }));
    out.extend(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::fold_normal;
    use serde_json::json;

    fn cat(id: u32, is_normal: bool, count: u64, collapsed: bool) -> serde_json::Value {
        json!({
            "id": id,
            "name": if is_normal { "normal" } else { "work" },
            "index": 0,
            "collapsed": collapsed,
            "is_normal": is_normal,
            "workspace_count": count,
        })
    }

    /// 창 둘에서 온 목록: normal 이 하나로 접히고 개수는 합해진다.
    #[test]
    fn normal_folds_into_one_row_carrying_the_summed_count() {
        let rows = vec![
            cat(0, true, 2, false),
            cat(1, false, 1, false),
            cat(0, true, 3, false),
            cat(2, false, 0, false),
        ];
        let out = fold_normal(rows);
        assert_eq!(out.len(), 3, "normal 이 접히지 않았다: {out:?}");
        assert_eq!(out[0]["is_normal"], json!(true), "normal 이 맨 앞이 아니다");
        assert_eq!(out[0]["workspace_count"], json!(5));
        assert_eq!(out[0]["index"], json!(0));
        // 나머지는 id 가 유일하므로 그대로 남는다.
        let ids: Vec<u64> = out.iter().map(|r| r["id"].as_u64().expect("id")).collect();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    /// `collapsed` 는 **전부 접혀 있을 때만** true — 한 창의 값을 대표로 쓰지 않는다.
    #[test]
    fn collapsed_is_true_only_when_every_window_has_it_collapsed() {
        assert_eq!(
            fold_normal(vec![cat(0, true, 0, true), cat(0, true, 0, true)])[0]["collapsed"],
            json!(true)
        );
        assert_eq!(
            fold_normal(vec![cat(0, true, 0, true), cat(0, true, 0, false)])[0]["collapsed"],
            json!(false),
            "한 창이라도 펼쳐져 있으면 접혔다고 답하면 안 된다"
        );
    }

    /// normal 이 없는 입력(창이 하나도 없거나 목록이 빈 경우)에 빈 normal 을 지어내지 않는다.
    #[test]
    fn no_normal_row_is_invented_when_none_was_listed() {
        assert!(fold_normal(vec![]).is_empty());
        let only_other = fold_normal(vec![cat(1, false, 1, false)]);
        assert_eq!(only_other.len(), 1);
        assert_eq!(only_other[0]["id"], json!(1));
    }
}
