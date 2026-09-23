# Plugin Protocol Changelog

이 문서는 외부 plugin 작성자가 의존하는 wire schema의 변경 이력을 기록한다.

## 정책

- `HOST_API_VERSION`은 메이저 버전 단위로 호스트와 plugin 사이에 매치된다 (`crates/tasty-plugin-manifest/src/types.rs::HOST_API_VERSION`).
- minor 추가는 같은 `api_version` 내에서 호환되어야 한다. 새 필드는 **optional + default**, 새 enum variant는 `#[serde(other)]`로 fallback 가능한 형태로만 허용한다.
- major 증가가 필요한 변경은 별도 RFC가 동반되어야 한다 (필드 의미 변경/제거, required 필드 추가, 에러 코드 의미 변경 등).
- 자세한 break 분류와 deprecation 절차는 [docs/dev-guide/plugin-packaging.md#생태계-정책--자동-upgrade--호환성-분류 §4](../../docs/dev-guide/plugin-packaging.md#생태계-정책--자동-upgrade--호환성-분류) + [docs/dev-guide/api-conventions.md](../../docs/dev-guide/api-conventions.md) 의 "안정성 정책" 절 참조.

본 changelog는 [Keep a Changelog 1.1](https://keepachangelog.com/en/1.1.0/) 형식을 따른다.

## [Unreleased]

### Added
- `PluginRequest` 에 `dropped_requests: u64` (optional + default 0 — additive, api_version 유지) — 호스트 → plugin 요청 큐가 가득 차면 호스트는 그 요청을 **기다리지 않고 버린다**([ADR-0315](../../docs/adr/0315-the-two-directions-of-a-plugin-channel-answer-saturation-differently.md)). 버려진 요청은 소켓에 안 나가므로 plugin 은 그것이 있었다는 사실 자체를 몰랐다. 이제 그 뒤로 **실제 큐에 들어가는 다음 요청**에 그 사이 버린 수가 실린다. 0 이면 직렬화에서 빠지므로 구버전 plugin 이 보는 바이트는 종전과 같고, 낯선 키는 `#[serde(default)]` 로 무시된다. SDK 노출은 `Plugin::on_host_dropped_requests`(기본 no-op) · `HostHandle::dropped_by_host()` · 자동 `warn` 로그. 근거는 [ADR-0339](../../docs/adr/0339-the-host-tells-a-plugin-what-saturation-dropped.md).
- `IpcCallResult` 에 `error_code: Option<i32>` (optional + default — additive, api_version 유지) — 호스트가 plugin caller 에게 돌려주는 `ipc.result` 가 JSON-RPC 코드를 함께 싣는다. 종전에는 메시지만 가서, 다른 plugin 을 부른 plugin 은 실패 사유를 문자열로 뒤져야 했다. 구버전 호스트가 보낸 모양은 `None` 으로 읽힌다. 근거는 [ADR-0171](../../docs/adr/0171-a-host-error-code-survives-the-plugin-boundary.md).
- `PluginEvent::BannerInvalidated { instance_id }` — 위 `PopupInvalidated` 의 egui-mesh banner 대응. banner 도 같은 `EguiMeshCore` 를 쓰므로 egui 가 `viewport_output` 으로 다음 pass 를 요청할 수 있는데(hover fade·스크롤 스무딩·스피너) 그 요청을 host 로 올리는 자리가 없었다 — SDK 가 값을 계산해 놓고 버렸다. `#[serde(other)]` fallback(`PluginEvent::Unknown`) 대상이라 구버전 host 는 안전하게 무시한다. (additive, api_version 유지)

## [0.10.2] - 2026-08-29

### Added
- `PluginEvent::PopupInvalidated { instance_id }` — [`PluginEvent::SurfaceInvalidated`]의 egui-mesh popup 대응. plugin이 out-of-band로(egui `viewport_output`의 self-repaint 요청 등) popup의 무입력 재-forward를 요청할 때 쓴다. `#[serde(other)]` fallback(`PluginEvent::Unknown`) 대상이라 구버전 host는 안전하게 무시한다. (additive, api_version 유지)
- `PluginEvent::SharedBufferReleased { id }` — plugin 이 자기 측 shared buffer 매핑을 폐기했음을 host 에 알리는 fire-and-forget 이벤트 (egui-mesh buffer 성장 재생성 시 SDK `ensure_buffer` 가 송신). host 는 대응 매핑을 해제한다 — 통지가 없으면 구세대 버퍼가 plugin 수명 내내 host 에 남는다 (soak s6 누수 조사에서 발견). 인스턴스 *닫힘* 경로는 host 가 frame 메타 기반으로 자체 해제하므로 이 이벤트는 생존 중 교체 전용.
- `PluginEvent::Unknown` (`#[serde(other)]` fallback) — 미지의 event kind 를 받아도 host 가 메시지 단위 파싱 실패 없이 무시하도록 하는 forward-compat 안전장치. 본 정책("새 enum variant 는 fallback 가능한 형태로만")의 구조적 이행이며, 이후 variant 추가가 additive 로 안전해진다. (구버전 host + 신버전 plugin 조합은 in-tree 재빌드 정책상 실사용 영향 없음)
- egui-mesh 텍스처 delta 체인 필드 (전부 optional + default — additive, api_version 유지):
  - `PluginEvent::PaintFrame` / `PopupPaintFrame` / `BannerPaintFrame` 에 `frame_seq: u64`(SDK 렌더 코어의 송신 frame 단조 시퀀스, shared buffer 재생성과 무관) + `full_textures: bool`(이 frame 의 textures_delta 가 plugin 의 전체 텍스처 상태를 full image 로 담음) 추가. host 는 `frame_seq == last + 1` 로 delta 체인 연속성을 검증하고, full frame 은 체인과 무관하게 수락한다.
  - `SurfaceSetContextParams` / `PopupSetContextParams` / `BannerSetContextParams` 에 `need_full_textures: bool` 추가 — host 가 텍스처 상태를 복구해야 할 때(신규 Renderer / 체인 단절) plugin SDK 에 전체 텍스처 상태의 full 재전송을 요청한다. SDK 는 이 플래그에서 출력 dedup 을 우회한다.
  - 구버전 plugin(필드 미송신)은 `frame_seq = 0` 으로 파싱돼 host 가 항상 체인 단절로 취급한다 — in-tree egui-mesh whitelist plugin 은 본체와 함께 재빌드되므로 실사용 영향 없음.
- IPC alias 정규화 layer — 옛 메서드 이름이 새 이름과 같은 핸들러로 라우팅된다.
- `AuthAck { ok, reason }` + `AuthAckEnvelope { auth_ack }` — plugin이 `AuthMessage` 송신 후 호스트로부터 받는 단일 노티. ok=true면 메인 루프 진입, ok=false면 즉시 거부. 메인 루프의 `PluginRequest`와 다른 envelope(`auth_ack` 키)로 파서가 분리된다. SDK 측에서 `PluginError::HandshakeRejected`/`HandshakeTimeout`으로 매핑됨. (additive, api_version 유지)
- (PR 4에서 제거됨)
- `PluginEvent::PaintFrame` 에 `byte_len: u32`(optional + default 0, additive) 추가 — SharedBuffer 는 `size.next_power_of_two()` 로 할당돼 뒤쪽에 이전 frame 의 잔여 capacity 바이트가 남을 수 있다. 로컬(같은 프로세스) GPU 디코드는 self-terminating 파싱이라 이를 무시했지만, attach mesh mirror(host가 원본 mesh 바이트를 네트워크로 그대로 재중계하는 경로, [`docs/dev-guide/egui-mesh-channel.md` "attach mesh mirror 소비 경로"](../../docs/dev-guide/egui-mesh-channel.md#attach-mesh-mirror-소비-경로))는 정확한 payload 경계가 필요해 추가됐다. 구버전 plugin(필드 미송신)은 `0`으로 파싱되고, attach 쪽은 그 경우 버퍼 전체 capacity 를 fallback 으로 쓴다.

### Changed
- `surface.meta_set` / `meta_get` / `meta_unset` / `meta_list`이 `surface.meta.set` / `meta.get` / `meta.unset` / `meta.list`(점 표기)로 정규화됨. 핸들러/method_meta는 새 이름만 등록.

### Deprecated
- `surface.meta_set` / `surface.meta_get` / `surface.meta_unset` / `surface.meta_list` — 0.7 tag 직전에 alias가 제거된다. 새 호출자는 점 표기 사용.

### Removed
- `METHOD_SURFACE_LIFECYCLE = "surface.lifecycle"` + `SurfaceLifecycleParams` + `SurfaceLifecycleEvent` + `SurfaceCloseReason` — Event Bus 1.0 `surface.closed`로 일원화. 옛 매니페스트 `[[contributes.surface_observer]]`도 함께 제거. plugin은 `event_subscribe = ["surface.closed"]`로 마이그레이션. (api_version 유지 — 머지 전이라 baseline에는 추가된 적 없음)

### Fixed
- (없음)

## [api_version = 1, baseline] — 2026-05-12

baseline 시점의 schema를 기록한다. 이후 모든 변경은 `[Unreleased]`에 적은 뒤 릴리스 시 버전 헤더로 옮긴다.

> **호스트 결합**: tasty 본체 **0.7.0** (2026-06-04) 부터 `api_version = 1` 시리즈가 안정 선언된다. 이후 본 시리즈에서는 추가만 가능하며, schema break 는 `api_version = 2` 도입과 함께 별도 트랙으로 분리된다 ([`docs/dev-guide/api-conventions.md` "안정성 정책"](../../docs/dev-guide/api-conventions.md#안정성-정책)).

### 핵심 메시지

- `AuthMessage { plugin_id, token }` — 호스트 listener에 첫 줄로 송신
- `PluginEvent` — plugin → 호스트 알림 (`Hello`, `Log`, `IpcCall`, surface event 등)
- `PluginRequest { id, method, params }` — 호스트 → plugin 요청 (`ping`, `surface.*`, `command.invoke`, `ipc.invoke`, `ipc.result`, `shutdown`)
- `PluginResponse { id, result, error, error_code }` — plugin → 호스트 응답 (JSON-RPC 형식)
- `IpcCallResult { call_id, result, error }` — 호스트가 `ipc.result` 요청 안에 담아 plugin에 전달
- `IpcInvokeParams { method, params, caller_plugin_id }` — `ipc.invoke` 요청 본문

### UI 트리

- `UiNode` (텍스트/버튼/Tree/Splitter 등)
- `UiEvent` (사용자 입력 노티)
- `ButtonStyle`, `LabelStyle`, `SelectionMode`, `SplitDir`

### 메서드 상수

- `METHOD_PING` / `METHOD_SURFACE_CREATE` / `METHOD_SURFACE_EVENT` / `METHOD_SURFACE_RESTORE` / `METHOD_SURFACE_SNAPSHOT` / `METHOD_SURFACE_DESTROY` / `METHOD_COMMAND_INVOKE` / `METHOD_IPC_INVOKE` / `METHOD_IPC_RESULT` / `METHOD_SHUTDOWN`

이 baseline은 phase1 plugin extension 완료 시점의 schema에 해당한다.
