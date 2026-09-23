//! Event Bus 라우팅: 호스트 발화 envelope, plugin publish → fan-out, pre/post hook
//! dispatch, throttle 처리, surface event flush.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::protocol;
use tasty_plugin_manifest::EventHookDecl;

use super::{PendingRequest, PendingRequestKind, PluginManager};

impl PluginManager {
    pub fn publish_host_event(&mut self, envelope: tasty_plugin_protocol::EventEnvelope) {
        let event_key = envelope.key.clone();
        let payload = envelope.payload.clone();
        let dispatches = self.event_bus.publish_from_host(envelope);
        self.send_event_dispatches(dispatches);
        self.fire_popup_triggers(&event_key, &payload);
    }

    /// `[[contributes.popup]] trigger.kind = "event"`로 선언된 popup 중 방금 발화된
    /// 이벤트 key에 매칭되는 것을 자동으로 연다. plugin process가 살아 있어야 한다.
    /// payload는 popup.open IPC의 `context`로 그대로 전달된다.
    pub(super) fn fire_popup_triggers(&mut self, event_key: &str, payload: &serde_json::Value) {
        let matches: Vec<(String, String)> = self
            .packages
            .iter()
            .filter(|pkg| self.processes.contains_key(&pkg.manifest.id))
            .flat_map(|pkg| {
                let plugin_id = pkg.manifest.id.clone();
                pkg.manifest.contributes.popup.iter().filter_map(move |p| {
                    if let tasty_plugin_manifest::PopupTrigger::Event { event_key: ek } = &p.trigger
                        && ek == event_key
                    {
                        return Some((plugin_id.clone(), p.id.clone()));
                    }
                    None
                })
            })
            .collect();
        for (plugin_id, popup_id) in matches {
            self.open_popup_instance(&plugin_id, &popup_id, payload.clone());
        }
    }

    /// `EventScope`/origin/trace_id를 호스트 기본값으로 채워 envelope을 만든다.
    pub fn build_host_envelope<P: serde::Serialize>(
        &self,
        key: &str,
        payload: &P,
        scope: tasty_plugin_protocol::EventScope,
    ) -> tasty_plugin_protocol::EventEnvelope {
        let trace_seq = self.event_trace_seq.fetch_add(1, Ordering::Relaxed);
        tasty_plugin_protocol::EventEnvelope {
            key: key.to_string(),
            payload: serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
            meta: tasty_plugin_protocol::EventMeta {
                trace_id: format!("h{trace_seq:x}"),
                hop: 0,
                origin: tasty_plugin_protocol::EventOrigin::Host,
                scope,
            },
        }
    }

    /// 호스트가 직접 한 줄로 발화. envelope을 만드는 호출자가 거의 모든 곳이라
    /// 편의 헬퍼.
    pub fn emit_host_event<P: serde::Serialize>(
        &mut self,
        key: &str,
        payload: &P,
        scope: tasty_plugin_protocol::EventScope,
    ) {
        let envelope = self.build_host_envelope(key, payload, scope);
        self.publish_host_event(envelope);
    }

    /// owner unicast 발화 — envelope를 정확히 한 plugin에만 전달. 호스트가 명시적으로
    /// 보내는 메시지이므로 구독 등록 여부와 무관하다. `command.invoked`처럼
    /// "broadcast 아님"으로 지정된 이벤트에 사용. plugin이 실행 중이 아니면 조용히 폐기.
    pub fn emit_host_event_to_plugin<P: serde::Serialize>(
        &mut self,
        plugin_id: &str,
        key: &str,
        payload: &P,
        scope: tasty_plugin_protocol::EventScope,
    ) {
        if !self.processes.contains_key(plugin_id) {
            tracing::trace!(
                "emit_host_event_to_plugin: plugin '{}' not running, dropping '{}'",
                plugin_id,
                key
            );
            return;
        }
        let envelope = self.build_host_envelope(key, payload, scope);
        let dispatch = self.event_bus.unicast_to_plugin(plugin_id, envelope);
        self.send_event_dispatches(vec![dispatch]);
    }

