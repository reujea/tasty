# ADR-0047: 훅/웹훅 공유 훅 핸들러 레지스트리 + source(트리거 출처) 게이트

- **Status**: Accepted
- **Date**: 2026-07-11
- **Tags**: hook-handler, webhook, hook, registry, source-gate, file-handler-mirror, trigger-source, patch-semantics, ipc-sequence, shell-command
- **Group**: webhook-hooks

## Context

tasty 에는 이미 **내부 이벤트 트리거**(`tasty-hooks`: surface/global hook — 프로세스 종료·출력 매칭·bell 등)가 있고, 여기에 **외부 HTTP 트리거**(웹훅, [ADR-0046](0046-webhook-owner-trust-one-way-ack.md))가 추가됐다. 두 트리거는 출처만 다를 뿐 "이벤트가 발생하면 무언가를 실행한다" 는 동일 구조다.

문제는 **실행 대상(핸들러)을 어떻게 표현·저장·게이트하느냐** 였다. 기존 훅은 핸들러 개념 없이 셸 명령을 훅마다 인라인(`SurfaceHook.command: String`)으로 저장했다. 이걸 그대로 웹훅에 쓰면 (1) 외부 HTTP 가 셸을 직접 구동하는 RCE 표면이 생기고, (2) 두 트리거가 핸들러를 공유·재사용할 방법이 없다. 한편 tasty 에는 이미 **파일 핸들러 레지스트리**(`crates/tasty-file-handler/src/`)가 host default(embedded) + plugin contribute + user config 를 patch semantics 로 병합하고, actor 별 action 스키마로 권한을 타입 강제하는 성숙한 정본 템플릿이 있었다.

또한 트리거 방향을 어떻게 명명할지 문제였다 — "inbound/outbound" 는 네트워크 방향(내부↔외부)과 대칭이 안 맞았다(webhook 은 인바운드가 맞지만 hook 은 내부 트리거 + 로컬 동작이라 "아웃바운드" 가 아니다).

## Decision

**파일 핸들러 레지스트리를 정본 템플릿으로 미러링한 공유 `HookHandlerRegistry` 를 신설**하고, 훅(내부 이벤트)과 웹훅(외부 HTTP)이 **같은 레지스트리를 공유**한다. 각 핸들러는 방향이 아니라 **트리거 출처** `source: HookSource { Hook, Webhook, Any }` 를 선언하고, 이 값이 **바인딩 게이트**가 된다.

- **source 게이트**(`validate_binding(handler, trigger)`): ① `disabled` 거부, ② `source` 가 trigger 를 안 받으면(`SourceMismatch`) 거부, ③ trigger 가 `Webhook` 인데 action 이 웹훅 바인딩 불가면(`ShellNotWebhookBindable`) 거부. 잘못된 조합(HTTP 바디를 기대하는 핸들러를 surface 이벤트에 연결, 그 반대)은 등록 단계에서 `invalid_params` 로 막는다.
- **action 종류**: 기본 코어 = `IpcSequence { calls }`(owner 고정 IPC 시퀀스, [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) 의 데이터/흐름 분리 전용). 셸 = `ShellCommand`(기존 훅 legacy). **`is_webhook_bindable()` 는 `IpcSequence` 만 true** — 셸은 항상 false 라 웹훅 바인딩이 타입 레벨에서 불가능하다.
- **actor 별 action 스키마**(파일 핸들러 `HandlerDecl<A>` 복제): `Host`=IpcSequence+ShellCommand, **`Plugin`=IpcSequence only**(`ShellCommand` variant 자체가 없어 manifest 의 `kind="shell_command"` 는 serde unknown-variant 로 reject), `User`=IpcSequence+ShellCommand. 추가로 finalize/decl 검증에서 `ShellCommand && source != Hook` 를 구조적으로 drop/reject 해 "셸은 내부 훅 전용" 을 이중 강제한다.
- **병합**: host default(embedded TOML) + plugin contribute + user config(`~/.tasty/hook-handlers.toml`) 를 patch semantics(install 순서 보존, `Some` 필드만 덮어씀)로 병합. 정렬은 priority↑ → owner tie-break(**User > Plugin > Host**) → id. 파일 핸들러와 동형이되, 인스턴스가 아니라 **프로세스 전역 싱글턴**(`global()`)이다 — 웹훅 리스너 thread 와 IPC main thread 가 공유해야 하기 때문.
- **명명**: `hook`(내부 이벤트) / `webhook`(외부 HTTP) 로 **트리거 출처**를 기준으로 가른다. `inbound`/`outbound` 네트워크 방향 용어는 쓰지 않는다.

