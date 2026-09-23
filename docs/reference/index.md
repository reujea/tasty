# 레퍼런스 (Reference)

조회용 계약/카탈로그 — 동작(behavior) 문서가 아니라 **lookup** 이다. 어떤 기능이 *왜·어떻게* 동작하는지는 [features/](../features/index.md), 여기는 *무엇이 있는지* 를 찾는 자리다.

| 문서 | 내용 | 코드 SoT |
|------|------|----------|
| [api.md](api.md) | 주요 IPC/CLI 표면 — 네임스페이스별 메서드·권한·시스템/윈도우 관측 범위 | `crates/tasty-ipc/src/method_meta.rs` |
| [event-catalog.md](event-catalog.md) | Event Bus 1.0 wire 계약 (plugin 공개 API) | `tasty_plugin_protocol::events` |
| [output-parsers.md](output-parsers.md) | 터미널 출력 파서 카탈로그 | `tasty-output` |
| [environments.md](environments.md) | OS별 경로·에이전트 부트스트랩 패턴 | — |
| [plan.schema.json](plan.schema.json) | 공유 컨텍스트 Plan 의 JSON Schema (memory 키 `tasty.plan.<id>`) | `crates/tasty-memory/src/plan.rs` |

> 메서드 시그니처·권한의 *정답*은 항상 코드(`method_meta.rs` / 각 핸들러)다. 이 문서들은 사람이 읽기 위한 요약이며, 충돌 시 코드를 따른다.