    /// plugin이 보낸 publish를 라우팅. 권한/origin/hop 검사 실패 시 경고 로그.
    /// 활성 extension이 있고 pre_event hook이 매칭되면 hook을 먼저 dispatch한 뒤
    /// 응답에 따라 fan-out 진행 (PR 6).
    pub(super) fn route_plugin_event_publish(
        &mut self,
        plugin_id: &str,
        mut envelope: tasty_plugin_protocol::EventEnvelope,
    ) {
        // 재발화 판정은 **도착한 순간**에 한다. hook 을 거치는 publish 는 fan-out 이
        // hook 응답 뒤로 밀리고, 그 사이에 dispatch 응답이 오면 하한이 사라진다.
        self.event_bus.apply_relay_floor(plugin_id, &mut envelope);
        // hook이 적용되는 publisher인지 먼저 검사. caller가 extension 자신이면 self-loop 방지.
        let hooks = self.find_active_event_hooks(plugin_id, &envelope.key);
        match hooks {
            Some((ext_id, pre_opt, post_opt)) => {
                let pre = pre_opt.filter(|h| !self.is_hook_in_backoff(&ext_id, &h.event));
                let post = post_opt.filter(|h| !self.is_hook_in_backoff(&ext_id, &h.event));
                if let Some(pre) = pre {
                    self.dispatch_pre_event_hook(plugin_id, ext_id, envelope, pre, post);
                } else if post.is_some() {
                    self.fan_out_then_post(plugin_id, envelope, ext_id, post);
                } else {
                    self.publish_and_dispatch(plugin_id, envelope);
                }
            }
            None => self.publish_and_dispatch(plugin_id, envelope),
        }
    }

    /// `publish_from_plugin` 호출 + 결과 dispatch. hook 없는 경로의 helper.
    pub(super) fn publish_and_dispatch(
        &mut self,
        plugin_id: &str,
        envelope: tasty_plugin_protocol::EventEnvelope,
    ) {
        let key_for_log = envelope.key.clone();
        let payload = envelope.payload.clone();
        match self.event_bus.publish_from_plugin(plugin_id, envelope) {
            Ok(dispatches) => {
                self.send_event_dispatches(dispatches);
                self.fire_popup_triggers(&key_for_log, &payload);
            }
            Err(e) => {
                tracing::warn!("plugin '{plugin_id}' publish '{key_for_log}' rejected: {e}");
            }
        }
    }

    /// 활성 extension이 있고 publisher가 그 extension의 target이며 매칭 event hook이
    /// 있으면 (ext_id, pre_hook, post_hook)을 반환. caller가 extension 자신이면 None.
    pub(super) fn find_active_event_hooks(
        &self,
        publisher_plugin_id: &str,
        event_key: &str,
    ) -> Option<(String, Option<EventHookDecl>, Option<EventHookDecl>)> {
        let ext_id = self
            .extensions
            .active_extension_for_target(publisher_plugin_id)?
            .to_string();
        if ext_id == publisher_plugin_id {
            return None;
        }
        let pkg = self.packages.iter().find(|p| p.manifest.id == ext_id)?;
        let extends = pkg.manifest.extends.as_ref()?;
        let pre = extends
            .pre_event
            .iter()
            .find(|h| h.event == event_key)
            .cloned();
        let post = extends
            .post_event
            .iter()
            .find(|h| h.event == event_key)
            .cloned();
        if pre.is_none() && post.is_none() {
            None
        } else {
            Some((ext_id, pre, post))
        }
    }

    pub(super) fn dispatch_pre_event_hook(
        &mut self,
        publisher_plugin_id: &str,
        ext_id: String,
        envelope: tasty_plugin_protocol::EventEnvelope,
        pre: EventHookDecl,
        post: Option<EventHookDecl>,
    ) {
        let payload = envelope.payload.clone();
        let deadline = Instant::now() + Duration::from_millis(pre.timeout_ms as u64);
        match self.send_extension_invoke_hook(
            &ext_id,
            tasty_plugin_protocol::ExtensionHookKind::Event,
            tasty_plugin_protocol::ExtensionHookPhase::Pre,
            pre.mode,
            &envelope.key,
            payload,
        ) {
            Ok(req_id) => {
                self.pending_requests.insert(
                    req_id,
                    PendingRequest::now(
                        ext_id.clone(),
                        PendingRequestKind::ExtensionPreEventHook {
                            publisher_plugin_id: publisher_plugin_id.to_string(),
                            extension_plugin_id: ext_id,
                            envelope,
                            pre_hook_mode: pre.mode,
                            post_hook: post,
                            deadline,
                        },
                    ),
                );
            }
            Err(msg) => {
                tracing::warn!("pre-event-hook dispatch failed: {msg}; bypassing");
                self.fan_out_then_post(publisher_plugin_id, envelope, ext_id, post);
            }
        }
    }