## Consequences

- **얻은 것**: 두 트리거가 하나의 핸들러 추상·저장·게이트를 공유한다. 파일 핸들러의 검증된 병합/우선순위/영속화 구조를 그대로 재사용해 설계 비용을 아꼈다. `source` + `is_webhook_bindable` 로 **셸이 외부 HTTP 로 구동되는 경로가 타입 레벨에서 불가능**하다([ADR-0046](0046-webhook-owner-trust-one-way-ack.md) 의 RCE-표면-없음을 구조로 뒷받침). plugin 은 셸 action 을 아예 선언할 수 없다.
- **잃은 것**: 파일 핸들러 대비 개념이 하나 더 늘었다(핸들러 종류가 file/hook 두 갈래). 전역 싱글턴이라 파일 핸들러처럼 인스턴스 단위 격리 테스트가 아니라 프로세스 전역 상태를 다뤄야 한다.
- **운영 비용 / 유지 부담**: 현재 레지스트리의 **소비처는 웹훅 등록 경로 한 곳**이다 — `webhook.register` 가 `--handler <id>` 를 조회하거나 인라인 `sequence` 를 익명 핸들러(`user/wh-<slug>`)로 upsert 한다. host embedded 기본값(`host/webhook-notify`)·user config·plugin contribute 를 install 하는 부팅 헬퍼(`install_default_sources`)와 user 편집 API 는 정의돼 있으나 부팅/Settings 에 배선되지 않았고, `hook_handler.*` IPC/CLI 도 아직 없다. 기존 `tasty-hooks`(surface/global hook)는 여전히 인라인 `command: String` + 직접 셸 실행이며 이 레지스트리를 참조하지 않는다. 두 시스템을 이 레지스트리로 수렴시키려면 하위호환 어댑터(인라인 명령 → 익명 hook 핸들러)와 회귀 방어가 필요하다.

## Alternatives Considered

- **웹훅 전용 별도 핸들러 시스템**(공유 안 함): 웹훅만의 핸들러 저장소를 새로 만든다. — 파일 핸들러의 검증된 병합/권한 구조를 재발명하게 되고, 훅↔웹훅 핸들러 재사용이 불가능. 미러링이 명백히 유리해 거부.
- **기존 셸 인라인 모델을 웹훅에 그대로 확장**: `command: String` 을 웹훅에도 씀. — 외부 HTTP 가 셸을 직접 구동하는 RCE 표면을 여는 설계. 데이터/흐름 분리([ADR-0046](0046-webhook-owner-trust-one-way-ack.md))와 정면 충돌.
- **`inbound`/`outbound` 방향 명명**: 트리거를 네트워크 방향으로 명명. — hook 은 내부 트리거 + 로컬 동작이라 "outbound(내부→외부)" 가 아니다. 방향 대칭이 깨져 트리거 출처(`hook`/`webhook`) 명명을 채택.
- **인스턴스 레지스트리**(파일 핸들러처럼): `global()` 싱글턴 대신 소유 인스턴스. — 웹훅 accept thread 와 IPC main thread 가 공유해야 해 전역 싱글턴이 불가피.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 파일 핸들러 레지스트리의 병합/우선순위 규칙이 hook 핸들러와 **본질적으로 갈라지는** 요구가 생기면 — 미러링 전제(동형)를 재평가.
- 웹훅이 셸 실행이나 OS 조작을 정당하게 요구하게 되면 — `is_webhook_bindable` 게이트를 재설계([ADR-0046](0046-webhook-owner-trust-one-way-ack.md) 트리거와 연동).
- 트리거 출처가 hook/webhook 을 넘어 3종 이상으로 늘어 `HookSource` enum 이 폭증하면 — source 모델을 일반화.

## References

- [`features/webhook/index.md`](../features/webhook/index.md) · [`features/hooks/index.md`](../features/hooks/index.md) · [`features/file-handler/index.md`](../features/file-handler/index.md)(정본 템플릿)
- [ADR-0046](0046-webhook-owner-trust-one-way-ack.md) — owner 신뢰 모델 + 불변식(셸 웹훅 거부의 보안 근거)
- 코드: `src/hook_handler/{types,config,registry,exec}.rs`, `src/hook_handler/defaults/default-hook-handlers.toml`; 정본 템플릿 `crates/tasty-file-handler/src/{registry,types,config}.rs`
- 부분 개정: [0430](0430-hook-handler-merge-applies-user-patches-last.md) (병합 순서 조항의 "install 순서 보존" 개정 — 병합은 출처 순서 Host → Plugin → User)
