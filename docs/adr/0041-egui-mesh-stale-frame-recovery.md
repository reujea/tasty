# ADR-0041: 체인 단절 유실 frame 의 stale atlas 는 host 단독 "매 tick full 재무장"으로 복구한다

- **Status**: Accepted
- **Date**: 2026-07-09
- **Tags**: plugin, render-channel, egui, epaint, mesh, texture-atlas, font, stale-frame, host-only, recovery, adr-0028, adr-0030
- **Group**: plugin-render

## Context

egui-mesh 렌더 채널(ADR-0028)에서 plugin 이 보내는 `textures_delta` 는 **증분**(full
atlas 는 Context 첫 run 에만)이고 SharedBuffer 는 latest-wins 라, host 가 중간 frame 을 못
보면 그 frame 의 텍스처 delta 가 유실된다. host 는 `frame_seq` 체인(`frame_seq == last+1`)으로
유실을 감지해 그 frame 의 delta 를 적용하지 않고 full 재전송을 요청한다.

mesh(기하) 채택은 이 delta 체인과 분리돼 있다 — reflow frame 의 mesh 는 자기완결적이라
리사이즈/split 로 seq 가 튀어도 옛 폭에 고정(우측 잘림)되지 않게 채택해야 한다. 그런데
기존 코드는 체인 단절 frame 의 mesh 를 채택할 때 `last_seq` 를 **전진**시켰다. 그러면 다음
frame 이 인위적으로 "연속"이 되어 full-복구 트리거(chain-broken 재요청)가 닫혀버린다. 결과:
delta 유실로 stale 해진 atlas 에 새 글리프 uv 가 영구 고착 → markdown 을 초단위로 연속
재기록할 때 문자 전량이 스크램블된 채 복구되지 않았다.

핵심 갈림: 유실로 atlas 내용이 stale 인 frame 의 mesh 를 채택하되, host 가 그 stale 을
어떻게 인지하고 다음 full frame 으로 수렴시킬 것인가. (선행 3d74217c 는 인접 문제인
아틀라스 리사이즈 시 부분 delta 오버런 크래시를 `deltas_fit_live` 경계 검사로 이미 차단해
둔 상태 — 이 결정은 그 방어선을 회귀시키지 않아야 한다.)

## Decision

**host 단독으로, delta 를 실제 적용하지 못한 frame 의 mesh 는 `last_seq` 를 전진시키지
않는다**(신규 결과 `DecodeOutcome::AcceptedStale`). 체인 단절(seq 점프)이지만 참조가 상주
하고 delta 경계가 정합하면 mesh(기하)만 채택하고 delta 는 적용하지 않으며 `last_seq` 를
그대로 둔다. 그러면 다음 tick 도 체인 단절로 남아 **full 재전송이 매 tick 계속 무장**되고,
유실로 stale 해진 atlas 는 다음 full frame 이 도착하는 즉시 정합 복구된다. plugin·SDK·wire
프로토콜은 건드리지 않는다 — 채택 게이트 판정(`classify_decode`)과 caller 재무장만 host 에서
바꾸는 최소 변경이다.

## Consequences

- **얻은 것**: 체인 단절 frame 의 stale atlas 가 영구 고착되지 않고 다음 full frame 으로
  자연 수렴한다(스크램블 제거). mesh 는 여전히 즉시 채택돼 reflow 지연/우측 잘림이 없다.
  판정이 순수 함수(`classify_decode`)로 분리돼 세 결과(Accepted / AcceptedStale / NeedsFull)의
  `last_seq`·delta·full-재요청 동작이 단위 테스트로 고정된다.
- **잃은 것**: full 이 도착하기 전 몇 tick 동안은 새 mesh 기하가 stale atlas uv 를 가리켜
  글리프가 잠깐 어긋날 수 있다(고착이 아니라 과도기 — 다음 full 로 해소). host 는 atlas
  *내용*의 최신성까지는 검증하지 않고 체인 연속성만 본다.
- **운영 비용 / 유지 부담**: 재무장은 `egui_mesh_*_full_requests`(HashSet)에 sid 당 1건/tick
  이라 부하가 낮다. plugin generation 이 정지해 새 frame 이 안 와도 무장된 surface 는 매 tick
  full 을 재요청하지만, 이는 재-tessellation·GPU 업로드 없이 `need_full_textures` IPC
  메시지 1건뿐이고 화면 깜빡임도 없다. 3d74217c 의 오버런 경계(`deltas_fit_live`)는 그대로
  살아 있어 오버런 frame 은 AcceptedStale 이 아니라 NeedsFull 로 보류된다(크래시 방어 무회귀).

## Alternatives Considered

- **A — atlas 내용 epoch/version 프로토콜**: plugin 이 atlas 내용 세대(generation)를 frame
  메타에 붙이고 host 가 상주 텍스처의 세대와 비교해 *내용 최신성*까지 검증한다. 체인 단절
  이어도 세대가 같으면 stale 이 아니라고 확정해 불필요한 full 재요청 자체를 없앨 수 있다.
  하지만 wire 프로토콜 + plugin-sdk + host 3계층을 가로지르는 cross-cutting 변경이고, 지금
  필요는 host 단독·최소 변경이다. 방법 B 의 재무장 부하가 낮아(위 "운영 비용") 프로토콜을
  확장할 이득이 크지 않으므로 채택하지 않았다. (아래 재검토 조건 참조.)
- **회복책(재open / 재시작)을 코드에 심기**: 증상만 우회할 뿐 유실 frame 의 stale 고착이라는
  근본 갭을 남긴다. 근본 수정(재무장) 대신 택하지 않았다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- plugin 이 영구 무응답이 되어(프로세스 hang 등) full 요청 IPC 가 백오프 없이 무한 반복되는
  부하가 **실측 문제**로 드러날 때 — 방법 A(epoch 프로토콜) 또는 재요청 백오프를 검토한다.
- atlas 내용 세대(generation) 정보가 이 복구 외의 다른 요구(예: 텍스처 캐시 무효화, 진단)
  로도 필요해질 때 — 방법 A 로 세대를 1급 개념화하는 편이 총비용이 낮아진다.

## References

- design/dev-guide: [`dev-guide/egui-mesh-channel.md`](../dev-guide/egui-mesh-channel.md) 의
  "텍스처 상태 수명 + delta 체인" 절 (세 결과 판정과 재무장 흐름)
- source: `src/gfx/gpu/egui_mesh_prepare.rs` — `classify_decode`,
  `DecodeOutcome::AcceptedStale`, `decode_mesh_into_target` 의 `last_seq` 전진 조건
- commit `28b18e97` (본 결정 구현: 체인 단절 frame 채택 시 last_seq 미전진)
- commit `3d74217c` (선행 오버런 방어선: 부분 delta 오버런 크래시 차단 — `deltas_fit_live`)
- [ADR-0028](0028-plugin-egui-mesh-render-channel.md), [ADR-0030](0030-image-egui-mesh-bitmap-texture.md)