    /// fan-out 실행 후 post_event hook이 있으면 dispatch.
    pub(super) fn fan_out_then_post(
        &mut self,
        publisher_plugin_id: &str,
        envelope: tasty_plugin_protocol::EventEnvelope,
        ext_id: String,
        post: Option<EventHookDecl>,
    ) {
        let event_key = envelope.key.clone();
        let payload = envelope.payload.clone();
        self.publish_and_dispatch(publisher_plugin_id, envelope);
        if let Some(post) = post {
            if self.is_hook_in_backoff(&ext_id, &event_key) {
                return;
            }
            let deadline = Instant::now() + Duration::from_millis(post.timeout_ms as u64);
            match self.send_extension_invoke_hook(
                &ext_id,
                tasty_plugin_protocol::ExtensionHookKind::Event,
                tasty_plugin_protocol::ExtensionHookPhase::Post,
                post.mode,
                &event_key,
                payload,
            ) {
                Ok(req_id) => {
                    self.pending_requests.insert(
                        req_id,
                        PendingRequest::now(
                            ext_id.clone(),
                            PendingRequestKind::ExtensionPostEventHook {
                                extension_plugin_id: ext_id,
                                event_key,
                                deadline,
                            },
                        ),
                    );
                }
                Err(msg) => {
                    tracing::warn!("post-event-hook dispatch failed: {msg}; ignoring");
                }
            }
        }
    }

    /// egui-mesh surface 에 렌더 컨텍스트(크기/ppp/raw input)를 forward (A1-S3).
    ///
    /// fire-and-forget — plugin 은 응답 대신 비동기 [`PluginEvent::PaintFrame`] 알림으로
    /// mesh 를 회신한다. 실제 raw_input 수집·송신 호출은 입력 forward 배선(A1-S5/S7)이
    /// 담당하고, 본 헬퍼는 host→plugin 송신 경로만 제공한다.
    ///
    /// [`PluginEvent`]: tasty_plugin_protocol::PluginEvent
    pub fn send_surface_set_context(
        &self,
        plugin_id: &str,
        params: &tasty_plugin_protocol::SurfaceSetContextParams,
    ) {
        let Some(proc) = self.processes.get(plugin_id) else {
            return;
        };
        let req = crate::protocol::PluginRequest::new(
            protocol::METHOD_SURFACE_SET_CONTEXT,
            json!(params),
            self.next_request_id.fetch_add(1, Ordering::Relaxed),
        );
        if let Err(e) = proc.try_send_request(req) {
            tracing::warn!("plugin '{plugin_id}' surface.set_context send failed: {e}");
        }
    }

    /// webview surface 가 시도한 navigation 의 URL 을 소유 plugin 에 fire-and-forget
    /// 통지(`webview.navigation_attempt`). `webview.set_url`(plugin→host)의 반대
    /// 방향. "원격 http(s) 차단" 판정과 독립 — 차단 여부와 무관하게 모든 navigation
    /// 시도(로컬 파일 링크 포함)마다 발사된다. plugin 은 응답하지 않는다(host 는 응답을
    /// 기다리지 않고 무시).
    pub fn send_webview_navigation_attempt(
        &self,
        plugin_id: &str,
        params: &tasty_plugin_protocol::WebviewNavigationAttemptParams,
    ) {
        let Some(proc) = self.processes.get(plugin_id) else {
            return;
        };
        let req = crate::protocol::PluginRequest::new(
            protocol::METHOD_WEBVIEW_NAVIGATION_ATTEMPT,
            json!(params),
            self.next_request_id.fetch_add(1, Ordering::Relaxed),
        );
        if let Err(e) = proc.try_send_request(req) {
            tracing::warn!("plugin '{plugin_id}' webview.navigation_attempt send failed: {e}");
        }
    }

