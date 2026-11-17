//! 이벤트 전달, extension hook, 렌더 컨텍스트 송신을 처리한다.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::protocol;
use tasty_plugin_manifest::EventHookDecl;

use super::{PendingRequest, PendingRequestKind, PendingSurfacePopup, PluginManager};
use tasty_plugin_manifest::PopupScopeDecl;

impl PluginManager {
    pub fn publish_host_event(&mut self, envelope: tasty_plugin_protocol::EventEnvelope) {
        let event_key = envelope.key.clone();
        let payload = envelope.payload.clone();
        let scope = envelope.meta.scope;
        let dispatches = self.event_bus.publish_from_host(envelope);
        self.send_event_dispatches(dispatches);
        self.fire_popup_triggers(&event_key, &payload, None, scope);
    }

    /// 실행 중인 plugin이 이벤트 trigger로 선언한 popup을 열고 payload를 context로 전달한다.
    /// surface 범위로 선언한 popup에 surface 이벤트 대상이 있으면 바로 열지 않고
    /// 호스트의 소유 확인을 기다린다. `publisher`는 발행한 플러그인, 호스트 발행이면 None이다.
    pub(super) fn fire_popup_triggers(
        &mut self,
        event_key: &str,
        payload: &serde_json::Value,
        publisher: Option<&str>,
        scope: tasty_plugin_protocol::EventScope,
    ) {
        let target = surface_event_target(scope, payload);
        let matches: Vec<(String, String, PopupScopeDecl)> = self
            .packages
            .iter()
            .filter(|pkg| self.processes.contains_key(&pkg.manifest.id))
            .flat_map(|pkg| {
                let plugin_id = pkg.manifest.id.clone();
                pkg.manifest.contributes.popup.iter().filter_map(move |p| {
                    if let tasty_plugin_manifest::PopupTrigger::Event { event_key: ek } = &p.trigger
                        && ek == event_key
                    {
                        return Some((plugin_id.clone(), p.id.clone(), p.scope));
                    }
                    None
                })
            })
            .collect();
        for (plugin_id, popup_id, decl) in matches {
            // surface 팝업은 그 플러그인이 발행한 이벤트로만 연다. 발행 선언은 플러그인 ID 에
            // 묶이지 않기 때문이다.
            if decl == PopupScopeDecl::Surface
                && let Some(publisher) = publisher
                && publisher != plugin_id
            {
                tracing::warn!(
                    "popup '{plugin_id}/{popup_id}' refused: event from plugin '{publisher}' \
                     cannot open another plugin's surface popup"
                );
                continue;
            }
            match (decl, target) {
                (PopupScopeDecl::Surface, Some(surface_id)) => {
                    self.pending_surface_popups.push(PendingSurfacePopup {
                        plugin_id,
                        popup_id,
                        context: payload.clone(),
                        publisher: publisher.map(str::to_string),
                        surface_id,
                    });
                }
                // 대상 surface 가 없으면 창 범위로 바꾸지 않고 열지 않는다. 이벤트는 응답이 없어 로그만 남긴다.
                (PopupScopeDecl::Surface, None) => {
                    tracing::warn!(
                        "popup '{plugin_id}/{popup_id}' not opened: event '{event_key}' has no surface target"
                    );
                }
                (PopupScopeDecl::Window, _) => {
                    self.open_popup_instance(&plugin_id, &popup_id, payload.clone());
                }
            }
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

    /// 기본 메타데이터를 채워 호스트 이벤트를 발행한다.
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

    /// plugin 이벤트를 검사하고 pre-event hook이 있으면 그 결과에 따라 전달한다.
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
        let scope = envelope.meta.scope;
        match self.event_bus.publish_from_plugin(plugin_id, envelope) {
            Ok(dispatches) => {
                self.send_event_dispatches(dispatches);
                self.fire_popup_triggers(&key_for_log, &payload, Some(plugin_id), scope);
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

    /// surface 렌더 컨텍스트를 보낸다. plugin은 별도 PaintFrame 알림으로 mesh를 돌려준다.
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

    /// 소유 plugin에 WebView navigation 시도를 알린다. 차단된 시도도 통지하며 응답은 기다리지 않는다.
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

    /// 첫 set_context보다 먼저 surface.create를 보내 생성 인자를 전달한다.
    /// 두 요청은 같은 plugin 채널의 FIFO 순서를 따른다. 응답은 별도로 기다리지 않는다.
    /// 호출 순서는 src/source_guards/mesh_bootstrap_order.rs에서 검사한다.
    pub fn needs_egui_mesh_bootstrap(
        &self,
        plugin_id: &str,
        binding: &crate::host_cmd::MeshBinding,
    ) -> bool {
        let Some(process) = self.processes.get(plugin_id) else {
            return false;
        };
        let publication = binding.publication.lock().expect("mesh binding poisoned");
        match &*publication {
            crate::host_cmd::MeshPublication::NeverSent => true,
            crate::host_cmd::MeshPublication::Sent(generations) => !generations
                .iter()
                .any(|generation| generation.process.ptr_eq(&process.reply_binding())),
            crate::host_cmd::MeshPublication::Retiring(_) => false,
        }
    }

    pub fn send_egui_mesh_surface_create(
        &self,
        plugin_id: &str,
        surface_id: u32,
        kind: &str,
        file: Option<&str>,
        display_name: &str,
        binding: &crate::host_cmd::MeshBinding,
    ) {
        let mut publication = binding.publication.lock().expect("mesh binding poisoned");
        if matches!(*publication, crate::host_cmd::MeshPublication::Retiring(_)) {
            return;
        }
        let Some(proc) = self.processes.get(plugin_id) else {
            return;
        };
        if let crate::host_cmd::MeshPublication::Sent(generations) = &*publication
            && generations
                .iter()
                .any(|generation| generation.process.ptr_eq(&proc.reply_binding()))
        {
            return;
        }
        let request = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let req = crate::protocol::PluginRequest::new(
            protocol::METHOD_SURFACE_CREATE,
            json!({
                "surface_id": surface_id,
                "kind": kind,
                "cwd": null,
                "params": { "file": file, "display_name": display_name },
            }),
            request,
        );
        if let Err(e) = proc.try_send_request(req) {
            tracing::warn!("plugin '{plugin_id}' surface.create (egui-mesh) send failed: {e}");
        } else {
            let generation = crate::host_cmd::MeshBootstrap {
                plugin: plugin_id.to_owned(),
                process: proc.reply_binding(),
                request,
            };
            match &mut *publication {
                crate::host_cmd::MeshPublication::Sent(generations) => generations.push(generation),
                _ => *publication = crate::host_cmd::MeshPublication::Sent(vec![generation]),
            }
            drop(publication);
            self.mesh_publications.register(binding);
        }
    }

    /// popup 렌더 컨텍스트를 보낸다. mesh는 별도 PopupPaintFrame 알림으로 받는다.
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

    /// banner 렌더 컨텍스트를 보낸다. mesh는 별도 BannerPaintFrame 알림으로 받는다.
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
                // 응답 전 발행에 hop 하한을 적용할 수 있도록 보낸 dispatch를 기록한다.
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

/// surface 이벤트의 대상. 프로토콜 규약대로 `EventScope::Surface` 이벤트의 payload
/// `surface_id` 만 읽는다. 그 밖의 범위나 키는 대상이 아니다.
fn surface_event_target(
    scope: tasty_plugin_protocol::EventScope,
    payload: &serde_json::Value,
) -> Option<u32> {
    if scope != tasty_plugin_protocol::EventScope::Surface {
        return None;
    }
    payload
        .get("surface_id")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}
