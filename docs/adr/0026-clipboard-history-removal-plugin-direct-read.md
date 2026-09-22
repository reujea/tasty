# ADR-0026: 클립보드 히스토리 백엔드 제거 + 뷰어는 plugin 직접-read

- **Status**: Accepted
- **Date**: 2026-06-28
- **Tags**: clipboard, plugin, removal, scope, sandbox, user-agent-separation, semver, breaking, adr-0009
- **Group**: bundled-plugins

## Context

기존 클립보드 히스토리는 host 가 소유한 누적형 백엔드였다:

- host 가 별도 스레드로 OS 클립보드를 주기 폴링(`clipboard.poll_interval_ms`)해 변경을 감지하고, `CoreState.clipboard_history`(메모리, `history_max` 상한) + `clipboard_history` DB 테이블(자리만 확보된 상태)에 기록.
- 내부 복사(`record_internal_copy`)·OS 복사(`clipboard.copied` 이벤트)를 출처 태그와 함께 누적.
- 사용자/에이전트 표면: CLI `tasty clipboard {list,get,paste,...}`, IPC `tool.clipboard.list/get/paste/remove/clear`, 설정 `settings.clipboard`(`ClipboardSettings.history_max` 등), plugin payload `ClipboardCopied`. 표시 UI 는 `clipboard-history` plugin(popup).

이 구조에는 세 가지 부담이 있었다. ① **상시 폴링 비용**(유휴 중에도 OS 클립보드를 깨움). ② **민감정보 누적 리스크** — 비밀번호 관리자 복사본 등이 host 메모리에 필터 없이 쌓인다(OS 민감 플래그 구분 수단 제한적). ③ **아키텍처 모순** — plugin 은 비-샌드박스 OS 프로세스라([ADR-0009](0009-plugin-sandbox-deferred.md)) 클립보드를 직접 read 할 수 있는데도, host 가 폴링·캡처·중계하는 별도 경로를 이중으로 유지하고 있었다.

실사용에서 "히스토리 누적"보다 "지금 클립보드에 무엇이 들어 있나"를 확인하려는 수요가 핵심이었다.

## Decision

**클립보드 히스토리 백엔드(폴링 캡처 + 메모리/DB 누적 + 그 CLI/IPC/이벤트/설정 표면)를 전면 제거한다.** `clipboard-history` plugin 을 제거하고, 같은 도구 메뉴·단축키 자리를 채우는 **`com.tasty.clipboard-viewer`** plugin 으로 교체한다.

새 뷰어는 popup open 시점에 **plugin 프로세스가 `arboard` 로 OS 클립보드를 1회 직접 read** 해 타입별로 분류·미리보기하는 read-only 뷰다. host 백엔드·폴링·누적·재복사·IPC 네임스페이스가 없다. 직접-read 는 [ADR-0009](0009-plugin-sandbox-deferred.md) 의 비-샌드박스 모델에 따른다 — plugin 은 OS 프로세스라 host 가 클립보드 접근을 막을 수도, 막을 필요도 없으므로 단발 read 를 host 경유로 우회시키지 않는다. manifest `permissions = ["clipboard.read", ...]` 의 `clipboard.read` 는 host-API 게이트가 아니라 **의도 선언**으로만 유지한다.

## Consequences

- **얻은 것**: 상시 폴링 제거(유휴 비용·웨이크업 감소), 민감정보 host 누적 리스크 소거(누적 자체가 없음), host↔plugin 이중 경로 단일화(plugin 직접-read 일원화), `CoreState`·DB·설정·IPC 표면 축소.
- **잃은 것 (BREAK)**: 외부 표면 5종 제거 — CLI `tasty clipboard *`, IPC `tool.clipboard.*`, 이벤트 `clipboard.copied`, plugin payload `ClipboardCopied`(+`ClipboardKind`), 설정 `settings.clipboard`. **에이전트 노출 손실**: 에이전트가 히스토리를 조회/재붙여넣기하던 IPC 경로가 사라진다. 다만 단발 클립보드 read/write 는 각 에이전트가 자기 프로세스에서 직접 수행할 수 있어(ADR-0009) 능력 자체의 순손실은 "과거 항목 누적 조회"에 국한된다. 또한 *과거 항목으로 되돌아가기*(history rollback) 기능이 사라진다.
- **운영 비용 / 유지 부담**: 0.x SemVer 정책상 직접 대체이며 호환 alias 를 두지 않는다 — `api_baseline` 가드의 method baseline 에서 `tool.clipboard.*` 5개를 제거해 기준선을 재고정한다(제거는 1.0 이전이라 break 허용 범위). `clipboard_history` DB 테이블 DDL 도 SCHEMA 에서 빠지므로 신규 DB 는 해당 테이블을 만들지 않는다.

## Alternatives Considered

- **히스토리 유지 + 폴링만 끄기(이벤트 기반 캡처)**: OS 별 클립보드 변경 이벤트 API 가 제각각이고 신뢰도가 낮아 폴링을 완전히 대체하지 못한다. 민감정보 누적 리스크도 그대로라 근본 문제를 못 푼다.
- **히스토리 유지 + host 캡처를 plugin 으로 이관**: plugin 이 폴링·누적을 떠안아도 누적 리스크와 상시 폴링 비용은 동일하게 남고, 단발 확인 수요에 비해 과한 상태를 plugin 에 만든다.
- **뷰어를 host-API(`tool.clipboard.read`) 경유 read 로**: plugin 이 직접 read 가능한데(ADR-0009) host 중계 IPC 를 새로 두는 것은 불필요한 경로 추가다. host 가 막을 수 없는 접근에 게이트 흉내를 내는 것은 보안 이득 없이 복잡도만 늘린다.

## Reconsideration Triggers

다음 중 하나가 충족되면 본 ADR 을 재검토한다.

- 클립보드 **히스토리/롤백**에 대한 실사용 수요가 반복 제기되고, 민감정보 필터(OS 민감 플래그·앱별 제외)로 누적 리스크를 충분히 낮출 수단이 생겼을 때.
- plugin 직접-read 모델이 바뀌어(예: ADR-0009 가 샌드박스 채택으로 Superseded) plugin 이 OS 클립보드를 직접 read 할 수 없게 될 때 — 이 경우 host-API 중계 read 경로가 필요해진다.
- 에이전트가 클립보드 히스토리를 IPC 로 조회해야 하는 멀티에이전트 워크플로 요구가 구체화될 때.

## References

- [ADR-0009: plugin sandbox deferred](0009-plugin-sandbox-deferred.md) — plugin = 비-샌드박스 OS 프로세스, 직접 native read 근거
- [features/clipboard](../features/clipboard/index.md) · [plugins/clipboard-viewer](../plugins/clipboard-viewer/index.md)
- [dev-guide/api-conventions "안정성 정책" 절](../dev-guide/api-conventions.md) — 0.x break 정책 / api_baseline 가드
- `crates/tasty-plugin-clipboard-viewer/` · CHANGELOG `[Unreleased] > Removed`