    /// egui-mesh surface 의 owning plugin 에 `surface.create` 를 fire-and-forget 으로
    /// 보낸다. host 측 surface(`EguiMeshSurface` stand-in)는 tree/handles 가 없어
    /// `RemoteSurfaceEntry` 를 만들지 않으므로 plugin 의 (빈) 응답은 무시된다.
    ///
    /// 호출자는 첫 set_context bootstrap **직전**에 부른다 — 같은 plugin req 채널 FIFO 라
    /// create 가 set_context 보다 먼저 도착해 plugin 이 생성 params(예: markdown `file`)를
    /// set_context 렌더 전에 받는다.
    ///
    /// 호출자를 여기 열거하지 않는다. 한때 `MainView::forward_egui_mesh_context` 하나로
    /// 적혀 있었는데 실제로는 셋이었고, 산문이 든 명부는 늘어도 따라오지 않는다.
    /// 그 순서는 본체의 `src/source_guards/mesh_bootstrap_order.rs` 가 호출자를 **스캔해서**
    /// 판정한다 — 넷째가 생기면 자동으로 좌변에 들어온다.
    pub fn send_egui_mesh_surface_create(
        &self,
        plugin_id: &str,
        surface_id: u32,
        kind: &str,
        file: Option<&str>,
        display_name: &str,
    ) {
        let Some(proc) = self.processes.get(plugin_id) else {
            return;
        };
        let req = crate::protocol::PluginRequest::new(
            protocol::METHOD_SURFACE_CREATE,
            json!({
                "surface_id": surface_id,
                "kind": kind,
                "cwd": null,
                "params": { "file": file, "display_name": display_name },
            }),
            self.next_request_id.fetch_add(1, Ordering::Relaxed),
        );
        if let Err(e) = proc.try_send_request(req) {
            tracing::warn!("plugin '{plugin_id}' surface.create (egui-mesh) send failed: {e}");
        }
    }

    /// egui-mesh popup 인스턴스에 렌더 컨텍스트(크기/ppp/raw input)를 forward (A2).
    ///
    /// [`send_surface_set_context`](Self::send_surface_set_context) 의 popup 대응 —
    /// fire-and-forget 이고, plugin 은 응답 대신 비동기 [`PopupPaintFrame`] 알림으로
    /// mesh 를 회신한다. 호스트 popup 합성기(`src/plugin_bridge/popup_render.rs`)가 host→plugin 송신
    /// 경로로 이 헬퍼를 호출한다.
    ///
    /// [`PopupPaintFrame`]: tasty_plugin_protocol::PluginEvent::PopupPaintFrame
    pub fn send_popup_set_context(
        &self,
        plugin_id: &str,
        params: &tasty_plugin_protocol::PopupSetContextParams,
    ) {
        let Some(proc) = self.processes.get(plugin_id) else {
            return;
        };
        let req = crate::protocol::PluginRequest::new(
            protocol::METHOD_POPUP_SET_CONTEXT,
            json!(params),
            self.next_request_id.fetch_add(1, Ordering::Relaxed),
        );
        if let Err(e) = proc.try_send_request(req) {
            tracing::warn!("plugin '{plugin_id}' popup.set_context send failed: {e}");
        }
    }

    /// egui-mesh banner 인스턴스에 렌더 컨텍스트(크기/ppp/raw input/theme)를 forward (A3).
    ///
    /// [`send_popup_set_context`](Self::send_popup_set_context) 의 banner 대응 —
    /// fire-and-forget 이고, plugin 은 응답 대신 비동기 [`BannerPaintFrame`] 알림으로
    /// mesh 를 회신한다. 호스트 banner 합성기가 host→plugin 송신 경로로 이 헬퍼를 호출한다.
    ///
    /// [`BannerPaintFrame`]: tasty_plugin_protocol::PluginEvent::BannerPaintFrame
    pub fn send_banner_set_context(
        &self,
        plugin_id: &str,
        params: &tasty_plugin_protocol::BannerSetContextParams,
    ) {
        let Some(proc) = self.processes.get(plugin_id) else {
            return;
        };
        let req = crate::protocol::PluginRequest::new(
            protocol::METHOD_BANNER_SET_CONTEXT,
            json!(params),
            self.next_request_id.fetch_add(1, Ordering::Relaxed),
        );
        if let Err(e) = proc.try_send_request(req) {
            tracing::warn!("plugin '{plugin_id}' banner.set_context send failed: {e}");
        }
    }

    pub(super) fn send_event_dispatches(
        &mut self,
        dispatches: Vec<crate::event_bus::PluginDispatch>,
    ) {
        for d in dispatches {
            let mut req = crate::event_bus::EventBus::build_dispatch_request(&d);
            req.id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            let request_id = req.id;
            let Some(proc) = self.processes.get(&d.plugin_id) else {
                continue;
            };
            match proc.try_send_request(req) {
                // 응답이 올 때까지 이 plugin 의 publish 는 이 사건에 대한 반응으로 친다
                // (재발화 hop 하한 — `EventBus::apply_relay_floor`).
                Ok(()) => {
                    self.event_bus
                        .note_dispatch_sent(&d.plugin_id, request_id, d.envelope.meta.hop)
                }
                Err(e) => {
                    tracing::warn!("plugin '{}' event.dispatch send failed: {}", d.plugin_id, e)
                }
            }
        }
    }
}
